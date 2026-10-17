// Fragment shader for the sky (#1671).
//
// What this does, per fragment of the sky cuboid that holds the camera:
//   1. Takes the view ray - the cuboid's shape does not matter, only the
//      direction from the camera through the fragment (under an
//      orthographic camera, where every ray runs the same way, the view's
//      forward).
//   2. Starts from the room's sky colour overhead (`sky.zenith`), and adds
//      the sun's disc and halo - or the moon's, by `sky.look.y` - and the
//      stars, by `sky.look.z`, all above the horizon only. The disc and
//      halo are lit by the scene's own directional light, so they follow
//      the record's sun colour, illuminance and position without a uniform
//      of their own.
//   3. Hazes the result toward its horizon by the air mass, with the
//      fog's sun glow - `sky_common::sky_toward`, which the water's
//      reflection shares (#1672) and whose header says how.
//
// WebGL2-safe: uniforms and arithmetic only, no textures, no storage. The
// one derivative is taken first, in uniform control flow, as WGSL (and
// Chrome's compiler) require.

#import bevy_pbr::{
    mesh_view_bindings::{view, lights, globals},
    forward_io::{Vertex, VertexOutput},
    mesh_functions::{get_world_from_local, mesh_position_local_to_world},
    view_transformations::position_world_to_clip,
}
#import "shaders/sky_common.wgsl"::sky_toward

