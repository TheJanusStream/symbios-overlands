// The splat terrain's prepass fragment stage (P4.2, #1597).
//
// An opaque splat material's depth-only prepass runs no fragment stage, so
// this runs only where one may discard: a far field with a detail patch's
// hole cut in it, drawn as a mask (`SPLAT_HOLE`). There the depth prepass
// must cut the same hole as the main pass, or the far field's coarse
// ground would stand in the depth buffer over the patch and hide it.
//
// Should a normal or motion-vector prepass ever be added, this writes the
// vertex normal - the splat layers' normal maps are not sampled here - and
// the motion vector Bevy's own prepass writes.

#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_prepass_functions,
}

// Mirrors `SplatUniforms` in `splat.wgsl` and on the Rust side, field for
// field: the uniform is read at the offsets the Rust block lays it out at.
struct SplatUniforms {
    tile_scale: f32,
    enabled: u32,
    triplanar_scale: f32,
    triplanar_sharpness: f32,
    water_y: f32,
    moisture_depth: f32,
    moisture_strength: f32,
    albedo_fade_near: f32,
    albedo_fade_far: f32,
    weight_uv_scale: f32,
    weight_uv_offset_u: f32,
    weight_uv_offset_v: f32,
    hole_min_x: f32,
    hole_min_z: f32,
    hole_max_x: f32,
    hole_max_z: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(106) var<uniform> splat_uniforms: SplatUniforms;

// Discard a fragment strictly inside the hole.
fn cut_hole(world_position: vec4<f32>) {
#ifdef SPLAT_HOLE
    let at = world_position.xz;
    if at.x > splat_uniforms.hole_min_x && at.x < splat_uniforms.hole_max_x
        && at.y > splat_uniforms.hole_min_z && at.y < splat_uniforms.hole_max_z {
        discard;
    }
#endif
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    cut_hole(in.world_position);
    var out: FragmentOutput;

#ifdef NORMAL_PREPASS
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
#endif

#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif

#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = pbr_prepass_functions::calculate_motion_vector(
        in.world_position,
        in.previous_world_position,
    );
#endif

    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    cut_hole(in.world_position);
}
#endif
