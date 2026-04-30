//! DID-keyed avatar cache + the async PDS-fetch task that populates it.
//! Decouples a cluster of peers landing in a room (e.g. portal hop)
//! from the IoTaskPool: a returning peer's record loads from memory
//! without any network I/O.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::pds::{self, AvatarRecord};
use crate::state::{DiagnosticsLog, RemotePeer};

/// DID → last-known `AvatarRecord` cache, keyed on the authenticated DID.
///
/// Every Identity message from a previously-unseen peer used to trigger an
/// unconditional HTTPS round trip against that peer's PDS (DID document
/// resolve → `getRecord`). When a portal hop brings a cluster of familiar
/// peers into a room at once, the IoTaskPool gets saturated and avatars
/// flicker in over several seconds. Caching here lets a returning peer's
/// record load from memory without any network I/O, and keeps subsequent
/// reconnects of the same DID within a session essentially free.
///
/// The cache is invalidated through the same channels that would invalidate
/// a stale in-memory copy: an inbound `AvatarStateUpdate` from the owner
/// overwrites it, and [`crate::state::AppState::InGame`] exit (`logout`)
/// wipes the whole map so a new login can't see a previous user's peers.
#[derive(Resource, Default)]
pub struct PeerAvatarCache {
    by_did: std::collections::HashMap<String, AvatarRecord>,
}

impl PeerAvatarCache {
    pub(super) fn get(&self, did: &str) -> Option<&AvatarRecord> {
        self.by_did.get(did)
    }

    pub(super) fn insert(&mut self, did: String, record: AvatarRecord) {
        self.by_did.insert(did, record);
    }

    pub fn clear(&mut self) {
        self.by_did.clear();
    }
}

/// In-flight `fetch_avatar_record` task attached to a throwaway entity so
/// the [`poll_peer_avatar_fetches`] system can drain it without a dedicated
/// resource. The `peer_id` field identifies which remote peer the result
/// belongs to — the peer's ECS entity may have despawned by the time the
/// task completes (late disconnect), so the poller has to look it up.
#[derive(Component)]
pub(super) struct PeerAvatarFetchTask {
    pub(super) peer_id: PeerId,
    pub(super) did: String,
    pub(super) task: bevy::tasks::Task<Result<Option<AvatarRecord>, pds::FetchError>>,
}

pub(super) fn spawn_peer_avatar_fetch(commands: &mut Commands, peer_id: PeerId, did: String) {
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let did_for_fetch = did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_avatar_record(&client, &did_for_fetch).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(PeerAvatarFetchTask { peer_id, did, task });
}

/// Drain completed peer-avatar fetch tasks and install the fetched record
/// onto the matching `RemotePeer`. A 404 means the peer has never published
/// an avatar, in which case we synthesise the deterministic default keyed
/// off their DID so their vessel is still distinguishable from other
/// "unpublished" peers.
pub(super) fn poll_peer_avatar_fetches(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerAvatarFetchTask)>,
    mut peers: Query<&mut RemotePeer>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs_f64();
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let peer_id = task.peer_id;
        let did = task.did.clone();
        commands.entity(entity).despawn();

        // Only a true 2xx-with-payload is cached: a 404 or transient
        // network error synthesises a DID-hashed default here, and caching
        // that would prevent a later Identity for the same peer from
        // retrying the real PDS fetch (a user who publishes their avatar
        // for the first time mid-session would otherwise be stuck with the
        // placeholder for every peer that happened to be on the PDS
        // fallback path).
        let (mut record, cacheable) = match result {
            Ok(Some(r)) => (r, true),
            Ok(None) => {
                info!(
                    "Peer {} ({}) has no avatar record — synthesising default",
                    peer_id, did
                );
                (AvatarRecord::default_for_did(&did), false)
            }
            Err(err) => {
                diagnostics.push(
                    elapsed,
                    format!("Avatar fetch failed for {peer_id}: {err:?} — using default"),
                );
                warn!(
                    "Avatar fetch failed for {} ({}): {:?} — falling back to default",
                    peer_id, did, err
                );
                (AvatarRecord::default_for_did(&did), false)
            }
        };
        record.sanitize();
        if cacheable {
            avatar_cache.insert(did.clone(), record.clone());
        }

        // Find the live peer entity; it may have despawned if the peer
        // disconnected between the fetch kick-off and its completion.
        //
        // Only install the fetched record if we haven't already received a
        // newer state for this peer. An `AvatarStateUpdate` broadcast (the
        // live-preview nudge from a peer dragging a slider in the Avatar
        // Editor) can land between the fetch kick-off and its completion;
        // overwriting it here would permanently fracture visual state —
        // this client would see the old PDS record while every other peer
        // in the room sees the live preview.
        if let Some(mut peer) = peers.iter_mut().find(|p| p.peer_id == peer_id)
            && peer.avatar.is_none()
        {
            peer.avatar = Some(record);
        }
    }
}
