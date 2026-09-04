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

use super::presence::{FetchState, PeerResolve, RetryBackoff};

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
        crate::config::http::run_or(
            fut,
            Err(pds::xrpc::FetchError::Network(config::http::timed_out(
                "peer avatar record fetch",
            ))),
        )
        .await
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
    /// The peer this resolves for, so a failure can record its backoff.
    peer_entity: Entity,
    /// The identity whose records these are — carried so the landing site can
    /// name it in `WardrobeResolved` / `AttachmentFetchFailed` (#1144) without
    /// re-reading the peer, which may have disconnected by then.
    did: String,
    avatar_rkey: String,
    attachment_rkeys: Vec<String>,
    task: bevy::tasks::Task<(
        Option<crate::pds::avatar::ResolvedRig>,
        crate::pds::avatar::wardrobe::ResolveReport,
    )>,
}

/// A reference set that failed to resolve, and when it may be tried again
/// (#1113).
///
/// Lives on the peer entity, so it goes when the peer does. Keyed by the
/// reference set itself rather than by peer: the wait exists because *these
/// records* could not be fetched, so any edit to what the peer wears retires
/// it immediately and the new references resolve on the next frame.
#[derive(Component)]
pub(super) struct PeerRigResolveBackoff {
    avatar_rkey: String,
    attachment_rkeys: Vec<String>,
    /// Seconds to wait from [`Self::failed_at`], doubling per attempt up to
    /// [`config::network::RIG_RESOLVE_RETRY_MAX_SECS`].
    wait_secs: f64,
    failed_at: f64,
}

impl PeerRigResolveBackoff {
    /// Whether this backoff still holds for `rig` at time `now` — the same
    /// references, and the wait not yet elapsed.
    fn holds(&self, rig: &crate::pds::avatar::RiggedBody, now: f64) -> bool {
        self.avatar_rkey == rig.avatar
            && self.attachment_rkeys == rig.attachments
            && now - self.failed_at < self.wait_secs
    }

    /// The backoff to record after a failure, doubling the previous wait
    /// for the same reference set and starting from the base otherwise.
    fn after_failure(
        previous: Option<&Self>,
        rig: &crate::pds::avatar::RiggedBody,
        now: f64,
    ) -> Self {
        let same_set = previous
            .is_some_and(|b| b.avatar_rkey == rig.avatar && b.attachment_rkeys == rig.attachments);
        // Shared arithmetic (#1217): the peer-fetch retries introduced by
        // #1217/#1218 double the same way, and "doubling from base, capped
        // at max" now means one thing in one place. A set that CHANGED
        // starts over — the wait exists because those records could not be
        // fetched, and these are different records.
        let wait_secs = super::presence::next_wait_secs(
            same_set.then(|| previous.map(|b| b.wait_secs)).flatten(),
            config::network::RIG_RESOLVE_RETRY_BASE_SECS,
            config::network::RIG_RESOLVE_RETRY_MAX_SECS,
        );
        Self {
            avatar_rkey: rig.avatar.clone(),
            attachment_rkeys: rig.attachments.clone(),
            wait_secs,
            failed_at: now,
        }
    }
}

/// When this peer last had a rigged-body resolution STARTED for it,
/// whatever the outcome and whatever it was wearing (#1126).
///
/// Distinct from [`PeerRigResolveBackoff`], which is keyed by reference set
/// and exists for references that *fail*. This one is unconditional and
/// exists for references that succeed: it caps how often one peer can make
/// every guest in the room fan out to hosts of its choosing.
#[derive(Component)]
pub(super) struct PeerRigResolveFloor {
    started_at: f64,
}

