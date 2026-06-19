//! Seeded tree-scatter specs.
//!
//! Emits 0–4 large-radius scatter specs per room, biased by biome —
//! lush / coastal rooms get a forested feel, arid / volcanic rooms
//! stay sparse, tundra / alpine sit in the middle. Each scatter picks
//! a [`TreeSpecies`] from a biome-weighted pool (conifers on alpine
//! ridges, gnarled gravity-bent trees in deserts, broadleaf mixes in
//! lush valleys), with its iteration count optionally bumped by ±1 so
//! two scatters of the same species read as different ages.
//!
//! The wiring layer ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did)) reads
//! these specs to build one named generator per scatter (so the
//! species and `iterations_delta` actually affect what gets compiled)
//! and emits a matching `Placement::Scatter` referencing each
//! generator with a grass-and-dirt-above-water biome filter.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt for the scatter deriver, distinct from palette /
/// terrain / textures / atmosphere so a future scatter-knob change
/// can't drift the rest of the room.
const SCATTER_STREAM_SALT: u64 = 0x5CA7_0000_5CA7_5CA7;

/// Per-placement local seed offset. Mixed with the scatter index so
/// each scatter has a deterministic but distinct RNG stream when the
/// world compiler samples instance positions.
const SCATTER_LOCAL_SEED_SALT: u64 = 0x7E55_7E55_7E55_7E55;

/// Tree species available to seeded scatters — each maps onto one of
/// the catalogue's L-system plant entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeSpecies {
    /// Broadleaf with foliage props (`lsys_ternary_props`).
    TernaryProps,
    /// Conifer-like single leader (`lsys_monopodial_tree`).
    Monopodial,
    /// Broad sympodial crown (`lsys_sympodial_tree`).
    Sympodial,
    /// Gnarled, gravity-bent silhouette (`lsys_ternary_gravity`).
    TernaryGravity,
    /// Columnar saguaro cactus (`lsys_cactus`) — desert succulent.
    Cactus,
    /// Leafless gnarled deadwood (`lsys_dead_shrub`) — dry / scorched scrub.
    DeadShrub,
    /// Tall bare trunk + frond crown (`lsys_palm`) — coastal / tropical.
    Palm,
    /// Stilt-rooted wetland tree (`lsys_mangrove`).
    Mangrove,
    /// Flat-crowned umbrella tree (`lsys_acacia`) — savanna.
    Acacia,
}

impl TreeSpecies {
    /// Catalogue slug for [`crate::catalogue::by_slug`].
    pub fn slug(self) -> &'static str {
        match self {
            Self::TernaryProps => "lsys_ternary_props",
            Self::Monopodial => "lsys_monopodial_tree",
            Self::Sympodial => "lsys_sympodial_tree",
            Self::TernaryGravity => "lsys_ternary_gravity",
            Self::Cactus => "lsys_cactus",
            Self::DeadShrub => "lsys_dead_shrub",
            Self::Palm => "lsys_palm",
            Self::Mangrove => "lsys_mangrove",
            Self::Acacia => "lsys_acacia",
        }
    }
}

/// Biome-weighted species pool. Repetition is weighting — lush rooms
/// roll broadleaf twice as often as conifer; tundra is conifer-only.
fn species_pool(biome: BiomeArchetype) -> &'static [TreeSpecies] {
    use TreeSpecies::*;
    match biome {
        BiomeArchetype::Lush => &[TernaryProps, TernaryProps, Sympodial, Monopodial],
        // Palms over the broadleaf shore (#491).
        BiomeArchetype::Coastal => &[Palm, Palm, Sympodial, TernaryProps],
        BiomeArchetype::Alpine => &[Monopodial, Monopodial, TernaryProps],
        BiomeArchetype::Tundra => &[Monopodial],
        // Saguaro + dead scrub over the odd gnarled survivor (#487).
        BiomeArchetype::Arid => &[Cactus, DeadShrub, TernaryGravity],
        // Scorched near-bare: deadwood + gnarled survivor (#490).
        BiomeArchetype::Volcanic => &[DeadShrub, TernaryGravity],
        // Tropical wall — palms over broadleaf, no conifer (#485).
        BiomeArchetype::Jungle => &[Palm, TernaryProps, TernaryProps, Sympodial],
        // Mixed broadleaf woodland, same blend as temperate Lush.
        BiomeArchetype::TemperateForest => &[TernaryProps, TernaryProps, Sympodial, Monopodial],
        // Conifer-dominant taiga.
        BiomeArchetype::Boreal => &[Monopodial, Monopodial, TernaryProps],
        // Stilt-rooted mangroves over a gnarled understory (#492).
        BiomeArchetype::Wetland => &[Mangrove, Mangrove, TernaryGravity, Sympodial],
        // Few trees over the grass — broad crowns where they stand.
        BiomeArchetype::Meadow => &[Sympodial, TernaryProps],
        // Scattered flat-crowned acacia + the odd gnarled survivor (#488).
        BiomeArchetype::Savanna => &[Acacia, Acacia, TernaryGravity],
        // Only the most stubborn dead scrub clings to the rock (#489).
        BiomeArchetype::Badlands => &[DeadShrub, TernaryGravity],
        // No vegetation; `count_range` keeps the count at zero so this
        // pool is never indexed.
        BiomeArchetype::Glacial => &[Monopodial],
    }
}

