//! The room's environment as it is drawn now (#1677, #1678): its record's,
//! at the hour of its time of day (`crate::time_of_day`) and under its
//! weather (`crate::weather`), in that order - a rainy night is a night that
//! rains. What the sun, the sky, the fog, the clouds, the water, the sky
//! light, the ground's weather (`terrain::splat::weather_the_ground`), what
//! falls and the lightning read in place of the record's own. The sun's
//! shadow reach and the ground cover's draw distance read its fog too
//! (#1688), held against it ([`HeldFog`]) so a weather that comes and goes
//! does not retune them at every step: under a weather's fog they spend
//! their work no further than about a fifth and a grid step past what
//! anyone can see.
//!
//! It is drawn anew when the record changes (the editor's preview lane
//! included, frame by frame through a drag), when a turning day's hour has
//! moved a minute ([`HOUR_STEP`]), and when a weather that comes and goes
//! has moved ([`WEATHER_STEP`]).
//!
//! [`HOUR_STEP`]: crate::config::lighting::time_of_day::HOUR_STEP
//! [`WEATHER_STEP`]: crate::config::lighting::weather::WEATHER_STEP

use bevy::prelude::*;

use crate::config::lighting::{time_of_day as day_cfg, weather as weather_cfg};
use crate::pds::Environment;
use crate::state::LiveRoomRecord;
use crate::time_of_day::{environment_at, hour_at, hours_apart};
use crate::weather::{WeatherNow, environment_in, weather_now};
use crate::world_builder::compile::EnvironmentPreview;

/// The room's environment as it is drawn now: its record's, at the hour of
/// its time of day and under its weather when it has them.
#[derive(Resource, Default)]
pub struct DrawnEnvironment {
    /// The environment drawn.
    pub env: Environment,
    /// The hour it was drawn at; `None` for a room with no time of day.
    pub hour: Option<f32>,
    /// Whether the key light is the moon (1) or the sun (0): what the hour
    /// says (`crate::time_of_day::AtHour::moon`), or under a weather with
    /// no time of day, what the sky reads off the room before its weather.
    /// `None` for a room with neither, whose sky reads it off the light and
    /// the sky as drawn (`crate::sky`).
    pub moon: Option<f32>,
    /// The weather it was drawn under; clear for a room with none.
    pub weather: WeatherNow,
}

/// The system that keeps [`DrawnEnvironment`] up: what reads it runs after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrawEnvironmentSet;

/// Register the drawn environment and the system that keeps it - in the
/// game and in the render tool's world, which draw a room alike.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<DrawnEnvironment>()
        .add_systems(Update, draw_environment.in_set(DrawEnvironmentSet));
}

/// Keep the drawn environment in step with the room (see the module docs).
fn draw_environment(
    record: Option<Res<LiveRoomRecord>>,
    preview: Option<Res<EnvironmentPreview>>,
    mut drawn: ResMut<DrawnEnvironment>,
) {
    let Some(record) = record else {
        return;
    };
    let env = &record.0.environment;
    let now = (env.time_of_day.is_some() || env.weather.is_some()).then(unix_now);
    let hour = env
        .time_of_day
        .as_ref()
        .zip(now)
        .map(|(time, now)| hour_at(time, now));
    let weather = env
        .weather
        .as_ref()
        .zip(now)
        .map_or_else(WeatherNow::default, |(weather, now)| {
            weather_now(weather, now)
        });
    let edited = record.is_changed() || preview.is_some_and(|p| p.is_changed());
    let turned = match (hour, drawn.hour) {
        (Some(now), Some(then)) => hours_apart(now, then) >= day_cfg::HOUR_STEP,
        (now, then) => now.is_some() != then.is_some(),
    };
    if !edited && !turned && !weather_moved(&weather, &drawn.weather) {
        return;
    }
    let (at_hour, moon) = match hour {
        Some(hour) => {
            let at = environment_at(env, hour);
            (at.env, Some(at.moon))
        }
        None => (env.clone(), None),
    };
    // A weather does not change which light is the key, but the sky would
    // read it off the weathered light: an overcast sun is dim, and under a
    // storm's dark sky it reads as the moon (`crate::sky`). So the sky is
    // told what it reads off the room before its weather.
    let moon =
        moon.or_else(|| (!weather.is_clear()).then(|| crate::sky::sky_uniforms(&at_hour).look.y));
    drawn.env = environment_in(&at_hour, &weather);
    drawn.moon = moon;
    drawn.hour = hour;
    drawn.weather = weather;
}

/// Whether the weather `now` has moved from the one drawn, `then`, enough
/// to draw anew: a step ([`WEATHER_STEP`]), or its last trace coming or
/// going - what falls stops when the rain does, not a step short of it.
///
/// [`WEATHER_STEP`]: crate::config::lighting::weather::WEATHER_STEP
fn weather_moved(now: &WeatherNow, then: &WeatherNow) -> bool {
    now.gap(then) >= weather_cfg::WEATHER_STEP
        || now.is_clear() != then.is_clear()
        || now.falls() != then.falls()
}

