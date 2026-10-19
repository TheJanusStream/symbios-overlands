//! The root `RoomRecord` lexicon: the record's shape on the wire, its
//! atmospheric [`Environment`] payload, its sanitiser, and the
//! deterministic `find_*` lookups every peer must agree on.
//!
//! This was a three-thousand-line file holding three jobs until #1159. The
//! other two now live beside the code they belong with:
//!
//! * [`wire`] - the XRPC fetch / publish / delete / reset wrappers and the
//!   #697 split-wire plan that decides what a publish writes. Re-exported
//!   here, so every existing `pds::room::…` path is unchanged.
//! * [`crate::seeded_defaults::room::build`] - the DID-seeded assembler
//!   behind [`RoomRecord::default_for_seed`], which is a determinism
//!   contract between peers rather than part of this record's shape.

use super::contact_effects::ContactEffects;
use super::generator::{Generator, GeneratorKind, Placement, RoadConfig};
use super::sanitize::{Sanitize, limits, sanitize_generator};
use super::terrain::SovereignTerrainConfig;
use super::types::{Fp, Fp2, Fp3, Fp4};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Non-spatial environment state - directional sun, ambient light, sky
/// cuboid tint, and atmospheric distance fog. Every field is wrapped in a
/// fixed-point type so the record stays DAG-CBOR compliant.
///
/// `#[serde(default)]` lets pre-atmosphere records (which only carried
/// `sun_color`) round-trip: any missing field falls back to the canonical
/// constant via `Environment::default()` rather than failing the whole
/// decode and stranding the owner on the recovery banner.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Environment {
    pub sun_color: Fp3,
    pub sun_illuminance: Fp,
    pub ambient_brightness: Fp,
    pub sky_color: Fp3,
    /// World-space position of the directional sun light. The
    /// renderer reads this as a direction (origin → position normalised
    /// is the unit vector *toward* the sun); the magnitude is informally
    /// "far away", any value with a sensible direction works. Authored
    /// per-room so seeded atmospheres can vary sun altitude / azimuth.
    /// `#[serde(default)]` on the parent struct lets pre-`sun_position`
    /// records round-trip with the canonical constant.
    pub sun_position: Fp3,

    pub fog_color: Fp4,
    pub fog_visibility: Fp,
    pub fog_extinction: Fp3,
    pub fog_inscattering: Fp3,
    pub fog_sun_color: Fp4,
    pub fog_sun_exponent: Fp,

    /// Tiling frequency for the close-distance scrolling detail normal map
    /// (world-unit reciprocal - higher = tighter tiling). Pairs with
    /// [`Self::water_normal_scale_far`] to kill the repeating-grid look on
    /// long camera sightlines.
    pub water_normal_scale_near: Fp,
    /// Tiling frequency for the far-distance scrolling detail normal map.
    pub water_normal_scale_far: Fp,
    /// Intensity of the sharp specular sun-glitter highlight on the water
    /// surface. `0` disables; ~2.0 is a pleasing default.
    pub water_sun_glitter: Fp,
    /// sRGB tint added to wave crests to simulate cheap subsurface scatter.
    pub water_scatter_color: Fp3,
    /// Width (m) of the procedural shoreline foam band. `0` disables;
    /// consumed by the water shader via the camera's opaque depth
    /// prepass to fade foam in where the water meets terrain.
    pub water_shore_foam_width: Fp,

    // ---- Cloud-deck (procedural FBM layer; see `crate::clouds`) -----------
    /// Fraction of sky covered by clouds. `0` = empty blue, `1` = totally
    /// overcast (since #1673 the share of the sky drawn; before it the cut
    /// covered far less at low covers and all of it from about 0.75).
    pub cloud_cover: Fp,
    /// Opacity of the clouds that survive the cover threshold, and how much
    /// of the sun their shade takes off the ground (#1673).
    pub cloud_density: Fp,
    /// Edge-softness band around the cover threshold. Larger ⇒ wispier.
    pub cloud_softness: Fp,
    /// Drift speed (m/s) along [`Self::cloud_wind_dir`].
    pub cloud_speed: Fp,
    /// World metres per UV unit for the cloud noise sampler.
    pub cloud_scale: Fp,
    /// Altitude (m) of the cloud-deck plane.
    pub cloud_height: Fp,
    /// 2D wind direction in world XZ. Need not be unit length - the shader
    /// normalises a small epsilon-padded copy.
    pub cloud_wind_dir: Fp2,
    /// sRGB colour of cloud the sun lights through.
    pub cloud_color: Fp3,
    /// sRGB colour of a cloud's thick underside, in the shade of the cloud
    /// between it and the sun; a cloud leans from [`Self::cloud_color`]
    /// toward it the thicker the cloud the light crossed, and the lower the
    /// sun (#1673).
    pub cloud_shadow_color: Fp3,

    // ---- Colour grading (#1669; see `crate::config::lighting::grading`) ----
    /// White balance, warmer (`+`) or cooler (`-`): an offset to the CIE
    /// 1931 *x* of the white point, Bevy's `ColorGradingGlobal::temperature`.
    /// `0` is neutral and `0.03` already a clear shift.
    pub grade_temperature: Fp,
    /// Green (`-`) to magenta (`+`): the white point's CIE *y* offset.
    pub grade_tint: Fp,
    /// Colour saturation after tonemapping: `1` is neutral, `0` grey.
    pub grade_saturation: Fp,

    /// Ambient audio for the room - a procedurally-baked
    /// [`AudioPatch`] / [`SequenceRecipe`] or a URL/DID-referenced
    /// clip. `None` (the default) plays no ambient track. Forward-
    /// compat across older records: `#[serde(default)]` on the parent
    /// struct lets pre-audio records decode cleanly with this field
    /// elided.
    ///
    /// [`AudioPatch`]: bevy_symbios_audio::AudioPatch
    /// [`SequenceRecipe`]: bevy_symbios_audio::SequenceRecipe
    pub ambient_audio: crate::pds::audio::SovereignAudioConfig,

    /// The room's time of day (#1677). `None`, the default and every room
    /// made before it, keeps the light exactly as the fields above set it.
    /// `Some` makes those fields the room at noon - its sun where
    /// [`Self::sun_position`] stands, its sky and fog as they are - and
    /// draws it at the hour [`TimeOfDay`] gives: the sun turned along its
    /// day's arc, low and red at dawn and dusk, set at night under the moon
    /// and the stars (see `crate::time_of_day`). Older clients ignore the
    /// key and draw the room at its noon.
    pub time_of_day: Option<TimeOfDay>,

    /// The room's weather (#1678). `None`, the default and every room made
    /// before it, keeps the sky, the light and the air as the fields above
    /// and the time of day set them. `Some` lays a weather over them - a
    /// grey sky, rain, a storm, snow or fog, at an intensity - that stands,
    /// or comes and goes with the clock, the same for every visitor (see
    /// `crate::weather`). Older clients ignore the key.
    pub weather: Option<Weather>,
}

