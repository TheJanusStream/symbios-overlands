//! Weather's tests (#1678): what each kind brings, a weather that comes and
//! goes, the light and the air under it, the record field and what falls.

use super::precipitation::{PrecipitationUniforms, uniforms, volume_mesh};
use super::*;
use crate::pds::limits::{MAX_SPELL_MINUTES, MIN_SPELL_MINUTES};

fn standing(kind: WeatherKind, intensity: f32) -> Weather {
    Weather {
        kind,
        intensity: Fp(intensity),
        spell_minutes: Fp(0.0),
    }
}

fn luminance(srgb: [f32; 3]) -> f32 {
    linear(srgb).dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// A weather that stands is its kind at its intensity, whatever the clock;
/// rain, a storm and snow bring clouds with them; a kind a newer client
/// added brings nothing.
#[test]
fn a_standing_weather_is_its_kind_at_its_intensity() {
    let t = 1_760_000_123.0;
    let rain = weather_now(&standing(WeatherKind::Rain, 0.5), t);
    assert_eq!(
        rain,
        weather_now(&standing(WeatherKind::Rain, 0.5), t + 9_999.0)
    );
    assert_eq!(rain.rain, 0.5);
    assert!(rain.overcast >= cfg::RAIN.overcast.0, "{rain:?}");
    let storm = weather_now(&standing(WeatherKind::Storm, 1.0), t);
    assert!(storm.storm == 1.0 && storm.rain == 1.0 && storm.overcast == 1.0);
    let snow = weather_now(&standing(WeatherKind::Snow, 1.0), t);
    assert!(snow.snow == 1.0 && snow.rain == 0.0 && snow.overcast > 0.5);
    let fog = weather_now(&standing(WeatherKind::Fog, 1.0), t);
    assert!(fog.fog == 1.0 && fog.rain == 0.0 && fog.snow == 0.0);
    assert!(weather_now(&standing(WeatherKind::Unknown, 1.0), t).is_clear());
    assert!(amounts(WeatherKind::Rain, f32::NAN).rain == 0.0);
    // A spell that is not a number (unsanitised) stands rather than spoiling
    // every drawn field.
    let nan = Weather {
        spell_minutes: Fp(f32::NAN),
        ..standing(WeatherKind::Rain, 0.5)
    };
    assert_eq!(weather_now(&nan, t), rain);
    assert_eq!(severity_at(&nan, t), 1.0);
}

/// A weather that comes and goes: one instant gives every visitor one
/// weather; it moves smoothly, never a jump; over many spells it is clear
/// at times and at its heaviest at times; the clouds gather before anything
/// falls; and two kinds keep different timelines.
#[test]
fn a_changing_weather_comes_and_goes_smoothly_on_one_timeline() {
    let weather = Weather {
        kind: WeatherKind::Rain,
        intensity: Fp(0.8),
        spell_minutes: Fp(10.0),
    };
    let t0 = 1_760_000_000.0;
    let (mut lowest, mut highest) = (f32::MAX, f32::MIN);
    let mut last = severity_at(&weather, t0);
    // Every ten seconds through a hundred spells.
    for step in 1..6_000 {
        let t = t0 + f64::from(step) * 10.0;
        let s = severity_at(&weather, t);
        assert!((0.0..=1.0).contains(&s), "{s}");
        assert!((s - last).abs() < 0.05, "a jump at {t}: {last} to {s}");
        lowest = lowest.min(s);
        highest = highest.max(s);
        last = s;
        let now = weather_now(&weather, t);
        if now.rain > 0.0 {
            assert!(now.overcast > 0.5 * cfg::RAIN.overcast.1, "{t}: {now:?}");
        }
        assert!(now.rain <= 0.8 + 1e-6);
    }
    assert!(lowest < cfg::CLOUDS_GATHER.0, "never clear: {lowest}");
    assert!(highest > cfg::FALLS.1, "never at its heaviest: {highest}");
    let snowy = Weather {
        kind: WeatherKind::Snow,
        ..weather
    };
    let apart = (0..100)
        .map(|k| t0 + f64::from(k) * 600.0)
        .filter(|&t| (severity_at(&weather, t) - severity_at(&snowy, t)).abs() > 0.05)
        .count();
    assert!(apart > 50, "two kinds, one timeline: {apart}");
}

/// The owner and every visitor read one weather: the spell length is held
/// on the wire's grid, so the visitor's record reads the owner's timeline
/// to the bit.
#[test]
fn the_owner_and_every_visitor_read_one_weather() {
    let owner = Weather {
        kind: WeatherKind::Storm,
        intensity: Weather::intensity_of(Fp(0.333_333)),
        spell_minutes: Weather::spell_length(Fp(7.123_456)),
    };
    let wire = serde_json::to_value(owner).expect("serialises");
    assert_eq!(
        wire["kind"]["$type"], "network.symbios.weather.storm",
        "{wire}"
    );
    let visitor: Weather = serde_json::from_value(wire).expect("reads back");
    assert_eq!(visitor, owner);
    let t = 1_760_000_123.456;
    assert_eq!(weather_now(&owner, t), weather_now(&visitor, t));
}

/// The record's weather: an intensity of 0 to 1 and a spell of two minutes
/// to a week or none, on the wire's grid; a kind a newer client wrote reads
/// as unknown and is kept; and a room without a weather writes nothing.
#[test]
fn a_weather_is_sanitised_and_absent_from_the_wire_without_one() {
    let mut env = Environment {
        weather: Some(Weather {
            kind: WeatherKind::Snow,
            intensity: Fp(3.0),
            spell_minutes: Fp(1.0e6),
        }),
        ..Environment::default()
    };
    env.sanitize();
    let weather = env.weather.expect("kept");
    assert_eq!(weather.intensity.0, 1.0);
    assert_eq!(weather.spell_minutes.0, MAX_SPELL_MINUTES);
    let spell = |minutes: f32| Weather::spell_length(Fp(minutes)).0;
    assert_eq!(spell(0.5), MIN_SPELL_MINUTES);
    assert_eq!(spell(45.0), 45.0);
    for standing in [0.0, -1.0, 0.000_01, f32::NAN] {
        assert_eq!(spell(standing), 0.0, "{standing}");
    }
    assert_eq!(Weather::intensity_of(Fp(f32::NAN)).0, 0.0);

    let newer = serde_json::json!({
        "kind": {"$type": "network.symbios.weather.hail"},
        "intensity": 5000,
        "spell_minutes": 0
    });
    let read: Weather = serde_json::from_value(newer).expect("a newer kind reads");
    assert_eq!(read.kind, WeatherKind::Unknown);
    assert!(weather_now(&read, 0.0).is_clear());

    let plain = serde_json::to_value(Environment::default()).expect("serialises");
    assert!(plain.get("weather").is_none(), "{plain}");
}

/// Under a clear weather the room is itself. Overcast closes the deck and
/// dims the sun far more than the sky's light; rain and a storm darken the
/// sky, snow lightens it; and fog closes the air most, then snow, then rain.
#[test]
fn the_weather_lays_over_the_light_and_the_air() {
    let env = Environment::default();
    let wire = |e: &Environment| serde_json::to_value(e).expect("serialises");
    assert_eq!(
        wire(&environment_in(&env, &WeatherNow::default())),
        wire(&env)
    );

    let at = |kind| environment_in(&env, &amounts(kind, 1.0));
    let overcast = at(WeatherKind::Overcast);
    assert_eq!(overcast.cloud_cover.0, 1.0);
    let sun = overcast.sun_illuminance.0 / env.sun_illuminance.0;
    let sky = overcast.ambient_brightness.0 / env.ambient_brightness.0;
    assert!(sun < 0.5 * sky, "sun {sun} against the sky's {sky}");
    assert_eq!(overcast.water_sun_glitter.0, 0.0);

    let rain = at(WeatherKind::Rain);
    let storm = at(WeatherKind::Storm);
    let snow = at(WeatherKind::Snow);
    let fog = at(WeatherKind::Fog);
    let sky = |e: &Environment| luminance(e.sky_color.0);
    assert!(sky(&storm) < sky(&rain) && sky(&rain) < sky(&overcast));
    assert!(sky(&snow) > sky(&rain));
    let visibility = |e: &Environment| e.fog_visibility.0;
    assert!(visibility(&fog) < visibility(&snow));
    assert!(visibility(&snow) < visibility(&rain));
    assert!(visibility(&rain) < visibility(&env));
    assert!((visibility(&fog) - cfg::FOG_VISIBILITY).abs() < 1.0);
}

/// The ground keeps what fell (#1683): under a weather that stands it is as
/// wet as its rain makes it and snowed over as far as its snow; under one
/// that comes and goes it is never drier than the rain now falling makes it,
/// it stays wet a while after the rain stops, and it dries in the end.
#[test]
fn the_ground_keeps_what_fell() {
    let soaked = weather_now(&standing(WeatherKind::Rain, 1.0), 0.0);
    assert_eq!((soaked.wet, soaked.cover), (1.0, 0.0));
    let damp = weather_now(&standing(WeatherKind::Rain, 0.2), 0.0);
    assert!(damp.wet > 0.0 && damp.wet < 1.0, "{damp:?}");
    let snowed = weather_now(&standing(WeatherKind::Snow, 1.0), 0.0);
    assert_eq!((snowed.cover, snowed.wet), (1.0, 0.0));
    assert_eq!(weather_now(&standing(WeatherKind::Fog, 1.0), 0.0).wet, 0.0);

    let turning = Weather {
        kind: WeatherKind::Rain,
        intensity: Fp(1.0),
        spell_minutes: Fp(10.0),
    };
    let t0 = 1_760_000_000.0;
    let (mut wet_after, mut dried) = (false, false);
    let mut last_rain = f64::NEG_INFINITY;
    // Every fifteen seconds through some five hundred spells.
    for step in 0..20_000 {
        let t = t0 + f64::from(step) * 15.0;
        let now = weather_now(&turning, t);
        // Never drier than the rain now falling makes it, but for the trace
        // under which the ground holds nothing.
        assert!(
            now.wet + cfg::GROUND_TRACE >= wet_under(now.rain),
            "{t}: {now:?}"
        );
        if now.rain > 0.0 {
            last_rain = t;
        } else if t - last_rain < 60.0 && now.wet > 0.3 {
            wet_after = true;
        } else if now.wet < 0.02 {
            dried = true;
        }
    }
    assert!(wet_after, "never wet after the rain");
    assert!(dried, "never dried");
}

/// What falls: the volume's uniforms carry the rain, the snow and the wind,
/// and a dry room's volume falls nothing; the block is 16-byte aligned, as
/// WebGL2 needs; the mesh holds a quad a drop, their ranks spread evenly so
/// that a share of the weather is that share of the drops.
#[test]
fn what_falls_is_the_weather_on_the_wind() {
    use bevy::render::render_resource::ShaderType;
    assert_eq!(PrecipitationUniforms::min_size().get() % 16, 0);

    let env = Environment {
        cloud_wind_dir: crate::pds::Fp2([3.0, 4.0]),
        cloud_speed: Fp(5.0),
        ..Environment::default()
    };
    let u = uniforms(&env, &amounts(WeatherKind::Rain, 0.7));
    assert_eq!((u.fall.x, u.fall.y), (0.7, 0.0));
    assert!((u.fall.z - 3.0).abs() < 1e-5 && (u.fall.w - 4.0).abs() < 1e-5);
    let dry = uniforms(&env, &WeatherNow::default());
    assert_eq!((dry.fall.x, dry.fall.y), (0.0, 0.0));

    let drops = 1_000;
    let mesh = volume_mesh(drops);
    assert_eq!(mesh.count_vertices(), drops as usize * 4);
    let Some(bevy::mesh::VertexAttributeValues::Float32x4(data)) =
        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("the drops' data");
    };
    let below_half = data.iter().step_by(4).filter(|d| d[0] < 0.5).count();
    assert!((below_half as i32 - 500).abs() <= 1, "{below_half}");
}
