//! Time of day's tests (#1677): the sun's arc, the light of each hour, the
//! clock's hour, the record field and the drawn environment.

use super::*;
use crate::pds::RoomRecord;
use crate::pds::room::sun_position_at;

/// A room whose noon sun stands at `altitude` degrees.
fn room(altitude: f32) -> Environment {
    Environment {
        sun_position: Fp3(sun_position_at(altitude, 30.0, 75.0)),
        ..Environment::default()
    }
}

fn toward(env: &Environment) -> Vec3 {
    Vec3::from_array(env.sun_position.0).normalize()
}

fn altitude(v: Vec3) -> f32 {
    v.y.asin().to_degrees()
}

fn close3(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() <= tol)
}

/// Noon is the room as its fields set it, however high or low its noon
/// sun stands: a room taking a time of day keeps its noon look.
#[test]
fn noon_is_the_room_as_its_fields_set_it() {
    for noon_altitude in [50.0, 4.0] {
        let env = room(noon_altitude);
        let at = environment_at(&env, 12.0);
        assert_eq!(at.moon, 0.0, "{noon_altitude}");
        let noon = at.env;
        assert!(
            toward(&noon).abs_diff_eq(toward(&env), 1e-5),
            "{noon_altitude}"
        );
        let lux = (noon.sun_illuminance.0 - env.sun_illuminance.0).abs();
        assert!(lux < env.sun_illuminance.0 * 1e-4, "{noon_altitude}: {lux}");
        assert!(close3(noon.sun_color.0, env.sun_color.0, 2e-3));
        assert!(close3(noon.sky_color.0, env.sky_color.0, 2e-3));
        let rgb = |c: Fp4| [c.0[0], c.0[1], c.0[2]];
        assert!(close3(rgb(noon.fog_color), rgb(env.fog_color), 2e-3));
        assert_eq!(noon.fog_sun_color, env.fog_sun_color);
        assert_eq!(noon.ambient_brightness, env.ambient_brightness);
    }
}

/// The sun rises at six and sets at eighteen, a quarter turn either side of
/// noon's bearing, and stands as far under the horizon at midnight as it
/// stood over it at noon.
#[test]
fn the_sun_rises_at_six_and_sets_at_eighteen() {
    let noon = Vec3::new(0.6, 0.8, 0.0);
    let rise = sun_toward(noon, 6.0);
    let set = sun_toward(noon, 18.0);
    assert!(rise.y.abs() < 1e-5 && set.y.abs() < 1e-5, "{rise} {set}");
    assert!(
        rise.dot(Vec3::X).abs() < 1e-5,
        "a quarter turn from noon's bearing"
    );
    assert!((rise + set).length() < 1e-5, "rising and setting opposite");
    let midnight = sun_toward(noon, 0.0);
    assert!((altitude(midnight) + altitude(noon)).abs() < 1e-3);
    let afternoon = altitude(sun_toward(noon, 15.0));
    assert!(afternoon > 0.0 && afternoon < altitude(noon), "{afternoon}");
}

/// By night the key light is the moon, standing where the noon sun stood,
/// and the sky is told so; the sky is dark enough for the stars; and the
/// ambient light is night's share.
#[test]
fn midnight_is_moonlit_and_starry() {
    let env = room(50.0);
    let at = environment_at(&env, 0.0);
    assert_eq!(at.moon, 1.0);
    let midnight = at.env;
    assert!(toward(&midnight).abs_diff_eq(toward(&env), 1e-4));
    let lux = midnight.sun_illuminance.0;
    assert!((cfg::MOON_LUX.0..=cfg::MOON_LUX.1).contains(&lux), "{lux}");
    let sky = crate::sky::sky_uniforms(&midnight);
    assert!(sky.look.z > 0.5, "the stars: {}", sky.look.z);
    let noon = crate::sky::sky_uniforms(&environment_at(&env, 12.0).env);
    assert!(noon.look.z < 0.1, "{}", noon.look);
    let share = midnight.ambient_brightness.0 / env.ambient_brightness.0;
    assert!((share - cfg::NIGHT_AMBIENT_SHARE).abs() < 1e-3, "{share}");
}

