// What the ground and the grass on it share (#1676): the world-space tone
// fields that vary them together, imported by asset path by `splat.wgsl` (the
// ground) and `grass.wgsl` (the blades), so the grass on a lighter stretch of
// ground is drier, and the grass fading out toward its reach fades into
// ground that varies as it does.
//
// Two fields, both in [0, 1] and 0.5 on average, both value noise of the
// world position alone - no texture, the same on every peer, and tone only:
// neither follows the terrain's height.
//
//   * the drift: tone drifting over tens of metres (61 m and 23 m features).
//     The ground is up to 12% lighter and 3% warmer (red up, blue down)
//     where it drifts high, as much darker and cooler where it drifts low;
//     the grass there is drier where it is high.
//   * the mottle: patches a few metres across (6.1 m and 2.3 m features).
//     The ground is up to 5% lighter or darker patch by patch; the grass
//     is a little shorter where it is high, taller where it is low.
//
// Together about 0.81 to 1.21 of a colour per channel, 1 on average, so a
// room's ground keeps its tone overall. Near-white ground (snow, ice) takes
// less of it the brighter it is, and none from 0.9: lightened, it would send
// back more light than reaches it.
//
// Why fields and not de-repetition: measured on two seeded rooms' bare ground
// (#1676), within the 100 m before the far fade (#1320) the splat tile's
// repeat is weak - a tile-period autocorrelation of at most 0.09 - and what
// reads at play distance is sameness, every square metre of a layer one
// tone. A second, moved arrangement of each tile chosen in patches moved
// that number by 0.01, for six more texture reads a pixel, and was dropped.

// David Hoskins' sine-free hash: well conditioned at the large coordinates a
// Berlin square reaches, where a `sin`-based hash bands.
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Smooth value noise in [0, 1], one feature a unit across.
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

// The drift at world `xz`.
fn ground_drift(xz: vec2<f32>) -> f32 {
    let n = value_noise(xz / 61.0 + vec2<f32>(5.3, 1.7)) * 0.6
        + value_noise(xz / 23.0 + vec2<f32>(-3.1, 8.2)) * 0.4;
    return clamp((n - 0.5) * 2.0 + 0.5, 0.0, 1.0);
}

// The mottle at world `xz`.
fn ground_mottle(xz: vec2<f32>) -> f32 {
    let n = value_noise(xz / 6.1 + vec2<f32>(7.7, -2.9)) * 0.65
        + value_noise(xz / 2.3 + vec2<f32>(-9.4, 3.3)) * 0.35;
    return clamp((n - 0.5) * 2.0 + 0.5, 0.0, 1.0);
}

// How far the drift and the mottle move the ground's brightness, and the
// drift its warmth, each from the lowest field value to the highest.
const DRIFT_BRIGHTNESS: f32 = 0.24;
const MOTTLE_BRIGHTNESS: f32 = 0.10;
const DRIFT_WARMTH: f32 = 0.06;

// The multiplier the ground's colour takes at `drift` and `mottle`: each
// factor 1 at the fields' mean, so the mean ground keeps its colour.
fn ground_tint(drift: f32, mottle: f32) -> vec3<f32> {
    let bright = (1.0 + DRIFT_BRIGHTNESS * (drift - 0.5))
        * (1.0 + MOTTLE_BRIGHTNESS * (mottle - 0.5));
    let warm = DRIFT_WARMTH * (drift - 0.5);
    return bright * vec3<f32>(1.0 + warm, 1.0, 1.0 - warm);
}

// Ground colour `colour` as it varies at `drift` and `mottle`: tinted, the
// less the brighter it is, not at all from 0.9.
fn ground_vary(colour: vec3<f32>, drift: f32, mottle: f32) -> vec3<f32> {
    let brightest = max(colour.r, max(colour.g, colour.b));
    let tint = mix(ground_tint(drift, mottle), vec3<f32>(1.0), smoothstep(0.55, 0.9, brightest));
    return colour * tint;
}

// Weather on the ground (#1683): what the ground and the grass on it share,
// so a far blade fades into ground snowed as it is.
//
//   * the snow settles from the top of the ground down: covered where the
//     ground stands over the snow line, which falls as the snow settles
//     (`terrain::splat::ground_weather` sets it), patchy at its edge, and
//     on all but steep ground.
//   * puddles gather where the ground is flat and lies low in a field of
//     its own, the more of it the wetter the ground.

// The share of its colour wet ground keeps.
const WET_DARKEN: f32 = 0.72;
// Settled snow's colour: a little blue, a little under white, so it keeps
// its shading in sunlight.
const SNOW_ALBEDO: vec3<f32> = vec3<f32>(0.86, 0.88, 0.92);
// The snow line's edge: patches of this size (m) move it up or down, by at
// most half this many of its feathers (a jitter of -0.5 to 0.5 times it).
const SNOW_PATCH_M: f32 = 9.0;
const SNOW_PATCHINESS: f32 = 1.2;
// Snow holds on ground facing up more than the first (its upward normal)
// and wholly from the second.
const SNOW_STEEP: f32 = 0.55;
const SNOW_FLAT: f32 = 0.8;

// How much snow lies at world `xz`, height `y`, on ground whose upward
// normal is `up`, under snow `snow` (0 to 1) with its line at `line` and
// its edge `feather` metres either side.
fn snow_settled(xz: vec2<f32>, y: f32, up: f32, snow: f32, line: f32, feather: f32) -> f32 {
    let jitter = value_noise(xz / SNOW_PATCH_M + vec2<f32>(13.1, -4.7)) - 0.5;
    let level = (y - line) / max(feather, 0.01) + jitter * SNOW_PATCHINESS;
    return snow * smoothstep(-0.5, 0.5, level) * smoothstep(SNOW_STEEP, SNOW_FLAT, up);
}

// Puddles: on ground facing up more than the first and wholly flat from the
// second; in patches of this size (m), at most this share of the ground,
// with an edge this soft.
const PUDDLE_FLAT: vec2<f32> = vec2<f32>(0.97, 0.995);
const PUDDLE_M: f32 = 3.3;
const PUDDLE_SHARE: f32 = 0.3;
const PUDDLE_EDGE: f32 = 0.06;

// How much of a puddle stands at world `xz` on ground whose upward normal
// is `up`, the ground `wet` (0 to 1).
fn puddle_at(xz: vec2<f32>, up: f32, wet: f32) -> f32 {
    let low = value_noise(xz / PUDDLE_M + vec2<f32>(-21.3, 8.9));
    let reach = wet * PUDDLE_SHARE;
    return (1.0 - smoothstep(reach - PUDDLE_EDGE, reach, low))
        * smoothstep(PUDDLE_FLAT.x, PUDDLE_FLAT.y, up) * step(0.001, wet);
}
