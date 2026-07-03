//! Per-construct spatial-audio bake-and-attach.
//!
//! Mirrors the [`crate::loading`] ambient-bake pipeline but at the
//! per-entity granularity that constructs need: when a
//! [`Generator`](crate::pds::Generator)
//! carries a non-`None` [`SovereignAudioConfig`] on its `audio` field,
//! we dispatch a background bake on
//! [`bevy::tasks::AsyncComputeTaskPool`] and, on completion, attach a
//! looping spatial [`AudioPlayer`] to the construct's spawned entity.
//! Bevy's built-in spatial-audio attenuation does the
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
//!         resolves via BakedAudioCache:     drains finished bakes,
//!         Ready  → attach immediately       promotes cache → Ready,
//!         Pending→ join waiter list         attaches AudioPlayer +
//!         miss   → register + spawn         Spatial to every waiter
//!         SpatialAudioBakeTask{key}
//! ```
//!
//! The cache ([`BakedAudioCache`]) is keyed by the serialised audio
//! config, so identical constructs — within one compile pass and
//! across the recompiles every World Editor edit triggers — share a
//! single bake. On the wasm build the bake pool *is* the main thread,
//! so every cache hit is a frame stall avoided.

use std::collections::BTreeMap;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings, SpatialScale, Volume};
use bevy::prelude::*;
use bevy::tasks::Task;

use crate::pds::SovereignAudioConfig;

/// Spatial-scale applied to looping construct + avatar-voice emitters. Bevy's
/// default spatial scale is `1.0` (one world-unit = one audio-unit), under
/// which rodio's inverse-distance falloff makes a small body's hum / shimmer
/// near-inaudible unless the camera is almost on top of it (the "only audible
/// when zoomed right in" report). The scale multiplies both emitter and
/// listener positions, so shrinking it stretches the audible range
/// proportionally — `0.25` ≈ carries ~4× farther — letting an avatar's engine
/// hum or arcane shimmer read from a normal viewing distance. One-shot impact
/// SFX keep the default scale (they fire right next to the listener).
const CONSTRUCT_SPATIAL_SCALE: f32 = 0.25;

/// Playback settings for a looping, spatial construct / avatar-voice emitter:
/// Bevy's `LOOP` shape, spatialised, with the gentler [`CONSTRUCT_SPATIAL_SCALE`]
/// so the loop carries across a normal viewing distance.
fn looping_construct_playback() -> PlaybackSettings {
    PlaybackSettings {
        spatial: true,
        spatial_scale: Some(SpatialScale::new(CONSTRUCT_SPATIAL_SCALE)),
        ..PlaybackSettings::LOOP
    }
}

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

/// In-flight audio bake for one cache key. Targets waiting on the
/// result live in the [`BakedAudioCache`]'s `Pending` entry for `key`,
/// so any number of identical constructs (or repeated impacts) share a
/// single bake.
#[derive(Component)]
pub struct SpatialAudioBakeTask {
    pub key: String,
    pub task: Task<crate::offload::GenResult>,
}

/// Hard cap on retained baked buffers. Keys are full serialised audio
/// configs and values pin `AudioSource` byte buffers, so the cache must
/// stay bounded across long editing sessions and portal hops; FIFO
/// eviction (Ready entries only — evicting a Pending entry would orphan
/// its waiters) keeps the most recently authored sounds resident.
const MAX_BAKED_AUDIO_ENTRIES: usize = 64;

/// One cache slot: a bake in flight (with the entities to attach once
/// it lands) or the finished shared buffer.
enum BakedAudioEntry {
    Pending(Vec<(Entity, BakeAttachmentMode)>),
    Ready(Handle<AudioSource>),
}

