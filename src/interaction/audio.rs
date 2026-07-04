//! Concrete `bevy_audio` contact-cue consumer — channel D of the
//! interaction framework (#262; replaces the no-op `ContactAudioHook`
//! trait that #246's remainder shipped).
//!
//! Walks this frame's [`AvatarContacts`] against the
//! [`ContactRecipeRegistry::audio`] cues and plays a one-shot sound for
//! every matched `(sample, recipe)` — cooldown-throttled per
//! `(avatar, recipe)`, volume scaled by contact speed, optionally
//! spatialised at the contact point. Inert (zero cost) until a room
//! authors a [`ContactEffectKind::AudioCue`](crate::pds::ContactEffectKind)
//! recipe.
//!
//! Clips are fetched once and cached ([`AudioClipCache`]) via the
//! shared [`blob_fetch`] path (URL or
//! ATProto `getBlob`, same as Sign textures). The first trigger for an
//! uncached clip primes the cache asynchronously and is silent; every
//! later trigger plays synchronously. Decoding is rodio's — Bevy's
//! default audio feature is `vorbis`, so v1 clips are Ogg/Vorbis.
//!
//! [`PlaybackMode::Despawn`] makes a finished voice GC itself, so there
//! is no manual reaper — only a global concurrent-voice cap
//! ([`vcfg::MAX_CONCURRENT_VOICES`]) and a room-exit stop.

use std::collections::{HashMap, VecDeque};

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};

use crate::config::interaction::audio as vcfg;
use crate::pds::AudioClipSource;
use crate::state::AppState;
use crate::world_builder::blob_fetch;

use super::contact::AvatarContacts;
use super::cooldown::CooldownTable;
use super::plugin::ContactProducerSet;
use super::recipes::ContactRecipeRegistry;

/// Fetchable identity of an authored clip. Mirrors the resolvable
/// [`AudioClipSource`] variants; `Unknown` / empty inputs yield `None`
/// (nothing to fetch) so the cache never spins on a 404.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioClipKey {
    Url(String),
    AtprotoBlob { did: String, cid: String },
}

impl AudioClipKey {
    pub fn from_source(s: &AudioClipSource) -> Option<Self> {
        match s {
            AudioClipSource::Url { url } if !url.is_empty() => Some(Self::Url(url.clone())),
            AudioClipSource::AtprotoBlob { did, cid } if !did.is_empty() && !cid.is_empty() => {
                Some(Self::AtprotoBlob {
                    did: did.clone(),
                    cid: cid.clone(),
                })
            }
            _ => None,
        }
    }
}

enum AudioClipEntry {
    /// Fetch in flight — later triggers for this key are silent until
    /// it lands (a missed lead-in cue is acceptable; the cache primes
    /// on first trigger).
    Pending,
    /// Decoded and asset-resident — plays synchronously.
    Ready(Handle<AudioSource>),
}

/// Source-keyed clip cache. FIFO-bounded by
/// [`vcfg::MAX_CACHE_ENTRIES`] (an attacker streaming randomised source
/// URLs can't grow client memory without bound). Cleared on room exit
/// so a new room re-fetches sources that may have changed upstream.
#[derive(Resource, Default)]
pub struct AudioClipCache {
    map: HashMap<AudioClipKey, AudioClipEntry>,
    order: VecDeque<AudioClipKey>,
}

impl AudioClipCache {
    fn insert_bounded(&mut self, key: AudioClipKey, entry: AudioClipEntry) {
        if !self.map.contains_key(&key) {
            while self.order.len() >= vcfg::MAX_CACHE_ENTRIES {
                match self.order.pop_front() {
                    Some(oldest) => {
                        self.map.remove(&oldest);
                    }
                    None => break,
                }
            }
            self.order.push_back(key.clone());
        }
        self.map.insert(key, entry);
    }

