//! The airplane's flight model, law by law and on the flight bench (#1615).
//!
//! The bench tests fly the default record through what the owner will do
//! with her - park, take off, cruise, turn, stall, land, brake, taxi - on
//! the game's own systems over avian, gravity on.

use super::*;
use crate::pds::avatar::AvatarRecord;
use crate::player::sim::FlightBench;

/// A record wearing the default airplane.
fn airplane() -> AvatarRecord {
    let mut record = AvatarRecord::default_for_did("did:plc:airplane-bench");
    record.locomotion = LocomotionConfig::Airplane(Box::default());
    record
}

fn params() -> AirplaneParams {
    AirplaneParams::default()
}

fn run(bench: &mut FlightBench, keys: &[KeyCode], secs: f32) {
    bench.hold(keys);
    for _ in 0..(secs * 64.0).round() as usize {
        bench.step();
    }
}

/// A bench with her flying level at `speed` m/s, `height` m up, nose -Z,
/// her throttle at `throttle`.
fn flying(record: &AvatarRecord, height: f32, speed: f32, throttle: f32) -> FlightBench {
    let mut bench = FlightBench::new(record, Vec3::new(0.0, height, 200.0));
    bench.set_velocity(Vec3::new(0.0, 0.0, -speed));
    bench.set_throttle(throttle);
    bench
}

/// Her heading (degrees, positive to the left of -Z), bank and pitch.
fn attitude(bench: &FlightBench) -> (f32, f32, f32) {
    let rot = bench.rotation();
    let (forward, right, up) = (rot * Vec3::NEG_Z, rot * Vec3::X, rot * Vec3::Y);
    (
        (-forward.x).atan2(-forward.z).to_degrees(),
        bank_of(right, up).to_degrees(),
        forward.y.clamp(-1.0, 1.0).asin().to_degrees(),
    )
}

// ---------------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------------

/// Her lift coefficient: 1 at no angle, `c_max` at the stall, a straight
/// line between - through negative angles too - and past either stall it
/// falls away to [`STALL_LIFT`] of its edge.
#[test]
fn the_lift_curve_rises_to_the_stall_and_falls_past_it() {
    let c_max = 3.0;
    let stall = STALL_ANGLE_DEG.to_radians();
    let blend = STALL_BLEND_DEG.to_radians();
    assert_eq!(lift_coefficient(0.0, c_max), 1.0);
    assert!((lift_coefficient(stall, c_max) - c_max).abs() < 1e-5);
    assert!((lift_coefficient(stall * 0.5, c_max) - 2.0).abs() < 1e-5);
    let past = lift_coefficient(stall + blend, c_max);
    assert!((past - c_max * STALL_LIFT).abs() < 1e-4, "{past}");
    assert!(lift_coefficient(stall + 2.0 * blend, c_max) < c_max);
    let negative = lift_coefficient(-stall, c_max);
    assert!((negative - (2.0 - c_max)).abs() < 1e-5, "{negative}");
    assert!(lift_coefficient(-stall - blend, c_max).abs() < negative.abs());
}

/// The two speeds the record names mean what they say: at `cruise_speed`
/// with the nose on the path her lift is her weight, and at `min_airspeed`
/// it is her weight only at the stall angle.
#[test]
fn her_lift_carries_her_weight_at_cruise_level_and_at_the_stall() {
    let p = params();
    let wings = Wings::of(&p);
    let weight = p.mass.0 * G;
    let axes = [Vec3::NEG_Z, Vec3::X, Vec3::Y];
    let level = air_force(
        Vec3::new(0.0, 0.0, -wings.cruise),
        axes,
        p.mass.0,
        0.0,
        wings,
    );
    assert!(
        (level.force.y - weight).abs() < weight * 1e-3,
        "{} against {weight}",
        level.force.y
    );
    // At the stall speed, nose up the stall angle over the path.
    let stall = STALL_ANGLE_DEG.to_radians();
    let path = Vec3::new(0.0, -stall.sin(), -stall.cos()) * wings.stall;
    let air = air_force(path, axes, p.mass.0, 0.0, wings);
    let lift = air.force.dot(Vec3::new(0.0, stall.cos(), -stall.sin()));
    assert!(
        (lift - weight).abs() < weight * 0.01,
        "{lift} against {weight}"
    );
    assert!((air.alpha - stall).abs() < 1e-4);
}

