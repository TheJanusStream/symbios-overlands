//! The room's environment as it is drawn now (#1677, #1678): its record's,
//! at the hour of its time of day (`crate::time_of_day`) and under its
//! weather (`crate::weather`), in that order - a rainy night is a night that
//! rains. What the sun, the sky, the fog, the clouds, the water, the sky
//! light, the ground's weather (`terrain::splat::weather_the_ground`), what
//! falls and the lightning read in place of the record's own. Two readers
//! keep the record's `fog_visibility`: the sun's shadow reach and the
//! ground cover's draw distance. Nothing is drawn wrong by it - a weather's
//! fog hides what lies past it - but they spend their work out to the
//! room's own visibility (#1688).
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
}
