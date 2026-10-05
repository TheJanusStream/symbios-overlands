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
//! * **Per-entity (constructs)** - a Generator carrying a Referenced
//!   audio source spawns a construct entity that should hum / drone /
//!   chime at its world position via spatial audio.
//!   [`AudioReferenceTarget::AttachToEntity`] gives that entity its
//!   looping voice once bytes land
//!   ([`super::voice_budget::attach_looping_voice`]), and the voice
//!   budget inserts the [`AudioPlayer`] + [`PlaybackSettings`] while
//!   the entity is near enough to hear (#1557).
//! * **Resource (ambient)** - the loading-gate ambient bake (#297)
//!   uses [`AudioReferenceTarget::AmbientHandle`] to publish the
//!   resolved handle into [`crate::loading::AmbientHandle`] so the
//!   InGame ambient-player spawner picks it up.
//!
//! Fetched bytes become a clip only through [`super::audio_probe`], which
//! builds the decoder a voice would play them with: bytes that are not audio
//! fail like a fetch instead of crashing every visitor when a voice first
//! plays them (#1560).
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
//!   across each other - that's a deliberate scope choice to keep #308
//!   from churning the proven contact-cue path.

use std::collections::{HashMap, VecDeque};

use bevy::audio::{AudioSource, PlaybackSettings};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};

use crate::config;
use crate::pds::SovereignAssetReference;

use super::asset_failure::{AssetFailure, AssetFetchError, AssetStatus};
use super::blob_fetch;

/// Cache key for an audio reference. Mirrors the variant shape of the
/// fetchable [`SovereignAssetReference`] variants - DidPfp is excluded
/// because it resolves to a profile picture, not an audio blob.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioReferenceKey {
    Url(String),
    AtprotoBlob { did: String, cid: String },
}

impl AudioReferenceKey {
    /// Build a cache key from a reference. Returns `None` for
    /// non-fetchable variants (DidPfp, Unknown) and for refs whose
    /// required fields are empty (placeholder URL etc.) - the cache
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

    /// The identity a diagnostic line names.
    pub fn describe(&self) -> String {
        match self {
            Self::Url(url) => url.clone(),
            Self::AtprotoBlob { did, cid } => format!("{did}/{cid}"),
        }
    }

    /// Whether following this source means talking to a host somebody else
    /// chose (#1248 f298). See `SignSourceKey::is_external`.
    pub fn is_external(&self) -> bool {
        matches!(self, Self::Url(_))
    }
}

/// Where the resolved [`Handle<AudioSource>`] should be delivered.
///
/// Kept as an enum (rather than a closure / dyn FnOnce) so the entry
/// list is `Clone + 'static + Send + Sync` - required because Bevy
/// components must be `Send + Sync`, and the Pending entry list lives
/// inside the cache resource.
#[derive(Clone, Debug)]
pub enum AudioReferenceTarget {
    /// Spatial-construct case - give `entity` a looping voice that plays
    /// the clip with the supplied `PlaybackSettings` once the fetch
    /// resolves, played and held back by the voice budget like a baked one.
    AttachToEntity {
        entity: Entity,
        settings: PlaybackSettings,
    },
    /// Ambient case - publish into [`crate::loading::AmbientHandle`].
    AmbientHandle,
}

/// Cache entry per [`AudioReferenceKey`]: either a list of targets
/// waiting on the in-flight fetch, or a finished handle ready to
/// dispatch synchronously.
pub enum AudioReferenceEntry {
    Pending(Vec<AudioReferenceTarget>),
    Ready(Handle<AudioSource>),
    /// The fetch gave up (#1246). The entry survives so the ambient row
    /// and the audio bridge can say what happened, and so the next
    /// requester waits out the doubling instead of re-issuing at once.
    Failed(AssetFailure),
}

/// Source-keyed coalescing cache. FIFO-bounded by
/// [`config::interaction::audio::MAX_CACHE_ENTRIES`] - the same cap
/// the contact-cue cache uses so the two paths get the same memory
/// envelope.
#[derive(Resource)]
pub struct BlobAudioCache {
    pub by_source: HashMap<AudioReferenceKey, AudioReferenceEntry>,
    insert_order: VecDeque<AudioReferenceKey>,
    /// `Time::elapsed_secs_f64` as of the last poll tick - the request
    /// path's clock for the backoff question. Stamped through
    /// `bypass_change_detection`; see [`super::image_cache::BlobImageCache`]
    /// for why the clock lives on the cache rather than in a parameter.
    now: f64,
    /// The viewer's `LocalSettings::load_external_assets`, stamped by
    /// `asset_failure::stamp_asset_policy` (#1248 f298).
    allow_external: bool,
}

