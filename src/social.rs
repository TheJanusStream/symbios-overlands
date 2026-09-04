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

use crate::network::presence::RetryBackoff;
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

/// When a [`SocialResonance::Failed`] peer may be asked about again
/// (#1218 f297). Shares [`RetryBackoff`]'s doubling with the avatar-record
/// and profile fetches, so "retrying" means one thing across the roster.
#[derive(Component)]
pub struct ResonanceRetry(RetryBackoff);

/// Whether the relationship query should be (re)dispatched for a peer.
///
/// Never asked, or asked and could not be answered (#1218 f297). A `Failed`
/// peer waits out its backoff and is then asked again — before the `Failed`
/// arm existed, the first hiccup WAS the answer for the session, and it was
/// the same answer a genuine stranger gets.
fn should_query(
    resonance: Option<&SocialResonance>,
    retry: Option<&ResonanceRetry>,
    now: f64,
) -> bool {
    match resonance {
        None => true,
        Some(SocialResonance::Failed) => retry.is_some_and(|r| r.0.ready(now)),
        Some(_) => false,
    }
}

/// Dispatch a relationship query for every peer that has announced a DID but
/// does not yet carry a `SocialResonance` state.  Requires an authenticated
/// `AtprotoSession` so we know which `actor` to ask about.
#[allow(clippy::type_complexity)]
fn dispatch_resonance_queries(
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    peers: Query<
        (
            Entity,
            &RemotePeer,
            Option<&SocialResonance>,
            Option<&ResonanceRetry>,
        ),
        Without<ResonanceFetchTask>,
    >,
    time: Res<Time>,
) {
    let Some(sess) = session else { return };
    let now = time.elapsed_secs_f64();
    for (entity, peer, resonance, retry) in peers.iter() {
        if !should_query(resonance, retry, now) {
            continue;
        }
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
            // A timeout is a failure to ASK, not an answer (#1218 f297) —
            // `Unknown` rendered identically to a genuine non-mutual.
            crate::config::http::run_or(fut, SocialResonance::Failed).await
        });
        commands.entity(entity).insert(ResonanceFetchTask(task));
    }
}

/// Drain completed `ResonanceFetchTask`s and write their results onto the
/// corresponding `RemotePeer` entities as a `SocialResonance` component.
fn poll_resonance_tasks(
    mut commands: Commands,
    mut tasks: Query<(
        Entity,
        &mut ResonanceFetchTask,
        &RemotePeer,
        Option<&ResonanceRetry>,
    )>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    for (entity, mut task, peer, retry) in tasks.iter_mut() {
        let Some(status) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        // Log the resolved resonance for the diagnostics timeline (#635a). The
        // The task now returns `SocialResonance::Failed` for every way the
        // question can go unanswered (#1218 f297), so this line distinguishes
        // "they don't follow you" from "we couldn't ask" — it could not
        // before, and neither could the ★.
        session_log.info(
            time.elapsed_secs_f64(),
            crate::diagnostics::event::EventPayload::SocialResonanceCompleted {
                peer: peer.peer_id.to_string(),
                resonance: format!("{status:?}"),
            },
        );
        let mut entity = commands.entity(entity);
        entity.remove::<ResonanceFetchTask>().insert(status);
        if status == SocialResonance::Failed {
            entity.insert(ResonanceRetry(RetryBackoff::after_failure(
                retry.map(|r| &r.0),
                time.elapsed_secs_f64(),
            )));
        } else {
            entity.remove::<ResonanceRetry>();
        }
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
            return SocialResonance::Failed;
        }
        Err(e) => {
            bevy::log::warn!("getRelationships transport error: {e}");
            return SocialResonance::Failed;
        }
    };

    let parsed: RelationshipsResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            bevy::log::warn!("getRelationships decode error: {e}");
            return SocialResonance::Failed;
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

    /// Drop `owner_did`'s slot so the next [`request_mutuals`] dispatches
    /// immediately (#1232 f284). The failed state's only recovery was a
    /// 15 s TTL re-arm under a line reading "Retrying shortly…", with no
    /// way to ask now — unlike the loading screen's rows, which have had a
    /// *Retry now* since #1230.
    pub fn forget(&mut self, owner_did: &str) {
        self.by_owner.remove(owner_did);
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
        crate::config::http::run_or(fut, Err(crate::config::http::timed_out("mutuals fetch"))).await
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
    deadline_secs: i64,
) -> Result<(Vec<ProfileView>, bool), String> {
    let mut out: Vec<ProfileView> = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_GRAPH_PAGES {
        // Out of budget (#1232 f284): stop with what we have rather than
        // let the whole-operation bound drop the walk. Twenty sequential
        // requests under one 30 s timer meant a well-connected owner's
        // gateway — the gateway most worth using — reliably produced
        // nothing at all, and the flag below is exactly the "this is a
        // lower bound" signal the picker already renders.
        if crate::state::now_epoch_secs() >= deadline_secs {
            return Ok((out, true));
        }
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
/// Clamp an AppView display name at INGEST (#1222 f295).
///
/// A display name is fully attacker-controlled — it is whatever the account
/// holder typed — and it arrived here with only a whitespace-emptiness
/// filter, in contrast to the careful clamping applied to peer-supplied chat
/// text and gift item names. Stripping control characters kills the
/// bidi-override and newline tricks that let one row impersonate another;
/// the length cap keeps a multi-thousand-character name out of a
/// fixed-width picker window. Clamped here rather than at the renderer so
/// no downstream consumer can be handed an unbounded string.
fn clamp_display_name(raw: Option<String>) -> Option<String> {
    let cleaned: String = raw?
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_DISPLAY_NAME_CHARS)
        .collect();
    Some(cleaned).filter(|n| !n.trim().is_empty())
}

/// Longest display name the gateway picker will carry. Matches the gift
/// item-name clamp's order of magnitude; the picker window is 380 px wide.
const MAX_DISPLAY_NAME_CHARS: usize = 48;

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
            display_name: clamp_display_name(p.display_name),
        })
        .collect();
    mutuals.sort_by_key(|a| a.handle.to_lowercase());
    mutuals
}

