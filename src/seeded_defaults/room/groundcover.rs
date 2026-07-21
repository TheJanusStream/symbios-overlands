//! Seeded ground-cover scatter specs (#911).
//!
//! The tier below the trees: grass tufts, wildflowers, ferns, reeds, dwarf
//! shrubs and the encrusting moss / lichen cushions. Where a tree scatter
//! places tens of instances of an expensive grammar, a ground-cover scatter
//! places hundreds of a two-entity card prop — so the biome reads as *covered*
//! rather than as bare splat colour with trees standing on it.
//!
//! Species come from a biome-weighted pool, exactly as
//! [`scatters`](super::scatters) does for trees, and the per-scatter instance
//! count comes from a named density band so WS7 can retune the whole tier from
//! one place (see [`DENSITY_SPARSE`] and friends).
//!
//! The wiring layer ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! turns each spec into one catalogue-built generator plus a matching
//! `Placement::Scatter`, and fits both vegetation tiers into the shared
//! room-wide entity budget.
//!
//! Two deliberate gaps, both deferred:
//!
//! * **Glacial stays lifeless.** Its count range is `(0, 0)`, so the pool is
//!   never indexed.
//! * **Reeds are not shoreline-bound.** `WaterRelation` is a half-space test,
//!   not a band, so there is no "within N metres of the waterline" predicate
//!   to place them against. Wetland terrain near the water is low-lying, so an
//!   ordinary above-water scatter lands them plausibly; a true shoreline band
//!   is WS6 work (#914).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::{Fp, ScatterNaturalness};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt distinct from every sibling room deriver — sharing one
/// would correlate the ground-cover layout with the tree or boulder layout.
const GROUNDCOVER_STREAM_SALT: u64 = 0x6D05_6D05_6D05_6D05;

/// Per-placement local seed offset, mixed with the scatter index so each
/// scatter samples an independent instance layout.
const GROUNDCOVER_LOCAL_SEED_SALT: u64 = 0x51E7_51E7_51E7_51E7;

// --- density bands ---------------------------------------------------------
//
// Instances per scatter, inclusive. The epic's standing decision is
// "sparse-but-everywhere" for v1, tuned up in WS7 once the perf picture is
// measured — these four constants are that dial, and nothing else in the tier
// hardcodes a count.

/// Harsh ground: the odd survivor clinging on.
pub const DENSITY_SPARSE: (u32, u32) = (60, 120);
/// Ordinary cover — most biomes sit here.
pub const DENSITY_MODERATE: (u32, u32) = (140, 260);
/// Verdant floor: jungle understory, meadow turf, wetland reed beds.
pub const DENSITY_LUSH: (u32, u32) = (260, 480);
/// Nothing grows.
pub const DENSITY_NONE: (u32, u32) = (0, 0);

/// Ground-cover species — each maps onto one of the catalogue's `gc_*`
/// card / cushion props.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundCoverSpecies {
    /// Crossed-card grass clump — the workhorse.
    GrassTuft,
    /// Sun-bleached grass for arid and savanna ground.
    DryGrassTuft,
    /// Grass tuft carrying a blossom.
    Wildflower,
    /// Low frond rosette for shaded floors.
    FernClump,
    /// Tall cattail reeds for wetland margins.
    ReedClump,
    /// Low woody cushion for tundra and alpine.
    DwarfShrub,
    /// Velvet moss cushion.
    MossPatch,
    /// Crustose lichen cushion — the tundra ground cover.
    LichenPatch,
}