/// Toward sunset the sun dims and reddens and the horizon warms; and no hour
/// is lit more brightly than noon, the moon included in a room whose noon
/// is dimmer than the moon's floor.
#[test]
fn the_low_sun_is_dim_red_and_warms_the_horizon() {
    let env = room(50.0);
    let noon = environment_at(&env, 12.0).env;
    let low = environment_at(&env, 17.6).env;
    assert!(altitude(toward(&low)) > 0.0 && altitude(toward(&low)) < 8.0);
    assert!(low.sun_illuminance.0 < noon.sun_illuminance.0 * 0.5);
    let warmth = |c: [f32; 3]| c[0] / c[2].max(1e-3);
    assert!(warmth(low.sun_color.0) > warmth(noon.sun_color.0) * 1.5);
    let rgb = |c: Fp4| [c.0[0], c.0[1], c.0[2]];
    assert!(warmth(rgb(low.fog_color)) > warmth(rgb(noon.fog_color)));
    let dim = Environment {
        sun_illuminance: Fp(cfg::MOON_LUX.0 * 0.5),
        ..room(50.0)
    };
    for env in [env, dim] {
        for quarter in 0..96 {
            let hour = quarter as f32 / 4.0;
            let lux = environment_at(&env, hour).env.sun_illuminance.0;
            assert!(lux <= env.sun_illuminance.0 * 1.0001, "{hour}: {lux}");
        }
    }
}

/// The low sun is drawn as the sun, however dim: the sky would read a dim
/// light under a darkening sky as the moon. And the key light passes from
/// the sun to the moon where neither gives any light, so neither the light
/// nor the disc jumps.
#[test]
fn the_setting_sun_stays_the_sun_and_hands_over_in_the_dark() {
    for noon_altitude in [50.0, 12.0] {
        let env = room(noon_altitude);
        let mut last: Option<AtHour> = None;
        // Every 3.6 s of the day from mid-afternoon to dawn.
        for step in 0..15_000 {
            let hour = 15.0 + step as f32 * 0.001;
            let at = environment_at(&env, hour);
            let sun = altitude(sun_toward(noon_toward(&env), hour));
            if sun > cfg::SUN_FADE_DEG.0 {
                assert_eq!(at.moon, 0.0, "{noon_altitude}: {hour}");
            }
            if sun < cfg::MOON_FADE_DEG.1 {
                assert_eq!(at.moon, 1.0, "{noon_altitude}: {hour}");
            }
            if let Some(before) = last
                && before.moon != at.moon
            {
                // With the two fades overlapping, the hand-over went from
                // 23 lux of sun to 29 of moon, the light turning half round.
                let lux = [before.env.sun_illuminance.0, at.env.sun_illuminance.0];
                assert!(lux[0] < 0.5 && lux[1] < 0.5, "{hour}: {lux:?}");
            }
            last = Some(at);
        }
    }
}

/// A turning day reads its hour off the clock: one instant gives every
/// visitor one hour, a whole day later the same, half a day later twelve
/// hours on; at a whole number of the clock's days it is the record's own
/// hour; and a standing hour stands.
#[test]
fn a_turning_day_reads_its_hour_off_the_clock() {
    let turning = TimeOfDay {
        hour: Fp(6.0),
        day_minutes: Fp(60.0),
    };
    let t = 1_760_000_123.0;
    let h = hour_at(&turning, t);
    assert!(hours_apart(hour_at(&turning, t + 3_600.0), h) < 1e-3);
    assert!((hours_apart(hour_at(&turning, t + 1_800.0), h) - 12.0).abs() < 1e-3);
    assert!((hours_apart(hour_at(&turning, t + 150.0), h) - 1.0).abs() < 1e-3);
    assert!(hours_apart(hour_at(&turning, 3_600.0 * 1_000.0), 6.0) < 1e-3);
    let standing = TimeOfDay {
        hour: Fp(21.5),
        day_minutes: Fp(0.0),
    };
    assert_eq!(hour_at(&standing, t), 21.5);
    assert_eq!(hour_at(&standing, t + 12_345.0), 21.5);
}

/// The owner sees the hour every visitor sees. A turning day's hour is its
/// share of every day since 1970, so a length a ten-thousandth of a minute
/// off is any hour at all: the owner holds the length as the wire carries
/// it, to the bit, and a visitor reads back the very hour.
#[test]
fn the_owner_and_every_visitor_read_one_hour() {
    let asked = Fp(7.123_456);
    let owner = TimeOfDay {
        hour: TimeOfDay::clock_hour(5.555_555),
        day_minutes: TimeOfDay::day_length(asked),
    };
    let wire = serde_json::to_value(owner).expect("serialises");
    let visitor: TimeOfDay = serde_json::from_value(wire).expect("reads back");
    assert_eq!(visitor, owner);
    let t = 1_760_000_123.456;
    assert_eq!(hour_at(&owner, t), hour_at(&visitor, t));
    // What holding the length as asked would have done.
    let unrounded = TimeOfDay {
        day_minutes: asked,
        ..owner
    };
    let apart = hours_apart(hour_at(&unrounded, t), hour_at(&visitor, t));
    assert!(apart > 0.1, "{apart}");
}

