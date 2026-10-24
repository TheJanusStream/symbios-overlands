//! A seeded world's signature hour (#1769): the one flattering hour a
//! procedural world holds, standing - its day turns only when its owner
//! turns it in the World Editor (owner decision, epic #1767).
//!
//! A world with a time of day takes its light fields as its noon and draws
//! its hour from them ([`crate::time_of_day`]): the sun along its arc, its
//! light dimmed and reddened by the air it crosses, the horizon's glow round
//! sunrise and sunset, the moon by night. So a procedural world's palette
//! and atmosphere are derived for noon (the build zeroes the scene's
//! daylight bias for them), and its mood is an hour rather than a sun
//! lowered by hand and tinted amber on top - which the engine would then
//! have lowered and reddened again.
//!
//! **The mood** ([`Mood`]) is drawn from its theme's table ([`weights`]):
//! a daylight theme most often stands at the golden hour or a low sun, a
//! dusk theme (its kit's lamps and windows carry it) toward the blue hour,
//! and a night theme - the old `theme_luminosity` dims under 0.5 - mostly
//! under its moon. The hour follows from the mood's sun altitude and the
//! day's noon altitude, morning or evening as drawn.
//!
//! **The light falls across the arrival's view** ([`SignatureHour::compose`]):
//! the noon sun's bearing is chosen so that at the hour the key light - the
//! sun, or by night the moon - stands well to one side of the way the
//! landing faces: side light that models what it falls on, never the flat
//! light of a sun at the camera's back, and only for a low sun a little
//! ahead, rimming what stands against it. The bearing is found through the
//! engine's own drawing ([`crate::time_of_day::environment_at`]), so the
//! seed and the sky cannot disagree about where the sun stands.

use std::f32::consts::PI;

use bevy::math::{Vec2, Vec3};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::{Environment, Fp, Fp3, TimeOfDay};
use crate::seeded_defaults::ThemeArchetype;
use crate::seeded_defaults::scene::{SceneCharacter, range_f32, unit_f32};

/// Sub-stream salt distinct from every other deriver's.
const HOUR_STREAM_SALT: u64 = 0x4F55_5253_4947_4E31;

/// The lowest and highest a seeded day's noon sun stands, degrees: from a
/// northern summer's to an autumn's, so a low sun and a moonlit night both
/// have room under it.
const NOON_ALTITUDE_DEG: (f32, f32) = (42.0, 68.0);

/// How far below its noon a day mood's sun keeps, degrees: a "day" that
/// rose to noon would be noon itself, which the fields already are.
const DAY_BELOW_NOON_DEG: f32 = 4.0;

/// The bearings tried for the noon sun, a degree apart.
const BEARINGS: u32 = 360;

/// A world's mood: the light it stands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    /// A high sun's clear light, late morning or early afternoon.
    Day,
    /// A low sun's long shadows and warm light.
    Low,
    /// The golden hour: the sun a few degrees up, the horizon aglow.
    Golden,
    /// The blue hour: the sun just under the horizon, the sky deep blue over
    /// a warm horizon, lamps and windows carrying the near light.
    Blue,
    /// Night under the moon.
    Night,
}

impl Mood {
    /// Every mood, in the order the weight tables list them.
    pub const ALL: [Mood; 5] = [Mood::Day, Mood::Low, Mood::Golden, Mood::Blue, Mood::Night];

    /// The band of the sun's altitude the mood draws from, degrees - by
    /// night the sun's, under the horizon, whose opposite the moon is. Each
    /// sits where the engine's fades put the look: the golden hour inside
    /// the horizon's glow over a sun still lit, the blue hour where the sun
    /// has faded and the moon not yet risen, night past the moon's fade
    /// (`config::lighting::time_of_day`).
    pub fn sun_altitude_deg(self) -> (f32, f32) {
        match self {
            Mood::Day => (32.0, 50.0),
            Mood::Low => (14.0, 24.0),
            Mood::Golden => (4.0, 9.0),
            Mood::Blue => (-2.5, -0.5),
            Mood::Night => (-28.0, -16.0),
        }
    }

    /// How far off the view's bearing the key light falls, degrees: across
    /// it, and for a low sun's warm light a little ahead as well, rimming
    /// what stands against it.
    pub fn across_deg(self) -> (f32, f32) {
        match self {
            Mood::Golden | Mood::Blue => (35.0, 110.0),
            _ => (60.0, 120.0),
        }
    }

