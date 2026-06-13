//! `SovereignTextureConfig::Referenced` splat layers: the URL /
//! ATProto-blob fetch path that overrides a procedural placeholder
//! layer with explicit image bytes once they arrive.

use bevy::prelude::*;
use bevy_symbios_texture::{TextureMap, map_to_images};

use crate::pds::SovereignAssetReference;
use crate::world_builder::blob_fetch;
use crate::world_builder::image_cache::MAX_IMAGE_BYTES;

use super::TerrainSplatState;

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
    task: bevy::tasks::Task<Option<Vec<u8>>>,
}

/// Spawn an `IoTaskPool` task that fetches the bytes referenced by
/// `source` and parks them on a [`PendingSplatLayerFetch`] component
/// for [`poll_splat_layer_fetches`] to drain.
pub(super) fn spawn_splat_layer_fetch(
    commands: &mut Commands,
    layer_idx: usize,
    source: &SovereignAssetReference,
    texture_size: u32,
) {
    // Empty / forward-compat / image-pfp variants don't resolve to
    // splat-usable bytes — bail without spawning a task that would
    // just return None.
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
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(PendingSplatLayerFetch {
        layer_idx,
        texture_size,
        task,
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
/// [`bevy_symbios_texture::map_to_images`] so the result carries the
/// same mip chain + repeat sampler shape as procedural layers. The
/// resolved albedo handle overrides [`TerrainSplatState::layer_albedo`]
/// for the layer index, and `state.applied` is flipped to `false` so
/// the next `apply_splat_textures` tick rebuilds the array atlas.
pub(super) fn poll_splat_layer_fetches(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PendingSplatLayerFetch)>,
    mut state: ResMut<TerrainSplatState>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, mut pending) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut pending.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        let Some(bytes) = result else {
            // Fetch failed — log was already emitted by blob_fetch.
            // The procedural placeholder stays in place; the layer
            // just doesn't honour the Referenced override on this room
            // load. A future load may retry (no negative-cache).
            continue;
        };

        let Some(handles) =
            decode_and_upload_splat_layer(&bytes, pending.texture_size, &mut images)
        else {
            continue;
        };

        // Override both albedo and normal slots. The Referenced image
        // doesn't ship a separate normal map, so the normal slot gets a
        // flat-up normal — the surface looks less detailed than a
        // procedural layer (no per-pixel bumps), which is a reasonable
        // v0.1.0 trade for explicit-asset support.
        state.layer_albedo[pending.layer_idx] = Some(handles.albedo);
        state.layer_normal[pending.layer_idx] = Some(handles.normal);
        // Force a rebuild of the array atlas — the previous one (if
        // any) was built from the procedural placeholder.
        state.applied = false;
        info!(
            "Splat layer {} overridden with Referenced bytes",
            pending.layer_idx
        );
    }
}

/// Decode `bytes` via the `image` crate, resize to
/// `texture_size × texture_size`, and turn the result into a
/// [`TextureMap`] with flat normal + neutral roughness fillers so
/// [`map_to_images`] produces the same shape (Rgba8 sRGB albedo +
/// Rgba8 normal + Rgba8 ORM, mipchain, repeat sampler) the procedural
/// pipeline yields. Returns `None` on any decode failure.
fn decode_and_upload_splat_layer(
    bytes: &[u8],
    texture_size: u32,
    images: &mut Assets<Image>,
) -> Option<bevy_symbios_texture::GeneratedHandles> {
    let dyn_img = crate::world_builder::blob_fetch::decode_image_capped(bytes, "Splat layer")?;
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
        // only level we synthesise — `map_to_images` mip-chains it on upload.
        emissive: None,
        mip_level_count: 1,
        width: texture_size,
        height: texture_size,
    };
    Some(map_to_images(map, images))
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

    #[test]
    fn decode_and_upload_returns_none_on_garbage_bytes() {
        let mut images = Assets::<Image>::default();
        let result = decode_and_upload_splat_layer(&[0, 1, 2, 3, 4], 64, &mut images);
        assert!(
            result.is_none(),
            "malformed bytes must return None (not panic)"
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
