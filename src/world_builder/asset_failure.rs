//! One vocabulary for "the asset did not arrive", shared by every
//! network-fetched asset a room can name (#1246, #1247).
//!
//! Before this module every asset fetch failed the same way: the shared
//! [`blob_fetch`](super::blob_fetch) helpers logged a `warn!` and returned
//! `None`, and each cache then *removed* the entry so the next requester
//! would spawn the whole fetch again. That is two defects wearing one coat.
//! The caller could not say what went wrong — a sign whose image is still
//! downloading, whose host answered 404, and which was never configured are
//! the same brown plane — and it could not say *not to ask again yet*, so a
//! dead URL was re-requested for as long as anything kept asking. For a
//! contact audio cue driven by dwell, "anything kept asking" is once per
//! frame, aimed at a host named in somebody else's record.
//!
//! Both halves are one state: a cache entry that SURVIVES failure, carrying
//! why it failed and when it may be tried again. [`AssetFailure`] is that
//! entry's payload, and it is the only thing the four asset caches
//! ([`BlobImageCache`](super::image_cache::BlobImageCache),
//! [`AudioClipCache`](crate::interaction::audio::AudioClipCache),
//! [`BlobAudioCache`](super::audio_resolver::BlobAudioCache) and the terrain
//! splat layers) need to agree on.
//!
//! **The waiting arithmetic is not new here.** [`RetryBackoff`] is the
//! doubling #1113 shipped for the wardrobe fan-out and #1217 generalised for
//! the peer-side fetches; asset fetches wait on the same shape with their own
//! two constants. A fifth hand-rolled backoff would have been a fifth set of
//! numbers to keep in step.
//!
//! Everything in this module is pure. The sentences it produces are drawn
//! verbatim by the room editor, so this file is named in
//! `ui::fonts::EXTRA_LABEL_SOURCES` and its literals are walked by the glyph
//! guard.

use crate::config;
use crate::network::presence::RetryBackoff;

/// Why an asset fetch did not produce usable bytes.
///
/// One variant per place the shared fetch/decode path can give up, because
/// the whole point of the type is that the owner — the only person who can
/// fix a broken source — is told which one happened. `String` payloads are
/// deliberately absent: every sentence is built from the variant and its
/// numbers, so a reason cannot smuggle a host's error text onto a UI label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetFetchError {
    /// The request never got an answer: DNS, TLS, a refused connection — or,
    /// on the web build, the host serving no `Access-Control-Allow-Origin`,
    /// which is by far the most common cause and is indistinguishable from
    /// the others at this layer.
    Unreachable,
    /// The host answered, with a status that is not a success.
    HttpStatus(u16),
    /// The body was larger than the caller's cap, either by its advertised
    /// length or measured mid-stream.
    TooLarge { limit: usize },
    /// The connection dropped part-way through the body.
    ReadFailed,
    /// An `AtprotoBlob` source whose DID does not resolve to a PDS.
    NoStorage,
    /// A `DidPfp` source for an account with no profile picture set.
    NoPicture,
    /// Bytes arrived and are not a file this client can decode.
    Undecodable,
    /// An image whose declared frame is past the decode caps — refused
    /// before the full-frame allocation, so the numbers are the header's.
    ImageTooBig { width: u32, height: u32 },
    /// The per-request bound in [`config::http::run_or`] elapsed first.
    /// Web only: on native the client's own builder timeout produces a
    /// transport error instead.
    TimedOut,
}

impl AssetFetchError {
    /// Whether retrying can never help.
    ///
    /// The direction to be wrong in is "retryable" — a bounded doubling
    /// against a host that is briefly down costs a request a minute, while
    /// giving up permanently on a blip means the owner's image never appears
    /// and nothing says why. So only answers that describe the *file* (it is
    /// too big, it is not decodable, the account has no picture) and the
    /// client-error half of the status codes are permanent.
    ///
    /// `408`, `425` and `429` are client-class codes that explicitly mean
    /// "later", so they stay retryable.
    pub fn permanent(&self) -> bool {
        match self {
            Self::HttpStatus(code) => (400..500).contains(code) && !matches!(code, 408 | 425 | 429),
            Self::TooLarge { .. }
            | Self::NoPicture
            | Self::Undecodable
            | Self::ImageTooBig { .. } => true,
            Self::Unreachable | Self::ReadFailed | Self::NoStorage | Self::TimedOut => false,
        }
    }

