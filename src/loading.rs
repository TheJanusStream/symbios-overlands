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
    LiveRoomRecord, RoomRecordRecovery, StoredAvatarRecord, StoredInventoryRecord,
    StoredRoomRecord,
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
            crate::config::http::block_on(fut)
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
        commands.insert_resource(LiveRoomRecord(record));
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
            crate::config::http::block_on(fut)
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
            crate::config::http::block_on(fut)
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

// ---------------------------------------------------------------------------
// Ambient audio bake (5th loading-gate task)
// ---------------------------------------------------------------------------

/// Resolved ambient-track handle for the active room. Inserted exactly
/// once during `AppState::Loading` so the loading gate is unblocked
/// even when a room carries no ambient audio (`None` / `Referenced` /
/// `Unknown` / parse-error variants all land here as `AmbientHandle(None)`).
///
/// A `Some(_)` value is the [`Handle<AudioSource>`] the world-builder
/// will hand to a Bevy `AudioPlayer` once the InGame state takes over;
/// `None` is the explicit "no ambient track" signal, distinct from
/// "still baking".
#[derive(Resource, Debug, Clone)]
pub struct AmbientHandle(pub Option<Handle<bevy::audio::AudioSource>>);

/// In-flight ambient-bake task. Carries WAV bytes (mono IEEE float)
/// produced by the audio crate's [`bake_sequence`](bevy_symbios_audio::bake_sequence)
/// / [`bake`](bevy_symbios_audio::bake()) +
/// [`samples_to_wav_bytes`](bevy_symbios_audio::samples_to_wav_bytes)
/// pipeline. The poll system wraps these as [`AudioSource`] and
/// writes [`AmbientHandle`].
#[derive(Component)]
pub(crate) struct AmbientBakeTask(bevy::tasks::Task<Option<Vec<u8>>>);

/// Latch flipped on by [`start_ambient_bake`] once it has either
/// inserted [`AmbientHandle`] directly (no-audio fast path) or kicked
/// off the dispatch — bake task for procedural variants, resolver
/// fetch for [`SovereignAssetReference`](crate::pds::SovereignAssetReference)
/// Referenced variants. Without
/// this guard the Referenced path would re-queue itself every frame
/// during the resolver fetch window.
#[derive(Resource)]
pub(crate) struct AmbientBakeStarted;

/// Bake the room's ambient track into WAV bytes off the main thread.
///
/// Returns `Some(bytes)` for successfully baked procedural variants;
/// `None` for variants that yield no audio under this loading-gate
/// path:
///
/// * [`SovereignAudioConfig::None`](crate::pds::SovereignAudioConfig::None)
///   / [`SovereignAudioConfig::Unknown`](crate::pds::SovereignAudioConfig::Unknown):
///   no ambient track requested.
/// * [`SovereignAudioConfig::Referenced`](crate::pds::SovereignAudioConfig::Referenced): handled by the audio
///   resolver path (filed under #308); this gate inserts `None` so the
///   loading gate progresses, and the resolver patches the handle in
///   later once bytes arrive.
/// * Malformed JSON inside `Patch` / `Sequence`: logged at warn-level
///   by the caller, treated as "no audio" so a corrupt record never
///   blocks room load.
fn bake_ambient_wav_bytes(audio: &crate::pds::SovereignAudioConfig) -> Option<Vec<u8>> {
    use crate::pds::SovereignAudioConfig;

    match audio {
        SovereignAudioConfig::None
        | SovereignAudioConfig::Unknown
        | SovereignAudioConfig::Referenced { .. } => None,
        SovereignAudioConfig::Patch { .. } => {
            // One-shot bake. Default duration of 4.0s matches the
            // audio crate's example envelope; future iterations can
            // pull the duration from a Patch-side wrapper.
            let patch = audio.parse_patch()?;
            let samples = bevy_symbios_audio::bake(&patch, 44_100, 4.0);
            Some(bevy_symbios_audio::samples_to_wav_bytes(&samples, 44_100))
        }
        SovereignAudioConfig::Sequence { .. } => {
            let recipe = audio.parse_sequence()?;
            let sample_rate = recipe.sample_rate;
            let samples = bevy_symbios_audio::bake_sequence(&recipe);
            Some(bevy_symbios_audio::samples_to_wav_bytes(
                &samples,
                sample_rate,
            ))
        }
    }
}