/// When a room stands in its day (#1677): an hour the owner sets, which
/// stands still, or, with [`Self::day_minutes`], a day that turns with the
/// clock - the same hour for every visitor, read off UTC, with no message
/// between them.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct TimeOfDay {
    /// The hour, 0 to 24: 12 is noon, the room as its other fields set it.
    /// With a turning day, the hour the day stands at when the clock's
    /// count of days (UTC, from 1970) begins a new one.
    pub hour: Fp,
    /// Real minutes a whole day takes, or 0 for an hour that stands still.
    pub day_minutes: Fp,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            hour: Fp(12.0),
            day_minutes: Fp(0.0),
        }
    }
}

impl TimeOfDay {
    /// `hours` as the record holds an hour: wrapped round the clock face
    /// into `0..24` - 25 is 1 - on the wire's grid, and a NaN or an
    /// infinity noon, the room as its other fields set it. What the
    /// sanitiser keeps and the editor and the render tool write.
    pub fn clock_hour(hours: f32) -> Fp {
        if !hours.is_finite() {
            return Fp(12.0);
        }
        let hour = Fp(hours.rem_euclid(24.0)).on_wire();
        // A hair under 24 rounds onto 24 itself, which is 0.
        if hour.0 >= 24.0 { Fp(0.0) } else { hour }
    }

    /// `minutes` as the record holds a day length: 0, an hour that stands
    /// still - as is a NaN, a negative length or one the wire writes as 0 -
    /// or a day of [`limits::MIN_DAY_MINUTES`] to [`limits::MAX_DAY_MINUTES`]
    /// on the wire's grid. What the sanitiser keeps and the editor writes.
    ///
    /// On the grid because a turning day's hour is its share of every day
    /// since 1970 - four million of them, for a seven-minute day - so a
    /// length a ten-thousandth of a minute off its wire value puts the hour
    /// anywhere at all: the owner must hold the very length every visitor
    /// reads back.
    pub fn day_length(minutes: Fp) -> Fp {
        if !minutes.0.is_finite() || minutes.0 <= 0.0 || minutes.is_wire_zero() {
            Fp(0.0)
        } else {
            Fp(minutes
                .0
                .clamp(limits::MIN_DAY_MINUTES, limits::MAX_DAY_MINUTES))
            .on_wire()
        }
    }
}

/// A room's weather (#1678): what it brings, how heavily, and whether it
/// stands or comes and goes with the clock.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Weather {
    /// What the weather brings.
    pub kind: WeatherKind,
    /// How heavily, 0 to 1: for a weather that comes and goes, at its
    /// heaviest.
    pub intensity: Fp,
    /// Real minutes a spell of weather lasts as it comes and goes with the
    /// clock (UTC) - clear spells and spells of [`Self::kind`], the clouds
    /// gathering before anything falls - or 0 for a weather that stands.
    pub spell_minutes: Fp,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            kind: WeatherKind::Rain,
            intensity: Fp(0.5),
            spell_minutes: Fp(0.0),
        }
    }
}

impl Weather {
    /// `intensity` as the record holds it: within `0..=1` on the wire's
    /// grid, a NaN none at all.
    pub fn intensity_of(intensity: Fp) -> Fp {
        intensity.clamped(0.0, 1.0).on_wire()
    }

    /// `minutes` as the record holds a spell's length: 0, a weather that
    /// stands - as is a NaN, a negative length or one the wire writes as
    /// 0 - or [`limits::MIN_SPELL_MINUTES`] to [`limits::MAX_SPELL_MINUTES`]
    /// on the wire's grid, as [`TimeOfDay::day_length`] holds a day: the
    /// weather's timeline counts its spells since 1970, so every visitor
    /// must read the very length the owner holds.
    pub fn spell_length(minutes: Fp) -> Fp {
        if !minutes.0.is_finite() || minutes.0 <= 0.0 || minutes.is_wire_zero() {
            Fp(0.0)
        } else {
            Fp(minutes
                .0
                .clamp(limits::MIN_SPELL_MINUTES, limits::MAX_SPELL_MINUTES))
            .on_wire()
        }
    }
}