    /// The clause a status line prints after "Could not load — ".
    ///
    /// Plain language on purpose: this is drawn in the room editor beside
    /// the field the owner typed, not in a log. `Unreachable` names the web
    /// cause because it is the likeliest one and the only one the owner can
    /// act on without leaving the app.
    pub fn sentence(&self) -> String {
        match self {
            Self::Unreachable => "could not reach the host. On the web this is usually the host \
                 not allowing other sites to load its files."
                .to_string(),
            Self::HttpStatus(code) => format!("the host answered with status {code}."),
            Self::TooLarge { limit } => format!(
                "the file is bigger than the {} limit.",
                crate::pds::record_size::human_bytes(*limit)
            ),
            Self::ReadFailed => "the download stopped part-way.".to_string(),
            Self::NoStorage => "that account's storage could not be found.".to_string(),
            Self::NoPicture => "that account has no profile picture.".to_string(),
            Self::Undecodable => "the file is not in a format this client can read.".to_string(),
            Self::ImageTooBig { width, height } => format!(
                "the image is {width}×{height}, past the {}×{} limit.",
                super::blob_fetch::MAX_IMAGE_AXIS,
                super::blob_fetch::MAX_IMAGE_AXIS
            ),
            Self::TimedOut => config::http::timed_out("the fetch") + ".",
        }
    }

    /// A short tag for logs and diagnostic events — the same information
    /// without the sentence, so a captured session log stays greppable.
    pub fn tag(&self) -> String {
        match self {
            Self::Unreachable => "unreachable".to_string(),
            Self::HttpStatus(code) => format!("status_{code}"),
            Self::TooLarge { .. } => "too_large".to_string(),
            Self::ReadFailed => "read_failed".to_string(),
            Self::NoStorage => "no_storage".to_string(),
            Self::NoPicture => "no_picture".to_string(),
            Self::Undecodable => "undecodable".to_string(),
            Self::ImageTooBig { width, height } => format!("image_{width}x{height}"),
            Self::TimedOut => "timed_out".to_string(),
        }
    }
}

/// How many times in a row a source may fail before the client stops asking
/// until something clears the entry (a room recompile, a logout, or the
/// owner's own "Retry now").
///
/// At the doubling below that is 2 + 4 + 8 + 16 + 32 + 60 s ≈ two minutes of
/// trying, which comfortably outlasts a host restart and stops well short of
/// a client that spends a whole session knocking on a door nobody answers
/// (#1247 f309 wanted exactly this ceiling for the contact-cue path).
pub const GIVE_UP_ATTEMPTS: u32 = 6;

/// A failed asset fetch: why, and when it may be tried again.
///
/// Held IN the cache entry rather than beside it, because the bug this
/// replaces was the entry being removed — anything kept in a sibling map
/// would have had to be swept in step with a cache that FIFO-evicts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssetFailure {
    pub reason: AssetFetchError,
    /// Attempt count and the wait, on the shared doubling
    /// ([`RetryBackoff`]).
    pub backoff: RetryBackoff,
}

impl AssetFailure {
    /// Record a failure, doubling `previous`'s wait when this source has
    /// failed before.
    pub fn after(previous: Option<&Self>, reason: AssetFetchError, now: f64) -> Self {
        Self {
            reason,
            backoff: RetryBackoff::after_failure_with(
                previous.map(|f| &f.backoff),
                now,
                config::asset::FETCH_RETRY_BASE_SECS,
                config::asset::FETCH_RETRY_MAX_SECS,
            ),
        }
    }

    /// Whether the client has stopped asking: the reason can never change,
    /// or the attempt ceiling is spent.
    pub fn settled(&self) -> bool {
        self.reason.permanent() || self.backoff.attempts >= GIVE_UP_ATTEMPTS
    }

    /// Whether a fresh attempt is due at `now`. `false` for a settled
    /// failure however long it is left alone.
    pub fn may_retry(&self, now: f64) -> bool {
        !self.settled() && self.backoff.ready(now)
    }

