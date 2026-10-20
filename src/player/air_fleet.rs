//! The air fleet's flight cards (#1619, epic #1611's closing sweep): every
//! helicopter, airplane and airship type flown on the game's own flight
//! systems over avian, gravity on, through [`FlightBench`] - one card a
//! type, read off the record the craft pin's own hunt finds for her (#1380),
//! as the surface fleet's drive cards are (`spawn`'s tests, #1381).
//!
//! A helicopter's card is what her cyclic, her yaw and her collective do
//! from a hover: her top speed with W held, her yaw rate with A held and
//! her climb with Space held - and every airship's, since they fly the same
//! preset (the seven types since #1664, epic #1656's closing sweep). An
//! airplane's is her take-off run on power alone from her three wheels, her
//! top speed at full power hands off, and in level flight her roll, pitch
//! and rudder rates with A, S and Q held, and how long A takes her to a 40
//! degree bank.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::pds::avatar::AvatarRecord;
use crate::seeded_defaults::{AvatarPins, CraftType};

use super::sim::{BENCH_HZ, FlightBench};

/// A helicopter's card: how she answers each key from a hover.
#[derive(Clone, Copy, Debug)]
struct RotorCard {
    /// Flat speed after [`ROTOR_STRAIGHT_S`] of W (m/s).
    top_speed: f32,
    /// How much that speed still moved over its last second, as a share.
    speed_drift: f32,
    /// Yaw rate after [`ROTOR_TURN_S`] of A (degrees a second).
    yaw_rate: f32,
    /// How much that rate still moved over its last second, as a share.
    yaw_drift: f32,
    /// Climb rate after [`ROTOR_CLIMB_S`] of Space (m/s).
    climb: f32,
}

/// An airplane's card: her take-off run, her top speed, her roll, pitch and
/// rudder rates, and how long a bank takes her.
#[derive(Clone, Copy, Debug)]
struct WingCard {
    /// How far she ran on power alone, Space held from her three wheels,
    /// before her underside stood a metre over the ground (m).
    take_off: f32,
    /// Her speed after [`WING_LEVEL_S`] at full power hands off (m/s).
    top_speed: f32,
    /// How much that speed still moved over its last second, as a share.
    speed_drift: f32,
    /// The fastest she rolled with A held in level flight (degrees a
    /// second).
    roll_rate: f32,
    /// How long A held takes her from level to a 40 degree bank (s), read
    /// between the steps either side of it.
    bank_40: f32,
    /// The fastest she pitched with S held in level flight (degrees a
    /// second).
    pitch_rate: f32,
    /// The fastest she yawed with Q held in level flight (degrees a
    /// second).
    yaw_rate: f32,
}

/// How long each helicopter key is held (s): long enough that her speed
/// and her yaw rate settle, which the cards' drifts prove.
const ROTOR_STRAIGHT_S: f32 = 12.0;
const ROTOR_TURN_S: f32 = 4.0;
const ROTOR_CLIMB_S: f32 = 3.0;

/// How long an airplane flies level at full power (s), and how long a turn
/// key - A, S or Q - is held in level flight (s): the rate controller
/// settles well inside it.
const WING_LEVEL_S: f32 = 30.0;
const WING_ROLL_S: f32 = 0.5;

/// Fixed steps in `secs`.
fn steps(secs: f32) -> usize {
    (secs * BENCH_HZ as f32).round() as usize
}

/// The record the craft pin's hunt finds for each aircraft type, by label.
fn seed_per_aircraft() -> Vec<(CraftType, u64)> {
    seed_per(
        CraftType::HELICOPTERS
            .into_iter()
            .chain(CraftType::AIRPLANES),
    )
}

/// The record the craft pin's hunt finds for each of `crafts`.
fn seed_per(crafts: impl IntoIterator<Item = CraftType>) -> Vec<(CraftType, u64)> {
    crafts
        .into_iter()
        .map(|craft| {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(craft));
            let seed = pins
                .find_seed(0)
                .unwrap_or_else(|| panic!("{} is reachable", craft.label()));
            (craft, seed)
        })
        .collect()
}

