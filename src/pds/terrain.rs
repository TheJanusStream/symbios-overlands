//! Terrain generator payload (ported from `symbios-ground-lab`): algorithm
//! selection, erosion tuning, and the four-layer splat/material configuration
//! used by the ground compiler.

use super::texture::{SovereignGroundConfig, SovereignRockConfig, SovereignTextureConfig};
use super::types::{Fp, Fp3, Fp64, u64_as_string};
use serde::{Deserialize, Serialize};

/// Builds [`SovereignGeneratorKind`] and everything that enumerates it from
/// the [`gen_jobs::for_each_heightmap_generator!`] roster.
///
/// The wire enum cannot simply *be* [`gen_jobs::GeneratorKind`] — see
/// [`SovereignGeneratorKind::Unknown`] — but the two must never disagree
/// about which algorithms exist, and before this they were two hand-typed
/// lists with two hand-typed translation ladders between them.
macro_rules! define_sovereign_generator_kind {
    ($( ($variant:ident, $label:literal) ),* $(,)?) => {
        /// Which base terrain algorithm to run — the wire form of
        /// [`gen_jobs::GeneratorKind`].
        ///
        /// Open union (#1119). `symbios-ground` gains algorithms; a Terrain
        /// child naming one this build has never compiled used to fail that
        /// child's decode outright, and `list_room_children` drops what it
        /// cannot read — so the room loaded with no ground under it, and the
        /// next publish from this client rewrote the manifest without the ref
        /// and orphaned the child. A fourth algorithm should cost a visitor
        /// its terracing, not the owner their terrain.
        ///
        /// That tolerance is exactly why this is not a re-export of
        /// [`gen_jobs::GeneratorKind`]: every variant of *that* enum must
        /// have a generator body, and `Unknown` by definition has none.
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum SovereignGeneratorKind {
            $( $variant, )*
            /// An algorithm from a newer engine. Runs as
            /// [`SovereignGeneratorKind::default`] so the room still has
            /// ground, and refuses to serialize so this build cannot save
            /// its substitute over the owner's real choice.
            #[serde(other, skip_serializing)]
            Unknown,
        }

        impl Default for SovereignGeneratorKind {
            fn default() -> Self {
                Self::VoronoiTerracing
            }
        }

        impl SovereignGeneratorKind {
            /// The algorithms this build can actually run, in roster order —
            /// what the terrain panel offers. `Unknown` is deliberately
            /// absent: picking a real algorithm is how the owner replaces
            /// it, and until they do the save stays refused rather than
            /// silently downgrading their choice.
            pub const SELECTABLE: &'static [SovereignGeneratorKind] =
                &[ $( SovereignGeneratorKind::$variant ),* ];

            /// Human-readable name for the picker.
            pub fn label(self) -> &'static str {
                match self {
                    $( Self::$variant => $label, )*
                    Self::Unknown => "Unknown (newer version)",
                }
            }

            /// The algorithm the generation job actually runs.
            ///
            /// An algorithm this build has never heard of still has to
            /// produce a heightmap — a visitor seeing the wrong terrain is
            /// recoverable, a visitor standing on nothing is not — so
            /// `Unknown` runs as the default. It is the only arm that
            /// translates to something other than its namesake.
            pub fn to_gen_job(self) -> gen_jobs::GeneratorKind {
                match self {
                    $( Self::$variant => gen_jobs::GeneratorKind::$variant, )*
                    Self::Unknown => gen_jobs::GeneratorKind::VoronoiTerracing,
                }
            }
        }
    };
}
gen_jobs::for_each_heightmap_generator!(define_sovereign_generator_kind);

/// Full terrain configuration stored inside a `Generator::Terrain` variant.
/// This is a serialisable mirror of `ground-lab::TerrainConfig` — all `f32`
/// fields are wrapped in [`Fp`] so the record stays DAG-CBOR compliant.
///
/// Default-eliding wire format (#695): fields matching
/// [`SovereignTerrainConfig::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignTerrainConfig {
    // Grid / world
    pub grid_size: u32,
    pub cell_scale: Fp,
    pub height_scale: Fp,

    // Algorithm selection
    pub generator_kind: SovereignGeneratorKind,
    #[serde(with = "u64_as_string")]
    pub seed: u64,

    // FBM params
    pub octaves: u32,
    pub persistence: Fp,
    pub lacunarity: Fp,
    pub base_frequency: Fp,

    // Diamond Square params
    pub ds_roughness: Fp,

    // Voronoi params
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: Fp,
    pub erosion_rate: Fp,
    pub deposition_rate: Fp,
    pub evaporation_rate: Fp,
    pub capacity_factor: Fp,

    // Thermal erosion
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: Fp,

    // Material (splat) config
    pub material: SovereignMaterialConfig,
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignTerrainConfig {
    grid_size,
    cell_scale,
    height_scale,
    generator_kind,
    seed via u64_as_string(u64),
    octaves,
    persistence,
    lacunarity,
    base_frequency,
    ds_roughness,
    voronoi_num_seeds,
    voronoi_num_terraces,
    erosion_enabled,
    erosion_drops,
    inertia,
    erosion_rate,
    deposition_rate,
    evaporation_rate,
    capacity_factor,
    thermal_enabled,
    thermal_iterations,
    thermal_talus_angle,
    material,
});