/// Manual for the same reason as [`super::image_cache::BlobImageCache`]'s:
/// `allow_external` must start TRUE (#1248 f298).
impl Default for BlobAudioCache {
    fn default() -> Self {
        Self {
            by_source: HashMap::new(),
            insert_order: VecDeque::new(),
            now: 0.0,
            allow_external: true,
        }
    }
}

impl BlobAudioCache {
    pub fn clear(&mut self) {
        self.by_source.clear();
        self.insert_order.clear();
    }

    /// Insert `entry` for `key`, evicting the oldest if at capacity.
    /// Replacing an existing key (`Pending → Ready`) preserves the
    /// entry's FIFO position - same contract as `BlobImageCache`.
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

    /// Publish the current elapsed time for the request path's backoff
    /// question.
    pub fn stamp_now(&mut self, now: f64) {
        self.now = now;
    }

    /// Publish the viewer's external-asset preference.
    pub fn stamp_external(&mut self, allow: bool) {
        self.allow_external = allow;
    }

    /// Whether this cache will follow a bare web address right now.
    pub fn allows_external(&self) -> bool {
        self.allow_external
    }

    /// How this reference stands right now.
    pub fn status(&self, key: &AudioReferenceKey) -> Option<AssetStatus> {
        match self.by_source.get(key)? {
            AudioReferenceEntry::Pending(_) => Some(AssetStatus::Pending),
            AudioReferenceEntry::Ready(_) => Some(AssetStatus::Ready),
            AudioReferenceEntry::Failed(failure) => Some(AssetStatus::Failed(*failure)),
        }
    }

    /// The clip this reference resolved to, once it has.
    pub fn ready(&self, key: &AudioReferenceKey) -> Option<&Handle<AudioSource>> {
        match self.by_source.get(key)? {
            AudioReferenceEntry::Ready(clip) => Some(clip),
            AudioReferenceEntry::Pending(_) | AudioReferenceEntry::Failed(_) => None,
        }
    }

    /// Drop a failed entry so the next requester starts a fresh attempt.
    pub fn clear_failure(&mut self, key: &AudioReferenceKey) -> bool {
        if matches!(
            self.by_source.get(key),
            Some(AudioReferenceEntry::Failed(_))
        ) {
            self.remove(key);
            return true;
        }
        false
    }
}

/// In-flight audio fetch. Attached to a throwaway entity so the task
/// survives across room rebuilds and despawns on completion.
#[derive(Component)]
pub struct BlobAudioTask {
    pub key: AudioReferenceKey,
    pub task: Task<blob_fetch::FetchedBytes>,
    /// The failure this attempt retries, so the wait keeps doubling.
    pub previous: Option<AssetFailure>,
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
    // The viewer declined to talk to hosts other people chose (#1248 f298).
    // An ambient target still has to publish its absence or the loading gate
    // waits forever - but silently, because nothing failed.
    if key.is_external() && !cache.allow_external {
        if matches!(target, AudioReferenceTarget::AmbientHandle) {
            commands.insert_resource(crate::loading::AmbientHandle(None));
        }
        return;
    }

    let previous = match cache.by_source.get_mut(&key) {
        // Cache hit - dispatch synchronously.
        Some(AudioReferenceEntry::Ready(handle)) => {
            apply_target(commands, &target, handle.clone());
            return;
        }
        // Fetch already in flight - enqueue the target.
        Some(AudioReferenceEntry::Pending(list)) => {
            list.push(target);
            return;
        }
        Some(AudioReferenceEntry::Failed(failure)) => {
            if !failure.may_retry(cache.now) {
                // Still inside the wait. An ambient target must not be
                // left hanging on the loading gate, though - the gate
                // waits on the resource existing, so a failure that is
                // not retried right now still has to publish its absence.
                if matches!(target, AudioReferenceTarget::AmbientHandle) {
                    report_ambient_failure(commands, failure);
                }
                return;
            }
            Some(*failure)
        }
        // First requester.
        None => None,
    };