/// What a room's weather brings (#1678). An open union: a kind a newer
/// client adds reads here as [`WeatherKind::Unknown`], drawn as no weather.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum WeatherKind {
    /// A grey sky, the sun behind it.
    #[serde(rename = "network.symbios.weather.overcast")]
    Overcast,
    /// Rain under a grey sky.
    #[serde(rename = "network.symbios.weather.rain")]
    #[default]
    Rain,
    /// Heavy rain under a dark sky (gusts, lightning and thunder: #1684).
    #[serde(rename = "network.symbios.weather.storm")]
    Storm,
    /// Snow falling from a pale sky.
    #[serde(rename = "network.symbios.weather.snow")]
    Snow,
    /// Fog closing in.
    #[serde(rename = "network.symbios.weather.fog")]
    Fog,

    #[serde(other, skip_serializing)]
    Unknown,
}

impl WeatherKind {
    /// Every kind an owner can choose, in the editor's order.
    pub const ALL: [WeatherKind; 5] = [
        WeatherKind::Overcast,
        WeatherKind::Rain,
        WeatherKind::Storm,
        WeatherKind::Snow,
        WeatherKind::Fog,
    ];

    /// The kind's name as the editor and the render tool write it.
    pub fn label(self) -> &'static str {
        match self {
            WeatherKind::Overcast => "overcast",
            WeatherKind::Rain => "rain",
            WeatherKind::Storm => "storm",
            WeatherKind::Snow => "snow",
            WeatherKind::Fog => "fog",
            WeatherKind::Unknown => "unknown",
        }
    }
}

// Default-eliding wire format (#695): a freshly-seeded room overrides only
// the palette-driven subset of these knobs, so the untouched remainder
// (fog shape, water detail tiling, cloud geometry) drops off the wire. The
// container `#[serde(default)]` above is the matching read-side contract.
crate::pds::serde_util::impl_default_eliding_serialize!(Environment {
    sun_color,
    sun_illuminance,
    ambient_brightness,
    sky_color,
    sun_position,
    fog_color,
    fog_visibility,
    fog_extinction,
    fog_inscattering,
    fog_sun_color,
    fog_sun_exponent,
    water_normal_scale_near,
    water_normal_scale_far,
    water_sun_glitter,
    water_scatter_color,
    water_shore_foam_width,
    cloud_cover,
    cloud_density,
    cloud_softness,
    cloud_speed,
    cloud_scale,
    cloud_height,
    cloud_wind_dir,
    cloud_color,
    cloud_shadow_color,
    grade_temperature,
    grade_tint,
    grade_saturation,
    ambient_audio,
    time_of_day,
    weather,
});

impl Default for Environment {
    fn default() -> Self {
        use crate::config::{
            camera::fog as f, lighting as l, lighting::clouds as c, lighting::grading as g,
            terrain::water as w,
        };
        Self {
            sun_color: Fp3(l::SUN_COLOR),
            sun_illuminance: Fp(l::ILLUMINANCE),
            ambient_brightness: Fp(l::AMBIENT_BRIGHTNESS),
            sky_color: Fp3(l::SKY_COLOR),
            sun_position: Fp3(l::LIGHT_POS),

            fog_color: Fp4(f::COLOR),
            fog_visibility: Fp(f::VISIBILITY),
            fog_extinction: Fp3(f::EXTINCTION_COLOR),
            fog_inscattering: Fp3(f::INSCATTERING_COLOR),
            fog_sun_color: Fp4(f::DIRECTIONAL_LIGHT_COLOR),
            fog_sun_exponent: Fp(f::DIRECTIONAL_LIGHT_EXPONENT),

            water_normal_scale_near: Fp(w::DEFAULT_NORMAL_SCALE_NEAR),
            water_normal_scale_far: Fp(w::DEFAULT_NORMAL_SCALE_FAR),
            water_sun_glitter: Fp(w::DEFAULT_SUN_GLITTER),
            water_scatter_color: Fp3(w::DEFAULT_SCATTER_COLOR),
            water_shore_foam_width: Fp(w::DEFAULT_SHORE_FOAM_WIDTH),

            cloud_cover: Fp(c::COVER),
            cloud_density: Fp(c::DENSITY),
            cloud_softness: Fp(c::SOFTNESS),
            cloud_speed: Fp(c::SPEED),
            cloud_scale: Fp(c::SCALE),
            cloud_height: Fp(c::HEIGHT),
            cloud_wind_dir: Fp2(c::WIND_DIR),
            cloud_color: Fp3(c::COLOR),
            cloud_shadow_color: Fp3(c::SHADOW_COLOR),

            grade_temperature: Fp(g::TEMPERATURE),
            grade_tint: Fp(g::TINT),
            grade_saturation: Fp(g::SATURATION),

            ambient_audio: crate::pds::audio::SovereignAudioConfig::None,

            time_of_day: None,
            weather: None,
        }
    }
}

/// How far from the origin a seeded room's sun stands (m) - and where the
/// Environment panel's sun sliders put one whose record left no distance
/// to keep (#1669). Only the direction is read.
pub(crate) const SEEDED_SUN_DISTANCE: f32 = 75.0;