impl Default for SovereignTerrainConfig {
    fn default() -> Self {
        Self {
            grid_size: 512,
            cell_scale: Fp(2.0),
            height_scale: Fp(50.0),

            generator_kind: SovereignGeneratorKind::VoronoiTerracing,
            seed: 42,

            octaves: 6,
            persistence: Fp(0.5),
            lacunarity: Fp(2.0),
            base_frequency: Fp(4.0),

            ds_roughness: Fp(0.5),

            voronoi_num_seeds: 1000,
            voronoi_num_terraces: 2,

            erosion_enabled: true,
            erosion_drops: 50_000,
            inertia: Fp(0.05),
            erosion_rate: Fp(0.3),
            deposition_rate: Fp(0.3),
            evaporation_rate: Fp(0.02),
            capacity_factor: Fp(8.0),

            thermal_enabled: true,
            thermal_iterations: 30,
            thermal_talus_angle: Fp(0.05),

            material: SovereignMaterialConfig::default(),
        }
    }
}

/// Splat rule for a single texture layer. `[0, 1]` normalised height; slope
/// is raw gradient magnitude in `[0, ∞)` (1.0 ≈ 45°).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SovereignSplatRule {
    pub height_min: Fp,
    pub height_max: Fp,
    pub slope_min: Fp,
    pub slope_max: Fp,
    pub sharpness: Fp,
}

/// Four-layer splat/texture configuration for a terrain generator.
///
/// `rules[i]` controls where layer `i` appears on the terrain (altitude and
/// slope bands); `layers[i]` is the procedural texture generator config that
/// bakes that layer's albedo/normal/ORM maps. Any
/// [`SovereignTextureConfig`] variant may appear in any slot — the canonical
/// defaults are Grass / Dirt / Rock / Snow (Ground / Ground / Rock / Ground),
/// but a room can swap any layer for e.g. `Brick`, `Cobblestone`, `Thatch`.
///
/// Default-eliding wire format (#695): fields matching
/// [`SovereignMaterialConfig::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SovereignMaterialConfig {
    pub texture_size: u32,
    pub tile_scale: Fp,
    /// Splat rules for channels R, G, B, A — one per layer.
    pub rules: [SovereignSplatRule; 4],
    /// Procedural texture configs for channels R, G, B, A.
    pub layers: [SovereignTextureConfig; 4],
}

crate::pds::serde_util::impl_default_eliding_serialize!(SovereignMaterialConfig {
    texture_size,
    tile_scale,
    rules,
    layers,
});

