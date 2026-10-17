//! A room's time of day (#1677): its light at the hour it stands at,
//! whether the owner sets that hour or it turns with the clock.
//!
//! A room with no [`TimeOfDay`] is drawn exactly as its [`Environment`]
//! says, as every room was before this. One with a time of day takes those
//! fields as the room at noon and draws the hour from them:
//!
//! * **The sun's arc.** At noon the sun stands where the room's
//!   `sun_position` puts it. Through the day it turns about the sky's pole -
//!   the axis square to the noon sun, tilted from the zenith away from it -
//!   fifteen degrees an hour, as the sun does at an equinox: it rises at six
//!   a quarter turn round the horizon from noon's bearing, sets at eighteen
//!   opposite, and stands as far under the horizon at midnight as it stood
//!   over it at noon.
//! * **Its light.** A lower sun's light crosses more air (the air mass), so
//!   it is dimmer and redder: blue scatters most ([`EXTINCTION`]). Noon's
//!   colour and level are the room's own, and every other hour's are noon's
//!   times the air's change. Below the horizon the sun's light fades out
//!   and then, opposite it, the moon's fades in: by night the key light is
//!   the moon, at a small share of the noon sun ([`MOON_SHARE`]), the
//!   adaptation of an eye to the night. The sky draws the key light's disc
//!   as the moon's or the sun's as [`AtHour::moon`] says: left to itself it
//!   reads any dim light under a dark sky as the moon (`crate::sky`), a low
//!   red sun among them.
//! * **The sky.** Overhead and at the horizon it darkens as the sun sinks
//!   and goes to night's deep blue, dark enough for the stars; the horizon
//!   takes a warm glow round sunrise and sunset; and the ambient light
//!   falls to a quarter of a high noon's. The fog's glow toward the key
//!   light stays the room's own: Bevy draws it in the light's colour and at
//!   its level, so it reddens and dims with the sun and is the moon's by
//!   night.
//!
//! A turning day reads its hour off UTC ([`hour_at`]): every visitor's
//! clock gives every other visitor's hour, as nearly as their clocks agree,
//! with no message between them. `crate::drawn_environment` draws the room
//! at its hour (and its weather over that) anew whenever the hour moves a
//! minute of its day ([`HOUR_STEP`]) - a quarter degree of the sun's arc,
//! each drawing a rebuild of the sky light's maps.
//!
//! [`EXTINCTION`]: crate::config::lighting::time_of_day::EXTINCTION
//! [`HOUR_STEP`]: crate::config::lighting::time_of_day::HOUR_STEP
//! [`MOON_SHARE`]: crate::config::lighting::time_of_day::MOON_SHARE

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::config::lighting::time_of_day as cfg;
use crate::pds::{Environment, Fp, Fp3, Fp4, TimeOfDay};

/// The hour (0 to 24) room time `time` stands at `unix_secs` seconds after
/// 1970 began, UTC: its own hour if it stands still, and if its day turns,
/// that hour advanced by the share of a day the clock has run since the
/// last whole day of the clock's count ended.
pub fn hour_at(time: &TimeOfDay, unix_secs: f64) -> f32 {
    let hour = f64::from(time.hour.0);
    let day_secs = f64::from(time.day_minutes.0) * 60.0;
    if day_secs.is_nan() || day_secs <= 0.0 {
        return hour.rem_euclid(24.0) as f32;
    }
    let turned = unix_secs.rem_euclid(day_secs) / day_secs * 24.0;
    (hour + turned).rem_euclid(24.0) as f32
}

/// The hour a record holds for a day of `day_minutes` that stands at
/// `hour_now` at `unix_secs`: what the editor writes when the owner sets
/// the hour it is now, or the length of the day, so the hour stays where
/// they see it. A turning day's hour is its share of every day since 1970,
/// so a new length alone would put it anywhere at all.
pub fn anchored(hour_now: f32, day_minutes: Fp, unix_secs: f64) -> Fp {
    let since_turn = hour_at(
        &TimeOfDay {
            hour: Fp(0.0),
            day_minutes,
        },
        unix_secs,
    );
    TimeOfDay::clock_hour(hour_now - since_turn)
}

/// How far apart two hours are round the clock face.
pub(crate) fn hours_apart(a: f32, b: f32) -> f32 {
    let gap = (a - b).rem_euclid(24.0);
    gap.min(24.0 - gap)
}

/// Lowest and highest a noon sun may stand (degrees): one under a degree
/// would be its own night, and one overhead has no bearing to turn from.
const NOON_ALTITUDE_DEG: (f32, f32) = (1.0, 89.9);