/// The sun's position `altitude_deg` above the horizon and `azimuth_deg`
/// round +Y (0 degrees at +X, 90 at +Z), `radius` from the origin - the
/// form [`Environment::sun_position`] stores. The seeding and the
/// Environment panel's sun sliders share it (#1669), so the two agree on
/// which way is which.
pub(crate) fn sun_position_at(altitude_deg: f32, azimuth_deg: f32, radius: f32) -> [f32; 3] {
    let alt = altitude_deg.to_radians();
    let az = azimuth_deg.to_radians();
    let horiz = radius * alt.cos();
    [horiz * az.cos(), radius * alt.sin(), horiz * az.sin()]
}

/// [`sun_position_at`]'s inverse: the altitude above the horizon and the
/// azimuth in `[0, 360)` (degrees), and the distance from the origin. A
/// sun straight overhead has no azimuth of its own and answers 0.
pub(crate) fn sun_angles(position: [f32; 3]) -> (f32, f32, f32) {
    let [x, y, z] = position;
    let radius = (x * x + y * y + z * z).sqrt();
    if radius <= f32::EPSILON {
        return (90.0, 0.0, 0.0);
    }
    let altitude = (y / radius).clamp(-1.0, 1.0).asin().to_degrees();
    let azimuth = z.atan2(x).to_degrees().rem_euclid(360.0);
    (altitude, azimuth, radius)
}

impl Environment {
    /// Clamp every field so a malicious or malformed record cannot crash
    /// the renderer with NaN, negative light values, or a zero visibility
    /// that makes `FogFalloff::from_visibility_colors` divide by zero.
    pub fn sanitize(&mut self) {
        let clamp_unit = |v: f32| v.clamp(0.0, 1.0);
        let clamp3 = |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
        let clamp4 = |c: Fp4| {
            Fp4([
                clamp_unit(c.0[0]),
                clamp_unit(c.0[1]),
                clamp_unit(c.0[2]),
                clamp_unit(c.0[3]),
            ])
        };

        self.sun_color = clamp3(self.sun_color);
        self.sky_color = clamp3(self.sky_color);
        self.fog_color = clamp4(self.fog_color);
        self.fog_extinction = clamp3(self.fog_extinction);
        self.fog_inscattering = clamp3(self.fog_inscattering);
        self.fog_sun_color = clamp4(self.fog_sun_color);

        self.sun_illuminance = Fp(self.sun_illuminance.0.clamp(0.0, 100_000.0));
        self.ambient_brightness = Fp(self.ambient_brightness.0.clamp(0.0, 10_000.0));

        // Sun-position guard: each component must be finite and the
        // vector cannot collapse to the origin (it's used as a
        // direction by `looking_at`). On any failure, fall back to the
        // canonical constant - that always gives a valid direction.
        let sp = self.sun_position.0;
        let bad = !sp[0].is_finite()
            || !sp[1].is_finite()
            || !sp[2].is_finite()
            || (sp[0] * sp[0] + sp[1] * sp[1] + sp[2] * sp[2]) < 1.0e-6;
        if bad {
            self.sun_position = Fp3(crate::config::lighting::LIGHT_POS);
        } else {
            self.sun_position = Fp3([
                sp[0].clamp(-10_000.0, 10_000.0),
                sp[1].clamp(-10_000.0, 10_000.0),
                sp[2].clamp(-10_000.0, 10_000.0),
            ]);
        }
        // A zero visibility would make `FogFalloff::from_visibility_colors`
        // blow up (it divides by `visibility` internally). Floor at 10 m so
        // the falloff remains well-defined even under an adversarial record.
        self.fog_visibility = Fp(self.fog_visibility.0.clamp(10.0, 10_000.0));
        self.fog_sun_exponent = Fp(self.fog_sun_exponent.0.clamp(0.0, 200.0));

        // Time of day (#1677): an hour on the clock face, wrapped rather
        // than clamped so 25 reads as 1 (`TimeOfDay::clock_hour`), and a
        // day of a minute to a week or none (`TimeOfDay::day_length`), both
        // as the wire holds them.
        if let Some(time) = self.time_of_day.as_mut() {
            time.hour = TimeOfDay::clock_hour(time.hour.0);
            time.day_minutes = TimeOfDay::day_length(time.day_minutes);
        }
        // Weather (#1678): an intensity of 0 to 1 and a spell of two minutes
        // to a week or none, as the wire holds them. An unknown kind stays
        // as it came, for a newer client, and is drawn as no weather.
        if let Some(weather) = self.weather.as_mut() {
            weather.intensity = Weather::intensity_of(weather.intensity);
            weather.spell_minutes = Weather::spell_length(weather.spell_minutes);
        }

        // Water-environment fields. Keep every channel in a finite,
        // physically-sane range - a NaN or negative normal-tiling scale
        // would poison the water shader's UV math every frame.
        let clamp_finite_pos = |v: f32, lo: f32, hi: f32, default: f32| -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        };
        self.water_normal_scale_near = Fp(clamp_finite_pos(
            self.water_normal_scale_near.0,
            0.0,
            64.0,
            0.85,
        ));
        self.water_normal_scale_far = Fp(clamp_finite_pos(
            self.water_normal_scale_far.0,
            0.0,
            64.0,
            0.08,
        ));
        self.water_sun_glitter = Fp(clamp_finite_pos(self.water_sun_glitter.0, 0.0, 16.0, 1.8));
        self.water_scatter_color = clamp3(self.water_scatter_color);
        self.water_shore_foam_width = Fp(clamp_finite_pos(
            self.water_shore_foam_width.0,
            0.0,
            50.0,
            0.0,
        ));