    /// Seconds until the next attempt, `None` once settled.
    pub fn retry_in(&self, now: f64) -> Option<f64> {
        if self.settled() {
            return None;
        }
        Some((self.backoff.failed_at + self.backoff.wait_secs - now).max(0.0))
    }

    /// The whole status line: what went wrong, and what the client will do
    /// about it. This is the sentence every asset surface prints, so a sign,
    /// a terrain layer and a contact cue cannot describe the same failure
    /// three different ways.
    pub fn status_line(&self, now: f64) -> String {
        let mut line = format!("Could not load — {}", self.reason.sentence());
        match self.retry_in(now) {
            Some(secs) if secs >= 1.0 => {
                line.push_str(&format!(" Trying again in {}s.", secs.ceil() as u64));
            }
            Some(_) => line.push_str(" Trying again."),
            None => line.push_str(" It will not be retried on its own."),
        }
        line
    }
}

// ---------------------------------------------------------------------------
// What this client can actually load
// ---------------------------------------------------------------------------

/// The contract for an image source, in one line, built from the constants
/// that enforce it (#1248 f350).
///
/// A grep of `src/ui` for any of these numbers or format names used to find
/// nothing at all, so an owner who pointed a sign at a 20 MB photograph or a
/// GIF got a blank with no way to learn a GIF was never going to work. Built
/// rather than hard-coded so a cap that moves moves here too.
pub fn image_source_caps() -> String {
    // The pixel budget as the square it was derived from, which is how the
    // limit reads to a person choosing a photo.
    let edge = (super::blob_fetch::MAX_IMAGE_PIXELS as f64).sqrt() as u32;
    format!(
        "PNG, JPEG or WebP · up to {} · at most {edge}×{edge}",
        crate::pds::record_size::human_bytes(super::image_cache::MAX_IMAGE_BYTES),
    )
}

/// The same for an audio clip. Bevy's default audio feature is `vorbis`, so
/// an MP3 is worse than a blank: the fetch succeeds, the entry is promoted to
/// Ready, and the cue is cached, plays nothing, and never retries.
pub fn audio_clip_caps() -> String {
    format!(
        "Ogg/Vorbis · up to {}",
        crate::pds::record_size::human_bytes(config::interaction::audio::MAX_CLIP_BYTES),
    )
}

/// Which asset path a failure came from — the metric it counts against and
/// the word the session log prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetClass {
    /// Sign panels, referenced material textures, particle sprites, portal
    /// profile pictures.
    Image,
    /// Ambient beds, construct audio and contact cues.
    Audio,
    /// A `Referenced` terrain splat layer.
    TerrainLayer,
}

impl AssetClass {
    /// The word the session log and the anomaly verdict use.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::TerrainLayer => "terrain layer",
        }
    }
}

/// The one place an asset failure becomes a diagnostic signal (#1246 f353).
///
/// Before this, a grep for `metrics` or `diagnostics` across `image_cache`,
/// `audio_resolver`, `blob_fetch`, `terrain::referenced` and
/// `interaction::audio` returned nothing: the Diagnostics HUD and the
/// anomaly engine — the app's designated "something is wrong" channel — were
/// blind to the entire surface that fails silently by construction and
/// depends on third-party hosts.
///
/// Both resources are optional because the fetch systems are exercised by
/// unit tests that build a two-plugin `App`, and a counter is not worth a
/// test harness knowing about the diagnostics suite.
pub struct FailureReporter<'a> {
    metrics: Option<&'a mut crate::diagnostics::MetricsRegistry>,
    log: Option<&'a mut crate::diagnostics::SessionLog>,
    now: f64,
}

/// The two optional diagnostics resources, as one `SystemParam`.
///
/// Every asset poll system takes them, and every one of them was already at
/// or over clippy's seven-argument bound — bundling is what keeps "report
/// the failure" from costing two parameters at four call sites.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AssetReport<'w> {
    metrics: Option<bevy::prelude::ResMut<'w, crate::diagnostics::MetricsRegistry>>,
    log: Option<bevy::prelude::ResMut<'w, crate::diagnostics::SessionLog>>,
}

