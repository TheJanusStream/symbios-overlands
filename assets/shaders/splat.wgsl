// Fragment shader for splat-based terrain materials.
//
// Replaces the CPU bake step: the weight map and two texture arrays
// (albedo + normal, each with 4 layers) are bound directly and blended
// per-pixel on the GPU.
//
// Layer indices within each array:
//   0 = Grass, 1 = Dirt, 2 = Rock, 3 = Snow
//
// Texture arrays reduce the active texture unit count from 9 (the old
// per-layer discrete binding scheme) down to 3, safely fitting within the
// WebGL 2 minimum guarantee of 16 texture image units.
//
// UVs on the terrain mesh span [0, 1] across the full terrain, so:
//   - The weight map is sampled at those UVs (one texel per heightmap cell).
//   - The layer textures are sampled at `uv * tile_scale`; the Repeat address
//     mode handles wrapping and preserves hardware derivatives for correct
//     mipmap selection (fract() would destroy derivatives at tile boundaries).
//
// When `splat_uniforms.enabled == 0` the splat logic is bypassed and the base
// StandardMaterial colour is passed through unchanged (useful for the disabled
// state and while textures are still loading).
//
// NOTE: Bevy 0.18 places the material bind group at group 3 (MATERIAL_BIND_GROUP = 3).
// All bindings use @group(#{MATERIAL_BIND_GROUP}) so this is correct regardless
// of the Bevy version.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{
        apply_pbr_lighting,
        main_pass_post_lighting_processing,
        calculate_tbn_mikktspace,
        apply_normal_mapping,
    },
    pbr_types::STANDARD_MATERIAL_FLAGS_FLIP_NORMAL_MAP_Y,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
}
#endif

// ---------------------------------------------------------------------------
// Extension bindings (slots 100+ are reserved for material extensions).
// ---------------------------------------------------------------------------

/// RGBA weight map — one texel per heightmap cell, full-terrain coverage.
/// Channels: R = Grass, G = Dirt, B = Rock, A = Snow.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var splat_weight_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var splat_weight_sampler: sampler;

/// Albedo texture array — 4 layers (Grass=0, Dirt=1, Rock=2, Snow=3), sRGB.
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var albedo_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var albedo_array_sampler: sampler;

/// Normal map texture array — 4 layers (Grass=0, Dirt=1, Rock=2, Snow=3), linear.
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var normal_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var normal_array_sampler: sampler;