/// Lift stands square to the air: in a descent it does not push her
/// forward or hold her back, and in a bank it leans with her wings.
#[test]
fn lift_stands_square_to_the_air_and_leans_with_her_bank() {
    let p = params();
    let wings = Wings::of(&p);
    let descending = Vec3::new(0.0, -2.0, -15.0);
    let air = air_force(
        descending,
        [Vec3::NEG_Z, Vec3::X, Vec3::Y],
        p.mass.0,
        0.0,
        wings,
    );
    // With no parasitic drag, all that pulls along the air is the drag lift
    // costs - never a push.
    assert!(air.force.dot(descending.normalize()) <= 0.0);
    let banked = Quat::from_rotation_z(-30f32.to_radians());
    let axes = [banked * Vec3::NEG_Z, banked * Vec3::X, banked * Vec3::Y];
    let air = air_force(
        Vec3::new(0.0, 0.0, -wings.cruise),
        axes,
        p.mass.0,
        0.0,
        wings,
    );
    // Right wing down: her lift leans right.
    assert!(air.force.x > 0.0, "{:?}", air.force);
}

/// Her bank is positive with her right wing down, her path is her nose's
/// until she has flying speed.
#[test]
fn bank_and_path_read_the_way_she_is_flying() {
    let right_down = Quat::from_rotation_z(-0.3);
    assert!(bank_of(right_down * Vec3::X, right_down * Vec3::Y) > 0.29);
    let left_down = Quat::from_rotation_z(0.3);
    assert!(bank_of(left_down * Vec3::X, left_down * Vec3::Y) < -0.29);
    let wings = Wings::of(&params());
    // Dropped onto her wheels: no "dive" to pull out of.
    assert_eq!(path_of(Vec3::new(0.0, -3.0, 0.0), Vec3::NEG_Z, wings), 0.0);
    let climbing = path_of(Vec3::new(0.0, 2.0, -15.0), Vec3::NEG_Z, wings);
    assert!(climbing > 0.13, "{climbing}");
}

/// The controller settles a body on exactly the rate asked for: at the
/// rate, all it supplies is the body's own damping.
#[test]
fn the_rate_controller_settles_on_the_rate_asked_for() {
    let (inertia, damping, want) = (6.0, 2.0, 1.5);
    assert_eq!(
        rate_torque(inertia, want, want, damping),
        inertia * damping * want
    );
    assert!(rate_torque(inertia, want, 0.0, damping) > 0.0);
    assert!(rate_torque(inertia, -want, 0.0, damping) < 0.0);
}

/// With no key held and flying level, nothing is asked of her; a held key
/// asks for its own rate, and a stall drops her nose whatever is held.
#[test]
fn the_air_laws_ask_for_what_each_key_wants() {
    let p = params();
    let wings = Wings::of(&p);
    let level = Attitude {
        forward_speed: 17.0,
        ..Attitude::default()
    };
    let none = air_rates(Stick::default(), &p, level, wings);
    assert_eq!(none, Rates::default());
    let left = Stick {
        roll: -1,
        ..Stick::default()
    };
    assert!(air_rates(left, &p, level, wings).roll < 0.0);
    // At the bank asked for, no more roll.
    let banked = Attitude {
        bank: -MAX_BANK_DEG.to_radians(),
        ..level
    };
    let held = air_rates(left, &p, banked, wings);
    assert!(held.roll.abs() < 1e-5);
    // In that bank the turn asks for its own pitch rate and yaw.
    assert!(held.pitch > 0.0 && held.yaw > 0.0, "{held:?}");
    let up = Stick {
        pitch: 1,
        ..Stick::default()
    };
    let stalling = Attitude {
        forward_speed: 9.0,
        stalled: true,
        ..level
    };
    let stalled = air_rates(up, &p, stalling, wings);
    assert!(stalled.pitch < 0.0, "a stall drops the nose: {stalled:?}");
    // Climbing hands off, the path is levelled.
    let climbing = Attitude { path: 0.2, ..level };
    assert!(air_rates(Stick::default(), &p, climbing, wings).pitch < 0.0);
}

