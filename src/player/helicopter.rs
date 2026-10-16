//! Helicopter preset - auto-stabilising arcade hover.
//!
//! Controls (chassis-local):
//!   * **W / S** - forward / backward cyclic horizontal force.
//!   * **A / D** - yaw torque.
//!   * **Q / E** - lateral strafe force.
//!   * **Space** - climb at `vertical_speed`; on the ground, take off.
//!   * **Shift** - descend at `vertical_speed`.
//!
//! Aloft, `hover_thrust` cancels gravity at idle so the helicopter floats
//! without sinking. The chassis auto-stabilises to upright via a Y-axis-
//! aligning torque so the player never has to fight rotor-induced spin.
//!
//! Down on something solid it is LANDED ([`RotorLift`], #1612): the hover
//! thrust spools away so it rests on its own weight, Shift and the push keys
//! do nothing, A/D still turn it on the spot, and only Space lifts it off. A
//! fresh chassis starts landed, so a body set down a metre up comes down.
//!
//! Hover and auto-stabilise are PASSIVE - they live in
//! [`apply_helicopter_stabilization`], which runs unconditionally (no
//! egui-keyboard gate, no [`TravelingTo`] early-return), so the airship
//! keeps floating while the player types in a chat/search field and
//! while a portal fetch is in flight (#821). Only the key-reading
//! control forces in [`apply_helicopter_forces`] are input-gated.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{HelicopterPreset, VisualsEditFreeze};

/// How long (s) a body aloft must rest on something before it counts as
/// landed: long enough that a skid brushing a ridge in flight does not drop
/// the thrust for a step, short enough that a touchdown settles at once.
const LAND_AFTER_S: f32 = 0.25;

/// How long (s) a landed body must be off everything before its thrust
/// comes back - longer than any set-down's drop takes, so a fresh chassis
/// comes down rather than hanging where it was put.
const AIRBORNE_AFTER_S: f32 = 1.0;

/// How fast (m/s) a landed body off everything may fall before its thrust
/// comes back at once rather than after [`AIRBORNE_AFTER_S`]: faster than a
/// set-down's drop ever gets (a metre falls to 4.4 m/s), slow enough that a
/// craft shoved off a roof catches itself after a metre and a quarter
/// rather than a dozen.
const FALLING_M_S: f32 = 5.0;

/// How far under its box's bottom corners a body may find the ground and
/// still be down on it (m). A craft that comes in fast is pushed a few
/// centimetres back off the ground by the contact, and held up by its
/// thrust it would hover there, touching nothing, never to land.
const NEAR_GROUND_M: f32 = 0.15;

/// The fastest (m/s) a body may move over the ground and still land: a
/// craft skimming rising ground under way is flying, not touching down.
const LAND_BELOW_M_S: f32 = 1.0;

/// The steepest ground a body lands on (the cosine of 30 degrees): steeper,
/// it would slide off on its own weight, so it hovers against the slope as
/// it always did.
const LAND_NORMAL_Y: f32 = 0.866;

/// Whether a rotorcraft is down on something or holding itself up (#1612).
///
/// The preset used to hold every body up all the time - its hover thrust
/// cancels its weight whatever is under it - so a landed craft weighed
/// nothing: set down a metre up at spawn it hung there, and pressed into a
/// hillside with Shift the ground sprang it a metre and a half back up when
/// let go (the agent's #1430 case). Down on something solid now, it rests
/// on its weight with the thrust spooled away, and only Space lifts it.
///
/// "Down" is the game's reading of a body on something
/// ([`super::resting_on`]), or a ground within [`NEAR_GROUND_M`] under its
/// box's bottom corners, no steeper than [`LAND_NORMAL_Y`]: the ground, a
/// roof or a deck, never water, which a rotorcraft hovers over as before. It lands only at rest - slower than [`LAND_BELOW_M_S`] over the
/// ground, with no climb or push key held - and every set-down (spawn,
/// recovery, a requested move, an arrival) puts it down with
/// [`RotorLift::default`]. Landed, its stabiliser still acts: a tall craft on
/// a cross-slope may rest on its uphill edge rather than lie flat, but it
/// never topples.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct RotorLift {
    landed: bool,
    /// How long (s) the body has been resting on something while aloft, or
    /// off everything while landed: each change waits out its dwell.
    dwell: f32,
    /// Whether the controls held the climb key on the last step they read:
    /// written by the input-gated [`apply_helicopter_forces`], read and
    /// cleared by the passive lift, so the one never reads the keyboard the
    /// other's way (#821). A body climbing away from the ground is not
    /// landing on it.
    climbing: bool,
    /// Whether they held a push key (W, S, Q or E) on that step, the same
    /// way: a body driven along the ground is flying, not touching down.
    pushing: bool,
}