/// Content-keyed cache of baked procedural audio buffers.
///
/// The key is the serialised [`SovereignAudioConfig`], so the cache is
/// hit by: identical constructs within one compile pass (five
/// teleporters bake one hum), the same constructs across recompiles
/// (every World Editor edit used to re-bake every audio-carrying
/// construct from scratch), and repeated terrain impacts on the same
/// material (which used to bake per footstep). This matters most on
/// wasm, where the "async" bake pool runs on the main thread and every
/// avoided bake is an avoided frame stall.
///
/// Reset by `logout::cleanup_on_logout`, FIFO-bounded at
/// [`MAX_BAKED_AUDIO_ENTRIES`] otherwise.
#[derive(Resource, Default)]
pub struct BakedAudioCache {
    entries: std::collections::HashMap<String, BakedAudioEntry>,
    /// Insertion order for FIFO eviction.
    order: std::collections::VecDeque<String>,
}

impl BakedAudioCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    /// Evict oldest `Ready` entries until under the cap. `Pending`
    /// entries are never evicted — their waiter lists must survive
    /// until the bake lands.
    fn evict_to_cap(&mut self) {
        while self.entries.len() > MAX_BAKED_AUDIO_ENTRIES {
            let Some(pos) = self
                .order
                .iter()
                .position(|k| matches!(self.entries.get(k), Some(BakedAudioEntry::Ready(_))))
            else {
                // Everything is Pending (pathological) — nothing safely
                // evictable; allow temporary overshoot.
                break;
            };
            if let Some(key) = self.order.remove(pos) {
                self.entries.remove(&key);
            }
        }
    }
}

/// Attach the baked buffer to `target` with the playback shape `mode`
/// asks for. Uses `try_insert` so an insert on a despawned target is a
/// silent no-op — the orphan case (room rebuild between dispatch and bake
/// completion) is common enough during editing that warn-logs would drown
/// the channel, and in Bevy 0.18 a plain `insert` on a missing entity
/// panics through the command error handler instead of dropping quietly.
fn attach_baked_audio(
    commands: &mut Commands,
    target: Entity,
    mode: BakeAttachmentMode,
    handle: Handle<AudioSource>,
) {
    let settings = match mode {
        BakeAttachmentMode::LoopingConstruct => looping_construct_playback(),
        BakeAttachmentMode::OneShot { volume } => PlaybackSettings {
            mode: PlaybackMode::Despawn,
            spatial: true,
            volume: Volume::Linear(volume.clamp(0.0, 1.0)),
            ..PlaybackSettings::ONCE
        },
    };
    commands
        .entity(target)
        .try_insert((AudioPlayer::new(handle), settings));
}

/// Resolve `audio` through the bake cache: attach immediately on a
/// `Ready` hit, join the waiter list of an in-flight bake, or register
/// a fresh `Pending` entry and dispatch the bake task. Callers have
/// already filtered the non-procedural variants.
fn request_baked_audio(
    commands: &mut Commands,
    bake_cache: &mut BakedAudioCache,
    audio: &SovereignAudioConfig,
    target: Entity,
    mode: BakeAttachmentMode,
) {
    let key = match serde_json::to_string(audio) {
        Ok(key) => key,
        Err(e) => {
            // Plain-data types — this is unreachable in practice, and a
            // construct without its hum is the right degraded mode.
            warn!("Construct audio config failed to serialise for bake cache: {e}");
            return;
        }
    };

    match bake_cache.entries.get_mut(&key) {
        Some(BakedAudioEntry::Ready(handle)) => {
            let handle = handle.clone();
            attach_baked_audio(commands, target, mode, handle);
        }
        Some(BakedAudioEntry::Pending(waiters)) => {
            waiters.push((target, mode));
        }
        None => {
            // Build the offloadable job first; a malformed procedural config
            // yields no job — the construct simply doesn't hum, and we leave no
            // cache entry so a corrected config can re-bake later.
            let Some(job) = construct_bake_job(audio) else {
                return;
            };
            bake_cache
                .entries
                .insert(key.clone(), BakedAudioEntry::Pending(vec![(target, mode)]));
            bake_cache.order.push_back(key.clone());
            bake_cache.evict_to_cap();

            let task = crate::offload::offload(crate::offload::GenJob::AudioBake(job));
            commands.spawn(SpatialAudioBakeTask { key, task });
        }
    }
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
    bake_cache: &mut BakedAudioCache,
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
                    settings: looping_construct_playback(),
                },
            );
        }
        // Procedural — resolve through the content-keyed bake cache:
        // identical configs (the same construct re-spawned by a room
        // recompile, or N copies of one catalogue item) share a single
        // bake and a single buffer.
        SovereignAudioConfig::Patch { .. } | SovereignAudioConfig::Sequence { .. } => {
            request_baked_audio(
                commands,
                bake_cache,
                audio,
                target,
                BakeAttachmentMode::LoopingConstruct,
            );
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
    bake_cache: &mut BakedAudioCache,
    position: Vec3,
    audio: &SovereignAudioConfig,
    volume: f32,
) {
    if matches!(
        audio,
        SovereignAudioConfig::None
            | SovereignAudioConfig::Unknown
            | SovereignAudioConfig::Referenced { .. }
    ) {
        return;
    }
    // Pre-spawn the carrier entity at the impact location so the
    // attach (cache hit: this frame; cache miss: when the bake lands)
    // can target it deterministically. Bevy's spatial audio reads the
    // entity's Transform, so positioning here means even an immediate
    // attach has a stable source position.
    let target = commands
        .spawn((
            Transform::from_translation(position),
            GlobalTransform::default(),
            Visibility::default(),
            crate::world_builder::RoomEntity,
        ))
        .id();
    // Cached path: repeated impacts on the same material (the caller
    // bakes at unit volume and scales at playback) hit `Ready` after
    // the first bake and never touch the bake pool again.
    request_baked_audio(
        commands,
        bake_cache,
        audio,
        target,
        BakeAttachmentMode::OneShot { volume },
    );
}

