// The sky's colour toward a direction (#1671, #1672), shared by the sky
// itself (`sky.wgsl`) and everything that reflects it (`water.wgsl`), so a
// pond shows the sky that is over it.
//
// `sky_toward` takes the sky's own colour toward the direction - its
// `body`: the room's sky colour, plus whatever shines there - and hazes it
// toward its horizon: what Bevy's own distance fog makes of that body at
// `look.w` metres, where the old sky cuboid's walls stood, so a room keeps
// the horizon its fog gave it and the far terrain fades into it as before.
// The haze's optical depth is `look.x` straight up and grows as `1 / sin`
// of the elevation (the air mass); at and below the horizon it is that
// horizon whole. The fog's sun glow, which the old sky carried at every
// height, is added back above it.
//
// A plain mix toward that horizon, not the fog's formula along the whole
// path: a room's fog extinguishes faster than it scatters back (its
// extinction and inscattering colours differ), which darkens a hill at
// middle distances - and drew a dark band above the horizon when the sky
// took the same road.
//
// A camera without distance fog - none sees the sky today - gets the body
// unhazed: Bevy binds the fog only under `DISTANCE_FOG`. WebGL2-safe:
// uniforms and arithmetic only.

#import bevy_pbr::{
    mesh_view_bindings::{view, lights},
    mesh_view_types,
}
#ifdef DISTANCE_FOG
#import bevy_pbr::{
    mesh_view_bindings::fog,
    fog::{linear_fog, exponential_fog, exponential_squared_fog, atmospheric_fog},
}
#endif

// The air mass just above the horizon: `1 / sin` capped where even a
// 10 km visibility's haze is 99 % through.
const MAX_AIR_MASS: f32 = 1000.0;

// What the room's distance fog makes of `body` at `distance` metres, with
// the sun glow `scattering` - Bevy's own fog, mode by mode.
fn fogged(body: vec3<f32>, distance: f32, scattering: vec3<f32>) -> vec3<f32> {
#ifdef DISTANCE_FOG
    let c = vec4<f32>(body, 1.0);
    if fog.mode == mesh_view_types::FOG_MODE_LINEAR {
        return linear_fog(fog, c, distance, scattering).rgb;
    } else if fog.mode == mesh_view_types::FOG_MODE_EXPONENTIAL {
        return exponential_fog(fog, c, distance, scattering).rgb;
    } else if fog.mode == mesh_view_types::FOG_MODE_EXPONENTIAL_SQUARED {
        return exponential_squared_fog(fog, c, distance, scattering).rgb;
    } else if fog.mode == mesh_view_types::FOG_MODE_ATMOSPHERIC {
        return atmospheric_fog(fog, c, distance, scattering).rgb;
    }
#endif
    return body;
}

// Bevy's fog sun glow toward `dir`, without its shadow-map lookup: nothing
// stands in the sky's haze, and a roof over the camera must not put out
// the glow. Floored so an exponent of 0 - a glow over the whole sky, which
// the editor allows - is never `pow(0, 0)`.
fn sun_scattering(dir: vec3<f32>) -> vec3<f32> {
    var scattering = vec3<f32>(0.0);
#ifdef DISTANCE_FOG
    if lights.n_directional_lights > 0u {
        let light = lights.directional_lights[0];
        // The light's colour already carries its illuminance; times the
        // exposure it is what Bevy's fog scatters toward the eye.
        scattering = pow(
            max(dot(dir, light.direction_to_light), 1.0e-4),
            max(fog.directional_light_exponent, 1.0e-3),
        ) * light.color.rgb * view.exposure;
    }
#endif
    return scattering;
}

// The sky toward the unit direction `dir`, from its `body` there and the
// sky's `look` uniform (x the haze's optical depth straight up, w the
// horizon's distance in metres).
fn sky_toward(dir: vec3<f32>, body: vec3<f32>, look: vec4<f32>) -> vec3<f32> {
    let scattering = sun_scattering(dir);
    // The sun's glow as the old sky had it at every height: what the fog
    // adds toward the sun at the horizon's distance. The horizon is fogged
    // from the body without it, so the glow there is the fog's own,
    // counted once whether or not the fog is saturated at that distance.
    let glow = fogged(vec3<f32>(0.0), look.w, scattering)
        - fogged(vec3<f32>(0.0), look.w, vec3<f32>(0.0));
    let horizon = fogged(body, look.w, scattering);
    let up = dir.y;
    let air_mass = 1.0 / max(up, 1.0 / MAX_AIR_MASS);
    let haze = select(1.0 - exp(-look.x * air_mass), 1.0, up <= 0.0);
    return mix(body + glow, horizon, haze);
}
