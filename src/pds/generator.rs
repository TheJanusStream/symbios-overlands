//! Open-union [`GeneratorKind`] and [`Placement`] enums — the building blocks
//! of a `RoomRecord`'s recipe. Both use `#[serde(other)] Unknown` so a client
//! visiting a room authored by a newer engine version skips unrecognised
//! variants instead of crashing its deserializer.
//!
//! **Unified Construct Model.** Every generator is hierarchical: it carries a
//! [`GeneratorKind`] (the variant-specific parameters), a local
//! [`TransformData`], and a `Vec<Generator>` of children. Any kind — primitive,
//! L-system, portal — can have children, so a portal can wear a doorframe, a
//! cuboid can carry a chimney, and Constructs are no longer a distinct kind.
//! `Terrain` and `Water` remain room-scoped and are sanitised away when a
//! hostile record tries to nest them as children or hang children off them.

use super::prim::PropMeshType;
use super::terrain::SovereignTerrainConfig;
use super::texture::SovereignMaterialSettings;
use super::types::{
    BiomeFilter, Fp, Fp2, Fp3, Fp4, ScatterBounds, TransformData, default_true, map_u8_as_string,
    map_u16_as_string, u64_as_string,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-volume appearance and wave parameters for [`GeneratorKind::Water`].
///
/// Everything on this struct describes the water body itself (its colour,
/// choppiness, prevailing wave direction). Room-wide water settings —
/// detail-normal tiling, sun glitter strength, shoreline foam width — live on
/// [`crate::pds::Environment`] instead so they match the room's overall mood
/// rather than varying between adjacent water volumes.
///
/// `#[serde(default)]` at both struct and field level means a record that only
/// carries `level_offset` (the pre-overhaul schema) round-trips cleanly with
/// every appearance field filled in from [`WaterSurface::default`].
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WaterSurface {
    /// sRGBA tint seen looking straight down (low alpha = transparent).
    pub shallow_color: Fp4,
    /// sRGBA tint seen at grazing angles (high alpha = opaque).
    pub deep_color: Fp4,
    /// PBR perceptual roughness. Water is typically very low (~0.05–0.12).
    pub roughness: Fp,
    /// PBR metallic. Water is dielectric so this is ~0.
    pub metallic: Fp,
    /// Schlick F0 reflectance — the base fraction of light reflected when
    /// viewed head-on. Real water is ~0.02; higher values bias toward a
    /// stylised, glossy look.
    pub reflectance: Fp,
    /// Global amplitude multiplier on the Gerstner waves. `0.0` = flat pond.
    pub wave_scale: Fp,
    /// Global time multiplier on the Gerstner waves. `0.0` = frozen.
    pub wave_speed: Fp,
    /// Prevailing wave direction in the world XZ plane. Need not be
    /// unit-length — the shader normalises.
    pub wave_direction: Fp2,
    /// Gerstner steepness in `[0, 1]`. `0` = smooth sines, `1` = sharp crests.
    pub wave_choppiness: Fp,
    /// Strength of the procedural foam on wave crests (`[0, 1]`).
    pub foam_amount: Fp,
    /// Force-per-metre-submerged applied to objects floating in this water,
    /// directed along the steepest-descent tangent of the surface (the
    /// projection of gravity onto the plane). `0.0` = still water; ~9.81 ≈
    /// "free-fall along the slope" for a 1-metre-deep avatar. Has no effect
    /// on flat water — the tangent component of gravity is then zero —
    /// which keeps existing rooms unchanged. This is the *physics* knob;
    /// the visual flow-map blend lives separately on `flow_amount`.
    pub flow_strength: Fp,
    /// Visual flow-map blend in `[0, 1]`. `0.0` = classic standing-wave
    /// Gerstner (still pond, even on a tilt — the existing look). `1.0` =
    /// pure flow-map mode (scrolling detail normals along the surface's
    /// downhill direction, suppressed Gerstner amplitude — the river /
    /// stream look). Mix in between for a choppy flowing river.
    /// Independent of `flow_strength` so a glassy "infinity-pool" effect
    /// (visible flow, no avatar push) is authorable.
    pub flow_amount: Fp,
}

impl Default for WaterSurface {
    fn default() -> Self {
        // Defaults tuned against the six-Gerstner-wave table in water.wgsl.
        // Lower choppiness + moderate roughness keep the specular lobe wide
        // enough to absorb small residual normal errors without revealing
        // wave interference bands at grazing angles.
        Self {
            shallow_color: Fp4([0.18, 0.48, 0.56, 0.22]),
            deep_color: Fp4([0.02, 0.14, 0.24, 0.9]),
            roughness: Fp(0.14),
            metallic: Fp(0.0),
            reflectance: Fp(0.3),
            wave_scale: Fp(0.7),
            wave_speed: Fp(1.0),
            wave_direction: Fp2([1.0, 0.3]),
            wave_choppiness: Fp(0.3),
            foam_amount: Fp(0.25),
            flow_strength: Fp(0.0),
            flow_amount: Fp(0.0),
        }
    }
}

/// Variant-specific payload for a [`Generator`]. Open union: unrecognised
/// `$type` tags deserialise to `Unknown` instead of failing the whole record.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum GeneratorKind {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    #[serde(rename = "network.symbios.gen.water")]
    Water {
        level_offset: Fp,
        #[serde(default)]
        surface: WaterSurface,
    },

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

    #[serde(rename = "network.symbios.gen.shape")]
    Shape {
        /// Multi-rule CGA Shape Grammar source. One rule per line in the
        /// `Name --> ops` form documented by `symbios_shape::grammar::parse_rule`.
        /// Lines that are blank or start with `//` are skipped at compile time.
        grammar_source: String,
        /// Entry rule that the interpreter starts deriving from. Must appear
        /// in `grammar_source`; if absent, the spawner skips the generator.
        root_rule: String,
        /// Initial scope size passed to `Interpreter::derive`. Y is
        /// typically `0.0` because most grammars `Extrude` the footprint
        /// themselves; the placement transform contributes the world
        /// position and rotation.
        footprint: Fp3,
        /// Stochastic-rule RNG seed. The interpreter weights `A | B | C` by
        /// percentage; the same seed across peers reproduces the same draw.
        #[serde(with = "u64_as_string")]
        seed: u64,
        /// Material name (the string emitted by `Mat("...")` in the grammar)
        /// → PBR settings. A terminal whose `material` is `None` or whose
        /// name has no entry here falls back to the spawner's default
        /// material.
        #[serde(default)]
        materials: HashMap<String, SovereignMaterialSettings>,
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

    #[serde(other)]
    Unknown,
}

impl GeneratorKind {
    /// Canonical default kind for a newly-added primitive — a 1×1×1 cuboid
    /// with zero torture and a blank material. Used by UI "+ Cuboid" flows
    /// and when the sanitizer overwrites a forbidden `Terrain`/`Water`
    /// generator nested inside another generator.
    pub fn default_cuboid() -> Self {
        GeneratorKind::Cuboid {
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
            GeneratorKind::Cuboid { .. }
                | GeneratorKind::Sphere { .. }
                | GeneratorKind::Cylinder { .. }
                | GeneratorKind::Capsule { .. }
                | GeneratorKind::Cone { .. }
                | GeneratorKind::Torus { .. }
                | GeneratorKind::Plane { .. }
                | GeneratorKind::Tetrahedron { .. }
        )
    }

    /// Short human-readable tag for the variant — used by the UI combo box
    /// to show the current kind and to drive `default_for_tag`.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            GeneratorKind::Terrain(_) => "Terrain",
            GeneratorKind::Water { .. } => "Water",
            GeneratorKind::Portal { .. } => "Portal",
            GeneratorKind::LSystem { .. } => "LSystem",
            GeneratorKind::Shape { .. } => "Shape",
            GeneratorKind::Cuboid { .. } => "Cuboid",
            GeneratorKind::Sphere { .. } => "Sphere",
            GeneratorKind::Cylinder { .. } => "Cylinder",
            GeneratorKind::Capsule { .. } => "Capsule",
            GeneratorKind::Cone { .. } => "Cone",
            GeneratorKind::Torus { .. } => "Torus",
            GeneratorKind::Plane { .. } => "Plane",
            GeneratorKind::Tetrahedron { .. } => "Tetrahedron",
            GeneratorKind::Unknown => "Unknown",
        }
    }

    /// Build a default primitive kind for `tag`. Returns `None` for non-
    /// primitive tags — callers that want an L-system or Portal should
    /// construct those variants directly since they carry more state than
    /// sensible defaults capture.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        let mat = SovereignMaterialSettings::default();
        let zero = Fp(0.0);
        let zero3 = Fp3([0.0, 0.0, 0.0]);
        Some(match tag {
            "Cuboid" => GeneratorKind::Cuboid {
                size: Fp3([1.0, 1.0, 1.0]),
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Sphere" => GeneratorKind::Sphere {
                radius: Fp(0.5),
                resolution: 3,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Cylinder" => GeneratorKind::Cylinder {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Capsule" => GeneratorKind::Capsule {
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
            "Cone" => GeneratorKind::Cone {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Torus" => GeneratorKind::Torus {
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
            "Plane" => GeneratorKind::Plane {
                size: Fp2([1.0, 1.0]),
                subdivisions: 0,
                solid: true,
                material: mat,
                twist: zero,
                taper: zero,
                bend: zero3,
            },
            "Tetrahedron" => GeneratorKind::Tetrahedron {
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

/// A hierarchical generator: variant-specific payload + local transform +
/// child generators. Top-level entries in `RoomRecord::generators` are
/// `Generator`s; so are every node in any of their child trees. The wire
/// format flattens `kind` so each node is one tagged JSON object carrying
/// `$type`, the variant fields, `transform`, and `children`.
///
/// A `Vec<Generator>` is heap-allocated, so the recursion through `children`
/// is finite-sized at compile time without an explicit `Box`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Generator {
    #[serde(flatten)]
    pub kind: GeneratorKind,
    #[serde(default)]
    pub transform: TransformData,
    #[serde(default)]
    pub children: Vec<Generator>,
}

impl Generator {
    /// Wrap a kind with the canonical defaults: identity transform and no
    /// children. Use this when you want a leaf-shaped generator and don't
    /// care about hierarchy.
    pub fn from_kind(kind: GeneratorKind) -> Self {
        Self {
            kind,
            transform: TransformData::default(),
            children: Vec::new(),
        }
    }

    /// Convenience constructor for the canonical 1×1×1 cuboid.
    pub fn default_cuboid() -> Self {
        Self::from_kind(GeneratorKind::default_cuboid())
    }

    /// `true` when the variant is a parametric primitive. Delegates to the
    /// inner kind so call sites that already hold a `Generator` don't have
    /// to peel into `.kind` themselves.
    pub fn is_primitive(&self) -> bool {
        self.kind.is_primitive()
    }

    /// Short human-readable tag for the variant. See [`GeneratorKind::kind_tag`].
    pub fn kind_tag(&self) -> &'static str {
        self.kind.kind_tag()
    }

    /// Build a default primitive `Generator` (identity transform, no
    /// children) for `tag`. Returns `None` for non-primitive tags.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        GeneratorKind::default_primitive_for_tag(tag).map(Self::from_kind)
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::default_cuboid()
    }
}

/// Where and how a generator is instantiated.
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
