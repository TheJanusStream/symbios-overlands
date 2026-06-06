//! Per-construct spatial-audio bake-and-attach.
//!
//! Mirrors the [`crate::loading`] ambient-bake pipeline but at the
//! per-entity granularity that constructs need: when a
//! [`Generator`](crate::pds::Generator)
//! carries a non-`None` [`SovereignAudioConfig`] on its `audio` field,
//! we dispatch a background bake on
//! [`bevy::tasks::AsyncComputeTaskPool`] and, on completion, attach a
//! looping spatial [`AudioPlayer`] to the construct's spawned entity.
//! Bevy's built-in spatial-audio attenuation (no HRTF, no Doppler —
//! per Pascal's "single line for now" scope on issue #301) does the
//! positional panning at runtime.
//!
//! # Listener
//!
//! [`SpatialListener`] is already attached to the active camera in
//! [`crate::camera`], so no listener-side wiring is needed here.
//!
//! # Pipeline shape
//!
//! ```text
//! spawn_generator()                       Update
//!     │                                     │
//!     └── dispatch_construct_audio()  ──>  poll_spatial_audio_tasks()
//!         spawns                            drains finished, inserts
//!         SpatialAudioBakeTask{target}      AudioPlayer + Spatial
//!                                           on `target` entity
//! ```

use std::collections::BTreeMap;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};

use crate::pds::SovereignAudioConfig;

/// How the spatial-audio bake should be attached once it completes.
#[derive(Clone, Copy, Debug)]
pub enum BakeAttachmentMode {
    /// Construct emitter — looping, sticky on the target entity.
    LoopingConstruct,
    /// One-shot impact / footstep — `PlaybackMode::Despawn` so the
    /// carrier entity GCs itself when the sound ends. `volume` is the
    /// linear gain in `[0, 1]`.
    OneShot { volume: f32 },
}

/// In-flight per-entity audio bake. `target` is the construct entity
/// the resulting [`AudioPlayer`] should be inserted onto.
#[derive(Component)]
pub struct SpatialAudioBakeTask {
    pub target: Entity,
    pub task: Task<Option<(Vec<u8>, u32)>>,
    pub mode: BakeAttachmentMode,
}

/// Dispatch a background bake for the given construct's audio config.
/// No-op when the variant carries no procedural data
/// ([`None`](crate::pds::SovereignAudioConfig::None) /
/// [`Unknown`](crate::pds::SovereignAudioConfig::Unknown) /
/// [`Referenced`](crate::pds::SovereignAudioConfig::Referenced) —
/// Referenced will eventually flow through the audio resolver, #308).
///
/// The bake runs off the main thread on `AsyncComputeTaskPool`; the
/// poll system below attaches the resulting `AudioPlayer` to `target`
/// once the bytes are ready.
pub fn dispatch_construct_audio(
    commands: &mut Commands,
    audio_cache: &mut super::audio_resolver::BlobAudioCache,
    target: Entity,
    audio: &SovereignAudioConfig,
) {
    match audio {
        // Silent / forward-compat — nothing to dispatch.
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        // External asset — hand the reference to the audio resolver,
        // which fetches and attaches the spatial-looping AudioPlayer
        // to `target` once bytes arrive. The settings shape matches
        // what poll_spatial_audio_tasks would apply for the
        // LoopingConstruct mode (spatial=true + LOOP).
        SovereignAudioConfig::Referenced { source } => {
            super::audio_resolver::request_blob_audio(
                commands,
                audio_cache,
                source,
                super::audio_resolver::AudioReferenceTarget::AttachToEntity {
                    entity: target,
                    settings: PlaybackSettings {
                        spatial: true,
                        ..PlaybackSettings::LOOP
                    },
                },
            );
        }
        // Procedural — bake on AsyncComputeTaskPool. Clone before
        // moving into the task so we don't borrow the caller's slot.
        SovereignAudioConfig::Patch { .. } | SovereignAudioConfig::Sequence { .. } => {
            let audio = audio.clone();
            let pool = AsyncComputeTaskPool::get();
            let task = pool.spawn(async move { bake_construct_wav_bytes(&audio) });
            commands.spawn(SpatialAudioBakeTask {
                target,
                task,
                mode: BakeAttachmentMode::LoopingConstruct,
            });
        }
    }
}

