//! Record sanitisation: clamp every numeric field a malicious peer might
//! inflate to crash the engine or exhaust host RAM. The limits mirror the
//! ranges the World Editor UI already exposes, so a hand-crafted record
//! cannot trigger behaviour the owner couldn't have requested via the
//! normal interface.
//!
//! Each path that accepts a `RoomRecord`/`AvatarRecord`/`InventoryRecord`
//! from the network calls its `sanitize()` method before handing the record
//! to the world compiler; those impls live alongside the record types and
//! delegate into the per-variant helpers defined here.

use super::generator::{
    AlphaModeKind, AnimationFrameMode, EmitterShape, Generator, GeneratorKind, SignSource,
    TextureAtlas, WaterSurface,
};
use super::terrain::SovereignTerrainConfig;
use super::texture::{SovereignMaterialSettings, SovereignTextureConfig};
use super::types::{Fp, Fp2, Fp3, Fp4, TransformData, truncate_on_char_boundary};

/// Maximum values allowed in a `RoomRecord`. Record fields outside these
/// bounds are clamped rather than rejected so slightly exotic records from
/// forward-compatible clients still round-trip, but a weaponised payload
/// cannot force a runaway allocation.
pub mod limits {
    /// Heightmap edge length (cells per side). 2048² ≈ 4M f32 cells ≈ 16 MiB.
    pub const MAX_GRID_SIZE: u32 = 2048;
    /// FBM / noise octaves.
    pub const MAX_OCTAVES: u32 = 32;
    /// Voronoi seed-point count.
    pub const MAX_VORONOI_SEEDS: u32 = 10_000;
    /// Voronoi terrace-level count.
    pub const MAX_VORONOI_TERRACES: u32 = 64;
    /// Hydraulic erosion drop count.
    pub const MAX_EROSION_DROPS: u32 = 500_000;
    /// Thermal erosion iteration count.
    pub const MAX_THERMAL_ITERATIONS: u32 = 500;
    /// Splat texture resolution per side (pixels).
    pub const MAX_TEXTURE_SIZE: u32 = 4096;
    /// Ground / rock generator octaves.
    pub const MAX_GROUND_OCTAVES: u32 = 12;
    pub const MAX_ROCK_OCTAVES: u32 = 16;
    /// Scatter placement count.
    pub const MAX_SCATTER_COUNT: u32 = 100_000;
    /// L-system derivation iterations. 12 is already enough to blow out most
    /// lexical grammars — anything beyond this is almost certainly an attack.
    pub const MAX_LSYSTEM_ITERATIONS: u32 = 12;
    /// L-system source / finalization code length in bytes.
    pub const MAX_LSYSTEM_CODE_BYTES: usize = 16_384;
    /// L-system mesh resolution (stroke segments per twig).
    pub const MAX_LSYSTEM_MESH_RESOLUTION: u32 = 32;
    /// CGA shape grammar source length in bytes. The upstream parser caps a
    /// single rule body at 1024 ops + 64 variants; the same DoS pressure
    /// applies at the source level — a megabyte of `Name --> Name | Name |
    /// …` lines would still spend its budget inside `parse_rule` before any
    /// derivation-time guard fires. 16 KiB matches the L-system code cap.
    pub const MAX_SHAPE_SOURCE_BYTES: usize = 16_384;
    /// CGA shape grammar root-rule identifier length. The upstream parser
    /// rejects identifiers above 64 bytes; we clamp earlier so a hostile
    /// record cannot smuggle a megabyte of Unicode through `kind_tag` /
    /// editor labels before the parser ever sees it.
    pub const MAX_SHAPE_ROOT_RULE_BYTES: usize = 64;
    /// Maximum number of named material slots on a `Shape` generator. Each
    /// slot may pin a baked foliage texture in `Assets<Image>`, so a record
    /// with thousands of unused slots inflates GPU memory even before any
    /// terminal references them.
    pub const MAX_SHAPE_MATERIAL_SLOTS: usize = 64;
    /// Per-axis footprint clamp (metres). 1 km is well past any plausible
    /// authored building / district footprint and keeps the initial scope
    /// finite so `Interpreter::derive` cannot be smuggled an `f64` infinity.
    pub const MAX_SHAPE_FOOTPRINT: f32 = 1_000.0;
    /// Maximum number of `Placement` entries per `RoomRecord`. Clamping
    /// `Scatter.count` alone is not enough — a record with ten-thousand
    /// single-count scatter entries still weaponises `compile_room_record`.
    pub const MAX_PLACEMENTS: usize = 1_024;
    /// Maximum number of named generators per `RoomRecord`. Every generator
    /// also materialises per-peer state (L-system material cache, lookup
    /// work in hot loops) so a record with a million generator entries
    /// would still inflate memory and slow every `compile_room_record` pass
    /// even if no placement referenced them.
    pub const MAX_GENERATORS: usize = 256;
    /// Horizontal cell spacing for the heightmap mesh. The lower bound keeps
    /// the mesh finite (cell_scale feeds straight into the collider builder
    /// and a NaN/zero would panic `avian3d`), and the upper bound caps the
    /// total world extent at a sane radius even with MAX_GRID_SIZE.
    pub const MIN_CELL_SCALE: f32 = 0.01;
    pub const MAX_CELL_SCALE: f32 = 64.0;
    /// Vertical scale applied to normalised heightmap samples. Same rationale:
    /// clamp to a finite positive range so a corrupted record can't smuggle
    /// NaN/infinity into `HeightMapMeshBuilder`.
    pub const MIN_HEIGHT_SCALE: f32 = 0.01;
    pub const MAX_HEIGHT_SCALE: f32 = 10_000.0;
    /// Maximum recursion depth for any generator's child tree. Deep
    /// hierarchies cost an entity + Transform chain per node; 16 is well
    /// past any plausible hand-authored assembly.
    pub const MAX_GENERATOR_DEPTH: u32 = 16;
    /// Maximum total node count (root + descendants) for a single named
    /// generator's tree. A malicious record with a million children would
    /// otherwise spawn a million Bevy entities + colliders on every compile.
    pub const MAX_GENERATOR_NODES: u32 = 1024;
    /// Maximum absolute `twist` angle (radians) applied across a primitive's
    /// Y extent. Two full turns in either direction is well past any
    /// sculpting need — anything beyond that is just geometry noise.
    pub const MAX_TORTURE_TWIST: f32 = 4.0 * std::f32::consts::PI;
    /// Maximum magnitude of the per-axis `taper` factor. Clamped below 1.0
    /// so a tapered primitive never collapses its top (or bottom) to a
    /// single point — we'd lose vertices and the collider builder would
    /// start returning zero-volume hulls.
    pub const MAX_TORTURE_TAPER: f32 = 0.99;
    /// Maximum magnitude of any component of the `bend` vector (world-units
    /// of vertex displacement at the shape's top). 10 m is already a
    /// dramatic curl on a 1 m primitive; beyond that the vertex torture pass
    /// produces visually degenerate meshes the collider can't hug.
    pub const MAX_TORTURE_BEND: f32 = 10.0;
    /// Maximum absolute `level_offset` (metres) on a Water node. The compiler
    /// adds this to a base sea level and writes it into the volume's transform
    /// Y; an unbounded value would smuggle infinity into the entity transform
    /// and the water shader's per-fragment uniforms.
    pub const MAX_WATER_LEVEL_OFFSET: f32 = 10_000.0;
    /// Maximum Gerstner amplitude / time multiplier on a Water surface.
    /// Both feed shader uniforms and a runaway value produces NaN normals.
    pub const MAX_WAVE_SCALE: f32 = 100.0;
    pub const MAX_WAVE_SPEED: f32 = 100.0;
    /// Maximum `flow_strength` (force-per-metre-submerged) on a Water
    /// surface. Bounded so a hostile record can't apply a near-infinite
    /// tangent force to every floating object — earth gravity is ~9.81, so
    /// 10× free-fall is the upper bound for any reasonable river / waterfall
    /// effect.
    pub const MAX_WATER_FLOW_STRENGTH: f32 = 100.0;
    /// Maximum URL length (bytes) for a [`super::super::generator::SignSource::Url`]
    /// payload. 2048 matches the de-facto browser cap and keeps a hostile
    /// record from smuggling megabytes of inert string through the room
    /// recipe.
    pub const MAX_SIGN_URL_BYTES: usize = 2048;
    /// Maximum DID / CID length (bytes) for a Sign source. ATProto DIDs are
    /// well under 256 bytes and CIDs (base32 v1) are ~60 bytes; 256 matches
    /// the existing Portal DID cap and gives forward-compat headroom.
    pub const MAX_SIGN_DID_BYTES: usize = 256;
    pub const MAX_SIGN_CID_BYTES: usize = 256;
    /// Per-axis panel size (metres) for a Sign generator. Mirrors the
    /// primitive `c_dim` envelope so a megastructure billboard stays within
    /// the 100 m world-cell budget.
    pub const MAX_SIGN_SIZE: f32 = 100.0;
    /// Per-axis UV repeat factor for a Sign generator. Bounded to keep the
    /// fragment shader from sampling at sub-pixel rates that pin the GPU
    /// on a hostile record. The lower bound is non-zero so the fragment's
    /// `uv * repeat` term doesn't collapse the texture to a single texel.
    pub const MIN_SIGN_UV_REPEAT: f32 = 0.001;
    pub const MAX_SIGN_UV_REPEAT: f32 = 1_000.0;
    /// Per-axis UV offset for a Sign generator. Wraps in the sampler
    /// regardless, so any reasonable bound is fine; 1_000 matches the
    /// repeat envelope so the editor's drag widgets feel symmetric.
    pub const MAX_SIGN_UV_OFFSET: f32 = 1_000.0;
    /// Hard cap on simultaneously-alive particles per emitter. Each
    /// particle is a Bevy entity in v1; 512 keeps a handful of emitters
    /// per room well within the engine's per-frame entity-iteration
    /// budget without precluding "fire" / "dust storm" densities.
    pub const MAX_PARTICLES: u32 = 512;
    /// Continuous emit rate in particles per second. With
    /// `MAX_PARTICLES` already capping the steady-state population,
    /// 256 / s lets a short-lived burst (~0.5 s) replenish the cap
    /// without overshooting it dramatically.
    pub const MAX_PARTICLE_RATE: f32 = 256.0;
    /// Per-cycle burst-count cap. Mirrors the per-emitter cap so a
    /// burst can fill the steady-state population in one shot but not
    /// queue up an arbitrary one-frame spike.
    pub const MAX_PARTICLE_BURST: u32 = 512;
    /// Per-particle lifetime envelope (seconds). 30 s keeps a slow
    /// trailing trail visible across a placement traversal without
    /// allowing a permanent fog effect that would never decay.
    pub const MIN_PARTICLE_LIFETIME: f32 = 0.01;
    pub const MAX_PARTICLE_LIFETIME: f32 = 30.0;
    /// Per-particle initial-speed envelope (metres per second).
    pub const MAX_PARTICLE_SPEED: f32 = 1_000.0;
    /// Magnitude cap on per-axis constant acceleration (m/s²). 100 is
    /// already 10× free-fall so any reasonable wind / float effect fits
    /// comfortably inside.
    pub const MAX_PARTICLE_ACCEL: f32 = 100.0;
    /// Cap on the gravity multiplier. Allowed to be negative so a
    /// "smoke rises" effect doesn't need a custom force vector.
    pub const MAX_PARTICLE_GRAVITY_MULT: f32 = 10.0;
    /// Linear drag coefficient cap (per-second exponential damping).
    pub const MAX_PARTICLE_DRAG: f32 = 100.0;
    /// Per-particle quad-size envelope (metres). Lower bound is `0.0`
    /// so a particle can fade out completely by end-of-life — a zero-
    /// area quad simply draws nothing, matching the natural
    /// "shrink to vanish" effect.
    pub const MIN_PARTICLE_SIZE: f32 = 0.0;
    pub const MAX_PARTICLE_SIZE: f32 = 100.0;
    /// Inherit-velocity factor cap. `1` matches the emitter, `2` lets
    /// exhaust-style effects jet ahead. Above 2 the trail decouples
    /// visually and looks bug-y rather than stylish.
    pub const MAX_PARTICLE_INHERIT_VELOCITY: f32 = 2.0;
    /// Active-emit duration cap (seconds). Looping emitters use this as
    /// the burst-cadence period.
    pub const MIN_PARTICLE_DURATION: f32 = 0.01;
    pub const MAX_PARTICLE_DURATION: f32 = 600.0;
    /// Emitter-shape geometry caps (metres / radians).
    pub const MAX_PARTICLE_SHAPE_RADIUS: f32 = 100.0;
    pub const MAX_PARTICLE_SHAPE_HALF_EXTENT: f32 = 100.0;
    pub const MAX_PARTICLE_SHAPE_HEIGHT: f32 = 100.0;
    pub const MAX_PARTICLE_CONE_HALF_ANGLE: f32 = std::f32::consts::PI;
    /// Per-axis sprite-sheet atlas dimension cap. 16 × 16 = 256 frames
    /// is well past any plausible animated particle effect and keeps
    /// the per-frame mesh cache bounded.
    pub const MAX_PARTICLE_ATLAS_DIM: u32 = 16;
    /// Frame-cycle FPS cap for `AnimationFrameMode::OverLifetime`. 60
    /// matches the typical render cadence; values above that just
    /// stutter visually since the tick system samples at frame rate.
    pub const MAX_PARTICLE_FRAME_FPS: f32 = 60.0;
}

