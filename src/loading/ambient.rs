//! Ambient-audio bake: the loading gate's fifth task.
//!
//! The frame the room record lands, [`start_ambient_bake`] reads the
//! room's `environment.ambient_audio`, picks the right pipeline
//! (no-audio fast path / referenced-asset resolver / procedural bake on
//! `AsyncComputeTaskPool`), and publishes the result as
//! [`AmbientHandle`]; [`swap_ambient_player_to_handle`] turns that handle
//! into the looping ambient player in-game, once the [`AmbientSettle`]
//! quiet window has drained so the sink isn't born mid-stall.

use bevy::prelude::*;

use crate::state::LiveRoomRecord;

/// Resolved ambient-track handle for the active room. Inserted exactly
/// once during `AppState::Loading` so the loading gate is unblocked
/// even when a room carries no ambient audio (`None` / `Referenced` /
/// `Unknown` / parse-error variants all land here as `AmbientHandle(None)`).
///
/// A `Some(_)` value is the [`Handle<AudioSource>`](bevy::audio::AudioSource)
/// the world-builder will hand to a Bevy `AudioPlayer` once the InGame
/// state takes over; `None` is the explicit "no ambient track" signal,
/// distinct from "still baking".
#[derive(Resource, Debug, Clone)]
pub struct AmbientHandle(pub Option<Handle<bevy::audio::AudioSource>>);

/// In-flight ambient-bake task. Carries WAV bytes (mono 16-bit PCM)
/// produced by the audio crate's [`bake_sequence`](bevy_symbios_audio::bake_sequence)
/// / [`bake`](bevy_symbios_audio::bake()) +
/// `samples_to_wav_bytes_pcm16`
/// pipeline. The poll system wraps these as `AudioSource` and
/// writes [`AmbientHandle`].
#[derive(Component)]
pub(crate) struct AmbientBakeTask(
    bevy::tasks::Task<crate::offload::GenResult>,
    /// Session-relative seconds at dispatch, for the E-4 completion latency.
    f64,
);

/// In-flight *in-game* ambient re-bake task — the editor counterpart of
/// [`AmbientBakeTask`]. Kept as a distinct component so the loading-gate
/// poll and the in-game poll never drain each other's tasks (the two run
/// in different `AppState`s and own separate pipelines).
#[derive(Component)]
pub(crate) struct AmbientRebakeTask(
    bevy::tasks::Task<crate::offload::GenResult>,
    /// Session-relative seconds at dispatch, for the E-4 completion latency.
    f64,
);

/// Wrap freshly-baked WAV bytes into an [`AudioSource`]
/// handle, or `None` for the no-audio variants. Shared by the loading-gate
/// poll and the in-game re-bake poll.
fn wrap_baked_handle(
    result: Option<Vec<u8>>,
    audio_sources: &mut Assets<bevy::audio::AudioSource>,
) -> Option<Handle<bevy::audio::AudioSource>> {
    result.map(|bytes| {
        audio_sources.add(bevy::audio::AudioSource {
            bytes: bytes.into(),
        })
    })
}

/// Latch flipped on by [`start_ambient_bake`] once it has either
/// inserted [`AmbientHandle`] directly (no-audio fast path) or kicked
/// off the dispatch — bake task for procedural variants, resolver
/// fetch for [`SovereignAssetReference`](crate::pds::SovereignAssetReference)
/// Referenced variants. Without
/// this guard the Referenced path would re-queue itself every frame
/// during the resolver fetch window.
#[derive(Resource)]
pub(crate) struct AmbientBakeStarted;

/// The ambient config currently realised as the playing [`AmbientPlayer`].
///
/// The loading gate bakes the ambient bed exactly once; this resource lets
/// the *in-game* re-bake ([`rebake_ambient_on_record_change`]) tell apart a
/// record edit that touched the ambient bed (a re-roll, a "Reset to
/// default", a direct audio edit — restart the loop) from one that didn't
/// (a terrain or colour tweak — leave the music alone). Initialised by
/// [`start_ambient_bake`] and cleared by [`reset_ambient_bake_state`].
#[derive(Resource, Default)]
pub(crate) struct LiveAmbientConfig(Option<crate::pds::SovereignAudioConfig>);