impl GroundCoverSpecies {
    /// Catalogue slug for [`crate::catalogue::by_slug`].
    pub fn slug(self) -> &'static str {
        match self {
            Self::GrassTuft => "gc_grass_tuft",
            Self::DryGrassTuft => "gc_dry_grass_tuft",
            Self::Wildflower => "gc_wildflower",
            Self::FernClump => "gc_fern_clump",
            Self::ReedClump => "gc_reed_clump",
            Self::DwarfShrub => "gc_dwarf_shrub",
            Self::MossPatch => "gc_moss_patch",
            Self::LichenPatch => "gc_lichen_patch",
        }
    }

    /// Placement naturalness for this species' scatters (#912).
    ///
    /// Ground cover is where these dials show most — it is the tier with
    /// the instance count — and the differences between species are real
    /// botany rather than decoration:
    ///
    /// * **Clumping** tracks how the plant spreads. Rhizomatous and
    ///   colonising growth (reeds, moss, lichen) arrives in dense mats with
    ///   bare ground between; seed-dispersed tufts and flowers are patchy
    ///   but far less so.
    /// * **Slope cutoff** tracks what the plant can hold onto. Soil-rooted
    ///   cover gives up on a steep face well before an encrusting moss or
    ///   lichen does — those two *prefer* the rock the others can't take.
    /// * **Tilt** is generous throughout: a card prop standing perfectly
    ///   plumb is the single most obvious tell that a field was stamped.
    pub fn naturalness(self) -> ScatterNaturalness {
        let (clumping, tilt, max_slope_deg) = match self {
            // Rhizome mats — the densest clumping in the tier.
            Self::ReedClump => (0.72, 0.10, 26.0),
            // Encrusting colonies that spread from a hold: very clumped,
            // and the only cover that belongs on a steep face.
            Self::MossPatch => (0.70, 0.06, 58.0),
            Self::LichenPatch => (0.68, 0.05, 62.0),
            // Shade-followers: patchy with the light gaps.
            Self::FernClump => (0.58, 0.13, 34.0),
            // Woody cushions, spaced by competition for thin soil.
            Self::DwarfShrub => (0.44, 0.09, 40.0),
            // Seed-dispersed: drifts rather than mats.
            Self::Wildflower => (0.50, 0.16, 32.0),
            Self::GrassTuft => (0.55, 0.15, 36.0),
            Self::DryGrassTuft => (0.48, 0.15, 38.0),
        };
        ScatterNaturalness {
            clumping: Fp(clumping),
            // Soft rim on every patch: the tier's whole job is to read as
            // continuous, and overlapping patches only blend if their
            // edges are not circular cutouts.
            edge_falloff: Fp(1.2),
            // ≈0.82×–1.22×. Cards are flat, so size is most of what
            // distinguishes one instance from the next.
            scale_jitter: Fp(0.2),
            tilt_jitter: Fp(tilt),
            max_slope_deg: Some(Fp(max_slope_deg)),
        }
    }
}

use GroundCoverSpecies as S;

// Biome-weighted pools. Repetition is weighting, matching the tree-pool
// idiom; these are `const` items rather than inline literals because a slice
// built from const expressions is not promoted to `'static`.

const POOL_LUSH: &[GroundCoverSpecies] = &[S::GrassTuft, S::GrassTuft, S::Wildflower, S::FernClump];

const POOL_COASTAL: &[GroundCoverSpecies] =
    &[S::GrassTuft, S::GrassTuft, S::DryGrassTuft, S::ReedClump];

const POOL_ALPINE: &[GroundCoverSpecies] =
    &[S::GrassTuft, S::DwarfShrub, S::MossPatch, S::LichenPatch];

// Tundra gets the lichen-and-dwarf-shrub cover the epic calls for.
const POOL_TUNDRA: &[GroundCoverSpecies] =
    &[S::LichenPatch, S::LichenPatch, S::DwarfShrub, S::MossPatch];

const POOL_ARID: &[GroundCoverSpecies] = &[S::DryGrassTuft, S::DryGrassTuft];

const POOL_VOLCANIC: &[GroundCoverSpecies] = &[S::DryGrassTuft];

// Jungle floor is fern-dominated, mossed in the damp.
const POOL_JUNGLE: &[GroundCoverSpecies] =
    &[S::FernClump, S::FernClump, S::GrassTuft, S::MossPatch];

const POOL_TEMPERATE_FOREST: &[GroundCoverSpecies] = &[
    S::GrassTuft,
    S::GrassTuft,
    S::FernClump,
    S::Wildflower,
    S::MossPatch,
];

// Mossy taiga floor under the conifers.
const POOL_BOREAL: &[GroundCoverSpecies] = &[
    S::MossPatch,
    S::MossPatch,
    S::GrassTuft,
    S::DwarfShrub,
    S::LichenPatch,
];

const POOL_WETLAND: &[GroundCoverSpecies] =
    &[S::ReedClump, S::ReedClump, S::GrassTuft, S::MossPatch];

// Wildflower-heavy, per the epic's meadow decision.
const POOL_MEADOW: &[GroundCoverSpecies] =
    &[S::Wildflower, S::Wildflower, S::GrassTuft, S::GrassTuft];