/// A helicopter aloft at 100 m, hands off for a second so she settles.
fn hovering(record: &AvatarRecord) -> FlightBench {
    let mut bench = FlightBench::new(record, Vec3::new(0.0, 100.0, 0.0)).aloft();
    bench.hold(&[]);
    for _ in 0..steps(1.0) {
        bench.step();
    }
    bench
}

/// The last value of a trajectory, and how much it moved over its last
/// second, as a share of it.
fn settled(trace: &[f32]) -> (f32, f32) {
    let last = *trace.last().expect("a trajectory");
    let before = trace[trace.len() - 1 - steps(1.0)];
    (last, ((last - before) / last).abs())
}

/// Fly a helicopter's card.
fn rotor_card(record: &AvatarRecord) -> RotorCard {
    let mut bench = hovering(record);
    bench.hold(&[KeyCode::KeyW]);
    let mut speeds = Vec::new();
    for _ in 0..steps(ROTOR_STRAIGHT_S) {
        bench.step();
        let v = bench.velocity();
        speeds.push(Vec2::new(v.x, v.z).length());
    }
    let (top_speed, speed_drift) = settled(&speeds);

    let mut bench = hovering(record);
    bench.hold(&[KeyCode::KeyA]);
    let mut rates = Vec::new();
    for _ in 0..steps(ROTOR_TURN_S) {
        bench.step();
        rates.push(bench.angular_velocity().y.to_degrees());
    }
    let (yaw_rate, yaw_drift) = settled(&rates);

    let mut bench = hovering(record);
    bench.hold(&[KeyCode::Space]);
    for _ in 0..steps(ROTOR_CLIMB_S) {
        bench.step();
    }
    RotorCard {
        top_speed,
        speed_drift,
        yaw_rate,
        yaw_drift,
        climb: bench.velocity().y,
    }
}

/// Fly an airplane's card.
fn wing_card(record: &AvatarRecord) -> WingCard {
    let mut bench = FlightBench::set_down(record, 0.0, 100.0);
    bench.hold(&[KeyCode::Space]);
    let mut take_off = None;
    for _ in 0..steps(15.0) {
        bench.step();
        if take_off.is_none() && bench.underside() > 1.0 {
            take_off = Some(100.0 - bench.position().z);
        }
    }
    let take_off = take_off.expect("she never left the ground");

    let mut bench = FlightBench::new(record, Vec3::new(0.0, 120.0, 400.0));
    bench.set_velocity(Vec3::new(0.0, 0.0, -20.0));
    bench.set_throttle(1.0);
    bench.hold(&[]);
    let mut speeds = Vec::new();
    for _ in 0..steps(WING_LEVEL_S) {
        bench.step();
        speeds.push(bench.velocity().length());
    }
    let (top_speed, speed_drift) = settled(&speeds);

    // In level flight at her own cruise, her throttle where it holds it.
    let cruise = match &record.locomotion {
        crate::pds::LocomotionConfig::Airplane(p) => p.cruise_speed.0,
        _ => panic!("the wing card is for an airplane"),
    };
    let level = || {
        let mut bench = FlightBench::new(record, Vec3::new(0.0, 150.0, 400.0));
        bench.set_velocity(Vec3::new(0.0, 0.0, -cruise));
        bench.set_throttle(0.6);
        bench
    };
    // The fastest she turns about her own `axis` with `key` held.
    let fastest = |key: KeyCode, axis: Vec3| {
        let mut bench = level();
        bench.hold(&[key]);
        let mut most = 0.0f32;
        for _ in 0..steps(WING_ROLL_S) {
            bench.step();
            let about = bench.rotation() * axis;
            most = most.max(bench.angular_velocity().dot(about).abs().to_degrees());
        }
        most
    };
    // The time to a 40 degree bank, read between the two steps that
    // straddle it: at whole steps, one step was worth 3 % of a ratio.
    let mut bench = level();
    bench.hold(&[KeyCode::KeyA]);
    let mut bank_40 = None;
    let mut before = (0.0f32, 0.0f32);
    for _ in 0..steps(3.0) {
        bench.step();
        let rot = bench.rotation();
        let bank = (rot * Vec3::X).y.clamp(-1.0, 1.0).asin().to_degrees().abs();
        let now = bench.elapsed() as f32;
        if bank_40.is_none() && bank >= 40.0 {
            let (t0, b0) = before;
            bank_40 = Some(t0 + (now - t0) * (40.0 - b0) / (bank - b0));
        }
        before = (now, bank);
    }
    WingCard {
        take_off,
        top_speed,
        speed_drift,
        roll_rate: fastest(KeyCode::KeyA, Vec3::NEG_Z),
        bank_40: bank_40.expect("she never banked 40 degrees"),
        pitch_rate: fastest(KeyCode::KeyS, Vec3::X),
        yaw_rate: fastest(KeyCode::KeyQ, Vec3::Y),
    }
}

