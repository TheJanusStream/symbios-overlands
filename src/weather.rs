//! A room's weather (#1678): a grey sky, rain, a storm, snow or fog laid
//! over its light, standing still or coming and going with the clock.
//!
//! The weather now is a handful of amounts, each 0 to 1 ([`WeatherNow`]):
//! how overcast, how hard it rains or snows, how thick the fog, how stormy.
//! [`weather_now`] reads them off the record's [`Weather`] and the clock,
//! and [`environment_in`] lays them over the room's environment - after its
//! hour (`crate::time_of_day`), so a rainy night is a night that rains:
//!
//! * **Overcast.** The cloud deck closes to a whole cover, denser; the sun's
//!   direct light dims far more than the sky's light does (overcast light is
//!   dim but flat, as seeding relates a room's cover to its light); the sky,
//!   the horizon and the fog's own tints grey toward their own brightness,
//!   and the sun's glow in the fog and its glitter on the water go behind
//!   the cloud.
//! * **Rain and a storm** darken the sky, the clouds and the sky's light
//!   besides; snow brightens the sky and the clouds a little and greys them
//!   further, and settled snow sends back more of the sky's light (#1683).
//! * **Rain, snow and fog** close in the visibility, fog the most, and
//!   [`precipitation`] draws what falls.
//!
//! The ground keeps what fell (#1683): how wet it is and how far the snow
//! has settled ([`WeatherNow::wet`], [`WeatherNow::cover`]) build up with
//! the rain and the snow and, after a weather that comes and goes, dry and
//! melt over its next spells, read off the same timeline. The ground's and
//! the grass's shaders draw them (`terrain::splat`, `grass`), and the
//! water's rings its rain (`water.wgsl`). Only those: roads, buildings and
//! placed pieces stay dry and bare (#1686), and what falls knows nothing of
//! shelter, so it falls through roofs (#1685). The visibility it closes in
//! is the drawn fog's alone; the sun's shadow reach and the ground cover's
//! draw distance keep the room's own (#1688).
//!
//! A storm (#1684) blows: its wind is the room's, raised by the storm, which
//! the clouds, the grass, the foliage and the rain all follow; gust fronts
//! cross the world over it (`storm_common.wgsl`); and [`lightning`] strikes
//! on the clock, flashing the sky and the world, its thunder after it.
//!
//! A weather that comes and goes reads its spells off UTC ([`severity_at`]),
//! as a turning day reads its hour: every visitor's clock gives every other
//! visitor's weather, as nearly as their clocks agree.

pub mod lightning;
pub mod precipitation;

use bevy::prelude::*;

use crate::config::lighting::weather as cfg;
use crate::pds::{Environment, Fp, Fp3, Fp4, Weather, WeatherKind};

/// The weather now: how much of each thing it brings, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherNow {
    /// How overcast: 0 the room's own sky, 1 a whole grey cover.
    pub overcast: f32,
    /// How hard it rains.
    pub rain: f32,
    /// How hard it snows.
    pub snow: f32,
    /// How thick the weather's fog.
    pub fog: f32,
    /// How stormy (#1684 draws the gusts, lightning and thunder).
    pub storm: f32,
    /// How wet the ground is, 0 dry to 1 soaked: wetted by the rain and,
    /// after a weather that comes and goes, drying once it stops (#1683).
    pub wet: f32,
    /// How far the snow has settled, from the top of the ground down: 0
    /// none, 1 the whole ground under it; melting once it stops (#1683).
    pub cover: f32,
}

impl WeatherNow {
    /// Whether it brings nothing at all: the room as its fields set it.
    pub fn is_clear(&self) -> bool {
        *self == WeatherNow::default()
    }

    /// Whether anything falls: rain or snow.
    pub(crate) fn falls(&self) -> bool {
        self.rain > 0.0 || self.snow > 0.0
    }

    /// The most any amount differs between `self` and `other`.
    pub(crate) fn gap(&self, other: &WeatherNow) -> f32 {
        [
            self.overcast - other.overcast,
            self.rain - other.rain,
            self.snow - other.snow,
            self.fog - other.fog,
            self.storm - other.storm,
            self.wet - other.wet,
            self.cover - other.cover,
        ]
        .into_iter()
        .fold(0.0, |most, d| most.max(d.abs()))
    }

    /// The overcast times `clouds`, and what falls or hangs in the air
    /// times `falls`; the ground as it lies under that alone.
    fn times(&self, clouds: f32, falls: f32) -> WeatherNow {
        let rain = self.rain * falls;
        let snow = self.snow * falls;
        WeatherNow {
            overcast: self.overcast * clouds,
            rain,
            snow,
            fog: self.fog * falls,
            storm: self.storm * falls,
            wet: wet_under(rain),
            cover: cover_under(snow),
        }
    }
}