/// Recursively clamp a [`Generator`] tree. Beyond the depth and total-node
/// budgets (see [`limits::MAX_GENERATOR_DEPTH`] and
/// [`limits::MAX_GENERATOR_NODES`]), each node's transform and kind are
/// clamped so a malicious record can't pass NaN/negative scales to Bevy's
/// primitive mesh constructors or the Avian collider builders.
///
/// **Strict positional rules.**
///
/// * **Terrain is root-only.** The terrain plugin owns the world's
///   heightmap; allowing a Terrain in a child slot would either spawn a
///   second heightfield collider (Avian forbids that) or be silently
///   ignored. A non-root Terrain is overwritten with a default cuboid.
///   *A Terrain root MAY have children* — that's the "region blueprint"
///   shape, where the terrain root anchors a tree of L-systems / portals /
///   props that travel together.
/// * **Water is child-only.** Every Water volume must inherit a parent
///   (typically a Terrain ancestor) so its world-space surface is
///   well-defined. A root Water is overwritten with a default cuboid —
///   `RoomRecord::default_for_did` puts water inside the terrain root, and
///   inventory-saved water should always be a child of the region it
///   belongs to. Water itself is a leaf (its `children` list is cleared).
fn sanitize_generator_node(node: &mut Generator, depth: u32, count: &mut u32, is_root: bool) {
    *count += 1;
    sanitize_prim_transform(&mut node.transform);

    if !is_root && matches!(&node.kind, GeneratorKind::Terrain(_)) {
        // Terrain at non-root: not a valid position. Overwrite rather than
        // reject so the node still round-trips and the owner can fix it.
        node.kind = GeneratorKind::default_cuboid();
    }
    if is_root && matches!(&node.kind, GeneratorKind::Water { .. }) {
        // Water at the root of a named generator: not a valid position.
        // Water needs an ancestor whose transform anchors the volume.
        node.kind = GeneratorKind::default_cuboid();
    }

    sanitize_kind(&mut node.kind);

    // Water is a leaf — `spawn_water_volume` does not consume children, so
    // strip authored children to keep the editor and spawner in sync.
    if matches!(&node.kind, GeneratorKind::Water { .. }) {
        node.children.clear();
        return;
    }

    if depth >= limits::MAX_GENERATOR_DEPTH || *count >= limits::MAX_GENERATOR_NODES {
        node.children.clear();
        return;
    }
    // Drop the tail children whose recursion budget we couldn't afford so
    // the survivor count matches the spawn budget exactly.
    let mut visited = 0usize;
    for (i, child) in node.children.iter_mut().enumerate() {
        if *count >= limits::MAX_GENERATOR_NODES {
            break;
        }
        sanitize_generator_node(child, depth + 1, count, false);
        visited = i + 1;
    }
    if visited < node.children.len() {
        node.children.truncate(visited);
    }
}

