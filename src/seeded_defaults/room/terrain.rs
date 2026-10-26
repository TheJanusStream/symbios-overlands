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
//! volcanic narrows grass, tundra widens snow. They are written for the
//! ground's plateau semantics (#1168, #1784): a band is full weight
//! across itself and fades over a skirt outside it, so the seeder keeps
//! the skirts narrow, leaves no height or slope that no layer covers, and
//! keeps rock off gentle ground and snow off cliffs
//! ([`SplatDraws::plateau`]).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::SovereignGeneratorKind;
use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, pick, range_f32,
};

/// Sub-stream salt for terrain-shape sampling. Distinct from
/// [`super::palette`]'s salt so the palette deriver and the shape
/// deriver advance independently - changing one cannot drift the other.
const TERRAIN_SHAPE_STREAM_SALT: u64 = 0x5EED_5AFE_E000_0000;

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

/// Full seeded terrain shape - every field maps onto its namesake in
/// [`crate::pds::SovereignTerrainConfig`] / `SovereignMaterialConfig`.
#[derive(Clone, Debug)]
pub struct TerrainShape {
    pub generator_kind: SovereignGeneratorKind,
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

    /// Per-layer splat rule (R Grass, G Dirt, B Rock, A Snow), written for
    /// the ground's plateau semantics (#1784).
    pub splat_rules: [SplatRule; 4],

