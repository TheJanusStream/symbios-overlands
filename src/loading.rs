//! Loading-phase plumbing: parallel record-fetch state machines (room /
//! avatar / inventory), exponential-backoff retry, and the
//! all-resources-present gate that unblocks `AppState::InGame`.
//!
//! All three fetches run in parallel during [`AppState::Loading`]. Room
//! and avatar fetches retry transient failures with capped exponential
//! backoff (deferred through the frame loop, not by parking
//! `IoTaskPool` workers); inventory is best-effort and falls through to
//! an empty stash on any failure. [`check_loading_complete`] only
//! transitions to `InGame` once every resource the first `InGame` frame
//! depends on is present, so a slow PDS round-trip cannot strand a
//! half-loaded recipe behind the world builder.

use bevy::prelude::*;

use crate::config;
use crate::pds::{self, AvatarRecord, RoomRecord};
use crate::state::{
    AppState, CurrentRoomDid, DiagnosticsLog, LiveAvatarRecord, LiveInventoryRecord,
    RoomRecordRecovery, StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
};
use crate::terrain;

// ---------------------------------------------------------------------------
// Room record loading
// ---------------------------------------------------------------------------

/// In-flight `fetch_room_record` task attached to a throwaway entity so the
/// `Loading` poll system can drain it without a dedicated resource.
///
/// The task result preserves the distinction between *no record* (404) and
/// *couldn't reach the PDS*, so the poll system only falls through to the
/// default homeworld on the former. Falling through on a transient network
/// failure is catastrophic: the owner would silently be staged on the blank
/// default, and a "Publish to PDS" click would overwrite their real
/// room with the default.
#[derive(Component)]
pub(crate) struct RoomRecordTask {
    task: bevy::tasks::Task<Result<Option<RoomRecord>, pds::FetchError>>,
    /// Zero for the initial fetch; incremented on each transient-failure
    /// respawn so `spawn_room_record_fetch` can pick a backoff delay.
    attempt: u32,
}

/// Exponential backoff for transient `fetch_room_record` failures. Without
/// a delay, a DNS error or immediate `ConnRefused` returns so fast that
/// the retry runs in the same or next frame, producing a busy loop that
/// burns a full CPU core and floods the log with warnings. Doubling from
/// 1 s up to a 60 s ceiling yields ~a minute-of-retries over six
/// attempts while still converging quickly when the PDS recovers.
fn record_backoff_secs(attempt: u32) -> u64 {
    if attempt == 0 {
        0
    } else {
        (1u64 << attempt.min(6)).min(60)
    }
}

/// Hard cap on record-fetch retries. The backoff saturates at 60 s after
/// six attempts, so twelve attempts buys roughly ten minutes of real-time
/// retrying against a flaky PDS — past that, persistent failure is
/// overwhelmingly more likely than a transient hiccup. Without this cap,
/// a misbehaving endpoint would spin the IoTaskPool indefinitely; on
/// `wasm32` it would also pile up an unbounded sequence of setTimeout
/// futures waiting in the browser event loop.
const MAX_RECORD_FETCH_ATTEMPTS: u32 = 12;

/// In-flight retry timer for a record fetch. The previous design parked
/// the backoff sleep *inside* the spawned `IoTaskPool` task — but
/// `tokio::time::sleep` awaited inside `block_on(fut)` holds the
/// underlying OS thread idle for the duration of the sleep, because
/// `block_on` dedicates one pool thread per task tree. Several flaky
/// fetches in retry simultaneously would saturate `IoTaskPool` (whose
/// thread count is small) and stall every other I/O job in the engine.
///
/// The fix is to defer the retry on Bevy's frame loop instead: when the
/// poll system decides to retry, it spawns one of these markers; the
/// [`fire_pending_record_retries`] system below watches `Time` and only
/// then dispatches the actual `IoTaskPool` task. The sleeping period
/// occupies a tiny ECS entity rather than a precious worker thread.
#[derive(Component)]
pub(crate) struct PendingRoomRecordRetry {
    did: String,
    attempt: u32,
    fire_at_secs: f64,
}

#[derive(Component)]
pub(crate) struct PendingAvatarRecordRetry {
    did: String,
    attempt: u32,
    fire_at_secs: f64,
}

fn spawn_room_record_fetch(commands: &mut Commands, did: String, attempt: u32) {
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_room_record(&client, &did).await
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
    commands.spawn(RoomRecordTask { task, attempt });
}