/// The audio handle currently fed to the live [`AmbientPlayer`].
///
/// [`swap_ambient_player_to_handle`] respawns the looping player only when
/// [`AmbientHandle`] differs from this — so a re-bake landing a new handle
/// swaps the loop, while an unchanged handle is a no-op (no per-frame
/// churn, no reliance on a fragile change-tick across schedules).
#[derive(Resource, Default)]
pub(crate) struct PlayingAmbient(Option<Handle<bevy::audio::AudioSource>>);

impl PlayingAmbient {
    /// Forget the currently-playing handle. Called from the logout
    /// teardown after the [`AmbientPlayer`] entity is despawned, so a
    /// later login doesn't think a (now-gone) loop is still playing and
    /// releases the baked `AudioSource` bytes promptly.
    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }
}

/// Bake the room's ambient track into WAV bytes off the main thread.
///
/// Returns `Some(job)` for procedural variants (the heavy synth is then run
/// off-thread via [`crate::offload`]); `None` for variants that yield no audio
/// under this loading-gate path:
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
fn ambient_bake_job(audio: &crate::pds::SovereignAudioConfig) -> Option<gen_jobs::AudioBakeJob> {
    use crate::pds::SovereignAudioConfig;

    match audio {
        SovereignAudioConfig::None
        | SovereignAudioConfig::Unknown
        | SovereignAudioConfig::Referenced { .. } => None,
        // One-shot bake. Default duration of 4.0s matches the audio crate's
        // example envelope; future iterations can pull it from a Patch wrapper.
        SovereignAudioConfig::Patch { .. } => Some(gen_jobs::AudioBakeJob::Patch {
            patch: audio.parse_patch()?,
            // 22.05 kHz halves the baked + decoded buffers; ambient content is
            // within the 11 kHz Nyquist (#568, matches the Sequence default).
            sample_rate: 22_050,
            duration_secs: 4.0,
        }),
        SovereignAudioConfig::Sequence { .. } => Some(gen_jobs::AudioBakeJob::Sequence {
            recipe: audio.parse_sequence()?,
        }),
    }
}

/// Test helper: run the full procedural bake path (parse → `gen-jobs` synth →
/// WAV) synchronously — what the offloaded task produces, minus the dispatch.
#[cfg(test)]
fn bake_ambient_wav_bytes(audio: &crate::pds::SovereignAudioConfig) -> Option<Vec<u8>> {
    let job = ambient_bake_job(audio)?;
    match gen_jobs::GenJob::AudioBake(job).run() {
        gen_jobs::GenResult::Audio(bytes) => Some(bytes),
        _ => None,
    }
}

/// Dispatch the ambient bake the frame `LiveRoomRecord` lands. Reads
/// the room's `environment.ambient_audio`, picks the right baker, and
/// spawns a single task entity. Subsequent frames are no-ops because
/// [`AmbientHandle`] either gets inserted directly (no-audio variants)
/// or the [`AmbientBakeTask`] component is in flight.
pub(crate) fn start_ambient_bake(
    mut commands: Commands,
    room_record: Option<Res<LiveRoomRecord>>,
    started: Option<Res<AmbientBakeStarted>>,
    mut audio_cache: ResMut<crate::world_builder::audio_resolver::BlobAudioCache>,
    mut live_cfg: ResMut<LiveAmbientConfig>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
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
    // Anchor the in-game re-bake against the config the loading gate is
    // baking now, so the first real edit (not the record landing) is what
    // triggers a live re-bake.
    live_cfg.0 = Some(audio.clone());

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
        // Procedural path — synth off the render frame via the offload seam
        // (native: AsyncComputeTaskPool; wasm: Web Worker).
        crate::pds::SovereignAudioConfig::Patch { .. }
        | crate::pds::SovereignAudioConfig::Sequence { .. } => match ambient_bake_job(&audio) {
            Some(job) => {
                // Start marker for the B-2 timeline + the AmbientBakeStall replay
                // rule's start→end pairing. (The `EventPayload` variant is
                // distinct from the same-named `AmbientBakeStarted` marker
                // resource inserted below.)
                let variant = if matches!(audio, crate::pds::SovereignAudioConfig::Patch { .. }) {
                    "patch"
                } else {
                    "sequence"
                };
                session_log.info(
                    time.elapsed_secs_f64(),
                    crate::diagnostics::event::EventPayload::AmbientBakeStarted {
                        variant: variant.to_string(),
                    },
                );
                commands.spawn(AmbientBakeTask(
                    crate::offload::offload(crate::offload::GenJob::AudioBake(job)),
                    time.elapsed_secs_f64(),
                ));
            }
            // Malformed Patch/Sequence JSON → treat as "no audio" so a corrupt
            // record never blocks room load.
            None => {
                session_log.warn(
                    time.elapsed_secs_f64(),
                    crate::diagnostics::event::EventPayload::AmbientBakeFallback {
                        reason: "malformed Patch/Sequence config".to_string(),
                    },
                );
                commands.insert_resource(AmbientHandle(None));
            }
        },
    }
    commands.insert_resource(AmbientBakeStarted);
}

