// Vertex shader for vegetation wind sway (#916).
//
// What this does:
//   1. Runs the stock Bevy mesh vertex transform, but displaces the vertex in
//      *world* space between the local→world and world→clip steps. Wind is a
//      world phenomenon: displacing in world space means a plant's own yaw
//      doesn't rotate the direction it leans, which is what happens if the
//      offset is applied to the local position instead.
//   2. Weights the displacement by height above the entity's own origin:
//      `w = saturate((world.y - origin.y) * height_scale + height_bias)`.
//      That one expression covers both callers without a branch —
//      see `WindUniforms::height_scale` in `src/wind.rs` for the two
//      configurations and why they differ.
//   3. Leans along `wind_dir` with a slow `bend` wave, adds a faster
//      perpendicular `flutter` so leaves shimmer rather than merely lean, and
//      drops the tip slightly as it leans so the frond reads as bending
//      rather than stretching. Cheap sines throughout — the GPU Gems 3 ch.16
//      (Crysis) leaf layer, minus the trunk bending we deliberately don't do
//      (foliage-only, by design: bark stays rigid and keeps a plain
//      `StandardMaterial`).
//   4. Phases each instance off a hash of its origin, and delays the gust by
//      distance along the wind direction. Without the first, a scatter of 500
//      identical cards beats in perfect unison; without the second, the whole
//      meadow leans as one slab instead of a gust crossing it.
//
// Per-instance variation is derived here from the instance origin rather than
// carried on the material, because a per-instance uniform would fork the
// material handle per instance and destroy the batching the entire scatter
// tier depends on (one mesh + one material for the whole scatter).
//
// Designed to run on WebGL2: arithmetic only — no new vertex attributes, no
// textures, no storage buffers, and no dependency on the depth prepass (which
// `camera.rs` omits on wasm).
//
// This file is used for BOTH the forward vertex shader and the prepass one.
// Bevy renders shadow maps through the prepass pipeline, so a displacement
// applied only in the forward pass would leave every foliage shadow frozen in
// the mesh's rest pose while the geometry above it moved. `PREPASS_PIPELINE`
// selects the matching IO structs; `wind_offset` is shared verbatim, which is
// what keeps the two passes agreeing.

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{Vertex, VertexOutput},
    mesh_functions,
    view_transformations::position_world_to_clip,
}
#import bevy_render::globals::Globals

// The prepass binds a *different, smaller* view layout than the main pass:
// `Globals` sits at `@group(0) @binding(1)` here but at binding 11 there, and
// `bevy_pbr::prepass_bindings` declares neither — the stock prepass shader
// has no use for the clock, so nothing in the tree declares it at the prepass
// index. Importing `mesh_view_bindings::globals` instead compiles cleanly and
// then fails at pipeline creation with
// `Shader global ResourceBinding { group: 0, binding: 11 } is not available
// in the pipeline layout` — a hard panic the moment a shadow-casting light
// sees foliage, which no forward-only render reaches.
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
}
#endif

