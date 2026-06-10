//! Seeded landmark spec — every home region gets exactly one
//! biome-appropriate structure near spawn.
//!
//! The pick is a weighted-by-repetition pool per [`BiomeArchetype`]
//! (lighthouses cluster on coasts, ziggurats in deserts and jungles,
//! stone circles on tundra), with the landform able to append extra
//! candidates (archipelagos lean lighthouse regardless of biome).
//! Placement faces the structure roughly toward the spawn origin and
//! keeps a per-structure clear distance so a castle doesn't swallow
//! the spawn square while a stone circle stays in easy walking range.
//!
//! The wiring layer ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! resolves [`Landmark::slug`] through the catalogue, restamps the
//! grammar seed of Shape-based entries with [`Landmark::grammar_seed`]
//! (so two users with the same structure still get different
//! stochastic derivations), and emits one `Placement::Absolute` with
//! terrain snap.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, range_f32, unit_f32,
};

/// Sub-stream salt distinct from every sibling room deriver.
const LANDMARK_STREAM_SALT: u64 = 0x1A4D_3A2C_1A4D_3A2C;

/// One seeded landmark: which catalogue structure, where, and how it
/// stands.
#[derive(Clone, Copy, Debug)]
pub struct Landmark {
    /// Catalogue slug (see `crate::catalogue::items`). Always one of
    /// the entries listed in [`biome_pool`] / [`landform_extra`].
    pub slug: &'static str,
    /// World XZ of the structure origin.
    pub offset: [f32; 2],
    /// Yaw (radians around Y). Faces the structure roughly toward the
    /// spawn origin, with jitter so rooms don't feel surveyed.
    pub yaw_rad: f32,
    /// Uniform scale multiplier.
    pub scale: f32,
    /// Replacement seed for Shape-grammar entries' stochastic rules.
    pub grammar_seed: u64,
    /// Dry-land clearance radius (m) for the compiler's
    /// water-avoidance walk — roughly the structure's bounding-circle
    /// radius around its (centred) anchor.
    pub clearance: f32,
}

/// `(slug, minimum spawn distance, water clearance radius)` — bigger
/// footprints sit further out so the spawn scatter square never lands
/// inside a wall, and carry a larger dry-land clearance so the
/// compiler's water-avoidance walk keeps the *whole* footprint out of
/// the sea, not just the anchor point.
type Candidate = (&'static str, f32, f32);

fn biome_pool(biome: BiomeArchetype) -> &'static [Candidate] {
    use BiomeArchetype::*;
    match biome {
        Lush => &[
            ("villa", 45.0, 13.5),
            ("ruined_temple", 45.0, 14.5),
            ("ziggurat", 60.0, 18.0),
            ("stone_circle", 40.0, 10.0),
        ],
        Arid => &[
            ("ziggurat", 60.0, 18.0),
            ("ruined_temple", 45.0, 14.5),
            ("observatory", 40.0, 4.5),
            ("watchtower", 35.0, 9.5),
        ],
        Alpine => &[
            ("watchtower", 35.0, 9.5),
            ("medieval_castle", 110.0, 54.0),
            ("observatory", 40.0, 4.5),
            ("stone_circle", 40.0, 10.0),
        ],
        Volcanic => &[
            ("ruined_temple", 45.0, 14.5),
            ("watchtower", 35.0, 9.5),
            ("ziggurat", 60.0, 18.0),
            ("stone_circle", 40.0, 10.0),
        ],
        Coastal => &[
            ("lighthouse", 45.0, 7.5),
            ("villa", 45.0, 13.5),
            ("watchtower", 35.0, 9.5),
            ("ruined_temple", 45.0, 14.5),
        ],
        Tundra => &[
            ("stone_circle", 40.0, 10.0),
            ("watchtower", 35.0, 9.5),
            ("ruined_temple", 45.0, 14.5),
            ("observatory", 40.0, 4.5),
        ],
    }
}

/// Landform-driven extra candidates appended to the biome pool. Each
/// repetition is one extra lottery ticket, so archipelagos double down
/// on lighthouses without excluding the biome's own picks.
fn landform_extra(landform: LandformArchetype) -> &'static [Candidate] {
    match landform {
        LandformArchetype::Archipelago => &[("lighthouse", 45.0, 7.5), ("lighthouse", 45.0, 7.5)],
        LandformArchetype::Mesa => &[("observatory", 40.0, 4.5)],
        _ => &[],
    }
}

impl Landmark {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ LANDMARK_STREAM_SALT);

        let pool = biome_pool(scene.biome);
        let extra = landform_extra(scene.landform);
        let total = pool.len() + extra.len();
        let idx = ((unit_f32(&mut rng) * total as f32) as usize).min(total - 1);
        let (slug, min_dist, clearance) = if idx < pool.len() {
            pool[idx]
        } else {
            extra[idx - pool.len()]
        };

        // Position: seeded compass angle at a structure-appropriate
        // distance band.
        let angle = unit_f32(&mut rng) * std::f32::consts::TAU;
        let dist = range_f32(&mut rng, min_dist, min_dist + 30.0);
        let offset = [angle.sin() * dist, angle.cos() * dist];

        // Face the spawn origin (±0.35 rad jitter): rotating by
        // atan2(x, z) points the structure's local -Z back at the
        // origin.
        let yaw_rad = offset[0].atan2(offset[1]) + range_f32(&mut rng, -0.35, 0.35);

        let scale = range_f32(&mut rng, 0.85, 1.20);
        let grammar_seed = rng.next_u64();

        Self {
            slug,
            offset,
            yaw_rad,
            scale,
            grammar_seed,
            clearance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(11);
        let a = Landmark::from_scene(&scene, 11);
        let b = Landmark::from_scene(&scene, 11);
        assert_eq!(a.slug, b.slug);
        assert_eq!(a.offset, b.offset);
        assert_eq!(a.yaw_rad, b.yaw_rad);
        assert_eq!(a.grammar_seed, b.grammar_seed);
    }

    #[test]
    fn slug_resolves_in_catalogue_for_every_combo() {
        for landform in LandformArchetype::ALL {
            for biome in BiomeArchetype::ALL {
                for s in 0u64..8 {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    let lm = Landmark::from_scene(&scene, s);
                    assert!(
                        crate::catalogue::by_slug(lm.slug).is_some(),
                        "landmark slug {} not in catalogue",
                        lm.slug
                    );
                }
            }
        }
    }

    #[test]
    fn distance_respects_structure_minimum() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..32 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let lm = Landmark::from_scene(&scene, s);
                let dist = (lm.offset[0].powi(2) + lm.offset[1].powi(2)).sqrt();
                let pool_min = biome_pool(biome)
                    .iter()
                    .chain(landform_extra(scene.landform))
                    .filter(|(slug, _, _)| *slug == lm.slug)
                    .map(|(_, d, _)| *d)
                    .next()
                    .expect("picked slug must come from the pool");
                assert!(
                    dist >= pool_min - 1e-3,
                    "{biome:?}/{}: landmark at {dist} m, min {pool_min}",
                    lm.slug
                );
                assert!((0.85..=1.20).contains(&lm.scale));
            }
        }
    }

    #[test]
    fn coastal_rooms_lean_lighthouse() {
        let mut lighthouse = 0;
        for s in 0u64..128 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.biome = BiomeArchetype::Coastal;
            if Landmark::from_scene(&scene, s).slug == "lighthouse" {
                lighthouse += 1;
            }
        }
        assert!(
            lighthouse > 10,
            "coastal pool should surface lighthouses regularly (got {lighthouse}/128)"
        );
    }
}