/// Kick off the async ATProto `getRecord` fetch for the room the client is
/// visiting. Runs exactly once on entry to `AppState::Loading`; the result is
/// picked up by [`poll_room_record_task`] on subsequent frames.
pub(crate) fn start_room_record_fetch(mut commands: Commands, room_did: Res<CurrentRoomDid>) {
    spawn_room_record_fetch(&mut commands, room_did.0.clone(), 0);
}

/// Drain a finished `RoomRecordTask`, install the resulting `RoomRecord` as a
/// Bevy resource, and synthesise the default recipe if the owner has never
/// published one (a 404 is not an error — it means a blank homeworld).
///
/// A non-404 failure (DNS timeout, 5xx, garbled JSON) retries the fetch
/// instead of substituting the default. This matters because the owner's
/// editor workflow is "load record → edit → publish": if we installed the
/// default on a transient error, a save-and-publish click would silently
/// clobber the owner's real room with the blank default.
pub(crate) fn poll_room_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RoomRecordTask)>,
    room_did: Res<CurrentRoomDid>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let prev_attempt = task.attempt;

        commands.entity(entity).despawn();

        let mut record = match result {
            Ok(Some(r)) => r,
            Ok(None) => {
                // Zero-configuration homeworld: a 404 from the PDS means the
                // owner has not customised their overland yet, so we
                // synthesise the canonical default recipe keyed to their DID.
                info!("No room record on PDS — using default homeworld");
                pds::RoomRecord::default_for_did(&room_did.0)
            }
            Err(pds::FetchError::Decode(msg)) => {
                // A decode failure is *not* transient: the stored record
                // exists but is incompatible with the current schema (e.g.
                // lexicon drift, partially-migrated field). Retrying will
                // never recover — the loading screen would hang forever and
                // spam the diagnostics log. Fall through to the default
                // homeworld so the session progresses, and surface a
                // `RoomRecordRecovery` marker so the world editor can show
                // the owner a "Reset PDS to default" affordance.
                let elapsed = time.elapsed_secs_f64();
                diagnostics.push(
                    elapsed,
                    format!("Stored room record incompatible ({msg}) — falling back to default"),
                );
                warn!(
                    "Stored room record could not be decoded ({}) — using default and entering recovery mode",
                    msg
                );
                commands.insert_resource(RoomRecordRecovery { reason: msg });
                pds::RoomRecord::default_for_did(&room_did.0)
            }
            Err(err) => {
                // Transient failure (DNS timeout, 5xx, DID resolution hiccup):
                // do NOT substitute the default. Log it, re-queue the fetch
                // with an exponential backoff, and keep the Loading state
                // active so the owner cannot accidentally overwrite their
                // room with a blank default on a network blip. Without the
                // backoff, an instantly-failing error (e.g. ConnRefused)
                // would return so fast that the retry fires in the same
                // frame, busy-looping on the IoTaskPool and flooding the
                // diagnostics log.
                let next_attempt = prev_attempt.saturating_add(1);
                let elapsed = time.elapsed_secs_f64();
                if next_attempt > MAX_RECORD_FETCH_ATTEMPTS {
                    // Persistent failure: stop hammering the endpoint and
                    // surface a recovery banner so the owner can reset to
                    // the default without risking a silent clobber.
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Room record fetch failed ({err:?}) — giving up after {MAX_RECORD_FETCH_ATTEMPTS} attempts"
                        ),
                    );
                    warn!(
                        "Room record fetch exhausted {} attempts: {:?} — entering recovery mode",
                        MAX_RECORD_FETCH_ATTEMPTS, err
                    );
                    commands.insert_resource(RoomRecordRecovery {
                        reason: format!("PDS unreachable: {err:?}"),
                    });
                    pds::RoomRecord::default_for_did(&room_did.0)
                } else {
                    let backoff = record_backoff_secs(next_attempt);
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Room record fetch failed ({err:?}) — retrying in {backoff}s (attempt {next_attempt})"
                        ),
                    );
                    warn!(
                        "Room record fetch failed: {:?} — retrying in {}s (attempt {})",
                        err, backoff, next_attempt
                    );
                    // Defer the retry through the frame-loop timer so the
                    // backoff doesn't park an `IoTaskPool` worker thread.
                    commands.spawn(PendingRoomRecordRetry {
                        did: room_did.0.clone(),
                        attempt: next_attempt,
                        fire_at_secs: elapsed + backoff as f64,
                    });
                    continue;
                }
            }
        };
        record.sanitize();
        info!(
            "Room record loaded: {} generators, {} placements",
            record.generators.len(),
            record.placements.len()
        );
        // Install both the live resource (mutated by the world editor) and
        // the stored snapshot (consulted by "Load from PDS" to undo
        // uncommitted edits). The two start identical — any divergence is
        // authored by the owner.
        commands.insert_resource(StoredRoomRecord(record.clone()));
        commands.insert_resource(record);
    }
}