        // Cloud-deck fields. Same NaN / range guarding as water - the cloud
        // shader divides by `cloud_scale` and reads `cloud_height` straight
        // into a `Transform.translation.y`, so a poisoned record must not
        // be allowed to feed Inf or negative values into either.
        self.cloud_cover = Fp(clamp_finite_pos(self.cloud_cover.0, 0.0, 1.0, 0.45));
        self.cloud_density = Fp(clamp_finite_pos(self.cloud_density.0, 0.0, 1.0, 0.85));
        self.cloud_softness = Fp(clamp_finite_pos(self.cloud_softness.0, 0.001, 1.0, 0.18));
        self.cloud_speed = Fp(clamp_finite_pos(self.cloud_speed.0, 0.0, 200.0, 4.0));
        self.cloud_scale = Fp(clamp_finite_pos(self.cloud_scale.0, 1.0, 10_000.0, 320.0));
        self.cloud_height = Fp(clamp_finite_pos(self.cloud_height.0, 5.0, 10_000.0, 250.0));
        let wd = self.cloud_wind_dir.0;
        let wd0 = if wd[0].is_finite() {
            wd[0].clamp(-100.0, 100.0)
        } else {
            1.0
        };
        let wd1 = if wd[1].is_finite() {
            wd[1].clamp(-100.0, 100.0)
        } else {
            0.3
        };
        // Reject the zero vector - the shader normalises wind_dir and a
        // bit-for-bit zero would NaN-out the noise sampling. A vanishingly
        // small magnitude falls back to the canonical default.
        let mag2 = wd0 * wd0 + wd1 * wd1;
        self.cloud_wind_dir = if mag2 > 1.0e-6 {
            Fp2([wd0, wd1])
        } else {
            Fp2([1.0, 0.3])
        };
        self.cloud_color = clamp3(self.cloud_color);
        self.cloud_shadow_color = clamp3(self.cloud_shadow_color);

        // Colour grading: a NaN would poison every pixel the camera draws.
        {
            use crate::config::lighting::grading as g;
            self.grade_temperature = Fp(clamp_finite_pos(
                self.grade_temperature.0,
                -g::TEMPERATURE_LIMIT,
                g::TEMPERATURE_LIMIT,
                g::TEMPERATURE,
            ));
            self.grade_tint = Fp(clamp_finite_pos(
                self.grade_tint.0,
                -g::TINT_LIMIT,
                g::TINT_LIMIT,
                g::TINT,
            ));
            self.grade_saturation = Fp(clamp_finite_pos(
                self.grade_saturation.0,
                0.0,
                g::SATURATION_MAX,
                g::SATURATION,
            ));
        }

        // Forward to the asset-class sanitiser - caps the embedded
        // patch / sequence JSON length and Referenced URL / DID / CID
        // strings so a hostile peer can't smuggle a megabyte through
        // the audio slot.
        self.ambient_audio.sanitize();
    }
}

/// Owner-configurable visitor arrival pose (#745). Applied to anyone who
/// enters the room *without* an explicit target pose: a plain login/home
/// spawn, gateway travel, and the fall-through respawn. An explicit pose -
/// a landmark link's `pos=`/`rot=` or a portal's baked `target_pos` -
/// always wins over this default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct DefaultLanding {
    /// Ground-plane landing position `(x, z)`, world-centred like every
    /// placement translation.
    pub pos: Fp2,
    /// Explicit landing height. `None` (the wire default) resolves the
    /// height from the terrain heightmap at `(x, z)` - the drop-pin form
    /// landmark links use - so the common case survives terrain edits
    /// without the owner re-aiming the pose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Fp>,
    /// Facing, in degrees, counter-clockwise seen from above: 0 faces −Z,
    /// 90 faces −X - a compass bearing negated, and the landmark link's
    /// `rot=` (`player::spawn_facing`, #1547). Seeded rooms aim this at the
    /// gateway landmark.
    #[serde(default)]
    pub yaw_deg: Fp,
}

/// The full recipe: environment + generators + placements + traits. Acts as
/// a Bevy `Resource` so the [`crate::world_builder`] module can compile it
/// into ECS entities.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub environment: Environment,
    pub generators: HashMap<String, Generator>,
    pub placements: Vec<Placement>,
    /// Maps a generator name to a list of trait strings (e.g.
    /// `"collider_heightfield"`, `"sensor"`) the world compiler should attach
    /// to every entity that generator spawns.
    pub traits: HashMap<String, Vec<String>>,
    /// Authored avatar-world contact-effect recipes (#246). `#[serde(default)]`
    /// so pre-Phase-4 records (which lack the key) deserialize with the
    /// canonical defaults and behave exactly as the old hardcoded
    /// registry; `RoomRecord` carries no `deny_unknown_fields`, so older
    /// clients reading a newer record simply ignore the extra key. The
    /// skip on write is the same fact in reverse (#695): an uncustomised
    /// set is omitted rather than re-stating the canonical recipes.
    #[serde(default, skip_serializing_if = "ContactEffects::is_default")]
    pub contact_effects: ContactEffects,
    /// Where visitors without an explicit target pose come to rest (#745).
    /// Same field-vs-container split as `contact_effects`: `None` (elided
    /// on the wire) keeps the legacy behaviour - a random scatter around
    /// the world origin - so pre-#745 records round-trip unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_landing: Option<DefaultLanding>,
    /// The square of real map data a geodata region is built from (#1583).
    /// `None` - elided on the wire - is the procedural world every record
    /// was before it, so those records round-trip byte for byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geo_source: Option<crate::pds::GeoSource>,
    /// Manifest refs (name → child rkey) whose child record was **listed on
    /// the PDS but could not be decoded by this build** (#1175).
    ///
    /// These are somebody else's content, not ours: a newer client wrote a
    /// child generator in a shape this build's `Generator` cannot parse. We
    /// cannot render it, so it is absent from `generators` - but it exists,
    /// the owner authored it, and the next publish from this client would
    /// otherwise rewrite the manifest without its ref and then GC the child
    /// as an orphan. Carrying the ref through fetch → edit → publish keeps
    /// the record referenced and the bytes intact, so upgrading the client
    /// brings the generator back rather than finding it gone.
    ///
    /// A ref whose child is missing from the listing *entirely* does NOT
    /// land here - that is a torn write pointing at nothing, and preserving
    /// it would resurrect a dangling ref forever. Only present-but-opaque.
    ///
    /// `BTreeMap` because it feeds `generator_refs`, whose ordering the
    /// manifest's byte-canonicality depends on.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub opaque_refs: std::collections::BTreeMap<String, String>,
}

