//! Open-union `Generator` and `Placement` enums — the building blocks of a
//! `RoomRecord`'s recipe. Both use `#[serde(other)] Unknown` so a client
//! visiting a room authored by a newer engine version skips unrecognised
//! variants instead of crashing its deserializer.
//!
//! **Fractal Construct Engine.** Every parametric primitive (Cuboid, Sphere,
//! Cylinder, …) is a first-class `Generator` variant that can live at the
//! top level of a room **or** inside a [`ConstructNode`] tree. The unified
//! [`Generator::Construct`] variant carries a `ConstructNode`, which itself
//! boxes a [`Generator`] and a list of child nodes — so a Construct can
//! contain another Construct (fractal nesting), an L-system, a portal, etc.
//! `Terrain` and `Water` are room-scoped and sanitised away if a hostile
//! record attempts to smuggle them inside a Construct.

use super::prim::PropMeshType;
use super::terrain::SovereignTerrainConfig;
use super::texture::SovereignMaterialSettings;
use super::types::{
    BiomeFilter, Fp, Fp2, Fp3, ScatterBounds, TransformData, default_true, map_u8_as_string,
    map_u16_as_string, u64_as_string,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    #[serde(rename = "network.symbios.gen.portal")]
    Portal { target_did: String, target_pos: Fp3 },

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

    #[serde(rename = "network.symbios.gen.cuboid")]
    Cuboid {
        size: Fp3,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.sphere")]
    Sphere {
        radius: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.cylinder")]
    Cylinder {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.capsule")]
    Capsule {
        radius: Fp,
        length: Fp,
        latitudes: u32,
        longitudes: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.cone")]
    Cone {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.torus")]
    Torus {
        minor_radius: Fp,
        major_radius: Fp,
        minor_resolution: u32,
        major_resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.plane")]
    Plane {
        size: Fp2,
        subdivisions: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.tetrahedron")]
    Tetrahedron {
        size: Fp,
        solid: bool,
        material: SovereignMaterialSettings,
        twist: Fp,
        taper: Fp,
        bend: Fp3,
    },

    #[serde(rename = "network.symbios.gen.construct")]
    Construct { root: ConstructNode },

    #[serde(other)]
    Unknown,
}

impl Generator {
    /// Canonical default for a newly-added primitive — a 1×1×1 cuboid with
    /// zero torture and a blank material. Used by UI "+ Cuboid" flows and
    /// when the sanitizer overwrites a forbidden `Terrain`/`Water` generator
    /// nested inside a Construct.
    pub fn default_cuboid() -> Self {
        Generator::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            solid: true,
            material: SovereignMaterialSettings::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }
    }

    /// `true` when the variant is a parametric primitive (Cuboid..Tetrahedron).
    /// Used by the UI primitive-kind picker and by the spawner to dispatch
    /// into the shared mesh/collider path.
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Generator::Cuboid { .. }
                | Generator::Sphere { .. }
                | Generator::Cylinder { .. }
                | Generator::Capsule { .. }
                | Generator::Cone { .. }
                | Generator::Torus { .. }
                | Generator::Plane { .. }
                | Generator::Tetrahedron { .. }
        )
    }

    /// Short human-readable tag for the variant — used by the UI combo box
    /// to show the current kind and to drive `default_for_tag`.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Generator::Terrain(_) => "Terrain",
            Generator::Water { .. } => "Water",
            Generator::Portal { .. } => "Portal",
            Generator::LSystem { .. } => "LSystem",
            Generator::Cuboid { .. } => "Cuboid",
            Generator::Sphere { .. } => "Sphere",
            Generator::Cylinder { .. } => "Cylinder",
            Generator::Capsule { .. } => "Capsule",
            Generator::Cone { .. } => "Cone",
            Generator::Torus { .. } => "Torus",
            Generator::Plane { .. } => "Plane",
            Generator::Tetrahedron { .. } => "Tetrahedron",
            Generator::Construct { .. } => "Construct",
            Generator::Unknown => "Unknown",
        }
    }

    /// Build a default primitive generator for `tag`. Returns `None` for
    /// non-primitive tags — callers that want to switch a ConstructNode into
    /// an L-system, Portal, or Construct should construct those variants
    /// directly since they carry more state than sensible defaults capture.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        let mat = SovereignMaterialSettings::default();
        let zero = Fp(0.0);
        let zero3 = Fp3([0.0, 0.0, 0.0]);
        Some(match tag {
            "Cuboid" => Generator::Cuboid {
                size: Fp3([1.0, 1.0, 1.0]),
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Sphere" => Generator::Sphere {
                radius: Fp(0.5),
                resolution: 3,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Cylinder" => Generator::Cylinder {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Capsule" => Generator::Capsule {
                radius: Fp(0.5),
                length: Fp(1.0),
                latitudes: 8,
                longitudes: 16,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Cone" => Generator::Cone {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Torus" => Generator::Torus {
                minor_radius: Fp(0.1),
                major_radius: Fp(0.5),
                minor_resolution: 12,
                major_resolution: 24,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Plane" => Generator::Plane {
                size: Fp2([1.0, 1.0]),
                subdivisions: 0,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Tetrahedron" => Generator::Tetrahedron {
                size: Fp(1.0),
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            _ => return None,
        })
    }
}

/// A single node in a `Construct` hierarchy. Each node composes a
/// [`Generator`] with a local [`TransformData`] (its placement in the parent
/// node's frame) and an optional child list. The generator is boxed so a
/// node can recursively carry another `Construct` — enabling fractal
/// blueprints without blowing up the enum's stack size at every nesting
/// level.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConstructNode {
    pub generator: Box<Generator>,
    pub transform: TransformData,
    #[serde(default)]
    pub children: Vec<ConstructNode>,
}

impl Default for ConstructNode {
    fn default() -> Self {
        Self {
            generator: Box::new(Generator::default_cuboid()),
            transform: TransformData::default(),
            children: Vec::new(),
        }
    }
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
