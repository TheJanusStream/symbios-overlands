// The cloud deck's shape (#1673), shared by the deck itself (`cloud.wgsl`)
// and the shadow it casts (`cloud_shadow.wgsl`): the shade on the ground is
// the cloud overhead because both read this one field.
//
// The field is domain-warped FBM over the deck's world XZ, scrolled along
// the wind and cut at the room's threshold, its edge feathered by the
// softness. The shadow reads it with fewer octaves - the shadow map's
// filtering blurs the fine ones away - and the octaves it skips are filled
// with their mean, so it keeps the full field's shape and contrast and loses
// only the detail (the cheap field correlates with the full one at 0.99,
// measured over 200,000 points). That keeps the shadow pass cheap: it runs
// for every texel of every cascade.
//
// WebGL2-safe: arithmetic only.

// The uniform block both shaders bind at `@binding(100)`; mirrors
// `crate::clouds::CloudUniforms` (`the_wgsl_block_mirrors_the_rust_one`).
struct CloudUniforms {
    // The sunlit cloud colour, linear RGB; w unused.
    color: vec4<f32>,
    // The colour of a cloud's thick, shaded base, linear RGB; w unused.
    shadow_color: vec4<f32>,
    // The sky the deck hazes into: `sky_common::sky_toward`'s zenith and
    // look, as the sky shader is given them.
    sky_zenith: vec4<f32>,
    sky_look: vec4<f32>,
    // 2D drift direction in world XZ (need not be unit length).
    wind_dir: vec2<f32>,
    // The field value a cloud's edge is centred on: the room's cover,
    // turned on the CPU into the share of the sky the clouds cover.
    threshold: f32,
    // Opacity of the clouds that survive the threshold.
    density: f32,
    // Edge-softness band around the threshold.
    softness: f32,
    // Drift speed (m/s) along `wind_dir`.
    speed: f32,
    // World metres per noise unit: the clouds' feature size.
    scale: f32,
    // The share of the direct sun the thickest cloud takes off the ground;
    // 0 casts no shadow.
    shadow_strength: f32,
    // How far from the camera the sun's shadow cascades reach, metres.
    shadow_reach: f32,
};

// The octaves of the full field. `fbm` with fewer fills the rest with their
// mean.
const FULL_OCTAVES: i32 = 5;
// The weight of all `FULL_OCTAVES`: 1/2 + 1/4 + ... + 1/32.
const FULL_WEIGHT: f32 = 0.96875;

// David Hoskins' sine-free hash: well-conditioned at the large integer
// coordinates a deck kilometres wide reaches at `world_xz / scale`.
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

// FBM, its first `octaves` of `FULL_OCTAVES` sampled and the rest held at
// their mean of one half, so fewer octaves smooth the field without moving
// its mean or its spread. Each octave is rotated about 30 degrees against
// the last so they never lock onto a common grid. In `[0, 1]`.
fn fbm(p_in: vec2<f32>, octaves: i32) -> f32 {
    var p = p_in;
    var amp = 0.5;
    var sum = 0.0;
    var weight = 0.0;
    let rot = mat2x2<f32>(0.866025, -0.5, 0.5, 0.866025);
    for (var i = 0; i < min(octaves, FULL_OCTAVES); i = i + 1) {
        sum = sum + amp * value_noise(p);
        weight = weight + amp;
        p = rot * (p * 2.03) + vec2<f32>(13.7, 7.3);
        amp = amp * 0.5;
    }
    return (sum + 0.5 * (FULL_WEIGHT - weight)) / FULL_WEIGHT;
}

// How much cloud there is over a point of the deck at world XZ, at `time`:
// 0 clear sky, 1 the full cloud, before the room's density. The field is
// domain-warped by a low-frequency FBM (`warp_octaves` deep) so outlines
// bulge rather than forming plain puffballs; it is itself `octaves` deep.
fn cloud_mass(
    world_xz: vec2<f32>,
    u: CloudUniforms,
    time: f32,
    warp_octaves: i32,
    octaves: i32,
) -> f32 {
    let scale = max(u.scale, 1.0e-3);
    // An epsilon-padded copy, so a zeroed wind can never normalise to NaN.
    let wind = normalize(u.wind_dir + vec2<f32>(1.0e-4, 0.0));
    let uv = world_xz / scale + wind * (u.speed * time / scale);
    let warp = vec2<f32>(
        fbm(uv * 0.6, warp_octaves),
        fbm(uv * 0.6 + vec2<f32>(31.0, 17.0), warp_octaves),
    );
    let n = fbm(uv + warp * 0.7, octaves);
    let soft = max(u.softness, 1.0e-3);
    return smoothstep(u.threshold - soft, u.threshold + soft, n);
}
