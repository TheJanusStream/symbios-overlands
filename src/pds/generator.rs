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
//! Two positional rules survive sanitisation: `Terrain` is **root-only**
//! (it may carry children — the "region blueprint" shape — but a Terrain
//! nested as a child is rewritten to a default cuboid because the terrain
//! plugin owns the single world heightmap), and `Water` is **child-only
//! and leaf-only** (it needs an ancestor's transform to anchor its volume,
//! and its own `children` list is cleared at sanitisation time).

use super::prim::PropMeshType;
use super::terrain::SovereignTerrainConfig;
use super::texture::SovereignMaterialSettings;
use super::types::{
    BiomeFilter, Fp, Fp2, Fp3, Fp4, ScatterBounds, TransformData, default_true, map_u16_as_string,
    u64_as_string,
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
    /// Strength of the avatar-wake ripple effect (Phase 1 of the
    /// interaction framework — see [`crate::interaction`]). `0.0`
    /// disables the effect entirely so existing scenes render
    /// unchanged. Higher values amplify the ripple per contact sample.
    pub wake_strength: Fp,
    /// Distance between ripple peaks in the wake, world metres.
    /// Smaller = tighter, busier ripples; larger = broader swells.
    pub wake_ripple_wavelength: Fp,
    /// Radial distance at which a single wake sample's contribution
    /// falls to `1/e` (~37%). Larger values produce wider wakes that
    /// reach further from the avatar; smaller values keep effects
    /// tightly localised.
    pub wake_decay_radius: Fp,
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
            // Wake effect off by default — existing rooms read as
            // pre-wake, only opt-in volumes show the ripples.
            wake_strength: Fp(0.0),
            wake_ripple_wavelength: Fp(1.5),
            wake_decay_radius: Fp(4.0),
        }
    }
}

/// Authored parameters for a [`GeneratorKind::RoadNetwork`] — a tensor-field
/// street grid that drapes over the parent terrain (see [`crate::urban`]). The
/// *config* is serialized / editable / seeded; the road *geometry* is recomputed
/// at load from this plus the heightmap, never stored. Like Water, a road
/// network is only valid as a child of a Terrain generator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoadConfig {
    /// Master toggle — a disabled network grows no roads (the editor "off").
    pub enabled: bool,
    /// Seed for the road layout *alone*, so an author can re-roll the streets
    /// without disturbing terrain or settlement. Seeded derivers default it
    /// from the room seed.
    #[serde(with = "u64_as_string")]
    pub seed: u64,
    /// Half-extent (m from spawn) of the district the network fills.
    pub district_half_extent: Fp,
    /// Spacing (m) between parallel major / minor roads.
    pub major_spacing: Fp,
    pub minor_spacing: Fp,
    /// Drivable-deck half-widths (m) by road class.
    pub major_half_width: Fp,
    pub minor_half_width: Fp,
    /// Curb lip height (m), curb-top flat width (m), and outward chamfer run (m).
    pub curb_height: Fp,
    pub curb_top_width: Fp,
    pub chamfer_width: Fp,
    /// Depth (m) the foundation skirt drops below the deck.
    pub skirt_depth: Fp,
    /// Whether the room grows buildings on the network's enclosed lots. When
    /// set, the terrain plugin's load-time populate-lots system derives
    /// footprints from this network and injects themed catalogue buildings onto
    /// them (see [`crate::terrain`] / [`crate::urban::extract_building_lots`]).
    /// Defaults on; older records without the field deserialise to `true`.
    #[serde(default = "default_populate_lots")]
    pub populate_lots: bool,
}

/// Serde default for [`RoadConfig::populate_lots`] — a road network in a record
/// predating the field still grows lot buildings.
fn default_populate_lots() -> bool {
    true
}

impl Default for RoadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed: 0,
            district_half_extent: Fp(170.0),
            major_spacing: Fp(95.0),
            minor_spacing: Fp(55.0),
            major_half_width: Fp(3.5),
            minor_half_width: Fp(2.0),
            curb_height: Fp(0.18),
            curb_top_width: Fp(0.22),
            chamfer_width: Fp(0.4),
            skirt_depth: Fp(5.0),
            populate_lots: true,
        }
    }
}

