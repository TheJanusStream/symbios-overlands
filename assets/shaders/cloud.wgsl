// Fragment shader for the cloud deck (#1673).
//
// What this does, per fragment of the deck plane:
//   1. Reads how much cloud stands over the fragment's world XZ - the
//      shared field (`cloud_common::cloud_mass`), the same one the deck's
//      shadow reads - times the room's density: the cloud's thickness, and
//      its opacity seen from straight below. Seen at a slant the eye looks
//      through more of the layer, so a cloud near the horizon is more
//      opaque than one overhead, as a real layer is.
//   2. Lights it with the scene's own sun and sky. The sunlight reaching a
//      cloud's base has crossed the cloud on its way down, so the field is
//      read a second time a little toward the sun: where that is thin the
//      cloud is lit through and takes the room's cloud colour, where it is
//      thick the cloud is in its own shade and takes the shadow colour -
//      bright sunward edges, dark undersides, and long shading under a low
//      sun. Thin cloud toward the sun glows with the light it scatters
//      forward, the silver lining. The light's colour and strength are the
//      record's, so a warm sun warms the clouds and a night's dim key light
//      leaves them dark. From above the deck the tops are lit.
//   3. Hazes it toward the horizon exactly as the sky behind it is hazed
//      (`sky_common::sky_toward`), so clouds sink into the horizon with the
//      sky and leave no seam against it.
//   4. Fades it out near the plane's edge, which `track_cloud_layer_to_camera`
//      keeps low toward the horizon, deep in that haze.
//
// The colours arrive linear (`clouds::cloud_uniforms`). WebGL2-safe:
// uniforms and arithmetic only.

#import bevy_pbr::{
    mesh_view_bindings::{globals, view, lights},
    forward_io::{VertexOutput, FragmentOutput},
}
#import "shaders/sky_common.wgsl"::sky_toward
#import "shaders/cloud_common.wgsl"::{CloudUniforms, cloud_mass, FULL_OCTAVES}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> cloud: CloudUniforms;

