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
        ..Weather::default()
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
        ..Weather::default()
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
        ..Weather::default()
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
            ..Weather::default()
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
        ..Weather::default()
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

/// Lightning (#1684): no strike without a storm; at a full storm each slot
/// strikes alike for every visitor, about its share of the slots, within
/// its slot's span, from a distance in range; a flash peaks at its strike
/// and is out a second later; and never more than two flashes fall in any
/// second - the pulses' onsets counted over two thousand slots of storm.
#[test]
fn lightning_strikes_on_the_clock_and_never_flashes_too_fast() {
    use super::lightning::{flash_at, strike_of};
    let storm = standing(WeatherKind::Storm, 1.0);
    let rain = standing(WeatherKind::Rain, 1.0);
    for slot in 0..1_000 {
        assert!(strike_of(&rain, slot).is_none());
    }
    let first = 440_000_000_i64;
    let (mut strikes, mut onsets) = (0, Vec::new());
    for slot in first..first + 2_000 {
        let Some(strike) = strike_of(&storm, slot) else {
            continue;
        };
        strikes += 1;
        assert_eq!(strike_of(&storm, slot), Some(strike));
        let range = cfg::storm::DISTANCE;
        assert!((range.0..=range.1).contains(&strike.distance));
        let start = slot as f64 * cfg::storm::SLOT_SECS;
        assert!(strike.at >= start);
        assert!(strike.at <= start + cfg::storm::SLOT_SECS - cfg::storm::SLOT_TAIL_SECS);
        assert!((flash_at(strike.at, &storm) - strike.light).abs() < 1e-3);
        assert_eq!(flash_at(strike.at + 1.0, &storm), 0.0, "out a second later");
        onsets.push(strike.at);
        if let Some((after, share)) = strike.second {
            assert!(share < 1.0);
            onsets.push(strike.at + after);
        }
    }
    let share = strikes as f32 / 2_000.0;
    assert!((share - cfg::storm::STRIKE_SHARE).abs() < 0.05, "{share}");
    onsets.sort_by(f64::total_cmp);
    for three in onsets.windows(3) {
        assert!(
            three[2] - three[0] > 1.0,
            "three flashes in a second: {three:?}"
        );
    }
}

/// The drawing holds the spacing whatever the schedule does (#1684): an
/// owner dragging the weather's sliders through a storm gives it a new
/// schedule every frame, and still no second holds three flashes - every
/// frame of ten minutes at 60 a second, each with a schedule of its own.
#[test]
fn a_storm_dragged_through_its_settings_never_flashes_too_fast() {
    use super::lightning::{may_draw, strike_of};
    let mut last = None;
    let mut onsets: Vec<f64> = Vec::new();
    let t0 = 1_760_000_000.0;
    for frame in 0..36_000_u32 {
        let now = t0 + f64::from(frame) / 60.0;
        // A new spell length each frame: a new salt, a new schedule.
        let weather = Weather {
            kind: WeatherKind::Storm,
            intensity: Fp(1.0),
            spell_minutes: Weather::spell_length(Fp(2.0 + frame as f32 * 0.37)),
            ..Weather::default()
        };
        let slot = (now / cfg::storm::SLOT_SECS).floor() as i64;
        for s in [slot - 1, slot] {
            let Some(strike) = strike_of(&weather, s).filter(|strike| strike.at <= now) else {
                continue;
            };
            if !may_draw(last.as_ref(), &strike) {
                continue;
            }
            if last != Some(strike) {
                last = Some(strike);
                onsets.push(strike.at);
                if let Some((after, _)) = strike.second {
                    onsets.push(strike.at + after);
                }
            }
        }
    }
    assert!(onsets.len() > 100, "the storm flashes: {}", onsets.len());
    onsets.sort_by(f64::total_cmp);
    for three in onsets.windows(3) {
        assert!(
            three[2] - three[0] > 1.0,
            "three flashes in a second: {three:?}"
        );
    }
}

