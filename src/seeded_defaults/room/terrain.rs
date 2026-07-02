//! Seeded terrain shape: heightmap algorithm + continuous knobs +
//! per-biome splat rules.
//!
//! Picks the [`LandformArchetype`] from the scene character first, then
//! samples the continuous heightmap knobs (octaves, persistence,
//! erosion intensity, height scale, talus angle) within
//! archetype-appropriate ranges. This is the "archetype-gated" sampling
//! strategy that keeps "rolling hills with crazy erosion" or "flat
//! archipelago with mesa terraces" from ever occurring.
//!
//! Splat rules (where each biome layer appears on the slope/height
//! surface) are biased by [`BiomeArchetype`] in a sibling table: alpine
//! pulls the snow line down, arid pushes it up to "effectively off",
//! volcanic narrows grass, tundra widens snow.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, pick, range_f32,
};

/// Sub-stream salt for terrain-shape sampling. Distinct from
/// [`super::palette`]'s salt so the palette deriver and the shape
/// deriver advance independently — changing one cannot drift the other.
const TERRAIN_SHAPE_STREAM_SALT: u64 = 0x5EED_5AFE_E000_0000;

/// Heightmap algorithm. Mirrors [`crate::pds::SovereignGeneratorKind`]
/// so the `apply_shape_to_terrain_config` mapping is one-to-one with no
/// translation table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    FbmNoise,
    DiamondSquare,
    VoronoiTerracing,
}

/// One splat rule (where biome layer `i` appears in normalised
/// height/slope space). Mirrors [`crate::pds::SovereignSplatRule`].
#[derive(Clone, Copy, Debug)]
pub struct SplatRule {
    pub height_min: f32,
    pub height_max: f32,
    pub slope_min: f32,
    pub slope_max: f32,
    pub sharpness: f32,
}

/// Full seeded terrain shape — every field maps onto its namesake in
/// [`crate::pds::SovereignTerrainConfig`] / `SovereignMaterialConfig`.
#[derive(Clone, Debug)]
pub struct TerrainShape {
    pub generator_kind: GeneratorKind,
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    pub base_frequency: f32,
    pub ds_roughness: f32,
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,
    pub height_scale: f32,
    pub cell_scale: f32,
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub erosion_rate: f32,
    pub deposition_rate: f32,
    pub capacity_factor: f32,
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: f32,

    /// Per-layer splat rule (R Grass, G Dirt, B Rock, A Snow).
    pub splat_rules: [SplatRule; 4],

    /// Sea-level altitude expressed as a fraction of [`Self::
    /// height_scale`]. Lives on `TerrainShape` rather than the
    /// atmosphere deriver because the meaningful coordinate system
    /// is the terrain's own (a "30 % submerged" room reads the same
    /// at any seeded height scale). Biased by landform (archipelago
    /// pushes water high so islands stand proud, mesa/craggy pull it
    /// low) and by biome (arid / volcanic dry it down a notch). The
    /// caller multiplies by `height_scale` to get the world-Y the
    /// Water generator's placement transform should sit at.
    pub water_level_fraction: f32,
}

impl TerrainShape {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ TERRAIN_SHAPE_STREAM_SALT);
        derive(scene, &mut rng)
    }
}

// ---------------------------------------------------------------------------
// Per-archetype profile tables
// ---------------------------------------------------------------------------

