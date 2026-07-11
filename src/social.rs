//! Social-graph resonance — asynchronously queries the public ATProto
//! `app.bsky.graph.getRelationships` lexicon for every newly-identified peer
//! and tags them with a [`SocialResonance`] component reflecting the
//! mutual-follow relationship.
//!
//! The query is dispatched from the `IoTaskPool` and polled each frame so
//! the main game loop never stalls on network I/O. The resonance tag is
//! consumed by the chat and people-panel UI (`ui/chat.rs` and
//! `ui/people.rs` both highlight `Mutual`-follow peers); the legacy
//! in-world mast-tip glow was dropped together with the rest of the
//! rover-specific marker plumbing during the avatar-unification work.
//!
//! The second half of this module is the mutuals enumeration service
//! (#746): a TTL-cached, on-demand listing of *everyone* a given DID
//! mutually follows, built from the public AppView's paginated
//! `getFollows` ∩ `getFollowers`. The gateway destination picker (#748)
//! is its consumer — it lists the **room owner's** mutuals, so visitors
//! browse the owner's social neighbourhood, not their own.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use bevy_symbios_multiuser::auth::AtprotoSession;
use futures_lite::future;
use serde::Deserialize;

use crate::state::{AppState, RemotePeer, SocialResonance};

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MutualsCache>().add_systems(
            Update,
            (
                dispatch_resonance_queries,
                poll_resonance_tasks,
                poll_mutuals_tasks,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// In-flight task resolving the relationship between the local DID and a peer
/// DID.  Stored on the `RemotePeer` entity while the GET request is pending.
#[derive(Component)]
pub struct ResonanceFetchTask(pub Task<SocialResonance>);

/// Dispatch a relationship query for every peer that has announced a DID but
/// does not yet carry a `SocialResonance` state.  Requires an authenticated
/// `AtprotoSession` so we know which `actor` to ask about.
#[allow(clippy::type_complexity)]
fn dispatch_resonance_queries(
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    peers: Query<(Entity, &RemotePeer), (Without<SocialResonance>, Without<ResonanceFetchTask>)>,
) {
    let Some(sess) = session else { return };
    for (entity, peer) in peers.iter() {
        let Some(remote_did) = peer.did.as_deref() else {
            continue;
        };
        if remote_did == sess.did {
            // Self-loop: nothing to query.
            commands.entity(entity).insert(SocialResonance::None);
            continue;
        }
        let local_did = sess.did.clone();
        let remote = remote_did.to_string();
        // `IoTaskPool` — not `AsyncComputeTaskPool` — is the correct pool for
        // blocking HTTP work. AsyncCompute is CPU-bound (rayon-sized, scales
        // with `physical_cores`); a handful of hung reqwest connections
        // there starves the whole async-compute budget (terrain generation,
        // texture baking), tanking FPS for the entire session. The IoTaskPool
        // is sized for exactly this pattern.
        let pool = IoTaskPool::get();
        let task = pool.spawn(async move {
            let fut = query_resonance(local_did, remote);
            #[cfg(target_arch = "wasm32")]
            {
                fut.await
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                crate::config::http::block_on(fut)
            }
        });
        commands.entity(entity).insert(ResonanceFetchTask(task));
    }
}

/// Drain completed `ResonanceFetchTask`s and write their results onto the
/// corresponding `RemotePeer` entities as a `SocialResonance` component.
fn poll_resonance_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ResonanceFetchTask, &RemotePeer)>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    for (entity, mut task, peer) in tasks.iter_mut() {
        let Some(status) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        // Log the resolved resonance for the diagnostics timeline (#635a). The
        // async task returns a bare `SocialResonance`, so a network failure is
        // indistinguishable from a legitimate `None` here — emitting the typed
        // `SocialResonanceFailed` needs the task to carry its error and is a
        // deliberately-deferred follow-up.
        session_log.info(
            time.elapsed_secs_f64(),
            crate::diagnostics::event::EventPayload::SocialResonanceCompleted {
                peer: peer.peer_id.to_string(),
                resonance: format!("{status:?}"),
            },
        );
        commands
            .entity(entity)
            .remove::<ResonanceFetchTask>()
            .insert(status);
    }
}

// ---------------------------------------------------------------------------
// ATProto lexicon query
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RelationshipsResponse {
    #[serde(default)]
    relationships: Vec<RelationshipEntry>,
}