/// A wheel's spring sinks [`GEAR_SAG`] of its reach under the load it
/// carries at rest; and the three wheels' shares are her whole weight, the
/// tail wheel's its lever over both (#1617) - a quarter for the layout
/// every airplane had before the record held one.
#[test]
fn the_gear_sags_its_share_under_her_weight() {
    let (load, reach) = (15.0, 0.25);
    let (stiffness, damping) = gear_spring(load, reach);
    let sag = load * G / stiffness;
    assert!((sag - GEAR_SAG * reach).abs() < 1e-5, "{sag}");
    assert!(damping > 0.0);
    let gear = wheels(&params());
    let total: f32 = gear.iter().map(|w| w.share).sum();
    assert!((total - 1.0).abs() < 1e-6, "{total}");
    assert!((gear[2].share - 0.25).abs() < 1e-6, "{:?}", gear[2]);
    // Her centre of mass forward over her wing lightens her tail wheel.
    let forward = AirplaneParams {
        center_of_mass_ahead: crate::pds::types::Fp(0.2),
        ..params()
    };
    assert!(
        wheels(&forward)[2].share < 0.15,
        "{:?}",
        wheels(&forward)[2]
    );
}

/// #1240 f162: an inverted airplane is righted and a banking one is not.
#[test]
fn an_inverted_airplane_is_righted_and_a_banking_one_is_not() {
    let mass = 40.0;
    assert!(upright_assist_torque(Vec3::Y, Vec3::X, Vec3::ZERO, mass).is_none());
    let banked = Quat::from_rotation_z(60_f32.to_radians()) * Vec3::Y;
    assert!(
        upright_assist_torque(banked, Vec3::X, Vec3::ZERO, mass).is_none(),
        "a 60 degree bank is flying, not a crash"
    );
    let inverted = Quat::from_rotation_z(170_f32.to_radians()) * Vec3::Y;
    let torque = upright_assist_torque(inverted, Vec3::X, Vec3::ZERO, mass)
        .expect("an inverted plane must right itself");
    assert!(torque.length() > 0.0);
    // DEAD inverted: `up x world-up` degenerates and the assist must fall
    // back to the roll axis, or the plane perches on its back.
    let fallback = upright_assist_torque(-Vec3::Y, Vec3::X, Vec3::ZERO, mass)
        .expect("dead-inverted must still right");
    assert!(fallback.normalize().dot(Vec3::X) > 0.99, "{fallback:?}");
}

/// #1240 f162, the other half - and #1615's: the engine is cut on her back
/// on the ground, but not inverted over the top of a loop, where she flies.
#[test]
fn the_engine_is_cut_on_her_back_but_not_over_a_loop() {
    let wings = Wings::of(&params());
    assert!(thrust_applies(Vec3::Y, 0.0, wings));
    assert!(!thrust_applies(-Vec3::Y, 0.0, wings));
    assert!(thrust_applies(-Vec3::Y, wings.stall + 1.0, wings));
    const { assert!(cfg::AIRPLANE_THRUST_CUT_TILT_DEGREES < cfg::AIRPLANE_UPRIGHT_ENGAGE_TILT_DEGREES) };
}

// ---------------------------------------------------------------------------
// On the bench
// ---------------------------------------------------------------------------

/// THE COMPLAINT (#1615): the arcade model took off within a second of
/// spawning and climbed for ever. Set down, she stays parked, the throttle
/// closed.
#[test]
fn a_set_down_airplane_stays_parked() {
    let mut bench = FlightBench::set_down(&airplane(), 0.0, 0.0);
    // Settled onto her springs - ten centimetres down - and then still.
    run(&mut bench, &[], 1.0);
    let start = bench.position();
    run(&mut bench, &[], 5.0);
    assert_eq!(bench.throttle(), 0.0);
    let moved = bench.position().distance(start);
    assert!(moved < 0.02, "parked, she moved {moved:.3} m");
    // Standing on her wheels, her box clear of the ground.
    assert!(bench.underside() > 0.05, "{}", bench.underside());
}