impl RoomRecord {
    /// Zero-configuration homeworld. When a client visits a DID whose owner
    /// has never saved a custom record, this builds the canonical default
    /// recipe on the fly - a base terrain plus a base water plane - so the
    /// world builder always has something valid to compile.
    ///
    /// The recipe itself lives with the other DID-seeded derivers, in
    /// [`crate::seeded_defaults::room::build`]: it is the determinism
    /// contract between peers, not part of this record's wire shape
    /// (#1159).
    pub fn default_for_did(did: &str) -> Self {
        crate::seeded_defaults::room::build::build_room_for_did(did)
    }

    /// Build the seeded default room from a pre-computed seed - the
    /// manual re-roll path. `default_for_did` is exactly
    /// `default_for_seed(fnv1a_64(did), did)`. See
    /// [`crate::seeded_defaults::room::build::build_room`].
    pub fn default_for_seed(seed: u64, did: &str) -> Self {
        crate::seeded_defaults::room::build::build_room(seed, did)
    }

    /// `self` as every peer holds it (#1707): written, read back and
    /// sanitised, as `network::inbound` takes in an owner's broadcast and a
    /// fetch takes in a save. Twice: a value the sanitiser clamps to a
    /// bound off the wire's 1/10 000 grid reads back rounded, and sanitised
    /// again it is that bound once more or stays the rounded value inside
    /// it - either way the second pass is the fixed point a peer reaches
    /// from the first. `None` where the room cannot be written (a variant
    /// this build only carries opaquely, #1111).
    pub(crate) fn as_peers_hold(&self) -> Option<RoomRecord> {
        let mut held: RoomRecord = crate::pds::read_back(self)?;
        held.sanitize();
        let mut again: RoomRecord = crate::pds::read_back(&held)?;
        again.sanitize();
        Some(again)
    }