/// The direction toward the room's noon sun: its `sun_position`, raised or
/// lowered into [`NOON_ALTITUDE_DEG`].
fn noon_toward(env: &Environment) -> Vec3 {
    let toward = Vec3::from_array(env.sun_position.0).normalize_or(Vec3::Y);
    let across = Vec3::new(toward.x, 0.0, toward.z).normalize_or(Vec3::X);
    let altitude = toward.y.clamp(-1.0, 1.0).asin().clamp(
        NOON_ALTITUDE_DEG.0.to_radians(),
        NOON_ALTITUDE_DEG.1.to_radians(),
    );
    across * altitude.cos() + Vec3::Y * altitude.sin()
}

/// The direction toward the sun at `hour` on a day whose noon sun stands
/// toward `noon` (a unit vector above the horizon): the noon sun turned
/// about the sky's pole, fifteen degrees an hour. The pole is square to the
/// noon sun in its vertical plane, past the zenith, so the sun's path is a
/// great circle through the noon sun that meets the horizon a quarter turn
/// either side of noon's bearing.
pub fn sun_toward(noon: Vec3, hour: f32) -> Vec3 {
    let across = Vec3::new(noon.x, 0.0, noon.z).normalize_or(Vec3::X);
    let altitude = noon.y.clamp(-1.0, 1.0).asin();
    let pole = Vec3::Y * altitude.cos() - across * altitude.sin();
    Quat::from_axis_angle(pole, (hour - 12.0) / 24.0 * TAU) * noon
}

/// The air a ray of sunlight crosses at `altitude_deg`, as a share of the
/// air straight overhead (Kasten and Young's formula); the horizon's at and
/// below it.
fn air_mass(altitude_deg: f32) -> f32 {
    let altitude = altitude_deg.max(0.0);
    1.0 / (altitude.to_radians().sin() + 0.505_72 * (altitude + 6.079_95).powf(-1.636_4))
}

/// The share of red, green and blue sunlight the air lets through at
/// `altitude_deg`.
fn transmission(altitude_deg: f32) -> Vec3 {
    let mass = air_mass(altitude_deg);
    Vec3::from_array(cfg::EXTINCTION).map(|k| (-k * mass).exp())
}

