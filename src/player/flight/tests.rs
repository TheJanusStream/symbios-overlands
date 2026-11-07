//! The flight prototype's model and its landings (#1861), flown on the
//! stand-in bird through the game's own systems and avian, stepped by hand
//! ([`FlightBench`]): deterministic, as the rotorcraft's and the airplane's
//! benches are.

use super::*;
use crate::pds::Fp;
use crate::pds::avatar::AvatarRecord;
use crate::player::sim::{BENCH_HZ, FlightBench};

/// The stand-in bird's whole avatar: its generator body on the on-foot
/// preset with a flight section (`tests/fixtures/winged`).
pub(crate) const BIRD_AVATAR: &str =
    include_str!("../../../tests/fixtures/winged/standin_bird_avatar.json");

/// The stand-in bird, as its owner's client installs it.
pub(crate) fn bird() -> AvatarRecord {
    let mut record: AvatarRecord = serde_json::from_str(BIRD_AVATAR).expect("the bird decodes");
    record.sanitize();
    record
}

/// The bird, flying `flight`'s numbers instead of its own.
fn bird_flying(change: impl FnOnce(&mut FlightParams)) -> AvatarRecord {
    let mut record = bird();
    let LocomotionConfig::Humanoid(p) = &mut record.locomotion else {
        panic!("the bird walks on foot");
    };
    change(p.flight.as_mut().expect("the bird flies"));
    record.sanitize();
    record
}

/// The bird's own flight numbers.
fn params(record: &AvatarRecord) -> (FlightParams, Wings) {
    let LocomotionConfig::Humanoid(p) = &record.locomotion else {
        panic!("the bird walks on foot");
    };
    let flight = p.flight.clone().expect("the bird flies");
    let wings = Wings::of(&flight, p);
    (flight, wings)
}

/// A bench with the bird at `at`, already flying in `mode` toward -Z at
/// `velocity`, its nose `pitch` up.
fn launched(
    record: &AvatarRecord,
    at: Vec3,
    mode: FlightMode,
    pitch: f32,
    velocity: Vec3,
) -> FlightBench {
    let mut bench = FlightBench::new(record, at);
    let flight = Flight {
        mode,
        pitch,
        ..Flight::default()
    };
    bench.set_flight(flight);
    bench.set_rotation(flight.rotation());
    bench.set_velocity(velocity);
    bench
}

/// Step `seconds` of physics, calling `each` after every step.
fn fly(bench: &mut FlightBench, seconds: f32, mut each: impl FnMut(&FlightBench)) {
    let steps = (seconds * BENCH_HZ as f32).round() as usize;
    for _ in 0..steps {
        bench.step();
        each(bench);
    }
}

/// How high the bird's feet are over the bench floor (m).
fn feet_over_floor(bench: &FlightBench, wings: &Wings) -> f32 {
    bench.position().y - wings.feet
}

/// **The fixture is a bird that stands** (#1861): its record decodes and is
/// settled as given - the sanitiser moves nothing - and its drawn feet are
/// its capsule's bottom, where the walk stands it. Set down, it stands there on
/// its feet.
#[test]
fn the_stand_in_bird_stands_on_its_capsule() {
    let raw: AvatarRecord = serde_json::from_str(BIRD_AVATAR).expect("the bird decodes");
    let record = bird();
    assert_eq!(record, raw, "the sanitiser moved the fixture");
    let visuals = record.body.visuals().expect("a generator body");
    let visuals_fixture: crate::pds::Generator = serde_json::from_str(include_str!(
        "../../../tests/fixtures/winged/standin_bird_visuals.json"
    ))
    .expect("the bird's body decodes");
    assert_eq!(&visuals_fixture, visuals, "the two fixtures disagree");
    let LocomotionConfig::Humanoid(p) = &record.locomotion else {
        panic!("the bird walks on foot");
    };
    let bounds = crate::catalogue::items::measure::tree_bounds(visuals).expect("it draws");
    let feet = -p.total_height() * 0.5;
    assert!(
        (bounds.min.y - feet).abs() < 0.01,
        "its drawn feet stand at {}, its capsule's bottom at {feet}",
        bounds.min.y
    );
    // Its wings reach out either side, and its beak forward.
    assert!(bounds.max.x > 0.7 && bounds.min.x < -0.7, "{bounds:?}");
    assert!(bounds.min.z < -0.4, "{bounds:?}");

    let mut bench = FlightBench::set_down(&record, 0.0, 0.0);
    fly(&mut bench, 1.0, |_| {});
    assert_eq!(bench.flight().mode(), FlightMode::Ground);
    assert!(bench.underside().abs() < 0.03, "{}", bench.underside());
    assert!(bench.velocity().length() < 0.05, "{}", bench.velocity());
}