/// Dispatch the ambient bake the frame `LiveRoomRecord` lands. Reads
/// the room's `environment.ambient_audio`, picks the right baker, and
/// spawns a single task entity. Subsequent frames are no-ops because
/// [`AmbientHandle`] either gets inserted directly (no-audio variants)
/// or the [`AmbientBakeTask`] component is in flight.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_ambient_bake(
    mut commands: Commands,
    room_record: Option<Res<LiveRoomRecord>>,
    started: Option<Res<AmbientBakeStarted>>,
    mut audio_cache: ResMut<crate::world_builder::audio_resolver::BlobAudioCache>,
) {
    // Wait until the room record has landed and we haven't already
    // dispatched the ambient pipeline (the latch flips on the frame
    // dispatch happens, so subsequent frames are no-ops).
    let Some(record) = room_record else {
        return;
    };
    if started.is_some() {
        return;
    }

    let audio = record.0.environment.ambient_audio.clone();

    match &audio {
        // No-audio fast path — insert the handle directly so the
        // loading gate unblocks without spinning up a task that would
        // just return None.
        crate::pds::SovereignAudioConfig::None | crate::pds::SovereignAudioConfig::Unknown => {
            commands.insert_resource(AmbientHandle(None));
        }
        // External-asset path — hand the reference to the audio
        // resolver, which fetches the bytes and writes
        // AmbientHandle(Some(_)) on success or AmbientHandle(None)
        // on failure. The loading gate sees the handle either way.
        crate::pds::SovereignAudioConfig::Referenced { source } => {
            crate::world_builder::audio_resolver::request_blob_audio(
                &mut commands,
                &mut audio_cache,
                source,
                crate::world_builder::audio_resolver::AudioReferenceTarget::AmbientHandle,
            );
        }
        // Procedural path — bake on AsyncComputeTaskPool. Works on
        // both native and wasm without the rayon/wasm split.
        crate::pds::SovereignAudioConfig::Patch { .. }
        | crate::pds::SovereignAudioConfig::Sequence { .. } => {
            let pool = bevy::tasks::AsyncComputeTaskPool::get();
            let task = pool.spawn(async move { bake_ambient_wav_bytes(&audio) });
            commands.spawn(AmbientBakeTask(task));
        }
    }
    commands.insert_resource(AmbientBakeStarted);
}

/// Marker for the entity that plays the room's ambient track. One
/// per active room — despawned (along with its `AudioPlayer`) when the
/// room transitions or the player logs out.
#[derive(Component)]
pub struct AmbientPlayer;

/// Clear ambient-bake state on `OnEnter(AppState::Loading)` so a
/// re-entry into Loading (room transition, log-out/log-in cycle) gets
/// a fresh dispatch instead of inheriting the previous room's
/// AmbientHandle / latch.
pub(crate) fn reset_ambient_bake_state(mut commands: Commands) {
    commands.remove_resource::<AmbientHandle>();
    commands.remove_resource::<AmbientBakeStarted>();
}

/// Spawn the ambient-track player once `AppState::InGame` is entered.
/// Reads [`AmbientHandle`] — `None` is a valid "no ambient track"
/// signal and no entity is spawned in that case.
///
/// Looping is requested explicitly via `PlaybackSettings::LOOP`; the
/// seamless tail-crossfade pre-mix baked into the buffer by the audio
/// crate's mixdown means a hard rodio loop still sounds seamless at
/// the seam.
pub(crate) fn spawn_ambient_player(mut commands: Commands, ambient: Option<Res<AmbientHandle>>) {
    let Some(ambient) = ambient else {
        // Loading gate would have inserted AmbientHandle before
        // transitioning; only reachable if someone forces the state
        // out of band. Stay silent rather than panicking.
        return;
    };
    let Some(handle) = ambient.0.clone() else {
        // Explicit "no ambient" — nothing to spawn.
        return;
    };
    commands.spawn((
        bevy::audio::AudioPlayer::new(handle),
        bevy::audio::PlaybackSettings::LOOP,
        AmbientPlayer,
    ));
    info!("Ambient track playing on loop");
}