/// Luminance of a linear colour.
fn luminance(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// An sRGB colour as linear light.
fn linear(srgb: [f32; 3]) -> Vec3 {
    let c = Color::srgb(srgb[0], srgb[1], srgb[2]).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

/// Linear light as an sRGB colour, each channel within `0..=1`.
fn srgb(linear: Vec3) -> [f32; 3] {
    let c = linear.clamp(Vec3::ZERO, Vec3::ONE);
    let s = Color::linear_rgb(c.x, c.y, c.z).to_srgba();
    [s.red, s.green, s.blue]
}

/// `c` scaled to luminance `target`; black stays black.
fn at_luminance(c: Vec3, target: f32) -> Vec3 {
    let lum = luminance(c);
    if lum > 1.0e-6 {
        c * (target / lum)
    } else {
        Vec3::ZERO
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A room drawn at an hour of its day ([`environment_at`]).
pub struct AtHour {
    /// Its environment at the hour.
    pub env: Environment,
    /// 1 when the key light is the moon, 0 when it is the sun: what the sky
    /// draws its disc as. The sky reads a dim light under a dark sky as the
    /// moon when nothing tells it (`crate::sky`), and a low sun at dusk is
    /// both: told nothing, it would draw the setting sun as the moon.
    pub moon: f32,
}

/// Room environment `env` - the room at noon - drawn at `hour`: the sun
/// along its day's arc, its light dimmed and reddened by the air or the
/// moon's in its place, and the sky and the ambient light with it (see the
/// module docs). At 12 it is `env` itself, for a noon sun from a degree
/// over the horizon to a tenth of a degree short of overhead
/// ([`NOON_ALTITUDE_DEG`]); one outside that is drawn from the nearest.
pub fn environment_at(env: &Environment, hour: f32) -> AtHour {
    let length = Vec3::from_array(env.sun_position.0).length();
    let distance = if length > 0.0 { length } else { 1.0 };
    let noon = noon_toward(env);
    let sun = sun_toward(noon, hour);
    let altitude = sun.y.clamp(-1.0, 1.0).asin().to_degrees();
    let noon_altitude = noon.y.asin().to_degrees();

    // Each weight as a share of noon's, so that noon is the room as its
    // fields set it however low its noon sun stands; the glow is what the
    // hour adds to noon's.
    let sun_up = |a: f32| smoothstep(cfg::SUN_FADE_DEG.0, cfg::SUN_FADE_DEG.1, a);
    let sky_up = |a: f32| smoothstep(cfg::SKY_FADE_DEG.0, cfg::SKY_FADE_DEG.1, a);
    let sky_high = |a: f32| {
        cfg::LOW_SKY_SHARE + (1.0 - cfg::LOW_SKY_SHARE) * smoothstep(0.0, cfg::LOW_SKY_FULL_DEG, a)
    };
    let glow_at = |a: f32| {
        smoothstep(cfg::GLOW_RISE_DEG.0, cfg::GLOW_RISE_DEG.1, a)
            * (1.0 - smoothstep(cfg::GLOW_FADE_DEG.0, cfg::GLOW_FADE_DEG.1, a))
    };
    let ambient_at = |a: f32| {
        cfg::NIGHT_AMBIENT_SHARE
            + (1.0 - cfg::NIGHT_AMBIENT_SHARE)
                * smoothstep(cfg::AMBIENT_FADE_DEG.0, cfg::AMBIENT_FADE_DEG.1, a)
    };
    let day = (sun_up(altitude) / sun_up(noon_altitude)).min(1.0);
    let night = 1.0 - smoothstep(cfg::MOON_FADE_DEG.0, cfg::MOON_FADE_DEG.1, altitude);
    let sky_day = (sky_up(altitude) / sky_up(noon_altitude)).min(1.0);
    let low = (sky_high(altitude) / sky_high(noon_altitude)).min(1.0);
    let glow = (glow_at(altitude) - glow_at(noon_altitude)).max(0.0);
    let ambient = (ambient_at(altitude) / ambient_at(noon_altitude)).min(1.0);

    // The air between the sun and the room now, against noon's.
    let noon_through = transmission(noon_altitude);
    let now_through = transmission(altitude);
    let dimming = (luminance(now_through) / luminance(noon_through)).min(1.0);
    let reddening = now_through / noon_through;
    let sun_light = linear(env.sun_color.0);

    let mut out = env.clone();
    let moon = if day >= night {
        // The sun, along its true arc even just under the horizon: there it
        // lights no upward face and its disc sinks out of sight.
        let mut colour = at_luminance(sun_light * reddening, luminance(sun_light));
        let mut illuminance = env.sun_illuminance.0 * dimming * day;
        // A red low sun can take a channel past 1: keep the colour a colour
        // and carry the excess in the light's level.
        let peak = colour.max_element();
        if peak > 1.0 {
            colour /= peak;
            illuminance *= peak;
        }
        out.sun_position = Fp3((sun * distance).to_array());
        out.sun_illuminance = Fp(illuminance);
        out.sun_color = Fp3(srgb(colour));
        0.0
    } else {
        // Never brighter than the noon sun, whatever the floor.
        let noon_lux = env.sun_illuminance.0;
        let moon = (noon_lux * cfg::MOON_SHARE)
            .clamp(cfg::MOON_LUX.0, cfg::MOON_LUX.1)
            .min(noon_lux);
        out.sun_position = Fp3((-sun * distance).to_array());
        out.sun_illuminance = Fp(moon * night);
        out.sun_color = Fp3(cfg::MOON_COLOR);
        1.0
    };

    // The sky overhead and at the horizon, in linear light.
    let sky = linear(env.sky_color.0);
    let horizon = linear([env.fog_color.0[0], env.fog_color.0[1], env.fog_color.0[2]]);
    let night_blue = Vec3::from_array(cfg::NIGHT_SKY);
    // The low sun's colour, as bright as the horizon it warms.
    let low_sun = sun_light * transmission(altitude.max(1.0));
    let warm = at_luminance(low_sun, luminance(horizon));
    let horizon_now = (horizon * low).lerp(warm * low, cfg::GLOW_SHARE * glow);
    let sky_now = (sky * cfg::NIGHT_SHARE + night_blue).lerp(sky * low, sky_day);
    let horizon_now = (horizon * cfg::NIGHT_SHARE + night_blue).lerp(horizon_now, sky_day);
    out.sky_color = Fp3(srgb(sky_now));
    let [r, g, b] = srgb(horizon_now);
    out.fog_color = Fp4([r, g, b, env.fog_color.0[3]]);

    out.ambient_brightness = Fp(env.ambient_brightness.0 * ambient);
    AtHour { env: out, moon }
}

#[cfg(test)]
mod tests;