    /// The fog's glow lobe at the mood, against noon's exponent: a low sun
    /// throws a broader glow than a high one.
    pub fn glow_widening(self) -> f32 {
        match self {
            Mood::Day => 1.0,
            Mood::Low => 0.85,
            Mood::Golden | Mood::Blue => 0.7,
            Mood::Night => 1.0,
        }
    }

    /// Whether the world stands in the dark: the blue hour's, or the
    /// night's - when its climate brings no storm (#1770).
    pub fn is_dark(self) -> bool {
        matches!(self, Mood::Blue | Mood::Night)
    }

    /// The mood's name, as the render tool prints it.
    pub fn label(self) -> &'static str {
        match self {
            Mood::Day => "day",
            Mood::Low => "low sun",
            Mood::Golden => "golden hour",
            Mood::Blue => "blue hour",
            Mood::Night => "night",
        }
    }
}

/// How often a theme's world stands in each [`Mood`], in [`Mood::ALL`]'s
/// order: by the dim its kit was staged for (`theme_luminosity`) - a
/// daylight theme's buildings read by the sun, a dusk theme's by its lamps
/// and windows as the light goes, a night theme's by their own light.
pub fn weights(theme: ThemeArchetype) -> [u32; 5] {
    let luminosity = crate::seeded_defaults::theme_luminosity(theme);
    if luminosity >= 1.0 {
        [25, 30, 40, 5, 0]
    } else if luminosity >= 0.5 {
        [10, 20, 35, 25, 10]
    } else {
        [0, 5, 15, 30, 50]
    }
}

/// A world's signature hour, before it is turned to its arrival.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SignatureHour {
    pub mood: Mood,
    /// The day's noon sun altitude, degrees.
    pub noon_altitude_deg: f32,
    /// The hour, 0 to 24.
    pub hour: f32,
    /// How far off the view the key light is to stand, degrees.
    pub across_deg: f32,
    /// Which side of the view it stands on: `1` to the left, `-1` the right.
    pub side: f32,
}

impl SignatureHour {
    /// The hour `scene`'s world stands at, from its own stream of `room_seed`.
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ HOUR_STREAM_SALT);
        let mood = pick_weighted(&weights(scene.theme), unit_f32(&mut rng));
        let noon_altitude_deg = range_f32(&mut rng, NOON_ALTITUDE_DEG.0, NOON_ALTITUDE_DEG.1);
        let (lo, hi) = mood.sun_altitude_deg();
        let altitude = range_f32(&mut rng, lo, hi).min(noon_altitude_deg - DAY_BELOW_NOON_DEG);
        let evening = unit_f32(&mut rng) < 0.5;
        let from_noon = hours_from_noon(noon_altitude_deg, altitude);
        let hour = if evening {
            12.0 + from_noon
        } else {
            12.0 - from_noon
        };
        let (lo, hi) = mood.across_deg();
        let across_deg = range_f32(&mut rng, lo, hi);
        let side = if unit_f32(&mut rng) < 0.5 { 1.0 } else { -1.0 };
        Self {
            mood,
            noon_altitude_deg,
            hour,
            across_deg,
            side,
        }
    }

    /// Turn the hour to an arrival looking along `view` (horizontal): write
    /// into `env` - the world at noon - the noon sun whose key light at the
    /// hour stands [`Self::across_deg`] off `view` on [`Self::side`], and the
    /// standing time of day; and widen the fog's glow for a low sun.
    ///
    /// The hour may be its mirror about noon - the evening's for the
    /// morning's, the same sun altitude: the engine's arc turns one way
    /// under a noon sun in the south and the other under one in the north
    /// (so it always rises in the east), and at a given hour that leaves a
    /// band of bearings no noon reaches; the mirrored hour reaches them.
    pub fn compose(&self, view: Vec2, env: &mut Environment) {
        let view = view.try_normalize().unwrap_or(Vec2::NEG_Y);
        let distance = Vec3::from_array(env.sun_position.0).length().max(1.0);
        let altitude = self.noon_altitude_deg.to_radians();
        let target = self.side * self.across_deg;
        let noon_at = |bearing: f32| {
            Vec3::new(
                bearing.sin() * altitude.cos(),
                altitude.sin(),
                bearing.cos() * altitude.cos(),
            )
        };
        // The noon bearing and the hour whose key light lands nearest the
        // target, as the engine draws them: the noon sun turned to the hour
        // - and to the moon opposite it by night.
        let mut probe = env.clone();
        let mut miss = |hour: f32, bearing: f32| {
            probe.sun_position = Fp3((noon_at(bearing) * distance).to_array());
            let key = crate::time_of_day::environment_at(&probe, hour)
                .env
                .sun_position
                .0;
            let off = signed_angle_deg(view, Vec2::new(key[0], key[2]));
            ((off - target + 180.0).rem_euclid(360.0) - 180.0).abs()
        };
        let (hour, bearing) = [self.hour, 24.0 - self.hour]
            .into_iter()
            .flat_map(|hour| {
                (0..BEARINGS)
                    .map(move |i| (hour, i as f32 * std::f32::consts::TAU / BEARINGS as f32))
            })
            .map(|(hour, bearing)| (hour, bearing, miss(hour, bearing)))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map_or((self.hour, 0.0), |(hour, bearing, _)| (hour, bearing));
        env.sun_position = Fp3((noon_at(bearing) * distance).to_array());
        env.time_of_day = Some(TimeOfDay {
            hour: TimeOfDay::clock_hour(hour),
            day_minutes: Fp(0.0),
        });
        env.fog_sun_exponent = Fp(env.fog_sun_exponent.0 * self.mood.glow_widening());
    }
}

