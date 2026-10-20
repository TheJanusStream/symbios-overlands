// Weather on the world's parts (#1686): roads, buildings, placed items and
// plants' trunks wet, gather puddles and take settled snow as the ground
// under them does (`splat.wgsl`), from the same fields (`ground_common.wgsl`)
// and the same values, so a road through a snowfield is white where the
// field is and a roof whitens with the hills.
//
// The forward fragment of Bevy's own `pbr.wgsl`, the weather laid on
// between the alpha discard and the lighting; everything a
// `StandardMaterial` draws (textures, alpha masks, unlit, the draw
// distance's dither) draws as it did. A material whose `weathers` is off -
// an avatar's, a preview's - compiles without `WEATHERED` and is that
// shader exactly. The prepass and the shadows keep `StandardMaterial`'s.
//
// What the weather reads is one block every weathered material shares
// (`crate::weathered`): written once a step, never per material, so a
// passing weather rebuilds no material.
//
//   * where it falls: the shelter map (#1685), the highest static shape's
//     top over each texel of the ground cover's grid - but for an instance
//     set's voxel shell, which stands up to a voxel over its own roof, so
//     a copy shelters no part. A surface standing more than
//     `SHELTER_CLEARANCE` under it - a table under a pavilion, a floor
//     under a roof - stays dry and bare. A sloped surface is allowed its
//     own rise across a texel, so a roof is not sheltered by its own
//     slope, and a wall is never sheltered at all. A texel takes its
//     highest shape's top whole: a chimney or a dormer keeps the roof in
//     its texel (a texel is half a ground cell) dry beside it.
//   * wet: darker (less for metal) and glossier, toward water's reflectance;
//     a surface facing the ground stays dry, under a roof or not;
//   * puddles on the flat, as on the ground;
//   * snow from the snow line up on surfaces facing up, which hides their
//     glow too.
//
// An unlit surface - a neon edge line, a sign's glow - takes no weather.

#import bevy_pbr::{
    pbr_types,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
    forward_io::{VertexOutput, FragmentOutput},
}

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#endif

#ifdef WEATHERED
#import "shaders/ground_common.wgsl"::{snow_settled, puddle_at, SNOW_ALBEDO, WET_DARKEN, WET_ROUGHNESS, PUDDLE_DARKEN, PUDDLE_ROUGHNESS, SNOW_ROUGHNESS, WATER_REFLECTANCE, WET_REFLECTANCE_SHARE}

// `crate::weathered::WorldWeatherUniforms`; a test holds the two in step.
struct WorldWeather {
    wetness: f32,
    snow: f32,
    snow_line: f32,
    snow_feather: f32,
    shelter_origin: vec2<f32>,
    shelter_texel: f32,
    shelter_size: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> weather: WorldWeather;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var shelter: texture_2d<f32>;

// `config::terrain::ground_cover::SHELTER_CLEARANCE`, which a test pins:
// how far under a shelter's top a surface stands dry, as the ground does.
const SHELTER_CLEARANCE: f32 = 0.5;
// A surface facing up less than this is a wall, which no shelter keeps
// dry: its texel holds its own building's roof, which the map cannot tell
// from one over it, and rain drives onto walls besides.
const SHELTER_WALL: f32 = 0.3;

// How open to the sky a surface at `p` facing up by `up` is: 1 in the open,
// off the map and on a wall, 0 well under a shelter.
fn open_sky(p: vec3<f32>, up: f32) -> f32 {
    if weather.shelter_size < 1.0 || abs(up) < SHELTER_WALL {
        return 1.0;
    }
    let g = floor((p.xz - weather.shelter_origin) / weather.shelter_texel);
    if any(g < vec2<f32>(0.0)) || any(g >= vec2<f32>(weather.shelter_size)) {
        return 1.0;
    }
    // Green: the highest top but an instance set's voxel shell, which
    // stands over the roof it encloses (`ground_cover::shelter`).
    let top = textureLoad(shelter, vec2<i32>(g), 0).g;
    // A surface at a slope rises across its texel by its gradient times
    // half the texel's diagonal: the map holds a roof's top at the
    // texel's centre, and the roof's own corner of the texel lies under
    // that by as much.
    let gradient = sqrt(max(1.0 - up * up, 0.0)) / abs(up);
    let allowed = SHELTER_CLEARANCE + gradient * weather.shelter_texel * 0.7072;
    return select(1.0, 0.0, top - p.y > allowed);
}
#endif

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;

#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    apply_decals(&pbr_input);

#ifdef WEATHERED
    // Lit surfaces only: an unlit one (a neon tube, a glowing sign) draws
    // its colour as it is, and a wet or snowed colour there would shine.
    let lit = (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u;
    if lit && (weather.wetness > 0.0 || weather.snow > 0.0) {
        let p = in.world_position.xyz;
        // The surface's own facing, not its normal map's: the side seen of
        // a two-sided part, so an awning seen from below faces down.
        let facing = pbr_input.world_normal;
        let up = facing.y;
        let open = open_sky(p, up);
        // Rain falls: what faces the ground - a ceiling, a soffit, an
        // awning's underside - is not wetted, walls are.
        let wet = weather.wetness * open * smoothstep(-0.5, -0.2, up);
        let puddle = puddle_at(p.xz, up, wet);
        let settled = snow_settled(p.xz, p.y, up, weather.snow * open, weather.snow_line, weather.snow_feather);
        let metal = pbr_input.material.metallic;

        var albedo = pbr_input.material.base_color.rgb * mix(1.0, WET_DARKEN, wet * (1.0 - metal));
        var rough = pbr_input.material.perceptual_roughness * mix(1.0, WET_ROUGHNESS, wet);
        albedo = albedo * mix(1.0, PUDDLE_DARKEN, puddle);
        rough = mix(rough, PUDDLE_ROUGHNESS, puddle);
        pbr_input.material.reflectance = mix(
            pbr_input.material.reflectance,
            vec3<f32>(WATER_REFLECTANCE),
            max(puddle, WET_REFLECTANCE_SHARE * wet),
        );
        albedo = mix(albedo, SNOW_ALBEDO, settled);
        rough = mix(rough, SNOW_ROUGHNESS, settled);
        pbr_input.material.metallic = mix(metal, 0.0, settled);
        pbr_input.material.emissive = pbr_input.material.emissive * (1.0 - settled);
        // A puddle and a snow cover lie flat: the surface's own detail goes.
        pbr_input.N = normalize(mix(pbr_input.N, facing, max(puddle, 0.7 * settled)));
        pbr_input.material.base_color = vec4<f32>(albedo, pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = rough;
    }
#endif

    var out: FragmentOutput;
    if (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        oit_draw(in.position, out.color);
        discard;
    }
#endif

    return out;
}