/// Drain finished spatial bakes: promote the cache entry to `Ready`
/// and attach the shared buffer to every entity that queued on it.
///
/// Targets despawned between dispatch and completion (room transition,
/// recompile) make the insert a queued-and-dropped no-op — rapid room
/// rebuilds during editing make orphan waiters common, so no warn-log.
pub fn poll_spatial_audio_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut SpatialAudioBakeTask)>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut bake_cache: ResMut<BakedAudioCache>,
) {
    for (task_entity, mut bake) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut bake.task))
        else {
            continue;
        };
        commands.entity(task_entity).despawn();

        // Pull the waiter list out of the Pending slot. A missing entry
        // means the cache was cleared (logout) mid-bake — nothing to
        // attach, nothing to retain.
        let waiters = match bake_cache.entries.remove(&bake.key) {
            Some(BakedAudioEntry::Pending(waiters)) => waiters,
            Some(ready @ BakedAudioEntry::Ready(_)) => {
                // Shouldn't happen (one task per key), but restoring the
                // Ready entry beats discarding a usable buffer.
                bake_cache.entries.insert(bake.key.clone(), ready);
                Vec::new()
            }
            None => Vec::new(),
        };

        let crate::offload::GenResult::Audio(bytes) = result else {
            // Unreachable: an AudioBake job yields Audio. Stay graceful — drop
            // the entry so a corrected config can re-bake.
            bake_cache.order.retain(|k| k != &bake.key);
            continue;
        };
        let handle = audio_sources.add(AudioSource {
            bytes: bytes.into(),
        });
        bake_cache
            .entries
            .insert(bake.key.clone(), BakedAudioEntry::Ready(handle.clone()));

        for (target, mode) in waiters {
            attach_baked_audio(&mut commands, target, mode, handle.clone());
        }
    }
}

/// Build the offloadable bake job for a construct's *procedural* audio config.
/// `None` for non-procedural variants or malformed JSON (the construct then
/// simply doesn't hum). Construct patches loop on a 1-second window — long
/// enough for transients to read, short enough that the loop seam is
/// imperceptible. The heavy synth runs off-thread via [`crate::offload`].
fn construct_bake_job(audio: &SovereignAudioConfig) -> Option<gen_jobs::AudioBakeJob> {
    match audio {
        SovereignAudioConfig::None
        | SovereignAudioConfig::Unknown
        | SovereignAudioConfig::Referenced { .. } => None,
        SovereignAudioConfig::Patch { .. } => Some(gen_jobs::AudioBakeJob::Patch {
            patch: audio.parse_patch()?,
            // 22.05 kHz halves the baked + cached buffers; per-construct hums
            // are within the 11 kHz Nyquist (#568, matches the ambient rate).
            sample_rate: 22_050,
            duration_secs: 1.0,
        }),
        SovereignAudioConfig::Sequence { .. } => Some(gen_jobs::AudioBakeJob::Sequence {
            recipe: audio.parse_sequence()?,
        }),
    }
}