impl PeerRigResolveFloor {
    /// Whether this floor still bars a new resolution at `now`.
    ///
    /// Takes no reference set on purpose — see
    /// [`config::network::RIG_RESOLVE_MIN_INTERVAL_SECS`]. A peer
    /// alternating between two valid outfits presents a changed set on
    /// every update, so any set-conditional test would wave it through.
    fn holds(&self, now: f64) -> bool {
        now - self.started_at < config::network::RIG_RESOLVE_MIN_INTERVAL_SECS
    }
}

/// Whether a rigged body's resolution covers every reference it names.
///
/// A resolution is a fetch of the records the reference list names, so it is
/// finished only when it came back with one record per attachment; a shorter
/// list means at least one record 404'd or failed in transit (#1122).
fn rig_is_fully_resolved(rig: &crate::pds::avatar::RiggedBody) -> bool {
    rig.resolved
        .as_ref()
        .is_some_and(|resolved| resolved.attachments.len() >= rig.attachments.len())
}

/// Start a resolution for every peer whose record is rigged but unresolved.
pub(super) fn spawn_peer_rig_resolutions(
    mut commands: Commands,
    peers: Query<(
        Entity,
        &RemotePeer,
        Option<&PeerRigResolveBackoff>,
        Option<&PeerRigResolveFloor>,
    )>,
    inflight: Query<&PeerRigResolveTask>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (peer_entity, peer, backoff, floor) in &peers {
        // A muted peer gets none of this (#1219 f287). The fan-out is N+2
        // records per attempt — a DID document, a wardrobe record and up to
        // sixteen attachments, to hosts of THEIR choosing, on the shared
        // `IoTaskPool` — and it is the largest lever a peer retains over a
        // client that has decided it is done with them. Re-resolved on
        // unmute: this system runs every frame and `resolved` never rides
        // the wire, so nothing has to be remembered.
        if peer.muted {
            continue;
        }
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
        // "Resolved" means resolved COMPLETELY (#1122). A prop worn from
        // the inventory has a minted TID that is not on the owner's PDS
        // until they save, so its record 404s and `resolve_rigged_body`
        // skips it — leaving `resolved` `Some` with a SHORT attachment
        // list. Treating that as done meant the prop was never fetched
        // again, not even after the owner published it: peers saw the
        // circlet only once its wearer next changed something else.
        // Re-resolution is bounded by the backoff below, which
        // `poll_peer_rig_resolutions` records for a short set exactly as it
        // does for an outright failure.
        if rig_is_fully_resolved(rig) || inflight.iter().any(|t| t.peer_id == peer.peer_id) {
            continue;
        }
        // A reference set that just failed is not retried until its wait has
        // elapsed (#1113). Without this the `None` result below simply left
        // `resolved` empty and this system re-spawned the whole fan-out on
        // the very next frame, forever.
        if backoff.is_some_and(|b| b.holds(rig, now)) {
            continue;
        }
        // Per-peer rate floor (#1126), checked whether or not the reference
        // set changed — a set-conditional check is precisely what a peer
        // alternating between two valid outfits walks through.
        if floor.is_some_and(|f| f.holds(now)) {
            continue;
        }
        let avatar_rkey = rig.avatar.clone();
        let attachment_rkeys = rig.attachments.clone();
        let (rkey_for_task, attachments_for_task) = (avatar_rkey.clone(), attachment_rkeys.clone());
        let pool = bevy::tasks::IoTaskPool::get();
        let did_for_event = did.clone();
        let task = pool.spawn(async move {
            let fut = async {
                let client = config::http::default_client();
                let Some(pds) = pds::xrpc::resolve_pds(&client, &did).await else {
                    return (
                        None,
                        pds::avatar::wardrobe::ResolveReport::aborted("PDS did not resolve"),
                    );
                };
                let mut rig = crate::pds::avatar::RiggedBody {
                    avatar: rkey_for_task,
                    attachments: attachments_for_task,
                    resolved: None,
                };
                let report =
                    pds::avatar::wardrobe::resolve_rigged_body(&client, &pds, &did, &mut rig).await;
                (rig.resolved, report)
            };
            // The report travels with the resolution so the landing site can
            // say what was skipped and why (#1144) — a `None` alone cannot
            // distinguish a deleted wardrobe record from a timeout.
            crate::config::http::run_or(
                fut,
                (
                    None,
                    pds::avatar::wardrobe::ResolveReport::aborted("request timed out"),
                ),
            )
            .await
        });
        commands.spawn(PeerRigResolveTask {
            peer_id: peer.peer_id,
            peer_entity,
            did: did_for_event,
            avatar_rkey,
            attachment_rkeys,
            task,
        });
        // Stamped at START, not at completion: the cost this bounds is the
        // fan-out itself, which is already spent by the time a result lands.
        commands
            .entity(peer_entity)
            .insert(PeerRigResolveFloor { started_at: now });
    }
}