/// Spawn a transient one-shot audio entity at `position` and dispatch
/// a bake of `audio`. Once the bake completes, an `AudioPlayer` with
/// `PlaybackMode::Despawn` is attached so the entity self-cleans after
/// the sound finishes. Used by [`crate::audio_materials`]'s terrain
/// impact-trigger system for footstep / landing SFX.
///
/// `volume` is the linear volume in `[0, 1]` for the playback. Caller
/// is responsible for clamping; values outside the range are passed
/// through to Bevy's `Volume::Linear` unchanged.
pub fn dispatch_one_shot_audio(
    commands: &mut Commands,
    position: Vec3,
    audio: &SovereignAudioConfig,
    volume: f32,
) {
    let audio = audio.clone();
    if matches!(
        &audio,
        SovereignAudioConfig::None
            | SovereignAudioConfig::Unknown
            | SovereignAudioConfig::Referenced { .. }
    ) {
        return;
    }
    // Pre-spawn the carrier entity at the impact location so the
    // bake-task component can target it deterministically. Bevy's
    // spatial audio reads the entity's Transform, so positioning here
    // (rather than only inserting AudioPlayer on completion) means a
    // very fast bake still has a stable source position.
    let target = commands
        .spawn((
            Transform::from_translation(position),
            GlobalTransform::default(),
            Visibility::default(),
            OneShotAudioVoice { volume },
            crate::world_builder::RoomEntity,
        ))
        .id();
    let pool = AsyncComputeTaskPool::get();
    let task = pool.spawn(async move { bake_construct_wav_bytes(&audio) });
    commands.spawn(SpatialAudioBakeTask {
        target,
        task,
        mode: BakeAttachmentMode::OneShot { volume },
    });
}

/// Marker on a transient one-shot voice entity. Carries the requested
/// playback volume so the poll system can pick it up at attach time
/// (without threading volume through the task channel).
#[derive(Component)]
pub struct OneShotAudioVoice {
    pub volume: f32,
}

/// Drain finished spatial bakes and attach a looping spatial
/// `AudioPlayer` to the corresponding construct entity.
///
/// If the target entity has been despawned between dispatch and
/// completion (room transition, sanitiser eviction), the insert is a
/// no-op via Bevy's `get_entity()` check — silently dropping the
/// orphaned bake rather than logging at warn-level, because rapid
/// room rebuilds during editing make orphan bakes common.
pub fn poll_spatial_audio_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut SpatialAudioBakeTask)>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    for (task_entity, mut bake) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut bake.task))
        else {
            continue;
        };
        commands.entity(task_entity).despawn();
        let Some((bytes, _sample_rate)) = result else {
            // bake_construct_wav_bytes returned None — malformed JSON
            // or future variants. Silent fallback is correct here; a
            // construct with broken audio just doesn't hum.
            continue;
        };
        let handle = audio_sources.add(AudioSource {
            bytes: bytes.into(),
        });
        // Insert on a despawned target is a no-op (Bevy queues and
        // drops); we don't pre-check because the orphan-bake case
        // (room rebuild between dispatch and completion) is common
        // enough that the warn-log noise would drown the channel.
        let settings = match bake.mode {
            BakeAttachmentMode::LoopingConstruct => PlaybackSettings {
                spatial: true,
                ..PlaybackSettings::LOOP
            },
            BakeAttachmentMode::OneShot { volume } => PlaybackSettings {
                mode: PlaybackMode::Despawn,
                spatial: true,
                volume: Volume::Linear(volume.clamp(0.0, 1.0)),
                ..PlaybackSettings::ONCE
            },
        };
        commands
            .entity(bake.target)
            .insert((AudioPlayer::new(handle), settings));
    }
}

/// Bake the construct's audio config into WAV bytes off the main
/// thread. Mirrors the loading-gate ambient bake but returns
/// `(bytes, sample_rate)` so a future enhancement can preserve the
/// sample rate through to the AudioSource bytes encoding. Currently
/// the sample rate is encoded in the WAV header itself, so the
/// returned `sample_rate` is informational only.
fn bake_construct_wav_bytes(audio: &SovereignAudioConfig) -> Option<(Vec<u8>, u32)> {
    match audio {
        SovereignAudioConfig::None
        | SovereignAudioConfig::Unknown
        | SovereignAudioConfig::Referenced { .. } => None,
        SovereignAudioConfig::Patch { .. } => {
            // Construct one-shot patches loop on a 1-second window —
            // long enough that fast transient noises read clearly,
            // short enough that the loop seam isn't perceptible.
            let patch = audio.parse_patch()?;
            let samples = bevy_symbios_audio::bake(&patch, 44_100, 1.0);
            Some((
                bevy_symbios_audio::samples_to_wav_bytes(&samples, 44_100),
                44_100,
            ))
        }
        SovereignAudioConfig::Sequence { .. } => {
            let recipe = audio.parse_sequence()?;
            let sample_rate = recipe.sample_rate;
            let samples = bevy_symbios_audio::bake_sequence(&recipe);
            Some((
                bevy_symbios_audio::samples_to_wav_bytes(&samples, sample_rate),
                sample_rate,
            ))
        }
    }
}

