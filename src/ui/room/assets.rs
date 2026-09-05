//! The asset status line: what happened to the image, sound or terrain
//! layer this field names (#1246, #1247).
//!
//! Every fetched asset in a room was silent about its own failure. A Sign
//! that is downloading, one whose host answered 404, one refused for being
//! too big and one that was never configured were the same brown plane; a
//! `Referenced` terrain layer that failed rendered a convincing procedural
//! ground, so the owner could not tell whether their texture had loaded at
//! all. The owner is the only person who can fix a broken source, and on the
//! web build they were strictly worse informed than a visitor with a console
//! open — every failure was a `warn!` and nothing else.
//!
//! One row closes all of it, because the four caches agree on one answer
//! ([`AssetStatus`]) and one sentence
//! ([`crate::world_builder::asset_failure::AssetFailure::status_line`]). The
//! surfaces differ only in which cache they ask.

use bevy_egui::egui;

use crate::interaction::audio::{AudioClipCache, AudioClipKey};
use crate::pds::{AudioClipSource, SignSource, SovereignAssetReference};
use crate::terrain::referenced::ReferencedLayerStatus;
use crate::world_builder::asset_failure::{AssetRetry, AssetRetryRequests, AssetStatus};
use crate::world_builder::audio_resolver::{AudioReferenceKey, BlobAudioCache};
use crate::world_builder::image_cache::{
    BlobImageCache, BlobImageKey, SamplerFilter, SignSourceKey,
};

/// The four asset caches plus the retry channel, as one `SystemParam`.
///
/// Bundled because both editors that draw asset fields — the room editor
/// and the avatar editor, which shares the whole generator tree — are at or
/// near Bevy's 16-parameter ceiling, and because "which caches answer the
/// asset question" is one fact that should not be spelled twice.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AssetCaches<'w> {
    images: bevy::prelude::Res<'w, BlobImageCache>,
    audio: bevy::prelude::Res<'w, BlobAudioCache>,
    clips: bevy::prelude::Res<'w, AudioClipCache>,
    layers: bevy::prelude::Res<'w, ReferencedLayerStatus>,
    retries: bevy::prelude::ResMut<'w, AssetRetryRequests>,
    settings: bevy::prelude::Res<'w, crate::state::LocalSettings>,
}

impl AssetCaches<'_> {
    /// The borrowed view the editors thread down.
    pub fn panel(&mut self, now: f64) -> AssetPanel<'_> {
        AssetPanel {
            images: &self.images,
            audio: &self.audio,
            clips: &self.clips,
            layers: &self.layers,
            retries: &mut self.retries,
            now,
            allow_external: self.settings.load_external_assets,
        }
    }
}

/// The four asset caches plus the retry channel, as one thing to thread
/// through the editors.
///
/// Threaded as a parameter rather than published as a resource snapshot
/// because the editors already take this shape — `grammar_diag` crosses the
/// same boundary the same way — and because a per-frame snapshot of four
/// caches would be work done for the frames nobody has the panel open.
pub struct AssetPanel<'a> {
    pub images: &'a BlobImageCache,
    pub audio: &'a BlobAudioCache,
    pub clips: &'a AudioClipCache,
    pub layers: &'a ReferencedLayerStatus,
    pub retries: &'a mut AssetRetryRequests,
    /// `Time::elapsed_secs_f64`, for the "trying again in N s" countdown.
    pub now: f64,
    /// The viewer's `load_external_assets` preference (#1248 f298). Read
    /// from the settings rather than from a cache, so a field explains the
    /// silence even before anything has tried to fetch it.
    pub allow_external: bool,
}