/// Unauthenticated relationship entry.  `following` / `followedBy` are present
/// (as `at://` URI strings) iff the corresponding edge exists in the graph.
#[derive(Deserialize)]
struct RelationshipEntry {
    #[serde(rename = "$type", default)]
    kind: Option<String>,
    #[serde(default)]
    following: Option<String>,
    #[serde(default, rename = "followedBy")]
    followed_by: Option<String>,
}

async fn query_resonance(local_did: String, remote_did: String) -> SocialResonance {
    let client = crate::config::http::default_client();

    let url = format!(
        "https://public.api.bsky.app/xrpc/app.bsky.graph.getRelationships?actor={}&others={}",
        local_did, remote_did
    );

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            bevy::log::warn!("getRelationships {} => {}", remote_did, r.status());
            return SocialResonance::None;
        }
        Err(e) => {
            bevy::log::warn!("getRelationships transport error: {e}");
            return SocialResonance::None;
        }
    };

    let parsed: RelationshipsResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            bevy::log::warn!("getRelationships decode error: {e}");
            return SocialResonance::None;
        }
    };

    for entry in parsed.relationships {
        // notFoundActor entries have no following/followedBy — skip them.
        if entry
            .kind
            .as_deref()
            .map(|k| k.contains("notFoundActor"))
            .unwrap_or(false)
        {
            continue;
        }
        if entry.following.is_some() && entry.followed_by.is_some() {
            return SocialResonance::Mutual;
        }
    }
    SocialResonance::None
}

// ---------------------------------------------------------------------------
// Mutuals enumeration (#746): TTL-cached getFollows ∩ getFollowers
// ---------------------------------------------------------------------------

/// How long a resolved mutuals list stays fresh. Follow graphs move on
/// human timescales; five minutes keeps a gateway picker snappy across
/// repeated opens without hammering the AppView.
const MUTUALS_TTL_SECS: f64 = 300.0;

/// Failed lookups retry much sooner — a transient AppView hiccup should
/// not lock the gateway out for the full TTL.
const MUTUALS_FAILED_RETRY_SECS: f64 = 15.0;

/// AppView page size (the lexicon maximum).
const GRAPH_PAGE_LIMIT: u32 = 100;

/// Hard cap on pages walked per direction (follows / followers), i.e. at
/// most 1000 accounts per side. Enormous accounts get a truncated —
/// flagged — intersection instead of an unbounded crawl; the picker
/// surfaces the flag so the cap is never silent.
const MAX_GRAPH_PAGES: u32 = 10;

/// One mutual follow of the queried owner, ready for a destination row.
#[derive(Clone, Debug, PartialEq)]
pub struct MutualEntry {
    pub did: String,
    pub handle: String,
    /// Bsky display name; `None` when unset or empty.
    pub display_name: Option<String>,
}

/// A resolved mutuals listing. `truncated` is true when either direction
/// of the graph walk hit [`MAX_GRAPH_PAGES`] — the intersection is then a
/// lower bound, not the complete set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MutualsList {
    pub mutuals: Vec<MutualEntry>,
    pub truncated: bool,
}

/// Lifecycle of one owner's cache slot.
#[derive(Clone, Debug)]
pub enum MutualsState {
    /// Fetch task in flight — never considered stale, so re-requests
    /// while pending are free no-ops.
    Loading,
    Ready(MutualsList),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct CachedMutuals {
    /// `Time::elapsed_secs_f64` when the state was written (wasm-safe —
    /// no `std::time` on the wasm32 target).
    pub at_secs: f64,
    pub state: MutualsState,
}

/// TTL cache of mutuals listings keyed by owner DID. Populated on demand
/// via [`request_mutuals`]; consumers read their slot every frame and
/// render Loading/Ready/Failed accordingly.
#[derive(Resource, Default)]
pub struct MutualsCache {
    pub by_owner: HashMap<String, CachedMutuals>,
}

impl MutualsCache {
    /// The current slot for `owner_did`, if any.
    pub fn get(&self, owner_did: &str) -> Option<&CachedMutuals> {
        self.by_owner.get(owner_did)
    }