// The second read toward the sun - how far, in noise units per unit of the
// sun's horizontal run over its height (capped as the sun nears the
// horizon), and how deep: its detail never shows, only whether a cloud
// stands there.
const SUNWARD_REACH: f32 = 0.18;
const SUNWARD_MAX_RUN: f32 = 3.0;
const SUNWARD_WARP_OCTAVES: i32 = 2;
const SUNWARD_OCTAVES: i32 = 3;
// The optical depth of a full cloud along the sunlight's path, by which
// thickness the base falls into its shadow colour; and the lowest sun the
// path length is taken at.
const SHADE_DEPTH: f32 = 1.0;
const MIN_SUN_HEIGHT: f32 = 0.15;
// Light a cloud gets through the thickest shade - scattered through it
// many times - as a share of the lit colour's lean, so a deck under a low
// sun stays a dim grey rather than going to its shadow colour whole.
const LIT_FLOOR: f32 = 0.2;
// Moonlight is white whatever the key light's tint, as the sky's moon disc
// is (`sky.wgsl`'s MOON_TINT): under the moon (`sky_look.y`) the clouds take
// the light's brightness in this colour.
const MOON_TINT: vec3<f32> = vec3<f32>(0.86, 0.90, 1.0);
// The share of the sun's light (its colour times illuminance times the
// exposure) a lit cloud returns per unit of the sun's height, and the share
// of the sky's light overhead every cloud takes.
const SUN_DIFFUSE: f32 = 0.20;
const SKY_DIFFUSE: f32 = 1.0;
// The tops, seen from above the deck, take the sun whole.
const TOP_DIFFUSE: f32 = 0.25;
// The silver lining: the forward scatter's strength and how tightly it
// hugs the sun.
const SILVER_GAIN: f32 = 0.35;
const SILVER_POWER: f32 = 8.0;
// The steepest view the layer's slant is taken at (sine of the elevation).
const MIN_VIEW_HEIGHT: f32 = 0.03;
// Where the fade at the plane's edge runs, as shares of its half-width.
const EDGE_FADE_FROM: f32 = 0.6;
const EDGE_FADE_TO: f32 = 0.95;

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let p = in.world_position.xyz;
    let thickness = clamp(
        cloud_mass(p.xz, cloud, globals.time, FULL_OCTAVES, FULL_OCTAVES) * cloud.density,
        0.0,
        1.0,
    );
    // A fragment with no cloud adds nothing: the blend leaves the sky.
    if thickness < 1.0e-3 {
        out.color = vec4<f32>(0.0);
        return out;
    }

    let v = normalize(p - view.world_position);
    let below = view.world_position.y < p.y;

    // Seen at a slant, the eye crosses the layer `1 / sin` times as deep.
    let slant = 1.0 / max(abs(v.y), MIN_VIEW_HEIGHT);
    let opacity = 1.0 - pow(1.0 - min(thickness, 0.999), slant);

    // The sky's light overhead, hazed as the sky shows it.
    let sky = sky_toward(vec3<f32>(0.0, 1.0, 0.0), cloud.sky_zenith.rgb, cloud.sky_look);
    var body = cloud.shadow_color.rgb * sky * SKY_DIFFUSE;

    if lights.n_directional_lights > 0u {
        let light = lights.directional_lights[0];
        let to_sun = light.direction_to_light;
        // The light's colour already carries its illuminance; times the
        // exposure it is in step with everything else the sun lights.
        let key = light.color.rgb * view.exposure;
        let moon_light = MOON_TINT * dot(key, vec3<f32>(0.2126, 0.7152, 0.0722));
        let sun = mix(key, moon_light, cloud.sky_look.y);
        // No light from a sun below the horizon.
        let up = smoothstep(-0.05, 0.05, to_sun.y);
        let height = max(to_sun.y, MIN_SUN_HEIGHT);

        if below {
            // The cloud the sunlight crossed on its way to this one's base.
            let run = min(length(to_sun.xz) / height, SUNWARD_MAX_RUN);
            let sunward_xz = p.xz + normalize(to_sun.xz + vec2<f32>(1.0e-5, 0.0))
                * (run * SUNWARD_REACH * cloud.scale);
            let sunward = clamp(
                cloud_mass(sunward_xz, cloud, globals.time, SUNWARD_WARP_OCTAVES, SUNWARD_OCTAVES)
                    * cloud.density,
                0.0,
                1.0,
            );
            let lit = mix(LIT_FLOOR, 1.0, exp(-SHADE_DEPTH * 0.5 * (thickness + sunward) / height));
            let tint = mix(cloud.shadow_color.rgb, cloud.color.rgb, lit);
            let toward = pow(max(dot(v, to_sun), 0.0), SILVER_POWER);
            body = tint * (sun * (SUN_DIFFUSE * max(to_sun.y, 0.0) * up) + sky * SKY_DIFFUSE)
                + cloud.color.rgb * sun * (SILVER_GAIN * toward * lit * (1.0 - thickness) * up);
        } else {
            body = cloud.color.rgb
                * (sun * (TOP_DIFFUSE * max(to_sun.y, 0.0) * up) + sky * SKY_DIFFUSE);
        }
    }

    // Hazed as the sky behind it is. From above, the haze is taken as for
    // the same angle upward: `sky_toward` would read any downward view as
    // the horizon whole.
    let hazed = sky_toward(vec3<f32>(v.x, abs(v.y), v.z), body, cloud.sky_look);

    // The plane's edge: its UVs run 0 to 1 across it.
    let edge = length(in.uv - vec2<f32>(0.5)) * 2.0;
    let fade = 1.0 - smoothstep(EDGE_FADE_FROM, EDGE_FADE_TO, edge);

    // Straight (non-premultiplied) alpha: `AlphaMode::Blend`.
    out.color = vec4<f32>(hazed, opacity * fade);
    return out;
}