struct SkyUniforms {
    // The sky overhead, linear RGB; w a lightning flash's light (#1684),
    // which the clouds glow with.
    zenith: vec4<f32>,
    // x: the haze's optical depth straight up; y: the moon's share of the
    // disc (0 the sun, 1 the moon); z: the stars' brightness (0 none, 1 a
    // clear night); w: the horizon's distance, metres.
    look: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: SkyUniforms;

// The sun's disc, radians of angular radius - about twice the real sun's,
// so it reads at play resolution.
const SUN_RADIUS: f32 = 0.0090;
// The disc's brightness against the light's own (illuminance times
// exposure): high enough that bloom makes it glow.
const SUN_GAIN: f32 = 4.0;
// The aureole: a tight glow around the disc, falling off by e every
// SUN_HALO_WIDTH radians.
const SUN_HALO_GAIN: f32 = 0.25;
const SUN_HALO_WIDTH: f32 = 0.035;
// The moon's disc - larger than the sun's, as the eye reads it - its
// brightness against the light's, high enough to glow through thin cloud,
// and its own wider, fainter glow.
const MOON_RADIUS: f32 = 0.0120;
const MOON_GAIN: f32 = 3.5;
const MOON_HALO_GAIN: f32 = 0.15;
const MOON_HALO_WIDTH: f32 = 0.08;
// Moonlight is white whatever the key light's tint; the disc takes the
// light's brightness and this colour.
const MOON_TINT: vec3<f32> = vec3<f32>(0.86, 0.90, 1.0);
// Stars: cells of a grid laid over the sky sphere, about a third of a
// degree each; this share of them hold a star.
const STAR_GRID: f32 = 160.0;
const STAR_SHARE: f32 = 0.08;
// The brightest star's radiance; most are far dimmer.
const STAR_GAIN: f32 = 1.2;

// Dave Hoskins' sine-free hash, as the cloud and water shaders use.
fn hash33(p_in: vec3<f32>) -> vec3<f32> {
    var p3 = fract(p_in * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yxz + 33.33);
    return fract((p3.xxy + p3.yxx) * p3.zyx);
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// One star per occupied grid cell, its centre pushed onto the sphere so
// whether it shows does not depend on the resolution. A centre the push
// carries out of its cell, or too near the cell's edge, is dropped: only
// its own cell draws a star, so it would show cut in half. `px` is the
// angle a pixel spans: a star is drawn about a pixel wide at any
// resolution.
fn stars(dir: vec3<f32>, px: f32) -> vec3<f32> {
    let p = dir * STAR_GRID;
    let cell = floor(p);
    let h = hash33(cell);
    let centre = normalize(cell + 0.3 + 0.4 * h) * STAR_GRID;
    let inside = all(floor(centre) == cell) && all(abs(fract(centre) - 0.5) < vec3<f32>(0.3));
    let d = length(p - centre);
    let size = max(px * STAR_GRID * 0.8, 0.05);
    let present = step(h.x, STAR_SHARE) * select(0.0, 1.0, inside);
    // A steep power: a few bright stars over many faint ones.
    let bright = pow(fract(h.y * 7.31 + h.z * 3.17), 10.0);
    let twinkle = 0.75 + 0.25 * sin(globals.time * (1.3 + 2.7 * h.z) + 40.0 * h.y);
    let tint = mix(vec3<f32>(0.78, 0.85, 1.0), vec3<f32>(1.0, 0.90, 0.76), h.z);
    return tint * (present * bright * twinkle * STAR_GAIN * exp(-(d * d) / (size * size)));
}

// The cuboid's vertices as Bevy's own mesh shader places them, but at the
// far plane: the sky is the backdrop, infinitely far, while its faces stand
// 2 km out. At their own depth they hid everything past them - the cloud
// deck reaches kilometres further (#1673). Reverse-Z puts the far plane at
// depth 0, which the depth buffer is cleared to, so the sky draws only where
// nothing else does, whichever is drawn first; Bevy's own skybox does the
// same. Only `world_position` reaches the fragment shader.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = get_world_from_local(vertex.instance_index);
    out.world_position = mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.position.z = 0.0;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

// The disc's mask: 1 inside the angular radius, 0 outside, anti-aliased
// over a pixel either side.
fn disc_mask(d: f32, radius: f32, px: f32) -> f32 {
    return 1.0 - smoothstep(radius - px, radius + px, d);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // An orthographic projection's last column is (.., 1); a perspective
    // one's (0, 0, near, 0).
    let orthographic = view.clip_from_view[3].w == 1.0;
    let dir = select(
        normalize(in.world_position.xyz - view.world_position),
        -normalize(view.world_from_view[2].xyz),
        orthographic,
    );
    // The angle one pixel spans, taken before any branch.
    let px = max(length(fwidth(dir)), 1.0e-5);

    let up = dir.y;
    // 1 above the horizon, 0 below it, over a pixel: the stars and the
    // disc set behind it.
    let above = smoothstep(-px, px, up);
    var body = sky.zenith.rgb + stars(dir, px) * (sky.look.z * above);

    if lights.n_directional_lights > 0u {
        let light = lights.directional_lights[0];
        let to_sun = light.direction_to_light;
        // The light's colour already carries its illuminance; times the
        // exposure, the discs' radiance is in step with the scene's light.
        let radiance = light.color.rgb * view.exposure;
        // Chord length: the angle from the disc's centre for small
        // angles, without `acos`'s precision loss near 1.
        let d = length(dir - to_sun);
        let moon = sky.look.y;

        let mu = sqrt(max(1.0 - (d * d) / (SUN_RADIUS * SUN_RADIUS), 0.0));
        let limb = 0.45 + 0.55 * mu;
        let sun = radiance * (SUN_GAIN * limb * disc_mask(d, SUN_RADIUS, px)
            + SUN_HALO_GAIN * exp(-d / SUN_HALO_WIDTH));

        // The moon's face: two octaves of noise for its seas, in the
        // disc's own plane so they hold still as the camera turns. The
        // plane's reference axis swaps off vertical for a moon overhead.
        let reference = select(
            vec3<f32>(0.0, 1.0, 0.0),
            vec3<f32>(1.0, 0.0, 0.0),
            abs(to_sun.y) > 0.99,
        );
        let side = normalize(cross(to_sun, reference));
        let face = vec2<f32>(dot(dir - to_sun, side), dot(dir - to_sun, cross(side, to_sun)))
            / MOON_RADIUS;
        let seas = 0.6 * value_noise(face * 1.7 + 3.1) + 0.4 * value_noise(face * 3.9 + 8.7);
        let albedo = 1.0 - 0.38 * smoothstep(0.45, 0.68, seas);
        let brightness = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
        let moon_light = MOON_TINT * brightness;
        let moon_disc = moon_light * (MOON_GAIN * albedo * disc_mask(d, MOON_RADIUS, px)
            + MOON_HALO_GAIN * exp(-d / MOON_HALO_WIDTH));

        body = body + mix(sun, moon_disc, moon) * above;
    }

    return vec4<f32>(sky_toward(dir, body, sky.look), 1.0);
}