    fn remove(&mut self, key: &AudioClipKey) {
        if self.map.remove(key).is_some() {
            self.order.retain(|k| k != key);
        }
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

/// In-flight clip fetch, parked on a throwaway entity so it survives
/// room rebuilds and is GC'd when its poll completes.
#[derive(Component)]
pub struct AudioClipTask {
    key: AudioClipKey,
    task: Task<Option<Vec<u8>>>,
}

/// Per-`(avatar, audio-recipe index)` cooldown state — a shared
/// [`CooldownTable`] behind this channel's own `Resource` type (mirrors
/// the particle / decal channels).
#[derive(Resource)]
pub struct AudioCueState {
    cooldowns: CooldownTable,
}

/// Drop cooldown entries older than this (s) — far longer than any sane
/// per-recipe cooldown, so pruning never resets a live throttle.
const COOLDOWN_ENTRY_TTL: f32 = 30.0;

impl Default for AudioCueState {
    fn default() -> Self {
        Self {
            cooldowns: CooldownTable::new(COOLDOWN_ENTRY_TTL),
        }
    }
}

/// Marks a spawned cue entity so the global voice cap can count live
/// voices and room-exit can stop them. Never added to any other audio.
#[derive(Component)]
pub struct ContactAudioVoice;

/// Cheap deterministic-enough pseudo-random in `[-1, 1]` from a few
/// integers — same policy as the particle dispatcher (cosmetic audio
/// doesn't need a seeded RNG resource, just non-repetition).
fn hash_unit(a: u64, b: u64, c: u64) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ c.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    // Map the top 24 bits to [-1, 1].
    ((h >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
}

/// Spawn the IoTaskPool fetch for an uncached clip. Mirrors
/// `image_cache`'s task shape (block a current-thread tokio runtime on
/// native; await directly on wasm).
fn spawn_fetch(commands: &mut Commands, key: AudioClipKey) {
    let pool = IoTaskPool::get();
    let key_for_task = key.clone();
    let task = pool.spawn(async move {
        let client = crate::config::http::default_client();
        let fut = async {
            match &key_for_task {
                AudioClipKey::Url(url) => {
                    blob_fetch::fetch_url_bytes(&client, url, vcfg::MAX_CLIP_BYTES, "Audio cue")
                        .await
                }
                AudioClipKey::AtprotoBlob { did, cid } => {
                    blob_fetch::fetch_blob_bytes(
                        &client,
                        did,
                        cid,
                        vcfg::MAX_CLIP_BYTES,
                        "Audio cue",
                    )
                    .await
                }
            }
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
    commands.spawn(AudioClipTask { key, task });
}

/// Phase-4 consumer: `AvatarContacts × audio cues` → one-shot
/// `bevy_audio` voices. Ordered `.after(ContactProducerSet)` so it
/// reads this frame's contacts.
#[allow(clippy::too_many_arguments)]
pub fn play_contact_audio(
    time: Res<Time>,
    contacts: Res<AvatarContacts>,
    registry: Res<ContactRecipeRegistry>,
    mut cache: ResMut<AudioClipCache>,
    mut state: ResMut<AudioCueState>,
    live_voices: Query<(), With<ContactAudioVoice>>,
    mut commands: Commands,
) {
    if registry.audio.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let mut voices = live_voices.iter().count();

    for sample in &contacts.samples {
        for (idx, recipe) in registry.audio.iter().enumerate() {
            if !recipe.enabled || !recipe.trigger.matches(sample) {
                continue;
            }
            // Per-(avatar, recipe) cooldown.
            if recipe.cooldown > 0.0
                && state
                    .cooldowns
                    .active((sample.avatar, idx), now, recipe.cooldown)
            {
                continue;
            }
            let Some(clip_key) = AudioClipKey::from_source(&recipe.params.source) else {
                continue; // Unknown / empty source — nothing to play.
            };

            // Resolve the clip; prime the cache on first sight.
            let handle = match cache.map.get(&clip_key) {
                Some(AudioClipEntry::Ready(h)) => h.clone(),
                Some(AudioClipEntry::Pending) => continue, // still loading
                None => {
                    cache.insert_bounded(clip_key.clone(), AudioClipEntry::Pending);
                    spawn_fetch(&mut commands, clip_key);
                    continue;
                }
            };

            // Global concurrent-voice cap — drop, never queue.
            if voices >= vcfg::MAX_CONCURRENT_VOICES {
                break;
            }

            let p = &recipe.params;
            let volume = p.volume_for(sample);
            let jitter = if p.pitch_jitter > 0.0 {
                hash_unit(sample.avatar.to_bits(), idx as u64, (now * 1000.0) as u64)
                    * p.pitch_jitter
            } else {
                0.0
            };
            let speed = (p.pitch + jitter).max(0.05);

            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(volume),
                    speed,
                    spatial: p.spatial,
                    ..PlaybackSettings::ONCE
                },
                // Position the emitter for spatial panning; harmless
                // for non-spatial cues.
                Transform::from_translation(sample.world_pos),
                ContactAudioVoice,
            ));
            voices += 1;

            if recipe.cooldown > 0.0 {
                state.cooldowns.mark((sample.avatar, idx), now);
            }
        }
    }

    // Prune stale cooldown entries (despawned avatars, long-idle).
    state.cooldowns.prune(now);
}

/// Drain finished clip fetches: wrap the bytes in an [`AudioSource`]
/// asset and promote the cache entry to `Ready`. A failed fetch drops
/// the entry so a later trigger re-attempts instead of staying silent
/// forever behind a transient blip.
pub fn poll_audio_clip_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AudioClipTask)>,
    mut assets: ResMut<Assets<AudioSource>>,
    mut cache: ResMut<AudioClipCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        match result {
            Some(bytes) => {
                let handle = assets.add(AudioSource {
                    bytes: bytes.into(),
                });
                cache.insert_bounded(task.key.clone(), AudioClipEntry::Ready(handle));
            }
            None => {
                // Fetch failed — drop so the next trigger retries.
                cache.remove(&task.key);
            }
        }
    }
}