impl AssetReport<'_> {
    /// A reporter stamped with this tick's clock.
    pub fn at(&mut self, now: f64) -> FailureReporter<'_> {
        FailureReporter {
            metrics: self.metrics.as_deref_mut(),
            log: self.log.as_deref_mut(),
            now,
        }
    }
}

impl FailureReporter<'_> {
    /// Publish the image cache's decoded-byte total as a gauge.
    pub fn gauge_image_cache_bytes(&mut self, bytes: usize) {
        if let Some(metrics) = self.metrics.as_deref_mut() {
            crate::diagnostics::samplers::asset_image_cache_bytes(metrics, bytes);
        }
    }

    /// Count one failure and record it in the session log.
    ///
    /// `source` is the human-readable identity of what failed (a URL, a
    /// `did:…/cid`, a layer index), truncated so a pathological record
    /// cannot bloat the ring.
    pub fn record(&mut self, class: AssetClass, source: &str, reason: AssetFetchError) {
        if let Some(metrics) = self.metrics.as_deref_mut() {
            crate::diagnostics::samplers::asset_fetch_failed(metrics, class);
        }
        if let Some(log) = self.log.as_deref_mut() {
            log.warn(
                self.now,
                crate::diagnostics::event::EventPayload::AssetFetchFailed {
                    asset: class.label().to_string(),
                    source: elide_source(source),
                    reason: reason.tag(),
                },
            );
        }
    }
}

/// Keep a logged source identity to a length a ring buffer can carry. A
/// record may name a URL of any length the sanitiser allows.
fn elide_source(source: &str) -> String {
    const MAX: usize = 120;
    if source.chars().count() <= MAX {
        return source.to_string();
    }
    let head: String = source.chars().take(MAX).collect();
    format!("{head}…")
}

/// How one asset stands right now, as every cache answers it and every
/// surface asks it (#1246).
///
/// `Copy` and owning rather than borrowing: the editor asks a cache for a
/// status and then, on the same row, may ask to CLEAR it, and a borrow held
/// across those two would be a borrow-checker fight for no gain —
/// [`AssetFailure`] is three words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AssetStatus {
    /// A fetch is in flight.
    Pending,
    /// The bytes arrived and are in use.
    Ready,
    /// The fetch gave up; the payload says why and what happens next.
    Failed(AssetFailure),
    /// Never attempted: the source names a web address and the viewer has
    /// "Load images and sounds from outside Bluesky" switched off (#1248
    /// f298). Not a failure — a choice, and the only status with a
    /// remedy that is not "fix the host".
    Blocked,
}