/// Variant-specific payload for a [`Generator`]. Open union: unrecognised
/// `$type` tags deserialise to `Unknown` instead of failing the whole record.
/// Vertex-torture parameters shared by every parametric primitive. Bundled
/// into one struct (rather than three flat fields on all eight variants) so a
/// new torture knob is a single field add — `#[serde(default)]` fills it on
/// records that predate it — instead of an edit to every variant and every
/// construction site. Applied CPU-side in `world_builder::prim`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct TortureParams {
    /// Radians of rotation around Y, linear in normalised height.
    pub twist: Fp,
    /// Per-axis taper: X and Z each scale by `1 - taper[axis] * t` toward the
    /// top. Equal components give a uniform taper (a cone / frustum); unequal
    /// ones give a wedge / fin.
    pub taper: Fp2,
    /// Quadratic top displacement `(x, y, z) * t²` — a single arc that pins
    /// the base and swings the top.
    pub bend: Fp3,
    /// Serpentine S-curve: a `sin(2π t)` lateral wave of amplitude `(x, z)`
    /// layered on top of `bend`, so a column can snake rather than only arc.
    pub s_bend: Fp2,
    /// Top-shear: a *linear* lateral displacement `(x, z) * t` that slides the
    /// top sideways relative to the pinned base (a parallelepiped / leaning
    /// tower / slanted roof). Unlike `bend` (quadratic, tangent at the base)
    /// the offset grows uniformly, so vertical edges stay straight but tilted.
    pub shear: Fp2,
    // --- Topology cuts (SL-style; honoured during mesh *generation* by the
    // unified sweep mesher, not the vertex post-pass; effective only on the
    // swept prims Sphere / Cylinder / Cone / Torus / Tube). Default = identity
    // (full sweep, full profile, solid). ---
    /// Kept angular fraction of the main sweep, `[begin, end]` in turns (0..1).
    /// `[0, 1]` = full revolution (no cut); `[0, 0.5]` keeps a half (half-
    /// cylinder trough, half-dome, half-torus archway). The opening gains two
    /// radial cap faces.
    pub path_cut: Fp2,
    /// Kept fraction of the cross-section / latitude, `[begin, end]` in 0..1.
    /// On a revolved profile (Sphere) this is the latitude band: `[0, 1]` full,
    /// `[0.5, 1]` a top dome, `[0, 0.5]` a bowl. On a Torus it opens the tube
    /// into a C-channel. Adds cap faces at the cut.
    pub profile_cut: Fp2,
    /// Bore as a fraction of the outer radius, `0..0.95`. `0` = solid; `> 0`
    /// hollows the prim (pipe / funnel / ring / shell) with an inner wall and
    /// annular rim caps — the general form of [`GeneratorKind::Tube`].
    pub hollow: Fp,
}

impl Default for TortureParams {
    fn default() -> Self {
        Self {
            twist: Fp(0.0),
            taper: Fp2([0.0, 0.0]),
            bend: Fp3([0.0, 0.0, 0.0]),
            s_bend: Fp2([0.0, 0.0]),
            shear: Fp2([0.0, 0.0]),
            path_cut: Fp2([0.0, 1.0]),
            profile_cut: Fp2([0.0, 1.0]),
            hollow: Fp(0.0),
        }
    }
}