    cache.insert_bounded(key.clone(), AudioReferenceEntry::Pending(vec![target]));
    let pool = IoTaskPool::get();
    let source_for_task = key.clone();
    let task = pool.spawn(async move {
        let fut = fetch_bytes_for(source_for_task);
        crate::config::http::run_or(fut, Err(AssetFetchError::TimedOut)).await
    });
    commands.spawn(BlobAudioTask {
        key,
        task,
        previous,
    });
}

/// Publish "there is no ambient bed, and here is why" (#1246 f341).
///
/// The loading gate reads [`crate::loading::AmbientHandle`] for its
/// "Ambient soundscape" row and computes the row purely from the resource
/// EXISTING - so `AmbientHandle(None)`, which is what a failed fetch
/// installs, rendered a green check identical to a successful bake and
/// identical to a room that authored no audio at all. The sibling marker
/// is what lets the row tell those apart.
fn report_ambient_failure(commands: &mut Commands, failure: &AssetFailure) {
    commands.insert_resource(crate::loading::AmbientHandle(None));
    commands.insert_resource(crate::loading::AmbientResolveFailed { failure: *failure });
}

/// Drain finished blob-audio fetches: build each clip through
/// [`super::audio_probe::playable_clip`], dispatch the resolved handle to
/// every waiting target, and promote the cache entry to `Ready` for future
/// requesters. Bytes no voice could play fail as an undecodable fetch.
pub fn poll_blob_audio_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut BlobAudioTask)>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut cache: ResMut<BlobAudioCache>,
    time: Res<Time>,
    mut report: super::asset_failure::AssetReport,
) {
    let now = time.elapsed_secs_f64();
    cache.bypass_change_detection().stamp_now(now);
    let mut reporter = report.at(now);
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // Take the pending list while leaving the entry's FIFO slot in
        // place - promotion preserves the original position (matches
        // BlobImageCache's documented contract).
        let pending = match cache.by_source.get_mut(&task.key) {
            Some(AudioReferenceEntry::Pending(list)) => std::mem::take(list),
            Some(AudioReferenceEntry::Ready(_)) => continue, // already promoted
            // Settled by a duplicate task's failure - one source is one
            // attempt, however many tasks raced for it.
            Some(AudioReferenceEntry::Failed(_)) => continue,
            None => continue,
        };

        let clip = match result.and_then(super::audio_probe::playable_clip) {
            Ok(clip) => clip,
            Err(reason) => {
                // The entry SURVIVES the failure (#1246/#1247), carrying the
                // reason and the doubling wait. The pending list is still
                // walked first: an AmbientHandle target must publish its
                // absence or the loading gate waits forever, and now it
                // publishes the REASON alongside. Entity targets get
                // nothing attached (the construct stays silent - preferable
                // to a default fallback hum the room author didn't ask for).
                let failure = AssetFailure::after(task.previous.as_ref(), reason, now);
                for target in pending {
                    if matches!(target, AudioReferenceTarget::AmbientHandle) {
                        report_ambient_failure(&mut commands, &failure);
                    }
                }
                reporter.record(
                    super::asset_failure::AssetClass::Audio,
                    &task.key.describe(),
                    reason,
                );
                cache.insert_bounded(task.key.clone(), AudioReferenceEntry::Failed(failure));
                continue;
            }
        };
        let handle = audio_sources.add(clip);
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
            // The entity may have despawned between dispatch and completion
            // (room rebuild); the voice-budget door inserts with `try_insert`,
            // because a plain `insert` on a missing entity panics through the
            // command error handler.
            super::voice_budget::attach_looping_voice(commands, *entity, handle, *settings);
        }
        AudioReferenceTarget::AmbientHandle => {
            commands.insert_resource(crate::loading::AmbientHandle(Some(handle)));
        }
    }
}