/// Seconds since 1970 began, UTC, as this machine's clock tells it.
pub(crate) fn unix_now() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1_000.0
}

/// The fog's visibility (m) as it is drawn now (#1688): the room's under
/// its hour and its weather, the record's own in an app that does not draw
/// the environment, and the config's while no room is in.
pub(crate) fn fog_now(record: Option<&LiveRoomRecord>, drawn: Option<&DrawnEnvironment>) -> f32 {
    match (record, drawn) {
        (None, _) => crate::config::camera::fog::VISIBILITY,
        (Some(_), Some(drawn)) => drawn.env.fog_visibility.0,
        (Some(record), None) => record.0.environment.fog_visibility.0,
    }
}

/// A fog a reach or a cut is set for, held against the drawn one (#1688):
/// on a grid of [`HOLD_STEP_M`] rounded up, so never short of what the fog
/// shows; taken at once when the fog lifts past it, so nothing ends in
/// sight; and when the fog closes in, taken only once it is [`HOLD_SHARE`]
/// nearer than the one held, so a weather that comes and goes retunes a
/// reach or a cut a few times a transition rather than at every step.
///
/// [`HOLD_STEP_M`]: crate::config::camera::fog::HOLD_STEP_M
/// [`HOLD_SHARE`]: crate::config::camera::fog::HOLD_SHARE
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HeldFog(Option<f32>);

impl HeldFog {
    /// The fog to set for, `fog` being the one drawn now. A fog that is not
    /// a finite number passes through, holding nothing.
    pub(crate) fn follow(&mut self, fog: f32) -> f32 {
        use crate::config::camera::fog::{HOLD_SHARE, HOLD_STEP_M};
        if !fog.is_finite() {
            self.0 = None;
            return fog;
        }
        let gridded = (fog.max(HOLD_STEP_M) / HOLD_STEP_M).ceil() * HOLD_STEP_M;
        let held = match self.0 {
            Some(held) if gridded <= held && fog > held * (1.0 - HOLD_SHARE) => held,
            _ => gridded,
        };
        self.0 = Some(held);
        held
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, RoomRecord, TimeOfDay, Weather, WeatherKind};

    /// The drawn environment is the record's for a room with neither a time
    /// of day nor a weather, the sky left to read the moon for itself; its
    /// hour's for one with a time of day, the sky told; and its weather's
    /// over that.
    #[test]
    fn the_drawn_environment_follows_the_record() {
        let mut app = App::new();
        let mut record = RoomRecord::default();
        record.environment.time_of_day = None;
        record.environment.weather = None;
        app.insert_resource(LiveRoomRecord(record.clone()));
        register(&mut app);
        app.update();
        let drawn = app.world().resource::<DrawnEnvironment>();
        assert_eq!((drawn.hour, drawn.moon), (None, None));
        assert!(drawn.weather.is_clear());
        assert_eq!(
            drawn.env.sun_illuminance,
            record.environment.sun_illuminance
        );

        record.environment.time_of_day = Some(TimeOfDay {
            hour: Fp(0.0),
            day_minutes: Fp(0.0),
        });
        app.insert_resource(LiveRoomRecord(record.clone()));
        app.update();
        let drawn = app.world().resource::<DrawnEnvironment>();
        assert_eq!((drawn.hour, drawn.moon), (Some(0.0), Some(1.0)));
        assert!(drawn.env.sun_illuminance.0 <= day_cfg::MOON_LUX.1);
        let moonlit = drawn.env.sun_illuminance.0;

        record.environment.weather = Some(Weather {
            kind: WeatherKind::Rain,
            intensity: Fp(1.0),
            spell_minutes: Fp(0.0),
            ..Weather::default()
        });
        app.insert_resource(LiveRoomRecord(record));
        app.update();
        let drawn = app.world().resource::<DrawnEnvironment>();
        assert_eq!(drawn.weather.rain, 1.0);
        assert_eq!(drawn.moon, Some(1.0), "a rainy night is a night");
        assert!(
            drawn.env.sun_illuminance.0 < moonlit,
            "the moon behind cloud"
        );
        assert_eq!(drawn.env.cloud_cover.0, 1.0);
    }

    /// A weather does not change which light is the key: with no time of
    /// day, the sky is told what it reads off the room before its weather,
    /// where a storm's dim light under a dark sky would read as the moon.
    #[test]
    fn a_weather_keeps_the_key_light_what_it_was() {
        let mut app = App::new();
        let mut record = RoomRecord::default();
        record.environment.time_of_day = None;
        record.environment.weather = Some(Weather {
            kind: WeatherKind::Storm,
            intensity: Fp(1.0),
            spell_minutes: Fp(0.0),
            ..Weather::default()
        });
        // A dim room under a dark sky: the storm takes it under the sky's
        // moon thresholds.
        record.environment.sky_color = crate::pds::Fp3([0.30, 0.34, 0.40]);
        record.environment.sun_illuminance = Fp(8_000.0);
        app.insert_resource(LiveRoomRecord(record.clone()));
        register(&mut app);
        app.update();
        let drawn = app.world().resource::<DrawnEnvironment>();
        let own = crate::sky::sky_uniforms(&record.environment).look.y;
        assert_eq!(drawn.moon, Some(own));
        assert!(own < 0.1, "a day room: {own}");
        let weathered = crate::sky::sky_uniforms(&drawn.env).look.y;
        assert!(
            weathered > 0.5,
            "told nothing, the sky reads a moon: {weathered}"
        );
    }