/// Enumerate `owner_did`'s mutual follows from the public AppView —
/// unauthenticated, so it works for any owner, not just the local user.
async fn fetch_mutuals(owner_did: String) -> Result<MutualsList, String> {
    let client = crate::config::http::default_client();
    let deadline = crate::state::now_epoch_secs() + MUTUALS_WALK_BUDGET_SECS;
    let (follows, follows_truncated) =
        walk_graph(&client, "app.bsky.graph.getFollows", &owner_did, deadline).await?;
    let (followers, followers_truncated) =
        walk_graph(&client, "app.bsky.graph.getFollowers", &owner_did, deadline).await?;
    Ok(MutualsList {
        mutuals: intersect_mutuals(follows, &followers),
        truncated: follows_truncated || followers_truncated,
    })
}

/// Wall-clock budget for both graph walks together (#1232 f284).
///
/// Comfortably inside [`crate::config::http::REQUEST_TIMEOUT`], which is
/// what `run_or` races the whole operation against on wasm — so the walk
/// gives up first and its partial intersection escapes, instead of the
/// future being dropped with everything in it. On native `run_or` is a
/// pass-through and each request carries its own builder timeout, so until
/// now twenty pages had no whole-operation bound at all; this gives it one.
///
/// The wall clock, not `Res<Time>`: this runs inside an `IoTaskPool` task
/// with no `World` to read, and the virtual clock would be the wrong
/// question anyway (#1216).
const MUTUALS_WALK_BUDGET_SECS: i64 = 20;