/// The throttle holds: Space opens it at its rate, let go it stays, Shift
/// closes it.
#[test]
fn the_throttle_holds_where_it_is_left() {
    let mut bench = FlightBench::set_down(&airplane(), 0.0, 0.0);
    run(&mut bench, &[KeyCode::Space], 1.0);
    let opened = bench.throttle();
    assert!((opened - THROTTLE_RATE).abs() < 0.02, "{opened}");
    run(&mut bench, &[], 2.0);
    assert_eq!(bench.throttle(), opened, "let go, the throttle moved");
    run(&mut bench, &[KeyCode::ShiftLeft], 2.0);
    assert_eq!(bench.throttle(), 0.0);
}

/// Full power and a pull take her off: she has her stall speed after a
/// few seconds of power, lifts her nose, and is flying - her wheels a metre
/// clear of the ground well inside the run the agent's pilot sweeps clear
/// before it takes off (`TAKE_OFF_RUN_M`, 40 m) - and climbs on hands-off.
#[test]
fn full_throttle_and_a_pull_take_her_off() {
    let mut bench = FlightBench::set_down(&airplane(), 0.0, 100.0);
    run(&mut bench, &[KeyCode::Space], 3.0);
    assert!(
        bench.velocity().length() > 11.0,
        "after 3 s of power she makes {:.1} m/s",
        bench.velocity().length()
    );
    // The nose up for a second, then hands off: where her wheels are first
    // a metre clear of the ground is the end of her run.
    let clear = params().gear_height.0 + 1.0;
    let mut lifted = None;
    for (keys, secs) in [([KeyCode::KeyS].as_slice(), 1.0), (&[], 3.0)] {
        bench.hold(keys);
        for _ in 0..(secs * 64.0) as usize {
            bench.step();
            if lifted.is_none() && bench.underside() > clear {
                lifted = Some(100.0 - bench.position().z);
            }
        }
    }
    let lifted = lifted.expect("her wheels never came a metre clear");
    assert!(lifted < 38.0, "a {lifted:.1} m take-off run");
    assert!(
        bench.underside() > 2.0,
        "hands off after it she is {:.2} m up",
        bench.underside()
    );
}

/// Hands off, she holds her height: level at cruise power for six seconds
/// she stays within a metre of it.
#[test]
fn hands_off_she_holds_her_height() {
    let mut bench = flying(&airplane(), 40.0, 17.0, 0.55);
    run(&mut bench, &[], 2.0);
    let settled = bench.position().y;
    run(&mut bench, &[], 6.0);
    let drift = bench.position().y - settled;
    assert!(drift.abs() < 1.0, "she drifted {drift:+.2} m");
    let (_, bank, _) = attitude(&bench);
    assert!(bank.abs() < 1.0, "{bank}");
}

/// A held A is a steady left turn: she banks to [`MAX_BANK_DEG`], comes
/// round at a steady rate and keeps her height through it - and let go,
/// her wings level within a second.
#[test]
fn a_held_bank_is_a_steady_turn_that_keeps_her_height() {
    let mut bench = flying(&airplane(), 40.0, 17.0, 0.55);
    run(&mut bench, &[], 1.0);
    let height = bench.position().y;
    run(&mut bench, &[KeyCode::KeyA], 1.5);
    let (from, bank, _) = attitude(&bench);
    assert!(
        (bank + MAX_BANK_DEG).abs() < 3.0,
        "held A, she banks {bank:.1} degrees"
    );
    run(&mut bench, &[KeyCode::KeyA], 2.0);
    let (to, _, _) = attitude(&bench);
    let rate = (to - from).rem_euclid(360.0) / 2.0;
    assert!((20.0..50.0).contains(&rate), "she turns at {rate:.1} deg/s");
    let lost = height - bench.position().y;
    assert!(lost < 2.0, "she lost {lost:.2} m in the turn");
    run(&mut bench, &[], 1.0);
    let (_, bank, _) = attitude(&bench);
    assert!(bank.abs() < 3.0, "let go, her bank is still {bank:.1}");
}