/// Thunder (#1684): synthesised alike every time, finite, its loudest at
/// 0.9; a near one cracks in its first tenth of a second, and is shorter
/// than a far one's long roll.
#[test]
fn thunder_cracks_near_and_rolls_far() {
    use super::lightning::thunder_samples;
    let near = thunder_samples(true);
    let far = thunder_samples(false);
    assert_eq!(near, thunder_samples(true));
    assert!(far.len() > near.len());
    for clip in [&near, &far] {
        assert!(clip.iter().all(|s| s.is_finite()));
        let peak = clip.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.9).abs() < 1e-4, "{peak}");
    }
    let energy = |clip: &[f32], secs: f32| {
        let n = (secs * super::lightning::THUNDER_RATE as f32) as usize;
        clip[..n].iter().map(|s| s * s).sum::<f32>() / n as f32
    };
    assert!(
        energy(&near, 0.1) > 4.0 * energy(&far, 0.1),
        "a near one cracks"
    );
}

/// A flash adds light (#1684): no flash opens nothing; the same flash opens
/// the exposure the more the dimmer the light it falls on - a night's the
/// most, at most the cap.
#[test]
fn a_flash_adds_light_the_more_by_night() {
    use super::lightning::flash_stops;
    assert_eq!(flash_stops(0.0, 20_000.0), 0.0);
    let day = flash_stops(1.0, 20_000.0);
    let storm_day = flash_stops(1.0, 700.0);
    let night = flash_stops(1.0, 10.0);
    assert!(
        0.0 < day && day < storm_day && storm_day < night,
        "{day} {storm_day} {night}"
    );
    assert_eq!(night, cfg::storm::FLASH_MAX_STOPS);
    assert!(flash_stops(0.5, 700.0) < storm_day);
}

/// The Effects setting turns lightning down for anyone for whom flashing
/// is a problem (#1687): at night, where a flash of any size opens the
/// exposure by the full four stops, a held flash opens all four under Full,
/// a third of them under Reduced, and none under Off - and the light it
/// adds to the sky (whose zenith carries the flash to the clouds as well)
/// is all of it, a third and none. A share of the light alone would have
/// left Reduced opening four.
#[test]
fn the_effects_setting_softens_or_stops_the_flash() {
    use super::lightning::{FlashOverride, flash_stops};
    use crate::sky::{SkyBox, SkyMaterial};
    use crate::state::{EffectsIntensity, LocalSettings};
    use bevy::camera::Exposure;
    let night = 10.0;
    let drawn_under = |level: EffectsIntensity| {
        let mut app = App::new();
        let mut drawn = crate::drawn_environment::DrawnEnvironment::default();
        drawn.env.sun_illuminance = Fp(night);
        app.insert_resource(drawn)
            .init_resource::<Assets<SkyMaterial>>()
            .init_resource::<Assets<crate::clouds::CloudMaterial>>()
            .init_resource::<Assets<super::precipitation::PrecipitationMaterial>>()
            .insert_resource(FlashOverride(1.0))
            .insert_resource(LocalSettings {
                effects_intensity: level,
                ..Default::default()
            });
        super::lightning::register(&mut app);
        let sky = app
            .world_mut()
            .resource_mut::<Assets<SkyMaterial>>()
            .add(SkyMaterial::default());
        app.world_mut().spawn((MeshMaterial3d(sky.clone()), SkyBox));
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), crate::camera::WorldCamera))
            .id();
        app.update();
        let stops = app
            .world()
            .get::<Exposure>(camera)
            .map(|e| Exposure::default().ev100 - e.ev100);
        let lit = app
            .world()
            .resource::<Assets<SkyMaterial>>()
            .get(&sky)
            .map(|m| m.uniforms.zenith.w);
        (stops, lit)
    };
    let near = |got: Option<f32>, want: f32| got.is_some_and(|got| (got - want).abs() < 1e-4);
    let four = flash_stops(1.0, night);
    assert_eq!(
        four,
        cfg::storm::FLASH_MAX_STOPS,
        "a night flash opens them all"
    );
    let (stops, lit) = drawn_under(EffectsIntensity::Full);
    assert!(near(stops, four), "{stops:?}");
    assert!(near(lit, 1.0), "{lit:?}");
    let (stops, lit) = drawn_under(EffectsIntensity::Reduced);
    let share = cfg::storm::REDUCED_FLASH;
    assert!(near(stops, four * share), "{stops:?}");
    assert!(near(lit, share), "{lit:?}");
    let (stops, lit) = drawn_under(EffectsIntensity::Off);
    assert_eq!(stops, None);
    assert_eq!(lit, Some(0.0), "the sky is never lit");
}