    /// The same bands as they were written before #1784, which a square of
    /// Berlin keeps: its ground is painted from its land use (#1586), and
    /// its record's bytes are pinned.
    pub square_splat_rules: [SplatRule; 4],

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
    generator_kinds: &'static [SovereignGeneratorKind],
    /// Diamond-Square roughness band - only consumed when the picked
    /// algorithm is [`SovereignGeneratorKind::DiamondSquare`], but sampled
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
    /// Maximum stable height step between adjacent heightmap cells, in
    /// world metres - **not** an angle (the erosion runs after heights
    /// are scaled to metres; see `ThermalErosion::talus_angle` in
    /// `symbios-ground`). Cells sit ~2 m apart, so ~0.5 m ≈ a 15°
    /// repose slope and ~1.3 m ≈ 34° scree. Low values act as a strong
    /// smoothing dial (every slope sheds material each sweep); high
    /// values leave everything below a genuine cliff untouched. Rolling
    /// stays low (soft hills), Craggy/Mesa run high so peaks and
    /// terrace lips keep their edges and only over-steep faces shed
    /// scree (#904 - the old 0.03–0.12 bands mushed exactly the
    /// landforms whose identity is sharpness).
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
    use LandformArchetype::*;
    use SovereignGeneratorKind::*;
    match l {
        Rolling => LandformProfile {
            generator_kinds: &[FbmNoise, FbmNoise, DiamondSquare],
            ds_roughness: (0.30, 0.45),
            height_scale: (25.0, 40.0),
            erosion_drops: (15_000, 35_000),
            erosion_rate: (0.15, 0.30),
            thermal_iterations: (10, 25),
            thermal_talus_angle: (0.05, 0.10),
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
            thermal_talus_angle: (0.30, 0.50),
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
            thermal_iterations: (10, 20),
            thermal_talus_angle: (0.32, 0.50),
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
            thermal_talus_angle: (0.10, 0.20),
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
            thermal_talus_angle: (0.10, 0.20),
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

/// Biome-specific splat thresholds: the centres each world's bands are drawn
/// round. [`SplatDraws::plateau`] writes them for the ground's plateau
/// semantics (#1784): it keeps grass's height and slope, dirt's start and
/// slope, rock's slope and the snow line as drawn, within the bounds that
/// leave no ground uncovered, and runs dirt up to the snow line whatever its
/// drawn top.
struct BiomeSplatProfile {
    grass_height_max: f32,
    grass_slope_max: f32,
    dirt_height_min: f32,
    dirt_height_max: f32,
    rock_slope_min: f32,
    snow_height_min: f32,
    /// Blend sharpness for the first three layers (Grass / Dirt / Rock)
    /// as the tent semantics read it, before #1168: the centre of each
    /// world's draw, which the plateau rules turn into a sharpness about
    /// [`PLATEAU_SHARPNESS`] ([`SplatDraws::plateau`]). Snow keeps a
    /// sharper edge since a soft snow line reads as muddy.
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
            // Snow rare in lush rooms - push the snow line up so it
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
            // Lots of snow - drop the snow line low.
            snow_height_min: 0.55,
            blend_sharpness: 0.4,
        },
        Volcanic => BiomeSplatProfile {
            // Sparse grass; ash (dirt) over the slopes, rock on every steep
            // face.
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

    let splat = SplatDraws::draw(rng, &bp);

    let mut shape = TerrainShape {
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
        splat_rules: splat.as_drawn(),
        square_splat_rules: splat.as_drawn(),
        water_level_fraction: derive_water_level_fraction(scene, rng, &lp),
    };
    // Written once the water is drawn, last: the grass band reaches above it.
    shape.splat_rules = splat.plateau(&bp, shape.water_level_fraction);
    shape
}

/// The sharpness of a grass, dirt or rock band's edge under the plateau
/// semantics (#1168, #1784): a skirt a third of the band's half-width wide
/// beyond each end, as the ground's own defaults have it - a transition
/// rather than a wash. Each world's draw is kept as a ratio round it.
const PLATEAU_SHARPNESS: f32 = 2.0;

/// The snow band's edge sharpness: a snow line reads as a line.
const SNOW_SHARPNESS: f32 = 4.0;

/// The steepest ground snow settles on (slope, `1 - normal.y`): a cliff
/// above the snow line is rock's.
const SNOW_SLOPE_MAX: f32 = 0.40;

/// How far under the snow line the dirt band stops (a fraction of the height
/// scale): its skirt reaches over the gap, so no flat ground between them is
/// left to no layer, and neither plateau ties the other on the snow line -
/// a tie goes to the lower layer, and would raise the line for every reader
/// of the ground's largest layer.
const DIRT_UNDER_SNOW: f32 = 0.01;

/// How far under the snow line grass stops at the highest.
const GRASS_UNDER_SNOW: f32 = 0.02;

/// How far past rock's gentlest slope dirt's slope band reaches: a narrow
/// blend of the two, then rock alone. Dirt full on slopes rock's band also
/// covers would tie it there, and the tie goes to dirt - the lower layer.
const DIRT_OVER_ROCK: f32 = 0.05;

/// How far under the grass band's top the dirt band starts at the most.
const DIRT_UNDER_GRASS: f32 = 0.02;

/// How far above grass's steepest slope rock's band begins, at the least
/// and at the most: never abutting it - two plateaus that touch both read
/// full on the boundary, and which wins there is a matter of rounding - and
/// never so far that their skirts leave a slope no layer covers.
const ROCK_OVER_GRASS: (f32, f32) = (0.03, 0.10);

/// The gentlest slope rock's band begins at: below it rock's skirt would
/// reach level ground at the softest edge a world draws.
const ROCK_SLOPE_FLOOR: f32 = 0.2;

/// The share of its band's height grass reaches above the world's water at
/// the least: an archipelago's water can stand above a band written for a
/// low sea, and its islands would grow none.
const GRASS_OVER_WATER: f32 = 0.5;

/// The least grass band (its profile's top) a biome keeps a shore of grass
/// for ([`GRASS_OVER_WATER`]): a glacier's, a tundra's, a badland's or a
/// desert's few metres of grass are meant to drown.
const GRASS_SHORE_BAND: f32 = 0.3;

/// A world's splat bands as drawn, in the order they have always been
/// drawn, so that every draw after them - the land's own shape - keeps its
/// place (#1784). Written as rules two ways: [`Self::plateau`] for a world
/// on its own ground, [`Self::as_drawn`] for a square of Berlin.
#[derive(Clone, Copy, Debug)]
struct SplatDraws {
    grass_height_max: f32,
    grass_slope_max: f32,
    grass_sharpness: f32,
    dirt_height_min: f32,
    dirt_height_max: f32,
    dirt_slope_max: f32,
    dirt_sharpness: f32,
    rock_slope_min: f32,
    rock_sharpness: f32,
    snow_height_min: f32,
}

impl SplatDraws {
    /// The draws, field by field in their order.
    fn draw(rng: &mut ChaCha8Rng, bp: &BiomeSplatProfile) -> Self {
        Self {
            grass_height_max: jitter_around(rng, bp.grass_height_max, 0.05, 0.05, 0.95),
            grass_slope_max: jitter_around(rng, bp.grass_slope_max, 0.05, 0.05, 0.95),
            grass_sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
            dirt_height_min: jitter_around(rng, bp.dirt_height_min, 0.04, 0.0, 0.95),
            dirt_height_max: jitter_around(rng, bp.dirt_height_max, 0.05, 0.1, 1.0),
            dirt_slope_max: jitter_around(rng, 0.55, 0.05, 0.2, 0.9),
            dirt_sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
            rock_slope_min: jitter_around(rng, bp.rock_slope_min, 0.04, 0.05, 0.6),
            rock_sharpness: jitter_around(rng, bp.blend_sharpness, 0.10, 0.2, 2.0),
            snow_height_min: jitter_around(rng, bp.snow_height_min, 0.03, 0.05, 0.99),
        }
    }

    /// The bands as they were written before #1784, for the tent semantics:
    /// what a square of Berlin keeps ([`TerrainShape::square_splat_rules`]).
    fn as_drawn(&self) -> [SplatRule; 4] {
        [
            SplatRule {
                height_min: 0.0,
                height_max: self.grass_height_max,
                slope_min: 0.0,
                slope_max: self.grass_slope_max,
                sharpness: self.grass_sharpness,
            },
            SplatRule {
                height_min: self.dirt_height_min,
                height_max: self.dirt_height_max,
                slope_min: 0.0,
                slope_max: self.dirt_slope_max,
                sharpness: self.dirt_sharpness,
            },
            SplatRule {
                height_min: 0.0,
                height_max: 1.0,
                slope_min: self.rock_slope_min,
                slope_max: 1.0,
                sharpness: self.rock_sharpness,
            },
            SplatRule {
                height_min: self.snow_height_min,
                height_max: 1.0,
                slope_min: 0.0,
                slope_max: 1.0,
                sharpness: SNOW_SHARPNESS,
            },
        ]
    }

    /// The bands as rules for the plateau semantics (#1168, #1784), on a
    /// world whose water stands at `water` (a fraction of its height scale):
    ///
    /// * every edge about [`PLATEAU_SHARPNESS`] - under the tent semantics'
    ///   0.35 to 0.65, rock's slope skirt reached level ground and put a
    ///   third rock on it;
    /// * grass up to at least [`GRASS_OVER_WATER`] of its band above the
    ///   water in a biome of grass ([`GRASS_SHORE_BAND`]), under the snow
    ///   line; dirt from no higher than grass reaches to just under the snow
    ///   line ([`DIRT_UNDER_SNOW`]) - so no height is left to no layer, which
    ///   the ground paints as rock;
    /// * rock from a little above grass's steepest slope
    ///   ([`ROCK_OVER_GRASS`]) and never below [`ROCK_SLOPE_FLOOR`], dirt's
    ///   slopes ending just past it ([`DIRT_OVER_ROCK`]): never on gentle
    ///   ground, never a slope left uncovered, and rock the ground's largest
    ///   layer on its slopes as it was before - no tie handing them to dirt;
    /// * snow on ground gentle enough to hold it ([`SNOW_SLOPE_MAX`]).
    fn plateau(&self, bp: &BiomeSplatProfile, water: f32) -> [SplatRule; 4] {
        let sharpness = |drawn: f32| PLATEAU_SHARPNESS * drawn / bp.blend_sharpness;
        let shore = if bp.grass_height_max >= GRASS_SHORE_BAND {
            water + GRASS_OVER_WATER * self.grass_height_max
        } else {
            0.0
        };
        let grass_top = self
            .grass_height_max
            .max(shore)
            .min(self.snow_height_min - GRASS_UNDER_SNOW);
        let dirt_top = self.snow_height_min - DIRT_UNDER_SNOW;
        let dirt_bottom = self
            .dirt_height_min
            .min(grass_top - DIRT_UNDER_GRASS)
            .clamp(0.0, dirt_top);
        let rock_min = self
            .rock_slope_min
            .clamp(
                self.grass_slope_max + ROCK_OVER_GRASS.0,
                self.grass_slope_max + ROCK_OVER_GRASS.1,
            )
            .max(ROCK_SLOPE_FLOOR);
        let dirt_slope_max = self.dirt_slope_max.min(rock_min + DIRT_OVER_ROCK);
        [
            SplatRule {
                height_min: 0.0,
                height_max: grass_top,
                slope_min: 0.0,
                slope_max: self.grass_slope_max,
                sharpness: sharpness(self.grass_sharpness),
            },
            SplatRule {
                height_min: dirt_bottom,
                height_max: dirt_top,
                slope_min: 0.0,
                slope_max: dirt_slope_max,
                sharpness: sharpness(self.dirt_sharpness),
            },
            SplatRule {
                height_min: 0.0,
                height_max: 1.0,
                slope_min: rock_min,
                slope_max: 1.0,
                sharpness: sharpness(self.rock_sharpness),
            },
            SplatRule {
                height_min: self.snow_height_min,
                height_max: 1.0,
                slope_min: 0.0,
                slope_max: SNOW_SLOPE_MAX,
                sharpness: SNOW_SHARPNESS,
            },
        ]
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
                SovereignGeneratorKind::VoronoiTerracing,
                "mesa must stay Voronoi-terraced"
            );
        }
        assert!(
            rolling_kinds.contains(&SovereignGeneratorKind::FbmNoise)
                && rolling_kinds.contains(&SovereignGeneratorKind::DiamondSquare),
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

    /// The talus bands encode landform identity (#904): sharp-edged
    /// landforms (Craggy peaks, Mesa terrace lips) keep a high stable
    /// step so thermal erosion only sheds genuine over-steep faces,
    /// while Rolling stays low (its identity *is* the smoothing).
    #[test]
    fn talus_keeps_sharp_landforms_sharp() {
        for s in 0u64..32 {
            let mut scene = SceneCharacter::for_seed(s);

            scene.landform = LandformArchetype::Craggy;
            let craggy = TerrainShape::from_scene(&scene, s).thermal_talus_angle;
            assert!(craggy >= 0.25, "craggy talus {craggy} too smoothing");

            scene.landform = LandformArchetype::Mesa;
            let mesa = TerrainShape::from_scene(&scene, s);
            assert!(
                mesa.thermal_talus_angle >= 0.25,
                "mesa talus {} would mush the terrace lips",
                mesa.thermal_talus_angle
            );
            assert!(
                mesa.thermal_iterations <= 20,
                "mesa sweeps {} too many for crisp terraces",
                mesa.thermal_iterations
            );

            scene.landform = LandformArchetype::Rolling;
            let rolling = TerrainShape::from_scene(&scene, s).thermal_talus_angle;
            assert!(
                rolling <= 0.12,
                "rolling talus {rolling} lost its soft-hill smoothing"
            );
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

    /// The ground's own reading of a band (symbios-ground's plateau, #1168).
    fn weights(rules: &[SplatRule; 4], height: f32, slope: f32) -> [f32; 4] {
        rules.map(|r| {
            bevy_symbios_ground::SplatRule::new(
                (r.height_min, r.height_max),
                (r.slope_min, r.slope_max),
                r.sharpness,
            )
            .weight(height, slope)
        })
    }

    /// Every world's bands for every biome and landform, over seeds enough
    /// to reach their draws' extremes.
    fn every_band() -> impl Iterator<Item = (BiomeArchetype, TerrainShape)> {
        LandformArchetype::ALL.into_iter().flat_map(|landform| {
            BiomeArchetype::ALL.into_iter().flat_map(move |biome| {
                (0u64..24).map(move |s| {
                    let mut scene = SceneCharacter::for_seed(s);
                    scene.landform = landform;
                    scene.biome = biome;
                    (biome, TerrainShape::from_scene(&scene, s))
                })
            })
        })
    }

    /// #1784: no ground from the water up is left to no layer - which the
    /// ground paints as rock, flat or not - however a world's bands are
    /// drawn: at every height and slope the four bands' weights sum to a
    /// real share.
    #[test]
    fn no_ground_is_left_to_no_layer() {
        let mut least = f32::MAX;
        for (biome, t) in every_band() {
            for hi in 0..=100 {
                let height =
                    t.water_level_fraction + (1.0 - t.water_level_fraction) * hi as f32 / 100.0;
                for si in 0..=100 {
                    let slope = si as f32 / 100.0;
                    let total: f32 = weights(&t.splat_rules, height, slope).iter().sum();
                    assert!(
                        total >= 0.05,
                        "{biome:?}: height {height} slope {slope} covered {total}: {:?}",
                        t.splat_rules
                    );
                    least = least.min(total);
                }
            }
        }
        assert!(least >= 0.05, "{least}");
    }

    /// #1784: rock keeps off gentle ground - under the tent semantics' soft
    /// edges its skirt reached level ground and put a third rock on it - and
    /// snow off cliffs, which its band covered as fully as the flats.
    #[test]
    fn rock_keeps_off_gentle_ground_and_snow_off_cliffs() {
        for (biome, t) in every_band() {
            for hi in 0..=50 {
                let height = hi as f32 / 50.0;
                for (slope, rock_most) in [(0.0, 0.01), (0.03, 0.02), (0.05, 0.08)] {
                    let w = weights(&t.splat_rules, height, slope);
                    let total: f32 = w.iter().sum();
                    if total > f32::EPSILON {
                        assert!(
                            w[2] / total <= rock_most,
                            "{biome:?}: rock {} at height {height} slope {slope}: {:?}",
                            w[2] / total,
                            t.splat_rules
                        );
                    }
                }
                let w = weights(&t.splat_rules, height, 0.5);
                assert_eq!(w[3], 0.0, "{biome:?}: snow on a cliff at height {height}");
            }
        }
    }

    /// #1784: a grassy biome's islands keep a shore of grass however high
    /// their water stands - under the snow line - and a barren biome's few
    /// metres of grass still drown.
    #[test]
    fn grass_keeps_a_shore_under_the_snow() {
        for (biome, t) in every_band() {
            let grass = t.splat_rules[0];
            let snow = t.splat_rules[3].height_min;
            assert!(
                grass.height_max <= snow - GRASS_UNDER_SNOW + 1e-6,
                "{biome:?}: {t:?}"
            );
            if biome_splat_profile(biome).grass_height_max >= GRASS_SHORE_BAND {
                let shore = (t.water_level_fraction + 0.1).min(snow - GRASS_UNDER_SNOW);
                assert!(
                    grass.height_max >= shore - 1e-6,
                    "{biome:?}: grass to {} over water at {}",
                    grass.height_max,
                    t.water_level_fraction
                );
            }
        }
    }

    /// #1784: past dirt's slopes rock is the ground's largest layer over
    /// grass and dirt, as it was where the tent semantics drew it - a
    /// plateau tie would hand the steep faces to dirt, the lower layer, for
    /// every reader of the largest layer - and over snow past snow's own
    /// slopes.
    #[test]
    fn rock_is_the_largest_layer_on_steep_ground() {
        for (biome, t) in every_band() {
            let past = t.splat_rules[1].slope_max.max(t.splat_rules[0].slope_max) + 0.01;
            for hi in 0..=50 {
                let height = hi as f32 / 50.0;
                for si in 0..=20 {
                    let slope = past + (1.0 - past) * si as f32 / 20.0;
                    let w = weights(&t.splat_rules, height, slope);
                    // Snow holds as steep as its own band (and its skirt)
                    // reaches above the snow line; past that, rock alone.
                    let past_snow = slope > SNOW_SLOPE_MAX + 0.05;
                    assert!(
                        w[2] > w[0] && w[2] > w[1] && (!past_snow || w[2] > w[3]),
                        "{biome:?}: height {height} slope {slope}: {w:?}"
                    );
                }
            }
        }
    }
}