/// Every aircraft type's card, by label.
fn air_fleet_cards() -> (
    BTreeMap<&'static str, RotorCard>,
    BTreeMap<&'static str, WingCard>,
) {
    let (mut rotors, mut wings) = (BTreeMap::new(), BTreeMap::new());
    for (craft, seed) in seed_per_aircraft() {
        let record = AvatarRecord::default_for_seed(seed);
        match craft {
            CraftType::Helicopter(_) => {
                rotors.insert(craft.label(), rotor_card(&record));
            }
            CraftType::Airplane(_) => {
                wings.insert(craft.label(), wing_card(&record));
            }
            _ => unreachable!("only aircraft are hunted"),
        }
    }
    (rotors, wings)
}

/// Every airship type's card, by label: a rotor card, since every airship
/// flies the helicopter preset (#1612).
fn airship_cards() -> BTreeMap<&'static str, RotorCard> {
    seed_per(CraftType::AIRSHIPS)
        .into_iter()
        .map(|(craft, seed)| {
            (
                craft.label(),
                rotor_card(&AvatarRecord::default_for_seed(seed)),
            )
        })
        .collect()
}

/// PROBE (#1664): print the airships' cards.
#[test]
#[ignore]
fn probe_the_airship_cards() {
    for (craft, seed) in seed_per(CraftType::AIRSHIPS) {
        println!("SEED {} = {seed}", craft.label());
    }
    for (name, c) in &airship_cards() {
        println!(
            "SHIP {name:<18} top {:6.3} m/s (drift {:.5})  yaw {:7.2} deg/s (drift {:.5})  climb {:5.2} m/s",
            c.top_speed, c.speed_drift, c.yaw_rate, c.yaw_drift, c.climb
        );
    }
}

/// PROBE (#1619): print the air fleet's cards.
#[test]
#[ignore]
fn probe_the_air_fleet_cards() {
    for (craft, seed) in seed_per_aircraft() {
        println!("SEED {} = {seed}", craft.label());
    }
    let (rotors, wings) = air_fleet_cards();
    for (name, c) in &rotors {
        println!(
            "ROTOR {name:<26} top {:6.3} m/s (drift {:.5})  yaw {:7.2} deg/s (drift {:.5})  climb {:5.2} m/s",
            c.top_speed, c.speed_drift, c.yaw_rate, c.yaw_drift, c.climb
        );
    }
    for (name, c) in &wings {
        println!(
            "WING  {name:<26} take-off {:5.1} m  top {:6.3} m/s (drift {:.5})  roll {:6.1} deg/s  to 40 deg {:.3} s  pitch {:5.1}  yaw {:5.1} deg/s",
            c.take_off,
            c.top_speed,
            c.speed_drift,
            c.roll_rate,
            c.bank_40,
            c.pitch_rate,
            c.yaw_rate
        );
    }
}

/// Assert that `a` clears `b` by at least `margin`, naming the claim it is
/// part of - the surface fleet's rule (`spawn`'s tests, #1381).
fn clears(claim: &str, a_name: &str, a: f32, b_name: &str, b: f32, margin: f32) {
    assert!(
        a >= b * (1.0 + margin),
        "{claim}: {a_name} {a:.3} is not {:.0}% clear of {b_name} {b:.3}",
        margin * 100.0
    );
}