impl AssetPanel<'_> {
    /// How a Sign / particle / portal image source stands. `filter` matters:
    /// the same URL at two sampler filters is two GPU images and therefore
    /// two cache entries, and asking with the wrong one would report on an
    /// image this field is not showing.
    pub(crate) fn sign_image(
        &self,
        source: &SignSource,
        filter: SamplerFilter,
    ) -> Option<AssetStatus> {
        let key = BlobImageKey {
            source: SignSourceKey::from_source(source)?,
            filter,
        };
        if self.blocked(key.source.is_external()) {
            return Some(AssetStatus::Blocked);
        }
        self.images.status(&key)
    }

    /// Whether an external source is refused by the viewer's own setting.
    fn blocked(&self, external: bool) -> bool {
        external && !self.allow_external
    }

    /// How a `Referenced` material texture stands. Material textures are
    /// always requested at the default filter.
    pub(crate) fn texture_reference(
        &self,
        reference: &SovereignAssetReference,
    ) -> Option<AssetStatus> {
        let source = SignSourceKey::from_source(&reference_as_sign_source(reference)?)?;
        if self.blocked(source.is_external()) {
            return Some(AssetStatus::Blocked);
        }
        self.images.status(&BlobImageKey {
            source,
            filter: SamplerFilter::Linear,
        })
    }

    /// How a `Referenced` audio source (ambient bed, construct hum) stands.
    pub(crate) fn audio_reference(
        &self,
        reference: &SovereignAssetReference,
    ) -> Option<AssetStatus> {
        let key = AudioReferenceKey::from_reference(reference)?;
        if self.blocked(key.is_external()) {
            return Some(AssetStatus::Blocked);
        }
        self.audio.status(&key)
    }

    /// How a contact-cue clip stands.
    pub(crate) fn contact_clip(&self, source: &AudioClipSource) -> Option<AssetStatus> {
        let key = AudioClipKey::from_source(source)?;
        if self.blocked(key.is_external()) {
            return Some(AssetStatus::Blocked);
        }
        self.clips.status(&key)
    }

    /// How terrain splat layer `idx` stands, for the reference it currently
    /// names.
    pub(crate) fn terrain_layer(
        &self,
        idx: usize,
        reference: &SovereignAssetReference,
    ) -> Option<AssetStatus> {
        if self.blocked(matches!(reference, SovereignAssetReference::Url { .. })) {
            return Some(AssetStatus::Blocked);
        }
        self.layers.status(idx, reference)
    }

    /// Ask for one entry to be dropped so the next request re-attempts it.
    pub(crate) fn retry(&mut self, retry: AssetRetry) {
        self.retries.push(retry);
    }

    /// The retry for a Sign source, if it resolves to a fetchable key.
    pub(crate) fn sign_retry(source: &SignSource, filter: SamplerFilter) -> Option<AssetRetry> {
        Some(AssetRetry::Image(BlobImageKey {
            source: SignSourceKey::from_source(source)?,
            filter,
        }))
    }

    /// The retry for a `Referenced` texture.
    pub(crate) fn texture_retry(reference: &SovereignAssetReference) -> Option<AssetRetry> {
        Some(AssetRetry::Image(BlobImageKey {
            source: SignSourceKey::from_source(&reference_as_sign_source(reference)?)?,
            filter: SamplerFilter::Linear,
        }))
    }

    /// The retry for a `Referenced` audio source.
    pub(crate) fn audio_retry(reference: &SovereignAssetReference) -> Option<AssetRetry> {
        Some(AssetRetry::AudioReference(
            AudioReferenceKey::from_reference(reference)?,
        ))
    }

    /// The retry for a contact cue clip.
    pub(crate) fn clip_retry(source: &AudioClipSource) -> Option<AssetRetry> {
        Some(AssetRetry::ContactClip(AudioClipKey::from_source(source)?))
    }
}

