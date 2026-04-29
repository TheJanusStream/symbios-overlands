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

    /// Hand-rolled CPU + ECS particle emitter. Spawns billboarded /
    /// velocity-aligned coloured quads from a parametric shape (point /
    /// sphere / box / cone), integrates them with gravity / drag /
    /// constant acceleration, fades start→end size and colour over each
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
    /// Determinism: every emitter carries a `seed`. Networked peers
    /// stepping the same dt path produce the same particle stream.
    /// Textured particles are tracked separately as a follow-up; this
    /// variant ships with coloured quads only.
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

/// Image-source open union for [`GeneratorKind::Sign`]. All three
/// variants resolve through the shared `BlobImageCache` in
/// `world_builder::image_cache`, so a room with multiple Signs pointing at
/// the same source issues one HTTPS round trip and reuses the resulting
/// `Handle<Image>` across every panel. `Unknown` keeps a record authored
/// by a future engine version round-tripping cleanly.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "$type")]
pub enum SignSource {
    /// Direct HTTPS image URL. Bytes are decoded via the `image` crate; on
    /// WASM the request goes through the same `reqwest` client as every
    /// other HTTP fetch. CORS is the caller's problem — a host that
    /// doesn't serve `Access-Control-Allow-Origin: *` will fail to load on
    /// web.
    #[serde(rename = "network.symbios.sign.url")]
    Url { url: String },
    /// ATProto blob ref pinned to a specific DID. Resolves the DID's PDS
    /// then calls `com.atproto.sync.getBlob?did=…&cid=…`. Use this when
    /// the image is hosted on a known PDS as a content-addressed blob.
    #[serde(rename = "network.symbios.sign.atproto_blob")]
    AtprotoBlob { did: String, cid: String },
    /// "This DID's current profile picture" — fetches `app.bsky.actor.
    /// getProfile` and resolves the avatar URL through the same path
    /// Portal uses today. Self-updating: a refresh between sessions picks
    /// up a new pfp without changing the record.
    #[serde(rename = "network.symbios.sign.did_pfp")]
    DidPfp { did: String },

    #[serde(other)]
    Unknown,
}

impl Default for SignSource {
    fn default() -> Self {
        SignSource::Url { url: String::new() }
    }
}

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

    /// Canonical default `Sign` — a 1×1 m unlit, opaque, single-sided panel
    /// with an empty URL source. Used by the UI "+ Sign" entry and by
    /// [`default_for_tag`].
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
    /// alpha-blended quads, no inheritance, no collisions. Used by
    /// the UI "+ ParticleSystem" entry; the editor surfaces every
    /// parameter for tuning afterwards.
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