    /// Clamp every numeric field to a safe upper bound. Every path that
    /// accepts a `RoomRecord` from the network (PDS fetch and peer-broadcast
    /// `RoomStateUpdate`) calls this before handing the record to the world
    /// compiler, so an attacker cannot weaponise an unbounded field to crash
    /// or OOM the victim.
    pub fn sanitize(&mut self) {
        // Clamp atmospheric fields first - cheap and independent of everything
        // else, and guarantees the world compiler never hands NaN or a zero
        // visibility to `FogFalloff::from_visibility_colors`.
        self.environment.sanitize();
        // Authored contact-effect recipes: clamp every numeric, bound
        // the recipe list deterministically (#246).
        self.contact_effects.sanitize();
        // Default-landing pose (#745): same positional bounds as a portal
        // `target_pos`. The finite guard matters even though the fixed-point
        // wire form can only decode to finite values - in-process mutation
        // (a future editor widget) feeds this too, and `f32::clamp`
        // propagates NaN.
        if let Some(landing) = &mut self.default_landing {
            let cf = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 0.0 };
            landing.pos.0[0] = cf(landing.pos.0[0], -10_000.0, 10_000.0);
            landing.pos.0[1] = cf(landing.pos.0[1], -10_000.0, 10_000.0);
            if let Some(y) = &mut landing.y {
                y.0 = cf(y.0, -1_000.0, 10_000.0);
            }
            landing.yaw_deg.0 = if landing.yaw_deg.0.is_finite() {
                landing.yaw_deg.0.rem_euclid(360.0)
            } else {
                0.0
            };
        }
        // Geodata source (#1583): a Berlin square is put back on the map if
        // it strays off it; a source that cannot be made safe is dropped,
        // and the region draws procedurally.
        if let Some(source) = &mut self.geo_source
            && !source.sanitize()
        {
            self.geo_source = None;
        }
        // Clean generator names before anything keyed by them is read:
        // cut to the shared length cap and stripped of invisible
        // characters (#1205), with placements and traits following each
        // rename so the world compiler can still resolve them. Unknown
        // placements stay untouched - their generator_ref is invisible
        // to this build.
        let renames = crate::pds::sanitize::names::sanitize_keys(
            &mut self.generators,
            limits::MAX_GENERATOR_NAME_CHARS,
        );
        for (old, new) in &renames {
            for p in self.placements.iter_mut() {
                match p {
                    Placement::Absolute { generator_ref, .. }
                    | Placement::Scatter { generator_ref, .. }
                    | Placement::Grid { generator_ref, .. }
                    | Placement::Instances { generator_ref, .. } => {
                        if generator_ref == old {
                            *generator_ref = new.clone();
                        }
                    }
                    Placement::Unknown => {}
                }
            }
            if let Some(traits) = self.traits.remove(old) {
                self.traits.entry(new.clone()).or_insert(traits);
            }
        }
        // Bound the total number of generators before touching any of them.
        // Drop entries in lexicographic key order so the survivor set is
        // deterministic across peers - otherwise a record with 1000
        // generators and `MAX_GENERATORS = 256` would resolve to a
        // different 256 on every client (HashMap iteration is SipHash
        // randomised) and fracture the shared world.
        if self.generators.len() > limits::MAX_GENERATORS {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(limits::MAX_GENERATORS) {
                self.generators.remove(&key);
            }
        }
        // Snapshot the names of generators whose root kind is Terrain or
        // Water *before* `sanitize_generator` rewrites them. Any
        // `Scatter`/`Grid` placement targeting one of these is positionally
        // invalid: a Scatter of a Terrain root would spawn duplicate
        // heightfield colliders (Avian forbids that), and Water can never
        // legally be a root. We capture the snapshot first because the
        // generator pass overwrites root Water with a default cuboid - if
        // we filtered after, a Scatter pointing at the now-cuboid would
        // silently spawn N copies of an unrelated shape instead of being
        // dropped outright.
        let ineligible_targets: std::collections::HashSet<String> = self
            .generators
            .iter()
            .filter(|(_, g)| {
                matches!(
                    g.kind,
                    GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
                )
            })
            .map(|(name, _)| name.clone())
            .collect();
        for generator in self.generators.values_mut() {
            sanitize_generator(generator);
        }
        // Drop offending Scatter/Grid placements - and instance sets
        // (#1643), which draw many copies as they do - before applying the
        // count cap, so 1024 ineligible entries can't push valid ones
        // past `MAX_PLACEMENTS`. Absolute is left alone - pointing it
        // at a Terrain root is the canonical home-world placement, and
        // a hostile Water-rooted Absolute is already neutralised by
        // the generator-level overwrite above.
        self.placements.retain(|p| match p {
            Placement::Scatter { generator_ref, .. }
            | Placement::Grid { generator_ref, .. }
            | Placement::Instances { generator_ref, .. } => {
                !ineligible_targets.contains(generator_ref)
            }
            _ => true,
        });
        // Drop excess placements so a 1M-entry array can't force
        // `compile_room_record` to spawn tens of millions of entities in
        // a single frame. Keeping a prefix is order-stable (serde
        // round-trips `Vec` in order) so every peer truncates to the
        // same survivor set.
        if self.placements.len() > limits::MAX_PLACEMENTS {
            self.placements.truncate(limits::MAX_PLACEMENTS);
        }
        for placement in self.placements.iter_mut() {
            match placement {
                Placement::Scatter {
                    count, naturalness, ..
                } => {
                    *count = (*count).min(limits::MAX_SCATTER_COUNT);
                    naturalness.sanitize();
                }
                Placement::Grid { counts, gaps, .. } => {
                    counts[0] = counts[0].clamp(1, 100);
                    counts[1] = counts[1].clamp(1, 100);
                    counts[2] = counts[2].clamp(1, 100);
                    let total = (counts[0] as usize)
                        .saturating_mul(counts[1] as usize)
                        .saturating_mul(counts[2] as usize);
                    if total > 10_000 {
                        counts[0] = counts[0].min(21);
                        counts[1] = counts[1].min(21);
                        counts[2] = counts[2].min(21);
                    }
                    gaps.0[0] = gaps.0[0].clamp(0.01, 1000.0);
                    gaps.0[1] = gaps.0[1].clamp(0.01, 1000.0);
                    gaps.0[2] = gaps.0[2].clamp(0.01, 1000.0);
                }
                _ => {}
            }
        }
        // Instance sets and grammar values (#1643): each set bounded, and
        // the copies of all of them together.
        crate::pds::sanitize::instances::sanitize_placements(&mut self.placements);
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self::default_for_did("")
    }
}

/// Return the terrain generator with the lexicographically smallest key.
///
/// `HashMap::values()` iteration order is randomised per execution (SipHash),
/// so a record with more than one `Generator::Terrain` entry would otherwise
/// have every client picking a different one and landing on a different
/// heightmap - instantly fracturing the shared world. Every site that needs
/// "the terrain" for a record must go through this function (or its sibling)
/// so the choice is deterministic across peers.
pub fn find_terrain_config(record: &RoomRecord) -> Option<&SovereignTerrainConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(generator) = record.generators.get(k)
            && let GeneratorKind::Terrain(cfg) = &generator.kind
        {
            return Some(cfg);
        }
    }
    None
}

/// Sub-stream salt so a room's road layout seed differs from its terrain seed
/// while staying deterministic in the DID.
/// Return the road-network config attached to the deterministically-chosen
/// terrain generator (its `RoadNetwork` child), if any. Mirrors
/// [`find_terrain_config`]'s sorted-key determinism so every peer reads the
/// same config; the terrain plugin builds the road mesh from this plus the
/// finished heightmap (see [`crate::urban`]).
pub fn find_road_config(record: &RoomRecord) -> Option<&RoadConfig> {
    find_road_configs(record).into_iter().next()
}

/// Cap on simultaneously-active road networks per room (#895): every network
/// costs a full graph trace + extrusion + lot layer, and four districts
/// already saturate a 1 km room. RoadNetwork children beyond the cap are
/// inert (the editor warns on them, #886).
pub const MAX_ROAD_NETWORKS: usize = 4;

