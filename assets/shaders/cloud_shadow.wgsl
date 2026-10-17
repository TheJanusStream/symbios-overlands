// The cloud deck's shadow (#1673): what the deck draws into the sun's
// shadow cascades, so a cloud overhead shades everything under it - ground,
// water, buildings, the player - and the shade drifts with the clouds.
//
// A shadow map holds one depth per texel, never a fraction of the light, so
// a cloud that takes `s` of the sun is drawn into `s` of the texels and left
// out of the rest, in a 4x4 ordered pattern the shadow filter averages back
// into an even shade. The pattern is fixed to the light's own texel grid,
// which Bevy snaps to whole texels as the camera moves, so it never crawls
// across the ground.
//
// The deck stands far above every cascade's near plane; Bevy pancakes such a
// caster onto that plane, so it shades everything below it. The cascades end
// at `shadow_reach` from the camera and nothing past their end is shaded at
// all, so the cloud's shade fades out before then - where the shade lands,
// along the sun's ray down to the ground under the camera - or the ground
// would cross a line from shade to full sun there.
//
// Runs in the shadow pass only: the deck stays out of the camera's depth
// prepass (`CloudExtension::enable_prepass`). The prepass binds a smaller
// view layout than the main pass, with `Globals` at binding 1 - see
// `wind.wgsl`, which hit it first. WebGL2-safe: arithmetic only, and the
// pancaking WebGL2 needs done by hand (`vertex`).

#import bevy_pbr::{
    prepass_io::{Vertex, VertexOutput},
    mesh_view_bindings::view,
    mesh_functions::{get_world_from_local, get_tag, mesh_position_local_to_world},
    view_transformations::position_world_to_clip,
}
#ifdef PREPASS_FRAGMENT
#import bevy_pbr::prepass_io::FragmentOutput
#endif
#import bevy_render::globals::Globals
#import "shaders/cloud_common.wgsl"::{CloudUniforms, cloud_mass}

@group(0) @binding(1) var<uniform> globals: Globals;
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> cloud: CloudUniforms;

// The field's depth here: two warp octaves and three of the field, the rest
// held at their mean (`cloud_common::fbm`).
const WARP_OCTAVES: i32 = 2;
const OCTAVES: i32 = 3;
// A cloud's shade gathers across the middle of its soft edge: from where
// it holds this little of its mass to where it holds this much. The deck's
// edges are feathered across a quarter of a noise unit and more - a band
// 50-100 m wide - which, cast as it stands, dims the ground so gradually
// that nobody reads a cloud's shadow in it (measured on the #1673 sheets).
// `clouds::casts_shadow` knows where this edge begins.
const SHADE_EDGE_FROM: f32 = 0.25;
const SHADE_EDGE_TO: f32 = 0.75;
// The shade fades out between these shares of `shadow_reach`. The ground
// under the camera stands in for the ground the shade lands on, which a
// hill can lift tens of metres; the margin keeps a hill's shade inside the
// cascades all the same.
const FADE_FROM: f32 = 0.35;
const FADE_TO: f32 = 0.8;
// A sun this low (the sine of its elevation) casts no cloud shadow, and
// full shadow from this high: a low sun's shade lands kilometres from its
// cloud, past the deck's far side.
const LOW_SUN_FROM: f32 = 0.08;
const LOW_SUN_TO: f32 = 0.2;

// Bevy's own prepass vertex shader for the deck, but for one line. Where a
// device cannot leave depth unclipped (WebGL2, and wherever
// `DEPTH_CLIP_CONTROL` is missing), Bevy pancakes by pulling each vertex in
// front of the near plane back onto it and writing the true depth per
// fragment. That pulls in the near side only, and each of the deck's two
// triangles is kilometres across, spanning the cascade's whole depth: with
// one corner pulled onto the near plane and another left past the far one,
// the rasteriser clipped all but a sliver of the triangle away, and the deck
// cast nothing (measured natively under `WGPU_SETTINGS_PRIO=webgpu`, which
// takes the same path). Holding every corner inside the depth range keeps
// the triangle whole; the fragment still writes its true depth, which the
// depth range then clamps as unclipped depth would.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    // The cascade is orthographic: w is 1.
    out.position.z = clamp(out.position.z, 0.0, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

// The 2x2 ordered-dither rank of a cell, each coordinate 0 or 1: 0 2 / 3 1.
fn bayer2(c: vec2<u32>) -> u32 {
    return ((c.x ^ c.y) << 1u) | c.y;
}

// The keep threshold, in (0, 1), of a whole-texel index: the 4x4 Bayer
// matrix, repeating exactly every 4 texels. Each 2x2 block holds four
// thresholds a quarter apart, so the shadow filter, about five texels wide,
// averages any share kept into an even shade.
fn pattern(texel: vec2<f32>) -> f32 {
    let c = vec2<u32>(texel - 4.0 * floor(texel / 4.0));
    let rank = 4u * bayer2(c & vec2<u32>(1u)) + bayer2(c >> vec2<u32>(1u));
    return (f32(rank) + 0.5) / 16.0;
}

// Whether this texel of the deck stands in the sun's way. Everything that
// can rule a texel out without the cloud field is weighed first: the field
// is most of the cost, and it runs for every texel of every cascade.
fn casts(in: VertexOutput) -> bool {
    let strength = cloud.shadow_strength * clamp(cloud.density, 0.0, 1.0);
    if strength <= 0.0 {
        return false;
    }
    let p = in.world_position.xyz;
    // The cascade's own axes: the light's, its z toward the sun.
    let light_x = view.world_from_view[0].xyz;
    let light_y = view.world_from_view[1].xyz;
    let to_sun = normalize(view.world_from_view[2].xyz);
    let high = smoothstep(LOW_SUN_FROM, LOW_SUN_TO, to_sun.y);
    if high <= 0.0 {
        return false;
    }
    // The deck is centred on the camera (`track_cloud_layer_to_camera`),
    // which tags it with the ground's height under the camera.
    let camera_xz = get_world_from_local(in.instance_index)[3].xz;
    let ground = bitcast<f32>(get_tag(in.instance_index));
    let lands = p.xz - to_sun.xz * ((p.y - ground) / to_sun.y);
    let reach = max(cloud.shadow_reach, 1.0);
    let near = 1.0 - smoothstep(FADE_FROM * reach, FADE_TO * reach, distance(lands, camera_xz));
    // The texel's index in the light's grid: the cascade is an orthographic
    // box `2 / clip_from_view[0][0]` metres across, `viewport.z` texels.
    let texel = 2.0 / (view.clip_from_view[0][0] * view.viewport.z);
    let index = floor(vec2<f32>(dot(p, light_x), dot(p, light_y)) / texel);
    let threshold = pattern(index);
    // Not even the thickest cloud would keep this texel.
    let most = strength * near * high;
    if most <= threshold {
        return false;
    }
    let mass = cloud_mass(p.xz, cloud, globals.time, WARP_OCTAVES, OCTAVES);
    return smoothstep(SHADE_EDGE_FROM, SHADE_EDGE_TO, mass) * most > threshold;
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    if !casts(in) {
        discard;
    }
    var out: FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    if !casts(in) {
        discard;
    }
}
#endif
