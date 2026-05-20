//! Avatar body proportions — overall scale, shoulder width, head
//! size, limb thickness, body archetype.
//!
//! Designed for a humanoid surface (the proposed avatar shape) but
//! the values map cleanly onto the current hover-boat default too:
//! `height_scale` scales the whole vessel, `shoulder_width_scale`
//! scales hull / pontoon X dimensions, `head_scale` scales the
//! sphere finial, `limb_thickness_scale` scales pontoon radius and
//! mast radius. The `torso_leg_ratio` field is unused for
//! hover-boats; it's computed because the surface proposal includes
//! it and a future humanoid will read it directly.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32};

const AVATAR_BODY_SALT: u64 = 0xB0DD_B0DD_B0DD_B0DD;

/// Discrete body family. Sampled first, then continuous knobs are
/// biased per archetype so a "stocky" body doesn't roll thin shoulders
/// and a "slim" body doesn't roll thick limbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyArchetype {
    Slim,
    Average,
    Stocky,
}

impl BodyArchetype {
    pub const ALL: [Self; 3] = [Self::Slim, Self::Average, Self::Stocky];
}

/// All seeded avatar body proportions. Every scale is a multiplier
/// against the body's nominal dimensions (i.e. `1.0` is "default
/// size"); the hover-boat default impl reads them as multiplicative
/// scalars on the existing cuboid / capsule sizes.
#[derive(Clone, Copy, Debug)]
pub struct AvatarBody {
    pub archetype: BodyArchetype,
    /// Overall body height multiplier. Affects every dimension
    /// uniformly (it's the "I'm tall / short" knob).
    pub height_scale: f32,
    /// Torso : leg length ratio (`0.5` is balanced). Unused by the
    /// hover-boat default; pre-computed for the future humanoid path.
    pub torso_leg_ratio: f32,
    /// Head / finial scale.
    pub head_scale: f32,
    /// Lateral (X-axis) scale. Hull width / shoulder width.
    pub shoulder_width_scale: f32,
    /// Limb thickness — pontoon radius, mast radius, arm/leg girth.
    pub limb_thickness_scale: f32,
}

impl AvatarBody {
    pub fn for_did(did: &str) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_BODY_SALT);
        let archetype = pick(&BodyArchetype::ALL, &mut rng);

        // Archetype biases the continuous sampling ranges. Slim gets a
        // narrower shoulder + thinner limbs; stocky goes the other way;
        // average sits in the middle of every band.
        let (height_lo, height_hi, shoulder_lo, shoulder_hi, limb_lo, limb_hi) = match archetype {
            BodyArchetype::Slim => (1.00, 1.15, 0.82, 0.95, 0.80, 0.95),
            BodyArchetype::Average => (0.92, 1.08, 0.92, 1.08, 0.92, 1.08),
            BodyArchetype::Stocky => (0.85, 1.00, 1.05, 1.18, 1.05, 1.20),
        };

        Self {
            archetype,
            height_scale: range_f32(&mut rng, height_lo, height_hi),
            torso_leg_ratio: range_f32(&mut rng, 0.42, 0.58),
            head_scale: range_f32(&mut rng, 0.90, 1.10),
            shoulder_width_scale: range_f32(&mut rng, shoulder_lo, shoulder_hi),
            limb_thickness_scale: range_f32(&mut rng, limb_lo, limb_hi),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AvatarBody::for_did("did:plc:test");
        let b = AvatarBody::for_did("did:plc:test");
        assert_eq!(a.archetype, b.archetype);
        assert_eq!(a.height_scale, b.height_scale);
        assert_eq!(a.shoulder_width_scale, b.shoulder_width_scale);
    }

    #[test]
    fn scales_within_bounds() {
        for s in 0u64..16 {
            let b = AvatarBody::for_did(&format!("did:test:{s}"));
            assert!((0.7..=1.3).contains(&b.height_scale));
            assert!((0.4..=0.6).contains(&b.torso_leg_ratio));
            assert!((0.85..=1.15).contains(&b.head_scale));
            assert!((0.75..=1.25).contains(&b.shoulder_width_scale));
            assert!((0.75..=1.30).contains(&b.limb_thickness_scale));
        }
    }
}