impl Default for RotorLift {
    /// Landed: a body set down comes down onto what is under it.
    fn default() -> Self {
        Self {
            landed: true,
            dwell: 0.0,
            climbing: false,
            pushing: false,
        }
    }
}

/// What the passive lift reads of its body on a step.
#[derive(Clone, Copy, Debug)]
struct Reading {
    /// Resting on something no steeper than a landing takes.
    resting: bool,
    /// Its speed over the ground (m/s).
    over_ground: f32,
    /// Its vertical speed (m/s, up positive).
    rising: f32,
}

impl RotorLift {
    /// Aloft, holding itself up: a bench that places a craft high to study
    /// its flight says so with this, where a body set down starts down.
    #[cfg(test)]
    pub(crate) fn aloft() -> Self {
        Self {
            landed: false,
            ..Self::default()
        }
    }

    /// One fixed step of `dt` seconds reading the body as `now`: the state
    /// the passive lift moves to. Pure, so the rules are unit-testable.
    fn stepped(self, now: Reading, dt: f32) -> Self {
        let mut next = Self {
            climbing: false,
            pushing: false,
            ..self
        };
        if self.landed && !now.resting && now.rising < -FALLING_M_S {
            next.landed = false;
            next.dwell = 0.0;
            return next;
        }
        let leaving = if self.landed {
            !now.resting
        } else {
            now.resting && !self.climbing && !self.pushing && now.over_ground < LAND_BELOW_M_S
        };
        if !leaving {
            next.dwell = 0.0;
            return next;
        }
        next.dwell += dt;
        let after = if self.landed {
            AIRBORNE_AFTER_S
        } else {
            LAND_AFTER_S
        };
        if next.dwell >= after {
            next.landed = !self.landed;
            next.dwell = 0.0;
        }
        next
    }
}

/// Set a rotorcraft down (#1612): whatever it was doing, a body put on the
/// ground - by a spawn, a recovery, a requested move or an arrival - starts
/// landed and comes down onto what is under it, rather than hanging where it
/// was put because it happened to be flying when it was moved. A body of any
/// other preset carries no [`RotorLift`] and is left as it is.
pub(crate) fn set_down(lift: Option<Mut<RotorLift>>) {
    if let Some(mut lift) = lift {
        lift.set_if_neq(RotorLift::default());
    }
}

/// Strength (N·m) of the auto-stabilising torque that pulls the
/// Shortest-path corrective torque that pulls `chassis_up` toward
/// world-up. Zero when already upright; strongest at 90° of tilt. Pure
/// so the stabilisation contract is unit-testable. `strength` is the
/// record's `stabilize_torque` (#876) - the default keeps the historical
/// never-inverts feel while still letting cyclic tilt read as motion.
fn stabilize_torque(chassis_up: Vec3, strength: f32) -> Vec3 {
    chassis_up.cross(Vec3::Y) * strength
}