    /// True when a fresh fetch should be dispatched for `owner_did`.
    fn needs_fetch(&self, owner_did: &str, now: f64) -> bool {
        match self.by_owner.get(owner_did) {
            None => true,
            Some(cached) => match &cached.state {
                MutualsState::Loading => false,
                MutualsState::Ready(_) => now - cached.at_secs > MUTUALS_TTL_SECS,
                MutualsState::Failed(_) => now - cached.at_secs > MUTUALS_FAILED_RETRY_SECS,
            },
        }
    }
}

/// In-flight mutuals enumeration for one owner DID.
#[derive(Component)]
pub struct MutualsFetchTask {
    owner_did: String,
    task: Task<Result<MutualsList, String>>,
}

/// Ensure a mutuals listing for `owner_did` is resident or in flight.
/// Call freely every frame (e.g. from an open picker) — fresh and pending
/// slots are no-ops. `now` is `Time::elapsed_secs_f64`.
pub fn request_mutuals(
    commands: &mut Commands,
    cache: &mut MutualsCache,
    owner_did: &str,
    now: f64,
) {
    if !cache.needs_fetch(owner_did, now) {
        return;
    }
    cache.by_owner.insert(
        owner_did.to_string(),
        CachedMutuals {
            at_secs: now,
            state: MutualsState::Loading,
        },
    );
    let did = owner_did.to_string();
    // IoTaskPool for the same reason as the resonance query above: this is
    // blocking-HTTP-shaped work and must not starve AsyncCompute.
    let pool = IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = fetch_mutuals(did);
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(MutualsFetchTask {
        owner_did: owner_did.to_string(),
        task,
    });
}

/// Drain completed [`MutualsFetchTask`]s into the cache.
fn poll_mutuals_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut MutualsFetchTask)>,
    mut cache: ResMut<MutualsCache>,
    time: Res<Time>,
) {
    for (entity, mut fetch) in tasks.iter_mut() {
        let Some(result) = future::block_on(future::poll_once(&mut fetch.task)) else {
            continue;
        };
        let state = match result {
            Ok(list) => MutualsState::Ready(list),
            Err(reason) => {
                warn!("mutuals fetch for {} failed: {reason}", fetch.owner_did);
                MutualsState::Failed(reason)
            }
        };
        cache.by_owner.insert(
            fetch.owner_did.clone(),
            CachedMutuals {
                at_secs: time.elapsed_secs_f64(),
                state,
            },
        );
        commands.entity(entity).despawn();
    }
}