/// Fetch the raw bytes for a key. Routes by variant - URL through
/// HTTPS GET, AtprotoBlob through `getBlob`. Reuses the shared
/// `blob_fetch` module so the OOM-guard and wasm/native split match
/// the image cache exactly.
async fn fetch_bytes_for(key: AudioReferenceKey) -> blob_fetch::FetchedBytes {
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

    /// A resolver with no network: a fetch is injected as an already
    /// finished task, and the poll drains it.
    fn harness() -> App {
        let mut app = App::new();
        // `AssetPlugin` rather than bare `init_asset`: `Assets::add` reaches
        // for the `AssetServer` to mint a handle.
        app.add_plugins((
            bevy::asset::AssetPlugin::default(),
            bevy::app::TaskPoolPlugin::default(),
        ))
        .init_asset::<AudioSource>()
        .init_resource::<BlobAudioCache>()
        .init_resource::<Time>()
        .add_systems(Update, poll_blob_audio_tasks);
        app
    }

    /// A construct entity and the ambient bed both waiting on `key`, whose
    /// fetch has already brought `bytes`; returns the construct.
    fn fetched(app: &mut App, key: &AudioReferenceKey, bytes: Vec<u8>) -> Entity {
        let construct = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<BlobAudioCache>()
            .insert_bounded(
                key.clone(),
                AudioReferenceEntry::Pending(vec![
                    AudioReferenceTarget::AttachToEntity {
                        entity: construct,
                        settings: PlaybackSettings::LOOP,
                    },
                    AudioReferenceTarget::AmbientHandle,
                ]),
            );
        let task =
            IoTaskPool::get_or_init(bevy::tasks::TaskPool::default).spawn(async move { Ok(bytes) });
        app.world_mut().spawn(BlobAudioTask {
            key: key.clone(),
            task,
            previous: None,
        });
        construct
    }

    /// Run frames until every injected fetch has been drained (a single
    /// update races the task pool).
    fn drain(app: &mut App) {
        for _ in 0..1000 {
            let pending = app
                .world_mut()
                .query::<&BlobAudioTask>()
                .iter(app.world())
                .next()
                .is_some();
            if !pending {
                return;
            }
            app.update();
        }
        panic!("a fetch task never resolved");
    }

    /// #1560: a web page served where a sound was promised. It used to
    /// become a clip, and the first voice to play it crashed the client of
    /// whoever came near; now the construct stays silent, the ambient bed
    /// says why it is missing, and the source is remembered as undecodable.
    #[test]
    fn bytes_that_are_not_audio_reach_no_voice_and_say_why() {
        let mut app = harness();
        let key = url_key("https://example.test/hum.ogg");
        let construct = fetched(&mut app, &key, b"<!doctype html><p>Not found</p>".to_vec());
        drain(&mut app);

        let cache = app.world().resource::<BlobAudioCache>();
        let status = cache.status(&key).expect("the source is remembered");
        let failure = status.failure().expect("as failed");
        assert_eq!(failure.reason, AssetFetchError::Undecodable);
        assert!(
            app.world()
                .get::<super::super::voice_budget::LoopingVoice>(construct)
                .is_none(),
            "no voice may be given bytes that are not audio"
        );
        assert_eq!(app.world().resource::<Assets<AudioSource>>().len(), 0);
        assert!(
            app.world()
                .resource::<crate::loading::AmbientHandle>()
                .0
                .is_none()
        );
        assert_eq!(
            app.world()
                .resource::<crate::loading::AmbientResolveFailed>()
                .failure
                .reason,
            AssetFetchError::Undecodable
        );
    }

    /// The control: a real clip reaches both of its targets.
    #[test]
    fn a_clip_reaches_its_voice_and_the_ambient_bed() {
        let mut app = harness();
        let key = url_key("https://example.test/hum.wav");
        let mut wav = Vec::new();
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 22_050,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer =
                hound::WavWriter::new(std::io::Cursor::new(&mut wav), spec).expect("a WAV writer");
            for s in [0i16, 900, -900, 0] {
                writer.write_sample(s).expect("a sample");
            }
            writer.finalize().expect("a finished WAV");
        }
        let construct = fetched(&mut app, &key, wav);
        drain(&mut app);

        let cache = app.world().resource::<BlobAudioCache>();
        let ready = cache.ready(&key).expect("the clip is ready").clone();
        let voice = app
            .world()
            .get::<super::super::voice_budget::LoopingVoice>(construct)
            .expect("the construct has its voice");
        assert_eq!(voice.clip, ready);
        assert_eq!(
            app.world().resource::<crate::loading::AmbientHandle>().0,
            Some(ready)
        );
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
