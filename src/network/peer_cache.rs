//! DID-keyed avatar cache + the async PDS-fetch task that populates it.
//! Decouples a cluster of peers landing in a room (e.g. portal hop)
//! from the IoTaskPool: a returning peer's record loads from memory
//! without any network I/O.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::{self, AvatarRecord};
use crate::state::RemotePeer;

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
/// The cache is FIFO-bounded at
/// [`config::network::MAX_PEER_AVATAR_CACHE_ENTRIES`]: a busy hub-room or
/// a malicious relay cycling thousands of authenticated DIDs would
/// otherwise grow the resident set without bound across a long session
/// (the cache used to only clear on logout). Re-inserting an existing key
/// promotes it to the back of the FIFO so live peers in a steady-state
/// room are not evicted by churn from short-lived joiners.
///
/// The cache is invalidated through the same channels that would invalidate
/// a stale in-memory copy: an inbound `AvatarStateUpdate` from the owner
/// overwrites it, and [`crate::state::AppState::InGame`] exit (`logout`)
/// wipes the whole map so a new login can't see a previous user's peers.
#[derive(Resource, Default)]
pub struct PeerAvatarCache {
    by_did: std::collections::HashMap<String, AvatarRecord>,
    order: std::collections::VecDeque<String>,
}

impl PeerAvatarCache {
    pub(super) fn get(&self, did: &str) -> Option<&AvatarRecord> {
        self.by_did.get(did)
    }

    pub(super) fn insert(&mut self, did: String, record: AvatarRecord) {
        if self.by_did.contains_key(&did) {
            self.order.retain(|d| d != &did);
        } else {
            while self.order.len() >= config::network::MAX_PEER_AVATAR_CACHE_ENTRIES {
                match self.order.pop_front() {
                    Some(oldest) => {
                        self.by_did.remove(&oldest);
                    }
                    None => break,
                }
            }
        }
        self.order.push_back(did.clone());
        self.by_did.insert(did, record);
    }

    pub fn clear(&mut self) {
        self.by_did.clear();
        self.order.clear();
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
    /// Session-relative seconds when the fetch was dispatched, so the poller can
    /// record its spawn→resolve latency (E-4).
    pub(super) spawned_at: f64,
}

pub(super) fn spawn_peer_avatar_fetch(
    commands: &mut Commands,
    peer_id: PeerId,
    did: String,
    spawned_at: f64,
) {
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
            config::http::block_on(fut)
        }
    });
    commands.spawn(PeerAvatarFetchTask {
        peer_id,
        did,
        task,
        spawned_at,
    });
}

/// A rigged reference resolution in flight for one peer (#1059).
///
/// A live-preview `AvatarStateUpdate` decodes with `resolved = None` — the
/// resolution never rides the wire — so a peer wearing a rigged body needs
/// its wardrobe + attachment references re-fetched before anything can be
/// built. The rkeys are snapshotted so a preview that changes the references
/// mid-flight simply drops this result and re-resolves.
#[derive(Component)]
pub(super) struct PeerRigResolveTask {
    peer_id: PeerId,
    avatar_rkey: String,
    attachment_rkeys: Vec<String>,
    task: bevy::tasks::Task<Option<crate::pds::avatar::ResolvedRig>>,
}

/// Start a resolution for every peer whose record is rigged but unresolved.
pub(super) fn spawn_peer_rig_resolutions(
    mut commands: Commands,
    peers: Query<&RemotePeer>,
    inflight: Query<&PeerRigResolveTask>,
) {
    for peer in &peers {
        let Some(did) = peer.did.clone() else {
            continue;
        };
        let Some(rig) = peer
            .avatar
            .as_ref()
            .and_then(|record| record.body.rigged_ref())
        else {
            continue;
        };
        if rig.resolved.is_some() || inflight.iter().any(|t| t.peer_id == peer.peer_id) {
            continue;
        }
        let avatar_rkey = rig.avatar.clone();
        let attachment_rkeys = rig.attachments.clone();
        let (rkey_for_task, attachments_for_task) = (avatar_rkey.clone(), attachment_rkeys.clone());
        let pool = bevy::tasks::IoTaskPool::get();
        let task = pool.spawn(async move {
            let fut = async {
                let client = config::http::default_client();
                let pds = pds::xrpc::resolve_pds(&client, &did).await?;
                let mut rig = crate::pds::avatar::RiggedBody {
                    avatar: rkey_for_task,
                    attachments: attachments_for_task,
                    resolved: None,
                };
                pds::avatar::wardrobe::resolve_rigged_body(&client, &pds, &did, &mut rig).await;
                rig.resolved
            };
            #[cfg(target_arch = "wasm32")]
            {
                fut.await
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                config::http::block_on(fut)
            }
        });
        commands.spawn(PeerRigResolveTask {
            peer_id: peer.peer_id,
            avatar_rkey,
            attachment_rkeys,
            task,
        });
    }
}

/// Land finished resolutions onto their peers. A result whose reference
/// snapshot no longer matches the peer's current record is dropped — the
/// spawn system re-resolves against the newer references next frame.
pub(super) fn poll_peer_rig_resolutions(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerRigResolveTask)>,
    mut peers: Query<&mut RemotePeer>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let Some(resolved) = result else {
            // Nothing resolved (deleted wardrobe record, transport failure):
            // the peer keeps whatever body is standing. NOT retried in a
            // loop — the next record change re-arms the spawn system.
            continue;
        };
        let Some(mut peer) = peers.iter_mut().find(|p| p.peer_id == task.peer_id) else {
            continue;
        };
        let Some(record) = peer.avatar.as_mut() else {
            continue;
        };
        let Some(rig) = record.body.rigged_mut() else {
            continue;
        };
        if rig.avatar != task.avatar_rkey || rig.attachments != task.attachment_rkeys {
            continue;
        }
        rig.resolved = Some(resolved);
    }
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
    mut session_log: ResMut<SessionLog>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
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
        // Record the fetch's spawn→resolve latency (E-4) before the task despawns.
        crate::diagnostics::samplers::avatar_fetch_latency_secs(
            &mut metrics,
            elapsed - task.spawned_at,
        );
        commands.entity(entity).despawn();

        // Only a true 2xx-with-payload is cached: a 404 or transient
        // network error synthesises a DID-hashed default here, and caching
        // that would prevent a later Identity for the same peer from
        // retrying the real PDS fetch (a user who publishes their avatar
        // for the first time mid-session would otherwise be stuck with the
        // placeholder for every peer that happened to be on the PDS
        // fallback path).
        let (mut record, cacheable) = match result {
            Ok(Some(r)) => {
                crate::diagnostics::samplers::avatar_fetch_succeeded(&mut metrics);
                (r, true)
            }
            Ok(None) => {
                // A 404 resolved to the DID-seeded default — still a successful
                // fetch (the peer simply hasn't published an avatar).
                crate::diagnostics::samplers::avatar_fetch_succeeded(&mut metrics);
                info!(
                    "Peer {} ({}) has no avatar record — synthesising default",
                    peer_id, did
                );
                (AvatarRecord::default_for_did(&did), false)
            }
            Err(err) => {
                crate::diagnostics::samplers::avatar_fetch_failed(&mut metrics);
                session_log.warn(
                    elapsed,
                    EventPayload::AvatarFetchFailed {
                        peer: peer_id.to_string(),
                        did: did.clone(),
                        error: format!("{err:?}"),
                    },
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
