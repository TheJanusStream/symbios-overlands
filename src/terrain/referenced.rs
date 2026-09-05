//! `SovereignTextureConfig::Referenced` splat layers: the URL /
//! ATProto-blob fetch path that overrides a procedural placeholder
//! layer with explicit image bytes once they arrive.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy_symbios_texture::{TextureMap, map_to_images_with_usages};

use crate::pds::SovereignAssetReference;
use crate::world_builder::asset_failure::{
    AssetClass, AssetFailure, AssetFetchError, AssetReport, AssetStatus, FailureReporter,
};
use crate::world_builder::blob_fetch;
use crate::world_builder::image_cache::MAX_IMAGE_BYTES;

use super::TerrainSplatState;

/// Per-layer outcome of the four `Referenced` splat fetches (#1246 f347).
///
/// **Why this resource exists.** A `Referenced` layer that fails does not
/// look failed: `texture_bake_job` maps the variant to a default ground
/// config, so the placeholder under everyone's feet is a plausible dirt
/// surface rather than an obvious blank. The owner had no way to tell "my
/// texture loaded" from "the fallback loaded" — which is worse than a blank,
/// and worse than the Sign's flat tint, because the wrong answer is
/// convincing.
///
/// It doubles as the layer cache. Nothing kept the resolved handles across a
/// terrain regeneration, and every terrain edit removes `TextureTasksStarted`
/// to force one — so a slider flush re-issued up to four HTTP fetches from
/// cold. Holding the handles here (keyed by the source AND the texture size
/// they were decoded at) means an unchanged layer is reused.
#[derive(Resource, Default)]
pub struct ReferencedLayerStatus {
    layers: [Option<LayerFetch>; 4],
}

/// One layer's state. Each variant carries the source it belongs to, so a
/// layer whose reference the owner has just changed is never answered with
/// the previous reference's outcome.
enum LayerFetch {
    Pending {
        source: SovereignAssetReference,
    },
    Ready {
        source: SovereignAssetReference,
        texture_size: u32,
        albedo: Handle<Image>,
        normal: Handle<Image>,
    },
    Failed {
        source: SovereignAssetReference,
        failure: AssetFailure,
    },
}

impl LayerFetch {
    fn source(&self) -> &SovereignAssetReference {
        match self {
            Self::Pending { source } | Self::Ready { source, .. } | Self::Failed { source, .. } => {
                source
            }
        }
    }
}

impl ReferencedLayerStatus {
    /// How layer `idx` stands, when the entry belongs to `source`. Returns
    /// `None` for a layer that is not `Referenced`, has never been asked
    /// for, or whose reference has been edited since.
    pub fn status(&self, idx: usize, source: &SovereignAssetReference) -> Option<AssetStatus> {
        let entry = self.layers.get(idx)?.as_ref()?;
        if entry.source() != source {
            return None;
        }
        Some(match entry {
            LayerFetch::Pending { .. } => AssetStatus::Pending,
            LayerFetch::Ready { .. } => AssetStatus::Ready,
            LayerFetch::Failed { failure, .. } => AssetStatus::Failed(*failure),
        })
    }

    /// Drop a failed layer so the next terrain generation re-fetches it —
    /// the "Retry now" click.
    pub fn clear_failure(&mut self, idx: usize) -> bool {
        if matches!(self.layers.get(idx), Some(Some(LayerFetch::Failed { .. }))) {
            self.layers[idx] = None;
            return true;
        }
        false
    }

    /// Every layer index that is currently failed, for the terrain panel's
    /// summary line.
    pub fn failed_layers(&self) -> impl Iterator<Item = (usize, &AssetFailure)> {
        self.layers
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| match entry {
                Some(LayerFetch::Failed { failure, .. }) => Some((idx, failure)),
                _ => None,
            })
    }
}

/// In-flight per-layer HTTPS / ATProto-blob fetch for a
/// [`crate::pds::SovereignTextureConfig::Referenced`] splat layer. The
/// task returns `Some(raw_bytes)` on success or `None` on a transient /
/// permanent failure (logged at warn-level in [`blob_fetch`]). The poll
/// system decodes the bytes, resizes to `texture_size`, and overrides
/// [`TerrainSplatState::layer_albedo`] for `layer_idx`.
#[derive(Component)]
pub(super) struct PendingSplatLayerFetch {
    layer_idx: usize,
    texture_size: u32,
    task: bevy::tasks::Task<blob_fetch::FetchedBytes>,
    /// The reference this fetch is for, so the poll system can record its
    /// outcome against the source rather than the slot — the owner may have
    /// edited the layer while it was in flight.
    source: SovereignAssetReference,
    /// The failure this attempt retries, so the wait keeps doubling.
    previous: Option<AssetFailure>,
}