/// THE PER-TYPE FEEL GUARD for the air fleet (#1619), the surface fleet's
/// measured half for every helicopter and airplane type: the fleet flies in
/// the order the owner agreed, type by type (#1613, #1616, #1621, #1623 to
/// #1625, #1628 to #1630), asked of the real flight systems over avian
/// rather than of the feel and handling tables.
///
/// Relations with a margin and never digits, for the surface guard's
/// reason - local glibc and CI's disagree in the last ulp of `f32`
/// transcendentals - each margin the measured gap rounded down to a tenth,
/// or to a twentieth where the gap is under a tenth. Where a gap is the
/// agreed ratio to the digit - the racer's pitch and rudder a fifth over
/// the biplane's and the high-wing's, 1.2008 measured - it is held a tenth
/// under it, since the rounding would leave a cushion an ulp could take.
///
/// It found the one place the fleet did not fly as agreed: the bank law
/// pulled every record toward her bank alike and capped her roll at 135
/// degrees a second, so the air racer agreed to roll at 150 banked barely
/// quicker than the biplane at 120. Its pull follows the record's roll rate
/// now (`airplane::bank_gain`), and the bank times below are its proof.
#[test]
fn the_air_fleet_flies_in_the_agreed_order() {
    let (rotors, wings) = air_fleet_cards();
    let rotor = |name: &str| -> RotorCard {
        *rotors
            .get(name)
            .unwrap_or_else(|| panic!("{name} is in the seeded fleet"))
    };
    let wing = |name: &str| -> WingCard {
        *wings
            .get(name)
            .unwrap_or_else(|| panic!("{name} is in the seeded fleet"))
    };

    // THE CONTROL FOR THE WINDOWS: every speed and yaw rate below is a
    // steady one or it is nothing.
    for (name, c) in &rotors {
        assert!(
            c.speed_drift < 0.001 && c.yaw_drift < 0.001,
            "{name}: her speed or her yaw rate had not settled ({:.5}, {:.5})",
            c.speed_drift,
            c.yaw_drift
        );
    }
    for (name, c) in &wings {
        assert!(
            c.speed_drift < 0.001,
            "{name}: her level flight had not settled ({:.5})",
            c.speed_drift
        );
    }

    // ---- THE HELICOPTERS ----

    // Top speed: the coaxial's pusher shoves her fastest (#1628 Q-G), the
    // utility next (#1613), the autogyro between her and the trainer
    // (#1624 Q-H), the lazy trainer (#1621 Q-G), and the heavy tandem the
    // slowest (#1630 Q-G).
    let fastest = [
        ("Coaxial helicopter", 0.1),
        ("Utility helicopter", 0.05),
        ("Autogyro", 0.1),
        ("Bubble-canopy trainer", 0.1),
        ("Tandem-rotor transport", 0.0),
    ];
    for pair in fastest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the helicopters' top speeds run as agreed",
            a,
            rotor(a).top_speed,
            b,
            rotor(b).top_speed,
            margin,
        );
    }
    // Yaw: the coaxial pirouettes with no tail rotor, the trainer's a
    // little quicker than the utility's, the autogyro's rudder slower than
    // either, and a tandem turns by tilting her two rotors against each
    // other, slowly.
    let quickest = [
        ("Coaxial helicopter", 0.2),
        ("Bubble-canopy trainer", 0.1),
        ("Utility helicopter", 0.1),
        ("Autogyro", 0.1),
        ("Tandem-rotor transport", 0.0),
    ];
    for pair in quickest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the helicopters' yaw rates run as agreed",
            a,
            rotor(a).yaw_rate,
            b,
            rotor(b).yaw_rate,
            margin,
        );
    }

    // ---- THE AIRPLANES ----

    // Top speed: the racer the fastest (#1629 Q-G), the twin faster than
    // the high-wing (#1625 Q-G), the biplane the slowest (#1623 Q-G).
    let fastest = [
        ("Air racer", 0.1),
        ("Twin-engine transport", 0.05),
        ("High-wing monoplane", 0.1),
        ("Biplane", 0.0),
    ];
    for pair in fastest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the airplanes' top speeds run as agreed",
            a,
            wing(a).top_speed,
            b,
            wing(b).top_speed,
            margin,
        );
    }
    // Every one leaves the ground a twentieth inside the run the agent's
    // pilot sweeps clear - unix-only, as the agent is - and the biplane's
    // big wing lifts her shortest of all, the high-wing's run the nearest
    // to hers.
    #[cfg(unix)]
    {
        let clear_run = crate::config::agent::TAKE_OFF_RUN_M;
        for (name, c) in &wings {
            assert!(
                c.take_off < clear_run * 0.95,
                "{name}: a {:.1} m take-off run against a {clear_run} m clear run",
                c.take_off
            );
        }
    }
    for other in wings.keys().filter(|name| **name != "Biplane") {
        clears(
            "the biplane takes off shortest",
            other,
            wing(other).take_off,
            "the biplane",
            wing("Biplane").take_off,
            0.4,
        );
    }
    // The bank: the racer the quickest-rolling, the biplane next, the
    // twin the steadiest - the time each takes to a 40 degree bank, A
    // held, in the order of her roll rate.
    let soonest = [
        ("Twin-engine transport", 0.3),
        ("High-wing monoplane", 0.3),
        ("Biplane", 0.2),
        ("Air racer", 0.0),
    ];
    for pair in soonest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the airplanes bank in the order of their roll rates",
            a,
            wing(a).bank_40,
            b,
            wing(b).bank_40,
            margin,
        );
    }
    // Pitch: the racer's the quickest, the biplane's next, the twin's the
    // slowest.
    let pitch = [
        ("Air racer", 0.1),
        ("Biplane", 0.1),
        ("High-wing monoplane", 0.1),
        ("Twin-engine transport", 0.0),
    ];
    for pair in pitch.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the airplanes' pitch rates run as agreed",
            a,
            wing(a).pitch_rate,
            b,
            wing(b).pitch_rate,
            margin,
        );
    }
    // Rudder: the racer's the quickest and the twin's the slowest, and the
    // biplane yaws as the high-wing does, on the preset's rate.
    clears(
        "the racer's rudder is the quickest",
        "the racer",
        wing("Air racer").yaw_rate,
        "the high-wing",
        wing("High-wing monoplane").yaw_rate,
        0.1,
    );
    clears(
        "the twin's rudder is the slowest",
        "the high-wing",
        wing("High-wing monoplane").yaw_rate,
        "the twin",
        wing("Twin-engine transport").yaw_rate,
        0.1,
    );
    let (b, h) = (
        wing("Biplane").yaw_rate,
        wing("High-wing monoplane").yaw_rate,
    );
    assert!(
        (b / h - 1.0).abs() < 0.01,
        "the biplane yaws as the high-wing does: {b:.2} against {h:.2}"
    );
}

