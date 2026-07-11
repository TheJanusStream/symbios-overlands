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
    BiomeFilter, Fp, Fp2, Fp3, Fp4, ScatterBounds, TransformData, default_true, is_false, is_true,
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
#[derive(Deserialize, Clone, Debug, PartialEq)]
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

// Default-eliding wire format (#695); the container `#[serde(default)]`
// above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(WaterSurface {
    shallow_color,
    deep_color,
    roughness,
    metallic,
    reflectance,
    wave_scale,
    wave_speed,
    wave_direction,
    wave_choppiness,
    foam_amount,
    flow_strength,
    flow_amount,
    wake_strength,
    wake_ripple_wavelength,
    wake_decay_radius,
});

/// Authored parameters for a [`GeneratorKind::RoadNetwork`] — a tensor-field
/// street grid that drapes over the parent terrain (see [`crate::urban`]). The
/// *config* is serialized / editable / seeded; the road *geometry* is recomputed
/// at load from this plus the heightmap, never stored. Like Water, a road
/// network is only valid as a child of a Terrain generator.
///
/// Default-eliding wire format (#695): fields matching
/// [`RoadConfig::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
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

crate::pds::serde_util::impl_default_eliding_serialize!(RoadConfig {
    enabled,
    seed via u64_as_string(u64),
    district_half_extent,
    major_spacing,
    minor_spacing,
    major_half_width,
    minor_half_width,
    curb_height,
    curb_top_width,
    chamfer_width,
    skirt_depth,
    populate_lots,
});

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

/// Vertex-torture parameters shared by every parametric primitive. Bundled
/// into one struct (rather than three flat fields on all eight variants) so a
/// new torture knob is a single field add — `#[serde(default)]` fills it on
/// records that predate it — instead of an edit to every variant and every
/// construction site. Applied CPU-side in `world_builder::prim`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct TortureParams {
    /// Radians of rotation around Y, linear in normalised height.
    pub twist: Fp,
    /// Per-axis taper: X and Z each scale by `1 - taper[axis] * t` toward the
    /// top. Equal components give a uniform taper (a cone / frustum); unequal
    /// ones give a wedge / fin.
    pub taper: Fp2,
    /// Per-axis **bottom** taper: X and Z each scale by
    /// `1 - taper_bottom[axis] * (1 - t)` toward the base, composing with
    /// `taper` so one prim can narrow at both ends (a lens / spearhead) —
    /// without the old author-it-upside-down-and-flip-π workaround that
    /// top-only taper forced on every downward-narrowing form.
    pub taper_bottom: Fp2,
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
    /// Per-axis mid-profile bulge (+) / pinch (−): X and Z scale gain
    /// `bulge[axis] * sin(π t)` — zero at both ends, peaking at mid-height.
    /// One positive slider turns a straight capsule into a muscle / belly /
    /// tree-trunk swell; a negative one gives a waist / hourglass. The
    /// combined per-axis scale is floored just above zero in the deform pass
    /// so a hard pinch collapses to the axis instead of inverting the surface.
    pub bulge: Fp2,
    // --- Topology cuts (SL-style; honoured during mesh *generation*, not
    // the vertex post-pass). As of #725 every prim honours them except
    // Plane (no revolve axis). Semantics per family: revolved prims cut
    // angularly / by band / by bore; box prims (Cuboid / Bevel) take a pie
    // wedge / vertical slice / matching bore; tubes (Helix / Spine) open
    // into channels / trim their path / become shells; Lathe trims the
    // kept arc-length band of its silhouette; BlobGroup applies them as
    // hard CSG on its distance field (Y-slab slice / pie wedge / inner
    // shell). Default = identity (full sweep, full profile, solid). ---
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
            taper_bottom: Fp2([0.0, 0.0]),
            bend: Fp3([0.0, 0.0, 0.0]),
            s_bend: Fp2([0.0, 0.0]),
            shear: Fp2([0.0, 0.0]),
            bulge: Fp2([0.0, 0.0]),
            path_cut: Fp2([0.0, 1.0]),
            profile_cut: Fp2([0.0, 1.0]),
            hollow: Fp(0.0),
        }
    }
}

// Default-eliding wire format (#695): an identity TortureParams — the
// overwhelmingly common case across catalogue prims — serializes as `{}`,
// and any prim whose torture IS identity omits the field entirely via
// `skip_serializing_if` at the variant field. The container
// `#[serde(default)]` above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(TortureParams {
    twist,
    taper,
    taper_bottom,
    bend,
    s_bend,
    shear,
    bulge,
    path_cut,
    profile_cut,
    hollow,
});

