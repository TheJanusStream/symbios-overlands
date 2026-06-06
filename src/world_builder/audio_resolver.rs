//! Source-keyed coalescing cache for [`SovereignAssetReference`] audio
//! fetches. Sister to [`super::image_cache::BlobImageCache`]; reuses the
//! shared [`super::blob_fetch`] HTTPS-GET + ATProto `getBlob` primitives
//! and the same FIFO-bounded eviction so a hostile peer streaming
//! randomised reference URLs can't grow client memory without bound.
//!
//! # Two target shapes
//!
//! Audio references are consumed by two different parts of the engine
//! and the cache supports both via a single dispatch enum:
//!
//! * **Per-entity (constructs)** — a Generator carrying a Referenced
//!   audio source spawns a construct entity that should hum / drone /
//!   chime at its world position via spatial audio.
//!   [`AudioReferenceTarget::AttachToEntity`] inserts an
//!   [`AudioPlayer`] + [`PlaybackSettings`] on that entity once bytes
//!   land.
//! * **Resource (ambient)** — the loading-gate ambient bake (#297)
//!   uses [`AudioReferenceTarget::AmbientHandle`] to publish the
//!   resolved handle into [`crate::loading::AmbientHandle`] so the
//!   InGame ambient-player spawner picks it up.
//!
//! # What's not handled here
//!
//! - [`SovereignAssetReference::DidPfp`] is image-only; the resolver
//!   ignores it (a JPEG isn't an audio source). Documented on the
//!   ref enum itself.
//! - The room-authored [`crate::interaction::audio::AudioClipCache`]
//!   has its own separate cache because contact cues are scoped to a
//!   single room and use a different reference type
//!   ([`crate::pds::AudioClipSource`]). The two caches don't dedup
//!   across each other — that's a deliberate scope choice to keep #308
//!   from churning the proven contact-cue path.

use std::collections::{HashMap, VecDeque};

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};

use crate::config;
use crate::pds::SovereignAssetReference;

use super::blob_fetch;

/// Cache key for an audio reference. Mirrors the variant shape of the
/// fetchable [`SovereignAssetReference`] variants — DidPfp is excluded
/// because it resolves to a profile picture, not an audio blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioReferenceKey {
    Url(String),
    AtprotoBlob { did: String, cid: String },
}

impl AudioReferenceKey {
    /// Build a cache key from a reference. Returns `None` for
    /// non-fetchable variants (DidPfp, Unknown) and for refs whose
    /// required fields are empty (placeholder URL etc.) — the cache
    /// would otherwise loop on a 404.
    pub fn from_reference(reference: &SovereignAssetReference) -> Option<Self> {
        match reference {
            SovereignAssetReference::Url { url } if !url.is_empty() => Some(Self::Url(url.clone())),
            SovereignAssetReference::AtprotoBlob { did, cid }
                if !did.is_empty() && !cid.is_empty() =>
            {
                Some(Self::AtprotoBlob {
                    did: did.clone(),
                    cid: cid.clone(),
                })
            }
            _ => None,
        }
    }
}

/// Where the resolved [`Handle<AudioSource>`] should be delivered.
///
/// Kept as an enum (rather than a closure / dyn FnOnce) so the entry
/// list is `Clone + 'static + Send + Sync` — required because Bevy
/// components must be `Send + Sync`, and the Pending entry list lives
/// inside the cache resource.
#[derive(Clone, Debug)]
pub enum AudioReferenceTarget {
    /// Spatial-construct case — attach an `AudioPlayer` + the supplied
    /// `PlaybackSettings` to `entity` once the bake resolves.
    AttachToEntity {
        entity: Entity,
        settings: PlaybackSettings,
    },
    /// Ambient case — publish into [`crate::loading::AmbientHandle`].
    AmbientHandle,
}

/// Cache entry per [`AudioReferenceKey`]: either a list of targets
/// waiting on the in-flight fetch, or a finished handle ready to
/// dispatch synchronously.
pub enum AudioReferenceEntry {
    Pending(Vec<AudioReferenceTarget>),
    Ready(Handle<AudioSource>),
}

/// Source-keyed coalescing cache. FIFO-bounded by
/// [`config::interaction::audio::MAX_CACHE_ENTRIES`] — the same cap
/// the contact-cue cache uses so the two paths get the same memory
/// envelope.
#[derive(Resource, Default)]
pub struct BlobAudioCache {
    pub by_source: HashMap<AudioReferenceKey, AudioReferenceEntry>,
    insert_order: VecDeque<AudioReferenceKey>,
}

impl BlobAudioCache {
    pub fn clear(&mut self) {
        self.by_source.clear();
        self.insert_order.clear();
    }

