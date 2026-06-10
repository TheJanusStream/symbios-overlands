//! Top-level chassis-family pick for the default avatar.
//!
//! Sampled before any per-family deriver so the rest of the avatar
//! pipeline can branch: a DID resolves to exactly one visual family
//! (hover-boat, airship, humanoid, land-skiff), and only that family's
//! design deriver contributes nodes to the visuals tree. Locomotion
//! follows the family (boat → HoverBoat, airship → Helicopter,
//! humanoid → Humanoid, skiff → Car) so the default chassis *feels*
//! like what it looks like.
//!
//! The pick is uniform — every family is equally likely on a fresh
//! DID. Diversity inside each family comes from the per-family design
//! derivers ([`super::vessel`], [`super::airship`], [`super::skiff`],
//! [`super::humanoid_style`]).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::pick;

const AVATAR_CHASSIS_SALT: u64 = 0xC4A5_51F0_C4A5_51F0;

/// Discrete visual family of the default avatar. Picked first; the
/// matching design deriver then shapes the silhouette within the
/// family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChassisFamily {
    /// Hover-boat — monohull / catamaran / trimaran / barge.
    Boat,
    /// Lighter-than-air — envelope + gondola.
    Airship,
    /// Primitive-built figure consuming skin / hair / eye / gait.
    Humanoid,
    /// Land vehicle — rover / dune-skiff / trike.
    Skiff,
}

impl ChassisFamily {
    pub const ALL: [Self; 4] = [Self::Boat, Self::Airship, Self::Humanoid, Self::Skiff];

    pub fn for_did(did: &str) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_CHASSIS_SALT);
        pick(&Self::ALL, &mut rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(
            ChassisFamily::for_did("did:plc:abc"),
            ChassisFamily::for_did("did:plc:abc")
        );
    }

    #[test]
    fn all_families_reachable() {
        // 200 seeds at 4 uniform families: the odds of any family never
        // appearing are (3/4)^200 ≈ 10^-25 — a miss means the sampler is
        // broken, not unlucky.
        let mut seen = [false; 4];
        for s in 0u64..200 {
            let f = ChassisFamily::for_did(&format!("did:test:{s}"));
            let i = ChassisFamily::ALL.iter().position(|x| *x == f).unwrap();
            seen[i] = true;
        }
        assert_eq!(seen, [true; 4], "some chassis family never sampled");
    }
}