/// No corner of the editor's sliders sends her to NaN. A stall speed far
/// under her cruise - the slider's floor is 0 - makes her lift so steep
/// with her angle of attack that one fixed step overshoots and feeds
/// itself, and at a stall of 2 m/s she grew until avian's bounds asserted
/// (the #1615 review); so does a cruise speed far under her speed. Flown
/// through power, a turn and a pull at each corner, she stays finite.
#[test]
fn no_corner_of_the_sliders_sends_her_to_nan() {
    use crate::pds::types::Fp;
    // Stall, cruise, thrust and mass: the sliders' ends.
    for (stall, cruise, thrust, mass) in [
        (0.0, 20.0, 300.0, 40.0),
        (2.0, 20.0, 300.0, 40.0),
        (0.0, 1.0, 10_000.0, 5.0),
        (40.0, 1.0, 10_000.0, 5.0),
        (0.0, 80.0, 10_000.0, 5.0),
        (0.0, 80.0, 10_000.0, 500.0),
    ] {
        let mut p = params();
        p.min_airspeed = Fp(stall);
        p.cruise_speed = Fp(cruise);
        p.thrust = Fp(thrust);
        p.mass = Fp(mass);
        let mut record = airplane();
        record.locomotion = LocomotionConfig::Airplane(Box::new(p));
        let mut bench = flying(&record, 60.0, 20.0, 1.0);
        for keys in [
            [KeyCode::Space].as_slice(),
            &[KeyCode::Space, KeyCode::KeyD],
            &[KeyCode::KeyS],
            &[],
        ] {
            run(&mut bench, keys, 1.5);
            let (at, moving) = (bench.position(), bench.velocity());
            assert!(
                at.is_finite() && moving.is_finite() && moving.length() < 300.0,
                "stall {stall}, cruise {cruise}, thrust {thrust}, mass {mass}, \
                 {keys:?}: at {at}, moving {moving}"
            );
        }
    }
}

/// What the agent's pilot lets A and D go early by (#1615): the turn her
/// bank still has in it as her wings level is the turn she makes - from
/// her steepest bank, and from part of the way into it, still rolling.
#[cfg(unix)]
#[test]
fn a_bank_let_go_turns_her_as_far_as_her_handling_says() {
    let handling = Handling::of(&params());
    for held in [2.0, 0.25] {
        let mut bench = flying(&airplane(), 40.0, 17.0, 0.55);
        run(&mut bench, &[], 0.5);
        run(&mut bench, &[KeyCode::KeyD], held);
        let (_, bank, _) = attitude(&bench);
        let forward = bench.rotation() * Vec3::NEG_Z;
        let rolling = bench.angular_velocity().dot(forward);
        let track = |v: Vec3| (-v.x).atan2(-v.z).to_degrees();
        let from = track(bench.velocity());
        let said = handling
            .roll_out_turn(bank.to_radians(), rolling, bench.velocity().dot(forward))
            .to_degrees();
        run(&mut bench, &[], 3.0);
        let made = (from - track(bench.velocity())).rem_euclid(360.0);
        assert!(
            (made - said).abs() < 1.0,
            "D held {held} s, banked {bank:.1}: she turned {made:.1} degrees, \
             her handling said {said:.1}"
        );
    }
}

/// The taildragger's stance: ten degrees nose-up, her box's floor 48 cm
/// over the ground above her mains.
const TAILDRAGGER_PITCH_DEG: f32 = 10.0;
const TAILDRAGGER_MAINS_M: f32 = 0.48;