const POOL_SAVANNA: &[GroundCoverSpecies] = &[S::DryGrassTuft, S::DryGrassTuft, S::GrassTuft];

const POOL_BADLANDS: &[GroundCoverSpecies] = &[S::DryGrassTuft];

/// Never indexed — Glacial's count range is `(0, 0)`.
const POOL_GLACIAL: &[GroundCoverSpecies] = &[S::LichenPatch];

fn species_pool(biome: BiomeArchetype) -> &'static [GroundCoverSpecies] {
    match biome {
        BiomeArchetype::Lush => POOL_LUSH,
        BiomeArchetype::Coastal => POOL_COASTAL,
        BiomeArchetype::Alpine => POOL_ALPINE,
        BiomeArchetype::Tundra => POOL_TUNDRA,
        BiomeArchetype::Arid => POOL_ARID,
        BiomeArchetype::Volcanic => POOL_VOLCANIC,
        BiomeArchetype::Jungle => POOL_JUNGLE,
        BiomeArchetype::TemperateForest => POOL_TEMPERATE_FOREST,
        BiomeArchetype::Boreal => POOL_BOREAL,
        BiomeArchetype::Wetland => POOL_WETLAND,
        BiomeArchetype::Meadow => POOL_MEADOW,
        BiomeArchetype::Savanna => POOL_SAVANNA,
        BiomeArchetype::Badlands => POOL_BADLANDS,
        BiomeArchetype::Glacial => POOL_GLACIAL,
    }
}

/// How many ground-cover scatters a room rolls, inclusive. Higher than the
/// tree ranges — the props are two entities apiece, so several overlapping
/// patches are what "everywhere" costs.
fn count_range(biome: BiomeArchetype) -> (u32, u32) {
    match biome {
        BiomeArchetype::Glacial => (0, 0),
        BiomeArchetype::Volcanic | BiomeArchetype::Badlands => (1, 2),
        BiomeArchetype::Arid => (1, 3),
        BiomeArchetype::Tundra | BiomeArchetype::Alpine => (2, 4),
        BiomeArchetype::Jungle
        | BiomeArchetype::Lush
        | BiomeArchetype::Meadow
        | BiomeArchetype::Wetland => (4, 5),
        _ => (3, 5),
    }
}

/// Instances per scatter for this biome — see the density-band constants.
fn density_range(biome: BiomeArchetype) -> (u32, u32) {
    match biome {
        BiomeArchetype::Glacial => DENSITY_NONE,
        BiomeArchetype::Volcanic | BiomeArchetype::Badlands | BiomeArchetype::Arid => {
            DENSITY_SPARSE
        }
        BiomeArchetype::Jungle
        | BiomeArchetype::Lush
        | BiomeArchetype::Meadow
        | BiomeArchetype::Wetland => DENSITY_LUSH,
        _ => DENSITY_MODERATE,
    }
}

/// One seeded ground-cover scatter.
#[derive(Clone, Copy, Debug)]
pub struct GroundCoverScatter {
    /// Which catalogue ground-cover prop this scatter instantiates.
    pub species: GroundCoverSpecies,
    /// Instances to place. The biome filter drops some samples, so the
    /// rendered count is typically lower.
    pub count: u32,
    /// Scatter circle centre in world XZ.
    pub center: [f32; 2],
    /// Scatter circle radius in world units.
    pub radius: f32,
    /// Per-scatter RNG seed for `Placement::Scatter::local_seed`.
    pub local_seed: u64,
}

/// Full set of seeded ground-cover scatters for a room — empty on Glacial.
#[derive(Clone, Debug, Default)]
pub struct GroundCoverScatters {
    pub scatters: Vec<GroundCoverScatter>,
}