/// Total entities one generator tree spawns: itself plus every descendant.
///
/// The vegetation budget needs a per-instance cost for the ground-cover props
/// (#911). Unlike an L-system - whose expansion has to be *estimated* by
/// deriving the grammar - these are plain primitive trees, so the node count
/// is the exact spawn cost.
pub(crate) fn generator_entity_count(generator: &Generator) -> u64 {
    1 + generator
        .children
        .iter()
        .map(generator_entity_count)
        .sum::<u64>()
}

/// Every active road network, in child order, up to [`MAX_ROAD_NETWORKS`]
/// (#895): all `RoadNetwork` children of the deterministically-chosen
/// (sorted-first) Terrain generator. Same determinism contract as
/// [`find_road_config`], which is simply this list's head.
pub fn find_road_configs(record: &RoomRecord) -> Vec<&RoadConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(generator) = record.generators.get(k)
            && let GeneratorKind::Terrain(_) = &generator.kind
        {
            return generator
                .children
                .iter()
                .filter_map(|c| match &c.kind {
                    GeneratorKind::RoadNetwork(cfg) => Some(cfg),
                    _ => None,
                })
                .take(MAX_ROAD_NETWORKS)
                .collect();
        }
    }
    Vec::new()
}

pub(crate) mod parts;
mod wire;

pub use parts::RoomCopiesRecord;
pub use wire::{
    RoomGeneratorRecord, child_rkey, delete_room_record, fetch_room_record,
    max_publish_record_bytes, measure_publish, publish_room_record, reset_room_record,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The sun's position and its angles are each other's inverse (#1669):
    /// the Environment panel reads a seeded room's sun and writes back the
    /// one the seeding would have placed at the same angles.
    #[test]
    fn a_suns_angles_and_its_position_round_trip() {
        for (altitude, azimuth) in [(10.0, 0.0), (45.0, 90.0), (68.0, 200.0), (30.0, 359.0)] {
            let p = sun_position_at(altitude, azimuth, 75.0);
            let (alt, az, r) = sun_angles(p);
            assert!((alt - altitude).abs() < 1e-3, "{altitude} came back {alt}");
            assert!((az - azimuth).abs() < 1e-3, "{azimuth} came back {az}");
            assert!((r - 75.0).abs() < 1e-3, "75 m came back {r}");
        }
        // 0 degrees of azimuth is +X, 90 is +Z.
        let east = sun_position_at(0.0, 0.0, 1.0);
        let south = sun_position_at(0.0, 90.0, 1.0);
        assert!(east[0] > 0.999 && south[2] > 0.999, "{east:?} {south:?}");
        // Overhead has no azimuth of its own, and a sun at the origin none
        // at all; neither panics or answers NaN.
        let (alt, az, _) = sun_angles([0.0, 5.0, 0.0]);
        assert!((alt - 90.0).abs() < 1e-3 && az.is_finite());
        let (alt, az, r) = sun_angles([0.0, 0.0, 0.0]);
        assert!(alt.is_finite() && az.is_finite() && r == 0.0);
    }

    /// The sun's position is the seeding's formula to the bit (#1669): the
    /// seeding and the Environment panel share [`sun_position_at`], so an
    /// edit to it would move the sun of every room still on its seed.
    #[test]
    fn a_suns_position_is_the_seeding_formula_to_the_bit() {
        for (altitude, azimuth) in [
            (18.0f32, 0.0f32),
            (40.5, 133.3),
            (60.0, 359.9),
            (68.0, 271.0),
        ] {
            let (alt, az) = (altitude.to_radians(), azimuth.to_radians());
            let horiz = SEEDED_SUN_DISTANCE * alt.cos();
            let want = [
                horiz * az.cos(),
                SEEDED_SUN_DISTANCE * alt.sin(),
                horiz * az.sin(),
            ];
            let got = sun_position_at(altitude, azimuth, SEEDED_SUN_DISTANCE);
            assert_eq!(
                got.map(f32::to_bits),
                want.map(f32::to_bits),
                "{altitude} {azimuth}"
            );
        }
    }

    /// A room that stores no grade keeps Bevy's neutral one, and a neutral
    /// grade stays off the wire (#1669): every room published before the
    /// grade existed reads, renders and re-serialises as it did.
    #[test]
    fn a_neutral_grade_is_the_default_and_stays_off_the_wire() {
        let env = Environment::default();
        assert_eq!(env.grade_temperature.0, 0.0);
        assert_eq!(env.grade_tint.0, 0.0);
        assert_eq!(env.grade_saturation.0, 1.0);
        let wire = serde_json::to_string(&env).expect("serialises");
        assert!(!wire.contains("grade_"), "{wire}");
        let mut graded = env.clone();
        graded.grade_temperature = Fp(0.02);
        let wire = serde_json::to_string(&graded).expect("serialises");
        assert!(wire.contains("grade_temperature"), "{wire}");
        assert!(!wire.contains("grade_tint"), "{wire}");
    }

    /// The sanitiser keeps a grade inside what the camera can show and puts
    /// a NaN back to neutral (#1669).
    #[test]
    fn the_sanitiser_holds_the_grade_in_range() {
        use crate::config::lighting::grading as g;
        let mut env = Environment {
            grade_temperature: Fp(9.0),
            grade_tint: Fp(-9.0),
            grade_saturation: Fp(f32::NAN),
            ..Environment::default()
        };
        env.sanitize();
        assert_eq!(env.grade_temperature.0, g::TEMPERATURE_LIMIT);
        assert_eq!(env.grade_tint.0, -g::TINT_LIMIT);
        assert_eq!(env.grade_saturation.0, g::SATURATION);
    }
}