/// What `kind` brings at `intensity` (0 to 1), at its full.
pub fn amounts(kind: WeatherKind, intensity: f32) -> WeatherNow {
    let table = match kind {
        WeatherKind::Overcast => &cfg::OVERCAST,
        WeatherKind::Rain => &cfg::RAIN,
        WeatherKind::Storm => &cfg::STORM,
        WeatherKind::Snow => &cfg::SNOW,
        WeatherKind::Fog => &cfg::FOG,
        // A kind a newer client added: drawn as no weather.
        WeatherKind::Unknown => return WeatherNow::default(),
    };
    let a = if intensity.is_finite() {
        intensity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let at = |(none, full): (f32, f32)| none + (full - none) * a;
    let (rain, snow) = (at(table.rain), at(table.snow));
    WeatherNow {
        overcast: at(table.overcast),
        rain,
        snow,
        fog: at(table.fog),
        storm: at(table.storm),
        wet: wet_under(rain),
        cover: cover_under(snow),
    }
}

/// How wet the ground lies under rain of `rain` for long: soaked from
/// [`cfg::WET_FULL`].
fn wet_under(rain: f32) -> f32 {
    smoothstep(0.0, cfg::WET_FULL, rain)
}

/// How far snow of `snow` settles for long: all of the ground from
/// [`cfg::COVER_FULL`].
fn cover_under(snow: f32) -> f32 {
    smoothstep(0.0, cfg::COVER_FULL, snow)
}

/// The weather `weather` brings at `unix_secs` seconds after 1970 began,
/// UTC: its kind at its intensity if it stands, and if it comes and goes,
/// as far as its severity then reaches - the clouds gathering first, then
/// anything falling ([`cfg::CLOUDS_GATHER`], [`cfg::FALLS`]).
pub fn weather_now(weather: &Weather, unix_secs: f64) -> WeatherNow {
    let full = amounts(weather.kind, weather.intensity.0);
    let spell = weather.spell_minutes.0;
    if spell.is_nan() || spell <= 0.0 {
        return full;
    }
    let severity = severity_at(weather, unix_secs);
    let mut now = full.times(
        smoothstep(cfg::CLOUDS_GATHER.0, cfg::CLOUDS_GATHER.1, severity),
        smoothstep(cfg::FALLS.0, cfg::FALLS.1, severity),
    );
    // The ground keeps what fell: wet from the rain of the spells before,
    // drying since, and the snow settled, melting since - the most any
    // instant of the past leaves, as it has dried or melted by now.
    let spell_secs = f64::from(spell) * 60.0;
    for k in 1..cfg::HISTORY_SAMPLES {
        let ago_spells = f64::from(k) * cfg::HISTORY_STEP_SPELLS;
        let then = severity_at(weather, unix_secs - ago_spells * spell_secs);
        let fell = smoothstep(cfg::FALLS.0, cfg::FALLS.1, then);
        let ago = ago_spells as f32;
        now.wet = now
            .wet
            .max(wet_under(full.rain * fell) * (-ago / cfg::DRY_SPELLS).exp());
        now.cover = now
            .cover
            .max(cover_under(full.snow * fell) * (-ago / cfg::MELT_SPELLS).exp());
    }
    // The last trace of a long-gone fall is none: below this the ground
    // shows nothing, and a ground left at a ten-thousandth would keep every
    // shader's weather branch running for it.
    if now.wet < cfg::GROUND_TRACE {
        now.wet = 0.0;
    }
    if now.cover < cfg::GROUND_TRACE {
        now.cover = 0.0;
    }
    now
}

/// How stormy `weather` is at `unix_secs`, as [`weather_now`] has it but
/// without reading the ground's past: what a slot's lightning strikes by
/// (`lightning::strike_of`).
pub fn storm_at(weather: &Weather, unix_secs: f64) -> f32 {
    let full = amounts(weather.kind, weather.intensity.0).storm;
    let spell = weather.spell_minutes.0;
    if full == 0.0 || spell.is_nan() || spell <= 0.0 {
        return full;
    }
    full * smoothstep(cfg::FALLS.0, cfg::FALLS.1, severity_at(weather, unix_secs))
}

/// How far into its weather a weather that comes and goes stands at
/// `unix_secs`: 0 clear, 1 its kind at its intensity. A smooth value noise
/// over the spells counted since 1970, each spell's value drawn from its
/// count and from the weather's kind and spell length, so every visitor
/// reads one timeline and two rooms set alike share theirs. A weather that
/// stands is at 1.
pub fn severity_at(weather: &Weather, unix_secs: f64) -> f32 {
    let spell_secs = f64::from(weather.spell_minutes.0) * 60.0;
    if spell_secs.is_nan() || spell_secs <= 0.0 {
        return 1.0;
    }
    let t = unix_secs / spell_secs;
    let count = t.floor();
    let into = (t - count) as f32;
    let salt = salt(weather);
    let a = spell_value(count as i64, salt);
    let b = spell_value(count as i64 + 1, salt);
    a + (b - a) * into * into * (3.0 - 2.0 * into)
}

/// What sets a weather's timeline apart from another's: its kind and its
/// spell length, as the wire holds them.
pub(crate) fn salt(weather: &Weather) -> u64 {
    let kind = WeatherKind::ALL
        .iter()
        .position(|k| *k == weather.kind)
        .unwrap_or(WeatherKind::ALL.len()) as u64;
    let spell = (weather.spell_minutes.0 * crate::pds::types::FP_SCALE).round() as i64 as u64;
    splitmix64(kind.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ spell)
}

/// Spell `count`'s value, 0 to 1.
fn spell_value(count: i64, salt: u64) -> f32 {
    (splitmix64(count as u64 ^ salt) >> 40) as f32 / (1u64 << 24) as f32
}

/// Sebastiano Vigna's SplitMix64 finaliser: every bit of `x` stirs every
/// bit of the result, the same on every machine.
pub(crate) fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

/// `c` (sRGB) greyed `grey` of the way toward a grey of its own luminance,
/// then darkened by `dark` (a share of its light taken) and lightened by
/// `light` (that share of its light added): both in proportion, so a night
/// stays a night under snow.
fn weathered(c: [f32; 3], grey: f32, dark: f32, light: f32) -> [f32; 3] {
    let lin = linear(c);
    let lum = lin.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    srgb(lin.lerp(Vec3::splat(lum), grey) * (1.0 - dark) * (1.0 + light))
}

/// Room environment `env` (at its hour) under the weather `now` (see the
/// module docs). Under a clear weather it is `env` itself.
pub fn environment_in(env: &Environment, now: &WeatherNow) -> Environment {
    let mut out = env.clone();
    if now.is_clear() {
        return out;
    }
    let o = now.overcast;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;

    // The deck closes; and a storm's wind blows harder, for everything that
    // follows the room's wind - the clouds, the grass, the foliage, the rain
    // (#1684).
    out.cloud_cover = Fp(lerp(env.cloud_cover.0, 1.0, o));
    out.cloud_speed = Fp(env.cloud_speed.0 * (1.0 + now.storm * cfg::storm::WIND_GAIN));
    out.cloud_density = Fp(lerp(
        env.cloud_density.0,
        env.cloud_density.0.max(cfg::OVERCAST_DENSITY),
        o,
    ));

    // The light: the sun's almost gone behind a whole overcast, the sky's
    // a little more; both darker in rain and a storm.
    let storm = now.storm;
    let dark = (now.rain * cfg::RAIN_DARK).max(storm * cfg::STORM_DARK);
    let sun = lerp(1.0, cfg::OVERCAST_SUN, o) * lerp(1.0, cfg::STORM_SUN, storm);
    let ambient =
        lerp(1.0, cfg::OVERCAST_AMBIENT, o) * (1.0 - dark) * (1.0 + now.cover * cfg::SNOW_GLARE);
    out.sun_illuminance = Fp(env.sun_illuminance.0 * sun);
    out.ambient_brightness = Fp(env.ambient_brightness.0 * ambient);

    // The sky, the horizon and the clouds: greyer, and darker in rain and
    // a storm, lighter in snow.
    let light = now.snow * cfg::SNOW_LIGHT;
    let grey = |share: f32| (o * share + now.snow * cfg::SNOW_GREY).min(1.0);
    out.sky_color = Fp3(weathered(env.sky_color.0, grey(cfg::SKY_GREY), dark, light));
    let rgb = |c: Fp4| [c.0[0], c.0[1], c.0[2]];
    let with_alpha = |c: [f32; 3], a: f32| Fp4([c[0], c[1], c[2], a]);
    out.fog_color = with_alpha(
        weathered(rgb(env.fog_color), grey(cfg::HORIZON_GREY), dark, light),
        env.fog_color.0[3],
    );
    out.cloud_color = Fp3(weathered(
        env.cloud_color.0,
        grey(cfg::SKY_GREY),
        dark,
        light,
    ));
    out.cloud_shadow_color = Fp3(weathered(
        env.cloud_shadow_color.0,
        grey(cfg::SKY_GREY),
        dark,
        light,
    ));

    // The haze of a weather is water and ice, which take no colour from
    // the light: the fog's own tints - what it takes from what lies behind
    // it and what it adds (Bevy's atmospheric fog) - grey with the sky's,
    // and wholly in a thick fog.
    let haze = grey(cfg::HORIZON_GREY).max(now.fog);
    out.fog_extinction = Fp3(weathered(env.fog_extinction.0, haze, 0.0, 0.0));
    out.fog_inscattering = Fp3(weathered(env.fog_inscattering.0, haze, 0.0, 0.0));

    // The sun goes behind the cloud: its glow in the fog and its glitter on
    // the water with it.
    out.fog_sun_color = with_alpha(rgb(env.fog_sun_color), env.fog_sun_color.0[3] * (1.0 - o));
    out.water_sun_glitter = Fp(env.water_sun_glitter.0 * (1.0 - o));

    // The air closes in: each of fog, rain and snow takes the visibility
    // that share of the way, in proportion, toward its own.
    let mut visibility = env.fog_visibility.0;
    for (amount, closed) in [
        (now.fog, cfg::FOG_VISIBILITY),
        (now.rain, cfg::RAIN_VISIBILITY),
        (now.snow, cfg::SNOW_VISIBILITY),
    ] {
        if closed < visibility {
            visibility *= (closed / visibility).powf(amount);
        }
    }
    out.fog_visibility = Fp(visibility);
    out
}

#[cfg(test)]
mod tests;
