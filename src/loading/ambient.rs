//! Ambient-audio bake: the loading gate's fifth task.
//!
//! The frame the room record lands, [`start_ambient_bake`] reads the
//! room's `environment.ambient_audio`, picks the right pipeline
//! (no-audio fast path / referenced-asset resolver / procedural bake on
//! `AsyncComputeTaskPool`), and publishes the result as
//! [`AmbientHandle`]; [`spawn_ambient_player`] turns that handle into
//! the looping ambient player on `InGame` entry.

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

/// In-flight ambient-bake task. Carries WAV bytes (mono IEEE float)
/// produced by the audio crate's [`bake_sequence`](bevy_symbios_audio::bake_sequence)
/// / [`bake`](bevy_symbios_audio::bake()) +
/// [`samples_to_wav_bytes`](bevy_symbios_audio::samples_to_wav_bytes)
/// pipeline. The poll system wraps these as `AudioSource` and
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
/// `AudioSource` and insert [`AmbientHandle`].
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
