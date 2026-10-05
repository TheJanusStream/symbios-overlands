//! DID-keyed avatar cache + the async PDS-fetch task that populates it.
//! A returning peer - a portal hop's cluster of familiar faces - stands at
//! once in the record this client remembers, and is refreshed from their
//! PDS in the background (#1489).

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
/// flicker in over several seconds. Caching here lets a returning peer
/// stand at once in the record this client remembers; the fetch that
/// refreshes it runs behind ([`RefreshCachedAvatar`], #1489), and its DID
/// hop is itself cached (#1126).
///
/// The cache is FIFO-bounded at
/// [`config::network::MAX_PEER_AVATAR_CACHE_ENTRIES`]: a busy hub-room or
/// a malicious relay cycling thousands of authenticated DIDs would
/// otherwise grow the resident set without bound across a long session
/// (the cache used to only clear on logout). Re-inserting an existing key
/// promotes it to the back of the FIFO so live peers in a steady-state
/// room are not evicted by churn from short-lived joiners.
///
/// What keeps an entry current: an inbound `AvatarStateUpdate` from the
/// owner replaces it - carrying the rigged body's fetched records along
/// when the references are unchanged, as the live copy does (#1113); a
/// record a fetch brings replaces it, and so does the live copy a refresh
/// has brought up to date ([`RefreshCachedAvatar`]); an outfit that
/// resolves is written into it ([`Self::learn_resolution`]); and
/// [`crate::state::AppState::InGame`] exit (`logout`) wipes the whole map
/// so a new login can't see a previous user's peers. An entry can still be
/// older than a save this client never heard about - the owner saved while
/// it was in another room, or its link was down, or the notice came before
/// the DID was adopted - which is why every adoption from it is refreshed
/// (#1485, #1489). A first meeting's fetch, or a rig resolution, already
/// running when a notice lands may bring records from before that save, so
/// each carries the notice count it started at ([`Self::notice_seq`]) and
/// one that started before its peer's last notice is neither installed nor
/// remembered as it stands (#1490).
#[derive(Resource, Default)]
pub struct PeerAvatarCache {
    by_did: std::collections::HashMap<String, AvatarRecord>,
    order: std::collections::VecDeque<String>,
    /// When each DID's last refresh was started (#1489), for
    /// [`Self::claim_refresh`]. Pruned with the entry it belongs to, and at
    /// logout.
    refreshed_at: std::collections::HashMap<String, f64>,
    /// How many save notices this client has taken in (#1490): stamped on a
    /// peer when its notice lands ([`AvatarPublishedAt`]) and on every fetch
    /// and resolution when it starts. A count rather than a clock because
    /// order is the whole question - "did this start before that notice?" -
    /// and a clock cannot always answer it: two events in one frame share a
    /// time, and so do two frames under a coarsened browser clock (#1494).
    /// Never reset, so a task that outlives a logout can still not pass for
    /// a later one.
    notices: u64,
}

impl PeerAvatarCache {
    /// Count a save notice, and return its place in the count (#1490).
    pub(super) fn note_publish(&mut self) -> u64 {
        self.notices += 1;
        self.notices
    }

    /// The notice count now: what a fetch or resolution starting now
    /// carries, to be compared with its peer's [`AvatarPublishedAt`] when it
    /// lands.
    pub(super) fn notice_seq(&self) -> u64 {
        self.notices
    }

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
                        self.refreshed_at.remove(&oldest);
                    }
                    None => break,
                }
            }
        }
        self.order.push_back(did.clone());
        self.by_did.insert(did, record);
    }

    /// Claim `did`'s refresh at `now` - at most one per
    /// [`config::network::PEER_AVATAR_REFRESH_MIN_SECS`], so a peer whose
    /// link flaps is fetched once, not once per reconnect (#1489) - or learn
    /// when the next claim can be had: a refresh is delayed, never dropped.
    pub(super) fn claim_refresh(&mut self, did: &str, now: f64) -> Result<(), f64> {
        if let Some(at) = self.refreshed_at.get(did) {
            let available_at = at + config::network::PEER_AVATAR_REFRESH_MIN_SECS;
            if now < available_at {
                return Err(available_at);
            }
        }
        self.refreshed_at.insert(did.to_owned(), now);
        Ok(())
    }

    /// Write a rig resolution that landed for `did` into its entry, when the
    /// entry names the same records (#1489): the next meeting then stands at
    /// once in the outfit this client last drew, not an older one.
    pub(super) fn learn_resolution(
        &mut self,
        did: &str,
        avatar_rkey: &str,
        attachment_rkeys: &[String],
        resolved: &crate::pds::avatar::ResolvedRig,
    ) {
        if let Some(rig) = self
            .by_did
            .get_mut(did)
            .and_then(|record| record.body.rigged_mut())
            && rig.avatar == avatar_rkey
            && rig.attachments == attachment_rkeys
        {
            rig.resolved = Some(resolved.clone());
        }
    }

    pub fn clear(&mut self) {
        self.by_did.clear();
        self.order.clear();
        self.refreshed_at.clear();
    }
}

/// In-flight `fetch_avatar_record` task attached to a throwaway entity so
/// the [`poll_peer_avatar_fetches`] system can drain it without a dedicated
/// resource. The `peer_id` field identifies which remote peer the result
/// belongs to - the peer's ECS entity may have despawned by the time the
/// task completes (late disconnect), so the poller has to look it up. A
/// refresh names the entity it was started for instead ([`Self::refresh_of`]).
#[derive(Component)]
pub(super) struct PeerAvatarFetchTask {
    pub(super) peer_id: PeerId,
    pub(super) did: String,
    pub(super) task: bevy::tasks::Task<Result<Option<AvatarRecord>, pds::FetchError>>,
    /// Session-relative seconds when the fetch was dispatched, so the poller can
    /// record its spawn→resolve latency (E-4).
    pub(super) spawned_at: f64,
    /// [`PeerAvatarCache::notice_seq`] when the fetch was dispatched: a peer
    /// whose last save notice counts higher saved after this started
    /// (#1490).
    pub(super) notice_seq: u64,
    /// For the refresh of a record remembered at adoption (#1489), the peer
    /// entity it was started for: its answer lands there or nowhere - never
    /// on a later entity that happens to reuse the peer id.
    pub(super) refresh_of: Option<Entity>,
}

/// A peer installed from [`PeerAvatarCache`] when their DID was adopted, and
/// refreshed from their PDS (#1489): the remembered record stands until the
/// saved one lands, in [`poll_peer_avatar_fetches`].
#[derive(Component, Default)]
pub(super) struct RefreshCachedAvatar {
    /// Whether [`spawn_cached_avatar_refreshes`] has started the current
    /// attempt.
    pub(super) spawned: bool,
    /// Whether this refresh holds its DID's claim
    /// ([`PeerAvatarCache::claim_refresh`]); a retry or a re-run does not
    /// ask again.
    pub(super) claimed: bool,
    /// Session seconds before which no attempt starts: the end of the claim
    /// window, or of the wait after a failed attempt.
    pub(super) not_before: f64,
    /// The doubling wait after failed attempts - the one a first meeting's
    /// retries use, with no cap on their number.
    pub(super) backoff: Option<RetryBackoff>,
}

/// Where this peer's last save notice stands in [`PeerAvatarCache`]'s count
/// (#1489, #1490): a fetch or resolution that started at a lower count may
/// carry the records from before that save. It was the notice's session
/// time until #1494, which a clock coarsened past a frame could not order.
#[derive(Component)]
pub(super) struct AvatarPublishedAt(pub(super) u64);

/// What [`spawn_peer_rig_resolutions`] reads of a peer.
type RigResolvePeer = (
    Entity,
    &'static RemotePeer,
    Option<&'static PeerRigResolveBackoff>,
    Option<&'static PeerRigResolveFloor>,
    Option<&'static RefreshCachedAvatar>,
    Option<&'static PeerResolve>,
);

/// What [`poll_peer_avatar_fetches`] reads and writes of a peer.
type AvatarFetchPeer = (
    Entity,
    &'static mut RemotePeer,
    &'static mut PeerResolve,
    Option<&'static mut RefreshCachedAvatar>,
    Option<&'static AvatarPublishedAt>,
);