    /// Insert `entry` for `key`, evicting the oldest if at capacity.
    /// Replacing an existing key (`Pending → Ready`) preserves the
    /// entry's FIFO position — same contract as `BlobImageCache`.
    pub fn insert_bounded(&mut self, key: AudioReferenceKey, entry: AudioReferenceEntry) {
        let max = config::interaction::audio::MAX_CACHE_ENTRIES;
        if !self.by_source.contains_key(&key) {
            while self.insert_order.len() >= max {
                match self.insert_order.pop_front() {
                    Some(oldest) => {
                        self.by_source.remove(&oldest);
                    }
                    None => break,
                }
            }
            self.insert_order.push_back(key.clone());
        }
        self.by_source.insert(key, entry);
    }

    pub fn remove(&mut self, key: &AudioReferenceKey) -> Option<AudioReferenceEntry> {
        let removed = self.by_source.remove(key);
        if removed.is_some() {
            self.insert_order.retain(|k| k != key);
        }
        removed
    }
}

/// In-flight audio fetch. Attached to a throwaway entity so the task
/// survives across room rebuilds and despawns on completion.
#[derive(Component)]
pub struct BlobAudioTask {
    pub key: AudioReferenceKey,
    pub task: Task<Option<Vec<u8>>>,
}

/// Request the bytes for `reference` and deliver the resolved handle
/// to `target`. No-op for non-fetchable references (DidPfp, Unknown,
/// empty placeholders). Coalesces with any in-flight fetch for the
/// same source.
pub fn request_blob_audio(
    commands: &mut Commands,
    cache: &mut BlobAudioCache,
    reference: &SovereignAssetReference,
    target: AudioReferenceTarget,
) {
    let Some(key) = AudioReferenceKey::from_reference(reference) else {
        return;
    };

    match cache.by_source.get_mut(&key) {
        // Cache hit — dispatch synchronously.
        Some(AudioReferenceEntry::Ready(handle)) => {
            apply_target(commands, &target, handle.clone());
        }
        // Fetch already in flight — enqueue the target.
        Some(AudioReferenceEntry::Pending(list)) => {
            list.push(target);
        }
        // First requester — register pending and spawn the fetch.
        None => {
            cache.insert_bounded(key.clone(), AudioReferenceEntry::Pending(vec![target]));
            let pool = IoTaskPool::get();
            let source_for_task = key.clone();
            let task = pool.spawn(async move {
                let fut = fetch_bytes_for(source_for_task);
                #[cfg(target_arch = "wasm32")]
                {
                    fut.await
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    crate::config::http::block_on(fut)
                }
            });
            commands.spawn(BlobAudioTask { key, task });
        }
    }
}

/// Drain finished blob-audio fetches: wrap bytes in [`AudioSource`],
/// dispatch the resolved handle to every waiting target, and promote
/// the cache entry to `Ready` for future requesters.
pub fn poll_blob_audio_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut BlobAudioTask)>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut cache: ResMut<BlobAudioCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // Take the pending list while leaving the entry's FIFO slot in
        // place — promotion preserves the original position (matches
        // BlobImageCache's documented contract).
        let pending = match cache.by_source.get_mut(&task.key) {
            Some(AudioReferenceEntry::Pending(list)) => std::mem::take(list),
            Some(AudioReferenceEntry::Ready(_)) => continue, // already promoted
            None => continue,
        };

        let Some(bytes) = result else {
            // Fetch failed. Drop the pending entry so a later
            // requester gets a fresh attempt instead of being stuck
            // behind a transient network blip — but walk the pending
            // list FIRST and dispatch a None to AmbientHandle targets
            // so the loading gate isn't stranded on a dead URL. Entity
            // targets get nothing attached (the construct stays silent
            // — preferable to a default fallback hum the room author
            // didn't ask for).
            for target in pending {
                if matches!(target, AudioReferenceTarget::AmbientHandle) {
                    commands.insert_resource(crate::loading::AmbientHandle(None));
                }
            }
            cache.remove(&task.key);
            continue;
        };
        let handle = audio_sources.add(AudioSource {
            bytes: bytes.into(),
        });
        for target in pending {
            apply_target(&mut commands, &target, handle.clone());
        }
        cache.insert_bounded(task.key.clone(), AudioReferenceEntry::Ready(handle));
    }
}

/// Synchronous-dispatch arm shared by the cache-hit path and the
/// post-fetch poll path. For `AttachToEntity` this defers the actual
/// insert through `Commands` (and is therefore a no-op when the
/// entity has been despawned in the meantime). For `AmbientHandle`
/// the resource gets inserted / replaced unconditionally.
fn apply_target(
    commands: &mut Commands,
    target: &AudioReferenceTarget,
    handle: Handle<AudioSource>,
) {
    match target {
        AudioReferenceTarget::AttachToEntity { entity, settings } => {
            commands
                .entity(*entity)
                .insert((AudioPlayer::new(handle), *settings));
        }
        AudioReferenceTarget::AmbientHandle => {
            commands.insert_resource(crate::loading::AmbientHandle(Some(handle)));
        }
    }
}