impl Default for SovereignMaterialConfig {
    fn default() -> Self {
        Self {
            texture_size: crate::config::textures::SPLAT,
            tile_scale: Fp(90.0),
            // Retuned for symbios-ground 0.4's plateau semantics (#1168). A
            // range now carries full weight across itself, endpoints included,
            // and fades over a skirt of `half / (1 + sharpness)` *outside* it.
            // Under the old tent it peaked at the midpoint and scored zero at
            // both ends, so `slope_min: 0.0` meant "absent on level ground" and
            // every one of these rules missed a dead-flat texel — the mapper
            // fell through to its no-rule-matched branch, which paints rock.
            //
            // Two things had to move with the semantics:
            //
            // * `sharpness: 0.5` was compensation, not taste. It widened the
            //   tents until near-level ground scored *something*; under
            //   plateaus it makes the skirts enormous — rock's slope skirt
            //   reached down to slope 0.0 and put 31% rock on a 0.05 slope.
            //   2.0 gives an edge a third of the half-range wide, which reads
            //   as a transition rather than a wash.
            // * Dirt's height band now runs up to snow. Rock owns every
            //   height but only steep slopes, so with dirt stopping at 0.65
            //   a *flat* texel at height 0.8 matched nothing at all and the
            //   fallback hole re-opened higher up the mountain.
            //
            // Snow's slope band is the third change and the one the tent was
            // actively lying about: `(0.0, 1.0)` peaked at slope 0.5, so snow
            // was strongest on 45° faces. It now sits on the gentle ground it
            // belongs on and leaves the cliffs to rock.
            //
            // These are defaults; a published region carries its own copy of
            // these five numbers in its record and keeps them. See the issue
            // for why they are deliberately not migrated.
            rules: [
                // R — Grass: low ground, gentle.
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(0.45),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.25),
                    sharpness: Fp(2.0),
                },
                // G — Dirt: the whole middle and upper band, up to the snow
                // line, tolerant of moderate slopes.
                SovereignSplatRule {
                    height_min: Fp(0.30),
                    height_max: Fp(0.90),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.50),
                    sharpness: Fp(2.0),
                },
                // B — Rock: any height, steep faces only.
                //
                // The gap to grass's 0.25 is deliberate. Two plateaus that
                // *abut* both read exactly 1 on the shared boundary, and
                // `dominant_biome` is an argmax — so which one wins there is
                // decided by the last bit of a `powf`, and a hairline of rock
                // appeared along dead-level ground at exactly slope 0.25.
                // Leaving 0.05 between them puts the handover inside the two
                // skirts, where the weights differ by a real margin.
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.30),
                    slope_max: Fp(1.0),
                    sharpness: Fp(2.0),
                },
                // A — Snow: the summit, and only where it could settle.
                SovereignSplatRule {
                    height_min: Fp(0.88),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.40),
                    sharpness: Fp(4.0),
                },
            ],
            layers: [
                // R — Grass
                SovereignTextureConfig::Ground(SovereignGroundConfig {
                    seed: 1,
                    macro_scale: Fp64(2.5),
                    macro_octaves: 4,
                    micro_scale: Fp64(10.0),
                    micro_octaves: 3,
                    micro_weight: Fp64(0.3),
                    color_dry: Fp3([0.07, 0.12, 0.03]),
                    color_moist: Fp3([0.03, 0.07, 0.01]),
                    normal_strength: Fp(4.5),
                }),
                // G — Dirt
                SovereignTextureConfig::Ground(SovereignGroundConfig::default()),
                // B — Rock
                SovereignTextureConfig::Rock(SovereignRockConfig::default()),
                // A — Snow
                SovereignTextureConfig::Ground(SovereignGroundConfig {
                    seed: 99,
                    macro_scale: Fp64(4.0),
                    macro_octaves: 3,
                    micro_scale: Fp64(12.0),
                    micro_octaves: 3,
                    micro_weight: Fp64(0.4),
                    color_dry: Fp3([0.95, 0.95, 0.98]),
                    color_moist: Fp3([0.80, 0.82, 0.88]),
                    normal_strength: Fp(0.8),
                }),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SovereignGeneratorKind;

    /// The wire enum and the dispatch enum are built from one roster, so
    /// their membership, order and labels agree by construction — this is
    /// what says so out loud.
    ///
    /// Before #1157 they were two hand-typed variant lists (plus a third in
    /// `seeded_defaults`) joined by two hand-written translation ladders,
    /// and adding a fourth algorithm meant editing all five sites. Adding
    /// it to only one of them compiled: the ladders' arms were exhaustive
    /// per-type, not across types, so the new algorithm would simply never
    /// be reachable from a record.
    #[test]
    fn the_wire_enum_and_the_dispatch_enum_share_one_roster() {
        let translated: Vec<gen_jobs::GeneratorKind> = SovereignGeneratorKind::SELECTABLE
            .iter()
            .map(|k| k.to_gen_job())
            .collect();
        assert_eq!(
            translated,
            gen_jobs::GeneratorKind::ALL,
            "the runnable algorithms and the wire algorithms have drifted"
        );
        for (wire, job) in SovereignGeneratorKind::SELECTABLE
            .iter()
            .zip(gen_jobs::GeneratorKind::ALL)
        {
            assert_eq!(wire.label(), job.label(), "labels disagree for {wire:?}");
        }
    }

    /// `Unknown` is the one arm that does not translate to its namesake:
    /// it has none. It must run as the default rather than failing, or a
    /// visitor whose peer named a newer algorithm stands on nothing
    /// (#1119).
    #[test]
    fn an_unknown_algorithm_runs_as_the_default() {
        assert_eq!(
            SovereignGeneratorKind::Unknown.to_gen_job(),
            SovereignGeneratorKind::default().to_gen_job()
        );
        assert!(!SovereignGeneratorKind::SELECTABLE.contains(&SovereignGeneratorKind::Unknown));
        assert_ne!(
            SovereignGeneratorKind::default(),
            SovereignGeneratorKind::Unknown
        );
    }
}