/// One seeded tree scatter — what the wiring layer turns into a
/// catalogue-built generator for [`TreeScatter::species`] plus a
/// matching `Placement::Scatter` referencing it.
#[derive(Clone, Copy, Debug)]
pub struct TreeScatter {
    /// Which catalogue plant this scatter instantiates.
    pub species: TreeSpecies,
    /// Added to `lsys_ternary_props`'s base iteration count. The
    /// deriver only samples `{-1, 0, +1}` — anything wider risks
    /// compile times spiking on a stray `+2` roll, or empty stubs on
    /// `-2`. The wiring layer is responsible for clamping the final
    /// `iterations` to its own minimum.
    pub iterations_delta: i32,
    /// How many tree instances to place. The scatter compiler may
    /// drop some samples to the biome filter, so the rendered count
    /// is typically lower than this.
    pub count: u32,
    /// Scatter circle centre in world XZ.
    pub center: [f32; 2],
    /// Scatter circle radius in world units.
    pub radius: f32,
    /// Per-scatter RNG seed handed to `Placement::Scatter::local_seed`.
    /// Distinct from `room_seed` so two scatters in the same room
    /// sample independent instance layouts.
    pub local_seed: u64,
}

/// Full set of seeded tree scatters for a room — empty for arid /
/// volcanic worlds on an unlucky roll, up to 4 entries for lush /
/// coastal worlds.
#[derive(Clone, Debug, Default)]
pub struct TreeScatters {
    pub scatters: Vec<TreeScatter>,
}

impl TreeScatters {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ SCATTER_STREAM_SALT);
        derive(scene, &mut rng, room_seed)
    }
}

/// Biome-weighted scatter-count range. Tighter at the dry / harsh
/// end, broader at the verdant end. Both bounds are inclusive.
fn count_range(biome: BiomeArchetype) -> (u32, u32) {
    match biome {
        // Densest canopies on the planet.
        BiomeArchetype::Jungle => (4, 4),
        BiomeArchetype::Lush
        | BiomeArchetype::Coastal
        | BiomeArchetype::TemperateForest
        | BiomeArchetype::Boreal => (3, 4),
        // Mangroves cluster but never fully forest the open water.
        BiomeArchetype::Wetland => (2, 4),
        BiomeArchetype::Alpine | BiomeArchetype::Tundra => (0, 2),
        // Open grassland with the odd stand of trees.
        BiomeArchetype::Savanna => (1, 3),
        BiomeArchetype::Meadow => (1, 2),
        BiomeArchetype::Arid | BiomeArchetype::Volcanic | BiomeArchetype::Badlands => (0, 1),
        // No trees on the ice.
        BiomeArchetype::Glacial => (0, 0),
    }
}

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng, room_seed: u64) -> TreeScatters {
    let (lo, hi) = count_range(scene.biome);
    let n = sample_inclusive(rng, lo, hi);

    let pool = species_pool(scene.biome);
    let mut scatters = Vec::with_capacity(n as usize);
    for i in 0..n {
        let species = pool[((unit_f32(rng) * pool.len() as f32) as usize).min(pool.len() - 1)];
        // Centre is held inside a 200 m square so a 300 m radius
        // scatter still fits comfortably inside the ~1024 m playable
        // terrain plane.
        let cx = range_f32(rng, -200.0, 200.0);
        let cz = range_f32(rng, -200.0, 200.0);
        let radius = range_f32(rng, 250.0, 400.0);
        let count = sample_inclusive(rng, 5, 50);
        let iterations_delta = match (unit_f32(rng) * 3.0) as u32 {
            0 => -1,
            1 => 0,
            _ => 1,
        };
        // Mix the scatter index back into the seed so two scatters in
        // the same room don't share a placement stream.
        let local_seed = room_seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((i as u64).wrapping_mul(SCATTER_LOCAL_SEED_SALT));
        scatters.push(TreeScatter {
            species,
            iterations_delta,
            count,
            center: [cx, cz],
            radius,
            local_seed,
        });
    }

    TreeScatters { scatters }
}