// ---------------------------------------------------------------------------
// Avatar record loading (in parallel with the room fetch — both must
// complete before entering InGame so the local player has a definitive
// starting pose *and* recipe).
// ---------------------------------------------------------------------------

/// In-flight `fetch_avatar_record` task for the *local* player's own
/// avatar. Mirrors [`RoomRecordTask`]: a component attached to a throwaway
/// entity drained by [`poll_avatar_record_task`].
#[derive(Component)]
pub(crate) struct AvatarRecordTask {
    did: String,
    task: bevy::tasks::Task<Result<Option<AvatarRecord>, pds::FetchError>>,
    attempt: u32,
}

fn spawn_avatar_record_fetch(commands: &mut Commands, did: String, attempt: u32) {
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
    commands.spawn(AvatarRecordTask { did, task, attempt });
}

/// Fire any retry markers whose backoff has elapsed. Runs on Bevy's main
/// frame loop, so an idle 60 s exponential-backoff window costs only the
/// `Time` resource read per frame instead of a permanently-sleeping
/// worker thread. See [`PendingRoomRecordRetry`] for the rationale.
pub(crate) fn fire_pending_record_retries(
    mut commands: Commands,
    room_pending: Query<(Entity, &PendingRoomRecordRetry)>,
    avatar_pending: Query<(Entity, &PendingAvatarRecordRetry)>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, pending) in room_pending.iter() {
        if now >= pending.fire_at_secs {
            let did = pending.did.clone();
            let attempt = pending.attempt;
            commands.entity(entity).despawn();
            spawn_room_record_fetch(&mut commands, did, attempt);
        }
    }
    for (entity, pending) in avatar_pending.iter() {
        if now >= pending.fire_at_secs {
            let did = pending.did.clone();
            let attempt = pending.attempt;
            commands.entity(entity).despawn();
            spawn_avatar_record_fetch(&mut commands, did, attempt);
        }
    }
}

/// Kick off the async `getRecord` fetch for the local player's avatar.
/// Silently no-ops if the user never logged in (session absent), in which
/// case [`check_loading_complete`] will also refuse to advance — we never
/// reach Loading without a session in normal flow.
pub(crate) fn start_avatar_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
) {
    let Some(sess) = session else {
        warn!("start_avatar_record_fetch: no session — local avatar will not load");
        return;
    };
    spawn_avatar_record_fetch(&mut commands, sess.did.clone(), 0);
}