impl AssetStatus {
    /// The failure, if this is one.
    pub fn failure(&self) -> Option<&AssetFailure> {
        match self {
            Self::Failed(failure) => Some(failure),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The manual escape
// ---------------------------------------------------------------------------

/// One "Retry now" click, naming the entry to drop (#1247 f346).
///
/// A settled failure never retries on its own — a 404 will still be a 404 in
/// an hour — so the owner needs a way to say "I have fixed the host". The
/// whole of the retry is dropping the cache entry: the next requester then
/// takes the miss arm exactly as it did the first time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetRetry {
    Image(super::image_cache::BlobImageKey),
    AudioReference(super::audio_resolver::AudioReferenceKey),
    ContactClip(crate::interaction::audio::AudioClipKey),
    TerrainLayer(usize),
}

/// Retries raised by the editor this frame, drained by
/// [`apply_asset_retries`].
///
/// A queue rather than a direct mutation because the room editor holds the
/// caches by shared reference — it reads a dozen statuses per frame and must
/// not take `ResMut` on four caches to service a button that is usually not
/// clicked.
#[derive(bevy::prelude::Resource, Default)]
pub struct AssetRetryRequests(Vec<AssetRetry>);

impl AssetRetryRequests {
    pub fn push(&mut self, retry: AssetRetry) {
        self.0.push(retry);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Publish the viewer's external-asset preference into the two caches the
/// world compiler reaches through (#1248 f298).
///
/// A system of its own rather than two more parameters on the poll systems,
/// which are already at clippy's bound — and the answer has to be somewhere
/// the request path can read it, because `request_blob_image_filtered` and
/// `request_blob_audio` are called from inside the unit builders, which are
/// handed a context struct and not a `SystemParam` list.
///
/// Written through `bypass_change_detection`: a preference re-stamped every
/// frame would mark both caches changed every frame and defeat every
/// `Changed<...>` reader (#879's guarded-dirty rule).
pub fn stamp_asset_policy(
    settings: bevy::prelude::Res<crate::state::LocalSettings>,
    mut images: bevy::prelude::ResMut<super::image_cache::BlobImageCache>,
    mut audio: bevy::prelude::ResMut<super::audio_resolver::BlobAudioCache>,
) {
    use bevy::ecs::change_detection::DetectChangesMut;
    images
        .bypass_change_detection()
        .stamp_external(settings.load_external_assets);
    audio
        .bypass_change_detection()
        .stamp_external(settings.load_external_assets);
}

/// Drain the retry queue: drop each named failure so the next request
/// re-attempts it.
///
/// The terrain layer needs one extra step. Its fetch is dispatched by
/// `start_texture_tasks`, which is a one-shot behind the
/// `TextureTasksStarted` marker — so clearing the layer's failure alone
/// would leave nothing to re-dispatch it. Removing the marker is what the
/// terrain lifecycle already does for a config change, and it re-bakes the
/// procedural layers alongside; that is more work than the click strictly
/// needs, and it is a deliberate click.
pub fn apply_asset_retries(
    mut commands: bevy::prelude::Commands,
    mut requests: bevy::prelude::ResMut<AssetRetryRequests>,
    mut images: bevy::prelude::ResMut<super::image_cache::BlobImageCache>,
    mut audio: bevy::prelude::ResMut<super::audio_resolver::BlobAudioCache>,
    mut clips: bevy::prelude::ResMut<crate::interaction::audio::AudioClipCache>,
    mut layers: bevy::prelude::ResMut<crate::terrain::referenced::ReferencedLayerStatus>,
) {
    if requests.0.is_empty() {
        return;
    }
    for retry in std::mem::take(&mut requests.0) {
        match retry {
            AssetRetry::Image(key) => {
                images.clear_failure(&key);
            }
            AssetRetry::AudioReference(key) => {
                audio.clear_failure(&key);
            }
            AssetRetry::ContactClip(key) => {
                clips.clear_failure(&key);
            }
            AssetRetry::TerrainLayer(idx) => {
                if layers.clear_failure(idx) {
                    commands.remove_resource::<crate::terrain::TextureTasksStarted>();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sequence: a host that is down comes back up. Each failure waits
    /// twice as long as the last, and the entry never stops being a hit in
    /// between — which is what turns the retry storm into a slow poll.
    #[test]
    fn repeated_failures_double_the_wait_and_stay_hits_between_attempts() {
        let mut now = 100.0;
        let mut failure = AssetFailure::after(None, AssetFetchError::Unreachable, now);
        assert_eq!(failure.backoff.attempts, 1);
        assert_eq!(failure.backoff.wait_secs, 2.0);
        assert!(!failure.may_retry(now + 1.0), "inside the first wait");
        assert!(failure.may_retry(now + 2.0), "the wait elapsed");

        for expected in [4.0, 8.0, 16.0, 32.0] {
            now += failure.backoff.wait_secs;
            failure = AssetFailure::after(Some(&failure), AssetFetchError::Unreachable, now);
            assert_eq!(failure.backoff.wait_secs, expected);
        }
        // The cap holds, and the sixth failure is the last one.
        now += failure.backoff.wait_secs;
        failure = AssetFailure::after(Some(&failure), AssetFetchError::Unreachable, now);
        assert_eq!(
            failure.backoff.wait_secs,
            config::asset::FETCH_RETRY_MAX_SECS
        );
        assert_eq!(failure.backoff.attempts, GIVE_UP_ATTEMPTS);
        assert!(failure.settled(), "the attempt ceiling is spent");
        assert!(!failure.may_retry(now + 10_000.0), "and stays spent");
    }

    /// A 404 must not buy six attempts: the file named is not there and no
    /// amount of asking changes that. A 503 must, because it is the shape a
    /// host restarting takes.
    #[test]
    fn client_errors_settle_immediately_and_server_errors_do_not() {
        let at = 5.0;
        let not_found = AssetFailure::after(None, AssetFetchError::HttpStatus(404), at);
        assert!(not_found.settled());
        assert_eq!(not_found.retry_in(at), None);

        let unavailable = AssetFailure::after(None, AssetFetchError::HttpStatus(503), at);
        assert!(!unavailable.settled());
        assert_eq!(unavailable.retry_in(at), Some(2.0));

        // "Later" codes are client-class and still mean later.
        for code in [408, 425, 429] {
            assert!(
                !AssetFetchError::HttpStatus(code).permanent(),
                "{code} asks to be retried"
            );
        }
    }

    /// #1248 f350: the caps are stated in the UI, and stated FROM the
    /// constants that enforce them. A hard-coded label would be a second
    /// place for the numbers to live, and the whole finding is that a cap
    /// nobody can see is a cap everybody discovers as a blank.
    #[test]
    fn the_caps_labels_are_built_from_the_constants_that_enforce_them() {
        let image = image_source_caps();
        for expected in [
            "PNG",
            "JPEG",
            "WebP",
            &crate::pds::record_size::human_bytes(super::super::image_cache::MAX_IMAGE_BYTES),
            "4096×4096",
        ] {
            assert!(
                image.contains(expected),
                "{expected:?} missing from {image:?}"
            );
        }

        let audio = audio_clip_caps();
        for expected in [
            "Ogg",
            &crate::pds::record_size::human_bytes(config::interaction::audio::MAX_CLIP_BYTES),
        ] {
            assert!(
                audio.contains(expected),
                "{expected:?} missing from {audio:?}"
            );
        }
    }

    /// The files that own an asset fetch, and must therefore report its
    /// failure. Walked by the guard below.
    const ASSET_FETCH_PATHS: &[&str] = &[
        "src/world_builder/image_cache.rs",
        "src/world_builder/audio_resolver.rs",
        "src/terrain/referenced.rs",
        "src/interaction/audio.rs",
    ];

    /// #1246 f353's finding, stated as a property of the code: a grep for
    /// `metrics` or `diagnostics` across the asset fetch paths returned
    /// NOTHING, so the Diagnostics HUD and the anomaly engine were blind to
    /// the one surface that fails silently by construction and depends
    /// entirely on third-party hosts.
    ///
    /// A source walk rather than a behavioural test because the property is
    /// "every path is wired", and a fifth asset cache added later is exactly
    /// the case a behavioural test of the existing four would not catch.
    #[test]
    fn every_asset_fetch_path_reports_its_failures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for path in ASSET_FETCH_PATHS {
            let src = std::fs::read_to_string(root.join(path))
                .unwrap_or_else(|e| panic!("{path} must be readable: {e}"));
            assert!(
                src.contains("AssetReport"),
                "{path} owns an asset fetch and takes no AssetReport — \
                 its failures reach nothing but the console"
            );
            assert!(
                src.contains(".record("),
                "{path} takes an AssetReport and never records through it"
            );
            assert!(
                src.contains("AssetFailure::after"),
                "{path} does not record a failure state, so its next requester \
                 re-issues the fetch immediately (#1247)"
            );
        }
    }

    /// The status line is the whole user-facing contract: it always names
    /// the cause, and it always says whether anything more will happen.
    #[test]
    fn every_status_line_names_a_cause_and_a_disposition() {
        let reasons = [
            AssetFetchError::Unreachable,
            AssetFetchError::HttpStatus(404),
            AssetFetchError::HttpStatus(500),
            AssetFetchError::TooLarge { limit: 1024 },
            AssetFetchError::ReadFailed,
            AssetFetchError::NoStorage,
            AssetFetchError::NoPicture,
            AssetFetchError::Undecodable,
            AssetFetchError::ImageTooBig {
                width: 30_000,
                height: 30_000,
            },
            AssetFetchError::TimedOut,
        ];
        for reason in reasons {
            let failure = AssetFailure::after(None, reason, 0.0);
            let line = failure.status_line(0.0);
            assert!(
                line.starts_with("Could not load — "),
                "{reason:?} produced: {line}"
            );
            assert!(
                line.contains("Trying again") || line.contains("will not be retried"),
                "{reason:?} says nothing about what happens next: {line}"
            );
            assert!(!reason.tag().is_empty(), "{reason:?} has no log tag");
        }
    }
}