/// Build a gentle teleporter hum — a quiet sine drone around 110 Hz
/// (low A) with a slow LFO modulating amplitude via a filter sweep.
/// Used by the [`crate::catalogue::items::my_teleporter`] entry as the
/// concrete proof-of-concept for #301's per-construct audio pipeline.
///
/// Reuses the impact patch's filter-as-amplitude-shaper trick (see
/// [`crate::audio_materials`] module docstring) because the audio
/// crate has no direct amplitude-multiplier node.
pub fn teleporter_hum_patch() -> bevy_symbios_audio::AudioPatch {
    use bevy_symbios_audio::{
        AudioPatch, BiquadLowpass, Connection, GraphNode, Lfo, LfoShape, NodeGraph, NodeId,
        NodeKind, SineOsc,
    };
    let sine_id = NodeId(0);
    let lfo_id = NodeId(1);
    let filter_id = NodeId(2);

    let sine = GraphNode {
        id: sine_id,
        kind: NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            // Slightly below unity so the filter sweep adding LFO
            // modulation doesn't push the output past 1.0 and into
            // clipping territory.
            amplitude: 0.6,
        }),
        inputs: BTreeMap::new(),
    };
    let lfo = GraphNode {
        id: lfo_id,
        kind: NodeKind::Lfo(Lfo {
            rate_hz: 0.4,
            shape: LfoShape::Sine,
            // Output sweep [0, 1] for the filter modulation.
            depth: 0.5,
            offset: 0.5,
        }),
        inputs: BTreeMap::new(),
    };
    let mut filter_inputs = BTreeMap::new();
    filter_inputs.insert("in".to_string(), vec![Connection::from_node(sine_id)]);
    // LFO sweeps the filter cutoff between ~150 Hz and ~650 Hz, so the
    // 110 Hz sine fundamental plus its harmonics breathe in and out.
    filter_inputs.insert(
        "cutoff_hz".to_string(),
        vec![Connection::modulation(lfo_id, 500.0)],
    );
    let filter = GraphNode {
        id: filter_id,
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 150.0,
            q: 1.5,
        }),
        inputs: filter_inputs,
    };

    AudioPatch {
        seed: 0,
        graph: NodeGraph {
            nodes: vec![sine, lfo, filter],
            output: filter_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::SovereignAssetReference;

    #[test]
    fn no_audio_variants_return_none() {
        assert!(bake_construct_wav_bytes(&SovereignAudioConfig::None).is_none());
        assert!(bake_construct_wav_bytes(&SovereignAudioConfig::Unknown).is_none());
        assert!(
            bake_construct_wav_bytes(&SovereignAudioConfig::Referenced {
                source: SovereignAssetReference::default(),
            })
            .is_none()
        );
    }

    #[test]
    fn default_variants_bake_to_silent_buffers() {
        // The structured mirror (#311) replaces the JSON-stash with
        // typed fields, so there's no longer a "malformed JSON" failure
        // path at this layer. A default Patch / Sequence carries an
        // empty graph that bakes to a buffer of zeros — non-None.
        let p = SovereignAudioConfig::Patch {
            patch: crate::pds::audio::SovereignAudioPatch::default(),
        };
        let s = SovereignAudioConfig::Sequence {
            recipe: crate::pds::audio::SovereignSequenceRecipe::default(),
        };
        assert!(bake_construct_wav_bytes(&p).is_some());
        assert!(bake_construct_wav_bytes(&s).is_some());
    }

    #[test]
    fn teleporter_hum_bakes_to_audible_wav() {
        let patch = teleporter_hum_patch();
        let stash = SovereignAudioConfig::from_patch(&patch);
        let (bytes, sample_rate) =
            bake_construct_wav_bytes(&stash).expect("hum bakes successfully");
        assert!(bytes.starts_with(b"RIFF"), "WAV header present");
        assert_eq!(sample_rate, 44_100);
        // 1 second at 44.1 kHz mono float32 = ~176 KB raw + ~44 byte
        // header. Sanity-check the magnitude.
        assert!(
            bytes.len() > 100_000,
            "hum WAV should be at least 100 KB; got {}",
            bytes.len()
        );
    }
}