impl TortureParams {
    /// `true` when the whole struct equals its default — the wire-format
    /// skip predicate for prim `torture` fields (#695).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `true` when no vertex deform is active (twist / taper / bulge / bend /
    /// S-bend / shear all zero). Meshers use this to skip the vertical
    /// subdivisions that only exist to give the deform pass mid-height
    /// vertices to move — a 2-ring wall renders a `sin(π t)` bulge as
    /// nothing at all.
    pub fn deforms_are_identity(&self) -> bool {
        let flat2 = |v: &Fp2| v.0[0].abs() < 1e-6 && v.0[1].abs() < 1e-6;
        self.twist.0.abs() < 1e-6
            && flat2(&self.taper)
            && flat2(&self.taper_bottom)
            && flat2(&self.bulge)
            && self.bend.0.iter().all(|c| c.abs() < 1e-6)
            && flat2(&self.s_bend)
            && flat2(&self.shear)
    }

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

/// Primitive shape of one [`BlobElement`]. Open union so future shapes
/// degrade gracefully on older clients — an `Unknown` element evaluates as
/// a sphere rather than failing the record.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum BlobShape {
    #[serde(rename = "network.symbios.blob.sphere")]
    #[default]
    Sphere,
    /// Capsule along the element's local +Y axis.
    #[serde(rename = "network.symbios.blob.capsule")]
    Capsule,
    #[serde(rename = "network.symbios.blob.ellipsoid")]
    Ellipsoid,
    /// Axis-aligned box (pre-rotation) — flat faces and hard masses inside
    /// smooth blends: pedestals, slabs, jaws.
    #[serde(rename = "network.symbios.blob.box")]
    Box,
    /// Capped cylinder along the element's local +Y axis.
    #[serde(rename = "network.symbios.blob.cylinder")]
    Cylinder,
    /// Torus lying in the element's local XZ plane (axis +Y).
    #[serde(rename = "network.symbios.blob.torus")]
    Torus,
    /// Capped cone: base radius `radii[0]` at local −Y, tip radius
    /// `radii[2]` at +Y (the sanitiser's 0.01 floor ≈ a point, so plain
    /// cones need no extra field; a real tip radius makes the
    /// truncated-cone limb segment).
    #[serde(rename = "network.symbios.blob.cone")]
    Cone,
    #[serde(other)]
    Unknown,
}

/// UV projection a [`GeneratorKind::BlobGroup`] bakes into its mesh (#739).
/// Surface nets has no analytic parameterisation, so texture coordinates
/// come from projecting each vertex — and which projection reads well is
/// shape-dependent, so it's an authorable knob rather than a constant.
/// Open union so future modes degrade gracefully on older clients — an
/// `Unknown` mode meshes as `Spherical`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum UvMapping {
    /// Equirectangular projection of each vertex's direction from the
    /// surface centroid — the original mapping and the wire default. Reads
    /// well on roundish masses; elongated or multi-lobed groups stretch
    /// (direction ignores distance) and concave regions repeat the texture
    /// where two surface points share a direction.
    #[serde(rename = "network.symbios.uv.spherical")]
    #[default]
    Spherical,
    /// Baked tri-planar box projection: each triangle projects along the
    /// axis its normal leans into most, at one uniform scale, so texel
    /// density is even everywhere. The all-round distortion fix; strongly
    /// patterned textures show seams where the projection axis changes.
    #[serde(rename = "network.symbios.uv.box")]
    Box,
    /// Wrap around the prim-local Y axis (the same reference axis the
    /// topology cuts use): U is azimuth, V climbs with height scaled so a
    /// texel stays square against the group's mean circumference (the
    /// swept prims' convention). Suits limbs, trunks and columns; surface
    /// facing straight up or down swirls.
    #[serde(rename = "network.symbios.uv.cylindrical")]
    Cylindrical,
    /// Flat projection along local X (texture lies on the YZ plane).
    #[serde(rename = "network.symbios.uv.planar_x")]
    PlanarX,
    /// Flat projection along local Y — top-down, for slab-like masses.
    #[serde(rename = "network.symbios.uv.planar_y")]
    PlanarY,
    /// Flat projection along local Z (texture lies on the XY plane).
    #[serde(rename = "network.symbios.uv.planar_z")]
    PlanarZ,
    #[serde(other)]
    Unknown,
}

impl UvMapping {
    /// Wire-format skip predicate: the default mode stays off the wire, so
    /// pre-#739 records re-serialise byte-identically (#695 elision
    /// discipline).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One stamp in a [`GeneratorKind::BlobGroup`]'s ordered edit list — the
/// Dreams model: elements evaluate in list order, each smoothly added to
/// (or carved out of) everything before it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BlobElement {
    pub shape: BlobShape,
    /// Element centre in the prim's local space.
    pub position: Fp3,
    /// Element orientation (unit quaternion) — orients a capsule's axis or
    /// an ellipsoid's semi-axes; irrelevant for a sphere.
    pub rotation: Fp4,
    /// Per-shape size: Sphere uses `radii[0]`; Ellipsoid and Box read all
    /// three (semi-axes / half-extents); Capsule and Cylinder read
    /// `radii[0]` = radius and `radii[1]` = half-length / half-height;
    /// Cone reads `radii[0]` = base radius, `radii[1]` = half-height and
    /// `radii[2]` = tip radius; Torus reads `radii[0]` = ring (major)
    /// radius and `radii[1]` = tube (minor) radius.
    pub radii: Fp3,
    /// `true` carves this element out of the accumulated shape (smooth
    /// subtraction — eye sockets, nostrils, creases) instead of adding it.
    pub subtract: bool,
    /// Smooth-blend distance (metres): how far from contact this element
    /// starts merging with the accumulated surface. `0` = hard union.
    pub blend: Fp,
}