/// THE PER-TYPE FEEL GUARD for the airships (#1664, epic #1656's closing
/// sweep): the seven types fly in the order the fan-out built them to
/// (#1658-#1663), which the owner approved with the types, and the Zeppelin
/// keeps the flight every airship had before the overhaul (#1656 decision
/// 5) - asked of the real flight systems over avian, as the aircraft's are
/// above.
///
/// Margins by the aircraft guard's rule - each the measured gap rounded
/// down to a tenth, or to a twentieth under a tenth - taken one step
/// further, to a hundredth under a twentieth: the airships' feel table runs
/// closer than the aircraft's, and at a twentieth six of their twelve pairs
/// would be held at no margin at all.
#[test]
fn the_airships_fly_in_the_agreed_order() {
    let ships = airship_cards();
    let ship = |name: &str| -> RotorCard {
        *ships
            .get(name)
            .unwrap_or_else(|| panic!("{name} is in the seeded fleet"))
    };

    // THE CONTROL FOR THE WINDOWS, as the helicopters' above.
    for (name, c) in &ships {
        assert!(
            c.speed_drift < 0.001 && c.yaw_drift < 0.001,
            "{name}: her speed or her yaw rate had not settled ({:.5}, {:.5})",
            c.speed_drift,
            c.yaw_drift
        );
    }

    // The hero keeps today's flight: 8 of drive over the preset's 0.8 of
    // damping, 10 m/s, as every airship flew before #1657.
    let zeppelin = ship("Zeppelin").top_speed;
    assert!(
        (zeppelin / 10.0 - 1.0).abs() < 0.01,
        "the Zeppelin tops out at {zeppelin:.3} m/s, not the 10 every airship had"
    );

    // Top speed: the hybrid's four ducted fans the fastest (#1662), the
    // Zeppelin next, then the blimp and the galleon close together, then the
    // drifters - the balloon, the steam dirigible's one wooden pusher and,
    // slowest, the floater pulsing along (#1663).
    let fastest = [
        ("Hybrid airship", 0.05),
        ("Zeppelin", 0.1),
        ("Blimp", 0.01),
        ("Sky galleon", 0.1),
        ("Hot-air balloon", 0.02),
        ("Steam dirigible", 0.02),
        ("Organic floater", 0.0),
    ];
    for pair in fastest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the airships' top speeds run as agreed",
            a,
            ship(a).top_speed,
            b,
            ship(b).top_speed,
            margin,
        );
    }
    // Yaw: the blimp the quickest on her X tail (#1658), the Zeppelin next
    // to her, then the hybrid, the balloon and the floater, the galleon
    // slower, and the ponderous steam dirigible the slowest of them all
    // (#1659).
    let quickest = [
        ("Blimp", 0.01),
        ("Zeppelin", 0.3),
        ("Hybrid airship", 0.1),
        ("Hot-air balloon", 0.01),
        ("Organic floater", 0.03),
        ("Sky galleon", 0.3),
        ("Steam dirigible", 0.0),
    ];
    for pair in quickest.windows(2) {
        let ((a, margin), (b, _)) = (pair[0], pair[1]);
        clears(
            "the airships' yaw rates run as agreed",
            a,
            ship(a).yaw_rate,
            b,
            ship(b).yaw_rate,
            margin,
        );
    }
}