/// How far into its weather a weather stood before there was a share
/// (#1735): [`severity_at`] as every release before drew it, kept to hold
/// the default share to the timeline every room drew.
fn severity_before_shares(weather: &Weather, unix_secs: f64) -> f32 {
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

/// **The default share draws the timeline every room drew** (#1735): every
/// value a spell can draw spreads to itself at the default share, to the
/// bit, and so every kind's severity there is what the formula before there
/// was a share gives - a thousand spells of each kind at three spell
/// lengths, four instants a spell. That formula reads today's salt and
/// spell values; the next test holds those to the old ones.
#[test]
fn the_default_share_draws_the_timeline_every_room_drew() {
    assert_eq!(1.0 - Weather::DEFAULT_SHARE, cfg::FALLS.0);
    assert_eq!(cfg::FALLS.0 * cfg::FALLS.0, cfg::CLOUDS_GATHER.0);
    for k in 0..(1_u32 << 24) {
        let value = k as f32 / (1_u32 << 24) as f32;
        assert_eq!(
            spread(value, Weather::DEFAULT_SHARE).to_bits(),
            value.to_bits(),
            "{value}"
        );
    }
    let t0 = 1_760_000_000.0;
    for kind in WeatherKind::ALL {
        for minutes in [2.0, 8.0, 45.5] {
            let weather = Weather {
                kind,
                intensity: Fp(0.8),
                spell_minutes: Weather::spell_length(Fp(minutes)),
                ..Weather::default()
            };
            for step in 0..4_000 {
                let t = t0 + f64::from(step) * f64::from(minutes) * 15.0 + 0.37;
                assert_eq!(
                    severity_at(&weather, t).to_bits(),
                    severity_before_shares(&weather, t).to_bits(),
                    "{kind:?} in {minutes}-minute spells at {t}"
                );
            }
        }
    }
}

/// **A share sets how much of the time a weather is here** (#1735): over
/// twenty thousand spells, something falls about the share's part of the
/// time and the clouds gather for longer round it; a rarer weather is here
/// only when a commoner one of its kind is, and a rare one still reaches
/// its heaviest; at none it never comes, and at all it never clears -
/// though between its spells it thins, at times to a trace.
#[test]
fn a_share_sets_how_much_of_the_time_a_weather_is_here() {
    let shares = [0.0, 0.125, 0.25, 0.5, 0.75, 0.9, 1.0];
    let at = |share: f32| Weather {
        kind: WeatherKind::Rain,
        intensity: Fp(1.0),
        spell_minutes: Fp(10.0),
        share: Fp(share),
    };
    let t0 = 1_760_000_000.0;
    let samples = 20_000 * 8;
    let (mut falls, mut gathers) = ([0_u32; 7], [0_u32; 7]);
    let mut heaviest = [false; 7];
    for step in 0..samples {
        let t = t0 + f64::from(step) * 75.0 + 1.3;
        let mut commoner = f32::NEG_INFINITY;
        for (i, &share) in shares.iter().enumerate() {
            let s = severity_at(&at(share), t);
            assert!(
                s >= commoner - 1e-6,
                "share {share} at {t}: {s} under {commoner}"
            );
            commoner = s;
            falls[i] += u32::from(s > cfg::FALLS.0);
            gathers[i] += u32::from(s > cfg::CLOUDS_GATHER.0);
            heaviest[i] |= s > cfg::FALLS.1;
        }
    }
    let part = |n: u32| n as f32 / samples as f32;
    assert_eq!(falls[0] + gathers[0], 0, "a weather here none of the time");
    assert!(
        part(falls[6]) > 0.999,
        "all of the time: {}",
        part(falls[6])
    );
    for (i, &share) in shares.iter().enumerate().skip(1).take(5) {
        let (fell, gathered) = (part(falls[i]), part(gathers[i]));
        assert!((fell - share).abs() < 0.03, "share {share} fell {fell}");
        assert!(
            gathered > fell + 0.05,
            "share {share}: clouds {gathered}, rain {fell}"
        );
        assert!(heaviest[i], "share {share} never at its heaviest");
    }
}

/// **The kinds before the dust storm keep their timelines** (#1735): each
/// one's salt, and its severity at three instants, as the code before the
/// dust storm drew them - blessed on that code, compiled on its own. A kind
/// put anywhere but last in [`WeatherKind::ALL`], or a change to the salt
/// or to a spell's value, moves every world's weather and its lightning,
/// and fails here.
#[test]
fn the_kinds_before_dust_keep_their_timelines() {
    let pinned: [(WeatherKind, u64, [u32; 3]); 5] = [
        (
            WeatherKind::Overcast,
            0x5629_9769_b887_b354,
            [0x3f18_7139, 0x3eef_40b8, 0x3f59_0466],
        ),
        (
            WeatherKind::Rain,
            0xce69_bc05_b5f9_edb0,
            [0x3f2c_f8aa, 0x3f21_bd38, 0x3ea7_fb55],
        ),
        (
            WeatherKind::Storm,
            0x39f1_dde7_a096_d9a8,
            [0x3e9b_19d6, 0x3e93_50dd, 0x3ed4_6ddb],
        ),
        (
            WeatherKind::Snow,
            0xe5d6_ba2b_6dcb_48fb,
            [0x3f0f_fe10, 0x3ec1_b49b, 0x3eec_8cc7],
        ),
        (
            WeatherKind::Fog,
            0x5d80_94a9_44b3_9347,
            [0x3f21_c510, 0x3f0f_42d8, 0x3e8a_aaa6],
        ),
    ];
    for (kind, salt_was, severities) in pinned {
        let weather = Weather {
            kind,
            intensity: Fp(0.8),
            spell_minutes: Fp(10.0),
            ..Weather::default()
        };
        assert_eq!(salt(&weather), salt_was, "{kind:?}'s salt");
        for (t, was) in [1_760_000_000.0, 1_760_000_123.456, 1_234_567_890.5]
            .into_iter()
            .zip(severities)
        {
            assert_eq!(severity_at(&weather, t).to_bits(), was, "{kind:?} at {t}");
        }
    }
}

/// **A share on the wire** (#1735): within 0 to 1 on the wire's grid, one
/// that is not a number the default; the default left off the wire, so a
/// record from before the setting reads back and writes as it was written;
/// another written and read back to the bit, a hair off the default written
/// as the default; and the sanitiser holds it as the wire does.
#[test]
fn a_share_is_held_on_the_grid_and_left_off_the_wire_at_the_default() {
    assert_eq!(Weather::share_of(Fp(1.5)).0, 1.0);
    assert_eq!(Weather::share_of(Fp(-0.2)).0, 0.0);
    assert_eq!(Weather::share_of(Fp(f32::NAN)).0, Weather::DEFAULT_SHARE);
    assert_eq!(Weather::share_of(Fp(0.123_456)), Fp(0.123_456).on_wire());
    assert!(
        serde_json::to_value(Weather::default())
            .expect("serialises")
            .get("share")
            .is_none()
    );
    let older = serde_json::json!({
        "kind": {"$type": "network.symbios.weather.rain"},
        "intensity": 5000,
        "spell_minutes": 100_000
    });
    let read: Weather = serde_json::from_value(older.clone()).expect("reads");
    assert_eq!(read.share.0, Weather::DEFAULT_SHARE);
    assert_eq!(serde_json::to_value(read).expect("serialises"), older);
    let rare = Weather {
        share: Weather::share_of(Fp(0.125)),
        ..read
    };
    let wire = serde_json::to_value(rare).expect("serialises");
    assert_eq!(wire["share"], 1250, "{wire}");
    let back: Weather = serde_json::from_value(wire).expect("reads back");
    assert_eq!(back, rare);
    let near = Weather {
        share: Fp(0.500_04),
        ..read
    };
    let not_a_number = Weather {
        share: Fp(f32::NAN),
        ..read
    };
    for default in [near, not_a_number] {
        assert!(
            serde_json::to_value(default)
                .expect("serialises")
                .get("share")
                .is_none(),
            "{default:?}"
        );
    }
    let mut env = Environment {
        weather: Some(Weather {
            share: Fp(7.0),
            ..read
        }),
        ..Environment::default()
    };
    env.sanitize();
    assert_eq!(env.weather.expect("kept").share.0, 1.0);
}

/// **A dust storm** (#1735): dust in the air from a light haze to a wall,
/// nothing falling and no cloud, the ground left dry, and the wind blowing
/// as a storm's without its lightning. Over the light and the air: the
/// visibility closed to the dust's, the horizon the dust's own warm hue
/// and the sky warmer, each about as bright as before but for what the dust
/// takes, so a night stays a night; the sun dimmed and reddened; the haze
/// taking no tint of its own, which Bevy's fog would turn red; the clouds
/// driven on its wind. Written under its own name.
#[test]
fn a_dust_storm_closes_the_air_warm_and_blows_without_lightning() {
    let full = amounts(WeatherKind::Dust, 1.0);
    assert_eq!(full.dust, 1.0);
    assert!(!full.falls() && full.overcast == 0.0 && full.storm == 0.0 && full.fog == 0.0);
    assert_eq!((full.wet, full.cover), (0.0, 0.0));
    assert_eq!(full.gusts(), 1.0);
    let light = amounts(WeatherKind::Dust, 0.0);
    assert_eq!(light.dust, cfg::DUST.dust.0);
    let turning = Weather {
        kind: WeatherKind::Dust,
        intensity: Fp(1.0),
        spell_minutes: Fp(10.0),
        ..Weather::default()
    };
    for step in 0..2_000 {
        let now = weather_now(&turning, 1_760_000_000.0 + f64::from(step) * 30.0);
        assert_eq!(
            (now.wet, now.cover, now.rain, now.snow),
            (0.0, 0.0, 0.0, 0.0)
        );
    }
    let standing_dust = standing(WeatherKind::Dust, 1.0);
    for slot in 0..1_000 {
        assert!(super::lightning::strike_of(&standing_dust, slot).is_none());
    }
    let wire = serde_json::to_value(standing_dust).expect("serialises");
    assert_eq!(wire["kind"]["$type"], "network.symbios.weather.dust");

    let env = Environment::default();
    let dusty = environment_in(&env, &full);
    assert!((dusty.fog_visibility.0 - cfg::DUST_VISIBILITY).abs() < 1.0);
    let warmth = |c: [f32; 3]| {
        let lin = linear(c);
        lin.x / lin.z.max(1e-6)
    };
    let horizon = |e: &Environment| [e.fog_color.0[0], e.fog_color.0[1], e.fog_color.0[2]];
    let (r, g, b) = (horizon(&dusty)[0], horizon(&dusty)[1], horizon(&dusty)[2]);
    assert!(
        r > g && g > b,
        "the horizon's hue is the dust's: {:?}",
        horizon(&dusty)
    );
    assert!(warmth(dusty.sky_color.0) > warmth(env.sky_color.0));
    let kept = luminance(dusty.sky_color.0) / luminance(env.sky_color.0);
    assert!(
        (kept - (1.0 - cfg::DUST_DARK)).abs() < 0.02,
        "the sky keeps its brightness but what the dust takes: {kept}"
    );
    assert!((dusty.sun_illuminance.0 / env.sun_illuminance.0 - cfg::DUST_SUN).abs() < 1e-4);
    assert!(warmth(dusty.sun_color.0) > warmth(env.sun_color.0));
    for tint in [dusty.fog_extinction.0, dusty.fog_inscattering.0] {
        let lin = linear(tint);
        assert!(
            (lin.x - lin.y).abs() < 1e-3 && (lin.y - lin.z).abs() < 1e-3,
            "the haze takes no tint of its own: {tint:?}"
        );
    }
    let windy = 1.0 + cfg::storm::WIND_GAIN;
    assert!((dusty.cloud_speed.0 / env.cloud_speed.0 - windy).abs() < 1e-4);
    assert_eq!(dusty.cloud_cover, env.cloud_cover, "no cloud of its own");

    let night = Environment {
        sky_color: Fp3([0.01, 0.012, 0.03]),
        ..env
    };
    let dusty_night = environment_in(&night, &full);
    assert!(luminance(dusty_night.sky_color.0) <= luminance(night.sky_color.0));
}

/// #1735 measurement: how much of the time something falls, and the clouds
/// gather, at each share - twenty thousand spells, eight instants a spell -
/// and how long a wet spell lasts on average, in spells.
#[test]
#[ignore = "probe for #1735: prints each share's measured time"]
fn probe_how_much_of_the_time_each_share_is_here() {
    for share in [0.0, 0.0625, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.9, 1.0] {
        let weather = Weather {
            kind: WeatherKind::Rain,
            intensity: Fp(1.0),
            spell_minutes: Fp(10.0),
            share: Fp(share),
        };
        let samples = 20_000 * 8;
        let (mut fell, mut gathered, mut heaviest, mut wet_runs) = (0_u32, 0_u32, 0_u32, 0_u32);
        let mut was_wet = false;
        for step in 0..samples {
            let s = severity_at(&weather, 1_760_000_000.0 + f64::from(step) * 75.0 + 1.3);
            let wet = s > cfg::FALLS.0;
            fell += u32::from(wet);
            gathered += u32::from(s > cfg::CLOUDS_GATHER.0);
            heaviest += u32::from(s > cfg::FALLS.1);
            wet_runs += u32::from(wet && !was_wet);
            was_wet = wet;
        }
        let part = |n: u32| n as f32 / samples as f32;
        let run = if wet_runs == 0 {
            0.0
        } else {
            fell as f32 / wet_runs as f32 / 8.0
        };
        println!(
            "SHARE {share:<6} falls {:.3} gathers {:.3} heaviest {:.3} | a wet run {run:.2} spells",
            part(fell),
            part(gathered),
            part(heaviest)
        );
    }
}

/// **Dust never turns a blue sky lilac** (#1735): from a deep blue and a
/// pale one, all the way to the dust's own colour, green is never the
/// least of the three - where it is, with red and blue above it, the sky is
/// a lilac, a mauve or a rose no dust shows. It greys first, then browns.
#[test]
fn dust_never_turns_a_blue_sky_lilac() {
    for sky in [[0.35, 0.55, 0.9], [0.45, 0.62, 0.85], [0.33, 0.52, 0.86]] {
        for step in 0..=100 {
            let share = step as f32 / 100.0;
            let [r, g, b] = dusted(sky, share);
            assert!(g >= r.min(b) - 1e-4, "{sky:?} at {share}: {:?}", [r, g, b]);
        }
        let full = dusted(sky, 1.0);
        let dust = cfg::DUST_COLOR;
        assert!(
            full[0] > full[1] && full[1] > full[2],
            "{full:?} is the dust's hue"
        );
        assert!(
            (full[0] / full[2] - dust[0] / dust[2]).abs() < 0.3,
            "{full:?}"
        );
    }
}