/// Start the refresh of each peer installed from the cache (#1489). Here, not
/// in `adopt_peer_did`, so adopting a familiar peer does no I/O itself. A
/// muted peer is not fetched for (#1219 f287): its refresh waits for the
/// unmute. A DID refreshed moments ago waits out the rest of its window
/// ([`PeerAvatarCache::claim_refresh`]) - delayed, never dropped - and a
/// retry after a failure waits out its backoff.
pub(super) fn spawn_cached_avatar_refreshes(
    mut commands: Commands,
    mut peers: Query<(Entity, &RemotePeer, &mut RefreshCachedAvatar)>,
    tasks: Query<&PeerAvatarFetchTask>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    // A hop into a room of familiar faces refreshes them a few at a time: a
    // fetch holds an IO thread while it runs, and the room's own loads
    // queue behind them.
    let mut in_flight = tasks
        .iter()
        .filter(|task| task.refresh_of.is_some())
        .count();
    for (entity, peer, mut refresh) in &mut peers {
        if refresh.spawned
            || peer.muted
            || now < refresh.not_before
            || in_flight >= config::network::PEER_AVATAR_REFRESHES_IN_FLIGHT
        {
            continue;
        }
        let Some(did) = peer.did.clone() else {
            continue;
        };
        if !refresh.claimed {
            match avatar_cache.claim_refresh(&did, now) {
                Ok(()) => refresh.claimed = true,
                Err(available_at) => {
                    refresh.not_before = available_at;
                    continue;
                }
            }
        }
        let notice_seq = avatar_cache.notice_seq();
        spawn_peer_avatar_fetch(
            &mut commands,
            peer.peer_id,
            did,
            now,
            notice_seq,
            Some(entity),
        );
        refresh.spawned = true;
        in_flight += 1;
    }
}

/// Drop the peer fetches still in flight when a session ends (#1489's
/// review): the cache is wiped at logout, and a fetch landing afterwards
/// would put the last session's peers into the next one's.
pub(super) fn drop_inflight_peer_fetches(
    mut commands: Commands,
    avatar_fetches: Query<Entity, With<PeerAvatarFetchTask>>,
    rig_fetches: Query<Entity, With<PeerRigResolveTask>>,
) {
    for entity in avatar_fetches.iter().chain(rig_fetches.iter()) {
        commands.entity(entity).despawn();
    }
}

/// Start fetching `did`'s avatar record. It starts at two kinds of now: the
/// session time `spawned_at`, for the latency sampler, and the save-notice
/// count `notice_seq` ([`PeerAvatarCache::notice_seq`]), for telling a
/// record from before the peer's last save (#1490).
pub(super) fn spawn_peer_avatar_fetch(
    commands: &mut Commands,
    peer_id: PeerId,
    did: String,
    spawned_at: f64,
    notice_seq: u64,
    refresh_of: Option<Entity>,
) {
    // `IoTaskPool` is the correct home for blocking HTTP calls - the
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
        notice_seq,
        refresh_of,
    });
}

/// A rigged reference resolution in flight for one peer (#1059).
///
/// A live-preview `AvatarStateUpdate` decodes with `resolved = None` - the
/// resolution never rides the wire - so a peer wearing a rigged body needs
/// its wardrobe + attachment references re-fetched before anything can be
/// built. The rkeys are snapshotted so a preview that changes the references
/// mid-flight simply drops this result and re-resolves.
#[derive(Component)]
pub(super) struct PeerRigResolveTask {
    peer_id: PeerId,
    /// The peer this resolves for, so a failure can record its backoff.
    peer_entity: Entity,
    /// The identity whose records these are - carried so the landing site can
    /// name it in `WardrobeResolved` / `AttachmentFetchFailed` (#1144) without
    /// re-reading the peer, which may have disconnected by then.
    did: String,
    avatar_rkey: String,
    attachment_rkeys: Vec<String>,
    /// [`PeerAvatarCache::notice_seq`] when the resolution started (#1490).
    /// The rkeys alone cannot tell it is stale: a save keeps them and moves
    /// the bytes behind them, so a resolution that read the PDS before the
    /// owner's save landing after its notice would otherwise install the
    /// pre-save outfit, where nothing fetches it again.
    notice_seq: u64,
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
    /// Whether this backoff still holds for `rig` at time `now` - the same
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
        // starts over - the wait exists because those records could not be
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
    /// Takes no reference set on purpose - see
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
    peers: Query<RigResolvePeer>,
    inflight: Query<&PeerRigResolveTask>,
    avatar_cache: Res<PeerAvatarCache>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (peer_entity, peer, backoff, floor, refresh, resolve) in &peers {
        // A muted peer gets none of this (#1219 f287). The fan-out is N+2
        // records per attempt - a DID document, a wardrobe record and up to
        // sixteen attachments, to hosts of THEIR choosing, on the shared
        // `IoTaskPool` - and it is the largest lever a peer retains over a
        // client that has decided it is done with them. Re-resolved on
        // unmute: this system runs every frame and `resolved` never rides
        // the wire, so nothing has to be remembered.
        if peer.muted {
            continue;
        }
        // A refresh in flight is fetching the saved record's outfit
        // (#1489), which is what this would fetch for the remembered one -
        // unless a live update has landed since, whose record the refresh
        // will not replace. Between a refresh's attempts, this takes over.
        let live_update =
            resolve.is_some_and(|resolve| matches!(resolve.avatar, FetchState::Landed));
        if refresh.is_some_and(|refresh| refresh.spawned) && !live_update {
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
        // skips it - leaving `resolved` `Some` with a SHORT attachment
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
        // set changed - a set-conditional check is precisely what a peer
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
            // say what was skipped and why (#1144) - a `None` alone cannot
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
            notice_seq: avatar_cache.notice_seq(),
            task,
        });
        // Stamped at START, not at completion: the cost this bounds is the
        // fan-out itself, which is already spent by the time a result lands.
        // `try_insert`, here and at the backoffs below (#1411): a peer
        // entity is despawned the frame its transport drops. These are the
        // stalest targets of the class - the backoffs address an `Entity`
        // carried in a task that spans frames, so the `peers.get()` guards
        // beside them cannot see a despawn that is merely QUEUED. An
        // ordinary insert landing on one aborts the client (#1410).
        commands
            .entity(peer_entity)
            .try_insert(PeerRigResolveFloor { started_at: now });
    }
}

/// Land finished resolutions onto their peers. A result whose reference
/// snapshot no longer matches the peer's current record is dropped - the
/// spawn system re-resolves against the newer references next frame.
pub(super) fn poll_peer_rig_resolutions(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerRigResolveTask)>,
    mut peers: Query<(
        &mut RemotePeer,
        &mut PeerResolve,
        Option<&PeerRigResolveBackoff>,
        Option<&AvatarPublishedAt>,
    )>,
    time: Res<Time>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut avatar_cache: ResMut<PeerAvatarCache>,
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
        // cross-owner, the one most likely to fail partially - yet it used to
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
        // Started before its peer's last save notice (#1490): what it read
        // may be the outfit from before that save, behind the very rkeys the
        // peer still names - the one change the reference check below cannot
        // see. Neither its outfit nor its failure is news about the saved
        // records, so it installs nothing, remembers nothing and backs nothing
        // off. The notice left `resolved` empty, so the spawner resolves the
        // saved outfit afresh, under the peer's floor (#1126).
        let notice_seq = task.notice_seq;
        let before_save = move |published: Option<&AvatarPublishedAt>| {
            published.is_some_and(|at| at.0 > notice_seq)
        };
        let stale = peers
            .get(task.peer_entity)
            .is_ok_and(|(_, _, _, published)| before_save(published));
        // The shortfall reaches the roster (#1217 f332). Until now the ONLY
        // trace of a worn item that could not be fetched was a session-log
        // line - and the failure is per-viewer, so two people in the same
        // room saw different outfits with no way to discover the
        // disagreement. The module's own comment named the consequence
        // ("why is Bob a bare chassis for me but not for Alice") without
        // giving anyone a way to answer it.
        // Only for the references still standing: a resolve of references
        // the peer has since changed - a refresh swapped the record in
        // (#1489) - says nothing about what they wear now.
        if !stale
            && let Ok((peer, mut resolve, _, _)) = peers.get_mut(task.peer_entity)
            && peer
                .avatar
                .as_ref()
                .and_then(|record| record.body.rigged_ref())
                .is_some_and(|rig| {
                    rig.avatar == task.avatar_rkey && rig.attachments == task.attachment_rkeys
                })
        {
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
            if stale {
                continue;
            }
            let rig = peers
                .get(task.peer_entity)
                .ok()
                .and_then(|(peer, _, backoff, _)| {
                    peer.avatar
                        .as_ref()
                        .and_then(|record| record.body.rigged_ref())
                        .map(|rig| PeerRigResolveBackoff::after_failure(backoff, rig, now))
                });
            if let Some(backoff) = rig {
                commands.entity(task.peer_entity).try_insert(backoff);
            }
            continue;
        };
        let Some((mut peer, _, previous_backoff, published)) = peers
            .iter_mut()
            .find(|(p, _, _, _)| p.peer_id == task.peer_id)
        else {
            continue;
        };
        if before_save(published) {
            continue;
        }
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
        // try - most often because their owner has not saved them yet, and
        // the record appears at that rkey the moment they do. The doubling
        // backoff that bounds an outright failure bounds this too, so a prop
        // that is never published settles into a slow poll rather than a
        // per-frame fan-out.
        let backoff =
            (!complete).then(|| PeerRigResolveBackoff::after_failure(previous_backoff, rig, now));
        // The next meeting stands at once in this outfit (#1489).
        avatar_cache.learn_resolution(
            &task.did,
            &task.avatar_rkey,
            &task.attachment_rkeys,
            &resolved,
        );
        rig.resolved = Some(resolved);
        match backoff {
            // Resolved in full: any wait recorded for these references is
            // spent.
            None => {
                commands
                    .entity(task.peer_entity)
                    .try_remove::<PeerRigResolveBackoff>();
            }
            Some(backoff) => {
                commands.entity(task.peer_entity).try_insert(backoff);
            }
        }
    }
}