/// #1626: an airplane tipped over on the ground comes back onto her wheels.
/// Every seeded airplane type, set down and then rolled onto a wing, her
/// side or her back, lay where the assist left her: it engaged only past
/// 120 degrees, so it rolled her off her back onto her side and let go
/// there, where her wheels no longer reach the ground (they count within 50
/// degrees) and her engine is cut. Fifteen seconds on, hands off, she
/// stands as she parks - each type against herself set down untouched,
/// since a taildragger parks tail-down and her box short of the ground.
#[test]
fn an_airplane_tipped_over_on_the_ground_comes_back_onto_her_wheels() {
    let rest = |record: &AvatarRecord, tilt: f32| {
        let mut bench = FlightBench::set_down(record, 0.0, 0.0);
        bench.set_rotation(Quat::from_rotation_z(tilt.to_radians()));
        bench.hold(&[]);
        for _ in 0..steps(15.0) {
            bench.step();
        }
        let rot = bench.rotation();
        let (right, up) = (rot * Vec3::X, rot * Vec3::Y);
        let bank = (-right.y).atan2(up.y).to_degrees();
        (
            up.y.clamp(-1.0, 1.0).acos().to_degrees(),
            bank,
            bench.underside(),
        )
    };
    for (craft, seed) in seed_per(CraftType::AIRPLANES) {
        let record = AvatarRecord::default_for_seed(seed);
        let (parked_tilt, _, parked_clear) = rest(&record, 0.0);
        for tipped in [60.0_f32, 90.0, 135.0, 150.0, 180.0] {
            let (tilt, bank, clear) = rest(&record, tipped);
            assert!(
                bank.abs() < 1.0 && (tilt - parked_tilt).abs() < 1.0,
                "the {} tipped {tipped} degrees ends banked {bank:.1} and {tilt:.1} degrees off \
                 upright; she parks {parked_tilt:.1}",
                craft.label()
            );
            assert!(
                (clear - parked_clear).abs() < 0.005,
                "the {} tipped {tipped} degrees stands her box {clear:.3} m over the ground; she \
                 parks it {parked_clear:.3} m over",
                craft.label()
            );
        }
    }
}