/// The editor keeps the hour where the owner sees it: setting the hour it
/// is now, or a new day length mid-drag, leaves the room at that hour.
#[test]
fn the_editor_keeps_the_hour_it_shows() {
    let t = 1_760_000_123.456;
    for minutes in [0.0, 1.0, 7.123_5, 1_440.0, 10_080.0] {
        let day_minutes = TimeOfDay::day_length(Fp(minutes));
        for hour in [0.0, 5.75, 12.0, 23.99] {
            let time = TimeOfDay {
                hour: anchored(hour, day_minutes, t),
                day_minutes,
            };
            let shown = hour_at(&time, t);
            assert!(hours_apart(shown, hour) < 1e-3, "{minutes} {hour}: {shown}");
            // A new length keeps the hour shown.
            for other in [1.0, 60.0, 1_440.0] {
                let length = TimeOfDay::day_length(Fp(other));
                let moved = TimeOfDay {
                    hour: anchored(shown, length, t),
                    day_minutes: length,
                };
                let after = hour_at(&moved, t);
                assert!(hours_apart(after, shown) < 1e-3, "{minutes}->{other}");
            }
        }
    }
}

/// The record's time of day: an hour wraps round the clock and a day is a
/// minute to a week or none - a shorter one would flash the room between day
/// and night - both on the wire's grid; a NaN hour is noon; and a room
/// without one writes nothing.
#[test]
fn a_time_of_day_is_sanitised_and_absent_from_the_wire_without_one() {
    let mut env = Environment {
        time_of_day: Some(TimeOfDay {
            hour: Fp(25.0),
            day_minutes: Fp(1.0e6),
        }),
        ..Environment::default()
    };
    env.sanitize();
    let time = env.time_of_day.expect("kept");
    assert_eq!(time.hour.0, 1.0);
    assert_eq!(time.day_minutes.0, crate::pds::limits::MAX_DAY_MINUTES);
    let hour = |hours: f32| TimeOfDay::clock_hour(hours).0;
    assert_eq!(hour(-1.0), 23.0);
    assert_eq!(hour(23.999_99), 0.0);
    assert_eq!(hour(f32::INFINITY), 12.0);
    let length = |minutes: f32| TimeOfDay::day_length(Fp(minutes)).0;
    assert_eq!(length(0.01), crate::pds::limits::MIN_DAY_MINUTES);
    assert_eq!(length(90.0), 90.0);
    assert_eq!(length(7.123_456), Fp(7.123_456).on_wire().0);
    for standing in [0.0, -5.0, 0.000_01, f32::INFINITY] {
        assert_eq!(length(standing), 0.0, "{standing}");
    }
    let mut nan = Environment {
        time_of_day: Some(TimeOfDay {
            hour: Fp(f32::NAN),
            day_minutes: Fp(f32::NAN),
        }),
        ..Environment::default()
    };
    nan.sanitize();
    assert_eq!(
        nan.time_of_day,
        Some(TimeOfDay {
            hour: Fp(12.0),
            day_minutes: Fp(0.0)
        })
    );
    let plain = serde_json::to_value(Environment::default()).expect("serialises");
    assert!(plain.get("time_of_day").is_none(), "{plain}");
    let set = Environment {
        time_of_day: Some(TimeOfDay::default()),
        ..Environment::default()
    };
    let wire = serde_json::to_value(&set).expect("serialises");
    let back: Environment = serde_json::from_value(wire).expect("reads back");
    assert_eq!(back.time_of_day, Some(TimeOfDay::default()));
}

/// The drawn environment is the record's for a room with no time of day,
/// the sky left to read the moon for itself, and its hour's for one with a
/// time of day, the sky told.
#[test]
fn the_drawn_environment_follows_the_record() {
    let mut app = App::new();
    let mut record = RoomRecord::default();
    record.environment.time_of_day = None;
    app.insert_resource(LiveRoomRecord(record.clone()));
    register(&mut app);
    app.update();
    let drawn = app.world().resource::<DrawnEnvironment>();
    assert_eq!((drawn.hour, drawn.moon), (None, None));
    assert_eq!(
        drawn.env.sun_illuminance,
        record.environment.sun_illuminance
    );
    record.environment.time_of_day = Some(TimeOfDay {
        hour: Fp(0.0),
        day_minutes: Fp(0.0),
    });
    app.insert_resource(LiveRoomRecord(record));
    app.update();
    let drawn = app.world().resource::<DrawnEnvironment>();
    assert_eq!((drawn.hour, drawn.moon), (Some(0.0), Some(1.0)));
    assert!(drawn.env.sun_illuminance.0 <= cfg::MOON_LUX.1);
}