/// Clamp a `TransformData` so the downstream Bevy/Avian constructors can't
/// be fed NaN, infinities, or non-positive scales.
pub(crate) fn sanitize_prim_transform(t: &mut TransformData) {
    let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
    let clamp_pos = |v: f32| {
        if v.is_finite() {
            v.clamp(0.001, 1_000.0)
        } else {
            1.0
        }
    };
    let clamp_offset = |v: f32| {
        if v.is_finite() {
            v.clamp(-10_000.0, 10_000.0)
        } else {
            0.0
        }
    };
    t.translation = Fp3([
        clamp_offset(t.translation.0[0]),
        clamp_offset(t.translation.0[1]),
        clamp_offset(t.translation.0[2]),
    ]);
    let rot = [
        finite(t.rotation.0[0], 0.0),
        finite(t.rotation.0[1], 0.0),
        finite(t.rotation.0[2], 0.0),
        finite(t.rotation.0[3], 1.0),
    ];
    let len_sq = rot[0] * rot[0] + rot[1] * rot[1] + rot[2] * rot[2] + rot[3] * rot[3];
    t.rotation = if len_sq > 1e-6 {
        let inv = len_sq.sqrt().recip();
        Fp4([rot[0] * inv, rot[1] * inv, rot[2] * inv, rot[3] * inv])
    } else {
        Fp4([0.0, 0.0, 0.0, 1.0])
    };
    t.scale = Fp3([
        clamp_pos(t.scale.0[0]),
        clamp_pos(t.scale.0[1]),
        clamp_pos(t.scale.0[2]),
    ]);
}

