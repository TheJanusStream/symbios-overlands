//! Humanoid styling knobs for the figure default avatar family.
//!
//! [`super::body`] carries the proportions and [`super::palette`] the
//! skin / hair / eye / clothing colours; this deriver adds the
//! discrete costume picks that make two same-proportioned figures
//! read as different characters — headwear, backpack, glowing eyes,
//! hair volume.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32, unit_f32};

const AVATAR_HUMANOID_SALT: u64 = 0x4F1A_60DD_4F1A_60DD;

/// Headwear family. `None` lets the hair carry the silhouette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HatStyle {
    None,
    /// Tall tapered cone — wizard / festival hat.
    Cone,
    /// Flat-topped cylinder — top hat.
    TopHat,
    /// Thin band across the brow — visor / circlet.
    Band,
}

impl HatStyle {
    pub const ALL: [Self; 4] = [Self::None, Self::Cone, Self::TopHat, Self::Band];
}

/// All seeded humanoid costume knobs.
#[derive(Clone, Copy, Debug)]
pub struct HumanoidStyle {
    pub hat: HatStyle,
    /// Whether the figure carries a backpack (which also hosts the
    /// pfp banner pole — without it the banner mounts to the belt).
    pub backpack: bool,
    /// Emissive eyes. Matte eyes read organic, glowing eyes read
    /// construct / golem.
    pub eye_glow: bool,
    /// Hair-cap volume multiplier.
    pub hair_volume_scale: f32,
    /// Shoulder pauldron spheres — small armour accents in the
    /// tertiary colour.
    pub pauldrons: bool,
}

impl HumanoidStyle {
    pub fn for_did(did: &str) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_HUMANOID_SALT);
        Self {
            hat: pick(&HatStyle::ALL, &mut rng),
            backpack: unit_f32(&mut rng) < 0.6,
            eye_glow: unit_f32(&mut rng) < 0.35,
            hair_volume_scale: range_f32(&mut rng, 0.85, 1.35),
            pauldrons: unit_f32(&mut rng) < 0.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = HumanoidStyle::for_did("did:plc:test");
        let b = HumanoidStyle::for_did("did:plc:test");
        assert_eq!(a.hat, b.hat);
        assert_eq!(a.backpack, b.backpack);
        assert_eq!(a.eye_glow, b.eye_glow);
        assert_eq!(a.hair_volume_scale, b.hair_volume_scale);
    }

    #[test]
    fn fields_in_range_and_all_hats_reachable() {
        let mut seen = [false; 4];
        for s in 0u64..200 {
            let h = HumanoidStyle::for_did(&format!("did:test:{s}"));
            assert!((0.85..=1.35).contains(&h.hair_volume_scale));
            let i = HatStyle::ALL.iter().position(|x| *x == h.hat).unwrap();
            seen[i] = true;
        }
        assert_eq!(seen, [true; 4], "some hat style never sampled");
    }
}