/// Marker for the entity that plays the room's ambient track. One
/// per active room — despawned (along with its `AudioPlayer`) when the
/// room transitions or the player logs out.
#[derive(Component)]
pub struct AmbientPlayer;

/// Looping playback settings for the ambient bed, born muted when the
/// master mute is engaged. Spawning pre-muted (rather than relying solely
/// on the per-frame reconcile in [`crate::audio_mute`]) means launching
/// muted never leaks even a one-frame blip of the loop's attack.
fn ambient_playback_settings(muted: bool) -> bevy::audio::PlaybackSettings {
    bevy::audio::PlaybackSettings {
        muted,
        ..bevy::audio::PlaybackSettings::LOOP
    }
}

/// Clear ambient-bake state on `OnEnter(AppState::Loading)` so a
/// re-entry into Loading (room transition, log-out/log-in cycle) gets
/// a fresh dispatch instead of inheriting the previous room's
/// AmbientHandle / latch.
pub(crate) fn reset_ambient_bake_state(
    mut commands: Commands,
    mut live_cfg: ResMut<LiveAmbientConfig>,
    mut playing: ResMut<PlayingAmbient>,
    mut pending: ResMut<AmbientRebakePending>,
) {
    commands.remove_resource::<AmbientHandle>();
    commands.remove_resource::<AmbientBakeStarted>();
    // Forget the previous room's ambient bed and player handle so the next
    // room bakes fresh and the player respawns from its new handle. Also drop
    // any debounced-but-undispatched config so it can't bake into the new room.
    live_cfg.0 = None;
    playing.0 = None;
    pending.0 = None;
}

/// Quiet window the ambient (re)start waits out before it spawns or
/// swaps the looping sink.
///
/// Starting a rodio sink during a heavy main-thread / CPU stall starves
/// the audio callback at the very moment playback begins, which is heard
/// as a choppy onset. Three triggers all collide playback-start with a
/// stall: the first-render GPU pipeline specialization when the world
/// first appears, the terrain + world recompile *plus* the fresh async
/// bake on a re-roll, and general load spikes. Holding the (re)start
/// until things have been quiet for this long pushes the sink's birth
/// past those stalls. Long enough to clear a typical recompile, short
/// enough not to read as a missing-audio bug.
const AMBIENT_SETTLE_SECS: f32 = 0.4;

/// Countdown gating the ambient (re)start. Armed to [`AMBIENT_SETTLE_SECS`]
/// on `InGame` entry and re-armed on every `LiveRoomRecord` change (a
/// recompile/bake burst); [`swap_ambient_player_to_handle`] only acts once
/// it drains to zero. See [`AMBIENT_SETTLE_SECS`] for the why.
#[derive(Resource)]
pub(crate) struct AmbientSettle {
    remaining: f32,
}

