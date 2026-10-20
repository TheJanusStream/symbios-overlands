// What falls (#1678): rain and snow as one volume round the camera.
//
// Every quad of the volume's mesh is a streak of rain or a flake of snow,
// placed here from the time alone:
//   1. It starts at its own point of the volume (`POSITION`, 0 to 1 across,
//      up and along) and falls from there at its own speed, drifting on the
//      room's wind - in world space, so it stands still as the camera moves.
//   2. Wrapped into the volume round the camera, it is always in view, and
//      fades out toward the volume's sides and close to the eye.
//   3. A streak turns about its fall to face the eye, as long as the fall a
//      camera's shutter would blur; a flake faces the eye, and sways. Either
//      is drawn at least a pixel across, fainter by as much as it was
//      widened, so neither flickers in and out as it falls.
// A quad ranked past the weather's amount is folded away outside the clip
// volume - heavier rain is more streaks, not brighter ones - and those
// ranked just under it fade, so a step of the amount brings no crowd of
// them in at once. An orthographic view (#1603) folds every quad away: its
// eye stands far off, and the volume round it would be a square of rain in
// the middle of the frame. Under a roof a quad folds away too (#1685): below
// the top of the highest static shape over where it falls.
//
// WebGL2-safe: two small uniform blocks and one texture, read texel by texel
// here in the vertex shader. `globals.time` wraps every hour, when every
// quad jumps at once - in falling rain, unseen.

#import bevy_pbr::{
    mesh_view_bindings::{view, globals, lights},
    view_transformations::position_world_to_clip,
}
#import "shaders/storm_common.wgsl"::storm_gust

struct PrecipitationUniforms {
    // x: how hard it rains, y: how hard it snows (0 to 1); z, w: the wind
    // (m/s) along world x and z.
    fall: vec4<f32>,
    // The sky's light on what falls, linear RGB as the sky is drawn; w how
    // stormy (#1684).
    light: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> weather: PrecipitationUniforms;

// The shelter from what falls (#1685): its red, the top of the highest
// static shape over each texel of the ground cover's grid, minus infinity
// under the open sky (`ground_cover::shelter`; green is the world's parts',
// #1686). `shelter_frame`: xy the grid's low corner in
// world XZ, z a texel's side (m), w its texels a side - 0 before it is drawn.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var shelter: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> shelter_frame: vec4<f32>;

// Whether `p` lies under something: below the highest top over its texel.
fn sheltered(p: vec3<f32>) -> bool {
    let size = shelter_frame.w;
    if size < 1.0 {
        return false;
    }
    let texel = floor((p.xz - shelter_frame.xy) / shelter_frame.z);
    if any(texel < vec2<f32>(0.0)) || any(texel >= vec2<f32>(size)) {
        return false;
    }
    return p.y < textureLoad(shelter, vec2<i32>(texel), 0).r;
}

// The volume round the camera (m): across, and top to bottom - its middle
// this far under the eye, where the ground is.
const BOX_ACROSS: f32 = 26.0;
const BOX_HEIGHT: f32 = 16.0;
const BOX_DROP: f32 = 3.0;
// Fading out (m): over this much inside the volume's sides, top and
// bottom, and from the first distance from the eye to the second.
const EDGE_FADE: f32 = 4.0;
const NEAR_FADE: vec2<f32> = vec2<f32>(0.5, 2.0);
// The share of the quads, ranked just under the amount, that fade in.
const RANK_FADE: f32 = 0.03;

// Rain: its fall (m/s), the share of the wind it drifts on, the time a
// streak's length is the fall of (s), its width (m) and how opaque it is.
const RAIN_SPEED: f32 = 9.0;
const RAIN_DRIFT: f32 = 0.6;
const RAIN_STREAK_SECONDS: f32 = 0.035;
const RAIN_WIDTH: f32 = 0.012;
const RAIN_ALPHA: f32 = 0.45;
// Snow: its fall, drift, sway (m), flake size (m) and how opaque.
const SNOW_SPEED: f32 = 1.0;
const SNOW_DRIFT: f32 = 0.9;
const SNOW_SWAY: f32 = 0.35;
const FLAKE_SIZE: f32 = 0.045;
const SNOW_ALPHA: f32 = 0.9;
// In a storm's gust front (#1684): how much more a streak slants, as a
// share of its drift.
const STORM_SLANT: f32 = 1.5;
// How much of the sky's light and of the key light each gives back: rain
// a little brighter than the sky behind it, glinting faintly; snow white,
// lit on its sunny side.
const RAIN_SKY: f32 = 1.25;
const RAIN_KEY: f32 = 0.015;
const SNOW_SKY: f32 = 0.95;
const SNOW_KEY: f32 = 0.15;

struct Vertex {
    // Where the quad starts in the volume, 0 to 1 across, up and along.
    @location(0) start: vec3<f32>,
    // The corner, (0, 0) to (1, 1).
    @location(2) corner: vec2<f32>,
    // x its rank, y its size, z its speed, w its phase, each 0 to 1.
    @location(5) data: vec4<f32>,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    // x: 1 a flake, 0 a streak; y: how opaque.
    @location(1) look: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    out.corner = v.corner;
    out.look = vec4<f32>(0.0);
    let rain = weather.fall.x;
    let snow = weather.fall.y;
    let amount = max(rain, snow);
    // An orthographic projection's last column is (.., 1).
    let orthographic = view.clip_from_view[3].w == 1.0;
    if v.data.x >= amount || orthographic {
        // Folded away, past the clip volume's depth.
        out.position = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        return out;
    }