impl Default for BlobElement {
    fn default() -> Self {
        Self {
            shape: BlobShape::Sphere,
            position: Fp3([0.0, 0.0, 0.0]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            radii: Fp3([0.25, 0.25, 0.25]),
            subtract: false,
            blend: Fp(0.1),
        }
    }
}

/// One control point of a [`GeneratorKind::Spine`]: a local-space position
/// the tube's centreline passes through, and the tube radius there. Both are
/// interpolated with the same Catmull-Rom spline, so the radius flows as
/// smoothly along the tube as the path does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SpinePoint {
    pub position: Fp3,
    pub radius: Fp,
}

/// One station of a [`GeneratorKind::Lathe`] profile: radial distance from
/// the Y axis at a given local height. Stations are meshed bottom-to-top in
/// list order; a zero radius pinches the surface onto the axis (a pole).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LathePoint {
    pub radius: Fp,
    pub height: Fp,
}

/// Full parameter set of a [`GeneratorKind::ParticleSystem`] emitter (#648).
///
/// Lives behind a `Box` on the variant so the enum's stack size doesn't
/// carry all ~30 fields (the same shape as `LocomotionConfig`'s boxed
/// `*Params`). Wire compat: an internally-tagged (`$type`) enum serialises
/// a newtype variant's struct fields inline beside the tag — byte-identical
/// to the old struct-variant form, so existing records round-trip
/// unchanged (guarded by the `particle_params_wire_format_*` tests).
///
/// Default-eliding wire format (#695): fields matching
/// [`ParticleParams::default`] are omitted on write; the container
/// `#[serde(default)]` restores them on read.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ParticleParams {
    pub emitter_shape: EmitterShape,

    /// Continuous emit rate in particles per second.
    pub rate_per_second: Fp,
    /// Per-cycle burst count. `0` disables bursts; `>0` emits that
    /// many particles at the start of each loop iteration (or at
    /// emitter activation for non-looping emitters).
    pub burst_count: u32,
    /// Hard cap on simultaneously-alive particles. Exhausting this
    /// budget causes new spawns to be skipped rather than evicting
    /// the oldest particle, which keeps the visual style stable
    /// under load.
    pub max_particles: u32,
    /// `true` re-emits forever; `false` stops emitting after
    /// `duration` seconds (existing particles continue to age out).
    pub looping: bool,
    /// Active-emit duration in seconds. For looping emitters this is
    /// the burst-cadence period.
    pub duration: Fp,

    /// Per-particle lifetime range in seconds. Sampled uniformly
    /// per spawn.
    pub lifetime_min: Fp,
    pub lifetime_max: Fp,
    /// Per-particle initial-speed range in metres / second. Sampled
    /// uniformly per spawn and scales the direction vector
    /// produced by `emitter_shape`.
    pub speed_min: Fp,
    pub speed_max: Fp,

    /// Multiplier on world gravity applied each frame. `1.0` =
    /// terrestrial, `0.0` = floats, `-1.0` = anti-gravity (smoke
    /// rising effect without a custom force).
    pub gravity_multiplier: Fp,
    /// Constant per-particle acceleration in world space (m/s²).
    /// Stacks with `gravity_multiplier * world_gravity`.
    pub acceleration: Fp3,
    /// Exponential linear damping per second. `0.0` = no drag,
    /// higher values brake the particle quadratically over its
    /// lifetime.
    pub linear_drag: Fp,

    /// Quad size at the start and end of the particle's lifetime;
    /// linearly interpolated each frame.
    pub start_size: Fp,
    pub end_size: Fp,
    /// RGBA at the start and end of lifetime; linearly
    /// interpolated each frame.
    pub start_color: Fp4,
    pub end_color: Fp4,
    pub blend_mode: ParticleBlendMode,
    /// `true` orients the quad to always face the camera (classic
    /// billboard); `false` aligns the quad along the velocity
    /// vector (streak / spark look).
    pub billboard: bool,

    pub simulation_space: SimulationSpace,
    /// Fraction of the emitter's world velocity added to each
    /// particle's initial velocity at spawn. `0.0` = ignore
    /// (sparks fly purely along their own emit direction), `1.0` =
    /// match emitter (running-dust effect), `>1.0` = exhaust
    /// (jets ahead). Sanitised to `[0, 2]`.
    pub inherit_velocity: Fp,

    /// Toggle particle collisions against the room's terrain
    /// heightfield. `false` = visual-only (cheaper).
    pub collide_terrain: bool,
    /// Toggle collisions against finite water surfaces.
    pub collide_water: bool,
    /// Toggle collisions against arbitrary avian3d colliders
    /// (placed primitives, walls, …).
    pub collide_colliders: bool,
    /// Restitution applied on collision: `0.0` = stick, `1.0` =
    /// perfect bounce.
    pub bounce: Fp,
    /// Friction applied to the tangential velocity on collision:
    /// `0.0` = frictionless slide, `1.0` = stick.
    pub friction: Fp,

    /// Deterministic emission seed. Same seed + same dt path on
    /// every peer produces the same particle stream.
    #[serde(with = "u64_as_string")]
    pub seed: u64,

    /// Optional per-particle texture. Resolves through the same
    /// [`SignSource`] union Sign uses, so a "leaf falling" emitter
    /// and a Sign signpost pointing at the same atlas image share
    /// one HTTPS round trip via [`super::super::world_builder::image_cache::BlobImageCache`].
    /// `None` keeps v1 behaviour: solid coloured quads tinted by
    /// `start_color` / `end_color`.
    pub texture: Option<SignSource>,
    /// Treat the loaded texture as a sprite-sheet atlas of
    /// `rows × cols` cells. `None` uses the whole image as a single
    /// frame (the default).
    pub texture_atlas: Option<TextureAtlas>,
    /// How a particle picks its current atlas frame. `Still` keeps
    /// frame 0 forever; `RandomFrame` picks once at spawn (per-RNG-
    /// stream draw) so different particles show different sprites
    /// from the same atlas; `OverLifetime { fps }` cycles through
    /// the frame array at the configured rate.
    pub frame_mode: AnimationFrameMode,
    /// Sampler filter applied to the loaded image. `Linear` is the
    /// natural smooth filtering for soft sprites; `Nearest` for
    /// pixel-art / retro looks. The cache keys on filter so a
    /// Linear and a Nearest request for the same source produce
    /// two distinct GPU images, neither stomping the other.
    pub texture_filter: TextureFilter,
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
    ///
    /// Wire-format note (#695): an ABSENT key legally means "pre-sprite
    /// legacy record → plain `None` quads" (this field-level default),
    /// which differs from the struct default (`SoftDisc`, #367). The field
    /// is therefore marked `(always)` in the eliding-serialize invocation
    /// below — it is written unconditionally so elision can never rewrite
    /// the legacy meaning.
    #[serde(default)]
    pub procedural_texture: super::texture::SovereignTextureConfig,
}