impl Default for AmbientSettle {
    fn default() -> Self {
        Self {
            remaining: AMBIENT_SETTLE_SECS,
        }
    }
}

/// Arm the settle countdown on `InGame` entry so the first ambient start
/// waits out the first-render pipeline stall instead of choking on it.
pub(crate) fn arm_ambient_settle(mut settle: ResMut<AmbientSettle>) {
    settle.remaining = AMBIENT_SETTLE_SECS;
}

/// Drain the settle countdown each frame, re-arming it whenever
/// `LiveRoomRecord` changes — every record edit kicks off a recompile
/// (and possibly an ambient re-bake), so the timer only reaches zero once
/// the owner has paused and the heavy work has drained.
pub(crate) fn tick_ambient_settle(
    time: Res<Time>,
    room_record: Option<Res<LiveRoomRecord>>,
    mut settle: ResMut<AmbientSettle>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    if room_record.is_some_and(|r| r.is_changed()) {
        settle.remaining = AMBIENT_SETTLE_SECS;
    } else {
        let was = settle.remaining;
        settle.remaining = (settle.remaining - time.delta_secs()).max(0.0);
        // Edge-detect the drain-to-zero (#635c): the timer then sits at 0 for
        // the rest of the session, so this fires exactly once — the moment the
        // ambient bed is cleared to (re)start after a recompile/bake burst.
        if was > 0.0 && settle.remaining == 0.0 {
            let now = time.elapsed_secs_f64();
            session_log.info(
                now,
                crate::diagnostics::event::EventPayload::AmbientSettleCompleted {
                    settled_at_secs: now,
                },
            );
        }
    }
}

/// Re-bake the ambient bed when the live room record's `ambient_audio`
/// changes in-game — the editor counterpart of the loading-gate bake.
///
/// Newest ambient-bed config awaiting a (debounced) re-bake dispatch.
///
/// A slider drag mutates `ambient_audio` many times a second; baking on each
/// change spawns a worker and a multi-MiB transient per frame (and orphans the
/// previous worker). [`rebake_ambient_on_record_change`] stashes the latest
/// config here and dispatches once — after the edit settles — so a drag bakes
/// a single time.
#[derive(Resource, Default)]
pub(crate) struct AmbientRebakePending(Option<crate::pds::SovereignAudioConfig>);

/// Mirrors [`start_ambient_bake`]'s pipeline split (None/Referenced/
/// procedural) but is driven by `LiveRoomRecord`'s change tick instead of
/// the one-shot loading gate, so a manual re-roll, a "Reset to default",
/// or a direct audio edit all restart the looping bed. Edits that leave
/// `ambient_audio` untouched (terrain, colours, scatters) compare equal
/// against [`LiveAmbientConfig`] and are skipped, so the music doesn't
/// stutter on every slider drag. The resulting handle lands in
/// [`AmbientHandle`] (directly, via the resolver, or via the baked task)
/// and [`swap_ambient_player_to_handle`] swaps the player.
///
/// The dispatch is **debounced** through [`AmbientRebakePending`] +
/// [`AmbientSettle`]: a change is stashed immediately, but the bake fires only
/// once the same quiet window the player start waits out has drained, so a
/// slider drag bakes once instead of per frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebake_ambient_on_record_change(
    mut commands: Commands,
    room_record: Option<Res<LiveRoomRecord>>,
    mut live_cfg: ResMut<LiveAmbientConfig>,
    mut pending: ResMut<AmbientRebakePending>,
    settle: Res<AmbientSettle>,
    mut audio_cache: ResMut<crate::world_builder::audio_resolver::BlobAudioCache>,
    in_flight: Query<Entity, With<AmbientRebakeTask>>,
    time: Res<Time>,
) {
    let Some(record) = room_record else {
        return;
    };

    // Capture an ambient-bed change into `pending`; the dispatch is deferred to
    // the settle gate below. Edits that don't touch the bed are skipped.
    if record.is_changed() {
        let audio = record.0.environment.ambient_audio.clone();
        if live_cfg.0.as_ref() != Some(&audio) {
            live_cfg.0 = Some(audio.clone());
            pending.0 = Some(audio);
        }
    }

    // Wait out the same quiet window `swap_ambient_player_to_handle` uses, so a
    // burst of edits collapses to a single bake once the owner pauses.
    if settle.remaining > 0.0 {
        return;
    }
    let Some(audio) = pending.0.take() else {
        return;
    };

    // Cancel any still-running re-bake so the settled dispatch wins
    // deterministically (a previous bed's bake may still be in flight).
    for entity in &in_flight {
        commands.entity(entity).despawn();
    }

    match &audio {
        crate::pds::SovereignAudioConfig::None | crate::pds::SovereignAudioConfig::Unknown => {
            // Silence: publish the absence; the swap system despawns the
            // player.
            commands.insert_resource(AmbientHandle(None));
        }
        crate::pds::SovereignAudioConfig::Referenced { source } => {
            crate::world_builder::audio_resolver::request_blob_audio(
                &mut commands,
                &mut audio_cache,
                source,
                crate::world_builder::audio_resolver::AudioReferenceTarget::AmbientHandle,
            );
        }
        crate::pds::SovereignAudioConfig::Patch { .. }
        | crate::pds::SovereignAudioConfig::Sequence { .. } => match ambient_bake_job(&audio) {
            Some(job) => {
                commands.spawn(AmbientRebakeTask(
                    crate::offload::offload(crate::offload::GenJob::AudioBake(job)),
                    time.elapsed_secs_f64(),
                ));
            }
            None => {
                commands.insert_resource(AmbientHandle(None));
            }
        },
    }
}