/// Landform-specific continuous-knob ranges. Each `(lo, hi)` pair is
/// the sampling band for that archetype; the deriver draws one value
/// per call from `range_f32(rng, lo, hi)`.
struct LandformProfile {
    /// Candidate heightmap algorithms; one is picked per room, with
    /// repetition acting as weighting. Mesa stays Voronoi-only (the
    /// terracing *is* the archetype); the noise-driven landforms mix
    /// FBM with Diamond-Square so two Rolling rooms can differ in
    /// macro character, not just in knob values.
    generator_kinds: &'static [GeneratorKind],
    /// Diamond-Square roughness band — only consumed when the picked
    /// algorithm is [`GeneratorKind::DiamondSquare`], but sampled
    /// unconditionally to keep the RNG stream stable.
    ds_roughness: (f32, f32),
    /// Total terrain amplitude (m). Rolling=low, Craggy=high.
    height_scale: (f32, f32),
    /// Hydraulic erosion drop count. Valleys=very high.
    erosion_drops: (u32, u32),
    /// Hydraulic erosion rate (per-drop dig strength).
    erosion_rate: (f32, f32),
    /// Thermal erosion sweep count. Craggy=high.
    thermal_iterations: (u32, u32),
    /// Talus repose angle (radians). Craggy=steep, Rolling=gentle.
    thermal_talus_angle: (f32, f32),
    /// FBM octaves. Higher = more fine detail.
    octaves: (u32, u32),
    /// FBM persistence (amplitude decay per octave).
    persistence: (f32, f32),
    /// FBM lacunarity (frequency growth per octave).
    lacunarity: (f32, f32),
    /// FBM base frequency.
    base_frequency: (f32, f32),
    /// Voronoi terrace count. Mesa=many, others=few.
    voronoi_num_terraces: (u32, u32),
    /// Voronoi seed point count.
    voronoi_num_seeds: (u32, u32),
    /// Water level as a fraction of `height_scale`. Archipelago is
    /// high (lots of islands), mesa/craggy/volcanic are low (mostly
    /// dry); the biome-side multiplier in [`derive()`] biases this
    /// further.
    water_level_fraction: (f32, f32),
}

fn landform_profile(l: LandformArchetype) -> LandformProfile {
    use GeneratorKind::*;
    use LandformArchetype::*;
    match l {
        Rolling => LandformProfile {
            generator_kinds: &[FbmNoise, FbmNoise, DiamondSquare],
            ds_roughness: (0.30, 0.45),
            height_scale: (25.0, 40.0),
            erosion_drops: (15_000, 35_000),
            erosion_rate: (0.15, 0.30),
            thermal_iterations: (10, 25),
            thermal_talus_angle: (0.04, 0.07),
            octaves: (4, 6),
            persistence: (0.40, 0.55),
            lacunarity: (1.9, 2.3),
            base_frequency: (2.5, 4.5),
            voronoi_num_terraces: (1, 2),
            voronoi_num_seeds: (600, 1200),
            water_level_fraction: (0.10, 0.25),
        },
        Craggy => LandformProfile {
            generator_kinds: &[FbmNoise, DiamondSquare],
            ds_roughness: (0.55, 0.75),
            height_scale: (60.0, 90.0),
            erosion_drops: (25_000, 55_000),
            erosion_rate: (0.30, 0.45),
            thermal_iterations: (40, 80),
            thermal_talus_angle: (0.07, 0.12),
            octaves: (6, 8),
            persistence: (0.50, 0.65),
            lacunarity: (2.0, 2.6),
            base_frequency: (4.0, 7.0),
            voronoi_num_terraces: (1, 2),
            voronoi_num_seeds: (800, 1600),
            water_level_fraction: (0.05, 0.18),
        },
        Mesa => LandformProfile {
            generator_kinds: &[VoronoiTerracing],
            ds_roughness: (0.40, 0.50),
            height_scale: (50.0, 75.0),
            erosion_drops: (8_000, 25_000),
            erosion_rate: (0.15, 0.30),
            thermal_iterations: (15, 35),
            thermal_talus_angle: (0.03, 0.06),
            octaves: (4, 6),
            persistence: (0.40, 0.55),
            lacunarity: (1.9, 2.4),
            base_frequency: (3.0, 5.5),
            voronoi_num_terraces: (3, 6),
            voronoi_num_seeds: (400, 900),
            water_level_fraction: (0.05, 0.15),
        },
        Archipelago => LandformProfile {
            generator_kinds: &[FbmNoise, FbmNoise, DiamondSquare],
            ds_roughness: (0.45, 0.60),
            height_scale: (35.0, 65.0),
            erosion_drops: (25_000, 50_000),
            erosion_rate: (0.20, 0.35),
            thermal_iterations: (15, 30),
            thermal_talus_angle: (0.04, 0.08),
            octaves: (5, 7),
            persistence: (0.45, 0.60),
            lacunarity: (2.0, 2.4),
            base_frequency: (2.0, 3.5),
            voronoi_num_terraces: (1, 2),
            voronoi_num_seeds: (600, 1200),
            water_level_fraction: (0.28, 0.48),
        },
        Valleys => LandformProfile {
            generator_kinds: &[FbmNoise, FbmNoise, DiamondSquare],
            ds_roughness: (0.45, 0.60),
            height_scale: (45.0, 70.0),
            erosion_drops: (60_000, 120_000),
            erosion_rate: (0.30, 0.45),
            thermal_iterations: (20, 40),
            thermal_talus_angle: (0.04, 0.07),
            octaves: (5, 7),
            persistence: (0.45, 0.60),
            lacunarity: (1.9, 2.3),
            base_frequency: (3.0, 5.0),
            voronoi_num_terraces: (1, 2),
            voronoi_num_seeds: (700, 1400),
            water_level_fraction: (0.12, 0.28),
        },
    }
}