/// Spawn an `IoTaskPool` task that fetches the bytes referenced by
/// `source` and parks them on a [`PendingSplatLayerFetch`] component
/// for [`poll_splat_layer_fetches`] to drain.
#[allow(clippy::too_many_arguments)] // one dispatch; each argument is a channel.
pub(super) fn spawn_splat_layer_fetch(
    commands: &mut Commands,
    status: &mut ReferencedLayerStatus,
    state: &mut TerrainSplatState,
    layer_idx: usize,
    source: &SovereignAssetReference,
    texture_size: u32,
    now: f64,
    allow_external: bool,
) {
    // The viewer declined to talk to hosts other people chose (#1248 f298).
    // The procedural placeholder stays, which is what the layer would have
    // shown while the fetch was in flight anyway.
    if matches!(source, SovereignAssetReference::Url { .. }) && !allow_external {
        return;
    }
    // What this layer did last time, if the reference has not been edited
    // since. Three of the four answers mean "do not fetch" (#1246 f347,
    // #1247 f346).
    let previous = match status.layers.get(layer_idx).and_then(|e| e.as_ref()) {
        Some(entry) if entry.source() == source => match entry {
            // Already resolved at this size — reuse the handles rather than
            // re-downloading the image every terrain edit.
            LayerFetch::Ready {
                texture_size: size,
                albedo,
                normal,
                ..
            } if *size == texture_size => {
                state.layer_albedo[layer_idx] = Some(albedo.clone());
                state.layer_normal[layer_idx] = Some(normal.clone());
                state.applied = false;
                return;
            }
            LayerFetch::Ready { .. } => None,
            // A fetch from the previous generation is still in flight; its
            // poll will fill this slot.
            LayerFetch::Pending { .. } => return,
            LayerFetch::Failed { failure, .. } => {
                if !failure.may_retry(now) {
                    return;
                }
                Some(*failure)
            }
        },
        _ => None,
    };

    // Empty / forward-compat / image-pfp variants don't resolve to
    // splat-usable bytes — bail without spawning a task that would
    // just fail.
    let request = match source {
        SovereignAssetReference::Url { url } if !url.is_empty() => {
            SplatFetchRequest::Url(url.clone())
        }
        SovereignAssetReference::AtprotoBlob { did, cid } if !did.is_empty() && !cid.is_empty() => {
            SplatFetchRequest::AtprotoBlob {
                did: did.clone(),
                cid: cid.clone(),
            }
        }
        _ => return,
    };

    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let client = crate::config::http::default_client();
        let fut = async {
            match request {
                SplatFetchRequest::Url(u) => {
                    blob_fetch::fetch_url_bytes(&client, &u, MAX_IMAGE_BYTES, "SplatLayer").await
                }
                SplatFetchRequest::AtprotoBlob { did, cid } => {
                    blob_fetch::fetch_blob_bytes(&client, &did, &cid, MAX_IMAGE_BYTES, "SplatLayer")
                        .await
                }
            }
        };
        crate::config::http::run_or(fut, Err(AssetFetchError::TimedOut)).await
    });
    status.layers[layer_idx] = Some(LayerFetch::Pending {
        source: source.clone(),
    });
    commands.spawn(PendingSplatLayerFetch {
        layer_idx,
        texture_size,
        task,
        source: source.clone(),
        previous,
    });
}

/// Discriminator handed into the spawned task. Owns its data so the
/// task body is `'static + Send`.
enum SplatFetchRequest {
    Url(String),
    AtprotoBlob { did: String, cid: String },
}