/// How many hours from noon a day whose noon sun stands `noon_deg` high has
/// its sun at `altitude_deg`: the engine's equinox arc
/// ([`crate::time_of_day::sun_toward`]) has `sin(altitude) = sin(noon) *
/// cos(15 degrees an hour from noon)`. An altitude past either end of the
/// day is taken as the end.
pub fn hours_from_noon(noon_deg: f32, altitude_deg: f32) -> f32 {
    let c = (altitude_deg.to_radians().sin() / noon_deg.to_radians().sin()).clamp(-1.0, 1.0);
    c.acos() * 12.0 / PI
}

/// The angle from `from` to `to` seen from above, degrees, -180 to 180:
/// positive turning counter-clockwise seen from above (from -Z toward -X,
/// as the spawn path's facing turns).
fn signed_angle_deg(from: Vec2, to: Vec2) -> f32 {
    // Seen from above with +X right and +Z down the page, counter-clockwise
    // takes -Z (up the page) toward -X (left): the cross product's sign in
    // (x, z) is the other way round from the page's (x, y).
    let cross = from.x * to.y - from.y * to.x;
    (-cross).atan2(from.dot(to)).to_degrees()
}

/// The mood `u` (0 to 1) falls on with `weights` laid end to end.
fn pick_weighted(weights: &[u32; 5], u: f32) -> Mood {
    let total: u32 = weights.iter().sum();
    let mut at = u.clamp(0.0, 1.0) * total as f32;
    let mut last = Mood::Day;
    for (mood, &w) in Mood::ALL.iter().zip(weights).filter(|(_, w)| **w > 0) {
        if at < w as f32 {
            return *mood;
        }
        at -= w as f32;
        last = *mood;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hour the arc puts a sun at: drawn through the engine's own arc,
    /// the hour from noon for an altitude stands the sun at that altitude.
    #[test]
    fn the_hour_from_noon_stands_the_sun_at_its_altitude() {
        for noon in [42.0f32, 55.0, 68.0] {
            let n = noon.to_radians();
            let noon_sun = Vec3::new(0.0, n.sin(), n.cos());
            for altitude in [45.0f32, 20.0, 6.0, -2.5, -22.0] {
                if altitude >= noon {
                    continue;
                }
                for evening in [false, true] {
                    let from = hours_from_noon(noon, altitude);
                    let hour = if evening { 12.0 + from } else { 12.0 - from };
                    let sun = crate::time_of_day::sun_toward(noon_sun, hour);
                    let drawn = sun.y.asin().to_degrees();
                    assert!(
                        (drawn - altitude).abs() < 1e-2,
                        "noon {noon}, altitude {altitude}, evening {evening}: drawn {drawn}"
                    );
                }
            }
        }
    }

    /// Counter-clockwise seen from above is positive, as the spawn's facing
    /// turns: from -Z a quarter turn toward -X is +90.
    #[test]
    fn a_turn_toward_minus_x_from_minus_z_is_positive() {
        let north = Vec2::new(0.0, -1.0);
        assert!((signed_angle_deg(north, Vec2::new(-1.0, 0.0)) - 90.0).abs() < 1e-4);
        assert!((signed_angle_deg(north, Vec2::new(1.0, 0.0)) + 90.0).abs() < 1e-4);
        assert!(signed_angle_deg(north, north).abs() < 1e-4);
    }

    /// The composed key light - the sun, or by night the moon - stands the
    /// asked angle off the view, on the asked side, as the engine draws the
    /// hour; and the world's time of day is the standing hour.
    #[test]
    fn the_key_light_falls_across_the_view_as_drawn() {
        let views = [
            Vec2::new(0.0, -1.0),
            Vec2::new(0.6, 0.8),
            Vec2::new(-1.0, 0.0),
        ];
        for mood in Mood::ALL {
            for (k, view) in views.into_iter().enumerate() {
                let (lo, hi) = mood.sun_altitude_deg();
                let noon = 55.0;
                let altitude = ((lo + hi) * 0.5).min(noon - DAY_BELOW_NOON_DEG);
                let hour = if k % 2 == 0 {
                    12.0 + hours_from_noon(noon, altitude)
                } else {
                    12.0 - hours_from_noon(noon, altitude)
                };
                let side = if k == 1 { -1.0 } else { 1.0 };
                let sign = SignatureHour {
                    mood,
                    noon_altitude_deg: noon,
                    hour,
                    across_deg: 80.0,
                    side,
                };
                let mut env = Environment::default();
                sign.compose(view, &mut env);
                let time = env.time_of_day.expect("a time of day");
                assert_eq!(time.day_minutes.0, 0.0, "it stands");
                let drawn = crate::time_of_day::environment_at(&env, time.hour.0);
                let key = drawn.env.sun_position.0;
                let off = signed_angle_deg(view, Vec2::new(key[0], key[2]));
                assert!(
                    (off - side * 80.0).abs() < 1.5,
                    "{mood:?} view {view}: the key light stands {off} off it"
                );
                assert_eq!(drawn.moon >= 0.5, mood == Mood::Night, "{mood:?}");
                // The blue hour's key is the sun just under the horizon.
                let up = Vec3::from_array(key).normalize().y.asin().to_degrees();
                assert_eq!(
                    up > 0.0,
                    mood != Mood::Blue,
                    "{mood:?}: the key stands at {up}"
                );
            }
        }
    }

    /// Each theme's table is a mood for every draw, and the dimmer the kit
    /// was staged the more of its worlds stand in the dark: no daylight
    /// theme's world is ever night, every night theme's mostly is.
    #[test]
    fn a_dimmer_kit_stands_its_worlds_later_in_the_day() {
        for theme in ThemeArchetype::ALL {
            let w = weights(theme);
            assert!(w.iter().sum::<u32>() > 0, "{theme:?}");
            let luminosity = crate::seeded_defaults::theme_luminosity(theme);
            if luminosity >= 1.0 {
                assert_eq!(w[4], 0, "{theme:?} is a daylight theme");
            }
            if luminosity < 0.5 {
                assert!(w[3] + w[4] > w[0] + w[1] + w[2], "{theme:?}");
            }
        }
        for u in [0.0, 0.3, 1.0] {
            assert_eq!(pick_weighted(&[0, 0, 1, 0, 0], u), Mood::Golden);
        }
        assert_eq!(pick_weighted(&[1, 0, 0, 0, 1], 0.25), Mood::Day);
        assert_eq!(pick_weighted(&[1, 0, 0, 0, 1], 0.75), Mood::Night);
    }

    /// A seeded hour is a standing hour of the drawn mood, its sun at the
    /// mood's altitude on a noon in the seeded band, and the same world
    /// draws the same hour every time.
    #[test]
    fn a_world_draws_its_hour_from_its_mood() {
        for seed in 0..64u64 {
            let scene = SceneCharacter::for_seed(seed);
            let sign = SignatureHour::from_scene(&scene, seed);
            assert_eq!(sign, SignatureHour::from_scene(&scene, seed));
            assert!((NOON_ALTITUDE_DEG.0..NOON_ALTITUDE_DEG.1).contains(&sign.noon_altitude_deg));
            let n = sign.noon_altitude_deg.to_radians();
            let sun = crate::time_of_day::sun_toward(Vec3::new(0.0, n.sin(), n.cos()), sign.hour);
            let altitude = sun.y.asin().to_degrees();
            let (lo, hi) = sign.mood.sun_altitude_deg();
            assert!(
                altitude >= lo - 1e-2 && altitude <= hi.min(sign.noon_altitude_deg) + 1e-2,
                "seed {seed}: {:?} at {altitude}",
                sign.mood
            );
            assert!(
                weights(scene.theme)[Mood::ALL.iter().position(|m| *m == sign.mood).unwrap()] > 0
            );
        }
    }
}