/// Biome-specific splat thresholds. Each entry describes where on the
/// normalised height/slope grid that biome's layer reads as dominant.
struct BiomeSplatProfile {
    grass_height_max: f32,
    grass_slope_max: f32,
    dirt_height_min: f32,
    dirt_height_max: f32,
    rock_slope_min: f32,
    snow_height_min: f32,
    /// Default blend sharpness for the first three layers (Grass /
    /// Dirt / Rock). Snow keeps the high-sharpness default since a
    /// soft snow line reads as muddy.
    blend_sharpness: f32,
}

fn biome_splat_profile(b: BiomeArchetype) -> BiomeSplatProfile {
    use BiomeArchetype::*;
    match b {
        Lush => BiomeSplatProfile {
            grass_height_max: 0.45,
            grass_slope_max: 0.30,
            dirt_height_min: 0.25,
            dirt_height_max: 0.70,
            rock_slope_min: 0.25,
            // Snow rare in lush rooms — push the snow line up so it
            // only caps the highest peaks.
            snow_height_min: 0.92,
            blend_sharpness: 0.5,
        },
        Arid => BiomeSplatProfile {
            // Narrow grass; dirt dominates the slopes.
            grass_height_max: 0.25,
            grass_slope_max: 0.20,
            dirt_height_min: 0.15,
            dirt_height_max: 0.80,
            rock_slope_min: 0.20,
            // Effectively no snow.
            snow_height_min: 0.98,
            blend_sharpness: 0.6,
        },
        Alpine => BiomeSplatProfile {
            grass_height_max: 0.30,
            grass_slope_max: 0.25,
            dirt_height_min: 0.20,
            dirt_height_max: 0.55,
            rock_slope_min: 0.20,
            // Lots of snow — drop the snow line low.
            snow_height_min: 0.55,
            blend_sharpness: 0.4,
        },
        Volcanic => BiomeSplatProfile {
            // Sparse grass; rock dominates.
            grass_height_max: 0.20,
            grass_slope_max: 0.18,
            dirt_height_min: 0.10,
            dirt_height_max: 0.50,
            rock_slope_min: 0.18,
            snow_height_min: 0.95,
            blend_sharpness: 0.6,
        },
        Coastal => BiomeSplatProfile {
            grass_height_max: 0.45,
            grass_slope_max: 0.30,
            dirt_height_min: 0.30,
            dirt_height_max: 0.65,
            rock_slope_min: 0.30,
            snow_height_min: 0.95,
            blend_sharpness: 0.5,
        },
        Tundra => BiomeSplatProfile {
            // Snow-dominant: minimal grass, dirt sparse, snow line low.
            grass_height_max: 0.15,
            grass_slope_max: 0.15,
            dirt_height_min: 0.10,
            dirt_height_max: 0.35,
            rock_slope_min: 0.25,
            snow_height_min: 0.40,
            blend_sharpness: 0.35,
        },
        Jungle => BiomeSplatProfile {
            // Tropical-dense: grass climbs high up the slopes, dirt
            // shows through the canopy floor, snow only on the rare peak.
            grass_height_max: 0.55,
            grass_slope_max: 0.40,
            dirt_height_min: 0.30,
            dirt_height_max: 0.75,
            rock_slope_min: 0.30,
            snow_height_min: 0.95,
            blend_sharpness: 0.45,
        },
        TemperateForest => BiomeSplatProfile {
            // Mixed broadleaf: a wide grass/leaf-litter floor, dirt over
            // the mid slopes, snow capping the highest ground.
            grass_height_max: 0.48,
            grass_slope_max: 0.34,
            dirt_height_min: 0.28,
            dirt_height_max: 0.72,
            rock_slope_min: 0.28,
            snow_height_min: 0.88,
            blend_sharpness: 0.5,
        },
        Boreal => BiomeSplatProfile {
            // Cold conifer below the tree line: green floor, dirt mid,
            // snow line lower than the broadleaf forests but well above
            // the bare-tundra line.
            grass_height_max: 0.40,
            grass_slope_max: 0.30,
            dirt_height_min: 0.22,
            dirt_height_max: 0.62,
            rock_slope_min: 0.25,
            snow_height_min: 0.72,
            blend_sharpness: 0.45,
        },
        Wetland => BiomeSplatProfile {
            // Low, waterlogged ground: grass/reed mat hugs the flats,
            // dirt (peat mud) dominates the gentle rises, snow absent.
            grass_height_max: 0.40,
            grass_slope_max: 0.22,
            dirt_height_min: 0.18,
            dirt_height_max: 0.65,
            rock_slope_min: 0.35,
            snow_height_min: 0.96,
            blend_sharpness: 0.4,
        },
        Meadow => BiomeSplatProfile {
            // Rolling grassland: grass blankets nearly everything, dirt
            // and rock only where the ground breaks; effectively no snow.
            grass_height_max: 0.60,
            grass_slope_max: 0.38,
            dirt_height_min: 0.35,
            dirt_height_max: 0.78,
            rock_slope_min: 0.35,
            snow_height_min: 0.94,
            blend_sharpness: 0.5,
        },
        Savanna => BiomeSplatProfile {
            // Dry golden grass with bare patches: a broad but thinner
            // grass band over plenty of exposed dirt; no snow.
            grass_height_max: 0.42,
            grass_slope_max: 0.26,
            dirt_height_min: 0.18,
            dirt_height_max: 0.78,
            rock_slope_min: 0.24,
            snow_height_min: 0.97,
            blend_sharpness: 0.55,
        },
        Badlands => BiomeSplatProfile {
            // Eroded rock: almost no grass, dirt over the lower terraces,
            // rock takes over the steep faces; no snow.
            grass_height_max: 0.12,
            grass_slope_max: 0.14,
            dirt_height_min: 0.08,
            dirt_height_max: 0.55,
            rock_slope_min: 0.16,
            snow_height_min: 0.98,
            blend_sharpness: 0.65,
        },
        Glacial => BiomeSplatProfile {
            // Ice-dominant: no vegetation worth speaking of, dirt only on
            // the rare moraine, snow/ice line dropped to the valley floor.
            grass_height_max: 0.08,
            grass_slope_max: 0.12,
            dirt_height_min: 0.06,
            dirt_height_max: 0.28,
            rock_slope_min: 0.28,
            snow_height_min: 0.30,
            blend_sharpness: 0.35,
        },
    }
}