/// Drain finished splat-layer fetches: decode the bytes, resize to the
/// room's authored `texture_size`, wrap as a [`TextureMap`] with flat
/// normal + neutral roughness fillers, and upload via
/// [`map_to_images_with_usages`] so the result carries the
/// same mip chain + repeat sampler shape as procedural layers. The
/// resolved albedo handle overrides [`TerrainSplatState::layer_albedo`]
/// for the layer index, and `state.applied` is flipped to `false` so
/// the next `apply_splat_textures` tick rebuilds the array atlas.
pub(super) fn poll_splat_layer_fetches(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PendingSplatLayerFetch)>,
    mut state: ResMut<TerrainSplatState>,
    mut status: ResMut<ReferencedLayerStatus>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    mut report: AssetReport,
) {
    let now = time.elapsed_secs_f64();
    let mut reporter = report.at(now);
    for (entity, mut pending) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut pending.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // The procedural placeholder stays in place either way — but the
        // owner is now told, because a plausible dirt surface standing in
        // for their image is the failure that looks most like success.
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(reason) => {
                note_layer_failure(&mut status, &mut reporter, &pending, reason, now);
                continue;
            }
        };

        let handles = match decode_and_upload_splat_layer(&bytes, pending.texture_size, &mut images)
        {
            Ok(handles) => handles,
            Err(reason) => {
                note_layer_failure(&mut status, &mut reporter, &pending, reason, now);
                continue;
            }
        };

        // Override both albedo and normal slots. The Referenced image
        // doesn't ship a separate normal map, so the normal slot gets a
        // flat-up normal — the surface looks less detailed than a
        // procedural layer (no per-pixel bumps), which is a reasonable
        // v0.1.0 trade for explicit-asset support.
        state.layer_albedo[pending.layer_idx] = Some(handles.albedo.clone());
        state.layer_normal[pending.layer_idx] = Some(handles.normal.clone());
        status.layers[pending.layer_idx] = Some(LayerFetch::Ready {
            source: pending.source.clone(),
            texture_size: pending.texture_size,
            albedo: handles.albedo,
            normal: handles.normal,
        });
        // Force a rebuild of the array atlas — the previous one (if
        // any) was built from the procedural placeholder.
        state.applied = false;
        info!(
            "Splat layer {} overridden with Referenced bytes",
            pending.layer_idx
        );
    }
}

/// Record one layer's failure against its source and report it once.
fn note_layer_failure(
    status: &mut ReferencedLayerStatus,
    reporter: &mut FailureReporter,
    pending: &PendingSplatLayerFetch,
    reason: AssetFetchError,
    now: f64,
) {
    let failure = AssetFailure::after(pending.previous.as_ref(), reason, now);
    reporter.record(
        AssetClass::TerrainLayer,
        &format!("layer {}", pending.layer_idx),
        reason,
    );
    status.layers[pending.layer_idx] = Some(LayerFetch::Failed {
        source: pending.source.clone(),
        failure,
    });
}

/// Decode `bytes` via the `image` crate, resize to
/// `texture_size × texture_size`, and turn the result into a
/// [`TextureMap`] with flat normal + neutral roughness fillers so
/// [`map_to_images_with_usages`] produces the same shape (Rgba8 sRGB albedo +
/// Rgba8 normal + Rgba8 ORM, mipchain, repeat sampler) the procedural
/// pipeline yields. Returns the decode failure's reason, which is what the
/// terrain panel prints beside the layer.
fn decode_and_upload_splat_layer(
    bytes: &[u8],
    texture_size: u32,
    images: &mut Assets<Image>,
) -> Result<bevy_symbios_texture::GeneratedHandles, AssetFetchError> {
    // The working size IS the layer size: a splat layer is resampled to a
    // square `texture_size` two lines below regardless, so decoding straight
    // to that box means the oversized frame is released before the resample
    // rather than after it (#1128).
    let dyn_img =
        crate::world_builder::blob_fetch::decode_image_capped(bytes, "Splat layer", texture_size)?;

    let resized = dyn_img
        .resize_exact(
            texture_size,
            texture_size,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    let albedo: Vec<u8> = resized.into_raw();

    // Flat tangent-space normal: (128, 128, 255, 255) — z-up.
    let pixel_count = (texture_size as usize) * (texture_size as usize);
    let mut normal = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        normal.extend_from_slice(&[128, 128, 255, 255]);
    }
    // Neutral ORM (occlusion 1.0, mid roughness, no metallic).
    let mut roughness = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        roughness.extend_from_slice(&[255, 128, 0, 255]);
    }

    let map = TextureMap {
        albedo,
        normal,
        roughness,
        // No glow layer on a decoded splat image, and the base level is the
        // only level we synthesise — the upload mip-chains it.
        emissive: None,
        mip_level_count: 1,
        width: texture_size,
        height: texture_size,
    };
    // `MAIN_WORLD`-only, like the procedural splat layers: this image is read
    // back on the CPU by `build_texture_array`, never bound to a material, so
    // skip the GPU upload and keep `Image::data` resident.
    Ok(map_to_images_with_usages(
        map,
        RenderAssetUsages::MAIN_WORLD,
        images,
    ))
}

#[cfg(test)]
mod tests {
    //! Pure-function tests for the splat-layer Referenced resolver
    //! (#310). The fetch dispatch + poll system flow requires a Bevy
    //! App harness which would compile-in the full plugin stack —
    //! prohibitive for fast unit tests. The decode-and-upload helper
    //! is the load-bearing piece (bytes → resized RGBA → TextureMap →
    //! Assets<Image>) and is tested directly here.
    use super::*;