/// Plain language for a mutuals-fetch failure (#1232 f284).
///
/// The picker interpolated the raw reason, so
/// `app.bsky.graph.getFollows => 429` reached the user — a lexicon name on
/// a control an ordinary visitor operates. The raw string is still what the
/// log and the session capture record; this is only what is shown.
pub fn mutuals_error(raw: &str) -> String {
    if raw.contains("timed out") {
        String::from("The network directory didn't answer in time.")
    } else if raw.contains("transport error") {
        String::from("Couldn't reach the network directory — check your connection.")
    } else {
        String::from("Couldn't reach the network directory.")
    }
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

#[cfg(test)]
mod display_name_tests {
    use super::*;

    /// #1222 f295. The sequence: an attacker sets their Bluesky display
    /// name to somebody else's handle, and the gateway picker renders it as
    /// the PRIMARY text with the verified handle greyed beside it — so a
    /// visitor choosing a destination clicks the wrong person's world. The
    /// display name arrived with only a whitespace-emptiness filter, in
    /// contrast to the careful clamping applied to peer-supplied chat and
    /// gift names.
    #[test]
    fn a_display_name_is_clamped_and_stripped_at_ingest() {
        // Control characters — newlines and bidi overrides — are what let a
        // name break out of its row or reverse the reading order.
        let sneaky =
            clamp_display_name(Some(String::from("alice\nsecond line"))).expect("still a name");
        assert!(!sneaky.contains('\n'), "{sneaky}");
        let bidi = clamp_display_name(Some(String::from("alice\u{202e}eciohc"))).expect("a name");
        assert!(!bidi.chars().any(char::is_control), "{bidi}");

        // Unbounded length inside a 380 px window.
        let long = "x".repeat(5_000);
        let clamped = clamp_display_name(Some(long)).expect("a name");
        assert_eq!(clamped.chars().count(), MAX_DISPLAY_NAME_CHARS);

        // The existing emptiness rule survives, including a name that is
        // ONLY control characters and therefore becomes empty.
        assert_eq!(clamp_display_name(Some(String::from("   "))), None);
        assert_eq!(clamp_display_name(Some(String::from("\u{7}\u{7}"))), None);
        assert_eq!(clamp_display_name(None), None);

        // An ordinary name is untouched.
        assert_eq!(
            clamp_display_name(Some(String::from("Alice Example"))).as_deref(),
            Some("Alice Example"),
        );
    }
}

#[cfg(test)]
mod resonance_tests {
    use super::*;

    /// #1218 f297. The sequence: the AppView times out on the one
    /// relationship query for a peer you actually follow. Before the
    /// `Failed` arm, that returned `SocialResonance::None` — whose own doc
    /// comment asserts the two of you do NOT follow each other — and both
    /// consumers rendered exactly the un-highlighted state a stranger gets.
    /// The ★ is the only trust signal the social UI carries, and it failed
    /// closed with no way to tell "no" from "couldn't ask".
    #[test]
    fn a_failed_lookup_is_asked_again_and_a_settled_one_is_not() {
        let backoff = RetryBackoff::after_failure(None, 100.0);
        let retry = ResonanceRetry(backoff);

        assert!(should_query(None, None, 0.0), "never asked: ask");
        assert!(
            !should_query(Some(&SocialResonance::None), None, 0.0),
            "a real answer is not re-asked"
        );
        assert!(
            !should_query(Some(&SocialResonance::Mutual), None, 0.0),
            "a real answer is not re-asked"
        );
        assert!(
            !should_query(Some(&SocialResonance::Failed), Some(&retry), 100.0),
            "a failure waits out its backoff first"
        );
        assert!(
            should_query(
                Some(&SocialResonance::Failed),
                Some(&retry),
                100.0 + backoff.wait_secs
            ),
            "and is then asked again"
        );
    }

    /// A `Failed` with no backoff recorded must not spin: the poll writes
    /// the two together, and a state that could re-dispatch every frame
    /// would be a load generator aimed at the AppView.
    #[test]
    fn a_failure_without_a_recorded_backoff_does_not_spin() {
        assert!(!should_query(Some(&SocialResonance::Failed), None, 1.0e9));
    }

    /// THE SEQUENCE (#1232 f284): a visitor opens a gateway in the world of
    /// somebody well-connected — the gateway most worth using. Twenty
    /// sequential graph pages run under one whole-operation timer, the
    /// timer wins, and the picker renders `app.bsky.graph.getFollows =>
    /// 429`: a lexicon name on a control an ordinary visitor operates,
    /// under "Retrying shortly…" that loops the same walk forever.
    #[test]
    fn a_failed_mutuals_walk_speaks_english_and_can_be_retried_now() {
        for raw in [
            "app.bsky.graph.getFollows => 429 Too Many Requests",
            "app.bsky.graph.getFollowers transport error: dns error",
            "mutuals fetch timed out after 30s",
        ] {
            let shown = mutuals_error(raw);
            assert!(
                !shown.contains("app.bsky") && !shown.contains("=>"),
                "lexicon leaked: {shown}"
            );
            assert!(shown.starts_with(char::is_uppercase) && shown.ends_with('.'));
        }
        // Distinguishable failures stay distinguishable.
        assert_ne!(
            mutuals_error("transport error: dns"),
            mutuals_error("mutuals fetch timed out after 30s")
        );

        // "Retry now" is `forget` plus the dispatch that already runs at
        // the top of the picker: a slot that is gone is a slot that
        // `needs_fetch` says yes to.
        let mut cache = MutualsCache::default();
        cache.by_owner.insert(
            "did:plc:owner".into(),
            CachedMutuals {
                at_secs: 100.0,
                state: MutualsState::Failed("boom".into()),
            },
        );
        assert!(
            !cache.needs_fetch("did:plc:owner", 101.0),
            "inside the failure TTL, nothing re-dispatches on its own"
        );
        cache.forget("did:plc:owner");
        assert!(cache.needs_fetch("did:plc:owner", 101.0));
    }

    /// The walk gives up on time rather than being dropped whole. The
    /// budget has to sit inside the bound `run_or` races the operation
    /// against on wasm, or the partial result never escapes.
    #[test]
    fn the_walk_budget_leaves_room_for_its_partial_result_to_escape() {
        assert!(
            MUTUALS_WALK_BUDGET_SECS < crate::config::http::REQUEST_TIMEOUT.as_secs() as i64,
            "a budget at or past the whole-operation bound returns nothing"
        );
    }
}