/// `[lo, hi]` inclusive uniform sample. Mirrors the inclusive-end
/// convention used by `sample_u32` in the sibling derivers.
fn sample_inclusive(rng: &mut ChaCha8Rng, lo: u32, hi: u32) -> u32 {
    if lo >= hi {
        return lo;
    }
    let span = (hi - lo) + 1;
    let v = (unit_f32(rng) * span as f32) as u32;
    lo + v.min(hi - lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(42);
        let a = TreeScatters::from_scene(&scene, 42);
        let b = TreeScatters::from_scene(&scene, 42);
        assert_eq!(a.scatters.len(), b.scatters.len());
        for (lhs, rhs) in a.scatters.iter().zip(b.scatters.iter()) {
            assert_eq!(lhs.species, rhs.species);
            assert_eq!(lhs.count, rhs.count);
            assert_eq!(lhs.iterations_delta, rhs.iterations_delta);
            assert_eq!(lhs.center, rhs.center);
            assert_eq!(lhs.radius, rhs.radius);
            assert_eq!(lhs.local_seed, rhs.local_seed);
        }
    }

    #[test]
    fn fields_in_range_across_biomes() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..32 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let ts = TreeScatters::from_scene(&scene, s);
                let (lo, hi) = count_range(biome);
                assert!(
                    ts.scatters.len() as u32 >= lo && ts.scatters.len() as u32 <= hi,
                    "{biome:?} seed {s}: scatter count {} not in [{lo}, {hi}]",
                    ts.scatters.len()
                );
                for sc in &ts.scatters {
                    assert!(
                        species_pool(biome).contains(&sc.species),
                        "{biome:?} rolled out-of-pool species {:?}",
                        sc.species
                    );
                    assert!(sc.count >= 5 && sc.count <= 50, "count {} OOR", sc.count);
                    assert!(
                        sc.iterations_delta >= -1 && sc.iterations_delta <= 1,
                        "iter delta {} OOR",
                        sc.iterations_delta
                    );
                    assert!(sc.radius >= 250.0 && sc.radius <= 400.0);
                    assert!(sc.center[0].abs() <= 200.0 && sc.center[1].abs() <= 200.0);
                }
            }
        }
    }

    #[test]
    fn every_pool_species_resolves_in_the_catalogue() {
        // A scatter references its species by slug; if a pool names a species
        // whose catalogue plant isn't registered, the wiring layer would
        // build nothing. Guard every species used by every biome's pool.
        for biome in BiomeArchetype::ALL {
            for sp in species_pool(biome) {
                assert!(
                    crate::catalogue::by_slug(sp.slug()).is_some(),
                    "{biome:?} pool species {sp:?} (slug {}) has no catalogue entry",
                    sp.slug()
                );
            }
        }
    }

    #[test]
    fn lush_more_forested_than_arid() {
        // Across many seeds, lush rooms should average a higher
        // scatter count than arid rooms — the biome bias is the whole
        // point of `count_range`.
        let mut lush_total = 0u32;
        let mut arid_total = 0u32;
        for s in 0u64..64 {
            let mut lush = SceneCharacter::for_seed(s);
            lush.biome = BiomeArchetype::Lush;
            lush_total += TreeScatters::from_scene(&lush, s).scatters.len() as u32;

            let mut arid = SceneCharacter::for_seed(s);
            arid.biome = BiomeArchetype::Arid;
            arid_total += TreeScatters::from_scene(&arid, s).scatters.len() as u32;
        }
        assert!(
            lush_total > arid_total,
            "lush should average more scatters than arid (lush={lush_total} arid={arid_total})"
        );
    }

    /// Diagnostic-only: enumerate candidate DIDs the local user might be
    /// authenticated as and dump the biome + scatter count `default_for_did`
    /// would produce for each. Run with
    /// `cargo test --lib seeded_defaults::room::scatters::tests::dump_local_did_scatters -- --nocapture`.
    /// Not an assertion test — exists so we can verify wiring deterministically
    /// when a freshly-seeded room "appears" empty of trees.
    #[test]
    fn dump_local_did_scatters() {
        use crate::seeded_defaults::hash::fnv1a_64;
        let candidates = [
            "",
            "did:plc:thejanusstream",
            "did:plc:janus",
            "thejanusstream@gmail.com",
            "thejanusstream.bsky.social",
            "TheJanusStream",
        ];
        for did in candidates {
            let seed = fnv1a_64(did);
            let scene = SceneCharacter::for_seed(seed);
            let ts = TreeScatters::from_scene(&scene, seed);
            let (lo, hi) = count_range(scene.biome);
            println!(
                "DID {did:?} → biome={:?} landform={:?} scatters={} (allowed range [{lo}, {hi}])",
                scene.biome,
                scene.landform,
                ts.scatters.len()
            );
            for (i, sc) in ts.scatters.iter().enumerate() {
                println!(
                    "  [{i}] count={} iter_delta={} center=({:.1}, {:.1}) radius={:.1}",
                    sc.count, sc.iterations_delta, sc.center[0], sc.center[1], sc.radius
                );
            }
        }
    }

    #[test]
    fn each_scatter_has_distinct_local_seed() {
        // Two scatters in the same room must not share a placement
        // stream — otherwise the world compiler would lay them down on
        // identical sample positions.
        for s in 0u64..32 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.biome = BiomeArchetype::Lush;
            let ts = TreeScatters::from_scene(&scene, s);
            for i in 0..ts.scatters.len() {
                for j in (i + 1)..ts.scatters.len() {
                    assert_ne!(ts.scatters[i].local_seed, ts.scatters[j].local_seed);
                }
            }
        }
    }
}
