//! Scatter / grid placement helpers: deterministic sampling inside a
//! [`ScatterBounds`] and the dominant-biome lookup the scatter biome filter
//! consults. The biome lookup delegates to `bevy_symbios_ground::SplatRule`
//! so the splat-rule weight formula stays single-sourced upstream.

use bevy_symbios_ground::SplatRule;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::RngCore;

use crate::pds::{ScatterBounds, SovereignSplatRule, SovereignTerrainConfig};

/// Uniform sample inside the scatter region. Circle bounds use rejection
/// sampling so the distribution stays flat instead of clumping at the
/// centre (which a naïve `radius * random()` would produce).
pub(crate) fn sample_bounds(bounds: &ScatterBounds, rng: &mut ChaCha8Rng) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
            let lx = unit_f32(rng) * extents.0[0];
            let lz = unit_f32(rng) * extents.0[1];
            let rot = rotation.0;
            let rx = lx * rot.cos() - lz * rot.sin();
            let rz = lx * rot.sin() + lz * rot.cos();
            (center.0[0] + rx, center.0[1] + rz)
        }
        ScatterBounds::Circle { center, radius } => loop {
            let x = unit_f32(rng);
            let z = unit_f32(rng);
            if x * x + z * z <= 1.0 {
                return (center.0[0] + x * radius.0, center.0[1] + z * radius.0);
            }
        },
    }
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`.
pub(crate) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// Biome evaluation
// ---------------------------------------------------------------------------

/// Convert a wire-format [`SovereignSplatRule`] into an upstream [`SplatRule`]
/// so the weight formula can be evaluated by [`SplatRule::weight`] directly,
/// without re-implementing the smooth-range logic locally.
fn convert_rule(r: &SovereignSplatRule) -> SplatRule {
    SplatRule::new(
        (r.height_min.0, r.height_max.0),
        (r.slope_min.0, r.slope_max.0),
        r.sharpness.0,
    )
}

/// Return the dominant biome index (0=Grass, 1=Dirt, 2=Rock, 3=Snow) at the
/// given world-space (height, slope) pair, using the terrain generator's
/// splat rules. The splat rules expect *normalised* heights so we divide
/// by `height_scale` first.
pub(crate) fn dominant_biome(cfg: &SovereignTerrainConfig, height_world: f32, slope: f32) -> u8 {
    let height_norm = if cfg.height_scale.0.abs() > f32::EPSILON {
        height_world / cfg.height_scale.0
    } else {
        0.0
    };
    let weights = [
        convert_rule(&cfg.material.rules[0]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[1]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[2]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[3]).weight(height_norm, slope),
    ];
    let mut best = 0;
    let mut max_w = weights[0];
    for (i, &w) in weights.iter().enumerate().skip(1) {
        if w > max_w {
            max_w = w;
            best = i;
        }
    }
    best as u8
}