// ---------------------------------------------------------------------------
// Master deriver
// ---------------------------------------------------------------------------

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> TerrainShape {
    let lp = landform_profile(scene.landform);
    let bp = biome_splat_profile(scene.biome);

    let generator_kind = pick(lp.generator_kinds, rng);
    let octaves = sample_u32(rng, lp.octaves);
    let height_scale = sample_f32(rng, lp.height_scale);
    let erosion_drops = sample_u32(rng, lp.erosion_drops);
    let thermal_iterations = sample_u32(rng, lp.thermal_iterations);

    let splat_rules = [
        SplatRule {
            height_min: 0.0,
            height_max: jitter_around(rng, bp.grass_height_max, 0.05, 0.05, 0.95),
            slope_min: 0.0,
            slope_max: jitter_around(rng, bp.grass_slope_max, 0.05, 0.05, 0.95),
            sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
        },
        SplatRule {
            height_min: jitter_around(rng, bp.dirt_height_min, 0.04, 0.0, 0.95),
            height_max: jitter_around(rng, bp.dirt_height_max, 0.05, 0.1, 1.0),
            slope_min: 0.0,
            slope_max: jitter_around(rng, 0.55, 0.05, 0.2, 0.9),
            sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
        },
        SplatRule {
            height_min: 0.0,
            height_max: 1.0,
            slope_min: jitter_around(rng, bp.rock_slope_min, 0.04, 0.05, 0.6),
            slope_max: 1.0,
            sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
        },
        SplatRule {
            height_min: jitter_around(rng, bp.snow_height_min, 0.03, 0.05, 0.99),
            height_max: 1.0,
            slope_min: 0.0,
            slope_max: 1.0,
            // Snow keeps the sharp transition default — a soft snow
            // line reads as muddy on a stylised palette.
            sharpness: 4.0,
        },
    ];

    TerrainShape {
        generator_kind,
        octaves,
        persistence: sample_f32(rng, lp.persistence),
        lacunarity: sample_f32(rng, lp.lacunarity),
        base_frequency: sample_f32(rng, lp.base_frequency),
        // Sampled unconditionally (FBM rooms ignore it) so the RNG
        // stream doesn't shift between algorithm picks.
        ds_roughness: sample_f32(rng, lp.ds_roughness),
        voronoi_num_seeds: sample_u32(rng, lp.voronoi_num_seeds),
        voronoi_num_terraces: sample_u32(rng, lp.voronoi_num_terraces),
        height_scale,
        cell_scale: range_f32(rng, 1.7, 2.3),
        erosion_enabled: true,
        erosion_drops,
        erosion_rate: sample_f32(rng, lp.erosion_rate),
        deposition_rate: range_f32(rng, 0.25, 0.35),
        capacity_factor: range_f32(rng, 6.0, 10.0),
        thermal_enabled: true,
        thermal_iterations,
        thermal_talus_angle: sample_f32(rng, lp.thermal_talus_angle),
        splat_rules,
        water_level_fraction: derive_water_level_fraction(scene, rng, &lp),
    }
}