/// Drain a finished `AvatarRecordTask`, install both the live and stored
/// resources, and synthesise a DID-derived default on a 404. Transient
/// failures retry with exponential backoff so a network blip cannot
/// silently clobber the user's published avatar with the default.
pub(crate) fn poll_avatar_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AvatarRecordTask)>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let prev_attempt = task.attempt;
        let did = task.did.clone();
        commands.entity(entity).despawn();

        let mut record = match result {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!("No avatar record on PDS — using DID-hashed default");
                AvatarRecord::default_for_did(&did)
            }
            Err(pds::FetchError::Decode(msg)) => {
                // Decode failure is permanent, not transient: the stored
                // record exists but its schema is incompatible with the
                // current `AvatarRecord` (lexicon drift, partially-migrated
                // field, bincode/JSON mismatch). Retrying will never
                // recover, so fall straight through to the DID-hashed
                // default — otherwise Loading hangs forever and the
                // diagnostics log fills with identical decode warnings.
                // The owner can re-publish from the avatar editor to
                // overwrite the incompatible record with the new schema.
                let elapsed = time.elapsed_secs_f64();
                diagnostics.push(
                    elapsed,
                    format!("Stored avatar record incompatible ({msg}) — falling back to default"),
                );
                warn!(
                    "Stored avatar record could not be decoded ({}) — using DID-hashed default",
                    msg
                );
                AvatarRecord::default_for_did(&did)
            }
            Err(err) => {
                // Transient failure — retry with backoff rather than
                // installing the default. Installing the default on a
                // network error would let a subsequent "Publish" click
                // silently clobber the user's real avatar. After
                // `MAX_RECORD_FETCH_ATTEMPTS` we stop retrying so a dead
                // PDS can't drive a permanent busy-loop against the user's
                // CPU or the remote endpoint.
                let next_attempt = prev_attempt.saturating_add(1);
                let elapsed = time.elapsed_secs_f64();
                if next_attempt > MAX_RECORD_FETCH_ATTEMPTS {
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Avatar record fetch failed ({err:?}) — giving up after {MAX_RECORD_FETCH_ATTEMPTS} attempts, using default"
                        ),
                    );
                    warn!(
                        "Avatar record fetch exhausted {} attempts: {:?} — falling back to default",
                        MAX_RECORD_FETCH_ATTEMPTS, err
                    );
                    AvatarRecord::default_for_did(&did)
                } else {
                    let backoff = record_backoff_secs(next_attempt);
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Avatar record fetch failed ({err:?}) — retrying in {backoff}s (attempt {next_attempt})"
                        ),
                    );
                    warn!(
                        "Avatar record fetch failed: {:?} — retrying in {}s (attempt {})",
                        err, backoff, next_attempt
                    );
                    // Defer the retry through the frame-loop timer so the
                    // backoff doesn't park an `IoTaskPool` worker thread.
                    commands.spawn(PendingAvatarRecordRetry {
                        did,
                        attempt: next_attempt,
                        fire_at_secs: elapsed + backoff as f64,
                    });
                    continue;
                }
            }
        };
        record.sanitize();
        commands.insert_resource(LiveAvatarRecord(record.clone()));
        commands.insert_resource(StoredAvatarRecord(record));
    }
}

// ---------------------------------------------------------------------------
// Inventory record loading. Unlike room + avatar, this is best-effort:
// transient failures fall through to an empty stash rather than retrying,
// because nothing gameplay-critical reads the inventory — the owner can
// re-open the Inventory window after login if they want to retry by
// publishing a saved item.
// ---------------------------------------------------------------------------

#[derive(Component)]
pub(crate) struct InventoryRecordTask(
    bevy::tasks::Task<Result<Option<crate::pds::InventoryRecord>, crate::pds::FetchError>>,
);

pub(crate) fn start_inventory_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
) {
    let Some(sess) = session else {
        return;
    };
    let pool = bevy::tasks::IoTaskPool::get();
    let did = sess.did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_inventory_record(&client, &did).await
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
    commands.spawn(InventoryRecordTask(task));
}

pub(crate) fn poll_inventory_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut InventoryRecordTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let mut record = match result {
            Ok(Some(r)) => r,
            _ => crate::pds::InventoryRecord::default(),
        };
        record.sanitize();
        commands.insert_resource(LiveInventoryRecord(record.clone()));
        commands.insert_resource(StoredInventoryRecord(record));
    }
}

/// Transition out of `Loading` only once *every* resource the first
/// `InGame` frame relies on is present:
///
/// - [`terrain::FinishedHeightMap`] — collider is solid
/// - [`RoomRecord`] — live room recipe (world builder consumes this)
/// - [`StoredRoomRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveAvatarRecord`] — live avatar driving `spawn_local_player`
/// - [`StoredAvatarRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveInventoryRecord`] / [`StoredInventoryRecord`] — owner's Generator stash
///
/// Advancing early leaves the poll systems orphaned (they only run in
/// `Loading`), which would strand a slower PDS round-trip and leave the
/// owner unable to edit what was never fetched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<RoomRecord>>,
    stored_room: Option<Res<StoredRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    stored_avatar: Option<Res<StoredAvatarRecord>>,
    live_inventory: Option<Res<LiveInventoryRecord>>,
    stored_inventory: Option<Res<StoredInventoryRecord>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some()
        && room_record.is_some()
        && stored_room.is_some()
        && live_avatar.is_some()
        && stored_avatar.is_some()
        && live_inventory.is_some()
        && stored_inventory.is_some()
    {
        next_state.set(AppState::InGame);
    }
}
