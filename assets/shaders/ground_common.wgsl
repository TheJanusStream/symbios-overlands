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
