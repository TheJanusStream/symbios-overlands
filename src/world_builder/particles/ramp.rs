//! Shared quantised-fade materials for particle emitters.
//!
//! The naive per-particle approach — allocate a `StandardMaterial` at
//! spawn and rewrite its colour every frame — is the single most
//! expensive pattern this renderer can express: every particle becomes
//! its own material instance (defeating Bevy's automatic mesh batching,
//! so N particles are N draw calls), and every per-frame
//! `Assets::get_mut` marks the asset modified, forcing the renderer to
//! re-prepare that particle's material bind group every frame. On
//! WebGL2 — the demo's floor — both costs are at their worst.
//!
//! Instead each emitter bakes its colour fade into a small **ramp** of
//! shared materials at first emission ([`RAMP_STEPS`] buckets along the
//! start→end gradient, or a single bucket when the two colours are
//! equal). A particle's fade is then a `MeshMaterial3d` *handle swap*
//! when its lifetime fraction crosses into the next bucket — an
//! asset-id copy identical in cost to the atlas-frame mesh swap, with
//! zero asset mutation. All particles of an emitter sharing a bucket
//! (and mesh) batch into one draw.
//!
//! Size fade is untouched by quantisation — it lives on
//! `Transform::scale` and stays perfectly smooth.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_texture::{TextureCache, TextureCacheKey, TextureConfig, map_to_images_card};

use super::super::image_cache::{BlobImageCache, SamplerFilter, request_blob_image_filtered};
use super::ParticleEmitter;
use crate::config::textures::PARTICLE_CELL;
use crate::pds::{ParticleBlendMode, TextureFilter};

/// Number of colour buckets a fading emitter bakes. 16 steps along a
/// typically low-contrast, fast-moving gradient is visually
/// indistinguishable from per-frame lerp while capping the emitter's
/// material count at 16 regardless of how many particles are alive.
pub(super) const RAMP_STEPS: usize = 16;

/// The baked ramp, attached to the emitter entity by
/// [`super::tick_emitter_spawn`] on first emission. Handles are strong:
/// the assets live while the emitter (or any straggler particle still
/// pointing at a bucket) holds them, and are freed when the last handle
/// drops after the emitter despawns.
#[derive(Component, Clone)]
pub struct EmitterMaterialRamp {
    handles: Vec<Handle<StandardMaterial>>,
}

impl EmitterMaterialRamp {
    /// Bucket index for a lifetime fraction `t ∈ [0, 1]`.
    pub(super) fn bucket_for(&self, t: f32) -> usize {
        let n = self.handles.len();
        ((t * n as f32) as usize).min(n.saturating_sub(1))
    }

    /// Shared handle for a bucket. `idx` is clamped so a stale index
    /// (e.g. from a particle that outlived an emitter rebuild) can't
    /// panic.
    pub(super) fn handle(&self, idx: usize) -> &Handle<StandardMaterial> {
        &self.handles[idx.min(self.handles.len().saturating_sub(1))]
    }
}

/// Resolve the per-emitter [`SamplerFilter`] from a record's
/// [`TextureFilter`]. Unknown forward-compat values fall back to
/// Linear so a forward-compat record renders smooth-filtered.
fn sampler_filter_for(filter: &TextureFilter) -> SamplerFilter {
    match filter {
        TextureFilter::Nearest => SamplerFilter::Nearest,
        TextureFilter::Linear | TextureFilter::Unknown => SamplerFilter::Linear,
    }
}

/// Bake the emitter's fade ramp. One material per bucket, colours
/// lerped start→end; additive emitters route the bucket colour through
/// `emissive` as well, so the additive accumulator stays lit on dark
/// backgrounds where pure-alpha would wash out. A non-fading emitter
/// (`start_color == end_color`) bakes a single bucket — its whole
/// particle population shares one material.
///
/// The optional texture is registered against the shared blob image
/// cache **once per bucket here**, not once per particle: the cache
/// patches `base_color_texture` on every ramp material when the bytes
/// arrive (Ready entries paint synchronously). The previous per-particle
/// registration grew the cache's pending list by one entry per spawned
/// particle for the whole fetch window.
pub(super) fn build_emitter_ramp(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    texture_cache: &mut TextureCache,
    blob_image_cache: &mut BlobImageCache,
    emitter: &ParticleEmitter,
) -> EmitterMaterialRamp {
    let steps = if emitter.start_color == emitter.end_color {
        1
    } else {
        RAMP_STEPS
    };

    // Bake the procedural sprite once (if any) before the bucket loop: every
    // ramp material shares the one albedo handle. The atlas dimensions were
    // resolved onto the emitter at compile time, so the bake is
    // `cols × rows` cells of `PARTICLE_CELL` each. The legacy fetched
    // `texture` takes precedence and is handled per-bucket below.
    let procedural_albedo = if emitter.texture.is_none() {
        emitter.procedural_texture.as_ref().and_then(|tc| {
            let (rows, cols) = emitter
                .texture_atlas
                .as_ref()
                .map(|a| (a.rows.max(1), a.cols.max(1)))
                .unwrap_or((1, 1));
            bake_procedural_albedo(
                tc,
                cols * PARTICLE_CELL,
                rows * PARTICLE_CELL,
                texture_cache,
                images,
            )
        })
    } else {
        None
    };
    let additive = matches!(
        emitter.blend_mode,
        ParticleBlendMode::Additive | ParticleBlendMode::Unknown
    );

    let mut handles = Vec::with_capacity(steps);
    for i in 0..steps {
        let t = if steps == 1 {
            0.0
        } else {
            i as f32 / (steps - 1) as f32
        };
        let color = LinearRgba::new(
            super::lerp_unit(t, emitter.start_color.red, emitter.end_color.red),
            super::lerp_unit(t, emitter.start_color.green, emitter.end_color.green),
            super::lerp_unit(t, emitter.start_color.blue, emitter.end_color.blue),
            super::lerp_unit(t, emitter.start_color.alpha, emitter.end_color.alpha),
        );

        let mut material = StandardMaterial {
            base_color: color.into(),
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        };
        material.alpha_mode = if additive {
            AlphaMode::Add
        } else {
            AlphaMode::Blend
        };
        if additive {
            material.emissive = color;
        }
        let handle = materials.add(material);

        if let Some(source) = &emitter.texture {
            // Legacy fetched texture: registered against the blob cache,
            // which patches `base_color_texture` when the bytes arrive.
            request_blob_image_filtered(
                commands,
                blob_image_cache,
                materials,
                &handle,
                source,
                sampler_filter_for(&emitter.texture_filter),
            );
        } else if let Some(albedo) = &procedural_albedo {
            // Procedural sprite: the albedo is already baked and uploaded, so
            // paint it straight onto this bucket's material.
            if let Some(mat) = materials.get_mut(&handle) {
                mat.base_color_texture = Some(albedo.clone());
            }
        }
        handles.push(handle);
    }

    EmitterMaterialRamp { handles }
}