/// Land finished resolutions onto their peers. A result whose reference
/// snapshot no longer matches the peer's current record is dropped — the
/// spawn system re-resolves against the newer references next frame.
pub(super) fn poll_peer_rig_resolutions(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerRigResolveTask)>,
    mut peers: Query<(
        &mut RemotePeer,
        &mut PeerResolve,
        Option<&PeerRigResolveBackoff>,
    )>,
    time: Res<Time>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut task) in tasks.iter_mut() {
        let Some((result, report)) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // Report the outcome before acting on it (#1144). This chain is N+2
        // records per peer and, since gifting made attachment records
        // cross-owner, the one most likely to fail partially — yet it used to
        // leave nothing behind but console `warn!` lines, so "why is Bob a
        // bare chassis for me but not for Alice" was unanswerable from a
        // captured log.
        let requested = task.attachment_rkeys.len() as u32;
        let installed = result
            .as_ref()
            .map(|r| r.attachments.len() as u32)
            .unwrap_or(0);
        session_log.record(
            now,
            if result.is_some() {
                crate::diagnostics::event::Severity::Info
            } else {
                crate::diagnostics::event::Severity::Warn
            },
            EventPayload::WardrobeResolved {
                did: task.did.clone(),
                requested,
                resolved: installed,
                body_ok: result.is_some(),
            },
        );
        if let Some(reason) = &report.body_error {
            debug!("wardrobe unresolved for {}: {reason}", task.did);
        }
        // The shortfall reaches the roster (#1217 f332). Until now the ONLY
        // trace of a worn item that could not be fetched was a session-log
        // line — and the failure is per-viewer, so two people in the same
        // room saw different outfits with no way to discover the
        // disagreement. The module's own comment named the consequence
        // ("why is Bob a bare chassis for me but not for Alice") without
        // giving anyone a way to answer it.
        if let Ok((_, mut resolve, _)) = peers.get_mut(task.peer_entity) {
            PeerResolve::record_outfit(&mut resolve, requested, installed);
        }
        for (rkey, reason) in &report.skipped {
            crate::diagnostics::samplers::attachment_fetch_failed(&mut metrics);
            session_log.warn(
                now,
                EventPayload::AttachmentFetchFailed {
                    did: task.did.clone(),
                    rkey: rkey.clone(),
                    reason: reason.clone(),
                },
            );
        }
        let Some(resolved) = result else {
            // Nothing resolved (deleted wardrobe record, transport failure,
            // or a body the owner has not published yet): the peer keeps
            // whatever body is standing, and the reference set is put on a
            // backoff (#1113) so this does not become one fan-out per frame
            // for every client in the room. The comment here used to claim
            // "NOT retried in a loop" while nothing recorded the failure,
            // which is exactly what made it a loop.
            let rig = peers
                .get(task.peer_entity)
                .ok()
                .and_then(|(peer, _, backoff)| {
                    peer.avatar
                        .as_ref()
                        .and_then(|record| record.body.rigged_ref())
                        .map(|rig| PeerRigResolveBackoff::after_failure(backoff, rig, now))
                });
            if let Some(backoff) = rig {
                commands.entity(task.peer_entity).insert(backoff);
            }
            continue;
        };
        let Some((mut peer, _, previous_backoff)) =
            peers.iter_mut().find(|(p, _, _)| p.peer_id == task.peer_id)
        else {
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
        let complete = resolved.attachments.len() >= rig.attachments.len();
        // A short set is a partial failure (#1122): the body is installed so
        // the peer is not left bare, but the missing props are worth another
        // try — most often because their owner has not saved them yet, and
        // the record appears at that rkey the moment they do. The doubling
        // backoff that bounds an outright failure bounds this too, so a prop
        // that is never published settles into a slow poll rather than a
        // per-frame fan-out.
        let backoff =
            (!complete).then(|| PeerRigResolveBackoff::after_failure(previous_backoff, rig, now));
        rig.resolved = Some(resolved);
        match backoff {
            // Resolved in full: any wait recorded for these references is
            // spent.
            None => {
                commands
                    .entity(task.peer_entity)
                    .remove::<PeerRigResolveBackoff>();
            }
            Some(backoff) => {
                commands.entity(task.peer_entity).insert(backoff);
            }
        }
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
    mut peers: Query<(&mut RemotePeer, &mut PeerResolve)>,
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
        let (mut record, cacheable, outcome) = match result {
            Ok(Some(r)) => {
                crate::diagnostics::samplers::avatar_fetch_succeeded(&mut metrics);
                (r, true, FetchState::Landed)
            }
            Ok(None) => {
                // A 404 resolved to the DID-seeded default — still a successful
                // fetch (the peer simply hasn't published an avatar).
                crate::diagnostics::samplers::avatar_fetch_succeeded(&mut metrics);
                info!(
                    "Peer {} ({}) has no avatar record — synthesising default",
                    peer_id, did
                );
                (
                    AvatarRecord::default_for_did(&did),
                    false,
                    FetchState::Landed,
                )
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
                // The two "no record" outcomes are NOT the same fact (#1217
                // f323). A 404 above is a finished question — this person has
                // not published an avatar — and the DID-seeded default IS
                // their appearance. A transport failure is an unanswered one,
                // and rendering it identically meant a one-second blip at
                // join time replaced someone's authored body with a
                // procedurally generated stranger, for the whole room, for
                // the whole session, with nothing said to anybody.
                let previous = peers
                    .iter()
                    .find(|(p, _)| p.peer_id == peer_id)
                    .and_then(|(_, resolve)| resolve.avatar.backoff().copied());
                (
                    AvatarRecord::default_for_did(&did),
                    false,
                    FetchState::Failed(RetryBackoff::after_failure(previous.as_ref(), elapsed)),
                )
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
        if let Some((mut peer, mut resolve)) = peers.iter_mut().find(|(p, _)| p.peer_id == peer_id)
        {
            // A stand-in installed by an earlier FAILED attempt may be
            // replaced — that is the whole point of the retry (#1217 f323).
            // A live preview may not: `AvatarStateUpdate` sets `avatar` to
            // `Landed`, so the state read here distinguishes "nothing real
            // is standing" from "something newer already arrived".
            let stand_in = resolve.avatar.is_failed();
            if peer.avatar.is_none() || stand_in {
                peer.avatar = Some(record);
            }
            resolve.avatar = outcome;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(avatar: &str, attachments: &[&str]) -> crate::pds::avatar::RiggedBody {
        crate::pds::avatar::RiggedBody {
            avatar: avatar.into(),
            attachments: attachments.iter().map(|a| (*a).to_string()).collect(),
            resolved: None,
        }
    }

    /// #1122. Sequence: a peer wears a prop from their inventory, which
    /// mints a fresh TID; the preview broadcast names it, every guest
    /// resolves, and that record is not on the owner's PDS yet — so it 404s
    /// and `resolve_rigged_body` skips the prop. The result is `Some` with a
    /// SHORT attachment list, and treating `Some` as "done" meant no guest
    /// ever fetched that prop again, not even after the owner saved it: the
    /// circlet appeared only when its wearer next changed something else.
    #[test]
    fn a_prop_that_is_not_on_the_pds_yet_is_not_a_finished_resolution() {
        let mut worn = rig("3jzfcijpj2z2a", &["att-1"]);
        worn.resolved = Some(crate::pds::avatar::body::ResolvedRig {
            body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:short"),
            attachments: Vec::new(),
        });
        assert!(
            !rig_is_fully_resolved(&worn),
            "one reference, no record: there is still something to fetch"
        );

        worn.resolved = Some(crate::pds::avatar::body::ResolvedRig {
            body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:short"),
            attachments: vec![crate::pds::avatar::ResolvedAttachment {
                rkey: String::from("att-1"),
                record: crate::pds::avatar::wardrobe::AttachmentRecord::new(
                    crate::pds::Generator::default(),
                    symbios_avatar::Socket::Crown,
                ),
            }],
        });
        assert!(rig_is_fully_resolved(&worn));
    }

    /// A body with nothing worn is finished the moment it resolves — the
    /// completeness rule must not turn every plain avatar into a re-fetch
    /// loop.
    #[test]
    fn a_body_wearing_nothing_resolves_once() {
        let mut bare = rig("3jzfcijpj2z2a", &[]);
        assert!(!rig_is_fully_resolved(&bare), "not fetched yet");
        bare.resolved = Some(crate::pds::avatar::body::ResolvedRig {
            body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:bare"),
            attachments: Vec::new(),
        });
        assert!(rig_is_fully_resolved(&bare));
    }

    /// #1219 f287. The sequence: you mute a hostile peer and assume you have
    /// disengaged, while their record keeps driving your client's network on
    /// every edit they make. This fan-out is the biggest lever they retain —
    /// N+2 records per attempt, to hosts of their choosing, on the shared
    /// `IoTaskPool` — and it had no mute check at all.
    ///
    /// Asserts the negative only, deliberately: the unmuted control would
    /// spawn a REAL HTTPS round trip, and under `cargo test --lib`, where
    /// every test shares one process and one `IoTaskPool`, a blocked fetch
    /// starves whatever else is waiting on a task. Removing the gate makes
    /// this test fail (a task appears), which is the regression it is for.
    #[test]
    fn a_muted_peer_does_not_fan_out_to_a_wardrobe() {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.add_systems(Update, spawn_peer_rig_resolutions);

        let mut record = AvatarRecord::default_for_did("did:plc:hostile");
        record.body = crate::pds::AvatarBody::rigged("3jzfcijpj2z2a");
        app.world_mut().spawn(RemotePeer {
            peer_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000001\"")
                .expect("a well-formed uuid"),
            did: Some(String::from("did:plc:hostile")),
            handle: None,
            muted: true,
            avatar: Some(record),
            build: None,
            connected_at: 0.0,
        });

        app.update();
        app.update();

        let mut tasks = app.world_mut().query::<&PeerRigResolveTask>();
        assert_eq!(
            tasks.iter(app.world()).count(),
            0,
            "a peer you have blocked does not get to keep making your client fetch"
        );
        let mut floors = app.world_mut().query::<&PeerRigResolveFloor>();
        assert_eq!(
            floors.iter(app.world()).count(),
            0,
            "and the rate floor is not stamped for work that never started"
        );
    }

    /// #1113: a reference set that cannot resolve is left alone for a while
    /// instead of being re-fetched every frame by every client in the room.
    #[test]
    fn a_failed_resolution_holds_off_the_next_attempt() {
        let unresolvable = rig("3jzfcijpj2z2a", &["att-1"]);
        let backoff = PeerRigResolveBackoff::after_failure(None, &unresolvable, 100.0);

        assert_eq!(
            backoff.wait_secs,
            config::network::RIG_RESOLVE_RETRY_BASE_SECS
        );
        assert!(backoff.holds(&unresolvable, 100.5), "still waiting");
        assert!(
            !backoff.holds(
                &unresolvable,
                100.0 + config::network::RIG_RESOLVE_RETRY_BASE_SECS
            ),
            "the wait elapses and the references are tried again"
        );
    }

    /// #1126: the failure backoff above is keyed by reference set, which
    /// does nothing against references that SUCCEED. Because `resolved` is
    /// `#[serde(skip)]`, every live-preview update arrives unresolved, so a
    /// peer alternating between two valid outfits made every guest in the
    /// room re-run the whole fan-out — a DID document, a wardrobe record
    /// and up to sixteen attachments — per round trip, to hosts of that
    /// peer's choosing, on the shared IoTaskPool.
    ///
    /// The floor is therefore unconditional. The issue proposed "unless the
    /// reference set changed since the last completed resolution", but that
    /// is the exact condition the alternating case satisfies every time.
    #[test]
    fn a_peer_cannot_re_resolve_faster_than_the_floor_however_it_redresses() {
        let floor = PeerRigResolveFloor { started_at: 100.0 };
        assert!(floor.holds(100.5), "a redress moments later waits");
        assert!(
            floor.holds(100.0 + config::network::RIG_RESOLVE_MIN_INTERVAL_SECS - 0.001),
            "still waiting right up to the interval"
        );
        assert!(
            !floor.holds(100.0 + config::network::RIG_RESOLVE_MIN_INTERVAL_SECS),
            "and the legitimate wearer's edit lands once it elapses"
        );
    }

    #[test]
    fn repeated_failure_of_the_same_references_backs_further_off_up_to_a_ceiling() {
        let same = rig("3jzfcijpj2z2a", &["att-1"]);
        let mut backoff = PeerRigResolveBackoff::after_failure(None, &same, 0.0);
        let first = backoff.wait_secs;
        backoff = PeerRigResolveBackoff::after_failure(Some(&backoff), &same, 10.0);
        assert_eq!(backoff.wait_secs, first * 2.0);

        for _ in 0..20 {
            backoff = PeerRigResolveBackoff::after_failure(Some(&backoff), &same, 0.0);
        }
        assert_eq!(
            backoff.wait_secs,
            config::network::RIG_RESOLVE_RETRY_MAX_SECS,
            "the doubling is capped, not unbounded"
        );
    }

    /// The backoff must never outlive the reason for it: the moment the peer
    /// changes what they wear (or publishes the body they were wearing), the
    /// new references are a different question and are asked immediately.
    #[test]
    fn changing_the_references_retires_the_backoff() {
        let failed = rig("3jzfcijpj2z2a", &["att-1"]);
        let backoff = PeerRigResolveBackoff::after_failure(None, &failed, 0.0);

        assert!(backoff.holds(&failed, 0.1));
        assert!(
            !backoff.holds(&rig("3jzfcijpj2z2b", &["att-1"]), 0.1),
            "a different body is a different question"
        );
        assert!(
            !backoff.holds(&rig("3jzfcijpj2z2a", &["att-1", "att-2"]), 0.1),
            "a changed outfit is too"
        );

        // And a failure against new references restarts from the base wait
        // rather than inheriting the old set's escalation.
        let escalated = PeerRigResolveBackoff::after_failure(Some(&backoff), &failed, 0.0);
        assert!(escalated.wait_secs > config::network::RIG_RESOLVE_RETRY_BASE_SECS);
        let fresh =
            PeerRigResolveBackoff::after_failure(Some(&escalated), &rig("3jzfcijpj2z2z", &[]), 0.0);
        assert_eq!(
            fresh.wait_secs,
            config::network::RIG_RESOLVE_RETRY_BASE_SECS
        );
    }
}