/// Sample the water level (fraction of `height_scale`) from the
/// landform's range, then bias it by biome. Arid / volcanic / tundra
/// dry the planet down; lush / coastal raise it. The final value is
/// clamped into a sane band so even adversarial multipliers can't
/// drown every terrain or strand all the water.
fn derive_water_level_fraction(
    scene: &SceneCharacter,
    rng: &mut ChaCha8Rng,
    lp: &LandformProfile,
) -> f32 {
    let base = sample_f32(rng, lp.water_level_fraction);
    let biome_mul = match scene.biome {
        BiomeArchetype::Arid | BiomeArchetype::Volcanic | BiomeArchetype::Badlands => 0.70,
        BiomeArchetype::Tundra | BiomeArchetype::Alpine | BiomeArchetype::Glacial => 0.85,
        BiomeArchetype::Savanna => 0.80,
        BiomeArchetype::Lush
        | BiomeArchetype::TemperateForest
        | BiomeArchetype::Boreal
        | BiomeArchetype::Meadow => 1.05,
        BiomeArchetype::Coastal | BiomeArchetype::Jungle | BiomeArchetype::Wetland => 1.15,
    };
    (base * biome_mul).clamp(0.02, 0.55)
}

// ---------------------------------------------------------------------------
// Local sampling helpers
// ---------------------------------------------------------------------------

fn sample_f32(rng: &mut ChaCha8Rng, (lo, hi): (f32, f32)) -> f32 {
    range_f32(rng, lo, hi)
}

fn sample_u32(rng: &mut ChaCha8Rng, (lo, hi): (u32, u32)) -> u32 {
    let (lo, hi) = (lo as f32, (hi + 1) as f32); // inclusive
    let v = range_f32(rng, lo, hi) as u32;
    v.clamp(lo as u32, hi as u32 - 1)
}