/// Test helper: run the construct bake path synchronously, returning
/// `(wav_bytes, sample_rate)` to match the prior contract.
#[cfg(test)]
fn bake_construct_wav_bytes(audio: &SovereignAudioConfig) -> Option<(Vec<u8>, u32)> {
    let job = construct_bake_job(audio)?;
    let sample_rate = match &job {
        gen_jobs::AudioBakeJob::Patch { sample_rate, .. } => *sample_rate,
        gen_jobs::AudioBakeJob::Sequence { recipe } => recipe.sample_rate,
    };
    match gen_jobs::GenJob::AudioBake(job).run() {
        gen_jobs::GenResult::Audio(bytes) => Some((bytes, sample_rate)),
        _ => None,
    }
}

/// Build a gentle teleporter hum — a quiet sine drone around 110 Hz
/// (low A) with a slow LFO modulating amplitude via a filter sweep.
/// Used by the [`crate::catalogue::items::tools::my_teleporter`] entry as the
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

    fn ready_entry() -> BakedAudioEntry {
        BakedAudioEntry::Ready(Handle::default())
    }

    #[test]
    fn bake_cache_evicts_oldest_ready_at_cap() {
        let mut cache = BakedAudioCache::default();
        for i in 0..=MAX_BAKED_AUDIO_ENTRIES {
            let key = format!("cfg-{i}");
            cache.entries.insert(key.clone(), ready_entry());
            cache.order.push_back(key);
            cache.evict_to_cap();
        }
        assert_eq!(cache.entries.len(), MAX_BAKED_AUDIO_ENTRIES);
        // FIFO: the first-inserted key is the one that went.
        assert!(!cache.entries.contains_key("cfg-0"));
        assert!(
            cache
                .entries
                .contains_key(&format!("cfg-{}", MAX_BAKED_AUDIO_ENTRIES))
        );
    }

    #[test]
    fn bake_cache_never_evicts_pending_entries() {
        let mut cache = BakedAudioCache::default();
        // Oldest entry is Pending — it must survive eviction because
        // its waiter list points at live entities.
        cache.entries.insert(
            "pending-0".into(),
            BakedAudioEntry::Pending(vec![(
                Entity::PLACEHOLDER,
                BakeAttachmentMode::LoopingConstruct,
            )]),
        );
        cache.order.push_back("pending-0".into());
        for i in 1..=MAX_BAKED_AUDIO_ENTRIES {
            let key = format!("cfg-{i}");
            cache.entries.insert(key.clone(), ready_entry());
            cache.order.push_back(key);
            cache.evict_to_cap();
        }
        assert!(cache.entries.contains_key("pending-0"));
        // The oldest READY entry was sacrificed instead.
        assert!(!cache.entries.contains_key("cfg-1"));
    }

    #[test]
    fn identical_configs_serialise_to_identical_cache_keys() {
        // The whole caching scheme rests on this: two constructs (or
        // two recompile passes) carrying equal configs must coalesce.
        let a = SovereignAudioConfig::Patch {
            patch: crate::pds::audio::SovereignAudioPatch::from_native(&teleporter_hum_patch()),
        };
        let b = SovereignAudioConfig::Patch {
            patch: crate::pds::audio::SovereignAudioPatch::from_native(&teleporter_hum_patch()),
        };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

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
        assert_eq!(sample_rate, 22_050);
        // 1 second at 22.05 kHz mono 16-bit PCM = ~44 KB raw + ~44 byte
        // header. Sanity-check the magnitude.
        assert!(
            bytes.len() > 40_000,
            "hum WAV should be at least 40 KB; got {}",
            bytes.len()
        );
    }
}