/// **Space on the ground takes off, and holding it climbs** (#1861): the leap
/// and the take-off's beats lift the bird, it flaps while the key is held,
/// each beat spends stamina, and it climbs away from the ground.
#[test]
fn a_take_off_leaps_and_flapping_climbs() {
    let record = bird();
    let (_, wings) = params(&record);
    let mut bench = FlightBench::set_down(&record, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    let start = bench.position();
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    bench.step();
    assert_eq!(bench.flight().mode(), FlightMode::Takeoff);
    assert!(bench.velocity().y > 2.0, "the leap: {}", bench.velocity());
    let mut modes = vec![];
    fly(&mut bench, 3.0, |bench| {
        let mode = bench.flight().mode();
        if modes.last() != Some(&mode) {
            modes.push(mode);
        }
    });
    let flight = bench.flight();
    let climbed = feet_over_floor(&bench, &wings) - (start.y - wings.feet);
    let travelled = (bench.position() - start).with_y(0.0).length();
    assert_eq!(modes, [FlightMode::Takeoff, FlightMode::Flap], "{modes:?}");
    // About a metre a second at the defaults: `beat_speed` is the lever.
    assert!(climbed > 2.0, "climbed {climbed} m in 3 s");
    assert!(travelled > 8.0, "flew {travelled} m forward");
    // The take-off's two, then one a beat's period from its end.
    assert!(flight.beats() >= 9, "{} beats", flight.beats());
    let spent = flight.beats() as f32 * wings.beat_cost;
    assert!(
        (1.0 - flight.stamina() - spent).abs() < 1e-3,
        "{} left after {} beats",
        flight.stamina(),
        flight.beats()
    );
}

/// **A hands-off glide flies its polar** (#1861): from a glide begun near
/// its trim, the bird settles at its trim speed and sinks at `trim_speed /
/// glide_ratio`, travelling `glide_ratio` metres for each it gives up.
#[test]
fn a_glide_sinks_as_its_polar_says() {
    let record = bird();
    let (flight, wings) = params(&record);
    let start = Vec3::new(0.0, 80.0, 120.0);
    let mut bench = launched(
        &record,
        start,
        FlightMode::Glide,
        wings.trim_pitch(5.0),
        Vec3::new(0.0, -1.0, -5.0),
    );
    fly(&mut bench, 6.0, |_| {});
    let (settled, at) = (bench.position(), bench.elapsed());
    fly(&mut bench, 8.0, |bench| {
        assert_eq!(bench.flight().mode(), FlightMode::Glide);
    });
    let seconds = (bench.elapsed() - at) as f32;
    let fell = settled.y - bench.position().y;
    let went = (bench.position() - settled).with_y(0.0).length();
    let sink = fell / seconds;
    let speed = bench.velocity().length();
    let expected = flight.trim_sink();
    assert!(
        (sink - expected).abs() < 0.12 * expected,
        "sinks {sink} m/s, the polar {expected}"
    );
    assert!(
        (speed - flight.trim_speed.0).abs() < 0.08 * flight.trim_speed.0,
        "flies {speed} m/s, trim {}",
        flight.trim_speed.0
    );
    let ratio = went / fell;
    assert!(
        (ratio - flight.glide_ratio.0).abs() < 0.12 * flight.glide_ratio.0,
        "glides {ratio}:1, the polar {}:1",
        flight.glide_ratio.0
    );
    assert!(!bench.flight().stalled());
}

/// **A stall drops the nose and dives back to flying, never a plain fall**
/// (#1861): slowed to a walk in the air, the bird's wings stall, its nose
/// goes down, it dives, and it is flying again in a few seconds - forward,
/// with its fall caught far short of a stone's.
#[test]
fn a_stall_drops_the_nose_and_flies_again() {
    let record = bird();
    let (flight, _) = params(&record);
    let mut bench = launched(
        &record,
        Vec3::new(0.0, 100.0, 0.0),
        FlightMode::Glide,
        0.3,
        Vec3::new(0.0, 0.0, -1.0),
    );
    bench.step();
    assert!(bench.flight().stalled(), "slow wings stall");
    let mut steepest = 0.0f32;
    let mut lowest_nose = 0.0f32;
    fly(&mut bench, 0.75, |bench| {
        lowest_nose = lowest_nose.min(bench.flight().attitude().1);
    });
    assert!(lowest_nose < -0.5, "the nose only went to {lowest_nose}");
    let mut recovered = None;
    fly(&mut bench, 4.0, |bench| {
        steepest = steepest.min(bench.velocity().y);
        if recovered.is_none() && !bench.flight().stalled() {
            recovered = Some(bench.elapsed());
        }
    });
    let recovered = recovered.expect("it never flew again");
    assert!(recovered < 3.0, "flying again only at {recovered} s");
    // A stone dropped for the same 4.8 s falls at 47 m/s.
    assert!(steepest > -10.0, "it fell at {steepest} m/s");
    let over_ground = bench.velocity().with_y(0.0).length();
    assert!(
        over_ground > flight.stall_speed.0,
        "{over_ground} m/s forward at the end"
    );
}

/// **A held bank turns the bird** (#1861): D rolls it to its steepest bank at
/// its roll rate - the chassis' right wing visibly down - and the bank turns
/// its heading and its path to the right; let go, the wings level.
#[test]
fn a_bank_turns_and_levels() {
    let record = bird();
    let (flight, wings) = params(&record);
    let mut bench = launched(
        &record,
        Vec3::new(0.0, 100.0, 0.0),
        FlightMode::Glide,
        wings.trim_pitch(flight.trim_speed.0),
        Vec3::new(0.0, -1.3, -6.5),
    );
    bench.hold(&[KeyCode::KeyD]);
    let roll_in = flight.max_bank.0 / flight.roll_rate.0;
    fly(&mut bench, roll_in + 0.1, |_| {});
    let (_, _, bank) = bench.flight().attitude();
    assert!(
        (bank - wings.max_bank).abs() < 1e-3,
        "banked {} deg after {roll_in} s",
        bank.to_degrees()
    );
    let right_wing = bench.rotation() * Vec3::X;
    assert!(
        right_wing.y < -0.7,
        "the chassis does not show the bank: its right wing points {right_wing}"
    );
    fly(&mut bench, 0.6, |_| {});
    let path = bench.velocity().with_y(0.0).normalize();
    assert!(path.x > 0.5, "the path turned to {path}, not right");
    let heading = bench.flight().attitude().0;
    // Right of -Z is a heading below zero, wrapped: turned well round.
    let turned = std::f32::consts::TAU - heading;
    assert!(turned > 0.8, "turned {} deg", turned.to_degrees());
    assert!(!bench.flight().stalled());
    bench.hold(&[]);
    fly(&mut bench, roll_in + 0.1, |_| {});
    let (_, _, bank) = bench.flight().attitude();
    assert!(bank.abs() < 1e-3, "still banked {}", bank.to_degrees());
}

/// **Stamina drains with beats and comes back gliding** (#1861): a held flap
/// beats until the stamina is spent, then the bird glides with the key still
/// held and no beat comes; let go and gliding, it comes back at the record's
/// rate, and over a quarter it flaps again.
#[test]
fn stamina_drains_and_recovers() {
    let record = bird();
    let (flight, wings) = params(&record);
    let mut bench = launched(
        &record,
        Vec3::new(0.0, 150.0, 200.0),
        FlightMode::Flap,
        0.1,
        Vec3::new(0.0, 0.0, -6.5),
    );
    bench.hold(&[KeyCode::Space]);
    let mut spent_at = None;
    fly(&mut bench, 12.0, |bench| {
        if spent_at.is_none() && bench.flight().stamina() < wings.beat_cost {
            spent_at = Some(bench.elapsed());
        }
    });
    let spent_at = spent_at.expect("the stamina never ran out");
    let expected = flight.stamina_beats.0 / flight.wingbeat_rate.0;
    assert!(
        (spent_at as f32 - expected).abs() < 1.0,
        "spent at {spent_at} s, {expected} s of beats"
    );
    // Still held, spent: it glides, and no beat comes.
    let beats = bench.flight().beats();
    fly(&mut bench, 0.5, |bench| {
        assert_eq!(bench.flight().mode(), FlightMode::Glide);
    });
    assert_eq!(bench.flight().beats(), beats, "a spent body beat");
    bench.hold(&[]);
    let before = bench.flight().stamina();
    fly(&mut bench, 2.0, |_| {});
    let gained = bench.flight().stamina() - before;
    let rate = 1.0 / flight.stamina_recovery.0;
    assert!(
        (gained - 2.0 * rate).abs() < 0.02,
        "gained {gained} in 2 s gliding"
    );
    bench.hold(&[KeyCode::Space]);
    fly(&mut bench, 0.5, |_| {});
    assert!(bench.flight().beats() > beats, "rested, it does not flap");
}

/// **A body with no flight power cannot flap, but leaps and glides**
/// (#1861): Space takes it off on a stronger leap than a bird's, holding it
/// in the air beats nothing, and from a height it glides far out.
#[test]
fn a_glider_cannot_flap_but_glides() {
    let glider = bird_flying(|flight| flight.flight_power = Fp(0.0));
    let (flight, wings) = params(&glider);
    assert!(!flight.can_flap());
    let mut bench = FlightBench::set_down(&glider, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    let mut apex = 0.0f32;
    let mut modes = vec![];
    fly(&mut bench, 1.5, |bench| {
        apex = apex.max(feet_over_floor(bench, &wings));
        let mode = bench.flight().mode();
        if modes.last() != Some(&mode) {
            modes.push(mode);
        }
    });
    assert!(!modes.contains(&FlightMode::Flap), "{modes:?}");
    assert_eq!(bench.flight().beats(), 0);
    assert!(apex > 0.8, "leapt only {apex} m");
    let first_velocity = flight.takeoff_speed.0 * GLIDER_LEAP;
    assert!(first_velocity > flight.takeoff_speed.0);

    // From a height: dropped, it opens its wings and glides away.
    let start = Vec3::new(0.0, 60.0, 100.0);
    let mut bench = FlightBench::new(&glider, start);
    bench.hold(&[KeyCode::Space]);
    fly(&mut bench, 10.0, |bench| {
        assert_ne!(bench.flight().mode(), FlightMode::Flap);
    });
    assert_eq!(bench.flight().mode(), FlightMode::Glide);
    assert_eq!(bench.flight().beats(), 0);
    let fell = start.y - bench.position().y;
    let went = (bench.position() - start).with_y(0.0).length();
    assert!(went > 2.5 * fell, "{went} m out for {fell} m down");
}

/// The bench with a static block whose top is at `top`, `half` its
/// half-extents across, centred at `(x, z)`.
fn with_top(bench: &mut FlightBench, top: f32, x: f32, z: f32, half: Vec3) {
    bench.block(
        Transform::from_translation(Vec3::new(x, top - half.y, z)),
        half,
    );
}

/// Glide the bird hands-off from `at` toward -Z for `seconds`: the modes it
/// went through, in order.
fn glide_in(bench: &mut FlightBench, seconds: f32) -> Vec<FlightMode> {
    let mut modes = vec![];
    fly(bench, seconds, |bench| {
        let mode = bench.flight().mode();
        if modes.last() != Some(&mode) {
            modes.push(mode);
        }
    });
    modes
}

/// **A glide onto a roof lands on its top and stands there** (#1861): the
/// bird glides down onto a flat roof, flares over it, lands on it rather
/// than the ground below, and ends on its feet, at rest, on the ground mode.
/// A branch's top - a beam narrower than the bird, passed beside within the
/// capture radius - is landed on the same way.
#[test]
fn a_glide_lands_on_a_roof_top() {
    let record = bird();
    let (flight, wings) = params(&record);
    let top = 3.0;
    let velocity = Vec3::new(0.0, -flight.trim_sink(), -flight.trim_speed.0);
    for (name, half, aside) in [
        ("a roof", Vec3::new(3.0, 0.15, 4.0), 0.0),
        ("a branch's top", Vec3::new(0.12, 0.05, 3.0), 0.3),
    ] {
        let mut bench = launched(
            &record,
            Vec3::new(aside, top + wings.feet + 2.0, 10.0),
            FlightMode::Glide,
            wings.trim_pitch(flight.trim_speed.0),
            velocity,
        );
        with_top(&mut bench, top, 0.0, 0.0, half);
        let modes = glide_in(&mut bench, 5.0);
        assert!(
            modes.starts_with(&[FlightMode::Glide, FlightMode::Flare, FlightMode::Land]),
            "{name}: {modes:?}"
        );
        assert_eq!(bench.flight().mode(), FlightMode::Ground, "{name}");
        let at = bench.position();
        assert!(
            (bench.underside() - top).abs() < 0.05,
            "{name}: stands at {} over a top at {top}",
            bench.underside()
        );
        assert!(
            at.x.abs() < half.x + wings.capture && at.z.abs() < half.z,
            "{name}: off it, at {at}"
        );
        assert!(
            bench.velocity().length() < 0.1,
            "{name}: {}",
            bench.velocity()
        );
        let upright = bench.rotation() * Vec3::Y;
        assert!(upright.y > 0.999, "{name}: standing tilted, up {upright}");
    }
}

/// **The flare begins by time to contact, not by distance** (#1861): a slow
/// glide and a steep dive onto the same ground both flare with contact about
/// [`FLARE_TAU_S`] away - the dive from three times the height.
#[test]
fn the_flare_starts_by_time_to_contact() {
    let record = bird();
    let (flight, wings) = params(&record);
    let mut onsets = vec![];
    for (up, pitch, velocity, keys) in [
        (
            6.0,
            wings.trim_pitch(flight.trim_speed.0),
            Vec3::new(0.0, -flight.trim_sink(), -flight.trim_speed.0),
            vec![],
        ),
        (12.0, -0.6, Vec3::new(0.0, -5.0, -7.0), vec![KeyCode::KeyW]),
    ] {
        let mut bench = launched(
            &record,
            Vec3::new(0.0, up, 100.0),
            FlightMode::Glide,
            pitch,
            velocity,
        );
        bench.hold(&keys);
        let mut before = (feet_over_floor(&bench, &wings), bench.velocity());
        let mut onset = None;
        fly(&mut bench, 8.0, |bench| {
            if onset.is_none() && bench.flight().mode() == FlightMode::Flare {
                onset = Some(before);
            }
            before = (feet_over_floor(bench, &wings), bench.velocity());
        });
        let (height, moving) = onset.expect("it never flared");
        onsets.push((height, height / -moving.y));
    }
    for (height, tau) in &onsets {
        assert!(
            (tau - FLARE_TAU_S).abs() < 0.04,
            "flared {tau} s from contact, {height} m up: {onsets:?}"
        );
    }
    assert!(
        onsets[1].0 > 2.5 * onsets[0].0,
        "the dive flared no higher than the glide: {onsets:?}"
    );
}

/// **Shift folds the wings and dives** (#1861): the nose goes down, the bird
/// gathers speed far past its trim, and let go it opens and glides again.
#[test]
fn shift_folds_the_wings_and_dives() {
    let record = bird();
    let (flight, wings) = params(&record);
    let mut bench = launched(
        &record,
        Vec3::new(0.0, 150.0, 0.0),
        FlightMode::Glide,
        wings.trim_pitch(flight.trim_speed.0),
        Vec3::new(0.0, -1.3, -6.5),
    );
    bench.hold(&[KeyCode::ShiftLeft]);
    fly(&mut bench, 2.0, |bench| {
        assert_eq!(bench.flight().mode(), FlightMode::Dive);
    });
    assert!(bench.flight().attitude().1 < -0.9);
    let speed = bench.velocity().length();
    assert!(
        speed > 2.0 * flight.trim_speed.0,
        "dived only to {speed} m/s"
    );
    bench.hold(&[]);
    bench.step();
    assert_eq!(bench.flight().mode(), FlightMode::Glide);
}

/// **A body walking off a ledge opens its wings** (#1861), and a body whose
/// record has no flight walks off it and falls as a person does.
#[test]
fn a_ledge_opens_the_wings() {
    let record = bird();
    let mut bench = FlightBench::new(&record, Vec3::new(0.0, 30.0, 0.0));
    fly(&mut bench, 0.5, |_| {});
    assert_eq!(bench.flight().mode(), FlightMode::Glide);

    let mut walker = record.clone();
    let LocomotionConfig::Humanoid(p) = &mut walker.locomotion else {
        panic!("on foot");
    };
    p.flight = None;
    let mut bench = FlightBench::new(&walker, Vec3::new(0.0, 30.0, 0.0));
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    fly(&mut bench, 1.0, |bench| {
        assert_eq!(bench.flight().mode(), FlightMode::Ground);
    });
    // Falling a second, through its record's linear damping.
    assert!(bench.velocity().y < -8.0, "{}", bench.velocity());
}

/// **A body without flight jumps on Space, as it always did** (#1861).
#[test]
fn a_body_without_flight_jumps() {
    let mut walker = bird();
    let LocomotionConfig::Humanoid(p) = &mut walker.locomotion else {
        panic!("on foot");
    };
    p.flight = None;
    let jump = p.jump_impulse.0 / p.mass.0;
    let mut bench = FlightBench::set_down(&walker, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    bench.tap();
    bench.step();
    assert_eq!(bench.flight().mode(), FlightMode::Ground);
    assert!(
        (bench.velocity().y - (jump - 9.81 / BENCH_HZ as f32)).abs() < 0.1,
        "{} against a jump of {jump}",
        bench.velocity().y
    );
}

/// **The modes' wire codes are append-only** (#1861): a peer reads a mode by
/// its code, so each keeps the one it shipped with, and a code past the last
/// is one this build does not know.
#[test]
fn the_modes_wire_codes_are_pinned() {
    let codes: Vec<(u8, &str)> = FlightMode::ALL
        .iter()
        .map(|mode| (mode.wire(), mode.word()))
        .collect();
    assert_eq!(
        codes,
        [
            (0, "ground"),
            (1, "takeoff"),
            (2, "flap"),
            (3, "glide"),
            (4, "dive"),
            (5, "flare"),
            (6, "land"),
        ]
    );
    for mode in FlightMode::ALL {
        assert_eq!(FlightMode::from_wire(mode.wire()), Some(mode));
    }
    assert_eq!(FlightMode::from_wire(7), None);
    let message = crate::protocol::OverlandsMessage::Flight {
        mode: FlightMode::Glide.wire(),
    };
    let bytes = message.to_chunk_bytes().expect("serializes");
    assert!(matches!(
        crate::protocol::OverlandsMessage::from_chunk_bytes(&bytes),
        Some(crate::protocol::OverlandsMessage::Flight { mode: 3 })
    ));
}

/// **The physics holds every rotation while the flight writes it, and gives
/// a lying body its turn back on its feet** (#1861): the locks follow the
/// mode through `sync_chassis`, not only the record.
#[test]
fn the_locks_follow_the_mode() {
    let record = bird();
    let mut bench = FlightBench::set_down(&record, 0.0, 0.0);
    fly(&mut bench, 0.3, |_| {});
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    fly(&mut bench, 0.3, |_| {});
    let locked = bench
        .world_mut()
        .query_filtered::<&LockedAxes, With<LocalPlayer>>()
        .single(bench.world_mut())
        .expect("locked")
        .to_bits();
    let all = LockedAxes::new()
        .lock_rotation_x()
        .lock_rotation_y()
        .lock_rotation_z()
        .to_bits();
    assert_eq!(locked, all);

    // A beast lying along its capsule, given wings: level on its feet, held
    // whole in the air.
    let mut beast = record.clone();
    let LocomotionConfig::Humanoid(p) = &mut beast.locomotion else {
        panic!("on foot");
    };
    p.shape = crate::pds::avatar::OnFootShape::Lying {
        height: Fp(0.5),
        ahead: Fp(0.0),
    };
    let level = LockedAxes::new()
        .lock_rotation_x()
        .lock_rotation_z()
        .to_bits();
    let locks = |bench: &mut FlightBench| {
        let world = bench.world_mut();
        world
            .query_filtered::<&LockedAxes, With<LocalPlayer>>()
            .single(world)
            .expect("locked")
            .to_bits()
    };
    let mut bench = FlightBench::set_down(&beast, 0.0, 0.0);
    fly(&mut bench, 0.3, |_| {});
    assert_eq!(locks(&mut bench), level, "on its feet");
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    fly(&mut bench, 0.3, |_| {});
    assert!(bench.flight().mode().airborne());
    assert_eq!(locks(&mut bench), all, "in the air");
    bench.hold(&[]);
    fly(&mut bench, 4.0, |_| {});
    assert_eq!(bench.flight().mode(), FlightMode::Ground);
    assert_eq!(locks(&mut bench), level, "back on its feet");
}

/// **The model is a pure step** (#1861): no physics is needed to read what a
/// mode does with its keys, and a hands-off glide's nose is the polar's
/// glide angle at its trim speed.
#[test]
fn the_hands_off_nose_is_the_polars() {
    let (flight, wings) = params(&bird());
    let glide = -(1.0 / flight.glide_ratio.0).asin();
    assert!((wings.trim_pitch(flight.trim_speed.0) - glide).abs() < 1e-6);
    assert!(wings.trim_pitch(0.5 * flight.trim_speed.0) < glide);
    assert!(wings.trim_pitch(1.5 * flight.trim_speed.0) > glide);
    // On its feet with no press, the walk moves it.
    let standing = Reading {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        facing: 1.0,
        on_ground: true,
        resting: true,
        approach: None,
        in_water: false,
        gravity: 9.81,
    };
    let (next, moved) =
        Flight::default().stepped(&wings, FlightKeys::default(), standing, 1.0 / 64.0);
    assert_eq!((next.mode(), moved), (FlightMode::Ground, None));
    // Into water from the air, it swims.
    let (next, moved) = Flight {
        mode: FlightMode::Glide,
        ..Flight::default()
    }
    .stepped(
        &wings,
        FlightKeys::default(),
        Reading {
            in_water: true,
            on_ground: false,
            resting: false,
            ..standing
        },
        1.0 / 64.0,
    );
    assert_eq!((next.mode(), moved), (FlightMode::Ground, None));
}

/// `record` set down on the bench floor and stood half a second, then taken
/// off: the lowest its collider reaches in the pose the flight writes on the
/// take-off's step, before avian moves it (m over the floor), and the nose
/// the flight set on that step (radians).
fn take_off(record: &AvatarRecord) -> (f32, f32) {
    let mut bench = FlightBench::set_down(record, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    assert_eq!(
        bench.flight().mode(),
        FlightMode::Ground,
        "the premise: it stands"
    );
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    let mut lowest = f32::NAN;
    bench.step_reading(|world| {
        let mut bodies =
            world.query_filtered::<(&Collider, &Position, &Rotation), With<LocalPlayer>>();
        let (collider, position, rotation) = bodies.single(world).expect("one body");
        lowest = collider.aabb(position.0, rotation.0).min.y;
    });
    assert_eq!(
        bench.flight().mode(),
        FlightMode::Takeoff,
        "the premise: it leapt"
    );
    (lowest, bench.flight().attitude().1)
}

/// **A lying winged body takes off without its tail in the ground** (#1877):
/// its nose eases up at its pitch rate from level rather than being set to
/// the take-off's 20 degrees on the leap's step, which - turned about the
/// capsule's middle - wrote a bat's and a glider's tail into the ground it
/// leapt from. The pose the take-off step writes reaches no lower than the
/// body stood.
#[test]
fn a_lying_winged_body_takes_off_without_its_tail_in_the_ground() {
    use crate::pds::avatar::locomotion::{default_winged, fitted_record};
    use symbios_avatar::WingedFamily;
    for family in [WingedFamily::Bat, WingedFamily::Glider] {
        let record = fitted_record(default_winged(family));
        let LocomotionConfig::Humanoid(p) = &record.locomotion else {
            panic!("on foot");
        };
        assert!(
            p.is_lying(),
            "the premise: a {family:?} lies along its capsule"
        );
        let (lowest, pitch) = take_off(&record);
        assert_eq!(pitch, 0.0, "{family:?}: its nose set up at once");
        assert!(
            lowest > -0.002,
            "{family:?}: its take-off wrote it {:.1} mm into the ground",
            -lowest * 1000.0
        );
    }
}

/// **A bat flies: it takes off and climbs on its flap key** (#1877): its
/// beat pushes as hard a second as `for_height` sizes it to, at the engine's
/// slower wingbeat. Pushed only as hard a beat, a bat's leap fell back before
/// its stall speed and it beat along the ground, climbing nothing in three
/// seconds.
#[test]
fn a_bat_takes_off_and_climbs() {
    use crate::pds::avatar::locomotion::{default_winged, fitted_record};
    let record = fitted_record(default_winged(symbios_avatar::WingedFamily::Bat));
    let (climb, speed, ..) = flies(&record);
    assert!(
        climb > 0.5 && speed > 3.0,
        "climbed {climb:.2} m/s at {speed:.2} m/s with the flap key held"
    );
}

/// **Every other body takes off as the prototype did** (#1877): the stand-in
/// bird, a bird-led rigged body standing in its upright capsule, and a beast
/// wearing a flight section set the take-off's nose at once.
#[test]
fn every_other_body_sets_its_take_off_nose_at_once() {
    use crate::pds::avatar::locomotion::{beast_record, default_winged, fitted_record};
    let mut beast = beast_record(symbios_avatar::plan::Family::Dog);
    if let LocomotionConfig::Humanoid(p) = &mut beast.locomotion {
        p.flight = Some(FlightParams::default());
    }
    for (name, record) in [
        ("the stand-in bird", bird()),
        (
            "a bird",
            fitted_record(default_winged(symbios_avatar::WingedFamily::Bird)),
        ),
        ("a beast", beast),
    ] {
        let (_, pitch) = take_off(&record);
        assert_eq!(pitch, TAKEOFF_PITCH, "{name}");
    }
}

/// **A lying winged body is righted over the whole landing** (#1877, the W6
/// end review): righted at three times its pitch rate, a bat or a glider
/// turned about 8.5 degrees a frame, which a body lying along the chassis
/// shows as about 100 mm a frame at its ends. Shared over what is left of
/// [`LAND_S`], a touch-down tilt turns no more a frame than that tilt over the
/// landing's frames, and none is left for `set_down` to zero; every other body
/// is righted as before.
#[test]
fn a_lying_winged_body_is_righted_over_the_whole_landing() {
    let (_, upright) = params(&bird());
    let dt = 1.0 / 60.0;
    let touched = Reading {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        facing: 0.0,
        on_ground: true,
        resting: true,
        approach: None,
        in_water: false,
        gravity: 9.81,
    };
    let (pitch0, bank0) = (0.87_f32, -0.4_f32);
    // Every frame's (pitch, bank) from touch-down until it is set down; the
    // last is set_down's.
    let land = |wings: &Wings| {
        let mut flight = Flight::default();
        flight.enter(FlightMode::Land);
        flight.pitch = pitch0;
        flight.bank = bank0;
        let mut attitudes = vec![(flight.pitch, flight.bank)];
        while flight.mode() == FlightMode::Land {
            assert!(attitudes.len() < 120, "never set down");
            flight = flight.stepped(wings, FlightKeys::default(), touched, dt).0;
            attitudes.push((flight.pitch, flight.bank));
        }
        attitudes
    };
    let turns = |attitudes: &[(f32, f32)]| -> Vec<f32> {
        attitudes
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).abs().max((w[1].1 - w[0].1).abs()))
            .collect()
    };

    let lying = land(&upright.lying(true));
    let share = pitch0.abs().max(bank0.abs()) * dt / LAND_S;
    let worst = turns(&lying).into_iter().fold(0.0_f32, f32::max);
    assert!(
        worst <= share * 1.08 + 1e-5,
        "a lying body turned {:.2} degrees in a frame, against its tilt over the landing's {:.2}",
        worst.to_degrees(),
        share.to_degrees()
    );
    let before = lying[lying.len() - 2];
    assert!(
        before.0.abs() < 1e-3 && before.1.abs() < 1e-3,
        "a lying body still tilted ({:.2}, {:.2}) degrees as it was set down",
        before.0.to_degrees(),
        before.1.to_degrees()
    );

    // An upright body keeps three times its pitch rate, frame for frame.
    let step = 3.0 * upright.pitch_rate.max(upright.roll_rate) * dt;
    for (i, w) in land(&upright).windows(2).enumerate() {
        let expected = (toward(w[0].0, 0.0, step), toward(w[0].1, 0.0, step));
        let set_down = w[1] == (0.0, 0.0);
        assert!(
            set_down || w[1] == expected,
            "frame {i}: an upright body righted to {:?}, not {expected:?}",
            w[1]
        );
    }
}

/// How `record` flies on the bench: a take-off and three seconds of the
/// flap key - its climb (m/s) and its speed (m/s) - then four seconds hands
/// off, the last two read - its glide's speed and sink (m/s) - and the mode
/// it ends in.
fn flies(record: &AvatarRecord) -> (f32, f32, f32, f32, FlightMode) {
    let mut bench = FlightBench::set_down(record, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    let start = bench.position();
    bench.hold(&[KeyCode::Space]);
    bench.tap();
    let mut highest = start.y;
    fly(&mut bench, 3.0, |bench| {
        highest = highest.max(bench.position().y)
    });
    let climb = (bench.position().y - start.y) / 3.0;
    let flapping = bench.velocity().length();
    bench.hold(&[]);
    fly(&mut bench, 2.0, |_| {});
    let from = bench.position();
    fly(&mut bench, 2.0, |_| {});
    let glide = (bench.position() - from) / 2.0;
    (
        climb,
        flapping,
        glide.with_y(0.0).length(),
        -glide.y,
        bench.flight().mode(),
    )
}

/// Every winged body the fit is asked of (#1877): each family's own, and
/// each cross of two at 0.49, 0.5 and 0.51 - either side of an even blend
/// and on it, where its second family leads - fitted as the editor fits it.
fn winged_bodies() -> Vec<(String, AvatarRecord)> {
    use crate::pds::avatar::locomotion::{default_winged, fitted_record};
    use symbios_avatar::{Archetype, WingedFamily};
    const FAMILIES: [WingedFamily; 3] =
        [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider];
    let mut bodies: Vec<(String, AvatarRecord)> = FAMILIES
        .iter()
        .map(|family| {
            (
                format!("{family:?}"),
                fitted_record(default_winged(*family)),
            )
        })
        .collect();
    for family in FAMILIES {
        for other in FAMILIES.into_iter().filter(|other| *other != family) {
            for blend in [0.49, 0.5, 0.51] {
                let mut body = default_winged(family);
                if let Archetype::Winged(params) = &mut body.archetype {
                    params.set_kind(family, Some(other), blend);
                    params.blend = blend;
                }
                bodies.push((format!("{family:?}x{other:?}@{blend}"), fitted_record(body)));
            }
        }
    }
    bodies
}

/// `record` set down on the bench floor and taken off with one press of its
/// key, nothing held, and flown until it is on its feet again - six seconds
/// at most: the highest its feet rose (m), how far it went (m), and whether
/// it glided on the way.
fn leaps(record: &AvatarRecord) -> (f32, f32, bool) {
    let (_, wings) = params(record);
    let mut bench = FlightBench::set_down(record, 0.0, 0.0);
    fly(&mut bench, 0.5, |_| {});
    let start = bench.position();
    bench.hold(&[]);
    bench.tap();
    bench.step();
    assert_eq!(
        bench.flight().mode(),
        FlightMode::Takeoff,
        "the premise: it leapt"
    );
    let (mut apex, mut glided) = (0.0f32, false);
    for _ in 0..(6.0 * BENCH_HZ) as usize {
        bench.step();
        apex = apex.max(feet_over_floor(&bench, &wings) - (start.y - wings.feet));
        glided |= bench.flight().mode() == FlightMode::Glide;
        if bench.flight().mode() == FlightMode::Ground {
            break;
        }
    }
    (
        apex,
        (bench.position() - start).with_y(0.0).length(),
        glided,
    )
}

/// **A body that cannot flap leaps strongly into a glide** (#1877, the
/// owner's decision on #1859: "the glider glide-only with a strong leap"):
/// from level ground the default glider and every cross a glider leads leap
/// over a metre, glide, and come down metres off. Its take-off is twice what
/// its stand gives a flapper, on top of the prototype's own glider's leap.
/// With a flapper's take-off a glider leapt 0.35 m and was down 0.61 s
/// later, never gliding.
#[test]
fn a_body_that_cannot_flap_leaps_strongly_into_a_glide() {
    let mut gliders = 0;
    for (name, record) in winged_bodies() {
        let (flight, _) = params(&record);
        if flight.can_flap() {
            continue;
        }
        gliders += 1;
        let (apex, went, glided) = leaps(&record);
        assert!(
            glided && apex > 1.0 && went > 3.0,
            "{name}: leapt {apex:.2} m and came down {went:.2} m off, gliding {glided}"
        );
    }
    assert!(
        gliders >= 7,
        "the premise: {gliders} bodies that cannot flap"
    );
}

/// **Every winged body that may flap clears its stall and climbs** (#1877):
/// each family and every cross that flaps at all - at or over
/// [`FlightParams::MIN_FLAP_POWER`] - climbs at least 0.3 m/s over three
/// seconds of its key from a take-off on level ground, flying faster than its
/// stall. A weak flapper's beat pushes as hard as a bat's power does and
/// tires it sooner. Pushed only by its own power, a cross a glider shares
/// half of flapped along at its stall: a bird-led one climbing -0.02 m/s at
/// 1.3 against a 3.2 stall, a bat-led one 0.2 at 1.8 against 2.0.
#[test]
fn every_winged_body_that_flaps_clears_its_stall_and_climbs() {
    let mut flappers = 0;
    for (name, record) in winged_bodies() {
        let (flight, _) = params(&record);
        if !flight.can_flap() {
            continue;
        }
        flappers += 1;
        let (climb, speed, ..) = flies(&record);
        assert!(
            climb >= 0.3 && speed > flight.stall_speed.0,
            "{name} (power {:.2}): climbed {climb:.2} m/s at {speed:.2} m/s against a {:.2} stall",
            flight.flight_power.0,
            flight.stall_speed.0
        );
    }
    assert!(flappers >= 12, "the premise: {flappers} bodies that flap");
}

/// **Probe, not a gate** (#1877): how every winged body flies - each family
/// and each cross at 0.49, 0.5 and 0.51 - as its fit sizes it: its flight
/// section, its climb and speed over three seconds of the flap key, its
/// glide hands off, and the leap of a take-off with nothing held. Prints a
/// table.
#[test]
#[ignore = "a probe: prints how the winged fits fly"]
fn probe_how_the_winged_fits_fly() {
    for (name, record) in winged_bodies() {
        let LocomotionConfig::Humanoid(p) = &record.locomotion else {
            panic!("on foot");
        };
        let f = p.flight.clone().expect("a winged fit flies");
        let (climb, flapping, speed, sink, ends) = flies(&record);
        let (apex, went, glided) = leaps(&record);
        println!(
            "{name}: {} power {:.2} trim {:.2} stall {:.2} beat {:.2} at {:.2} Hz, takeoff {:.2} | \
             climb {climb:.2} m/s at {flapping:.2}; glide {speed:.2} sinking {sink:.2}, ends {ends:?} | \
             leap apex {apex:.2} went {went:.2} glided {glided}",
            if p.is_lying() { "lying" } else { "upright" },
            f.flight_power.0,
            f.trim_speed.0,
            f.stall_speed.0,
            f.beat_speed.0,
            f.wingbeat_rate.0,
            f.takeoff_speed.0,
        );
    }
}
