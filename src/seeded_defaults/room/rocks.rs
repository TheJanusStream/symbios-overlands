//! Seeded boulder-scatter specs.
//!
//! The ground-detail layer trees can't provide: weathered boulders
//! strewn across the region, biased by landform (craggy and mesa
//! rooms are stonier than rolling meadows) and biome (volcanic and
//! arid pile up more exposed rock than lush turf). Each room rolls
//! one or two scatters of a single per-room boulder design — a
//! low-resolution icosphere with seeded taper/twist irregularity so
//! it reads as a hewn rock rather than a geodesic ball.
//!
//! The wiring layer ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! builds the boulder generator (colouring it from the room palette's
//! rock channels) and emits one `Placement::Scatter` per spec,
//! filtered to dirt-and-rock ground above water.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, range_f32, unit_f32,
};

/// Sub-stream salt distinct from every sibling room deriver.
const ROCK_STREAM_SALT: u64 = 0x0C4B_0C4B_0C4B_0C4B;

/// One seeded boulder scatter.
#[derive(Clone, Copy, Debug)]
pub struct RockScatter {
    /// Instances to place (the biome filter drops some samples).
    pub count: u32,
    /// Scatter circle centre in world XZ.
    pub center: [f32; 2],
    /// Scatter circle radius in world units.
    pub radius: f32,
    /// Per-scatter RNG seed for `Placement::Scatter::local_seed`.
    pub local_seed: u64,
}

/// Per-room boulder design + scatter list.
#[derive(Clone, Debug, Default)]
pub struct RockScatters {
    pub scatters: Vec<RockScatter>,
    /// Boulder base radius (m).
    pub boulder_radius: f32,
    /// Vertex-torture taper — leans the boulder into a crag.
    pub boulder_taper: f32,
    /// Vertex-torture twist (radians) — shears the facets so the
    /// silhouette stops reading as a perfect icosphere.
    pub boulder_twist: f32,
}

impl RockScatters {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ ROCK_STREAM_SALT);
        derive(scene, &mut rng, room_seed)
    }
}

/// Boulder-count band per scatter, biased stony by landform and dry
/// by biome.
fn count_range(scene: &SceneCharacter) -> (u32, u32) {
    let (lo, hi) = match scene.landform {
        LandformArchetype::Craggy | LandformArchetype::Mesa => (14, 30),
        LandformArchetype::Valleys | LandformArchetype::Archipelago => (8, 20),
        LandformArchetype::Rolling => (5, 14),
    };
    match scene.biome {
        // Bare, stony, erosion-strewn ground piles up exposed rock.
        BiomeArchetype::Volcanic | BiomeArchetype::Arid | BiomeArchetype::Badlands => {
            (lo + 4, hi + 8)
        }
        // Dense turf / canopy / standing water hides the boulders.
        BiomeArchetype::Lush
        | BiomeArchetype::Jungle
        | BiomeArchetype::Wetland
        | BiomeArchetype::Meadow => (lo.saturating_sub(2), hi.saturating_sub(4).max(lo)),
        _ => (lo, hi),
    }
}

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng, room_seed: u64) -> RockScatters {
    // Stony rooms roll two independent fields, soft rooms one.
    let scatter_count = match scene.landform {
        LandformArchetype::Craggy | LandformArchetype::Mesa => 2,
        _ => 1,
    };

    let (lo, hi) = count_range(scene);
    let mut scatters = Vec::with_capacity(scatter_count);
    for i in 0..scatter_count {
        let count = lo + (unit_f32(rng) * (hi - lo + 1) as f32) as u32;
        scatters.push(RockScatter {
            count: count.min(hi),
            center: [range_f32(rng, -150.0, 150.0), range_f32(rng, -150.0, 150.0)],
            radius: range_f32(rng, 280.0, 420.0),
            local_seed: room_seed
                .wrapping_mul(0xD134_2543_DE82_EF95)
                .wrapping_add((i as u64).wrapping_mul(ROCK_STREAM_SALT)),
        });
    }

    RockScatters {
        scatters,
        boulder_radius: range_f32(rng, 0.6, 1.6),
        boulder_taper: range_f32(rng, 0.10, 0.40),
        boulder_twist: range_f32(rng, 0.2, 0.9),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(5);
        let a = RockScatters::from_scene(&scene, 5);
        let b = RockScatters::from_scene(&scene, 5);
        assert_eq!(a.scatters.len(), b.scatters.len());
        assert_eq!(a.boulder_radius, b.boulder_radius);
        for (x, y) in a.scatters.iter().zip(b.scatters.iter()) {
            assert_eq!(x.count, y.count);
            assert_eq!(x.center, y.center);
            assert_eq!(x.local_seed, y.local_seed);
        }
    }

    #[test]
    fn fields_in_range_across_combos() {
        for landform in LandformArchetype::ALL {
            for biome in BiomeArchetype::ALL {
                for s in 0u64..8 {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    let r = RockScatters::from_scene(&scene, s);
                    assert!(!r.scatters.is_empty(), "every room gets some rocks");
                    assert!(r.scatters.len() <= 2);
                    for sc in &r.scatters {
                        assert!(sc.count >= 1 && sc.count <= 40, "count {} OOR", sc.count);
                        assert!((280.0..=420.0).contains(&sc.radius));
                    }
                    assert!((0.6..=1.6).contains(&r.boulder_radius));
                    assert!((0.10..=0.40).contains(&r.boulder_taper));
                    assert!((0.2..=0.9).contains(&r.boulder_twist));
                }
            }
        }
    }

    #[test]
    fn craggy_stonier_than_rolling() {
        let mut craggy_total = 0u32;
        let mut rolling_total = 0u32;
        for s in 0u64..64 {
            let mut c = SceneCharacter::for_seed(s);
            c.landform = LandformArchetype::Craggy;
            craggy_total += RockScatters::from_scene(&c, s)
                .scatters
                .iter()
                .map(|x| x.count)
                .sum::<u32>();

            let mut r = SceneCharacter::for_seed(s);
            r.landform = LandformArchetype::Rolling;
            rolling_total += RockScatters::from_scene(&r, s)
                .scatters
                .iter()
                .map(|x| x.count)
                .sum::<u32>();
        }
        assert!(
            craggy_total > rolling_total,
            "craggy ({craggy_total}) should out-rock rolling ({rolling_total})"
        );
    }
}