/// A taildragger's record (#1617): mains out on splayed legs well ahead of
/// her middle, a short tail wheel at her tail, her centre of mass forward
/// over her wing - the seeded airplane's layout, near enough - reached to
/// stand as the two constants above say ([`reach_for_stance`]).
fn taildragger() -> AvatarRecord {
    use crate::pds::types::{Fp, Fp3};
    let mut p = AirplaneParams {
        chassis_half_extents: Fp3([0.27, 0.33, 1.59]),
        gear_track: Fp(1.53),
        gear_ahead: Fp(0.55),
        tail_wheel_aft: Fp(0.96),
        center_of_mass_ahead: Fp(0.28),
        ..params()
    };
    // Both legs meet her floor, so standing pitched her tail's stands lower
    // by the rise of the floor between them.
    let pitch = TAILDRAGGER_PITCH_DEG.to_radians();
    let between = (p.gear_ahead.0 + p.tail_wheel_aft.0) * p.chassis_half_extents.0[2];
    let tail = TAILDRAGGER_MAINS_M - between * pitch.sin();
    let (reach, share) = reach_for_stance(&p, pitch, TAILDRAGGER_MAINS_M, tail);
    p.gear_height = Fp(reach);
    p.tail_gear_share = Fp(share);
    let mut record = airplane();
    record.locomotion = LocomotionConfig::Airplane(Box::new(p));
    record
}

/// Set down, she stands at the pitch her reaches were worked out for, her
/// box's floor at the height asked over the ground above her mains - her
/// rays plumb, each wheel carrying what it carries standing pitched, not
/// what its spring was sized for - and stays put.
#[test]
fn a_taildragger_stands_where_her_reaches_were_worked_out_to_stand_her() {
    let record = taildragger();
    let LocomotionConfig::Airplane(p) = &record.locomotion else {
        unreachable!("an airplane record")
    };
    let mut bench = FlightBench::set_down(&record, 0.0, 100.0);
    run(&mut bench, &[], 3.0);
    let (_, _, pitch) = attitude(&bench);
    assert!(
        (pitch - TAILDRAGGER_PITCH_DEG).abs() < 0.05,
        "she stands {pitch:.3} degrees nose-up, her reaches were worked out for \
         {TAILDRAGGER_PITCH_DEG}"
    );
    let [hx, hy, hz] = p.chassis_half_extents.0;
    for side in [-1.0, 1.0] {
        let leg = Vec3::new(side * p.gear_track.0 * hx, -hy, -p.gear_ahead.0 * hz);
        let floor = (bench.position() + bench.rotation() * leg).y;
        assert!(
            (floor - TAILDRAGGER_MAINS_M).abs() < 1e-3,
            "her floor stands {floor:.4} m over a main, {TAILDRAGGER_MAINS_M} was asked"
        );
    }
    let at = bench.position();
    run(&mut bench, &[], 2.0);
    assert!(bench.position().distance(at) < 0.02, "she crept");
}

/// From her three wheels she takes off as a taildragger does, with no pull
/// at all: her tail-down stance is her wing's angle of attack, so full power
/// alone lifts her off - within the run the agent's pilot sweeps clear.
#[test]
fn a_taildragger_takes_off_from_three_points_on_power_alone() {
    let mut bench = FlightBench::set_down(&taildragger(), 0.0, 100.0);
    let mut lifted = None;
    bench.hold(&[KeyCode::Space]);
    for _ in 0..(8.0 * 64.0) as usize {
        bench.step();
        if lifted.is_none() && bench.underside() > 1.0 {
            lifted = Some(100.0 - bench.position().z);
        }
    }
    let lifted = lifted.expect("she never left the ground");
    assert!(lifted < 38.0, "a {lifted:.1} m take-off run");
}

/// Q and E swing her nose at her rudder's rate, and her path follows it
/// round.
#[test]
fn the_rudder_turns_her_and_her_path_follows() {
    let mut bench = flying(&airplane(), 40.0, 17.0, 0.55);
    run(&mut bench, &[KeyCode::KeyQ], 2.0);
    let (heading, _, _) = attitude(&bench);
    let yaw_rate = params().yaw_rate.0;
    assert!(
        (heading / 2.0 - yaw_rate).abs() < 0.2 * yaw_rate,
        "Q turned her {heading:.1} degrees in 2 s"
    );
    let path = (-bench.velocity().x)
        .atan2(-bench.velocity().z)
        .to_degrees();
    assert!(
        path > 0.4 * heading,
        "her path came round {path:.1} of {heading:.1}"
    );
}