    /// A room whose light stands still is drawn once (#1679): with no time
    /// of day and no weather, or with a standing hour and a standing
    /// weather, the drawn environment does not change frame to frame - and
    /// so nothing that follows it (the sky, the sky light, the ground's
    /// weather, the rain, the materials the environment writes) is written
    /// again.
    #[test]
    fn a_room_whose_light_stands_still_is_drawn_once() {
        let standing = {
            let mut record = RoomRecord::default();
            record.environment.time_of_day = Some(TimeOfDay {
                hour: Fp(18.0),
                day_minutes: Fp(0.0),
            });
            record.environment.weather = Some(Weather {
                kind: WeatherKind::Rain,
                intensity: Fp(0.5),
                spell_minutes: Fp(0.0),
                ..Weather::default()
            });
            record
        };
        let mut calm = RoomRecord::default();
        calm.environment.time_of_day = None;
        calm.environment.weather = None;
        let tick = |app: &App| {
            app.world()
                .resource_ref::<DrawnEnvironment>()
                .last_changed()
        };
        for record in [calm, standing] {
            let mut app = App::new();
            app.insert_resource(LiveRoomRecord(record));
            register(&mut app);
            let made = tick(&app);
            app.update();
            let drawn = tick(&app);
            // The first frame draws the room: this is not watching a system
            // that never runs.
            assert_ne!(drawn, made);
            for _ in 0..5 {
                app.update();
            }
            assert_eq!(tick(&app), drawn);
        }
    }

    /// The weather is drawn anew at each step of it, and when its last trace
    /// of rain goes, however small: what falls stops with the rain.
    #[test]
    fn the_weather_redraws_when_the_rain_stops() {
        let then = WeatherNow {
            overcast: 0.5,
            rain: 0.008,
            ..WeatherNow::default()
        };
        let now = WeatherNow { rain: 0.0, ..then };
        assert!(weather_moved(&now, &then));
        let drizzle = WeatherNow {
            rain: 0.009,
            ..then
        };
        assert!(!weather_moved(&drizzle, &then), "under a step");
        let more = WeatherNow { rain: 0.02, ..then };
        assert!(weather_moved(&more, &then));
    }

    /// A held fog is never short of the drawn one, takes a lifting fog at
    /// once, and holds a thickening one until it is a fifth nearer (#1688):
    /// a weather's fog closing from 350 m to 60 m and lifting again retunes
    /// a reach or a cut a handful of times, not at every step.
    #[test]
    fn a_held_fog_is_never_short_of_the_drawn_one_and_holds_a_thickening_one() {
        let mut held = HeldFog::default();
        assert_eq!(held.follow(350.0), 350.0);
        assert_eq!(held.follow(300.0), 350.0, "a sixth nearer: held");
        assert_eq!(
            held.follow(270.0),
            275.0,
            "past a fifth: taken, on the grid"
        );
        assert_eq!(held.follow(281.0), 300.0, "lifting past it: taken at once");
        assert_eq!(held.follow(9.0), 25.0, "never under one grid step");
        assert!(held.follow(f32::NAN).is_nan(), "a fault passes through");
        assert_eq!(held.follow(60.0), 75.0, "and holds nothing after it");

        let mut held = HeldFog::default();
        let mut retunes = 0;
        let mut last = held.follow(350.0);
        let mut step = |fog: f32, retunes: &mut i32, last: &mut f32| {
            let now = held.follow(fog);
            assert!(now >= fog, "{now} short of {fog}");
            if now != *last {
                *retunes += 1;
                *last = now;
            }
        };
        for i in 0..=290 {
            step(350.0 - i as f32, &mut retunes, &mut last);
        }
        for i in 0..=290 {
            step(60.0 + i as f32, &mut retunes, &mut last);
        }
        assert!(retunes <= 20, "{retunes} retunes for 580 steps of fog");
    }

    /// The fog a reach or a cut reads is the drawn one, a weather's in it,
    /// where the environment is drawn; the record's where it is not; and
    /// the config's with no room in.
    #[test]
    fn the_fog_now_is_the_drawn_one() {
        let mut record = RoomRecord::default();
        record.environment.fog_visibility = Fp(350.0);
        let record = LiveRoomRecord(record);
        let mut drawn = DrawnEnvironment::default();
        drawn.env.fog_visibility = Fp(60.0);
        assert_eq!(fog_now(Some(&record), Some(&drawn)), 60.0);
        assert_eq!(fog_now(Some(&record), None), 350.0);
        assert_eq!(
            fog_now(None, Some(&drawn)),
            crate::config::camera::fog::VISIBILITY
        );
    }
}