impl GroundCoverScatters {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ GROUNDCOVER_STREAM_SALT);
        derive(scene, &mut rng, room_seed)
    }
}

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng, room_seed: u64) -> GroundCoverScatters {
    let (lo, hi) = count_range(scene.biome);
    let n = sample_inclusive(rng, lo, hi);

    let pool = species_pool(scene.biome);
    let (dlo, dhi) = density_range(scene.biome);

    let mut scatters = Vec::with_capacity(n as usize);
    for i in 0..n {
        let species = pool[((unit_f32(rng) * pool.len() as f32) as usize).min(pool.len() - 1)];
        // Same 200 m centre box as the tree scatters, so a wide patch still
        // fits inside the playable terrain plane.
        let cx = range_f32(rng, -200.0, 200.0);
        let cz = range_f32(rng, -200.0, 200.0);
        // Wider than a tree stand: ground cover is meant to read as continuous
        // rather than as discrete clumps.
        let radius = range_f32(rng, 260.0, 460.0);
        let count = sample_inclusive(rng, dlo, dhi);
        let local_seed = room_seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((i as u64).wrapping_mul(GROUNDCOVER_LOCAL_SEED_SALT));
        scatters.push(GroundCoverScatter {
            species,
            count,
            center: [cx, cz],
            radius,
            local_seed,
        });
    }

    GroundCoverScatters { scatters }
}

/// `[lo, hi]` inclusive uniform sample, matching the sibling derivers'
/// inclusive-end convention.
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
        let a = GroundCoverScatters::from_scene(&scene, 42);
        let b = GroundCoverScatters::from_scene(&scene, 42);
        assert_eq!(a.scatters.len(), b.scatters.len());
        for (lhs, rhs) in a.scatters.iter().zip(b.scatters.iter()) {
            assert_eq!(lhs.species, rhs.species);
            assert_eq!(lhs.count, rhs.count);
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
                let gc = GroundCoverScatters::from_scene(&scene, s);
                let (lo, hi) = count_range(biome);
                assert!(
                    gc.scatters.len() as u32 >= lo && gc.scatters.len() as u32 <= hi,
                    "{biome:?} seed {s}: scatter count {} not in [{lo}, {hi}]",
                    gc.scatters.len()
                );
                let (dlo, dhi) = density_range(biome);
                for sc in &gc.scatters {
                    assert!(
                        species_pool(biome).contains(&sc.species),
                        "{biome:?} rolled out-of-pool species {:?}",
                        sc.species
                    );
                    assert!(
                        sc.count >= dlo && sc.count <= dhi,
                        "{biome:?} count {} outside density band [{dlo}, {dhi}]",
                        sc.count
                    );
                    assert!(sc.radius >= 260.0 && sc.radius <= 460.0);
                    assert!(sc.center[0].abs() <= 200.0 && sc.center[1].abs() <= 200.0);
                }
            }
        }
    }

    /// The epic's binding decision: Glacial stays lifeless.
    #[test]
    fn glacial_grows_nothing() {
        for s in 0u64..64 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.biome = BiomeArchetype::Glacial;
            assert!(
                GroundCoverScatters::from_scene(&scene, s)
                    .scatters
                    .is_empty(),
                "Glacial seed {s} grew ground cover"
            );
        }
    }

    /// Tundra's cover is the lichen / dwarf-shrub mat the epic specifies, not
    /// grass.
    #[test]
    fn tundra_cover_is_lichen_and_dwarf_shrub() {
        for sp in POOL_TUNDRA {
            assert!(
                matches!(sp, S::LichenPatch | S::DwarfShrub | S::MossPatch),
                "tundra pool should not contain {sp:?}"
            );
        }
    }

    /// Every pooled species must resolve in the catalogue, or the wiring layer
    /// would build nothing for it.
    #[test]
    fn every_pool_species_resolves_in_the_catalogue() {
        for biome in BiomeArchetype::ALL {
            for sp in species_pool(biome) {
                assert!(
                    crate::catalogue::by_slug(sp.slug()).is_some(),
                    "{biome:?} pools {:?} but slug `{}` is not registered",
                    sp,
                    sp.slug()
                );
            }
        }
    }

    /// A ground-cover scatter must not correlate with the tree scatter that
    /// shares the room seed — distinct stream salts are what keep the two
    /// layouts independent.
    #[test]
    fn layout_is_independent_of_the_tree_scatter() {
        let scene = SceneCharacter::for_seed(9);
        let gc = GroundCoverScatters::from_scene(&scene, 9);
        let trees = super::super::scatters::TreeScatters::from_scene(&scene, 9);
        for g in &gc.scatters {
            for t in &trees.scatters {
                assert_ne!(
                    g.local_seed, t.local_seed,
                    "ground cover shares a placement stream with a tree stand"
                );
            }
        }
    }
}