impl TortureParams {
    /// `true` when no topology cut is active (full sweep, full profile, solid),
    /// so the mesher can take the cheap closed-surface path.
    pub fn cuts_are_identity(&self) -> bool {
        self.path_cut.0[0] <= 1e-4
            && self.path_cut.0[1] >= 1.0 - 1e-4
            && self.profile_cut.0[0] <= 1e-4
            && self.profile_cut.0[1] >= 1.0 - 1e-4
            && self.hollow.0 <= 1e-4
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum GeneratorKind {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    /// Water volume. Vertical position comes from the placement
    /// transform's translation.y — no separate level_offset field
    /// (removed as redundant; see [`crate::pds::room`]'s
    /// `default_for_did` for how the canonical homeworld places its
    /// water at the historical altitude via the placement transform).
    #[serde(rename = "network.symbios.gen.water")]
    Water {
        #[serde(default)]
        surface: WaterSurface,
    },

    /// Tensor-field road network draped over the parent terrain. Child-only
    /// (like Water); the sanitiser drops it at root. Its mesh is built by the
    /// terrain plugin from [`RoadConfig`] + the finished heightmap, so the
    /// compile dispatch treats it as inert (no entity), exactly as it does the
    /// Terrain root's own mesh.
    #[serde(rename = "network.symbios.gen.road_network")]
    RoadNetwork(RoadConfig),

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
        #[serde(with = "map_u16_as_string")]
        materials: HashMap<u16, SovereignMaterialSettings>,
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
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.sphere")]
    Sphere {
        radius: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.cylinder")]
    Cylinder {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.capsule")]
    Capsule {
        radius: Fp,
        length: Fp,
        latitudes: u32,
        longitudes: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.cone")]
    Cone {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.torus")]
    Torus {
        minor_radius: Fp,
        major_radius: Fp,
        minor_resolution: u32,
        major_resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.plane")]
    Plane {
        size: Fp2,
        subdivisions: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.tetrahedron")]
    Tetrahedron {
        size: Fp,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    /// Hollow cylinder (pipe / ring / well-curb). `radius` is the outer wall,
    /// `inner_radius` the bore (`< radius`); annular caps close the ends. The
    /// collider is a solid outer cylinder — the bore is not a walk-through
    /// volume.
    #[serde(rename = "network.symbios.gen.tube")]
    Tube {
        radius: Fp,
        inner_radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    /// Box with chamfered / rounded **vertical** edges — an extruded
    /// rounded-rectangle prism (columns, furniture, rounded buildings).
    /// `bevel` is the corner cut/radius; `bevel_segments` is `1` for a flat
    /// chamfer (octagonal prism) or higher for a rounded corner.
    #[serde(rename = "network.symbios.gen.bevel")]
    Bevel {
        size: Fp3,
        bevel: Fp,
        bevel_segments: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    /// Right-triangular prism — a ramp / roof pitch / buttress / eave. `size` is
    /// the bounding box; the slope rises from the front-bottom (`+Z`, `-Y`) to
    /// the back-top (`-Z`, `+Y`) across the full width (X).
    #[serde(rename = "network.symbios.gen.wedge")]
    Wedge {
        size: Fp3,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    /// Helical tube — a spring / screw / spiral-stair rail / horn / vine.
    /// `radius` is the helix radius, `tube_radius` the wire thickness, `pitch`
    /// the vertical rise per full turn, `turns` the revolution count, and
    /// `resolution` the segments per turn.
    #[serde(rename = "network.symbios.gen.helix")]
    Helix {
        radius: Fp,
        tube_radius: Fp,
        pitch: Fp,
        turns: Fp,
        resolution: u32,
        solid: bool,
        material: SovereignMaterialSettings,
        #[serde(default)]
        torture: TortureParams,
    },

    /// Hand-rolled CPU + ECS particle emitter. Spawns billboarded /
    /// velocity-aligned quads from a parametric shape (point / sphere /
    /// box / cone), integrates them with gravity / drag / constant
    /// acceleration, fades start→end size and colour over each
    /// particle's lifetime, and optionally collides them against
    /// terrain / water / colliders. WASM-friendly because no GPU compute
    /// is involved.
    ///
    /// Velocity inheritance: at spawn, each particle's initial velocity
    /// is `init_velocity + inherit_velocity * emitter_world_velocity`,
    /// where the emitter velocity comes from avian3d's `LinearVelocity`
    /// on the nearest `RigidBody` ancestor (covers the "particle
    /// generator parented under a moving avatar" case) or, failing
    /// that, a numerical derivative of the emitter's world transform.
    /// This lets exhaust trails move correctly with airplanes /
    /// hover-boats / running humanoids without any per-vehicle code.
    ///
    /// Optional texturing rides on the same [`SignSource`] union as the
    /// `Sign` generator and shares the
    /// [`BlobImageCache`](super::super::world_builder::image_cache::BlobImageCache),
    /// so a Sign panel and a particle emitter pointing at the same
    /// image issue one HTTPS round trip. `texture_atlas` plus
    /// `frame_mode` turns a sprite-sheet into per-particle animation
    /// (still / random / over-lifetime cycling).
    ///
    /// Determinism: every emitter carries a `seed`. Networked peers
    /// stepping the same dt path produce the same particle stream.
    #[serde(rename = "network.symbios.gen.particles")]
    ParticleSystem {
        emitter_shape: EmitterShape,

        /// Continuous emit rate in particles per second.
        rate_per_second: Fp,
        /// Per-cycle burst count. `0` disables bursts; `>0` emits that
        /// many particles at the start of each loop iteration (or at
        /// emitter activation for non-looping emitters).
        burst_count: u32,
        /// Hard cap on simultaneously-alive particles. Exhausting this
        /// budget causes new spawns to be skipped rather than evicting
        /// the oldest particle, which keeps the visual style stable
        /// under load.
        max_particles: u32,
        /// `true` re-emits forever; `false` stops emitting after
        /// `duration` seconds (existing particles continue to age out).
        looping: bool,
        /// Active-emit duration in seconds. For looping emitters this is
        /// the burst-cadence period.
        duration: Fp,

        /// Per-particle lifetime range in seconds. Sampled uniformly
        /// per spawn.
        lifetime_min: Fp,
        lifetime_max: Fp,
        /// Per-particle initial-speed range in metres / second. Sampled
        /// uniformly per spawn and scales the direction vector
        /// produced by `emitter_shape`.
        speed_min: Fp,
        speed_max: Fp,

        /// Multiplier on world gravity applied each frame. `1.0` =
        /// terrestrial, `0.0` = floats, `-1.0` = anti-gravity (smoke
        /// rising effect without a custom force).
        gravity_multiplier: Fp,
        /// Constant per-particle acceleration in world space (m/s²).
        /// Stacks with `gravity_multiplier * world_gravity`.
        acceleration: Fp3,
        /// Exponential linear damping per second. `0.0` = no drag,
        /// higher values brake the particle quadratically over its
        /// lifetime.
        linear_drag: Fp,

        /// Quad size at the start and end of the particle's lifetime;
        /// linearly interpolated each frame.
        start_size: Fp,
        end_size: Fp,
        /// RGBA at the start and end of lifetime; linearly
        /// interpolated each frame.
        start_color: Fp4,
        end_color: Fp4,
        blend_mode: ParticleBlendMode,
        /// `true` orients the quad to always face the camera (classic
        /// billboard); `false` aligns the quad along the velocity
        /// vector (streak / spark look).
        billboard: bool,

        simulation_space: SimulationSpace,
        /// Fraction of the emitter's world velocity added to each
        /// particle's initial velocity at spawn. `0.0` = ignore
        /// (sparks fly purely along their own emit direction), `1.0` =
        /// match emitter (running-dust effect), `>1.0` = exhaust
        /// (jets ahead). Sanitised to `[0, 2]`.
        inherit_velocity: Fp,

        /// Toggle particle collisions against the room's terrain
        /// heightfield. `false` = visual-only (cheaper).
        collide_terrain: bool,
        /// Toggle collisions against finite water surfaces.
        collide_water: bool,
        /// Toggle collisions against arbitrary avian3d colliders
        /// (placed primitives, walls, …).
        collide_colliders: bool,
        /// Restitution applied on collision: `0.0` = stick, `1.0` =
        /// perfect bounce.
        bounce: Fp,
        /// Friction applied to the tangential velocity on collision:
        /// `0.0` = frictionless slide, `1.0` = stick.
        friction: Fp,

        /// Deterministic emission seed. Same seed + same dt path on
        /// every peer produces the same particle stream.
        #[serde(with = "u64_as_string")]
        seed: u64,

        /// Optional per-particle texture. Resolves through the same
        /// [`SignSource`] union Sign uses, so a "leaf falling" emitter
        /// and a Sign signpost pointing at the same atlas image share
        /// one HTTPS round trip via [`super::super::world_builder::image_cache::BlobImageCache`].
        /// `None` keeps v1 behaviour: solid coloured quads tinted by
        /// `start_color` / `end_color`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        texture: Option<SignSource>,
        /// Treat the loaded texture as a sprite-sheet atlas of
        /// `rows × cols` cells. `None` uses the whole image as a single
        /// frame (the default).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        texture_atlas: Option<TextureAtlas>,
        /// How a particle picks its current atlas frame. `Still` keeps
        /// frame 0 forever; `RandomFrame` picks once at spawn (per-RNG-
        /// stream draw) so different particles show different sprites
        /// from the same atlas; `OverLifetime { fps }` cycles through
        /// the frame array at the configured rate.
        #[serde(default)]
        frame_mode: AnimationFrameMode,
        /// Sampler filter applied to the loaded image. `Linear` is the
        /// natural smooth filtering for soft sprites; `Nearest` for
        /// pixel-art / retro looks. The cache keys on filter so a
        /// Linear and a Nearest request for the same source produce
        /// two distinct GPU images, neither stomping the other.
        #[serde(default)]
        texture_filter: TextureFilter,
        /// Procedurally-baked particle sprite, generated locally instead of
        /// fetched. When this is set (non-`None`) and `texture` is `None`,
        /// the emitter bakes this generator at
        /// [`crate::config::textures::PARTICLE_CELL`] per atlas cell and
        /// uses the result as the particle albedo. The sprite generators
        /// (SoftDisc, Snowflake, Flame, …) carry `variant_rows × variant_cols`,
        /// which auto-derives the `texture_atlas` so a `RandomFrame` emitter
        /// draws a different variant per particle from one bake. The legacy
        /// `texture` reference wins when both are set, so already-published
        /// records are unaffected.
        #[serde(default)]
        procedural_texture: super::texture::SovereignTextureConfig,
    },

    /// Image-bearing panel — a flat plane textured with a fetched image
    /// from one of three [`SignSource`] variants. Subsumes the standalone
    /// "profile picture panel" use case (Portal already does the same fetch
    /// internally). `size` is the panel extent in metres, `uv_repeat` /
    /// `uv_offset` let the user tile / pan the texture without resizing the
    /// mesh, and the StandardMaterial toggles surface every common knob a
    /// signpost / billboard / pfp panel might need.
    #[serde(rename = "network.symbios.gen.sign")]
    Sign {
        source: SignSource,
        /// Panel size in metres along the local X / Z axes.
        size: Fp2,
        /// UV repeat factor per axis. `1.0` = the texture covers the panel
        /// once; `2.0` = tiled twice along that axis.
        uv_repeat: Fp2,
        /// UV offset per axis applied after the repeat. Useful for panning
        /// across an atlas image without changing the panel size.
        uv_offset: Fp2,
        /// Tint + emissive + PBR knobs. The texture overrides the
        /// procedural slot — set `texture` to `None` so the loaded image
        /// is the only colour source.
        material: SovereignMaterialSettings,
        /// `true` renders both faces of the plane (and disables backface
        /// culling). Useful for free-standing signs viewable from either
        /// side; `false` for wall-mounted decals.
        double_sided: bool,
        /// Translucency mode. `Opaque` (no alpha), `Mask(cutoff)` for
        /// punch-through PNGs (cutout signs), `Blend` for soft-edged
        /// translucent textures. Mirrors Bevy's `AlphaMode` open-union-
        /// style.
        alpha_mode: AlphaModeKind,
        /// `true` skips PBR lighting, painting the texture flat regardless
        /// of sun angle. Critical for legibility on profile pics / signs.
        unlit: bool,
    },

    #[serde(other)]
    Unknown,
}

/// Image-source alias retained for backwards compatibility. The canonical
/// type is [`SovereignAssetReference`] — the same enum was originally
/// introduced here as `SignSource` but generalised when texture and audio
/// dropdowns gained their own Referenced variants. The `$type` wire tags
/// (`network.symbios.sign.*`) are unchanged so already-published records
/// keep deserialising; only the in-code name moved.
///
/// All three variants still resolve through the shared `BlobImageCache`
/// in `world_builder::image_cache` for image consumers (Sign panels,
/// particle textures); audio consumers go through the sibling
/// `BlobAudioCache` pattern.
///
/// [`SovereignAssetReference`]: crate::pds::asset_reference::SovereignAssetReference
pub use crate::pds::asset_reference::SovereignAssetReference as SignSource;

/// Open-union mirror of Bevy's `AlphaMode`. Wire-tagged so an unknown
/// variant from a forward-compatible record decodes to `Unknown` rather
/// than failing the whole generator.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum AlphaModeKind {
    /// Fully opaque — no alpha lookup, fastest.
    #[serde(rename = "network.symbios.alpha.opaque")]
    #[default]
    Opaque,
    /// Hard cutout: alpha < `cutoff` → discard, alpha ≥ `cutoff` → opaque.
    /// `cutoff` is in `[0, 1]`; the sanitiser clamps.
    #[serde(rename = "network.symbios.alpha.mask")]
    Mask { cutoff: Fp },
    /// Standard alpha blending. Sorted by Bevy's transparent-pass writer.
    #[serde(rename = "network.symbios.alpha.blend")]
    Blend,

    #[serde(other)]
    Unknown,
}

/// Emitter-shape open union for [`GeneratorKind::ParticleSystem`]. Each
/// variant defines the spawn-position distribution and the default
/// emit-direction; per-variant payload fields tune the shape itself.
/// `Unknown` keeps a record from a future engine version round-tripping.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum EmitterShape {
    /// Single point emitter at the local origin. Default emit direction
    /// is local +Y; particle spread comes from per-particle randomness
    /// in the speed sample rather than the shape itself.
    #[serde(rename = "network.symbios.particle.point")]
    #[default]
    Point,
    /// Solid sphere of `radius`. Particles spawn at a uniform-random
    /// position inside the sphere and inherit a default outward emit
    /// direction (radial unit vector).
    #[serde(rename = "network.symbios.particle.sphere")]
    Sphere { radius: Fp },
    /// Axis-aligned box of `half_extents`. Particles spawn uniformly
    /// inside; emit direction defaults to local +Y.
    #[serde(rename = "network.symbios.particle.box")]
    Box { half_extents: Fp3 },
    /// Cone with apex at the local origin pointing along local +Y.
    /// `half_angle` (radians) bounds the spawn cone; `height` scales
    /// the cone's depth so particles can spawn anywhere along it.
    #[serde(rename = "network.symbios.particle.cone")]
    Cone { half_angle: Fp, height: Fp },

    #[serde(other)]
    Unknown,
}

/// Particle blend-mode open union. `Alpha` is standard front-to-back
/// transparency (smoke, soft sprites); `Additive` is brightness-additive
/// (sparks, fire, glow). Mirrors the two surface-level blend modes any
/// reasonable particle system supports without exposing the full GPU
/// blend-state matrix.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum ParticleBlendMode {
    #[serde(rename = "network.symbios.particle.blend.alpha")]
    #[default]
    Alpha,
    #[serde(rename = "network.symbios.particle.blend.additive")]
    Additive,

    #[serde(other)]
    Unknown,
}

/// Simulation-space open union for [`GeneratorKind::ParticleSystem`].
/// `Local` parents particles under the emitter (auras and clouds that
/// follow the emitter); `World` spawns particles unparented in world
/// coordinates so they are left behind as the emitter moves (exhaust,
/// dust trails).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum SimulationSpace {
    #[serde(rename = "network.symbios.particle.space.world")]
    #[default]
    World,
    #[serde(rename = "network.symbios.particle.space.local")]
    Local,

    #[serde(other)]
    Unknown,
}

/// Sprite-sheet atlas dimensions for a textured particle. The image is
/// divided into a `rows × cols` grid; each cell is one animation frame
/// (or one randomised sprite, depending on
/// [`AnimationFrameMode`]). The sanitiser caps each axis at 16, so an
/// atlas tops out at 256 frames — well past any plausible particle
/// effect and inside the per-frame mesh-cache budget.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TextureAtlas {
    pub rows: u32,
    pub cols: u32,
}

impl Default for TextureAtlas {
    fn default() -> Self {
        Self { rows: 1, cols: 1 }
    }
}

/// Frame-cycling mode for textured particles.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum AnimationFrameMode {
    /// Single static frame (frame 0). Default — matches a solid
    /// non-animated sprite.
    #[serde(rename = "network.symbios.particle.frame.still")]
    #[default]
    Still,
    /// Each particle picks one frame uniformly at spawn and keeps it
    /// for its entire lifetime. Useful when an atlas holds a set of
    /// "leaf shape" or "snowflake" variants and you want visual
    /// variety without animation.
    #[serde(rename = "network.symbios.particle.frame.random")]
    RandomFrame,
    /// Cycle through every frame in `rows × cols` order at the
    /// configured rate. Particles whose lifetime is shorter than
    /// `frame_count / fps` truncate; longer lifetimes loop back to
    /// frame 0 (modulo).
    #[serde(rename = "network.symbios.particle.frame.over_lifetime")]
    OverLifetime { fps: Fp },

    #[serde(other)]
    Unknown,
}

/// Sampler filter applied to a textured particle's image. `Linear`
/// smooth-filters (default; soft sprites); `Nearest` snaps to texels
/// (pixel-art look). The image cache keys on this so a Linear and a
/// Nearest request for the same source coexist as separate Image
/// assets.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum TextureFilter {
    #[serde(rename = "network.symbios.particle.filter.linear")]
    #[default]
    Linear,
    #[serde(rename = "network.symbios.particle.filter.nearest")]
    Nearest,

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
            torture: TortureParams::default(),
        }
    }

    /// Shared read access to the vertex-torture parameters of any parametric
    /// primitive; `None` for non-primitive variants. Centralises the
    /// eight-arm match so the mesher, sanitiser, and editor don't each repeat
    /// it.
    pub fn torture(&self) -> Option<&TortureParams> {
        match self {
            GeneratorKind::Cuboid { torture, .. }
            | GeneratorKind::Sphere { torture, .. }
            | GeneratorKind::Cylinder { torture, .. }
            | GeneratorKind::Capsule { torture, .. }
            | GeneratorKind::Cone { torture, .. }
            | GeneratorKind::Torus { torture, .. }
            | GeneratorKind::Plane { torture, .. }
            | GeneratorKind::Tetrahedron { torture, .. }
            | GeneratorKind::Tube { torture, .. }
            | GeneratorKind::Bevel { torture, .. }
            | GeneratorKind::Wedge { torture, .. }
            | GeneratorKind::Helix { torture, .. } => Some(torture),
            _ => None,
        }
    }

    /// Shared mutable access to a primitive's vertex-torture parameters; `None`
    /// for non-primitive variants.
    pub fn torture_mut(&mut self) -> Option<&mut TortureParams> {
        match self {
            GeneratorKind::Cuboid { torture, .. }
            | GeneratorKind::Sphere { torture, .. }
            | GeneratorKind::Cylinder { torture, .. }
            | GeneratorKind::Capsule { torture, .. }
            | GeneratorKind::Cone { torture, .. }
            | GeneratorKind::Torus { torture, .. }
            | GeneratorKind::Plane { torture, .. }
            | GeneratorKind::Tetrahedron { torture, .. }
            | GeneratorKind::Tube { torture, .. }
            | GeneratorKind::Bevel { torture, .. }
            | GeneratorKind::Wedge { torture, .. }
            | GeneratorKind::Helix { torture, .. } => Some(torture),
            _ => None,
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
                | GeneratorKind::Tube { .. }
                | GeneratorKind::Bevel { .. }
                | GeneratorKind::Wedge { .. }
                | GeneratorKind::Helix { .. }
        )
    }

    /// Short human-readable tag for the variant — used by the UI combo box
    /// to show the current kind and to key into
    /// `ui::room::construct::make_default_for_kind`.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            GeneratorKind::Terrain(_) => "Terrain",
            GeneratorKind::Water { .. } => "Water",
            GeneratorKind::RoadNetwork(_) => "RoadNetwork",
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
            GeneratorKind::Tube { .. } => "Tube",
            GeneratorKind::Bevel { .. } => "Bevel",
            GeneratorKind::Wedge { .. } => "Wedge",
            GeneratorKind::Helix { .. } => "Helix",
            GeneratorKind::Sign { .. } => "Sign",
            GeneratorKind::ParticleSystem { .. } => "ParticleSystem",
            GeneratorKind::Unknown => "Unknown",
        }
    }

    /// Build a default primitive kind for `tag`. Returns `None` for non-
    /// primitive tags — callers that want an L-system or Portal should
    /// construct those variants directly since they carry more state than
    /// sensible defaults capture.
    pub fn default_primitive_for_tag(tag: &str) -> Option<Self> {
        let mat = SovereignMaterialSettings::default();
        Some(match tag {
            "Cuboid" => GeneratorKind::Cuboid {
                size: Fp3([1.0, 1.0, 1.0]),
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Sphere" => GeneratorKind::Sphere {
                radius: Fp(0.5),
                resolution: 3,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Cylinder" => GeneratorKind::Cylinder {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Capsule" => GeneratorKind::Capsule {
                radius: Fp(0.5),
                length: Fp(1.0),
                latitudes: 8,
                longitudes: 16,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Cone" => GeneratorKind::Cone {
                radius: Fp(0.5),
                height: Fp(1.0),
                resolution: 16,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Torus" => GeneratorKind::Torus {
                minor_radius: Fp(0.1),
                major_radius: Fp(0.5),
                minor_resolution: 12,
                major_resolution: 24,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Plane" => GeneratorKind::Plane {
                size: Fp2([1.0, 1.0]),
                subdivisions: 0,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Tetrahedron" => GeneratorKind::Tetrahedron {
                size: Fp(1.0),
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Tube" => GeneratorKind::Tube {
                radius: Fp(0.5),
                inner_radius: Fp(0.3),
                height: Fp(1.0),
                resolution: 24,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Bevel" => GeneratorKind::Bevel {
                size: Fp3([1.0, 1.0, 1.0]),
                bevel: Fp(0.15),
                bevel_segments: 3,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Wedge" => GeneratorKind::Wedge {
                size: Fp3([1.0, 1.0, 1.0]),
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            "Helix" => GeneratorKind::Helix {
                radius: Fp(0.5),
                tube_radius: Fp(0.1),
                pitch: Fp(0.4),
                turns: Fp(3.0),
                resolution: 24,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            _ => return None,
        })
    }

    /// Canonical default `Sign` — a 1×1 m unlit, opaque, single-sided panel
    /// with an empty URL source. Used by the UI "+ Sign" entry and by
    /// `ui::room::construct::make_default_for_kind`.
    pub fn default_sign() -> Self {
        GeneratorKind::Sign {
            source: SignSource::default(),
            size: Fp2([1.0, 1.0]),
            uv_repeat: Fp2([1.0, 1.0]),
            uv_offset: Fp2([0.0, 0.0]),
            material: SovereignMaterialSettings::default(),
            double_sided: false,
            alpha_mode: AlphaModeKind::Opaque,
            unlit: true,
        }
    }

    /// Canonical default `ParticleSystem` — a small upward-spraying
    /// emitter with 32 particles/s, 2 s lifetime, white→fade-out
    /// alpha-blended particles on a soft-disc sprite (#367, so a
    /// freshly-added emitter reads as soft motes rather than hard
    /// squares), no inheritance, no collisions. Used by the UI
    /// "+ ParticleSystem" entry; the editor surfaces every parameter —
    /// including the sprite picker — for tuning afterwards.
    pub fn default_particles() -> Self {
        GeneratorKind::ParticleSystem {
            emitter_shape: EmitterShape::Cone {
                half_angle: Fp(0.4),
                height: Fp(0.5),
            },
            rate_per_second: Fp(32.0),
            burst_count: 0,
            max_particles: 128,
            looping: true,
            duration: Fp(1.0),
            lifetime_min: Fp(1.0),
            lifetime_max: Fp(2.0),
            speed_min: Fp(1.0),
            speed_max: Fp(2.0),
            gravity_multiplier: Fp(0.0),
            acceleration: Fp3([0.0, 0.0, 0.0]),
            linear_drag: Fp(0.5),
            start_size: Fp(0.1),
            end_size: Fp(0.0),
            start_color: Fp4([1.0, 1.0, 1.0, 1.0]),
            end_color: Fp4([1.0, 1.0, 1.0, 0.0]),
            blend_mode: ParticleBlendMode::Alpha,
            billboard: true,
            simulation_space: SimulationSpace::World,
            inherit_velocity: Fp(0.0),
            collide_terrain: false,
            collide_water: false,
            collide_colliders: false,
            bounce: Fp(0.3),
            friction: Fp(0.5),
            seed: 0xC0FFEE,
            texture: None,
            texture_atlas: None,
            frame_mode: AnimationFrameMode::Still,
            texture_filter: TextureFilter::Linear,
            procedural_texture: super::texture::SovereignTextureConfig::SoftDisc(
                super::texture::SovereignSoftDiscConfig::default(),
            ),
        }
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Generator {
    #[serde(flatten)]
    pub kind: GeneratorKind,
    #[serde(default)]
    pub transform: TransformData,
    #[serde(default)]
    pub children: Vec<Generator>,
    /// Optional emissive audio source attached to this node — spatially
    /// played at the node's world position by Bevy's spatial audio
    /// pipeline. Forward-compat across older records: missing field
    /// decodes via `#[serde(default)]` to
    /// [`SovereignAudioConfig::None`](super::audio::SovereignAudioConfig::None)
    /// (silent). Set non-None by
    /// catalogue entries that want a construct to hum / chime / drone
    /// at its location (e.g. the teleporter's portal core).
    #[serde(default)]
    pub audio: super::audio::SovereignAudioConfig,
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
            audio: super::audio::SovereignAudioConfig::None,
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
        /// When terrain-snapped, refuse submerged ground: the compiler
        /// walks the anchor along its bearing through the origin
        /// (preserving a spawn-facing yaw) until the terrain rises
        /// above the room's water line. Used by the seeded landmark so
        /// a coastal villa doesn't spawn waist-deep in the sea.
        /// `#[serde(default)]` keeps older records decoding unchanged.
        #[serde(default)]
        avoid_water: bool,
        /// Dry-land clearance radius (m) for [`Self::Absolute::avoid_water`]:
        /// the walk requires the centre *and* a ring of samples at this
        /// radius to clear the water line, so a wide footprint can't pass
        /// on a dry anchor while the rest of the building floods. `0`
        /// checks the centre only.
        #[serde(default)]
        avoid_water_clearance: Fp,
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
        /// Reject scatter points that fall inside the room's road-network
        /// district — a circle of radius `RoadConfig::district_half_extent`
        /// around spawn. Keeps the seeded natural scatters (trees, boulders)
        /// clear of the built-up urban area (roads *and* lot buildings)
        /// without needing an annulus bounds shape. Resolved at compile
        /// against [`crate::pds::room::find_road_config`]; a no-op in a room
        /// with no enabled road network. Defaults `false`.
        #[serde(default)]
        avoid_urban: bool,
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