/// Drain completed peer-avatar fetch tasks and install the fetched record
/// onto the matching `RemotePeer`. For a first meeting, a 404 means the peer
/// has never published an avatar, in which case we synthesise the
/// deterministic default keyed off their DID so their vessel is still
/// distinguishable from other "unpublished" peers. The refresh of a record
/// remembered at adoption (#1489) synthesises nothing: the remembered
/// record stands until the saved one lands.
pub(super) fn poll_peer_avatar_fetches(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PeerAvatarFetchTask)>,
    mut peers: Query<AvatarFetchPeer>,
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
        let spawned_at = task.spawned_at;
        let notice_seq = task.notice_seq;
        let refresh_of = task.refresh_of;
        // Record the fetch's spawn→resolve latency (E-4) before the task despawns.
        crate::diagnostics::samplers::avatar_fetch_latency_secs(&mut metrics, elapsed - spawned_at);
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
                // A 404 resolved to the DID-seeded default - still a successful
                // fetch (the peer simply hasn't published an avatar).
                crate::diagnostics::samplers::avatar_fetch_succeeded(&mut metrics);
                if refresh_of.is_some() {
                    info!(
                        "Peer {} ({}) refresh found no avatar record - the remembered one stands",
                        peer_id, did
                    );
                } else {
                    info!(
                        "Peer {} ({}) has no avatar record - synthesising default",
                        peer_id, did
                    );
                }
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
                warn!("Avatar fetch failed for {} ({}): {:?}", peer_id, did, err);
                // The two "no record" outcomes are NOT the same fact (#1217
                // f323). A 404 above is a finished question - this person has
                // not published an avatar - and the DID-seeded default IS
                // their appearance. A transport failure is an unanswered one,
                // and rendering it identically meant a one-second blip at
                // join time replaced someone's authored body with a
                // procedurally generated stranger, for the whole room, for
                // the whole session, with nothing said to anybody.
                let previous = peers
                    .iter()
                    .find(|(_, p, _, _, _)| p.peer_id == peer_id)
                    .and_then(|(_, _, resolve, _, _)| resolve.avatar.backoff().copied());
                (
                    AvatarRecord::default_for_did(&did),
                    false,
                    FetchState::Failed(RetryBackoff::after_failure(previous.as_ref(), elapsed)),
                )
            }
        };
        record.sanitize();

        if let Some(target) = refresh_of {
            // The refresh of a record remembered at adoption (#1489). It
            // lands on the entity it was started for, still wearing the
            // identity it was started for, or nowhere: a peer who has left
            // takes its answer with them, and the next meeting refreshes
            // again.
            let Ok((_, mut peer, mut resolve, Some(mut refresh), published)) =
                peers.get_mut(target)
            else {
                continue;
            };
            if peer.did.as_deref() != Some(did.as_str()) {
                continue;
            }
            if published.is_some_and(|at| at.0 > notice_seq) {
                // Started before this peer's last save notice, so its
                // records may be the ones from before that save: run it
                // again - under a fresh claim, so a peer who sends notices
                // at will cannot make this client fetch at will. Ordered
                // by the notice count, not the clock (#1494): a notice in
                // the frame after this started could share its time.
                refresh.spawned = false;
                refresh.claimed = false;
                refresh.not_before = elapsed;
                continue;
            }
            // After a few failures the wait tops out, and what came back is
            // taken as the answer: a short outfit is installed and its
            // missing items left to the rig resolver's own backoff (#1122),
            // and "no record" leaves the remembered one standing - a first
            // meeting's settled answers, reached a little later.
            let settled = refresh
                .backoff
                .is_some_and(|b| b.wait_secs >= config::network::PEER_FETCH_RETRY_MAX_SECS);
            // An outfit that came back without its body, or with fewer of
            // the standing references' items than the standing outfit
            // resolves - a wardrobe or attachment record that failed to
            // fetch - is no better than no answer until then.
            let short = cacheable
                && record
                    .body
                    .rigged_ref()
                    .is_some_and(|fresh| match &fresh.resolved {
                        None => true,
                        Some(resolved) => peer
                            .avatar
                            .as_ref()
                            .and_then(|standing| standing.body.rigged_ref())
                            .is_some_and(|standing| {
                                standing.avatar == fresh.avatar
                                    && standing.attachments == fresh.attachments
                                    && standing.resolved.as_ref().is_some_and(|held| {
                                        resolved.attachments.len() < held.attachments.len()
                                    })
                            }),
                    });
            if (!cacheable || short) && !settled {
                // A transport failure - a wardrobe fetch that failed in
                // transit is one too since #1493 - a "no record", or a short
                // outfit. The remembered avatar
                // stands and the state is left as it was - `Failed` means a
                // generated stand-in (#1217 f323) - and the refresh tries
                // again on a first meeting's doubling wait.
                let backoff = RetryBackoff::after_failure(refresh.backoff.as_ref(), elapsed);
                refresh.spawned = false;
                refresh.not_before = backoff.failed_at + backoff.wait_secs;
                refresh.backoff = Some(backoff);
                continue;
            }
            if !cacheable {
                if matches!(outcome, FetchState::Landed) {
                    // "No record", settled: the remembered one stands.
                    commands.entity(target).try_remove::<RefreshCachedAvatar>();
                } else {
                    // A transport failure keeps its doubling wait, as a
                    // first meeting's retry does, for as long as the peer
                    // stays.
                    let backoff = RetryBackoff::after_failure(refresh.backoff.as_ref(), elapsed);
                    refresh.spawned = false;
                    refresh.not_before = backoff.failed_at + backoff.wait_secs;
                    refresh.backoff = Some(backoff);
                }
                continue;
            }
            if matches!(resolve.avatar, FetchState::Landed) {
                // A live update came first. Its references are newer than
                // both, but the resolution it carried came from the
                // remembered record - `resolved` never rides the wire - so it
                // may be the one from before the save. Where the references
                // match, the fetched outfit replaces it.
                let fresh = record.body.rigged_ref();
                let stale = peer
                    .avatar
                    .as_ref()
                    .and_then(|live| live.body.rigged_ref())
                    .zip(fresh)
                    .is_some_and(|(live, fresh)| {
                        live.avatar == fresh.avatar
                            && live.attachments == fresh.attachments
                            && live.resolved != fresh.resolved
                    });
                if stale
                    && let Some(rig) = peer.avatar.as_mut().and_then(|live| live.body.rigged_mut())
                {
                    rig.resolved = fresh.and_then(|fresh| fresh.resolved.clone());
                }
                if let Some(live) = peer.avatar.clone() {
                    avatar_cache.insert(did, live);
                }
            } else {
                // The saved record replaces the remembered one.
                if peer.avatar.as_ref() != Some(&record) {
                    peer.avatar = Some(record.clone());
                }
                resolve.avatar = FetchState::Landed;
                avatar_cache.insert(did, record);
            }
            // What stands now says what the roster's outfit chip says: a
            // chip left by an earlier short resolve would otherwise stay.
            let (requested, installed) = peer
                .avatar
                .as_ref()
                .and_then(|standing| standing.body.rigged_ref())
                .map_or((0, 0), |rig| {
                    (
                        rig.attachments.len() as u32,
                        rig.resolved
                            .as_ref()
                            .map_or(0, |resolved| resolved.attachments.len() as u32),
                    )
                });
            PeerResolve::record_outfit(&mut resolve, requested, installed);
            commands.entity(target).try_remove::<RefreshCachedAvatar>();
            continue;
        }

        // Find the live peer entity; it may have despawned if the peer
        // disconnected between the fetch kick-off and its completion. What
        // the fetch brought is then still this client's latest word on them,
        // for the next meeting - which refreshes it anyway (#1489).
        let Some((target, mut peer, mut resolve, refresh, published)) = peers
            .iter_mut()
            .find(|(_, p, _, _, _)| p.peer_id == peer_id && p.did.as_deref() == Some(did.as_str()))
        else {
            if cacheable {
                avatar_cache.insert(did, record);
            }
            continue;
        };
        // A peer being refreshed is its refresh's to settle (#1489): this is
        // a first meeting's fetch that outlived an earlier entity with the
        // same peer id.
        if refresh.is_some() {
            continue;
        }
        // Only install the fetched record if we haven't already received a
        // newer state for this peer. An `AvatarStateUpdate` broadcast (the
        // live-preview nudge from a peer dragging a slider in the Avatar
        // Editor) can land between the fetch kick-off and its completion;
        // overwriting it here would permanently fracture visual state - this
        // client would see the old PDS record while every other peer in the
        // room sees the live preview.
        //
        // A stand-in installed by an earlier FAILED attempt may be replaced -
        // that is the whole point of the retry (#1217 f323). A live preview
        // may not: `AvatarStateUpdate` sets `avatar` to `Landed`, so the
        // state read here distinguishes "nothing real is standing" from
        // "something newer already arrived". And a failed attempt changes
        // nothing under a real record already standing: marking it `Failed`
        // called it a stand-in, and the retry that followed put one over it
        // (#1489's third review).
        let stand_in = resolve.avatar.is_failed();
        let nothing_real = peer.avatar.is_none() || stand_in;
        // Started before this peer's last save notice (#1490): what it
        // brought may be from before that save - the outfit behind the
        // references, or the references themselves - and nothing would fetch
        // it again. It is neither settled on nor remembered.
        if published.is_some_and(|at| at.0 > notice_seq) {
            if !nothing_real {
                // A live preview stands, newer than both.
                continue;
            }
            peer.avatar = Some(record);
            resolve.avatar = if cacheable {
                // It stands as a remembered record does, which beats a
                // stand-in, and the saved one comes by the refresh a
                // remembered record gets - under a claim, so a peer who
                // sends notices at will cannot make this client fetch at
                // will.
                commands
                    .entity(target)
                    .try_insert(RefreshCachedAvatar::default());
                FetchState::Pending
            } else {
                // "No record", or a failure, from before a save that may be
                // what made one: a stand-in either way, fetched again on a
                // failed fetch's doubling wait.
                match outcome {
                    FetchState::Failed(backoff) => FetchState::Failed(backoff),
                    _ => FetchState::Failed(RetryBackoff::after_failure(
                        resolve.avatar.backoff(),
                        elapsed,
                    )),
                }
            };
            continue;
        }
        if nothing_real {
            if cacheable {
                avatar_cache.insert(did, record.clone());
            }
            peer.avatar = Some(record);
            resolve.avatar = outcome;
        } else if matches!(outcome, FetchState::Landed) {
            // The live preview standing is newer, and it is what the cache
            // already holds (`handle_avatar_state`): this record is not
            // remembered over it (#1490).
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
    /// resolves, and that record is not on the owner's PDS yet - so it 404s
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

    /// A body with nothing worn is finished the moment it resolves - the
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
    /// every edit they make. This fan-out is the biggest lever they retain -
    /// N+2 records per attempt, to hosts of their choosing, on the shared
    /// `IoTaskPool` - and it had no mute check at all.
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
        app.init_resource::<PeerAvatarCache>();
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
    /// room re-run the whole fan-out - a DID document, a wardrobe record
    /// and up to sixteen attachments - per round trip, to hosts of that
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

    fn peer_id(n: u8) -> PeerId {
        serde_json::from_str(&format!("\"00000000-0000-0000-0000-0000000000{n:02}\""))
            .expect("a well-formed uuid")
    }

    /// A DID no directory is asked about: `did:key` has no document, so a
    /// fetch for it fails at once, with no network (#1489's tests).
    fn key_did(n: u8) -> String {
        format!("did:key:z6MkrememberedPeer{n:02}")
    }

    fn outfit(seed: &str) -> crate::pds::avatar::ResolvedRig {
        crate::pds::avatar::ResolvedRig {
            body: crate::pds::avatar::wardrobe::engine_default_for_did(seed),
            attachments: Vec::new(),
        }
    }

    /// `rkey` worn, resolved to `seed`'s body.
    fn wearing(rkey: &str, seed: &str) -> AvatarRecord {
        let mut record = AvatarRecord::wearing(rkey);
        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(outfit(seed));
        }
        record
    }

    fn remembered_peer(n: u8, avatar: AvatarRecord) -> RemotePeer {
        RemotePeer {
            peer_id: peer_id(n),
            did: Some(key_did(n)),
            handle: None,
            muted: false,
            avatar: Some(avatar),
            build: None,
            connected_at: 0.0,
        }
    }

    fn in_flight() -> RefreshCachedAvatar {
        RefreshCachedAvatar {
            spawned: true,
            claimed: true,
            ..Default::default()
        }
    }

    fn live() -> PeerResolve {
        PeerResolve {
            avatar: FetchState::Landed,
            ..Default::default()
        }
    }

    /// A task already finished when it is handed over (#1295's pattern):
    /// spawned on a one-thread pool of this module's own and waited for
    /// against the clock, so what is tested is what the pollers do with an
    /// answer, not whether the shared pool got round to producing one.
    fn finished<T: Send + 'static>(value: T) -> bevy::tasks::Task<T> {
        static POOL: std::sync::OnceLock<bevy::tasks::TaskPool> = std::sync::OnceLock::new();
        let pool = POOL.get_or_init(|| {
            bevy::tasks::TaskPoolBuilder::new()
                .num_threads(1)
                .thread_name("peer-cache-test".into())
                .build()
        });
        let task = pool.spawn(async move { value });
        let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !task.is_finished() {
            assert!(
                std::time::Instant::now() < give_up_at,
                "a trivial task never ran"
            );
            std::thread::yield_now();
        }
        task
    }

    /// The resources the pollers read, and no systems.
    fn bare_app() -> App {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.init_resource::<SessionLog>()
            .init_resource::<PeerAvatarCache>()
            .init_resource::<crate::diagnostics::MetricsRegistry>();
        app
    }

    fn refresh_app() -> App {
        let mut app = bare_app();
        app.add_systems(Update, poll_peer_avatar_fetches);
        app
    }

    /// Peer `n`'s fetch, started at 1 s and already answered; a refresh of
    /// `refresh_of` when given.
    fn answer(
        app: &mut App,
        n: u8,
        result: Result<Option<AvatarRecord>, pds::FetchError>,
        refresh_of: Option<Entity>,
    ) {
        app.world_mut().spawn(PeerAvatarFetchTask {
            peer_id: peer_id(n),
            did: key_did(n),
            task: finished(result),
            spawned_at: 1.0,
            notice_seq: 1,
            refresh_of,
        });
    }

    fn offline() -> Result<Option<AvatarRecord>, pds::FetchError> {
        Err(pds::FetchError::Network(String::from("offline")))
    }

    fn standing(app: &App, entity: Entity) -> AvatarRecord {
        app.world()
            .get::<RemotePeer>(entity)
            .and_then(|peer| peer.avatar.clone())
            .expect("a record stands")
    }

    fn rig_of(record: &AvatarRecord) -> &crate::pds::avatar::RiggedBody {
        record.body.rigged_ref().expect("a rigged body")
    }

    fn remembered(app: &App, n: u8) -> Option<AvatarRecord> {
        app.world()
            .resource::<PeerAvatarCache>()
            .get(&key_did(n))
            .cloned()
    }

    /// #1489: the refresh of a record remembered at adoption swaps in the
    /// saved record - a switched body here - and remembers it for the next
    /// meeting.
    #[test]
    fn a_refresh_replaces_the_remembered_record_and_remembers_the_saved_one() {
        let mut app = refresh_app();
        let peer = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing("3jzfcijpj2z2a", "old")),
                PeerResolve::default(),
                in_flight(),
            ))
            .id();
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "saved"))),
            Some(peer),
        );
        app.update();

        assert_eq!(rig_of(&standing(&app, peer)).avatar, "3jzfcijpj2z2b");
        assert!(app.world().get::<RefreshCachedAvatar>(peer).is_none());
        assert_eq!(
            remembered(&app, 1).map(|record| rig_of(&record).avatar.clone()),
            Some(String::from("3jzfcijpj2z2b"))
        );
    }

    /// #1489's second and third reviews: a live update that came first
    /// keeps its references - they are newer than both - but the outfit it
    /// carried came from the remembered record, maybe from before the save.
    /// Where the references match, a complete fetched outfit replaces it
    /// and the cache learns it; where they differ, the live update stands;
    /// an outfit that came back short is a failed attempt, tried again.
    #[test]
    fn a_live_update_keeps_its_references_and_takes_the_fresh_outfit() {
        let mut app = refresh_app();
        let spawn = |app: &mut App, n: u8, rkey: &str| {
            app.world_mut()
                .spawn((
                    remembered_peer(n, wearing(rkey, "before")),
                    live(),
                    in_flight(),
                ))
                .id()
        };
        let same = spawn(&mut app, 1, "3jzfcijpj2z2a");
        let moved_on = spawn(&mut app, 2, "3jzfcijpj2z2c");
        let short = spawn(&mut app, 3, "3jzfcijpj2z2a");
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2a", "after"))),
            Some(same),
        );
        answer(
            &mut app,
            2,
            Ok(Some(wearing("3jzfcijpj2z2a", "after"))),
            Some(moved_on),
        );
        answer(
            &mut app,
            3,
            Ok(Some(AvatarRecord::wearing("3jzfcijpj2z2a"))),
            Some(short),
        );
        app.update();

        let fresh = standing(&app, same);
        assert_eq!(rig_of(&fresh).avatar, "3jzfcijpj2z2a");
        assert_eq!(rig_of(&fresh).resolved, Some(outfit("after")));
        assert_eq!(
            remembered(&app, 1).and_then(|record| rig_of(&record).resolved.clone()),
            Some(outfit("after")),
            "and the next meeting stands in it"
        );
        let untouched = standing(&app, moved_on);
        assert_eq!(rig_of(&untouched).avatar, "3jzfcijpj2z2c");
        assert_eq!(rig_of(&untouched).resolved, Some(outfit("before")));
        assert_eq!(
            rig_of(&standing(&app, short)).resolved,
            Some(outfit("before")),
            "a short outfit takes nothing off"
        );
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(short)
                .is_some_and(|refresh| !refresh.spawned),
            "and is tried again"
        );
    }

    /// #1489's reviews: a refresh that brings no record - a transport
    /// failure, or a "no record" (a profile naming a body that is gone) -
    /// leaves the remembered body standing, never calls it a stand-in
    /// (#1217 f323), and tries again on a first meeting's doubling wait,
    /// however many times it has failed.
    #[test]
    fn a_failed_refresh_keeps_the_remembered_body_and_tries_again() {
        let mut app = refresh_app();
        let mut long_outage = None;
        for _ in 0..10 {
            long_outage = Some(RetryBackoff::after_failure(long_outage.as_ref(), 0.0));
        }
        let spawn = |app: &mut App, n: u8, backoff: Option<RetryBackoff>| {
            app.world_mut()
                .spawn((
                    remembered_peer(n, wearing("3jzfcijpj2z2a", "kept")),
                    PeerResolve::default(),
                    RefreshCachedAvatar {
                        backoff,
                        ..in_flight()
                    },
                ))
                .id()
        };
        let failed = spawn(&mut app, 1, None);
        let empty = spawn(&mut app, 2, None);
        let still_down = spawn(&mut app, 3, long_outage);
        let settled_empty = spawn(&mut app, 4, long_outage);
        let twice = spawn(&mut app, 5, Some(RetryBackoff::after_failure(None, 0.0)));
        answer(&mut app, 1, offline(), Some(failed));
        answer(&mut app, 5, offline(), Some(twice));
        answer(&mut app, 2, Ok(None), Some(empty));
        answer(&mut app, 3, offline(), Some(still_down));
        answer(&mut app, 4, Ok(None), Some(settled_empty));
        app.update();

        for peer in [failed, empty, still_down, settled_empty] {
            assert_eq!(
                rig_of(&standing(&app, peer)).resolved,
                Some(outfit("kept")),
                "the remembered body stands"
            );
            assert!(
                !app.world()
                    .get::<PeerResolve>(peer)
                    .is_some_and(|resolve| resolve.avatar.is_failed()),
                "and is not called a stand-in"
            );
        }
        for peer in [failed, empty, still_down] {
            let refresh = app
                .world()
                .get::<RefreshCachedAvatar>(peer)
                .expect("the refresh stays due, however long the outage");
            assert!(!refresh.spawned, "for another attempt");
            assert!(
                refresh.backoff.is_some() && refresh.not_before > 0.0,
                "after its wait"
            );
        }
        let wait = |peer: Entity| {
            app.world()
                .get::<RefreshCachedAvatar>(peer)
                .and_then(|refresh| refresh.backoff)
                .expect("a backoff")
        };
        assert_eq!(
            wait(failed).wait_secs,
            config::network::PEER_FETCH_RETRY_BASE_SECS
        );
        assert_eq!(
            wait(twice).wait_secs,
            2.0 * config::network::PEER_FETCH_RETRY_BASE_SECS,
            "each failure doubles the wait"
        );
        assert_eq!(wait(still_down).attempts, 11, "the doubling goes on");
        assert_eq!(
            wait(still_down).wait_secs,
            config::network::PEER_FETCH_RETRY_MAX_SECS,
            "up to its ceiling, however long the outage"
        );
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(settled_empty)
                .is_none(),
            "once the wait tops out, \"no record\" is settled and the remembered one stays"
        );
    }

    /// #1489's reviews: a refresh that started before the owner's save
    /// notice may bring the records from before that save. It is neither
    /// installed nor remembered, and runs again at once under the claim it
    /// holds. A notice that came before the refresh started does not count.
    #[test]
    fn a_refresh_started_before_a_save_notice_runs_again() {
        let mut app = refresh_app();
        let spawn = |app: &mut App, n: u8, notice_at: u64| {
            app.world_mut()
                .spawn((
                    remembered_peer(n, wearing("3jzfcijpj2z2a", "remembered")),
                    PeerResolve::default(),
                    in_flight(),
                    AvatarPublishedAt(notice_at),
                ))
                .id()
        };
        let overtaken = spawn(&mut app, 1, 2);
        let after_notice = spawn(&mut app, 2, 1);
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "pre-save"))),
            Some(overtaken),
        );
        answer(
            &mut app,
            2,
            Ok(Some(wearing("3jzfcijpj2z2b", "saved"))),
            Some(after_notice),
        );
        app.update();

        assert_eq!(rig_of(&standing(&app, overtaken)).avatar, "3jzfcijpj2z2a");
        assert!(remembered(&app, 1).is_none(), "nor remembered");
        let rerun = app
            .world()
            .get::<RefreshCachedAvatar>(overtaken)
            .expect("the refresh runs again");
        assert!(
            !rerun.spawned && !rerun.claimed,
            "under a fresh claim, so notices sent at will cannot make it fetch at will"
        );
        assert_eq!(
            rig_of(&standing(&app, after_notice)).avatar,
            "3jzfcijpj2z2b",
            "a refresh started after the notice is the saved record"
        );
    }

    /// #1489's third review: a refresh lands on the entity it was started
    /// for, or nowhere. The peer left, and this client hopped out and back:
    /// the later entity with the same peer id is not the refresh's to touch,
    /// with a failure (which marked the real body `Failed`, and a retry then
    /// put a stand-in over it) or with an answer.
    #[test]
    fn a_refresh_for_a_peer_who_left_lands_nowhere() {
        let mut app = refresh_app();
        let gone = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(gone);
        let returned = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing("3jzfcijpj2z2a", "remembered")),
                PeerResolve::default(),
                in_flight(),
            ))
            .id();
        answer(&mut app, 1, offline(), Some(gone));
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "elsewhere"))),
            Some(gone),
        );
        app.update();

        assert_eq!(rig_of(&standing(&app, returned)).avatar, "3jzfcijpj2z2a");
        assert!(
            !app.world()
                .get::<PeerResolve>(returned)
                .is_some_and(|resolve| resolve.avatar.is_failed())
        );
        assert!(remembered(&app, 1).is_none());
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(returned)
                .is_some_and(|refresh| refresh.spawned && refresh.backoff.is_none()),
            "and the returned entity's own refresh is left to land"
        );
    }

    /// #1489's third review: a first meeting's fetch that fails under a real
    /// record already standing - a live preview - changes nothing. Marking
    /// it `Failed` called it a stand-in, and the retry put one over it.
    #[test]
    fn a_failed_first_meeting_fetch_leaves_a_real_record_standing() {
        let mut app = refresh_app();
        let peer = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing("3jzfcijpj2z2a", "preview")),
                live(),
            ))
            .id();
        answer(&mut app, 1, offline(), None);
        app.update();

        assert_eq!(
            rig_of(&standing(&app, peer)).resolved,
            Some(outfit("preview"))
        );
        assert!(
            app.world()
                .get::<PeerResolve>(peer)
                .is_some_and(|resolve| matches!(resolve.avatar, FetchState::Landed)),
            "still the live preview's, not a stand-in's"
        );
    }

    /// #1489's third review: a first meeting's fetch that outlived an earlier
    /// entity with the same peer id leaves a peer being refreshed to its
    /// own refresh - nothing installed, the state untouched.
    #[test]
    fn a_first_meeting_fetch_leaves_a_refreshing_peer_to_its_refresh() {
        let mut app = refresh_app();
        let peer = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing("3jzfcijpj2z2a", "remembered")),
                PeerResolve::default(),
                in_flight(),
            ))
            .id();
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "stale fetch"))),
            None,
        );
        app.update();

        assert_eq!(rig_of(&standing(&app, peer)).avatar, "3jzfcijpj2z2a");
        assert!(
            app.world()
                .get::<PeerResolve>(peer)
                .is_some_and(|resolve| matches!(resolve.avatar, FetchState::Pending))
        );
        assert!(app.world().get::<RefreshCachedAvatar>(peer).is_some());
    }

    /// A first meeting: a peer with nothing standing yet, whose last save
    /// notice is `notice` in the count (`0` for none).
    fn met(app: &mut App, n: u8, notice: u64) -> Entity {
        let mut peer = remembered_peer(n, wearing("3jzfcijpj2z2a", "unused"));
        peer.avatar = None;
        let entity = app.world_mut().spawn((peer, PeerResolve::default())).id();
        if notice > 0 {
            app.world_mut()
                .entity_mut(entity)
                .insert(AvatarPublishedAt(notice));
        }
        entity
    }

    fn state(app: &App, entity: Entity) -> FetchState {
        app.world()
            .get::<PeerResolve>(entity)
            .expect("a resolve record")
            .avatar
    }

    /// #1490: a first meeting's fetch started (count 1) before the owner's
    /// save notice (count 2) may bring the record from before that save. It
    /// stands, as a remembered record would - better than a stand-in - but
    /// it is not settled on: the refresh a remembered record gets fetches the
    /// saved one, under a claim. Nor is it remembered. A fetch that started
    /// after the notice is the saved record, settled and remembered.
    #[test]
    fn a_first_meeting_fetch_from_before_a_save_stands_until_the_saved_one_comes() {
        let mut app = refresh_app();
        let overtaken = met(&mut app, 1, 2);
        let after_notice = met(&mut app, 2, 1);
        for n in [1, 2] {
            answer(
                &mut app,
                n,
                Ok(Some(wearing("3jzfcijpj2z2b", "fetched"))),
                None,
            );
        }
        app.update();

        assert_eq!(rig_of(&standing(&app, overtaken)).avatar, "3jzfcijpj2z2b");
        assert_eq!(state(&app, overtaken), FetchState::Pending, "not settled");
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(overtaken)
                .is_some_and(|refresh| !refresh.spawned && !refresh.claimed),
            "the saved record is fetched by a refresh, under a claim of its own"
        );
        assert!(remembered(&app, 1).is_none(), "nor remembered");

        assert_eq!(state(&app, after_notice), FetchState::Landed);
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(after_notice)
                .is_none()
        );
        assert!(
            remembered(&app, 2).is_some(),
            "a fetch that started after the notice is the saved record"
        );
    }

    /// #1490: "no record", or a failure, from before a save that may be what
    /// made the record is a stand-in, asked again on a failed fetch's wait -
    /// never the settled answer a first meeting used to keep for good.
    #[test]
    fn a_first_meeting_no_record_from_before_a_save_is_asked_again() {
        let mut app = refresh_app();
        let overtaken = met(&mut app, 1, 2);
        let failed = met(&mut app, 2, 2);
        let after_notice = met(&mut app, 3, 1);
        answer(&mut app, 1, Ok(None), None);
        answer(&mut app, 2, offline(), None);
        answer(&mut app, 3, Ok(None), None);
        app.update();

        assert!(state(&app, overtaken).is_failed(), "asked again");
        assert!(state(&app, failed).is_failed());
        assert!(
            app.world()
                .get::<RemotePeer>(overtaken)
                .is_some_and(|peer| peer.avatar.is_some()),
            "a stand-in stands meanwhile"
        );
        assert_eq!(
            state(&app, after_notice),
            FetchState::Landed,
            "after the notice, \"no record\" is the answer"
        );
    }

    /// #1490: a first meeting's fetch landing under a live preview is older
    /// than it. The install was already skipped; the cache insert was not,
    /// so the next meeting stood in the older record. Now neither happens.
    #[test]
    fn a_first_meeting_fetch_does_not_overwrite_a_newer_preview_in_the_cache() {
        let mut app = refresh_app();
        let preview = wearing("3jzfcijpj2z2a", "preview");
        app.world_mut()
            .resource_mut::<PeerAvatarCache>()
            .insert(key_did(1), preview.clone());
        let peer = app
            .world_mut()
            .spawn((remembered_peer(1, preview.clone()), live()))
            .id();
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "older"))),
            None,
        );
        app.update();

        assert_eq!(standing(&app, peer), preview);
        assert_eq!(remembered(&app, 1), Some(preview));
    }

    /// #1490: a rig resolution that started (count 1) before the owner's save
    /// notice (count 2) read the PDS before the save, behind the very rkeys
    /// the peer still names. Its outfit is neither installed nor remembered,
    /// and its failure backs nothing off - neither says anything about the
    /// saved records. One that started after the notice lands as before.
    #[test]
    fn a_rig_resolution_from_before_a_save_is_not_installed() {
        let mut app = bare_app();
        app.add_systems(Update, poll_peer_rig_resolutions);
        for n in 1..=4 {
            app.world_mut()
                .resource_mut::<PeerAvatarCache>()
                .insert(key_did(n), AvatarRecord::wearing("3jzfcijpj2z2a"));
        }
        // (peer, last notice, outfit the resolution brought)
        let cases = [(1, 2, true), (2, 1, true), (3, 2, false), (4, 1, false)];
        let mut entities = Vec::new();
        for (n, notice, resolved) in cases {
            let peer = app
                .world_mut()
                .spawn((
                    remembered_peer(n, AvatarRecord::wearing("3jzfcijpj2z2a")),
                    PeerResolve::default(),
                    AvatarPublishedAt(notice),
                ))
                .id();
            app.world_mut().spawn(PeerRigResolveTask {
                peer_id: peer_id(n),
                peer_entity: peer,
                did: key_did(n),
                avatar_rkey: String::from("3jzfcijpj2z2a"),
                attachment_rkeys: Vec::new(),
                notice_seq: 1,
                task: finished((
                    resolved.then(|| outfit("read")),
                    crate::pds::avatar::wardrobe::ResolveReport::default(),
                )),
            });
            entities.push(peer);
        }
        app.update();

        let resolved = |entity: Entity| rig_of(&standing(&app, entity)).resolved.clone();
        let backed_off =
            |entity: Entity| app.world().get::<PeerRigResolveBackoff>(entity).is_some();
        assert_eq!(resolved(entities[0]), None, "from before the save");
        assert!(
            remembered(&app, 1).is_some_and(|record| rig_of(&record).resolved.is_none()),
            "and not remembered either"
        );
        assert_eq!(resolved(entities[1]), Some(outfit("read")));
        assert!(remembered(&app, 2).is_some_and(|record| rig_of(&record).resolved.is_some()));
        assert!(!backed_off(entities[2]), "a failure from before the save");
        assert!(backed_off(entities[3]), "a failure after it waits as ever");
    }

    /// #1494: the order of a save notice and a fetch is read from the count,
    /// so it holds when both happen in one frame - or in two frames a
    /// coarsened browser clock gives one time - where comparing session
    /// seconds could not tell which came first.
    #[test]
    fn a_save_notice_and_a_fetch_are_ordered_by_the_count_not_the_clock() {
        let mut cache = PeerAvatarCache::default();
        let started_before = cache.notice_seq();
        let notice = cache.note_publish();
        let started_after = cache.notice_seq();
        assert!(
            notice > started_before,
            "the fetch before the notice is stale"
        );
        assert!(notice <= started_after, "the one after it is not");
        cache.clear();
        assert_eq!(
            cache.notice_seq(),
            notice,
            "a logout keeps the count, so a task that outlives it cannot pass for a later one"
        );
    }

    /// #1489's third review: a DID refreshed moments ago waits out the rest
    /// of its window - the refresh is delayed, not dropped.
    #[test]
    fn a_refresh_claim_is_delayed_not_refused() {
        let mut cache = PeerAvatarCache::default();
        let window = config::network::PEER_AVATAR_REFRESH_MIN_SECS;
        assert_eq!(cache.claim_refresh("did:key:a", 0.0), Ok(()));
        assert_eq!(cache.claim_refresh("did:key:a", 1.0), Err(window));
        assert_eq!(cache.claim_refresh("did:key:b", 1.0), Ok(()), "per DID");
        assert_eq!(cache.claim_refresh("did:key:a", window), Ok(()));
    }

    /// #1489: a peer installed from the cache is refreshed once - not once
    /// a frame - a muted one waits for the unmute (#1219 f287), one inside
    /// its DID's claim window waits for its end, and a retry still waiting
    /// does not start. One peer here can start, so the in-flight cap never
    /// decides. The fetch spawned here is for a `did:key`, which fails with
    /// no network.
    #[test]
    fn a_remembered_peer_is_refreshed_once_and_a_muted_one_waits() {
        // No poller: a `did:key` fetch fails at once and would be taken
        // before the count.
        let mut app = bare_app();
        app.add_systems(Update, spawn_cached_avatar_refreshes);
        let spawn = |app: &mut App, n: u8, muted: bool, refresh: RefreshCachedAvatar| {
            let mut peer = remembered_peer(n, wearing("3jzfcijpj2z2a", "kept"));
            peer.muted = muted;
            app.world_mut().spawn((peer, refresh)).id()
        };
        let familiar = spawn(&mut app, 1, false, RefreshCachedAvatar::default());
        let muted = spawn(&mut app, 2, true, RefreshCachedAvatar::default());
        let windowed = spawn(&mut app, 3, false, RefreshCachedAvatar::default());
        let waiting = spawn(
            &mut app,
            5,
            false,
            RefreshCachedAvatar {
                claimed: true,
                not_before: 1.0e9,
                ..Default::default()
            },
        );
        {
            let mut cache = app.world_mut().resource_mut::<PeerAvatarCache>();
            assert_eq!(cache.claim_refresh(&key_did(3), 0.0), Ok(()));
        }

        for _ in 0..3 {
            app.update();
        }

        let mut tasks = app.world_mut().query::<&PeerAvatarFetchTask>();
        let mut fetched: Vec<(PeerId, Option<Entity>)> = tasks
            .iter(app.world())
            .map(|task| (task.peer_id, task.refresh_of))
            .collect();
        fetched.sort_by_key(|(id, _)| id.to_string());
        assert_eq!(
            fetched,
            vec![(peer_id(1), Some(familiar))],
            "one fetch, for the familiar peer, aimed at its entity"
        );
        let refresh = |entity: Entity| {
            app.world()
                .get::<RefreshCachedAvatar>(entity)
                .expect("the refresh stays")
        };
        assert!(!refresh(muted).spawned, "the muted peer's refresh waits");
        assert!(
            !refresh(windowed).spawned
                && refresh(windowed).not_before == config::network::PEER_AVATAR_REFRESH_MIN_SECS,
            "inside the window the refresh waits for its end"
        );
        assert!(!refresh(waiting).spawned, "a retry waits out its backoff");
    }

    /// #1489's fourth review: a retry after a failed attempt holds the claim
    /// its refresh won, so it starts inside its DID's claim window.
    #[test]
    fn a_retry_needs_no_new_claim() {
        let mut app = bare_app();
        app.add_systems(Update, spawn_cached_avatar_refreshes);
        let retry = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing("3jzfcijpj2z2a", "kept")),
                RefreshCachedAvatar {
                    claimed: true,
                    ..Default::default()
                },
            ))
            .id();
        assert_eq!(
            app.world_mut()
                .resource_mut::<PeerAvatarCache>()
                .claim_refresh(&key_did(1), 0.0),
            Ok(())
        );
        app.update();

        let mut tasks = app.world_mut().query::<&PeerAvatarFetchTask>();
        let fetched: Vec<Option<Entity>> = tasks
            .iter(app.world())
            .map(|task| task.refresh_of)
            .collect();
        assert_eq!(fetched, vec![Some(retry)]);
    }

    /// #1489's reviews: while a refresh fetches the outfit the standing
    /// record names, the rig resolver does not fetch it a second time; once
    /// a live update has changed the record, or while the refresh waits
    /// between attempts, the resolver does its own work. The resolutions
    /// spawned here are for `did:key`s, which fail with no network.
    #[test]
    fn the_resolver_steps_aside_only_while_a_refresh_fetches_the_same_outfit() {
        let mut app = App::new();
        app.add_plugins((bevy::app::TaskPoolPlugin::default(), bevy::time::TimePlugin));
        app.init_resource::<PeerAvatarCache>();
        app.add_systems(Update, spawn_peer_rig_resolutions);
        let spawn = |app: &mut App, n: u8, resolve: PeerResolve, refresh: RefreshCachedAvatar| {
            app.world_mut()
                .spawn((
                    remembered_peer(n, AvatarRecord::wearing("3jzfcijpj2z2a")),
                    resolve,
                    refresh,
                ))
                .id()
        };
        spawn(&mut app, 1, PeerResolve::default(), in_flight());
        spawn(&mut app, 2, live(), in_flight());
        spawn(
            &mut app,
            3,
            PeerResolve::default(),
            RefreshCachedAvatar::default(),
        );

        app.update();

        let mut tasks = app.world_mut().query::<&PeerRigResolveTask>();
        let mut resolving: Vec<String> = tasks
            .iter(app.world())
            .map(|task| task.peer_id.to_string())
            .collect();
        resolving.sort();
        assert_eq!(
            resolving,
            vec![peer_id(2).to_string(), peer_id(3).to_string()]
        );
    }

    /// #1489's reviews: the cache never learned a resolution, so the next
    /// meeting stood in an older outfit, or bare. A resolution that lands
    /// is written through - but only into an entry that names the same
    /// records.
    #[test]
    fn a_landed_outfit_is_remembered_for_the_next_meeting() {
        let mut app = bare_app();
        app.add_systems(Update, poll_peer_rig_resolutions);
        let mut cache = PeerAvatarCache::default();
        cache.insert(key_did(1), AvatarRecord::wearing("3jzfcijpj2z2a"));
        cache.insert(key_did(2), AvatarRecord::wearing("3jzfcijpj2z2z"));
        app.insert_resource(cache);
        for (n, rkey) in [(1, "3jzfcijpj2z2a"), (2, "3jzfcijpj2z2b")] {
            let peer = app
                .world_mut()
                .spawn((
                    remembered_peer(n, AvatarRecord::wearing(rkey)),
                    PeerResolve::default(),
                ))
                .id();
            app.world_mut().spawn(PeerRigResolveTask {
                peer_id: peer_id(n),
                peer_entity: peer,
                did: key_did(n),
                avatar_rkey: String::from(rkey),
                attachment_rkeys: Vec::new(),
                notice_seq: 0,
                task: finished((
                    Some(outfit("landed")),
                    crate::pds::avatar::wardrobe::ResolveReport::default(),
                )),
            });
        }
        app.update();

        assert_eq!(
            remembered(&app, 1).and_then(|record| rig_of(&record).resolved.clone()),
            Some(outfit("landed"))
        );
        assert!(
            remembered(&app, 2).is_some_and(|record| rig_of(&record).resolved.is_none()),
            "an entry naming other records is left alone"
        );
    }

    /// #1489's second review: the cache is wiped at logout, but a fetch
    /// still in flight - an avatar record or an outfit - would land in the
    /// next session's.
    #[test]
    fn a_logout_drops_the_fetches_still_in_flight() {
        // No poller, which would take the finished task itself.
        let mut app = bare_app();
        app.add_systems(Update, drop_inflight_peer_fetches);
        let peer = app.world_mut().spawn_empty().id();
        answer(&mut app, 1, Ok(None), None);
        app.world_mut().spawn(PeerRigResolveTask {
            peer_id: peer_id(1),
            peer_entity: peer,
            did: key_did(1),
            avatar_rkey: String::from("3jzfcijpj2z2a"),
            attachment_rkeys: Vec::new(),
            notice_seq: 0,
            task: finished((None, crate::pds::avatar::wardrobe::ResolveReport::default())),
        });
        app.update();

        let mut avatar = app.world_mut().query::<&PeerAvatarFetchTask>();
        assert_eq!(avatar.iter(app.world()).count(), 0);
        let mut rig = app.world_mut().query::<&PeerRigResolveTask>();
        assert_eq!(rig.iter(app.world()).count(), 0);
    }

    fn worn(rkey: &str) -> crate::pds::avatar::ResolvedAttachment {
        crate::pds::avatar::ResolvedAttachment {
            rkey: String::from(rkey),
            record: crate::pds::avatar::wardrobe::AttachmentRecord {
                lex_type: crate::pds::AVATAR_ATTACHMENT_COLLECTION.into(),
                item: serde_json::from_value(serde_json::json!({
                    "$type": "network.symbios.gen.cuboid",
                    "size": [1000, 1000, 1000],
                    "solid": false
                }))
                .expect("a cuboid"),
                socket: String::from("head"),
                offset: Default::default(),
                fit_band_mm: 0,
                source: None,
            },
        }
    }

    /// `rkey` worn with `att-1`, resolved to `seed` with that item fetched
    /// or not.
    fn wearing_one(rkey: &str, seed: &str, fetched: bool) -> AvatarRecord {
        let mut record = wearing(rkey, seed);
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments = vec![String::from("att-1")];
            if fetched && let Some(resolved) = rig.resolved.as_mut() {
                resolved.attachments = vec![worn("att-1")];
            }
        }
        record
    }

    /// #1489's fourth review: an outfit that comes back without its body,
    /// or with fewer items than the standing one resolves under the same
    /// references, waits for another attempt - until the wait tops out,
    /// when it is taken, and the missing items are the rig resolver's.
    #[test]
    fn a_short_outfit_waits_until_the_wait_tops_out() {
        let mut app = refresh_app();
        let mut topped_out = None;
        for _ in 0..10 {
            topped_out = Some(RetryBackoff::after_failure(topped_out.as_ref(), 0.0));
        }
        let spawn = |app: &mut App, n: u8, backoff: Option<RetryBackoff>| {
            app.world_mut()
                .spawn((
                    remembered_peer(n, wearing_one("3jzfcijpj2z2a", "held", true)),
                    PeerResolve::default(),
                    RefreshCachedAvatar {
                        backoff,
                        ..in_flight()
                    },
                ))
                .id()
        };
        let fewer = spawn(&mut app, 1, None);
        let bodiless = spawn(&mut app, 2, None);
        let taken = spawn(&mut app, 3, topped_out);
        answer(
            &mut app,
            1,
            Ok(Some(wearing_one("3jzfcijpj2z2a", "fetched", false))),
            Some(fewer),
        );
        answer(
            &mut app,
            2,
            Ok(Some(AvatarRecord::wearing("3jzfcijpj2z2b"))),
            Some(bodiless),
        );
        answer(
            &mut app,
            3,
            Ok(Some(wearing_one("3jzfcijpj2z2a", "fetched", false))),
            Some(taken),
        );
        app.update();

        for peer in [fewer, bodiless] {
            assert_eq!(
                rig_of(&standing(&app, peer)).resolved,
                wearing_one("3jzfcijpj2z2a", "held", true)
                    .body
                    .rigged_ref()
                    .and_then(|rig| rig.resolved.clone()),
                "the fuller outfit stands"
            );
            assert!(app.world().get::<RefreshCachedAvatar>(peer).is_some());
        }
        assert_eq!(
            rig_of(&standing(&app, taken))
                .resolved
                .as_ref()
                .map(|resolved| resolved.body.clone()),
            Some(outfit("fetched").body),
            "once the wait tops out the answer is taken"
        );
        assert!(app.world().get::<RefreshCachedAvatar>(taken).is_none());
    }

    /// #1489's fourth review: a refresh lands only on the identity it was
    /// started for. An entity that has adopted another DID since is not its
    /// to touch; nor is a first meeting's fetch installed on an entity
    /// whose DID is not adopted yet.
    #[test]
    fn a_fetch_lands_only_on_the_identity_it_was_for() {
        let mut app = refresh_app();
        let mut other = remembered_peer(1, wearing("3jzfcijpj2z2a", "someone else"));
        other.did = Some(key_did(9));
        let changed = app
            .world_mut()
            .spawn((other, PeerResolve::default(), in_flight()))
            .id();
        let mut unknown = remembered_peer(2, wearing("3jzfcijpj2z2a", "unused"));
        unknown.did = None;
        unknown.avatar = None;
        let unadopted = app
            .world_mut()
            .spawn((unknown, PeerResolve::default()))
            .id();
        answer(
            &mut app,
            1,
            Ok(Some(wearing("3jzfcijpj2z2b", "first identity"))),
            Some(changed),
        );
        answer(
            &mut app,
            2,
            Ok(Some(wearing("3jzfcijpj2z2b", "early"))),
            None,
        );
        app.update();

        assert_eq!(rig_of(&standing(&app, changed)).avatar, "3jzfcijpj2z2a");
        assert!(
            app.world()
                .get::<RefreshCachedAvatar>(changed)
                .is_some_and(|refresh| refresh.spawned),
            "its own refresh is left to land"
        );
        assert!(
            app.world()
                .get::<RemotePeer>(unadopted)
                .is_some_and(|peer| peer.avatar.is_none())
        );
    }

    /// #1489's fourth review: the roster's outfit chip follows what the
    /// refresh stood up; and a rig resolve for references the peer no
    /// longer wears says nothing about the outfit they do.
    #[test]
    fn the_outfit_chip_follows_what_stands() {
        let mut app = refresh_app();
        app.add_systems(Update, poll_peer_rig_resolutions);
        let refreshed = app
            .world_mut()
            .spawn((
                remembered_peer(1, wearing_one("3jzfcijpj2z2a", "short", false)),
                PeerResolve {
                    outfit_missing: 1,
                    ..Default::default()
                },
                in_flight(),
            ))
            .id();
        answer(
            &mut app,
            1,
            Ok(Some(wearing_one("3jzfcijpj2z2a", "complete", true))),
            Some(refreshed),
        );
        let moved_on = app
            .world_mut()
            .spawn((
                remembered_peer(2, wearing("3jzfcijpj2z2b", "worn now")),
                PeerResolve::default(),
            ))
            .id();
        app.world_mut().spawn(PeerRigResolveTask {
            peer_id: peer_id(2),
            peer_entity: moved_on,
            did: key_did(2),
            avatar_rkey: String::from("3jzfcijpj2z2a"),
            attachment_rkeys: vec![String::from("att-1"), String::from("att-2")],
            notice_seq: 0,
            task: finished((None, crate::pds::avatar::wardrobe::ResolveReport::default())),
        });
        app.update();

        let missing = |peer: Entity| {
            app.world()
                .get::<PeerResolve>(peer)
                .map(|resolve| resolve.outfit_missing)
        };
        assert_eq!(missing(refreshed), Some(0));
        assert_eq!(missing(moved_on), Some(0));
    }

    /// #1489's fourth review: a hop into a room of familiar faces refreshes
    /// them a few at a time - a fetch holds an IO thread while it runs.
    #[test]
    fn refreshes_run_a_few_at_a_time() {
        let mut app = bare_app();
        app.add_systems(Update, spawn_cached_avatar_refreshes);
        for n in 1..=5 {
            app.world_mut().spawn((
                remembered_peer(n, wearing("3jzfcijpj2z2a", "kept")),
                RefreshCachedAvatar::default(),
            ));
        }
        app.update();

        let mut tasks = app.world_mut().query::<&PeerAvatarFetchTask>();
        assert_eq!(
            tasks.iter(app.world()).count(),
            config::network::PEER_AVATAR_REFRESHES_IN_FLIGHT
        );
    }
}