struct SplatUniforms {
    /// How many times the tiling textures repeat across the terrain.
    tile_scale: f32,
    /// Non-zero enables splat blending; zero passes through the base material.
    enabled: u32,
    /// World-space UV scale for the Rock triplanar projection.
    /// Equals tile_scale / world_extent so density matches the top-down layers.
    triplanar_scale: f32,
    /// Blend sharpness for the triplanar axis transition (k >= 1; 4 is good).
    triplanar_sharpness: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(106) var<uniform> splat_uniforms: SplatUniforms;

// ---------------------------------------------------------------------------
// Triplanar helpers (used for the Rock layer only)
// ---------------------------------------------------------------------------

/// Compute per-axis blend weights from the world-space surface normal.
/// |normal| is assumed to be unit length; k sharpens the transition seam.
fn triplanar_weights(world_normal: vec3<f32>, k: f32) -> vec3<f32> {
    var w = pow(abs(world_normal), vec3<f32>(k));
    return w / (w.x + w.y + w.z + 0.0001);
}

/// Sample a texture array layer using triplanar world-space projection and
/// return vec4.  Three lookups — YZ, XZ, XY planes — are blended by `weights`.
fn triplanar_albedo(
    tex: texture_2d_array<f32>,
    samp: sampler,
    layer: i32,
    world_pos: vec3<f32>,
    scale: f32,
    weights: vec3<f32>,
) -> vec4<f32> {
    let col_x = textureSample(tex, samp, world_pos.zy * scale, layer);
    let col_y = textureSample(tex, samp, world_pos.xz * scale, layer);
    let col_z = textureSample(tex, samp, world_pos.xy * scale, layer);
    return col_x * weights.x + col_y * weights.y + col_z * weights.z;
}

/// Decode a packed normal-map sample from [0, 1] to [-1, 1] tangent space.
fn decode_normal(encoded: vec3<f32>) -> vec3<f32> {
    return encoded * 2.0 - 1.0;
}

/// Sample a normal map array layer using triplanar projection and return a
/// world-space normal.
///
/// Each projection plane gets its own synthesized TBN so that cliff-face
/// normals (dominant X/Z contribution) are decoded relative to the correct
/// tangent frame before blending, instead of being misinterpreted through the
/// top-down mesh TBN.
///
/// Axis TBN frames — aligned with UV derivatives for correct perturbation:
///   X-projection (uv = world.zy): T = sign_x·Z, B = +Y, N via cross
///   Y-projection (uv = world.xz): T = +X,       B = sign_y·Z, N via cross
///   Z-projection (uv = world.xy): T = sign_z·X,  B = +Y,      N = sign_z·Z
///
/// The X and Y frames are left-handed (T×B = -N) so that the tangent and
/// bitangent point in the same direction as the UV's U and V axes.  The
/// outward (tz) component is negated implicitly in the reprojection formula
/// to compensate, ensuring a flat tangent-space normal (0,0,1) still maps
/// to the correct face normal.
fn triplanar_normal_world(
    tex: texture_2d_array<f32>,
    samp: sampler,
    layer: i32,
    world_pos: vec3<f32>,
    world_normal: vec3<f32>,
    scale: f32,
    weights: vec3<f32>,
) -> vec3<f32> {
    let sign_x = select(-1.0, 1.0, world_normal.x >= 0.0);
    let sign_y = select(-1.0, 1.0, world_normal.y >= 0.0);
    let sign_z = select(-1.0, 1.0, world_normal.z >= 0.0);

    let tn_x = decode_normal(textureSample(tex, samp, world_pos.zy * scale, layer).rgb);
    let tn_y = decode_normal(textureSample(tex, samp, world_pos.xz * scale, layer).rgb);
    let tn_z = decode_normal(textureSample(tex, samp, world_pos.xy * scale, layer).rgb);

    // Reproject each tangent-space normal into world space via the per-face TBN.
    // T/B are aligned with the UV sampling directions so that tangent-space
    // perturbations (tx, ty) map to the correct world-space directions.
    //   X: T=sx·Z, B=+Y  (left-handed, tz negated) → world = (tz·sx,  ty,  tx·sx)
    //   Y: T=+X,   B=sy·Z (left-handed, tz negated) → world = (tx,    tz·sy, ty·sy)
    //   Z: T=sz·X, B=+Y   (right-handed)            → world = (tx·sz, ty,    tz·sz)
    //
    // The Y-projection's tx and ty are negated below to invert the
    // horizontal perturbation direction. Without this, rock-texture bumps
    // render as if lit from the opposite direction (i.e. as dents instead of
    // ridges) on top-down terrain, while the standard layers (grass/dirt/snow)
    // — which use Bevy's mesh TBN with FLIP_NORMAL_MAP_Y — render correctly.
    // The negation makes the rock layer's lit direction match the others.
    let wn_x = vec3<f32>(tn_x.z * sign_x, tn_x.y, tn_x.x * sign_x);
    let wn_y = vec3<f32>(-tn_y.x, tn_y.z * sign_y, -tn_y.y * sign_y);
    let wn_z = vec3<f32>(tn_z.x * sign_z, tn_z.y, tn_z.z * sign_z);

    return normalize(wn_x * weights.x + wn_y * weights.y + wn_z * weights.z);
}

// ---------------------------------------------------------------------------
// Fragment entry point
// ---------------------------------------------------------------------------

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // Start from standard PBR state (reads base_color, roughness, etc. from
    // the StandardMaterial uniform; no textures are set on the base so N is
    // the interpolated vertex normal and base_color is the uniform value).
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    if splat_uniforms.enabled != 0u {
        // Sample splat weights from the terrain-space UV ([0, 1] → full mesh).
        let raw_weights = textureSample(splat_weight_map, splat_weight_sampler, in.uv);

        // Normalise weights so they always sum to exactly 1.  SplatMapper rules
        // can overlap or leave gaps, causing the raw sum to differ from 1.  A
        // non-unit sum corrupts the normal decode step: encoded normals are in
        // [0, 1] and decoded via `* 2 - 1`.  If the blended vector drifts
        // toward (0.5, 0.5, 0.5) the decoded result is (0, 0, 0), and
        // normalising a zero vector produces NaN that infects the whole pixel.
        //
        // When no rule matches at all (raw_sum == 0 — common on very steep
        // cliffs that exceed every layer's slope_max), dividing vec4(0) by a
        // small epsilon still yields vec4(0), which would normalise to NaN.
        // Fall back to 100% rock (channel B) in that case: rock is the most
        // appropriate material for unclassified steep terrain.
        let raw_sum = raw_weights.r + raw_weights.g + raw_weights.b + raw_weights.a;
        let no_coverage = raw_sum < 0.0001;
        let safe_sum = select(raw_sum, 1.0, no_coverage);
        let weights = select(raw_weights / safe_sum, vec4<f32>(0.0, 0.0, 1.0, 0.0), no_coverage);

        // Tiled UV — sampler Repeat mode handles wrapping, preserving GPU derivatives.
        let tiled_uv = in.uv * splat_uniforms.tile_scale;

        // Triplanar data for the Rock layer — computed once, shared by albedo
        // and normal sampling below.
        let world_pos = in.world_position.xyz;
        let tp_weights = triplanar_weights(
            in.world_normal,
            splat_uniforms.triplanar_sharpness,
        );

        // --- Albedo blend ---------------------------------------------------
        // Grass (0), Dirt (1), Snow (3) use standard top-down tiled UVs.
        // Rock (2) uses triplanar world-space projection to eliminate stretching
        // on steep cliff faces.
        let a0 = textureSample(albedo_array, albedo_array_sampler, tiled_uv, 0);
        let a1 = textureSample(albedo_array, albedo_array_sampler, tiled_uv, 1);
        let a2 = triplanar_albedo(
            albedo_array, albedo_array_sampler, 2,
            world_pos, splat_uniforms.triplanar_scale, tp_weights,
        );
        let a3 = textureSample(albedo_array, albedo_array_sampler, tiled_uv, 3);

        pbr_input.material.base_color =
            a0 * weights.r + a1 * weights.g + a2 * weights.b + a3 * weights.a;

        // --- Normal-map blend -----------------------------------------------
        // All normals are converted to world space per-layer before blending.
        // Grass, Dirt, and Snow use the mesh Mikktspace TBN (top-down UV frame);
        // Rock uses a per-axis synthesized TBN so cliff-face projections are
        // correctly oriented before contributing to the blend.
        //
        // FLIP_NORMAL_MAP_Y is required: bevy_mesh flips mikktspace's tangent.w
        // sign after generation (bevy_mesh-0.18 mikktspace.rs:127), so for our
        // terrain UVs (+V → +Z world) Bevy's bitangent ends up as -Z, opposite
        // the +V direction that bevy_symbios_texture::normal encodes against.
        // The flag negates Nt.y to compensate, restoring correct V/Z perturbation.
        // (The Rock triplanar path builds its own TBN and does not need this.)
#ifdef VERTEX_TANGENTS
        let tbn = calculate_tbn_mikktspace(in.world_normal, in.world_tangent);

        // Convert packed tangent-space normals → world space via the mesh TBN.
        let n0 = textureSample(normal_array, normal_array_sampler, tiled_uv, 0).rgb;
        let n1 = textureSample(normal_array, normal_array_sampler, tiled_uv, 1).rgb;
        let n3 = textureSample(normal_array, normal_array_sampler, tiled_uv, 3).rgb;
        let flip_y = STANDARD_MATERIAL_FLAGS_FLIP_NORMAL_MAP_Y;
        let wn0 = apply_normal_mapping(flip_y, tbn, false, is_front, n0);
        let wn1 = apply_normal_mapping(flip_y, tbn, false, is_front, n1);
        let wn3 = apply_normal_mapping(flip_y, tbn, false, is_front, n3);

        // Rock: triplanar world-space conversion per projection plane.
        let wn2 = triplanar_normal_world(
            normal_array, normal_array_sampler, 2,
            world_pos, in.world_normal, splat_uniforms.triplanar_scale, tp_weights,
        );

        pbr_input.N = normalize(
            wn0 * weights.r + wn1 * weights.g + wn2 * weights.b + wn3 * weights.a
        );
#endif // VERTEX_TANGENTS
    }

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