/// Symmetric jitter around `center` by `±span`, clamped to `[min, max]`.
fn jitter_around(rng: &mut ChaCha8Rng, center: f32, span: f32, min: f32, max: f32) -> f32 {
    (range_f32(rng, center - span, center + span)).clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(7);
        let a = TerrainShape::from_scene(&scene, 7);
        let b = TerrainShape::from_scene(&scene, 7);
        assert_eq!(a.octaves, b.octaves);
        assert_eq!(a.height_scale, b.height_scale);
        assert_eq!(a.splat_rules[0].height_max, b.splat_rules[0].height_max);
    }

    #[test]
    fn all_fields_in_finite_ranges_across_archetypes() {
        for landform in LandformArchetype::ALL {
            for biome in BiomeArchetype::ALL {
                for s in 0u64..3 {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    let t = TerrainShape::from_scene(&scene, s);

                    assert!(t.octaves >= 1 && t.octaves <= 16);
                    assert!(t.persistence > 0.0 && t.persistence < 1.0);
                    assert!(t.lacunarity > 1.0);
                    assert!(t.height_scale > 0.0);
                    assert!(t.erosion_drops > 0);

                    for (i, rule) in t.splat_rules.iter().enumerate() {
                        assert!(
                            (0.0..=1.0).contains(&rule.height_min),
                            "layer {i} height_min OOR: {rule:?}"
                        );
                        assert!(
                            (0.0..=1.0).contains(&rule.height_max),
                            "layer {i} height_max OOR: {rule:?}"
                        );
                        assert!(rule.height_min <= rule.height_max);
                        assert!(rule.sharpness > 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn noise_landforms_mix_algorithms_and_mesa_stays_voronoi() {
        let mut rolling_kinds = std::collections::HashSet::new();
        for s in 0u64..96 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.landform = LandformArchetype::Rolling;
            rolling_kinds.insert(TerrainShape::from_scene(&scene, s).generator_kind);

            let mut mesa = SceneCharacter::for_seed(s);
            mesa.landform = LandformArchetype::Mesa;
            assert_eq!(
                TerrainShape::from_scene(&mesa, s).generator_kind,
                GeneratorKind::VoronoiTerracing,
                "mesa must stay Voronoi-terraced"
            );
        }
        assert!(
            rolling_kinds.contains(&GeneratorKind::FbmNoise)
                && rolling_kinds.contains(&GeneratorKind::DiamondSquare),
            "rolling rooms should mix FBM and Diamond-Square; saw {rolling_kinds:?}"
        );
    }

    #[test]
    fn archipelago_floats_higher_than_mesa() {
        // Archipelago vessels need a high water line so the landforms
        // read as islands; mesa rooms keep water low so the plateaus
        // read as dry. Average across many seeds to dampen the
        // sampling jitter and the biome multiplier.
        let mut arch_total = 0.0;
        let mut mesa_total = 0.0;
        for s in 0u64..64 {
            let mut arch = SceneCharacter::for_seed(s);
            arch.landform = LandformArchetype::Archipelago;
            arch_total += TerrainShape::from_scene(&arch, s).water_level_fraction;

            let mut mesa = SceneCharacter::for_seed(s);
            mesa.landform = LandformArchetype::Mesa;
            mesa_total += TerrainShape::from_scene(&mesa, s).water_level_fraction;
        }
        assert!(
            arch_total > mesa_total,
            "archipelago water={arch_total} should sit above mesa={mesa_total}"
        );
    }

    #[test]
    fn water_level_fraction_in_safe_range() {
        // The clamp in `derive_water_level_fraction` should keep the
        // value well away from 0 (dry world) and 1 (drowned world).
        for landform in LandformArchetype::ALL {
            for biome in BiomeArchetype::ALL {
                for s in 0u64..4 {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    let f = TerrainShape::from_scene(&scene, s).water_level_fraction;
                    assert!((0.02..=0.55).contains(&f), "{landform:?}/{biome:?}: {f}");
                }
            }
        }
    }

    #[test]
    fn alpine_has_lower_snow_line_than_arid() {
        // Sanity-check the biome profile table: the same seed should
        // produce a lower snow line for alpine than for arid, modulo
        // jitter. Average across a few seeds to dampen the jitter.
        let mut alpine_total = 0.0;
        let mut arid_total = 0.0;
        for s in 0u64..32 {
            let mut alp = SceneCharacter::for_seed(s);
            alp.biome = BiomeArchetype::Alpine;
            alpine_total += TerrainShape::from_scene(&alp, s).splat_rules[3].height_min;

            let mut ari = SceneCharacter::for_seed(s);
            ari.biome = BiomeArchetype::Arid;
            arid_total += TerrainShape::from_scene(&ari, s).splat_rules[3].height_min;
        }
        assert!(
            alpine_total < arid_total,
            "alpine snow_min={alpine_total} should be lower than arid={arid_total}"
        );
    }
}