    /// Synthesise a tiny in-memory PNG via the `image` crate so the
    /// decoder has something realistic to chew on. 16×16 is below the
    /// target `texture_size` so the resize path is exercised in the
    /// up-scaling direction.
    fn tiny_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
        for (x, y, p) in img.enumerate_pixels_mut() {
            // Subtle checkerboard so the resized output isn't all zero.
            let c = if (x + y) % 2 == 0 { 200 } else { 80 };
            *p = Rgba([c, c, c, 255]);
        }
        let mut bytes: Vec<u8> = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");
        bytes
    }

    /// The #1246 f347 sequence: the owner edits the layer's URL while the
    /// old one is still recorded as failed. The status must answer for the
    /// reference the field NAMES, not for the slot — otherwise a fresh URL
    /// inherits the previous one's failure line and its backoff.
    #[test]
    fn a_layer_status_belongs_to_its_source_not_its_slot() {
        let old_source = SovereignAssetReference::Url {
            url: "https://x.test/old.png".into(),
        };
        let new_source = SovereignAssetReference::Url {
            url: "https://x.test/new.png".into(),
        };
        let mut status = ReferencedLayerStatus::default();
        status.layers[2] = Some(LayerFetch::Failed {
            source: old_source.clone(),
            failure: AssetFailure::after(None, AssetFetchError::HttpStatus(404), 0.0),
        });

        assert!(
            status.status(2, &old_source).is_some(),
            "the reference that failed must report its failure"
        );
        assert!(
            status.status(2, &new_source).is_none(),
            "a re-typed reference must start clean, not inherit the old failure"
        );
        assert!(
            status.status(0, &old_source).is_none(),
            "and the failure belongs to layer 2 only"
        );

        assert_eq!(status.failed_layers().count(), 1);
        assert!(status.clear_failure(2), "Retry now clears it");
        assert!(status.status(2, &old_source).is_none());
        assert!(
            !status.clear_failure(2),
            "and there is nothing left to clear"
        );
    }

    #[test]
    fn decode_and_upload_produces_handles_at_target_size() {
        let bytes = tiny_png(16, 16);
        let mut images = Assets::<Image>::default();
        let handles =
            decode_and_upload_splat_layer(&bytes, 64, &mut images).expect("decode succeeds");
        let albedo = images
            .get(handles.albedo.id())
            .expect("albedo asset present");
        assert_eq!(
            albedo.texture_descriptor.size.width, 64,
            "albedo must be resized to texture_size width"
        );
        assert_eq!(
            albedo.texture_descriptor.size.height, 64,
            "albedo must be resized to texture_size height"
        );
        // The normal slot is filled with the flat-up placeholder, so a
        // handle must exist even though the source PNG has no normal.
        assert!(
            images.get(handles.normal.id()).is_some(),
            "normal slot must carry a flat-up placeholder"
        );
    }

    #[test]
    fn decode_and_upload_handles_non_square_input() {
        let bytes = tiny_png(32, 8);
        let mut images = Assets::<Image>::default();
        let handles =
            decode_and_upload_splat_layer(&bytes, 128, &mut images).expect("decode succeeds");
        let albedo = images
            .get(handles.albedo.id())
            .expect("albedo asset present");
        assert_eq!(albedo.texture_descriptor.size.width, 128);
        assert_eq!(albedo.texture_descriptor.size.height, 128);
    }

    /// Garbage bytes name their reason now (#1246): the terrain panel
    /// prints it beside the layer, so "could not be read" has to survive
    /// the decode helper rather than being flattened to a `None`.
    #[test]
    fn decode_and_upload_reports_why_garbage_bytes_failed() {
        let mut images = Assets::<Image>::default();
        let result = decode_and_upload_splat_layer(&[0, 1, 2, 3, 4], 64, &mut images);
        assert_eq!(
            result.err(),
            Some(AssetFetchError::Undecodable),
            "malformed bytes must report the reason (not panic, not a bare None)"
        );
    }

    #[test]
    fn decode_and_upload_normal_placeholder_is_flat_up() {
        // The normal placeholder is (128, 128, 255, 255) per pixel.
        // Spot-check the upload produces that pattern.
        let bytes = tiny_png(4, 4);
        let mut images = Assets::<Image>::default();
        let handles =
            decode_and_upload_splat_layer(&bytes, 8, &mut images).expect("decode succeeds");
        let normal = images.get(handles.normal.id()).expect("normal");
        // `map_to_images` mip-chains the base, so the first 8*8*4 bytes
        // are the base level. Check the first pixel only — the
        // generator's mipmap pipeline shouldn't disturb the base
        // level's first texel.
        let data = normal.data.as_ref().expect("normal has data");
        assert_eq!(
            &data[0..4],
            &[128, 128, 255, 255],
            "normal base level must carry the flat-up tangent normal"
        );
    }
}