/// Drain a finished ambient-bake task: wrap the WAV bytes in
/// [`AudioSource`] and insert [`AmbientHandle`].
pub(crate) fn poll_ambient_bake_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AmbientBakeTask)>,
    mut audio_sources: ResMut<Assets<bevy::audio::AudioSource>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        let handle = result.map(|bytes| {
            audio_sources.add(bevy::audio::AudioSource {
                bytes: bytes.into(),
            })
        });
        if handle.is_some() {
            info!("Ambient audio baked");
        }
        commands.insert_resource(AmbientHandle(handle));
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
/// - [`AmbientHandle`] — ambient audio either baked or explicitly absent
///
/// Advancing early leaves the poll systems orphaned (they only run in
/// `Loading`), which would strand a slower PDS round-trip and leave the
/// owner unable to edit what was never fetched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<LiveRoomRecord>>,
    stored_room: Option<Res<StoredRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    stored_avatar: Option<Res<StoredAvatarRecord>>,
    live_inventory: Option<Res<LiveInventoryRecord>>,
    stored_inventory: Option<Res<StoredInventoryRecord>>,
    ambient: Option<Res<AmbientHandle>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some()
        && room_record.is_some()
        && stored_room.is_some()
        && live_avatar.is_some()
        && stored_avatar.is_some()
        && live_inventory.is_some()
        && stored_inventory.is_some()
        && ambient.is_some()
    {
        next_state.set(AppState::InGame);
    }
}

#[cfg(test)]
mod tests {
    //! Pure-function tests for [`bake_ambient_wav_bytes`]. ECS-level
    //! flow (the system order, the AsyncComputeTaskPool dispatch, the
    //! loading-gate transition) is exercised by manual smoke-tests
    //! rather than wired up here — bringing up a full Bevy `App` just
    //! to drive a one-shot loading transition is heavier than the
    //! coverage warrants for an isolated bake helper.
    use super::*;
    use crate::pds::{SovereignAssetReference, SovereignAudioConfig};

    #[test]
    fn none_variant_returns_no_bytes() {
        assert!(bake_ambient_wav_bytes(&SovereignAudioConfig::None).is_none());
    }

    #[test]
    fn referenced_variant_returns_no_bytes() {
        let r = SovereignAudioConfig::Referenced {
            source: SovereignAssetReference::default(),
        };
        assert!(bake_ambient_wav_bytes(&r).is_none());
    }

    #[test]
    fn unknown_variant_returns_no_bytes() {
        assert!(bake_ambient_wav_bytes(&SovereignAudioConfig::Unknown).is_none());
    }

    #[test]
    fn default_patch_variant_bakes_silently() {
        // With the structured mirror (#311), there's no "malformed
        // JSON" path — the wire IS the structured Fp form, so any
        // record that decodes into Patch carries a well-formed
        // SovereignAudioPatch. A default empty graph (a single
        // Silence node) bakes silently — non-zero samples, just at
        // zero amplitude.
        let r = SovereignAudioConfig::Patch {
            patch: crate::pds::audio::SovereignAudioPatch::default(),
        };
        // Bake produces bytes even for silent output (the WAV envelope
        // wraps a buffer of zeros).
        assert!(bake_ambient_wav_bytes(&r).is_some());
    }

    #[test]
    fn default_sequence_variant_bakes_silently() {
        let r = SovereignAudioConfig::Sequence {
            recipe: crate::pds::audio::SovereignSequenceRecipe::default(),
        };
        assert!(bake_ambient_wav_bytes(&r).is_some());
    }

    #[test]
    fn sequence_variant_produces_wav_bytes() {
        // Use the seeded deriver so the recipe is realistic — same
        // wiring the default homeworld produces at runtime.
        let scene = crate::seeded_defaults::SceneCharacter::for_did("did:plc:bake_test");
        let recipe = crate::seeded_defaults::AmbientRecipe::from_scene(&scene, 42).recipe;
        let stash = SovereignAudioConfig::from_sequence(&recipe);
        let bytes = bake_ambient_wav_bytes(&stash).expect("bake produces bytes");
        // RIFF/WAVE header check — the audio crate emits IEEE-float
        // mono WAV; "RIFF" at offset 0, "WAVEfmt " at offset 8.
        assert!(
            bytes.starts_with(b"RIFF"),
            "bytes must start with RIFF header"
        );
        assert!(
            bytes[8..16] == *b"WAVEfmt ",
            "bytes must carry WAVEfmt subchunk"
        );
        // Sanity-check size — 16 beats at 60 BPM = 16 seconds, plus
        // crossfade tail, at 44.1 kHz mono float = at least 16 s ×
        // 44_100 × 4 bytes = 2.8 MiB. WAV header (~44 bytes) is
        // dwarfed by the data chunk.
        assert!(
            bytes.len() > 2_000_000,
            "wav bytes should be at least 2 MB for a 16-beat loop at 60 BPM, got {}",
            bytes.len()
        );
    }
}