/// Bring the looping [`AmbientPlayer`] into agreement with
/// [`AmbientHandle`] — the single in-game authority over the player
/// entity, covering **both** the first spawn on `InGame` entry and every
/// later swap (re-roll, Reset, room edit, resolver fetch).
///
/// It despawns the old loop and spawns the new one (or none, for silence)
/// only when the desired handle differs from [`PlayingAmbient`]. Both the
/// resolver (Referenced) and the baked-task poll feed `AmbientHandle`, so
/// this covers every variant uniformly. Holding the old loop until the new
/// handle lands means a procedural re-bake doesn't leave an audible gap.
///
/// **Settle gate.** The (re)start is deferred until [`AmbientSettle`] has
/// drained, so the sink is never born in the middle of a first-render
/// pipeline stall or a recompile/bake burst (see [`AMBIENT_SETTLE_SECS`]).
/// The old loop keeps playing in the meantime, so a re-roll is heard as
/// "old bed continues, then clean swap" rather than a choppy onset.
pub(crate) fn swap_ambient_player_to_handle(
    mut commands: Commands,
    ambient: Option<Res<AmbientHandle>>,
    mut playing: ResMut<PlayingAmbient>,
    players: Query<Entity, With<AmbientPlayer>>,
    audio_muted: Res<crate::audio_mute::AudioMuted>,
    settle: Res<AmbientSettle>,
) {
    let Some(ambient) = ambient else {
        return;
    };
    if ambient.0 == playing.0 {
        return;
    }
    // Wait out the post-change / post-entry quiet window so the sink isn't
    // born during a stall. The old loop (if any) keeps playing until then.
    if settle.remaining > 0.0 {
        return;
    }
    // Desired ambient differs from what's looping — swap atomically.
    for entity in &players {
        commands.entity(entity).despawn();
    }
    match ambient.0.clone() {
        Some(handle) => {
            commands.spawn((
                bevy::audio::AudioPlayer::new(handle.clone()),
                ambient_playback_settings(audio_muted.0),
                AmbientPlayer,
            ));
            playing.0 = Some(handle);
            info!("Ambient track re-baked — loop swapped");
        }
        None => {
            playing.0 = None;
            info!("Ambient bed cleared — loop stopped");
        }
    }
}

