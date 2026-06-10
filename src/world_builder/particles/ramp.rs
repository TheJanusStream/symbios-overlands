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

use bevy::prelude::*;

use super::super::image_cache::{BlobImageCache, SamplerFilter, request_blob_image_filtered};
use super::ParticleEmitter;
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
    blob_image_cache: &mut BlobImageCache,
    emitter: &ParticleEmitter,
) -> EmitterMaterialRamp {
    let steps = if emitter.start_color == emitter.end_color {
        1
    } else {
        RAMP_STEPS
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
            request_blob_image_filtered(
                commands,
                blob_image_cache,
                materials,
                &handle,
                source,
                sampler_filter_for(&emitter.texture_filter),
            );
        }
        handles.push(handle);
    }

    EmitterMaterialRamp { handles }
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
}