/// Bake (or fetch from cache) the albedo image for a procedural particle
/// sprite. Generation runs synchronously here because the ramp is built
/// once per emitter (first emission) and particle bakes are small
/// (`PARTICLE_CELL`-sized cells); the [`TextureCache`] dedups identical
/// configs across emitters, and on wasm a cache hit avoids the main-thread
/// stall entirely. Returns `None` for a non-generator config
/// ([`TextureConfig::None`]) or a generation failure (logged).
fn bake_procedural_albedo(
    config: &TextureConfig,
    width: u32,
    height: u32,
    texture_cache: &mut TextureCache,
    images: &mut Assets<Image>,
) -> Option<Handle<Image>> {
    let key = TextureCacheKey {
        kind: config.label(),
        fingerprint: config.fingerprint(),
        width,
        height,
    };
    if let Some(handles) = texture_cache.get(&key, images) {
        return Some(handles.albedo.clone());
    }
    match config.generate_sync(width, height)? {
        Ok(map) => {
            // Sprites are alpha-silhouette cards: clamp-to-edge upload so the
            // transparent border doesn't bleed across atlas-cell seams.
            let handles = Arc::new(map_to_images_card(map, images));
            let albedo = handles.albedo.clone();
            texture_cache.insert(key, handles);
            Some(albedo)
        }
        Err(e) => {
            warn!("procedural particle sprite generation failed: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_of(n: usize) -> EmitterMaterialRamp {
        EmitterMaterialRamp {
            handles: (0..n).map(|_| Handle::default()).collect(),
        }
    }

    #[test]
    fn bucket_for_spans_the_unit_interval() {
        let ramp = ramp_of(16);
        assert_eq!(ramp.bucket_for(0.0), 0);
        assert_eq!(ramp.bucket_for(0.5), 8);
        // t = 1.0 would index one past the end without the clamp.
        assert_eq!(ramp.bucket_for(1.0), 15);
        assert_eq!(ramp.bucket_for(2.0), 15);
    }

    #[test]
    fn single_bucket_ramp_always_selects_zero() {
        let ramp = ramp_of(1);
        assert_eq!(ramp.bucket_for(0.0), 0);
        assert_eq!(ramp.bucket_for(0.99), 0);
        assert_eq!(ramp.bucket_for(1.0), 0);
    }

    #[test]
    fn handle_clamps_stale_indices() {
        let ramp = ramp_of(4);
        // A particle that swapped to bucket 15 before its emitter was
        // rebuilt with a shorter ramp must not panic.
        let _ = ramp.handle(15);
    }

    /// The procedural bake produces an albedo handle for a sprite config and
    /// populates the cache, so a second bake of the same key is a cache hit.
    #[test]
    fn procedural_albedo_bakes_and_caches() {
        let mut images = Assets::<Image>::default();
        let mut cache = TextureCache::memory(4);
        let config = TextureConfig::SoftDisc(Default::default());

        let first = bake_procedural_albedo(&config, 64, 64, &mut cache, &mut images);
        assert!(first.is_some(), "sprite config must bake an albedo image");

        // Same key → served from cache (still Some, no second generation).
        let key = TextureCacheKey {
            kind: config.label(),
            fingerprint: config.fingerprint(),
            width: 64,
            height: 64,
        };
        assert!(
            cache.get(&key, &mut images).is_some(),
            "first bake must have populated the cache"
        );
    }

    /// `TextureConfig::None` is not a generator, so the baker returns `None`
    /// rather than panicking or uploading an empty image.
    #[test]
    fn procedural_albedo_none_config_is_none() {
        let mut images = Assets::<Image>::default();
        let mut cache = TextureCache::memory(4);
        assert!(
            bake_procedural_albedo(&TextureConfig::None, 64, 64, &mut cache, &mut images).is_none()
        );
    }
}