/// `app.bsky.actor.defs#profileView` — the subset the picker needs.
#[derive(Deserialize, Clone)]
struct ProfileView {
    did: String,
    handle: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

/// One page of either `getFollows` or `getFollowers`. The two lexicons
/// differ only in the list key, so both keys default-decode and
/// [`walk_graph`] reads whichever is populated.
#[derive(Deserialize)]
struct GraphPage {
    #[serde(default)]
    follows: Vec<ProfileView>,
    #[serde(default)]
    followers: Vec<ProfileView>,
    cursor: Option<String>,
}

/// Walk one direction of the graph (`app.bsky.graph.getFollows` or
/// `…getFollowers`) up to [`MAX_GRAPH_PAGES`]. Returns the accumulated
/// profiles and whether the walk was truncated (a cursor remained).
async fn walk_graph(
    client: &reqwest::Client,
    lexicon: &str,
    actor: &str,
) -> Result<(Vec<ProfileView>, bool), String> {
    let mut out: Vec<ProfileView> = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_GRAPH_PAGES {
        let url = format!("https://public.api.bsky.app/xrpc/{lexicon}");
        let mut query: Vec<(&str, String)> = vec![
            ("actor", actor.to_string()),
            ("limit", GRAPH_PAGE_LIMIT.to_string()),
        ];
        if let Some(c) = &cursor {
            query.push(("cursor", c.clone()));
        }
        let resp = client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| format!("{lexicon} transport error: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("{lexicon} => {}", resp.status()));
        }
        let page: GraphPage = resp
            .json()
            .await
            .map_err(|e| format!("{lexicon} decode error: {e}"))?;
        let batch = if page.follows.is_empty() {
            page.followers
        } else {
            page.follows
        };
        let empty_page = batch.is_empty();
        out.extend(batch);
        cursor = page.cursor;
        // A missing cursor is the AppView's end-of-list signal; an empty
        // page guards against a server that keeps echoing cursors.
        if cursor.is_none() || empty_page {
            return Ok((out, false));
        }
    }
    Ok((out, cursor.is_some()))
}

/// Intersect the two directions into the mutual set. Profile data is
/// taken from the follows side; the result is handle-sorted so the picker
/// is stable across refreshes regardless of AppView page order.
fn intersect_mutuals(follows: Vec<ProfileView>, followers: &[ProfileView]) -> Vec<MutualEntry> {
    let follower_dids: std::collections::HashSet<&str> =
        followers.iter().map(|p| p.did.as_str()).collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut mutuals: Vec<MutualEntry> = follows
        .into_iter()
        .filter(|p| follower_dids.contains(p.did.as_str()))
        .filter(|p| seen.insert(p.did.clone()))
        .map(|p| MutualEntry {
            did: p.did,
            handle: p.handle,
            display_name: p.display_name.filter(|n| !n.trim().is_empty()),
        })
        .collect();
    mutuals.sort_by_key(|a| a.handle.to_lowercase());
    mutuals
}

/// Enumerate `owner_did`'s mutual follows from the public AppView —
/// unauthenticated, so it works for any owner, not just the local user.
async fn fetch_mutuals(owner_did: String) -> Result<MutualsList, String> {
    let client = crate::config::http::default_client();
    let (follows, follows_truncated) =
        walk_graph(&client, "app.bsky.graph.getFollows", &owner_did).await?;
    let (followers, followers_truncated) =
        walk_graph(&client, "app.bsky.graph.getFollowers", &owner_did).await?;
    Ok(MutualsList {
        mutuals: intersect_mutuals(follows, &followers),
        truncated: follows_truncated || followers_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(did: &str, handle: &str, name: Option<&str>) -> ProfileView {
        ProfileView {
            did: did.into(),
            handle: handle.into(),
            display_name: name.map(str::to_owned),
        }
    }

    #[test]
    fn intersection_keeps_only_bidirectional_edges() {
        let follows = vec![
            profile("did:plc:a", "zed.example", Some("Zed")),
            profile("did:plc:b", "amy.example", None),
            profile("did:plc:c", "onlyfollowed.example", None),
        ];
        let followers = vec![
            profile("did:plc:a", "zed.example", Some("Zed")),
            profile("did:plc:b", "amy.example", None),
            profile("did:plc:d", "onlyfollower.example", None),
        ];
        let mutuals = intersect_mutuals(follows, &followers);
        assert_eq!(
            mutuals.iter().map(|m| m.did.as_str()).collect::<Vec<_>>(),
            // Handle-sorted, not input-ordered.
            vec!["did:plc:b", "did:plc:a"],
        );
    }

    #[test]
    fn intersection_dedupes_and_drops_blank_display_names() {
        let follows = vec![
            profile("did:plc:a", "amy.example", Some("   ")),
            profile("did:plc:a", "amy.example", Some("Amy")),
        ];
        let followers = vec![profile("did:plc:a", "amy.example", None)];
        let mutuals = intersect_mutuals(follows, &followers);
        assert_eq!(mutuals.len(), 1, "duplicate page entries collapse");
        assert_eq!(
            mutuals[0].display_name, None,
            "first occurrence wins; whitespace-only names drop to None"
        );
    }

    #[test]
    fn cache_ttl_gates_refetches() {
        let mut cache = MutualsCache::default();
        assert!(cache.needs_fetch("did:plc:x", 100.0), "empty slot fetches");
        cache.by_owner.insert(
            "did:plc:x".into(),
            CachedMutuals {
                at_secs: 100.0,
                state: MutualsState::Loading,
            },
        );
        assert!(
            !cache.needs_fetch("did:plc:x", 100_000.0),
            "in-flight slot never re-dispatches"
        );
        cache.by_owner.insert(
            "did:plc:x".into(),
            CachedMutuals {
                at_secs: 100.0,
                state: MutualsState::Ready(MutualsList::default()),
            },
        );
        assert!(!cache.needs_fetch("did:plc:x", 100.0 + MUTUALS_TTL_SECS - 1.0));
        assert!(cache.needs_fetch("did:plc:x", 100.0 + MUTUALS_TTL_SECS + 1.0));
        cache.by_owner.insert(
            "did:plc:x".into(),
            CachedMutuals {
                at_secs: 100.0,
                state: MutualsState::Failed("boom".into()),
            },
        );
        assert!(
            cache.needs_fetch("did:plc:x", 100.0 + MUTUALS_FAILED_RETRY_SECS + 1.0),
            "failures retry on the short interval"
        );
    }
}