// Mirror of `WindUniforms` in `src/wind.rs`. The two are hand-written copies
// of one layout and nothing in the build compiles this file, so a field added
// to one and not the other reads at the wrong offsets at runtime rather than
// failing to build — `wind::tests::wgsl_block_mirrors_the_rust_one` is what
// catches that.
struct WindUniforms {
    wind_dir: vec2<f32>,
    speed: f32,
    strength: f32,
    height_scale: f32,
    height_bias: f32,
    flutter: f32,
    _pad0: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> wind: WindUniforms;

// David Hoskins' sine-free hash, as used by `cloud.wgsl` and `water.wgsl`.
// Stays well-conditioned at the large world coordinates a region reaches,
// where a `sin`-based hash degenerates into visible banding.
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// World-space displacement for a vertex at `world_pos` belonging to an
// instance whose origin is `origin` (the entity's world translation).
fn wind_offset(world_pos: vec3<f32>, origin: vec3<f32>) -> vec3<f32> {
    // Height weight, squared so the base is near-rigid and the motion
    // accumulates toward the tip instead of shearing the whole card sideways.
    let h = saturate((world_pos.y - origin.y) * wind.height_scale + wind.height_bias);
    let weight = h * h;

    // `wind_dir` is authored freely (it need not be unit length, and a room
    // may legitimately set it to zero), so pad before normalising: an exactly
    // zero vector would otherwise produce NaNs across every foliage vertex.
    let dir2 = normalize(wind.wind_dir + vec2<f32>(1.0e-5, 0.0));
    let dir = vec3<f32>(dir2.x, 0.0, dir2.y);
    let side = vec3<f32>(-dir2.y, 0.0, dir2.x);

    // Quantised to 0.25 m so an instance's phase is stable frame to frame
    // while neighbours still differ.
    let phase = hash21(floor(origin.xz * 4.0)) * 6.2831853;
    let t = globals.time * wind.speed;
    // Gusts travel *with* the wind: sampling the origin along `dir2` delays
    // downwind plants, so the lean crosses a meadow as a wave.
    let gust = dot(origin.xz, dir2) * 0.08;

    let bend = sin(t * 0.35 - gust + phase);
    let flutter = sin(t * 1.7 + phase * 2.3 + (world_pos.y - origin.y) * 3.0);
    let amp = wind.strength * weight;

    // The vertical term is a cheap stand-in for arc-length preservation: a
    // leaning frond's tip descends, and without this the silhouette stretches
    // as it swings.
    return dir * (bend * amp)
        + side * (flutter * amp * wind.flutter)
        - vec3<f32>(0.0, abs(bend) * amp * 0.25, 0.0);
}

#ifdef PREPASS_PIPELINE

// Prepass / shadow-map variant. Mirrors `bevy_pbr::prepass::prepass.wgsl`'s
// vertex entry point, minus the skinning and morph-target paths: this material
// is only ever attached to foliage buckets and ground-cover cards, neither of
// which is skinned or morphed. Everything else is kept faithfully, because
// dropping any of it silently breaks a pass that only some configurations
// reach — `UNCLIPPED_DEPTH_ORTHO_EMULATION` in particular is what keeps
// directional-light shadow maps from clipping on hardware without native
// depth-clip control.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let rest_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0)
    );
    let origin = world_from_local[3].xyz;
    let offset = wind_offset(rest_position.xyz, origin);

    out.world_position = vec4<f32>(rest_position.xyz + offset, rest_position.w);
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0); // Clamp depth to avoid clipping
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
#ifdef VERTEX_NORMALS
    // Left as the rest-pose normal. Recomputing it would mean rebuilding the
    // tangent frame per vertex for a sub-degree change on geometry that is
    // double-sided alpha-masked foliage — invisible, and it would have to be
    // duplicated identically in the forward pass to avoid a shading seam.
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index
    );
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex.instance_index
    );
#endif
#endif // NORMAL_PREPASS_OR_DEFERRED_PREPASS

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef MOTION_VECTOR_PREPASS
    // Displace the previous frame's position by the *current* offset rather
    // than re-evaluating the sway at the previous time. The sway then
    // contributes nothing to the motion vector, which is the conservative
    // choice: a TAA pass reprojecting a fast per-vertex flutter smears worse
    // than one that ignores it.
    let prev_rest = mesh_functions::mesh_position_local_to_world(
        mesh_functions::get_previous_world_from_local(vertex.instance_index),
        vec4<f32>(vertex.position, 1.0)
    );
    out.previous_world_position = vec4<f32>(prev_rest.xyz + offset, prev_rest.w);
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif

    return out;
}

#else // PREPASS_PIPELINE

// Forward variant. Mirrors `bevy_pbr::render::mesh.wgsl`'s vertex entry point
// (again without skinning / morph targets) so the standard PBR fragment
// shader — which this material keeps, via `ShaderRef::Default` — receives
// exactly the inputs it expects.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);

#ifdef VERTEX_NORMALS
    // See the note in the prepass variant: the rest-pose normal is used in
    // both passes so the two agree.
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex.instance_index
    );
#endif

#ifdef VERTEX_POSITIONS
    let rest_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0)
    );
    let origin = world_from_local[3].xyz;
    let offset = wind_offset(rest_position.xyz, origin);
    out.world_position = vec4<f32>(rest_position.xyz + offset, rest_position.w);
    out.position = position_world_to_clip(out.world_position.xyz);
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif

#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        vertex.tangent,
        vertex.instance_index
    );
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif

    return out;
}

#endif // PREPASS_PIPELINE