crate::pds::serde_util::impl_default_eliding_serialize!(ParticleParams {
    emitter_shape,
    rate_per_second,
    burst_count,
    max_particles,
    looping,
    duration,
    lifetime_min,
    lifetime_max,
    speed_min,
    speed_max,
    gravity_multiplier,
    acceleration,
    linear_drag,
    start_size,
    end_size,
    start_color,
    end_color,
    blend_mode,
    billboard,
    simulation_space,
    inherit_velocity,
    collide_terrain,
    collide_water,
    collide_colliders,
    bounce,
    friction,
    seed via u64_as_string(u64),
    texture,
    texture_atlas,
    frame_mode,
    texture_filter,
    procedural_texture(always),
});

impl Default for ParticleParams {
    /// Canonical default emitter — a small upward-spraying cone with
    /// 32 particles/s, 2 s lifetime, white→fade-out alpha-blended
    /// particles on a soft-disc sprite (#367, so a freshly-added emitter
    /// reads as soft motes rather than hard squares), no inheritance, no
    /// collisions. See [`GeneratorKind::default_particles`].
    fn default() -> Self {
        Self {
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
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.sphere")]
    Sphere {
        radius: Fp,
        resolution: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.cylinder")]
    Cylinder {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.capsule")]
    Capsule {
        radius: Fp,
        length: Fp,
        latitudes: u32,
        longitudes: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.cone")]
    Cone {
        radius: Fp,
        height: Fp,
        resolution: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.torus")]
    Torus {
        minor_radius: Fp,
        major_radius: Fp,
        minor_resolution: u32,
        major_resolution: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.plane")]
    Plane {
        size: Fp2,
        subdivisions: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    #[serde(rename = "network.symbios.gen.tetrahedron")]
    Tetrahedron {
        size: Fp,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
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
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
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
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    /// Right-triangular prism — a ramp / roof pitch / buttress / eave. `size` is
    /// the bounding box; the slope rises from the front-bottom (`+Z`, `-Y`) to
    /// the back-top (`-Z`, `+Y`) across the full width (X).
    #[serde(rename = "network.symbios.gen.wedge")]
    Wedge {
        size: Fp3,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
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
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    /// Barr superellipsoid — one prim that morphs continuously from box
    /// (small exponents) through pillow / sphere (`1.0`) toward a pinched
    /// octahedral form (large exponents). `exponent_ns` shapes the
    /// north–south (latitude) profile, `exponent_ew` the east–west
    /// cross-section; `half_extents` scale the three axes. The organic
    /// workhorse for skulls, torsos, pebbles, cushions — the rounded masses
    /// that previously took a scaled sphere or a bevel-box compromise.
    #[serde(rename = "network.symbios.gen.superellipsoid")]
    Superellipsoid {
        half_extents: Fp3,
        exponent_ns: Fp,
        exponent_ew: Fp,
        latitudes: u32,
        longitudes: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    /// Circular-profile tube swept along a user-editable Catmull-Rom spine
    /// with a per-point radius — the one-prim replacement for the tapered-
    /// capsule chains that limbs / tails / horns / tentacles / vines used to
    /// take. The spline passes through every control point (2..16); radius
    /// interpolates along the same spline, and both ends are capped with
    /// flat discs. Vertex torture composes on top, and the topology cuts
    /// map tube-wise (#691): `path_cut` keeps an angular range of the ring
    /// (an open gutter / half-pipe along the curve), `profile_cut` trims
    /// the kept stretch of the path, and `hollow` makes the tube a shell.
    #[serde(rename = "network.symbios.gen.spine")]
    Spine {
        points: Vec<SpinePoint>,
        /// Ring segments around the tube's circular cross-section.
        resolution: u32,
        /// Path samples per spline segment (between consecutive points).
        samples_per_segment: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    /// Profile revolved around local Y — the SL-"rokuro" vase / bell / hoof /
    /// chess-piece prim. `points` is the silhouette from bottom to top
    /// (2..16 stations of radius-at-height); `smooth` interpolates it with a
    /// Catmull-Rom spline (organic curves from few points) or keeps straight
    /// polyline segments (sharp ridges). `path_cut` (angular wedge) and
    /// `hollow` (proportional inner shell) compose exactly like the other
    /// swept prims; `profile_cut` keeps an arc-length band of the silhouette
    /// (slice a vase's top off without re-authoring its stations), with the
    /// trimmed ends capped.
    #[serde(rename = "network.symbios.gen.lathe")]
    Lathe {
        points: Vec<LathePoint>,
        /// Revolve segments around the Y axis.
        resolution: u32,
        /// Spline (`true`) vs straight-segment (`false`) profile.
        smooth: bool,
        solid: bool,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
        torture: TortureParams,
    },

    /// Smooth-blend SDF group — an ordered list of add/subtract elements
    /// (spheres / capsules / ellipsoids / boxes / cylinders / tori / cones)
    /// evaluated as one signed distance field with per-element polynomial
    /// smooth-min, then meshed once on spawn with surface nets. The Spore /
    /// Dreams organic primitive: a pile of overlapping ellipsoids becomes
    /// one seamless muscle mass, a subtracted sphere carves an eye socket,
    /// and the result is watertight by construction (a broken mesh is
    /// unrepresentable). `resolution` is the sample-grid cell count along
    /// the group's longest axis — the quality/cost dial, clamped hard in
    /// sanitize because grid cost is cubic. Topology cuts apply as hard CSG
    /// on the final field: `profile_cut` keeps a Y-band of the group's
    /// bounds (flat slices — a blob that sits flush on the ground),
    /// `path_cut` keeps a pie wedge around the prim-local Y axis, and
    /// `hollow` erodes an inner shell whose wall is `(1 - hollow)` of the
    /// group's thinnest half-extent (visible wherever a carve or cut opens
    /// the surface). `uv_mapping` picks the texture projection baked into
    /// the meshed surface — see [`UvMapping`] for the trade-offs per mode.
    #[serde(rename = "network.symbios.gen.blob_group")]
    BlobGroup {
        elements: Vec<BlobElement>,
        resolution: u32,
        solid: bool,
        #[serde(default, skip_serializing_if = "UvMapping::is_default")]
        uv_mapping: UvMapping,
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
        material: SovereignMaterialSettings,
        #[serde(default, skip_serializing_if = "TortureParams::is_default")]
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
    ParticleSystem(Box<ParticleParams>),

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
        #[serde(default, skip_serializing_if = "SovereignMaterialSettings::is_default")]
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
        /// Sampler filter for the fetched image. `Linear` (the default,
        /// and the behaviour of every pre-#663 record) smooths photos;
        /// `Nearest` keeps pixel-art signage crisp. Serde-defaulted so
        /// existing records deserialize unchanged.
        #[serde(default)]
        texture_filter: TextureFilter,
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
            | GeneratorKind::Helix { torture, .. }
            | GeneratorKind::Superellipsoid { torture, .. }
            | GeneratorKind::Spine { torture, .. }
            | GeneratorKind::Lathe { torture, .. }
            | GeneratorKind::BlobGroup { torture, .. } => Some(torture),
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
            | GeneratorKind::Helix { torture, .. }
            | GeneratorKind::Superellipsoid { torture, .. }
            | GeneratorKind::Spine { torture, .. }
            | GeneratorKind::Lathe { torture, .. }
            | GeneratorKind::BlobGroup { torture, .. } => Some(torture),
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
                | GeneratorKind::Superellipsoid { .. }
                | GeneratorKind::Spine { .. }
                | GeneratorKind::Lathe { .. }
                | GeneratorKind::BlobGroup { .. }
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
            GeneratorKind::Superellipsoid { .. } => "Superellipsoid",
            GeneratorKind::Spine { .. } => "Spine",
            GeneratorKind::Lathe { .. } => "Lathe",
            GeneratorKind::BlobGroup { .. } => "BlobGroup",
            GeneratorKind::Sign { .. } => "Sign",
            GeneratorKind::ParticleSystem(..) => "ParticleSystem",
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
            // Exponents at 0.5 default to the pillow / rounded-box middle of
            // the family — visually distinct from both Cuboid and Sphere, so
            // a freshly-added prim reads as its own thing.
            "Superellipsoid" => GeneratorKind::Superellipsoid {
                half_extents: Fp3([0.5, 0.5, 0.5]),
                exponent_ns: Fp(0.5),
                exponent_ew: Fp(0.5),
                latitudes: 16,
                longitudes: 24,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            // A gentle forward-arcing taper so a freshly-added spine reads
            // as a limb / tail rather than a straight pipe.
            "Spine" => GeneratorKind::Spine {
                points: vec![
                    SpinePoint {
                        position: Fp3([0.0, -0.5, 0.0]),
                        radius: Fp(0.2),
                    },
                    SpinePoint {
                        position: Fp3([0.12, 0.0, 0.08]),
                        radius: Fp(0.15),
                    },
                    SpinePoint {
                        position: Fp3([0.0, 0.5, 0.0]),
                        radius: Fp(0.09),
                    },
                ],
                resolution: 12,
                samples_per_segment: 8,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            // A bellied vase silhouette — the canonical lathe demo shape.
            "Lathe" => GeneratorKind::Lathe {
                points: vec![
                    LathePoint {
                        radius: Fp(0.18),
                        height: Fp(-0.5),
                    },
                    LathePoint {
                        radius: Fp(0.32),
                        height: Fp(-0.25),
                    },
                    LathePoint {
                        radius: Fp(0.2),
                        height: Fp(0.1),
                    },
                    LathePoint {
                        radius: Fp(0.1),
                        height: Fp(0.3),
                    },
                    LathePoint {
                        radius: Fp(0.16),
                        height: Fp(0.5),
                    },
                ],
                resolution: 24,
                smooth: true,
                solid: true,
                material: mat,
                torture: TortureParams::default(),
            },
            // Two generously-blended spheres — the smallest recipe that
            // shows what the prim is for (they merge into one peanut mass).
            "BlobGroup" => GeneratorKind::BlobGroup {
                elements: vec![
                    BlobElement {
                        position: Fp3([0.0, -0.15, 0.0]),
                        radii: Fp3([0.3, 0.3, 0.3]),
                        ..Default::default()
                    },
                    BlobElement {
                        position: Fp3([0.0, 0.22, 0.0]),
                        radii: Fp3([0.2, 0.2, 0.2]),
                        blend: Fp(0.15),
                        ..Default::default()
                    },
                ],
                resolution: 32,
                solid: true,
                uv_mapping: UvMapping::default(),
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
            texture_filter: TextureFilter::Linear,
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
        GeneratorKind::ParticleSystem(Box::default())
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
    // The three non-kind fields elide their common case on the wire (#695):
    // an identity transform, no children, silent audio. Each already
    // decodes missing → default, so elision is round-trip-exact.
    #[serde(default, skip_serializing_if = "TransformData::is_identity")]
    pub transform: TransformData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Generator>,
    /// Optional emissive audio source attached to this node — spatially
    /// played at the node's world position by Bevy's spatial audio
    /// pipeline. Forward-compat across older records: missing field
    /// decodes via `#[serde(default)]` to
    /// [`SovereignAudioConfig::None`](super::audio::SovereignAudioConfig::None)
    /// (silent). Set non-None by
    /// catalogue entries that want a construct to hum / chime / drone
    /// at its location (e.g. the teleporter's portal core).
    #[serde(
        default,
        skip_serializing_if = "super::audio::SovereignAudioConfig::is_none"
    )]
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
        #[serde(default, skip_serializing_if = "TransformData::is_identity")]
        transform: TransformData,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// When terrain-snapped, refuse submerged ground: the compiler
        /// walks the anchor along its bearing through the origin
        /// (preserving a spawn-facing yaw) until the terrain rises
        /// above the room's water line. Used by the seeded landmark so
        /// a coastal villa doesn't spawn waist-deep in the sea.
        /// `#[serde(default)]` keeps older records decoding unchanged.
        #[serde(default, skip_serializing_if = "is_false")]
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
        /// `BiomeFilter` accepts every sample (and is elided on the wire —
        /// `is_noop` is exactly the default state).
        #[serde(default, skip_serializing_if = "BiomeFilter::is_noop")]
        biome_filter: BiomeFilter,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// Apply a deterministic random yaw (per `local_seed`) to every
        /// scattered instance. Defaults to `true` for backward compatibility
        /// with records written before this field existed.
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        random_yaw: bool,
        /// Reject scatter points that fall inside the room's road-network
        /// district — a circle of radius `RoadConfig::district_half_extent`
        /// around spawn. Keeps the seeded natural scatters (trees, boulders)
        /// clear of the built-up urban area (roads *and* lot buildings)
        /// without needing an annulus bounds shape. Resolved at compile
        /// against [`crate::pds::room::find_road_config`]; a no-op in a room
        /// with no enabled road network. Defaults `false`.
        #[serde(default, skip_serializing_if = "is_false")]
        avoid_urban: bool,
    },

    #[serde(rename = "network.symbios.place.grid")]
    Grid {
        generator_ref: String,
        #[serde(default, skip_serializing_if = "TransformData::is_identity")]
        transform: TransformData,
        counts: [u32; 3],
        gaps: Fp3,
        #[serde(default = "default_true", skip_serializing_if = "is_true")]
        snap_to_terrain: bool,
        /// Apply a per-cell deterministic random yaw. Defaults to `false`
        /// — grids are typically axis-aligned.
        #[serde(default, skip_serializing_if = "is_false")]
        random_yaw: bool,
    },

    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod prim_wire_tests {
    //! Wire-format guards for the organic-prim additions (#688): the new
    //! `TortureParams` knobs must default cleanly on records that predate
    //! them, and the Superellipsoid variant must round-trip with its tag.
    use super::*;

    #[test]
    fn torture_params_predating_new_knobs_default_to_identity() {
        // A pre-#688 torture block carries no `taper_bottom` / `bulge` keys —
        // and since #695 the eliding wire format omits every identity knob
        // anyway, so serializing a twist+taper-only value produces exactly
        // the shape an already-published record carries.
        let current = TortureParams {
            twist: Fp(0.5),
            taper: Fp2([0.2, 0.2]),
            ..Default::default()
        };
        let old = serde_json::to_value(current).expect("serialises");
        let obj = old.as_object().expect("one flat JSON object");
        assert!(obj.contains_key("twist"), "authored knobs stay on the wire");
        assert!(obj.contains_key("taper"), "authored knobs stay on the wire");
        assert!(
            !obj.contains_key("taper_bottom") && !obj.contains_key("bulge"),
            "identity knobs are elided (#695)"
        );

        let t: TortureParams = serde_json::from_value(old).expect("old torture block parses");
        assert_eq!(t.taper_bottom, Fp2([0.0, 0.0]));
        assert_eq!(t.bulge, Fp2([0.0, 0.0]));
        assert_eq!(t.taper, Fp2([0.2, 0.2]), "existing knobs still decode");
        assert!(!t.deforms_are_identity() && t.cuts_are_identity());

        // Round trip lands on the same value.
        let re: TortureParams = serde_json::from_value(serde_json::to_value(t).unwrap()).unwrap();
        assert_eq!(re, t);
    }

    #[test]
    fn spine_and_lathe_wire_format_round_trips() {
        for (tag, wire) in [
            ("Spine", "network.symbios.gen.spine"),
            ("Lathe", "network.symbios.gen.lathe"),
        ] {
            let kind = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            let v = serde_json::to_value(&kind).expect("serialises");
            let obj = v.as_object().expect("one flat JSON object");
            assert_eq!(obj.get("$type").and_then(|t| t.as_str()), Some(wire));
            assert!(
                obj.get("points").is_some_and(|p| p.is_array()),
                "{tag} points stay an inline array"
            );
            let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
            assert_eq!(re, kind);
        }
    }

    #[test]
    fn blob_group_wire_format_round_trips() {
        let kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.blob_group")
        );
        let elements = obj.get("elements").and_then(|e| e.as_array()).unwrap();
        assert_eq!(
            elements[0]
                .get("shape")
                .and_then(|s| s.get("$type"))
                .and_then(|t| t.as_str()),
            Some("network.symbios.blob.sphere"),
            "element shape is its own open union"
        );
        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);

        // Every known shape round-trips through its own tag (#725 grew the
        // union past the original sphere/capsule/ellipsoid trio).
        for (shape, wire) in [
            (BlobShape::Sphere, "network.symbios.blob.sphere"),
            (BlobShape::Capsule, "network.symbios.blob.capsule"),
            (BlobShape::Ellipsoid, "network.symbios.blob.ellipsoid"),
            (BlobShape::Box, "network.symbios.blob.box"),
            (BlobShape::Cylinder, "network.symbios.blob.cylinder"),
            (BlobShape::Torus, "network.symbios.blob.torus"),
            (BlobShape::Cone, "network.symbios.blob.cone"),
        ] {
            let sv = serde_json::to_value(shape).expect("shape serialises");
            assert_eq!(sv.get("$type").and_then(|t| t.as_str()), Some(wire));
            let rs: BlobShape = serde_json::from_value(sv).expect("shape reparses");
            assert_eq!(rs, shape);
        }

        // Forward compat: an unknown element shape degrades to Unknown, not
        // a parse failure.
        let mut v2 = serde_json::to_value(&kind).unwrap();
        v2["elements"][0]["shape"]["$type"] = serde_json::json!("network.symbios.blob.hyperboloid");
        let re2: GeneratorKind = serde_json::from_value(v2).expect("future shape still parses");
        let GeneratorKind::BlobGroup { elements, .. } = &re2 else {
            panic!("wrong variant");
        };
        assert_eq!(elements[0].shape, BlobShape::Unknown);
    }

    /// The `uv_mapping` knob (#739) is default-elided on the wire, every
    /// known mode round-trips through its own tag, and an unrecognised
    /// mode tag degrades to `Unknown` instead of failing the record.
    #[test]
    fn blob_group_uv_mapping_wire_format() {
        // Default mode stays off the wire — pre-#739 records re-serialise
        // byte-identically.
        let kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        let v = serde_json::to_value(&kind).unwrap();
        assert!(
            v.get("uv_mapping").is_none(),
            "default uv_mapping must be elided"
        );

        for (mode, wire) in [
            (UvMapping::Spherical, "network.symbios.uv.spherical"),
            (UvMapping::Box, "network.symbios.uv.box"),
            (UvMapping::Cylindrical, "network.symbios.uv.cylindrical"),
            (UvMapping::PlanarX, "network.symbios.uv.planar_x"),
            (UvMapping::PlanarY, "network.symbios.uv.planar_y"),
            (UvMapping::PlanarZ, "network.symbios.uv.planar_z"),
        ] {
            let sv = serde_json::to_value(mode).expect("mode serialises");
            assert_eq!(sv.get("$type").and_then(|t| t.as_str()), Some(wire));
            let rm: UvMapping = serde_json::from_value(sv).expect("mode reparses");
            assert_eq!(rm, mode);
        }

        // A non-default mode survives a full generator round trip.
        let GeneratorKind::BlobGroup {
            elements,
            resolution,
            solid,
            material,
            torture,
            ..
        } = kind
        else {
            panic!("wrong variant");
        };
        let kind = GeneratorKind::BlobGroup {
            elements,
            resolution,
            solid,
            uv_mapping: UvMapping::Box,
            material,
            torture,
        };
        let v = serde_json::to_value(&kind).unwrap();
        assert_eq!(
            v["uv_mapping"]["$type"].as_str(),
            Some("network.symbios.uv.box")
        );
        let re: GeneratorKind = serde_json::from_value(v.clone()).expect("reparses");
        assert_eq!(re, kind);

        // Forward compat: a future mode tag degrades to Unknown.
        let mut v3 = v;
        v3["uv_mapping"]["$type"] = serde_json::json!("network.symbios.uv.conformal");
        let re3: GeneratorKind = serde_json::from_value(v3).expect("future mode still parses");
        let GeneratorKind::BlobGroup { uv_mapping, .. } = re3 else {
            panic!("wrong variant");
        };
        assert_eq!(uv_mapping, UvMapping::Unknown);
    }

    #[test]
    fn superellipsoid_wire_format_round_trips() {
        let kind = GeneratorKind::default_primitive_for_tag("Superellipsoid").unwrap();
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.superellipsoid")
        );
        assert!(obj.contains_key("half_extents"), "fields stay inline");
        assert!(obj.contains_key("exponent_ns"));

        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);
    }
}

#[cfg(test)]
mod particle_params_tests {
    //! Wire-format guards for the #648 `ParticleSystem` boxed-params
    //! refactor: the internally-tagged enum must keep serialising the
    //! params inline beside `$type`, exactly like the old struct variant,
    //! so already-published records round-trip unchanged.
    use super::*;

    #[test]
    fn particle_params_wire_format_is_inline() {
        // Author a few non-default fields so they must appear on the wire —
        // since #695 default-valued params are elided, so the all-defaults
        // emitter serializes as just its `$type` tag.
        let kind = GeneratorKind::ParticleSystem(Box::new(ParticleParams {
            rate_per_second: Fp(64.0),
            burst_count: 3,
            seed: 7,
            ..Default::default()
        }));
        let v = serde_json::to_value(&kind).expect("serialises");
        let obj = v.as_object().expect("one flat JSON object");
        // Tag + fields side by side — no nested params wrapper key.
        assert_eq!(
            obj.get("$type").and_then(|t| t.as_str()),
            Some("network.symbios.gen.particles")
        );
        assert!(obj.contains_key("rate_per_second"), "fields stay inline");
        assert!(obj.contains_key("burst_count"));
        assert!(
            obj.get("seed").is_some_and(|s| s.is_string()),
            "seed keeps its string encoding"
        );
        assert!(
            !obj.contains_key("emitter_shape"),
            "default-valued params are elided (#695)"
        );
        assert!(
            !obj.values().any(|x| {
                x.as_object()
                    .is_some_and(|inner| inner.contains_key("rate_per_second"))
            }),
            "no boxed-struct wrapper object appeared"
        );
        // And the elided form round-trips to the authored value.
        let re: GeneratorKind = serde_json::from_value(v).expect("reparses");
        assert_eq!(re, kind);
    }

    #[test]
    fn particle_params_old_format_record_round_trips() {
        // A pre-#648 record fragment: struct-variant inline fields, no
        // optional texture keys (their serde defaults must fill in).
        let old = serde_json::json!({
            "$type": "network.symbios.gen.particles",
            "emitter_shape": serde_json::to_value(EmitterShape::Point).unwrap(),
            "rate_per_second": serde_json::to_value(Fp(8.0)).unwrap(),
            "burst_count": 3,
            "max_particles": 64,
            "looping": true,
            "duration": serde_json::to_value(Fp(2.0)).unwrap(),
            "lifetime_min": serde_json::to_value(Fp(0.5)).unwrap(),
            "lifetime_max": serde_json::to_value(Fp(1.5)).unwrap(),
            "speed_min": serde_json::to_value(Fp(1.0)).unwrap(),
            "speed_max": serde_json::to_value(Fp(2.0)).unwrap(),
            "gravity_multiplier": serde_json::to_value(Fp(0.0)).unwrap(),
            "acceleration": serde_json::to_value(Fp3([0.0, 0.0, 0.0])).unwrap(),
            "linear_drag": serde_json::to_value(Fp(0.1)).unwrap(),
            "start_size": serde_json::to_value(Fp(0.2)).unwrap(),
            "end_size": serde_json::to_value(Fp(0.0)).unwrap(),
            "start_color": serde_json::to_value(Fp4([1.0, 1.0, 1.0, 1.0])).unwrap(),
            "end_color": serde_json::to_value(Fp4([1.0, 1.0, 1.0, 0.0])).unwrap(),
            "blend_mode": serde_json::to_value(ParticleBlendMode::Alpha).unwrap(),
            "billboard": true,
            "simulation_space": serde_json::to_value(SimulationSpace::World).unwrap(),
            "inherit_velocity": serde_json::to_value(Fp(0.0)).unwrap(),
            "collide_terrain": false,
            "collide_water": false,
            "collide_colliders": false,
            "bounce": serde_json::to_value(Fp(0.3)).unwrap(),
            "friction": serde_json::to_value(Fp(0.5)).unwrap(),
            "seed": "42",
        });
        let kind: GeneratorKind = serde_json::from_value(old).expect("old record parses");
        let GeneratorKind::ParticleSystem(p) = &kind else {
            panic!("wrong variant");
        };
        assert_eq!(p.seed, 42);
        assert_eq!(p.burst_count, 3);
        assert!(p.texture.is_none(), "missing optional fields default");

        // Round trip: serialise + reparse lands on the same value.
        let re: GeneratorKind =
            serde_json::from_value(serde_json::to_value(&kind).unwrap()).unwrap();
        assert_eq!(re, kind);
    }
}
