//! Open-union `Generator` and `Placement` enums — the building blocks of a
//! `RoomRecord`'s recipe. Both use `#[serde(other)] Unknown` so a client
//! visiting a room authored by a newer engine version skips unrecognised
//! variants instead of crashing its deserializer.

use super::prim::PrimNode;
use super::terrain::SovereignTerrainConfig;
use super::texture::SovereignMaterialSettings;
use super::types::{
    BiomeFilter, Fp, Fp3, ScatterBounds, TransformData, default_true, map_u8_as_string,
    map_u16_as_string, u64_as_string,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::prim::PropMeshType;

/// Blueprint for something that can be spawned into a room.  Open union:
/// unknown tags deserialize to `Unknown` instead of failing.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum Generator {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    #[serde(rename = "network.symbios.gen.water")]
    Water { level_offset: Fp },

    #[serde(rename = "network.symbios.gen.shape")]
    Shape { style: String, floors: u32 },

    #[serde(rename = "network.symbios.gen.portal")]
    Portal { target_did: String, target_pos: Fp3 },

    #[serde(rename = "network.symbios.gen.construct")]
    Construct { root: PrimNode },

    #[serde(rename = "network.symbios.gen.lsystem")]
    LSystem {
        source_code: String,
        finalization_code: String,
        iterations: u32,
        #[serde(with = "u64_as_string")]
        seed: u64,
        angle: Fp,
        step: Fp,
        width: Fp,
        elasticity: Fp,
        tropism: Option<Fp3>,
        /// Material slot id → PBR settings.
        #[serde(with = "map_u8_as_string")]
        materials: HashMap<u8, SovereignMaterialSettings>,
        /// Prop id → mesh shape.
        #[serde(with = "map_u16_as_string")]
        prop_mappings: HashMap<u16, PropMeshType>,
        prop_scale: Fp,
        mesh_resolution: u32,
    },

    #[serde(other)]
    Unknown,
}

/// Where and how a `Generator` is instantiated.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
pub enum Placement {
    #[serde(rename = "network.symbios.place.absolute")]
    Absolute {
        generator_ref: String,
        transform: TransformData,
        #[serde(default = "default_true")]
        snap_to_terrain: bool,
    },

    #[serde(rename = "network.symbios.place.scatter")]
    Scatter {
        generator_ref: String,
        bounds: ScatterBounds,
        count: u32,
        #[serde(with = "u64_as_string")]
        local_seed: u64,
        /// Combined biome allow-list + water-surface relation. A default
        /// `BiomeFilter` accepts every sample.
        #[serde(default)]
        biome_filter: BiomeFilter,
        #[serde(default = "default_true")]
        snap_to_terrain: bool,
        /// Apply a deterministic random yaw (per `local_seed`) to every
        /// scattered instance. Defaults to `true` for backward compatibility
        /// with records written before this field existed.
        #[serde(default = "default_true")]
        random_yaw: bool,
    },

    #[serde(rename = "network.symbios.place.grid")]
    Grid {
        generator_ref: String,
        transform: TransformData,
        counts: [u32; 3],
        gaps: Fp3,
        #[serde(default = "default_true")]
        snap_to_terrain: bool,
        /// Apply a per-cell deterministic random yaw. Defaults to `false`
        /// — grids are typically axis-aligned.
        #[serde(default)]
        random_yaw: bool,
    },

    #[serde(other)]
    Unknown,
}