/// A stall drops her nose, and she flies out of it: with the power off and
/// S held, the wing stalls and her nose goes down whatever is held - and
/// let go, she is back at flying speed.
#[test]
fn a_stall_drops_her_nose_and_she_flies_out_of_it() {
    let mut bench = flying(&airplane(), 80.0, 17.0, 0.0);
    let mut nose_dropped = false;
    bench.hold(&[KeyCode::KeyS]);
    for _ in 0..(5.0 * 64.0) as usize {
        bench.step();
        let (_, _, pitch) = attitude(&bench);
        nose_dropped |= pitch < -5.0;
    }
    assert!(
        nose_dropped,
        "held S with the power off, her nose never dropped"
    );
    run(&mut bench, &[], 4.0);
    let speed = bench.velocity().length();
    let stall = params().min_airspeed.0;
    assert!(speed > 0.9 * stall, "she came out at {speed:.1} m/s");
    assert!(
        bench.position().y > 20.0,
        "and fell to {:.1} m",
        bench.position().y
    );
}

/// Shift with the throttle closed is the brakes: rolling at 10 m/s, she
/// stops inside 20 m - and stays stopped.
#[test]
fn the_brakes_stop_her_and_she_stays_stopped() {
    let mut bench = FlightBench::set_down(&airplane(), 0.0, 100.0);
    run(&mut bench, &[], 0.5);
    bench.set_velocity(Vec3::new(0.0, 0.0, -10.0));
    let from = bench.position();
    run(&mut bench, &[KeyCode::ShiftLeft], 6.0);
    let rolled = bench.position().distance(from);
    assert!(rolled < 20.0, "she rolled {rolled:.1} m braking");
    assert!(bench.velocity().length() < 0.05);
}

/// On her wheels A and D steer, on the spot too - and rolling, the steering
/// is held to a turn her gear can take.
#[test]
fn she_steers_on_the_ground_and_turns_on_the_spot() {
    let mut bench = FlightBench::set_down(&airplane(), 0.0, 0.0);
    run(&mut bench, &[], 0.5);
    let start = bench.position();
    run(&mut bench, &[KeyCode::KeyA], 1.5);
    let (heading, bank, _) = attitude(&bench);
    assert!(heading > 50.0, "A at rest turned her {heading:.1} degrees");
    assert!(bench.position().distance(start) < 1.5);
    assert!(bank.abs() < 3.0, "turning on the spot leaned her {bank:.1}");
    // Taxiing: the turn leans her no more than a few degrees.
    bench.set_throttle(0.3);
    run(&mut bench, &[], 3.0);
    let mut leaned: f32 = 0.0;
    bench.hold(&[KeyCode::KeyD]);
    for _ in 0..128 {
        bench.step();
        leaned = leaned.max(attitude(&bench).1.abs());
    }
    assert!(
        leaned < 6.0,
        "a taxiing turn leaned her {leaned:.1} degrees"
    );
}

/// Her rates are her own whatever her size: an airplane twice as big and
/// eight times the weight banks as fast as the default does.
#[test]
fn a_big_airplane_banks_as_fast_as_a_small_one() {
    let bank_after = |record: &AvatarRecord| {
        let mut bench = flying(record, 40.0, 17.0, 0.55);
        run(&mut bench, &[KeyCode::KeyA], 0.3);
        attitude(&bench).1
    };
    let small = bank_after(&airplane());
    let mut big = airplane();
    if let LocomotionConfig::Airplane(p) = &mut big.locomotion {
        p.chassis_half_extents.0 = p.chassis_half_extents.0.map(|v| v * 2.0);
        p.mass.0 *= 8.0;
        p.thrust.0 *= 8.0;
        p.drag_coefficient.0 *= 8.0;
    }
    let large = bank_after(&big);
    assert!(small < -10.0, "{small}");
    assert!(
        (large - small).abs() < 2.0,
        "big {large:.1} against small {small:.1}"
    );
}