/// Passive lift and self-righting for the helicopter chassis: hover
/// thrust that cancels gravity while aloft, plus the upright-stabilising
/// torque - and the touchdown and lift-off of [`RotorLift`] (#1612).
///
/// Deliberately NOT gated on `egui_wants_any_keyboard_input`,
/// `avatar_visuals_row_selected`, or [`TravelingTo`] - these forces are
/// stabilisation, not input response. Before the split the hover force
/// lived in [`apply_helicopter_forces`], so focusing any egui text field
/// (or touching a portal) cut the rotor and the airship fell out of the
/// sky (#821). The one legitimate suppressor is the avatar-editor freeze:
/// while [`VisualsEditFreeze`] parks the chassis (all axes locked,
/// gravity zeroed, velocity re-zeroed per frame, #814) applying hover
/// thrust would only fight it, so frozen bodies are filtered out.
#[allow(clippy::type_complexity)]
pub(super) fn apply_helicopter_stabilization(
    live: Res<LiveAvatarRecord>,
    time: Res<Time>,
    contacts: Option<Res<ContactGraph>>,
    sensors: Query<Entity, With<Sensor>>,
    spatial: SpatialQuery,
    mut query: Query<
        (Entity, Forces, &mut RotorLift),
        (
            With<LocalPlayer>,
            With<HelicopterPreset>,
            Without<VisualsEditFreeze>,
        ),
    >,
) {
    let LocomotionConfig::Helicopter(p) = &live.0.locomotion else {
        return;
    };
    let Ok((body, mut forces, mut lift)) = query.single_mut() else {
        return;
    };
    let global_tf = &super::physics_pose(forces.position(), forces.rotation());

    let moving = forces.linear_velocity();
    let touching = contacts
        .as_deref()
        .is_some_and(|contacts| super::resting_on(contacts, &sensors, body, LAND_NORMAL_Y));
    let pose = global_tf.compute_transform();
    let filter = super::ground_ray_filter(body, sensors.iter());
    let near = || {
        super::chassis_corners(Vec3::from_array(p.chassis_half_extents.0))
            .into_iter()
            .any(|corner| {
                spatial
                    .cast_ray(
                        pose.transform_point(corner),
                        Dir3::NEG_Y,
                        NEAR_GROUND_M,
                        true,
                        &filter,
                    )
                    .is_some_and(|hit| hit.normal.y >= LAND_NORMAL_Y)
            })
    };
    let now = Reading {
        resting: touching || near(),
        over_ground: moving.with_y(0.0).length(),
        rising: moving.y,
    };
    // Guarded: a `&mut` through a query stamps a change tick even when the
    // value is the same.
    let next = lift.stepped(now, time.delta_secs());
    if *lift != next {
        *lift = next;
    }

    // Hover thrust along world-Y so the helicopter floats independent
    // of cyclic pitch - players can tilt for forward speed without
    // bleeding altitude. Landed, it rests on its weight instead.
    if !next.landed {
        forces.apply_force(Vec3::Y * p.hover_thrust.0);
    }

    // Auto-stabilise: torque the chassis-up axis toward world-up so
    // the player never inverts. The cross product gives a torque
    // perpendicular to both, which is the rotation-axis of the
    // shortest-path correction.
    forces.apply_torque(stabilize_torque(
        global_tf.up().as_vec3(),
        p.stabilize_torque.0,
    ));
}