/// Drain a finished ambient-bake task: wrap the WAV bytes in
/// `AudioSource` and insert [`AmbientHandle`].
pub(crate) fn poll_ambient_bake_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AmbientBakeTask)>,
    mut audio_sources: ResMut<Assets<bevy::audio::AudioSource>>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        let now = time.elapsed_secs_f64();
        let spawned_at = task.1;
        commands.entity(entity).despawn();

        let wav = match result {
            // Success only: record the bake latency (E-4) + a typed completion
            // (B-2 timeline / ambient stage distro + the AmbientBakeStall replay
            // rule's end marker).
            crate::offload::GenResult::Audio(bytes) => {
                crate::diagnostics::samplers::ambient_bake_latency_secs(
                    &mut metrics,
                    now - spawned_at,
                );
                session_log.info(
                    now,
                    crate::diagnostics::event::EventPayload::AmbientBakeCompleted {
                        bytes: bytes.len() as u64,
                        duration_secs: now - spawned_at,
                    },
                );
                Some(bytes)
            }
            // A non-audio result means the bake job failed to produce audio —
            // count it as an offload error (E-4) and log the fallback; the None
            // falls back to silence.
            _ => {
                crate::diagnostics::samplers::offload_job_error(&mut metrics, now);
                session_log.warn(
                    now,
                    crate::diagnostics::event::EventPayload::AmbientBakeFallback {
                        reason: "bake produced no audio".to_string(),
                    },
                );
                None
            }
        };
        let handle = wrap_baked_handle(wav, &mut audio_sources);
        if handle.is_some() {
            info!("Ambient audio baked");
        }
        commands.insert_resource(AmbientHandle(handle));
    }
}

/// Drain a finished in-game re-bake ([`AmbientRebakeTask`]) and publish the
/// handle into [`AmbientHandle`]; [`swap_ambient_player_to_handle`] then
/// swaps the looping player. The loading-gate counterpart is
/// [`poll_ambient_bake_task`].
pub(crate) fn poll_ambient_rebake_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AmbientRebakeTask)>,
    mut audio_sources: ResMut<Assets<bevy::audio::AudioSource>>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        let now = time.elapsed_secs_f64();
        let spawned_at = task.1;
        commands.entity(entity).despawn();
        let wav = match result {
            // Success only: record the bake latency (E-4).
            crate::offload::GenResult::Audio(bytes) => {
                crate::diagnostics::samplers::ambient_bake_latency_secs(
                    &mut metrics,
                    now - spawned_at,
                );
                // Surface the in-game re-bake in the timeline (#627) — matches
                // the loading-gate bake's `poll_ambient_bake_task`. Only the
                // Completed side is emitted (not Started): a superseded re-bake
                // is despawned by the dispatcher, so a paired Started could
                // read as a false ambient-bake stall.
                session_log.info(
                    now,
                    crate::diagnostics::event::EventPayload::AmbientBakeCompleted {
                        bytes: bytes.len() as u64,
                        duration_secs: now - spawned_at,
                    },
                );
                Some(bytes)
            }
            _ => {
                crate::diagnostics::samplers::offload_job_error(&mut metrics, now);
                None
            }
        };
        commands.insert_resource(AmbientHandle(wrap_baked_handle(wav, &mut audio_sources)));
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
        // RIFF/WAVE header check — the offload path emits 16-bit PCM
        // mono WAV; "RIFF" at offset 0, "WAVEfmt " at offset 8.
        assert!(
            bytes.starts_with(b"RIFF"),
            "bytes must start with RIFF header"
        );
        assert!(
            bytes[8..16] == *b"WAVEfmt ",
            "bytes must carry WAVEfmt subchunk"
        );
        // Sanity-check size — WARMUP_BEATS + LOOP_BEATS = 34 beats at 60 BPM =
        // 34 seconds, plus crossfade tail, at 22.05 kHz mono 16-bit PCM ≈ 34 s ×
        // 22_050 × 2 bytes ≈ 1.5 MiB. WAV header (~44 bytes) is dwarfed by the
        // data chunk.
        assert!(
            bytes.len() > 1_000_000,
            "wav bytes should be at least 1 MB for the seeded loop at 60 BPM, got {}",
            bytes.len()
        );
    }
}
