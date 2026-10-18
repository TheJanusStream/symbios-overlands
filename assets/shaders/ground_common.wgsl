// What the ground and the grass on it share (#1676): the world-space tone
// fields that vary them together, imported by asset path by `splat.wgsl` (the
// ground) and `grass.wgsl` (the blades), so the grass on a lighter stretch of
// ground is drier, and the grass fading out toward its reach fades into
// ground that varies as it does - and the weather on both (#1683), and what
// the ground shows of the grass growing on it (#1695), further down.
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

// The grass on the ground (#1695): what the ground shows of the grass growing
// on it, so grassland reads as grass out to the fog, past the last blade, and
// the far blades fade into ground that shows them. The ground's shader lays it
// over the ground; the grass's far blades fade into ground laid so.
//
//   * how much of the ground the grass hides: little from straight above,
//     at a glance as much as it grows - blades standing up stand in the way
//     of a view the flatter it runs, and thin ground grows them in clumps
//     with bare ground between, which a glance sees too;
//   * what of the blades shows: their tips at a glance, more of their height
//     from higher up, the dry ones' share, drier where the ground drifts
//     lighter, as `grass.wgsl` colours a blade - and its flowers' heads, by
//     the share of the blades that flower (#1700);
//   * the wind: the blades' gust wave, and a storm's fronts, lighten it where
//     they lean the blades over, the further the more the grass sways - the
//     sheen of grass bending in the wind;
//   * its light: the ground shows it lit as the far blades are, their
//     normal leaning toward the eye as well as up (`carpet_normal`);
//   * the weather: a puddle drowns it, as it drowns the blades.

// How far a blade's normal leans to the sky from facing the eye
// (`grass.wgsl`'s `blade_point`).
const BLADE_SKYWARD: f32 = 0.6;
// How much lighter grass leaning over in a gust is, by how far it leans:
// in the room's default wind, a swell about a fifth lighter than a lull.
const CARPET_GUST: f32 = 0.2;
// The most the wind leans a blade over, as a share of
// `config::grass::WIND_LEAN` (a world's sway scales the lean, not this): its
// tip at most nine tenths of its height over (`grass.wgsl`'s bend), about
// three times `WIND_LEAN` on a blade `BLADE_HEIGHT` tall.
const LEAN_MOST: f32 = 3.0;
// The wind speed (m/s) at which the blades lean as far as the room says
// (`config::grass::WIND_LEAN` times its sway, #1700), and how much harder a
// storm's gust front leans them.
const REFERENCE_WIND: f32 = 4.0;
const STORM_LEAN: f32 = 1.5;
// How much of a field's colour its flowers take, as a share of the share of
// its blades that flower (#1700): a flower's head is a cup at its top,
// standing above the grass where a blade narrows to its tip, so a field
// shows its flowers at nearly twice their share. Measured at flower shares
// of 0.15, 0.45 and 0.9, seen from 8 to 70 degrees: the ground under a far
// field shows 0.8 to 1.2 of the tint the near blades show at the same view
// angle. Past a share of 0.56 the ground is all flower colour; at 0.9 the
// near blades read nearly all of it too, so that opens no seam - where a
// curve saturating smoothly from the same slope fell to 0.7 at mid shares.
const FLOWER_SHOWS: f32 = 1.8;

// How thickly grass grows at world `xz` on ground growing it
// `ground_density` thick (0 to 1): thin ground grows it in clumps a metre
// or two across, bare between, not as stalks spaced evenly over it - up to
// 2.2 times its own density in a clump, by 0.85 of how thin the ground is
// times the world's `clumpiness` (#1702: 0 none, 1 unset), at most all of
// it (`config::grass::CLUMP_PEAK` and `CLUMP_SHARE`, which the grass's tiles
// bound what grows by: a test pins these literals to them).
fn grass_density(ground_density: f32, xz: vec2<f32>, clumpiness: f32) -> f32 {
    let clump = 2.2 * smoothstep(0.25, 0.75, value_noise(xz * 0.6 + vec2<f32>(3.0, 29.0)));
    return clumped(ground_density, clump, clumpiness);
}

// Grass `ground_density` thick in a clump `clump` times as thick, in a world
// `clumpiness` clumpy.
fn clumped(ground_density: f32, clump: f32, clumpiness: f32) -> f32 {
    return ground_density * mix(1.0, clump, min(0.85 * clumpiness, 1.0) * (1.0 - ground_density));
}

// The clumps' mean: half their peak, as the noise is even about 0.5 and the
// smoothstep odd about it.
const CLUMP_MEAN: f32 = 1.1;
// The clumps' size (m), as the noise draws them at 0.6 a metre.
const CLUMP_M: f32 = 1.7;

// How thickly grass reads as growing at world `xz` (`grass_density`) seen
// from `distance` metres, the view's cosine with the ground's normal
// `facing`: its clumps while they span a few pixels, their mean once they
// would shimmer, sub-pixel - what the ground shows of the grass reads it so,
// and the far blades fade into ground read so.
fn grass_density_seen(
    ground_density: f32,
    xz: vec2<f32>,
    distance: f32,
    facing: f32,
    clumpiness: f32,
) -> f32 {
    let fine = fineness(CLUMP_M, distance, facing);
    var clump = CLUMP_MEAN;
    if fine < 1.0 {
        clump = mix(2.2 * smoothstep(0.25, 0.75, value_noise(xz * 0.6 + vec2<f32>(3.0, 29.0))), CLUMP_MEAN, fine);
    }
    return clumped(ground_density, clump, clumpiness);
}