/// Fetch the raw bytes for a key. Routes by variant — URL through
/// HTTPS GET, AtprotoBlob through `getBlob`. Reuses the shared
/// `blob_fetch` module so the OOM-guard and wasm/native split match
/// the image cache exactly.
async fn fetch_bytes_for(key: AudioReferenceKey) -> Option<Vec<u8>> {
    let client = config::http::default_client();
    let max = config::interaction::audio::MAX_CLIP_BYTES;
    match key {
        AudioReferenceKey::Url(url) => {
            blob_fetch::fetch_url_bytes(&client, &url, max, "AudioRef").await
        }
        AudioReferenceKey::AtprotoBlob { did, cid } => {
            blob_fetch::fetch_blob_bytes(&client, &did, &cid, max, "AudioRef").await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_key(s: &str) -> AudioReferenceKey {
        AudioReferenceKey::Url(s.to_string())
    }

    #[test]
    fn from_reference_extracts_url() {
        let r = SovereignAssetReference::Url {
            url: "https://example.org/x.ogg".into(),
        };
        assert_eq!(
            AudioReferenceKey::from_reference(&r),
            Some(url_key("https://example.org/x.ogg"))
        );
    }

    #[test]
    fn from_reference_extracts_atproto_blob() {
        let r = SovereignAssetReference::AtprotoBlob {
            did: "did:plc:abc".into(),
            cid: "bafyrei...".into(),
        };
        assert_eq!(
            AudioReferenceKey::from_reference(&r),
            Some(AudioReferenceKey::AtprotoBlob {
                did: "did:plc:abc".into(),
                cid: "bafyrei...".into(),
            })
        );
    }

    #[test]
    fn empty_url_yields_no_key() {
        let r = SovereignAssetReference::Url { url: String::new() };
        assert!(AudioReferenceKey::from_reference(&r).is_none());
    }

    #[test]
    fn empty_blob_fields_yield_no_key() {
        let r = SovereignAssetReference::AtprotoBlob {
            did: "did:plc:abc".into(),
            cid: String::new(),
        };
        assert!(AudioReferenceKey::from_reference(&r).is_none());
    }

    #[test]
    fn did_pfp_yields_no_key() {
        // Image-only; the resolver explicitly does not handle pfp for
        // audio (a JPEG isn't an audio source). Per the docstring on
        // SovereignAssetReference::DidPfp.
        let r = SovereignAssetReference::DidPfp {
            did: "did:plc:abc".into(),
        };
        assert!(AudioReferenceKey::from_reference(&r).is_none());
    }

    #[test]
    fn unknown_yields_no_key() {
        let r = SovereignAssetReference::Unknown;
        assert!(AudioReferenceKey::from_reference(&r).is_none());
    }

    #[test]
    fn cache_evicts_oldest_when_over_capacity() {
        let mut cache = BlobAudioCache::default();
        let max = config::interaction::audio::MAX_CACHE_ENTRIES;
        for i in 0..max {
            cache.insert_bounded(
                url_key(&format!("https://example.test/{i}")),
                AudioReferenceEntry::Pending(Vec::new()),
            );
        }
        assert_eq!(cache.by_source.len(), max);
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/0"))
        );

        cache.insert_bounded(
            url_key("https://example.test/overflow"),
            AudioReferenceEntry::Pending(Vec::new()),
        );
        assert_eq!(cache.by_source.len(), max);
        assert!(
            !cache
                .by_source
                .contains_key(&url_key("https://example.test/0")),
            "oldest entry must be evicted when overflowing"
        );
        assert!(
            cache
                .by_source
                .contains_key(&url_key("https://example.test/overflow")),
        );
    }

    #[test]
    fn promotion_preserves_fifo_position() {
        // Mirror BlobImageCache's documented contract: replacing
        // Pending with Ready must not re-queue at the back, so a
        // recently-completed entry doesn't artificially outlive the
        // FIFO bound.
        let mut cache = BlobAudioCache::default();
        let early = url_key("https://example.test/early");
        let middle = url_key("https://example.test/middle");
        let late = url_key("https://example.test/late");
        cache.insert_bounded(early.clone(), AudioReferenceEntry::Pending(Vec::new()));
        cache.insert_bounded(middle.clone(), AudioReferenceEntry::Pending(Vec::new()));
        cache.insert_bounded(late.clone(), AudioReferenceEntry::Pending(Vec::new()));

        cache.insert_bounded(
            middle.clone(),
            AudioReferenceEntry::Ready(Handle::default()),
        );

        let order: Vec<&AudioReferenceKey> = cache.insert_order.iter().collect();
        assert_eq!(order, vec![&early, &middle, &late]);
    }

    #[test]
    fn cache_clear_resets_both_structures() {
        let mut cache = BlobAudioCache::default();
        for i in 0..4 {
            cache.insert_bounded(
                url_key(&format!("https://example.test/{i}")),
                AudioReferenceEntry::Pending(Vec::new()),
            );
        }
        cache.clear();
        assert!(cache.by_source.is_empty());
        assert!(cache.insert_order.is_empty());
    }
}