    // A flake or a streak, in the weather's own proportions.
    let snow_share = snow / max(rain + snow, 1.0e-4);
    let flake = select(0.0, 1.0, fract(v.data.y * 3.71 + v.data.z * 5.33) < snow_share);

    // Fallen and drifted since the clock began, swaying if a flake.
    let t = globals.time;
    let speed = mix(RAIN_SPEED, SNOW_SPEED, flake) * (0.8 + 0.4 * v.data.z);
    let drift = weather.fall.zw * mix(RAIN_DRIFT, SNOW_DRIFT, flake);
    let velocity = vec3<f32>(drift.x, -speed, drift.y);
    let size = vec3<f32>(BOX_ACROSS, BOX_HEIGHT, BOX_ACROSS);
    var p = v.start * size + velocity * t;
    let phase = v.data.w * 6.2831853;
    p.x += flake * SNOW_SWAY * sin(t * 0.8 + phase);
    p.z += flake * SNOW_SWAY * sin(t * 0.6 + phase * 1.7 + 1.3);

    // Wrapped into the volume round the camera.
    let eye = view.world_position;
    let middle = eye - vec3<f32>(0.0, BOX_DROP, 0.0);
    let rel = p - middle + size * 0.5;
    let wrapped = rel - size * floor(rel / size) - size * 0.5;
    let centre = middle + wrapped;
    // Under a roof, or inside a wall (#1685): folded away, as one ranked
    // out is.
    if sheltered(centre) {
        out.position = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        return out;
    }

    let to_eye = eye - centre;
    let dist = max(length(to_eye), 1.0e-4);
    let side = 0.5 * BOX_ACROSS - max(abs(wrapped.x), abs(wrapped.z));
    let fade = smoothstep(NEAR_FADE.x, NEAR_FADE.y, dist)
        * smoothstep(0.0, EDGE_FADE, side)
        * smoothstep(0.0, EDGE_FADE, 0.5 * BOX_HEIGHT - abs(wrapped.y))
        * clamp((amount - v.data.x) / RANK_FADE, 0.0, 1.0);

    // A pixel's size there (perspective: an orthographic view never gets
    // here).
    let pixel = dist * 2.0 / (view.clip_from_view[1][1] * view.viewport.w);

    // A streak along the fall, turned about it to face the eye; looking
    // straight along the fall, the screen's own across will do. A storm's
    // gust front (#1684) slants it harder as it passes - its slant only: the
    // drop's place follows the steady fall, and a fall that changed with
    // the gust would move every drop by the change times the clock.
    let wind_dir = normalize(weather.fall.zw + vec2<f32>(1.0e-5, 0.0));
    let front = weather.light.w * storm_gust(centre.xz, wind_dir, t);
    let fall = normalize(velocity + vec3<f32>(drift.x, 0.0, drift.y) * (STORM_SLANT * front));
    var across = cross(fall, to_eye / dist);
    if length(across) < 1.0e-3 {
        across = view.world_from_view[0].xyz;
    }
    across = normalize(across);
    let streak = length(velocity) * RAIN_STREAK_SECONDS;
    let width = max(RAIN_WIDTH, pixel);
    let rain_at = centre + fall * ((v.corner.y - 0.5) * streak)
        + across * ((v.corner.x - 0.5) * width);
    let rain_alpha = RAIN_ALPHA * RAIN_WIDTH / width;

    // A flake facing the eye.
    let own = FLAKE_SIZE * (0.6 + 0.8 * v.data.y);
    let drawn = max(own, pixel * 1.5);
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    let flake_at = centre + (right * (v.corner.x - 0.5) + up * (v.corner.y - 0.5)) * drawn;
    let flake_alpha = SNOW_ALPHA * (own / drawn) * (own / drawn);

    out.position = position_world_to_clip(mix(rain_at, flake_at, flake));
    out.look = vec4<f32>(flake, mix(rain_alpha, flake_alpha, flake) * fade, 0.0, 0.0);
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let flake = in.look.x;
    let c = in.corner * 2.0 - 1.0;
    // A streak soft across and tapering to both ends; a flake a soft disc.
    let streak = (1.0 - c.x * c.x) * (1.0 - smoothstep(0.5, 1.0, abs(c.y)));
    let disc = 1.0 - smoothstep(0.25, 1.0, dot(c, c));
    let shape = mix(streak, disc, flake);

    var key = vec3<f32>(0.0);
    if lights.n_directional_lights > 0u {
        key = lights.directional_lights[0].color.rgb * view.exposure;
    }
    let colour = weather.light.rgb * mix(RAIN_SKY, SNOW_SKY, flake)
        + key * mix(RAIN_KEY, SNOW_KEY, flake);
    return vec4<f32>(colour, shape * in.look.y);
}