/// The key-driven controls: climb and descent, cyclic, strafe and yaw - and
/// take-off, which is Space on a [`RotorLift`] that is down (#1612). Landed,
/// only the yaw keys still act: Shift asked a body on the ground for a
/// descent it could not make, pressing it into the ground (down a slope, it
/// slid) to spring back when let go, and the push keys would skate it on
/// its skids.
#[allow(clippy::type_complexity)]
pub(super) fn apply_helicopter_forces(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(Forces, &mut RotorLift), (With<LocalPlayer>, With<HelicopterPreset>)>,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Helicopter(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, mut lift)) = query.single_mut() else {
        return;
    };
    let global_tf = &super::physics_pose(forces.position(), forces.rotation());

    let want_climb = keyboard.pressed(KeyCode::Space);
    let want_descend =
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let climbing = want_climb && !want_descend;
    let pushing = [
        KeyCode::KeyW,
        KeyCode::ArrowUp,
        KeyCode::KeyS,
        KeyCode::ArrowDown,
        KeyCode::KeyQ,
        KeyCode::KeyE,
    ]
    .into_iter()
    .any(|key| keyboard.pressed(key));
    let mut next = *lift;
    next.climbing = climbing;
    next.pushing = pushing;
    if next.landed && climbing {
        // Take off: the thrust is back from the passive lift's next step,
        // and the climb below lifts the body off this one.
        next.landed = false;
        next.dwell = 0.0;
    }
    if *lift != next {
        *lift = next;
    }

    // Yaw on A/D - on the spot as well as aloft.
    let local_up = global_tf.up().as_vec3();
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        forces.apply_torque(local_up * p.yaw_torque.0);
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        forces.apply_torque(-local_up * p.yaw_torque.0);
    }
    if next.landed {
        return;
    }

    // Vertical climb / descend lerps the Y component of velocity toward
    // ±vertical_speed when Space / Shift is held; idle leaves vertical
    // motion to the gravity / hover_thrust equilibrium (the hover force
    // itself is applied by `apply_helicopter_stabilization`).
    let lin_vel = forces.linear_velocity();
    let target_vy = match (want_climb, want_descend) {
        (true, false) => p.vertical_speed.0,
        (false, true) => -p.vertical_speed.0,
        _ => lin_vel.y,
    };
    // Approach the target Y velocity at a fixed rate so input feels
    // immediate without overshooting. The 4.0 here matches the cyclic
    // feel - tunable, but again a behavioural constant.
    let dy = (target_vy - lin_vel.y).clamp(-15.0, 15.0);
    forces.apply_force(Vec3::Y * dy * p.mass.0 * 4.0);

    // Cyclic forward/back - uses the chassis-flat forward so a tilted
    // helicopter still moves "where the player is looking".
    let forward = global_tf.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        forces.apply_force(flat_forward * p.cyclic_force.0);
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        forces.apply_force(-flat_forward * p.cyclic_force.0);
    }

    // Strafe - chassis-flat right.
    let right = global_tf.right().as_vec3();
    let flat_right = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
    if keyboard.pressed(KeyCode::KeyQ) {
        forces.apply_force(-flat_right * p.strafe_force.0);
    }
    if keyboard.pressed(KeyCode::KeyE) {
        forces.apply_force(flat_right * p.strafe_force.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default record strength - tests exercise the historical feel.
    fn strength() -> f32 {
        crate::pds::HelicopterParams::default().stabilize_torque.0
    }

    #[test]
    fn upright_chassis_gets_no_stabilize_torque() {
        assert_eq!(stabilize_torque(Vec3::Y, strength()), Vec3::ZERO);
    }

    #[test]
    fn tilted_chassis_gets_shortest_path_correction() {
        // Chassis-up pointing along +X: X × Y = +Z, and a +Z torque
        // rotates +X toward +Y (right-hand rule) - i.e. back upright.
        let torque = stabilize_torque(Vec3::X, strength());
        assert!(torque.z > 0.0, "expected +Z corrective torque: {torque}");
        assert_eq!(torque.x, 0.0);
        assert_eq!(torque.y, 0.0);
    }

    #[test]
    fn inverted_chassis_still_gets_finite_torque_magnitude() {
        // Exactly inverted is the degenerate antipode: cross(−Y, Y) = 0,
        // so the torque vanishes - any perturbation off the pole
        // re-engages it. Document the dead point rather than pretend
        // it recovers instantly.
        assert_eq!(stabilize_torque(-Vec3::Y, strength()), Vec3::ZERO);
        let nudged = stabilize_torque(Vec3::new(0.01, -1.0, 0.0).normalize(), strength());
        assert!(nudged.length() > 0.0);
    }

    #[test]
    fn stabilize_torque_scales_with_the_record_strength() {
        let half = stabilize_torque(Vec3::X, strength() * 0.5);
        let full = stabilize_torque(Vec3::X, strength());
        assert!((full.length() - 2.0 * half.length()).abs() < 1e-3);
    }

    /// Steps of 1/64 s, the game's fixed step.
    const DT: f32 = 1.0 / 64.0;

    /// A body still, resting or not.
    fn still(resting: bool) -> Reading {
        Reading {
            resting,
            over_ground: 0.0,
            rising: 0.0,
        }
    }

    /// `lift` after `steps` fixed steps reading `now` all along, with the
    /// controls holding climb or push keys.
    fn after(
        mut lift: RotorLift,
        now: Reading,
        (climbing, pushing): (bool, bool),
        steps: usize,
    ) -> RotorLift {
        for _ in 0..steps {
            lift.climbing = climbing;
            lift.pushing = pushing;
            lift = lift.stepped(now, DT);
        }
        lift
    }

    const NO_KEYS: (bool, bool) = (false, false);

    /// A body aloft lands once it has rested on something for its dwell,
    /// and not a step before - a skid brushing a ridge is not a landing.
    #[test]
    fn a_body_aloft_lands_once_it_has_rested_for_its_dwell() {
        let dwell = (LAND_AFTER_S / DT).ceil() as usize;
        assert!(!after(RotorLift::aloft(), still(true), NO_KEYS, dwell - 1).landed);
        assert!(after(RotorLift::aloft(), still(true), NO_KEYS, dwell).landed);
        // Touching and leaving, it never builds up the dwell.
        let mut lift = RotorLift::aloft();
        for i in 0..4 * dwell {
            lift = lift.stepped(still(i % 2 == 0), DT);
        }
        assert!(!lift.landed, "a body brushing the ground on and off landed");
    }

    /// A body only lands at rest: climbing away from what it touches, driven
    /// along by a push key, or skimming the ground under way, it is flying -
    /// a craft low over rising ground used to land and stop dead.
    #[test]
    fn a_body_lands_only_at_rest() {
        let long = 128;
        assert!(!after(RotorLift::aloft(), still(true), (true, false), long).landed);
        assert!(!after(RotorLift::aloft(), still(true), (false, true), long).landed);
        let skimming = Reading {
            over_ground: LAND_BELOW_M_S * 2.0,
            ..still(true)
        };
        assert!(!after(RotorLift::aloft(), skimming, NO_KEYS, long).landed);
    }

    /// A landed body stays down while it rests, and its thrust comes back
    /// once it has been off everything for its dwell - a set-down's drop is
    /// shorter, so a body set down comes down rather than hovering - or at
    /// once if it is falling fast, so a craft shoved off a roof catches
    /// itself rather than dropping a dozen metres first.
    #[test]
    fn a_landed_body_lifts_once_off_everything_or_falling() {
        assert!(after(RotorLift::default(), still(true), NO_KEYS, 640).landed);
        let dwell = (AIRBORNE_AFTER_S / DT).ceil() as usize;
        assert!(after(RotorLift::default(), still(false), NO_KEYS, dwell - 1).landed);
        assert!(!after(RotorLift::default(), still(false), NO_KEYS, dwell).landed);
        let dropping = Reading {
            rising: -FALLING_M_S * 1.5,
            ..still(false)
        };
        assert!(!after(RotorLift::default(), dropping, NO_KEYS, 1).landed);
    }

    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::FlightBench;

    /// The agent's airship stand-in: a twin envelope, the heaviest of the
    /// old airships, until #1657, and a Zeppelin since.
    fn airship() -> AvatarRecord {
        AvatarRecord::default_for_did("did:plc:agentofflineair222222222")
    }

    fn run(bench: &mut FlightBench, keys: &[KeyCode], secs: f32) {
        bench.hold(keys);
        for _ in 0..(secs / DT).round() as usize {
            bench.step();
        }
    }

    /// THE COMPLAINT (#1612): set down a metre up, as a spawn sets a body
    /// down, a rotorcraft hung there - its thrust cancelled its weight
    /// whatever was under it. It comes down now, and rests on the floor.
    #[test]
    fn a_rotorcraft_put_down_a_metre_up_comes_down_and_rests() {
        let record = airship();
        let probe = FlightBench::set_down(&record, 0.0, 0.0);
        let rest_y = probe.position().y - probe.underside();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, rest_y + 1.0, 0.0));
        run(&mut bench, &[], 3.0);
        assert!(
            bench.underside().abs() < 0.02,
            "three seconds after a set-down a metre up its underside is {:.3} m off the floor",
            bench.underside()
        );
    }

    /// THE AGENT'S #1430 HILLSIDE, on the flat: Shift held on the ground
    /// asked for a descent the floor stopped, pressing the body in, and let
    /// go the floor sprang it back up with nothing to stop it rising.
    /// Landed, Shift does nothing.
    #[test]
    fn shift_on_the_ground_neither_presses_a_rotorcraft_in_nor_springs_it_up() {
        let mut bench = FlightBench::set_down(&airship(), 0.0, 0.0);
        run(&mut bench, &[], 1.0);
        let rest = bench.position().y;
        run(&mut bench, &[KeyCode::ShiftLeft], 2.0);
        assert!(
            (bench.position().y - rest).abs() < 0.02,
            "held Shift moved a landed body {:+.3} m",
            bench.position().y - rest
        );
        let mut highest = f32::MIN;
        bench.hold(&[]);
        for _ in 0..128 {
            bench.step();
            highest = highest.max(bench.position().y);
        }
        assert!(
            highest - rest < 0.02,
            "let go, the floor sprang it {:.2} m up",
            highest - rest
        );
    }

    /// Space lifts a landed rotorcraft off, and let go aloft it holds itself
    /// up again.
    #[test]
    fn space_lifts_a_landed_rotorcraft_off_and_it_hovers() {
        let mut bench = FlightBench::set_down(&airship(), 0.0, 0.0);
        run(&mut bench, &[], 1.0);
        run(&mut bench, &[KeyCode::Space], 1.0);
        let up = bench.underside();
        assert!(up > 2.0, "a second of Space lifted it {up:.2} m");
        // Let go it coasts on its climb and holds where that ends, rather
        // than falling: back aloft, its thrust carries its weight.
        run(&mut bench, &[], 6.0);
        let settled = bench.underside();
        run(&mut bench, &[], 2.0);
        assert!(
            settled > up && (bench.underside() - settled).abs() < 0.05,
            "let go aloft it went from {settled:.2} m to {:.2} m in two seconds",
            bench.underside()
        );
    }

    /// A craft that comes in fast with nothing held - Shift let go above the
    /// ground, coasting down at full speed - lands, rather than being pushed
    /// a few centimetres back off the ground by the contact and hovering
    /// there on its thrust, touching nothing, for ever (#1612).
    #[test]
    fn a_craft_coasting_in_fast_lands_rather_than_hovering_over_the_ground() {
        let record = airship();
        let probe = FlightBench::set_down(&record, 0.0, 0.0);
        let rest_y = probe.position().y - probe.underside();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, rest_y + 2.0, 0.0)).aloft();
        bench.set_velocity(Vec3::new(0.0, -6.0, 0.0));
        run(&mut bench, &[], 4.0);
        assert!(
            bench.underside().abs() < 0.02,
            "four seconds after coasting in its underside is {:.3} m off the floor",
            bench.underside()
        );
    }

    /// An airship taller than the spawn's metre, set down where the game
    /// sets a body down, starts clear of the ground and comes down onto it
    /// (#1612). Its box reaches further below its origin than the metre, and
    /// set down a metre up its keel started in the ground and was shoved out.
    ///
    /// Every seeded airship was that tall until #1657 - an envelope over a
    /// gondola hung on cables - and every one PUBLISHED between #1612 and
    /// #1657 still is, keeping its box (no migration). The Zeppelin that
    /// replaced them stands about a metre tall in all, so the tall ship is
    /// one of them: a seeded airship's record with its box stood up to 1.2 m
    /// below and above its origin.
    #[test]
    fn a_tall_airship_is_set_down_clear_and_comes_down() {
        use crate::seeded_defaults::ChassisFamily;
        let mut record = (0u64..400)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airship)
            .map(AvatarRecord::default_for_seed)
            .expect("an airship seed under 400");
        let LocomotionConfig::Helicopter(p) = &mut record.locomotion else {
            panic!("an airship flies the helicopter preset");
        };
        p.chassis_half_extents.0[1] = 1.2;
        let half = p.chassis_half_extents.0[1];
        assert!(
            half > crate::config::rover::SPAWN_HEIGHT_OFFSET,
            "the ship is no taller than the spawn's metre: {half}"
        );
        let up = super::super::preset::set_down_height(&record.locomotion);
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, up, 0.0));
        assert!(bench.underside() > 0.0, "set down in the ground");
        run(&mut bench, &[], 2.0);
        assert!(
            bench.underside().abs() < 0.02,
            "two seconds after its set-down its underside is {:.3} m off the floor",
            bench.underside()
        );
    }

    /// A seeded HELICOPTER flies the same preset (#1614), and this is her
    /// physics read as a player meets it: set down where the game sets a
    /// body down, she comes down onto her skids - her box's floor - and
    /// rests there; Space lifts her off; and held forward she tops out at her
    /// type's 12.5 m/s (cyclic 10 over damping 0.8), where the airship
    /// stand-in makes 10 - the "nimbler than an airship" of #1613
    /// recommendation 8, measured on the systems rather than read off the
    /// table (12.49 against 7.50 when it was written, the stand-in then a
    /// twin envelope; a Zeppelin since #1657, every seeded airship's 10).
    #[test]
    fn a_seeded_helicopter_rests_on_her_skids_lifts_off_and_outflies_the_airship() {
        use crate::seeded_defaults::ChassisFamily;
        let record = (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Helicopter)
            .map(AvatarRecord::default_for_seed)
            .expect("a helicopter seed under 600");
        let up = super::super::preset::set_down_height(&record.locomotion);
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, up, 0.0));
        run(&mut bench, &[], 2.0);
        assert!(
            bench.underside().abs() < 0.02,
            "two seconds after her set-down her skids are {:.3} m off the floor",
            bench.underside()
        );
        run(&mut bench, &[KeyCode::Space], 1.0);
        assert!(
            bench.underside() > 2.0,
            "a second of Space lifted her {:.2} m",
            bench.underside()
        );
        // Held forward from five metres up, aloft and clear of the floor:
        // the ground covered in the eighth second is her top speed.
        let top_speed = |record: &AvatarRecord| {
            let at = FlightBench::set_down(record, 0.0, 0.0).position() + Vec3::Y * 5.0;
            let mut bench = FlightBench::new(record, at).aloft();
            run(&mut bench, &[KeyCode::KeyW], 7.0);
            let from = bench.position();
            run(&mut bench, &[KeyCode::KeyW], 1.0);
            (bench.position() - from).with_y(0.0).length()
        };
        let (heli, ship) = (top_speed(&record), top_speed(&airship()));
        assert!(
            (heli - 12.5).abs() < 0.6,
            "held forward she makes {heli:.2} m/s, not her 12.5"
        );
        assert!(
            heli > ship + 1.5,
            "she makes {heli:.2} m/s against the airship's {ship:.2}"
        );
    }

    /// Landed, the push keys do not skate a body over the ground on its
    /// skids, but A and D still turn it on the spot.
    #[test]
    fn a_landed_rotorcraft_turns_on_the_spot_but_does_not_skate() {
        let mut bench = FlightBench::set_down(&airship(), 0.0, 0.0);
        run(&mut bench, &[], 1.0);
        let at = bench.position();
        run(&mut bench, &[KeyCode::KeyW, KeyCode::KeyE], 1.0);
        let skated = (bench.position() - at).with_y(0.0).length();
        assert!(skated < 0.02, "W and E skated a landed body {skated:.3} m");
        let heading = |bench: &FlightBench| {
            let forward = bench.rotation() * Vec3::NEG_Z;
            forward.x.atan2(-forward.z)
        };
        let before = heading(&bench);
        run(&mut bench, &[KeyCode::KeyA], 2.0);
        let turned = (heading(&bench) - before).abs().to_degrees();
        assert!(
            turned > 20.0,
            "two seconds of A turned it {turned:.1} degrees"
        );
    }
}