/// A `SovereignAssetReference` as the `SignSource` the image cache keys on.
///
/// The two enums are the same three variants under two names — the image
/// cache was written for Sign panels and the reference type came later — and
/// `request_blob_image` is reached from the material path through exactly
/// this conversion. Asking the cache anything about a reference means
/// speaking its key's language.
fn reference_as_sign_source(reference: &SovereignAssetReference) -> Option<SignSource> {
    Some(match reference {
        SovereignAssetReference::Url { url } => SignSource::Url { url: url.clone() },
        SovereignAssetReference::AtprotoBlob { did, cid } => SignSource::AtprotoBlob {
            did: did.clone(),
            cid: cid.clone(),
        },
        SovereignAssetReference::DidPfp { did } => SignSource::DidPfp { did: did.clone() },
        SovereignAssetReference::Unknown => return None,
    })
}

/// Draw the status line for one asset field. Returns `true` when the owner
/// clicked "Retry now".
///
/// `None` draws nothing: the source is empty or is a variant that resolves
/// to no fetch, and a status line about an asset nobody asked for would be
/// noise on every freshly-added Sign.
///
/// `Ready` draws a line too, and deliberately. "Did my image load?" is the
/// question the panel exists to answer, and answering it only when the
/// answer is bad leaves the good case indistinguishable from a surface that
/// has not been wired up.
pub(crate) fn asset_status_row(ui: &mut egui::Ui, status: Option<AssetStatus>, now: f64) -> bool {
    let theme = crate::ui::theme::current(ui.ctx());
    let mut retry = false;
    match status {
        None => {}
        Some(AssetStatus::Pending) => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    egui::RichText::new("Loading…")
                        .small()
                        .color(theme.text_weak),
                );
            });
        }
        Some(AssetStatus::Ready) => {
            ui.label(
                egui::RichText::new(format!("{} Loaded", crate::ui::affordances::CHECK))
                    .small()
                    .color(theme.status.ok),
            );
        }
        Some(AssetStatus::Blocked) => {
            // Not a failure and not a retry: nothing was attempted, and the
            // remedy is a setting rather than a host (#1248 f298).
            ui.label(
                egui::RichText::new(
                    "Not loaded — \"Load images and sounds from outside Bluesky\" \
                     is off in Settings.",
                )
                .small()
                .color(theme.text_weak),
            );
        }
        Some(AssetStatus::Failed(failure)) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(failure.status_line(now))
                        .small()
                        .color(theme.status.warn),
                );
                // The manual escape a settled failure needs: nothing will
                // retry a 404 on its own, and the owner is the one who can
                // fix the host it came from.
                retry = ui
                    .small_button("Retry now")
                    .on_hover_text("Try this source again from scratch.")
                    .clicked();
            });
        }
    }
    retry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_builder::asset_failure::{AssetFailure, AssetFetchError};

    /// The conversion the whole reference-status path rests on: every
    /// fetchable reference variant has to reach the same cache key the
    /// world-builder's own request used, or the editor would report
    /// "nothing here" over a source that really did fail.
    #[test]
    fn every_fetchable_reference_variant_maps_to_an_image_key() {
        let cases = [
            SovereignAssetReference::Url {
                url: "https://example.org/a.png".into(),
            },
            SovereignAssetReference::AtprotoBlob {
                did: "did:plc:abc".into(),
                cid: "bafy".into(),
            },
            SovereignAssetReference::DidPfp {
                did: "did:plc:abc".into(),
            },
        ];
        for reference in cases {
            let source = reference_as_sign_source(&reference).expect("fetchable variant");
            assert!(
                SignSourceKey::from_source(&source).is_some(),
                "{reference:?} must resolve to a cache key"
            );
            assert!(
                AssetPanel::texture_retry(&reference).is_some(),
                "{reference:?} must have a retry"
            );
        }
        // The forward-compat variant has nothing to fetch and therefore
        // nothing to retry.
        assert!(reference_as_sign_source(&SovereignAssetReference::Unknown).is_none());
        assert!(AssetPanel::texture_retry(&SovereignAssetReference::Unknown).is_none());
    }

    /// An empty field is not a failure. A Sign the owner has just added has
    /// an empty URL, and a status line on it would be the editor shouting
    /// about a source nobody has typed yet.
    #[test]
    fn an_empty_source_has_no_status_and_no_retry() {
        assert!(
            AssetPanel::sign_retry(
                &SignSource::Url { url: String::new() },
                SamplerFilter::Linear
            )
            .is_none()
        );
        assert!(AssetPanel::sign_retry(&SignSource::Unknown, SamplerFilter::Linear).is_none());
    }

    /// The two filters are two entries, so the retry a Nearest field raises
    /// must not drop the Linear entry (and vice versa) — the sequence: a
    /// pixel-art sign and a photo sign pointing at the same URL.
    #[test]
    fn the_sampler_filter_is_part_of_the_retry_identity() {
        let source = SignSource::Url {
            url: "https://example.org/a.png".into(),
        };
        let linear = AssetPanel::sign_retry(&source, SamplerFilter::Linear);
        let nearest = AssetPanel::sign_retry(&source, SamplerFilter::Nearest);
        assert!(linear.is_some() && nearest.is_some());
        assert_ne!(
            linear, nearest,
            "one filter's retry must not clear the other's entry"
        );
    }

    /// #1248 f298: with the preference off, every URL-shaped source in
    /// every editor says so — and the ATProto ones, which stay inside the
    /// infrastructure the session already talks to, keep working.
    #[test]
    fn the_external_preference_blocks_url_sources_and_only_url_sources() {
        let images = BlobImageCache::default();
        let audio = BlobAudioCache::default();
        let clips = AudioClipCache::default();
        let layers = ReferencedLayerStatus::default();
        let mut retries = AssetRetryRequests::default();
        let mut panel = AssetPanel {
            images: &images,
            audio: &audio,
            clips: &clips,
            layers: &layers,
            retries: &mut retries,
            now: 0.0,
            allow_external: false,
        };

        let url = SignSource::Url {
            url: "https://cdn.example.org/a.png".into(),
        };
        let blob = SignSource::AtprotoBlob {
            did: "did:plc:abc".into(),
            cid: "bafy".into(),
        };
        let pfp = SignSource::DidPfp {
            did: "did:plc:abc".into(),
        };
        assert!(matches!(
            panel.sign_image(&url, SamplerFilter::Linear),
            Some(AssetStatus::Blocked)
        ));
        assert!(
            panel.sign_image(&blob, SamplerFilter::Linear).is_none(),
            "an ATProto blob is not blocked — nothing has asked for it yet"
        );
        assert!(panel.sign_image(&pfp, SamplerFilter::Linear).is_none());

        assert!(matches!(
            panel.contact_clip(&AudioClipSource::Url {
                url: "https://cdn.example.org/step.ogg".into()
            }),
            Some(AssetStatus::Blocked)
        ));
        assert!(matches!(
            panel.texture_reference(&SovereignAssetReference::Url {
                url: "https://cdn.example.org/a.png".into()
            }),
            Some(AssetStatus::Blocked)
        ));
        assert!(matches!(
            panel.terrain_layer(
                0,
                &SovereignAssetReference::Url {
                    url: "https://cdn.example.org/a.png".into()
                }
            ),
            Some(AssetStatus::Blocked)
        ));

        // With the preference on, the same sources are back to "nothing has
        // asked yet" rather than blocked.
        panel.allow_external = true;
        assert!(panel.sign_image(&url, SamplerFilter::Linear).is_none());
    }

    /// A failure the editor can read is the whole point; check the row's
    /// input rather than egui's output, which has no test harness here.
    #[test]
    fn a_settled_failure_reads_as_settled() {
        let failure = AssetFailure::after(None, AssetFetchError::HttpStatus(404), 0.0);
        let status = AssetStatus::Failed(failure);
        let text = status.failure().expect("failed").status_line(0.0);
        assert!(text.contains("404"), "{text}");
        assert!(text.contains("will not be retried"), "{text}");
    }
}
