// What a storm's gusts share (#1684): fronts of stronger wind crossing the
// world downwind, imported by asset path by the grass (`grass.wgsl`), the
// foliage (`wind.wgsl`) and what falls (`precipitation.wgsl`), so a gust
// bends them together as it passes. The storm's steady wind is the room's
// own, raised by the storm (`weather::environment_in`); the fronts come and
// go over it. Time is each client's own clock: a gust need not reach every
// visitor at the same moment, as lightning must.

#import "shaders/ground_common.wgsl"::value_noise

// Fronts about this far apart along the wind, broken this often across it,
// travelling downwind this fast (m, m, m/s).
const GUST_SPACING: f32 = 26.0;
const GUST_BREAK: f32 = 40.0;
const GUST_SPEED: f32 = 9.0;

// How hard a gust front blows at world `xz` at time `t` (s), the wind
// blowing toward `dir` (a unit vector): 0 between the fronts, 1 in one.
fn storm_gust(xz: vec2<f32>, dir: vec2<f32>, t: f32) -> f32 {
    let along = dot(xz, dir) - t * GUST_SPEED;
    let across = dot(xz, vec2<f32>(-dir.y, dir.x));
    let n = value_noise(vec2<f32>(along / GUST_SPACING, across / GUST_BREAK) + vec2<f32>(31.7, -12.3));
    return smoothstep(0.45, 0.8, n);
}