/// Clamp a `SovereignMaterialSettings` so render/PBR parameters stay in
/// physically sensible ranges. Colour channels are `[0,1]`, roughness and
/// metallic are `[0,1]`, emission strength is capped. Also clamps the
/// embedded [`SovereignTextureConfig`] so octave-style DoS vectors can't
/// ride in via a PBR material.
pub(crate) fn sanitize_material_settings(m: &mut SovereignMaterialSettings) {
    let clamp_unit = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
    m.base_color = clamp3(m.base_color);
    m.emission_color = clamp3(m.emission_color);
    m.emission_strength = Fp(if m.emission_strength.0.is_finite() {
        m.emission_strength.0.clamp(0.0, 1_000.0)
    } else {
        0.0
    });
    m.roughness = Fp(clamp_unit(m.roughness.0));
    m.metallic = Fp(clamp_unit(m.metallic.0));
    m.uv_scale = Fp(if m.uv_scale.0.is_finite() {
        m.uv_scale.0.clamp(0.001, 1_000.0)
    } else {
        1.0
    });
    sanitize_texture_config(&mut m.texture);
}

/// Clamp octave-style fields on a `SovereignTextureConfig` variant so a
/// malicious record cannot tell the procedural texture pipeline to run
/// billions of noise iterations per pixel. Variants without an octave-like
/// parameter are passed through untouched — their cost is bounded by the
/// texture resolution cap in [`limits::MAX_TEXTURE_SIZE`].
pub(crate) fn sanitize_texture_config(cfg: &mut SovereignTextureConfig) {
    match cfg {
        SovereignTextureConfig::Ground(g) => {
            g.macro_octaves = g.macro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
            g.micro_octaves = g.micro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
        }
        SovereignTextureConfig::Rock(r) => {
            r.octaves = r.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Bark(b) => {
            b.octaves = b.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Stucco(s) => {
            s.octaves = s.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Concrete(c) => {
            c.octaves = c.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        SovereignTextureConfig::Marble(m) => {
            m.octaves = m.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
        }
        _ => {}
    }
}

/// Clamp a single numeric value to a finite range, replacing NaN/Inf with
/// `default`. Used by the primitive sanitizer and `sanitize_torture`.
fn clamp_finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

/// Clamp the `(twist, taper, bend)` torture triple attached to every
/// primitive. Values drive the CPU-side vertex mutation pass in
/// `world_builder::prim::apply_vertex_torture`; out-of-range inputs produce
/// degenerate meshes (NaN vertex positions, zero-volume colliders) so we
/// clamp them on ingest rather than in the spawn loop.
fn sanitize_torture(twist: &mut Fp, taper: &mut Fp, bend: &mut Fp3) {
    let t = limits::MAX_TORTURE_TWIST;
    let tp = limits::MAX_TORTURE_TAPER;
    let b = limits::MAX_TORTURE_BEND;
    twist.0 = clamp_finite(twist.0, -t, t, 0.0);
    taper.0 = clamp_finite(taper.0, -tp, tp, 0.0);
    for i in 0..3 {
        bend.0[i] = clamp_finite(bend.0[i], -b, b, 0.0);
    }
}

/// Clamp every numeric field on a parametric primitive [`GeneratorKind`].
/// Mirrors the bounds the World Editor UI exposes so a hand-crafted record
/// can't push mesh/collider builders into NaN / OOM territory.
fn sanitize_primitive(kind: &mut GeneratorKind) {
    let c_dim = |v: f32| clamp_finite(v, 0.01, 100.0, 1.0);
    match kind {
        GeneratorKind::Cuboid {
            size,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Sphere {
            radius,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *resolution = (*resolution).clamp(0, 10);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *length = Fp(c_dim(length.0));
            *latitudes = (*latitudes).clamp(2, 64);
            *longitudes = (*longitudes).clamp(4, 128);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *minor_radius = Fp(c_dim(minor_radius.0));
            *major_radius = Fp(c_dim(major_radius.0));
            *minor_resolution = (*minor_resolution).clamp(3, 64);
            *major_resolution = (*major_resolution).clamp(3, 128);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Plane {
            size,
            subdivisions,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *size = Fp2([c_dim(size.0[0]), c_dim(size.0[1])]);
            *subdivisions = (*subdivisions).clamp(0, 32);
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        GeneratorKind::Tetrahedron {
            size,
            material,
            twist,
            taper,
            bend,
            ..
        } => {
            *size = Fp(c_dim(size.0));
            sanitize_material_settings(material);
            sanitize_torture(twist, taper, bend);
        }
        _ => {}
    }
}

/// Clamp every numeric field on a Water node — `level_offset` (compiler adds
/// it to the volume transform Y) and the [`WaterSurface`] appearance bundle
/// (every field flows directly to a shader uniform). Without this a hostile
/// record can push NaN/infinity into the volume transform or into the
/// per-fragment Gerstner-wave math, producing world-corrupting normals or a
/// portalled-away `Plane3d` whose collider cannot be built.
fn sanitize_water(level_offset: &mut Fp, surface: &mut WaterSurface) {
    let off = limits::MAX_WATER_LEVEL_OFFSET;
    level_offset.0 = clamp_finite(level_offset.0, -off, off, 0.0);

    let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
    surface.shallow_color = Fp4([
        unit(surface.shallow_color.0[0], 0.0),
        unit(surface.shallow_color.0[1], 0.0),
        unit(surface.shallow_color.0[2], 0.0),
        unit(surface.shallow_color.0[3], 1.0),
    ]);
    surface.deep_color = Fp4([
        unit(surface.deep_color.0[0], 0.0),
        unit(surface.deep_color.0[1], 0.0),
        unit(surface.deep_color.0[2], 0.0),
        unit(surface.deep_color.0[3], 1.0),
    ]);
    surface.roughness = Fp(unit(surface.roughness.0, 0.14));
    surface.metallic = Fp(unit(surface.metallic.0, 0.0));
    surface.reflectance = Fp(unit(surface.reflectance.0, 0.3));
    surface.wave_choppiness = Fp(unit(surface.wave_choppiness.0, 0.3));
    surface.foam_amount = Fp(unit(surface.foam_amount.0, 0.25));
    surface.wave_scale = Fp(clamp_finite(
        surface.wave_scale.0,
        0.0,
        limits::MAX_WAVE_SCALE,
        0.7,
    ));
    surface.wave_speed = Fp(clamp_finite(
        surface.wave_speed.0,
        -limits::MAX_WAVE_SPEED,
        limits::MAX_WAVE_SPEED,
        1.0,
    ));
    // The shader normalises `wave_direction`; a near-zero vector would
    // produce NaN there, so fall back to the default heading when the
    // sanitised components round to zero.
    let dx = clamp_finite(surface.wave_direction.0[0], -10.0, 10.0, 1.0);
    let dz = clamp_finite(surface.wave_direction.0[1], -10.0, 10.0, 0.3);
    let len_sq = dx * dx + dz * dz;
    surface.wave_direction = if len_sq > 1e-6 {
        Fp2([dx, dz])
    } else {
        Fp2([1.0, 0.3])
    };
    surface.flow_strength = Fp(clamp_finite(
        surface.flow_strength.0,
        0.0,
        limits::MAX_WATER_FLOW_STRENGTH,
        0.0,
    ));
    surface.flow_amount = Fp(clamp_finite(surface.flow_amount.0, 0.0, 1.0, 0.0));
}

/// Clamp the per-variant payload strings of a [`SignSource`]. Used
/// both by Sign generators and by the particle-texture sanitiser so
/// the URL / DID / CID caps stay consistent across both call sites.
pub(crate) fn sanitize_sign_source(source: &mut SignSource) {
    match source {
        SignSource::Url { url } => {
            truncate_on_char_boundary(url, limits::MAX_SIGN_URL_BYTES);
        }
        SignSource::AtprotoBlob { did, cid } => {
            truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
            truncate_on_char_boundary(cid, limits::MAX_SIGN_CID_BYTES);
        }
        SignSource::DidPfp { did } => {
            truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
        }
        SignSource::Unknown => {}
    }
}

/// Clamp every numeric field on a `Sign` generator and bound its source
/// strings. Defends against three weaponised inputs: (1) megabyte URLs that
/// would otherwise sit in the room recipe and bloat every record fetch,
/// (2) NaN / negative panel sizes that would smuggle a degenerate plane
/// mesh into the GPU, and (3) UV repeat factors so high they pin the
/// fragment shader on a sub-pixel texel pattern.
fn sanitize_sign(
    source: &mut SignSource,
    size: &mut Fp2,
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
    alpha_mode: &mut AlphaModeKind,
) {
    sanitize_sign_source(source);

    let s = limits::MAX_SIGN_SIZE;
    size.0[0] = clamp_finite(size.0[0], 0.01, s, 1.0);
    size.0[1] = clamp_finite(size.0[1], 0.01, s, 1.0);

    let r_lo = limits::MIN_SIGN_UV_REPEAT;
    let r_hi = limits::MAX_SIGN_UV_REPEAT;
    uv_repeat.0[0] = clamp_finite(uv_repeat.0[0], r_lo, r_hi, 1.0);
    uv_repeat.0[1] = clamp_finite(uv_repeat.0[1], r_lo, r_hi, 1.0);

    let o = limits::MAX_SIGN_UV_OFFSET;
    uv_offset.0[0] = clamp_finite(uv_offset.0[0], -o, o, 0.0);
    uv_offset.0[1] = clamp_finite(uv_offset.0[1], -o, o, 0.0);

    sanitize_material_settings(material);

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        cutoff.0 = clamp_finite(cutoff.0, 0.0, 1.0, 0.5);
    }
}

/// Clamp every numeric field on a `ParticleSystem` generator. Defends
/// against the three weaponised inputs particle systems are
/// historically vulnerable to: (1) emit rates so high they pin every
/// frame on entity spawning, (2) lifetimes so long the steady-state
/// population never decays, (3) acceleration / drag values that
/// produce NaN positions inside one tick. Also enforces `min ≤ max`
/// on the sampled ranges so the deterministic per-particle sampler
/// can't trip on an inverted interval.
#[allow(clippy::too_many_arguments)]
fn sanitize_particles(
    emitter_shape: &mut EmitterShape,
    rate_per_second: &mut Fp,
    burst_count: &mut u32,
    max_particles: &mut u32,
    duration: &mut Fp,
    lifetime_min: &mut Fp,
    lifetime_max: &mut Fp,
    speed_min: &mut Fp,
    speed_max: &mut Fp,
    gravity_multiplier: &mut Fp,
    acceleration: &mut Fp3,
    linear_drag: &mut Fp,
    start_size: &mut Fp,
    end_size: &mut Fp,
    start_color: &mut Fp4,
    end_color: &mut Fp4,
    inherit_velocity: &mut Fp,
    bounce: &mut Fp,
    friction: &mut Fp,
    texture: &mut Option<SignSource>,
    texture_atlas: &mut Option<TextureAtlas>,
    frame_mode: &mut AnimationFrameMode,
) {
    *max_particles = (*max_particles).min(limits::MAX_PARTICLES);
    rate_per_second.0 = clamp_finite(rate_per_second.0, 0.0, limits::MAX_PARTICLE_RATE, 0.0);
    *burst_count = (*burst_count).min(limits::MAX_PARTICLE_BURST);
    duration.0 = clamp_finite(
        duration.0,
        limits::MIN_PARTICLE_DURATION,
        limits::MAX_PARTICLE_DURATION,
        1.0,
    );

    lifetime_min.0 = clamp_finite(
        lifetime_min.0,
        limits::MIN_PARTICLE_LIFETIME,
        limits::MAX_PARTICLE_LIFETIME,
        limits::MIN_PARTICLE_LIFETIME,
    );
    lifetime_max.0 = clamp_finite(
        lifetime_max.0,
        limits::MIN_PARTICLE_LIFETIME,
        limits::MAX_PARTICLE_LIFETIME,
        limits::MIN_PARTICLE_LIFETIME,
    );
    if lifetime_max.0 < lifetime_min.0 {
        lifetime_max.0 = lifetime_min.0;
    }

    speed_min.0 = clamp_finite(speed_min.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
    speed_max.0 = clamp_finite(speed_max.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
    if speed_max.0 < speed_min.0 {
        speed_max.0 = speed_min.0;
    }

    gravity_multiplier.0 = clamp_finite(
        gravity_multiplier.0,
        -limits::MAX_PARTICLE_GRAVITY_MULT,
        limits::MAX_PARTICLE_GRAVITY_MULT,
        0.0,
    );
    let a = limits::MAX_PARTICLE_ACCEL;
    acceleration.0[0] = clamp_finite(acceleration.0[0], -a, a, 0.0);
    acceleration.0[1] = clamp_finite(acceleration.0[1], -a, a, 0.0);
    acceleration.0[2] = clamp_finite(acceleration.0[2], -a, a, 0.0);
    linear_drag.0 = clamp_finite(linear_drag.0, 0.0, limits::MAX_PARTICLE_DRAG, 0.0);

    start_size.0 = clamp_finite(
        start_size.0,
        limits::MIN_PARTICLE_SIZE,
        limits::MAX_PARTICLE_SIZE,
        0.1,
    );
    end_size.0 = clamp_finite(
        end_size.0,
        limits::MIN_PARTICLE_SIZE,
        limits::MAX_PARTICLE_SIZE,
        0.1,
    );

    let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
    *start_color = Fp4([
        unit(start_color.0[0], 1.0),
        unit(start_color.0[1], 1.0),
        unit(start_color.0[2], 1.0),
        unit(start_color.0[3], 1.0),
    ]);
    *end_color = Fp4([
        unit(end_color.0[0], 1.0),
        unit(end_color.0[1], 1.0),
        unit(end_color.0[2], 1.0),
        unit(end_color.0[3], 1.0),
    ]);

    inherit_velocity.0 = clamp_finite(
        inherit_velocity.0,
        0.0,
        limits::MAX_PARTICLE_INHERIT_VELOCITY,
        0.0,
    );
    bounce.0 = clamp_finite(bounce.0, 0.0, 1.0, 0.0);
    friction.0 = clamp_finite(friction.0, 0.0, 1.0, 0.0);

    if let Some(src) = texture {
        sanitize_sign_source(src);
    }
    if let Some(atlas) = texture_atlas {
        atlas.rows = atlas.rows.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
        atlas.cols = atlas.cols.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
    }
    if let AnimationFrameMode::OverLifetime { fps } = frame_mode {
        fps.0 = clamp_finite(fps.0, 0.0, limits::MAX_PARTICLE_FRAME_FPS, 0.0);
    }

    match emitter_shape {
        EmitterShape::Sphere { radius } => {
            radius.0 = clamp_finite(radius.0, 0.0, limits::MAX_PARTICLE_SHAPE_RADIUS, 0.5);
        }
        EmitterShape::Box { half_extents } => {
            let h = limits::MAX_PARTICLE_SHAPE_HALF_EXTENT;
            half_extents.0[0] = clamp_finite(half_extents.0[0], 0.0, h, 0.5);
            half_extents.0[1] = clamp_finite(half_extents.0[1], 0.0, h, 0.5);
            half_extents.0[2] = clamp_finite(half_extents.0[2], 0.0, h, 0.5);
        }
        EmitterShape::Cone { half_angle, height } => {
            half_angle.0 =
                clamp_finite(half_angle.0, 0.0, limits::MAX_PARTICLE_CONE_HALF_ANGLE, 0.4);
            height.0 = clamp_finite(height.0, 0.0, limits::MAX_PARTICLE_SHAPE_HEIGHT, 0.5);
        }
        EmitterShape::Point | EmitterShape::Unknown => {}
    }
}

/// Clamp the variant-specific payload of a [`GeneratorKind`] in place. Does
/// not touch the wrapping [`Generator`]'s transform or children — those are
/// handled by [`sanitize_generator_node`] which calls this on every node.
pub fn sanitize_kind(kind: &mut GeneratorKind) {
    match kind {
        GeneratorKind::Terrain(cfg) => sanitize_terrain_cfg(cfg),
        GeneratorKind::LSystem {
            source_code,
            finalization_code,
            iterations,
            mesh_resolution,
            materials,
            ..
        } => {
            truncate_on_char_boundary(source_code, limits::MAX_LSYSTEM_CODE_BYTES);
            truncate_on_char_boundary(finalization_code, limits::MAX_LSYSTEM_CODE_BYTES);
            *iterations = (*iterations).min(limits::MAX_LSYSTEM_ITERATIONS);
            *mesh_resolution = (*mesh_resolution).clamp(3, limits::MAX_LSYSTEM_MESH_RESOLUTION);
            // Without this, a peer could ship a `Bark` slot with
            // `octaves = 4_000_000_000` (or NaN emission) and hang the
            // procedural texture task the moment a scatter lands.
            for settings in materials.values_mut() {
                sanitize_material_settings(settings);
            }
        }
        GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            materials,
            ..
        } => {
            truncate_on_char_boundary(grammar_source, limits::MAX_SHAPE_SOURCE_BYTES);
            truncate_on_char_boundary(root_rule, limits::MAX_SHAPE_ROOT_RULE_BYTES);
            // Clamp each footprint axis to a finite, non-negative range. Y is
            // allowed to be 0.0 because most grammars `Extrude` from a flat
            // 2-D plot; the others must stay positive so the interpreter's
            // split / repeat math doesn't divide by zero.
            footprint.0[0] = clamp_finite(footprint.0[0], 0.001, limits::MAX_SHAPE_FOOTPRINT, 10.0);
            footprint.0[1] = clamp_finite(footprint.0[1], 0.0, limits::MAX_SHAPE_FOOTPRINT, 0.0);
            footprint.0[2] = clamp_finite(footprint.0[2], 0.001, limits::MAX_SHAPE_FOOTPRINT, 10.0);
            // Cap the slot count first so the per-slot sanitiser doesn't
            // walk an attacker-supplied million-entry map. Slot keys above
            // the upstream identifier cap are dropped — they could never
            // match an emitted `Mat("...")` anyway.
            if materials.len() > limits::MAX_SHAPE_MATERIAL_SLOTS {
                let mut keys: Vec<String> = materials.keys().cloned().collect();
                keys.sort();
                for k in keys.into_iter().skip(limits::MAX_SHAPE_MATERIAL_SLOTS) {
                    materials.remove(&k);
                }
            }
            materials.retain(|k, _| k.len() <= limits::MAX_SHAPE_ROOT_RULE_BYTES);
            for settings in materials.values_mut() {
                sanitize_material_settings(settings);
            }
        }
        GeneratorKind::Portal {
            target_did,
            target_pos,
        } => {
            truncate_on_char_boundary(target_did, 256);
            target_pos.0[0] = target_pos.0[0].clamp(-10_000.0, 10_000.0);
            target_pos.0[1] = target_pos.0[1].clamp(-1_000.0, 10_000.0);
            target_pos.0[2] = target_pos.0[2].clamp(-10_000.0, 10_000.0);
        }
        GeneratorKind::Cuboid { .. }
        | GeneratorKind::Sphere { .. }
        | GeneratorKind::Cylinder { .. }
        | GeneratorKind::Capsule { .. }
        | GeneratorKind::Cone { .. }
        | GeneratorKind::Torus { .. }
        | GeneratorKind::Plane { .. }
        | GeneratorKind::Tetrahedron { .. } => sanitize_primitive(kind),
        GeneratorKind::Water {
            level_offset,
            surface,
        } => sanitize_water(level_offset, surface),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            alpha_mode,
            ..
        } => sanitize_sign(source, size, uv_repeat, uv_offset, material, alpha_mode),
        GeneratorKind::ParticleSystem {
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
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
            inherit_velocity,
            bounce,
            friction,
            texture,
            texture_atlas,
            frame_mode,
            ..
        } => sanitize_particles(
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
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
            inherit_velocity,
            bounce,
            friction,
            texture,
            texture_atlas,
            frame_mode,
        ),
        GeneratorKind::Unknown => {}
    }
}

/// Clamp a whole [`Generator`] tree (root + descendants) in place. Shared
/// by [`super::room::RoomRecord::sanitize`] and
/// [`super::inventory::InventoryRecord::sanitize`] so the per-variant
/// bounds — and the depth / total-node budgets — stay identical between
/// the room recipe and the inventory stash.
pub fn sanitize_generator(generator: &mut Generator) {
    let mut count: u32 = 0;
    sanitize_generator_node(generator, 0, &mut count, true);
}

/// Avatar-specific sanitiser. Reuses [`sanitize_generator_node`]'s
/// depth, total-node, and per-kind clamps, then walks the tree and
/// rewrites every kind that is forbidden inside an avatar's visual
/// subtree (Terrain, Water, Portal) into a default cuboid.
///
/// Terrain / Water / Portal are excluded by design. Terrain owns the
/// world heightmap; allowing it inside an avatar would either spawn a
/// second heightfield collider (Avian forbids) or be silently ignored.
/// Water needs an ancestor whose transform anchors the volume in world
/// space — meaningless on a vehicle. Portal would let an avatar carry
/// a moving travel target into another peer's space, which is both
/// abusive (drag a stranger through your portal) and confusing (the
/// portal moves with the player).
///
/// Primitives + LSystem + Shape all round-trip; the avatar spawn path
/// (`world_builder::avatar_spawn::spawn_avatar_visuals_subtree`)
/// reuses the same dispatcher as the room compiler with the room-only
/// behaviours (RoomEntity, PrimMarker, per-prim colliders) suppressed.
pub fn sanitize_avatar_visuals(generator: &mut Generator) {
    sanitize_generator(generator);
    enforce_avatar_kinds(generator);
}

fn enforce_avatar_kinds(node: &mut Generator) {
    if matches!(
        &node.kind,
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::Portal { .. }
    ) {
        node.kind = GeneratorKind::default_cuboid();
    }
    for child in node.children.iter_mut() {
        enforce_avatar_kinds(child);
    }
}

pub(crate) fn sanitize_terrain_cfg(cfg: &mut SovereignTerrainConfig) {
    cfg.grid_size = cfg.grid_size.clamp(2, limits::MAX_GRID_SIZE);
    // `cell_scale` and `height_scale` feed straight into the heightmap
    // mesh/collider builders. A NaN or infinity smuggled in via a malicious
    // record propagates to `avian3d` collider construction and panics the
    // physics step, so clamp both to finite positive ranges.
    cfg.cell_scale = Fp(cfg
        .cell_scale
        .0
        .clamp(limits::MIN_CELL_SCALE, limits::MAX_CELL_SCALE));
    cfg.height_scale = Fp(cfg
        .height_scale
        .0
        .clamp(limits::MIN_HEIGHT_SCALE, limits::MAX_HEIGHT_SCALE));
    cfg.octaves = cfg.octaves.clamp(1, limits::MAX_OCTAVES);
    cfg.voronoi_num_seeds = cfg.voronoi_num_seeds.clamp(1, limits::MAX_VORONOI_SEEDS);
    cfg.voronoi_num_terraces = cfg
        .voronoi_num_terraces
        .clamp(1, limits::MAX_VORONOI_TERRACES);
    cfg.erosion_drops = cfg.erosion_drops.min(limits::MAX_EROSION_DROPS);
    cfg.thermal_iterations = cfg.thermal_iterations.min(limits::MAX_THERMAL_ITERATIONS);
    cfg.material.texture_size = cfg
        .material
        .texture_size
        .clamp(16, limits::MAX_TEXTURE_SIZE);
    // Cap per-variant octave-like fields so a forward-compat peer cannot
    // weaponise texture-size × octave blowups.
    for layer in cfg.material.layers.iter_mut() {
        sanitize_texture_config(layer);
    }
}