/// A rolls left and D rolls right (#1529), on the bench: held for a few
/// steps in level flight, A takes the left wing down - the right wing's end
/// rises - and D the right.
#[test]
fn a_rolls_left_and_d_rolls_right() {
    let record = airplane();
    let right_wing_up = |key: KeyCode| {
        let mut bench = flying(&record, 40.0, 15.0, 0.5);
        run(&mut bench, &[key], 0.25);
        (bench.rotation() * Vec3::X).y
    };
    let a = right_wing_up(KeyCode::KeyA);
    let d = right_wing_up(KeyCode::KeyD);
    assert!(a > 0.1, "A must roll left, the right wing up: got {a:.3}");
    assert!(
        d < -0.1,
        "D must roll right, the right wing down: got {d:.3}"
    );
}

/// A step the keys were not read on is a step with no key (#821): the
/// stick is taken and left empty, so the owner typing in chat leaves her
/// flying hands-off rather than holding the last key down.
#[test]
fn a_step_without_its_keys_read_is_a_step_with_none() {
    let mut bench = flying(&airplane(), 40.0, 17.0, 0.55);
    run(&mut bench, &[], 0.5);
    // A held as the reader left it, the step before the owner started
    // typing; then the flight model steps alone, the reader standing down.
    let world = bench.world_mut();
    let mut query = world.query::<&mut Stick>();
    *query.single_mut(world).expect("one airplane") = Stick {
        roll: -1,
        ..Stick::default()
    };
    let mut schedule = Schedule::default();
    schedule.add_systems(fly_airplane);
    schedule.run(world);
    let mut query = world.query::<&Stick>();
    assert_eq!(
        *query.single(world).expect("one airplane"),
        Stick::default(),
        "the stick was left holding a key"
    );
    // And held by the editor's freeze, the step spends its keys all the
    // same: none is left to fire the step the freeze lets go.
    let body = world
        .query_filtered::<Entity, With<Stick>>()
        .single(world)
        .expect("one airplane");
    world.entity_mut(body).insert((
        super::super::VisualsEditFreeze {
            prior_locked_axes: None,
        },
        Stick {
            roll: -1,
            ..Stick::default()
        },
    ));
    schedule.run(world);
    let mut query = world.query::<&Stick>();
    assert_eq!(
        *query.single(world).expect("one airplane"),
        Stick::default(),
        "the freeze left the stick holding a key"
    );
}

/// PRINT-ONLY. A whole flight on the bench, a line every quarter second -
/// what the numbers in this file were tuned against.
#[test]
#[ignore = "prints a flight for tuning"]
fn print_a_flight() {
    let record = airplane();
    let mut bench = FlightBench::set_down(&record, 0.0, 200.0);
    for (keys, secs, label) in [
        (&[KeyCode::Space][..], 3.0, "power"),
        (&[KeyCode::KeyS][..], 1.0, "rotate"),
        (&[][..], 3.0, "climb-out"),
        (&[KeyCode::ShiftLeft][..], 0.9, "power-back"),
        (&[][..], 3.0, "cruise"),
        (&[KeyCode::KeyA][..], 6.0, "turn"),
        (&[][..], 2.0, "roll-out"),
    ] {
        bench.hold(keys);
        for i in 0..(secs * 64.0) as usize {
            bench.step();
            if i % 16 == 15 {
                let (heading, bank, pitch) = attitude(&bench);
                println!(
                    "{label:10} t={:5.2} y={:6.2} v={:5.2} hdg={heading:6.1} bank={bank:6.1} \
                     pitch={pitch:6.1} thr={:.2}",
                    bench.elapsed(),
                    bench.position().y,
                    bench.velocity().length(),
                    bench.throttle()
                );
            }
        }
    }
}
