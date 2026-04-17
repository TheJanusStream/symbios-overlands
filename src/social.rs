//! Social-graph resonance — asynchronously queries the public ATProto
//! `app.bsky.graph.getRelationships` lexicon for every newly-identified peer
//! and lights up the mast tip of any peer that bidirectionally follows the
//! local actor.
//!
//! The query is dispatched from an `AsyncComputeTaskPool` task and polled each
//! frame so the main game loop never stalls on network I/O.

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use bevy_symbios_multiuser::auth::AtprotoSession;
use futures_lite::future;
use serde::Deserialize;

use crate::rover::MastTip;
use crate::state::{AppState, RemotePeer, SocialResonance};

pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                dispatch_resonance_queries,
                poll_resonance_tasks,
                sync_social_resonance,
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
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()
                    .map(|rt| rt.block_on(fut))
                    .unwrap_or(SocialResonance::None)
            }
        });
        commands.entity(entity).insert(ResonanceFetchTask(task));
    }
}

/// Drain completed `ResonanceFetchTask`s and write their results onto the
/// corresponding `RemotePeer` entities as a `SocialResonance` component.
fn poll_resonance_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ResonanceFetchTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(status) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<ResonanceFetchTask>()
            .insert(status);
    }
}

/// Make the mast-tip cap of every `Mutual` peer emissive in its own mast
/// colour so mutual follows are visually obvious at a glance.  Runs each
/// frame but only does work for peers whose resonance flag *or* whose child
/// set has just changed — the latter catches airship redesigns.
#[allow(clippy::type_complexity)]
fn sync_social_resonance(
    changed: Query<
        (&RemotePeer, &SocialResonance, &Children),
        Or<(Changed<SocialResonance>, Changed<Children>)>,
    >,
    mast_tips: Query<(), With<MastTip>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (peer, resonance, children) in changed.iter() {
        if *resonance != SocialResonance::Mutual {
            continue;
        }
        let Some(airship) = peer.airship.as_ref() else {
            continue;
        };
        let [mr, mg, mb] = airship.mast_color;
        let intensity = crate::config::network::MUTUAL_MAST_EMISSIVE;
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(mr, mg, mb),
            emissive: LinearRgba::rgb(mr * intensity, mg * intensity, mb * intensity),
            metallic: 0.75,
            perceptual_roughness: 0.35,
            ..default()
        });
        for child in children.iter() {
            if mast_tips.get(child).is_ok() {
                let handle = mat.clone();
                commands.queue(move |world: &mut World| {
                    if let Ok(mut eref) = world.get_entity_mut(child) {
                        eref.insert(MeshMaterial3d(handle));
                    }
                });
            }
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