/// Room exit: stop every live cue voice, drop in-flight clip fetches,
/// and clear the clip cache so a new room starts silent and re-fetches
/// (sources may have changed).
///
/// In-flight [`AudioClipTask`] entities must go too:
/// [`poll_audio_clip_tasks`] only runs in `AppState::InGame`, so a task
/// outstanding at exit would otherwise sit unpolled in the ECS world
/// for as long as the app stays out of game (e.g. after a logout).
/// Despawning drops the `Task`, which cancels the fetch.
pub fn cleanup_audio(
    mut commands: Commands,
    voices: Query<Entity, With<ContactAudioVoice>>,
    tasks: Query<Entity, With<AudioClipTask>>,
    mut cache: ResMut<AudioClipCache>,
) {
    for e in voices.iter().chain(tasks.iter()) {
        commands.entity(e).despawn();
    }
    cache.clear();
}

/// Register the audio-cue channel. Inert until a room authors a
/// [`ContactEffectKind::AudioCue`](crate::pds::ContactEffectKind)
/// recipe (the registry's `audio` list is empty by default).
pub fn build(app: &mut App) {
    app.init_resource::<AudioClipCache>()
        .init_resource::<AudioCueState>()
        .add_systems(
            Update,
            (
                play_contact_audio.after(ContactProducerSet),
                poll_audio_clip_tasks,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), cleanup_audio);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_key_maps_resolvable_sources_only() {
        assert_eq!(
            AudioClipKey::from_source(&AudioClipSource::Url {
                url: "https://x.test/a.ogg".into()
            }),
            Some(AudioClipKey::Url("https://x.test/a.ogg".into()))
        );
        assert_eq!(
            AudioClipKey::from_source(&AudioClipSource::AtprotoBlob {
                did: "did:plc:x".into(),
                cid: "bafy".into()
            }),
            Some(AudioClipKey::AtprotoBlob {
                did: "did:plc:x".into(),
                cid: "bafy".into()
            })
        );
        // Empty required fields and Unknown are not fetchable.
        assert_eq!(
            AudioClipKey::from_source(&AudioClipSource::Url { url: String::new() }),
            None
        );
        assert_eq!(AudioClipKey::from_source(&AudioClipSource::Unknown), None);
    }

    #[test]
    fn cache_is_fifo_bounded() {
        let mut c = AudioClipCache::default();
        for i in 0..vcfg::MAX_CACHE_ENTRIES {
            c.insert_bounded(AudioClipKey::Url(format!("u{i}")), AudioClipEntry::Pending);
        }
        assert_eq!(c.map.len(), vcfg::MAX_CACHE_ENTRIES);
        assert!(c.map.contains_key(&AudioClipKey::Url("u0".into())));
        // One over cap → oldest ("u0") evicted.
        c.insert_bounded(AudioClipKey::Url("over".into()), AudioClipEntry::Pending);
        assert_eq!(c.map.len(), vcfg::MAX_CACHE_ENTRIES);
        assert!(!c.map.contains_key(&AudioClipKey::Url("u0".into())));
        assert!(c.map.contains_key(&AudioClipKey::Url("over".into())));
        // Replacing a key (Pending → Ready) keeps its FIFO slot.
        let before = c.order.len();
        c.insert_bounded(
            AudioClipKey::Url("u1".into()),
            AudioClipEntry::Ready(Handle::default()),
        );
        assert_eq!(c.order.len(), before);
        c.remove(&AudioClipKey::Url("u1".into()));
        assert!(!c.map.contains_key(&AudioClipKey::Url("u1".into())));
    }

    #[test]
    fn pitch_jitter_is_bounded_and_varies() {
        // hash_unit stays in [-1, 1] so speed = pitch ± jitter is sane.
        for i in 0..1000u64 {
            let v = hash_unit(i, i.wrapping_mul(7), i ^ 0xABCD);
            assert!((-1.0..=1.0).contains(&v), "hash_unit out of range: {v}");
        }
        // Different inputs generally differ (not a constant).
        assert!(hash_unit(1, 2, 3) != hash_unit(4, 5, 6));
    }
}