// How much of the ground grass `density` thick (0 to 1) hides from a view
// whose cosine with the ground's normal is `facing`: at a glance its
// density, from straight above none - seen 45 degrees from straight above,
// `1 - exp(-depth)` of it: the world's grass's `carpet_depth`
// (`config::grass::CARPET_DEPTH` for grass as it grows unset, deeper for
// taller or wider blades, #1702).
fn carpet_cover(density: f32, facing: f32, depth: f32) -> f32 {
    let f = clamp(facing, 1.0e-3, 1.0);
    let slant = sqrt(1.0 - f * f) / f;
    return density * (1.0 - exp(-depth * slant));
}

// The normal grass on ground of normal `ground` shows a view coming from
// `toward` (level, unit), the ground `wet` (0 to 1): a far blade's, facing
// the eye and leaning to the sky, the ground's normal in the sky's place so
// hillsides keep their light - and the ground's own as it soaks, as wet
// grass lies down and shines as the wet ground does: leaning to the eye, a
// shining carpet would mirror the bright sky overhead in blotches.
fn carpet_normal(toward: vec3<f32>, ground: vec3<f32>, wet: f32) -> vec3<f32> {
    return normalize(mix(toward, ground, mix(BLADE_SKYWARD, 1.0, clamp(wet, 0.0, 1.0))));
}

// The colour grass shows to a view whose cosine with the ground's normal is
// `facing`, its blades from `base` at the root to `tip`, the `dry_share` of
// them dry ones going to `dry` and the `flower_share` of them flowers whose
// heads are `flower` on average, on ground drifting by `drift` and growing it
// `density` thick: a blade's colour at the share of its height in view, and
// the flowers' heads over them, in the light.
fn carpet_colour(
    base: vec3<f32>,
    tip: vec3<f32>,
    dry: vec3<f32>,
    dry_share: f32,
    flower: vec3<f32>,
    flower_share: f32,
    drift: f32,
    density: f32,
    facing: f32,
) -> vec3<f32> {
    let seen = mix(0.9, 0.5, clamp(facing, 0.0, 1.0));
    let green = mix(base, tip, pow(seen, 0.8));
    let dried = mix(base, dry, smoothstep(0.0, 0.6, seen));
    var colour = mix(green, dried, dry_share);
    colour = mix(colour, dry, 0.4 * smoothstep(0.55, 0.9, drift) * seen);
    colour = colour * mix(1.0 - 0.6 * density, 1.0, seen);
    return mix(colour, flower, min(clamp(flower_share, 0.0, 1.0) * FLOWER_SHOWS, 1.0));
}

// The grass's gust wave at world `xz` at time `t`, in a wind blowing toward
// `wind_dir` at `speed` (m/s): 0.1 to 1, 0.55 on average, swells a few tens
// of metres apart running downwind.
fn grass_gust(xz: vec2<f32>, wind_dir: vec2<f32>, speed: f32, t: f32) -> f32 {
    let wave = dot(xz, wind_dir) * 0.21 - t * (0.8 + 0.12 * min(speed, 20.0));
    return 0.55 + 0.45 * sin(wave) * sin(wave * 0.43 + 1.3);
}

// How far the wind leans the grass at a point, as a share of the room's own
// lean: by the wind's speed, its gust wave there `gust` and a storm's front
// `front` (0 to 1).
fn grass_lean(gust: f32, front: f32, speed: f32) -> f32 {
    return clamp(speed / REFERENCE_WIND, 0.0, 2.5) * gust * (1.0 + STORM_LEAN * front);
}

// The light the wind brings the grass on the ground: lighter where it leans
// over further than the wind's mean lean, darker where less, 1 on average
// outside a storm - no further over than a blade can lean - and none where
// the swells are too fine to see, `fine` being 1 there (they would
// shimmer). Grass that sways `sway` as far as it does unset (#1700) leans
// that much further at each.
fn carpet_shade(gust: f32, front: f32, speed: f32, sway: f32, fine: f32) -> f32 {
    let mean = min(sway * clamp(speed / REFERENCE_WIND, 0.0, 2.5) * 0.55, LEAN_MOST);
    let lean = min(sway * grass_lean(gust, front, speed), LEAN_MOST);
    return 1.0 + CARPET_GUST * (lean - mean) * (1.0 - fine);
}

// How fine a feature `size` metres across looks from `distance` metres off,
// the view's cosine with the ground's normal `facing`: 0 while it spans a
// few pixels on screen at the least, 1 once it spans under about one - seen
// at a glance, ground foreshortens to a sliver.
fn fineness(size: f32, distance: f32, facing: f32) -> f32 {
    let span = size * max(facing, 1.0e-3) / max(distance, 1.0e-3);
    return 1.0 - smoothstep(0.0008, 0.004, span);
}

// How fine the gust's swells look (`fineness`): they run about 30 m apart.
fn gust_fineness(distance: f32, facing: f32) -> f32 {
    return fineness(30.0, distance, facing);
}
