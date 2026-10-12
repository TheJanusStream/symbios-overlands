//! Airplane preset - a light airplane's flight model (#1615).
//!
//! Controls (keys unchanged from the arcade model this replaced):
//!   * **W / S** - nose down / up, at the record's `pitch_rate`. Let go, the
//!     flight path levels: she holds her height.
//!   * **A / D** - bank left / right, at `roll_rate`, to [`MAX_BANK_DEG`]
//!     and held there while the key is: a held key is a steady turn, since
//!     her lift leans with her wings. Let go, the wings level.
//!   * **Q / E** - rudder: nose left / right at `yaw_rate`. Let go, the nose
//!     comes round onto her path.
//!   * **Space / Shift** - throttle up / down. It HOLDS where it is left:
//!     closed, she stays parked; open, she keeps her power with no key held.
//!     Shift held with the throttle closed brakes on the ground.
//!   * On the ground A / D and Q / E steer - on the spot, too.
//!
//! # The model
//!
//! Lift acts square to the air in the plane of her wings, so a bank turns
//! her. It grows with
//! the square of her forward speed and with her angle of attack, and is
//! read off two of the record's speeds: at `cruise_speed` her wings carry
//! her weight with the nose on the path, and at `min_airspeed` (her stall
//! speed) only at the steepest angle they take before they stall, past
//! which the lift falls away and her nose drops until she flies again
//! ([`lift_coefficient`]). Drag is a square-law term plus what lift costs;
//! a keel force turns her path toward her nose. Thrust is the throttle's
//! share of `thrust`, along the nose.
//!
//! Control is by RATE: each key asks for a turn rate about one of her axes
//! and a controller drives her own inertia - read off her box and mass -
//! toward it, so a big airplane answers as a small one does
//! ([`rate_torque`]). With no key held the same controller levels her
//! wings, levels her path and keeps her nose on it ([`air_rates`]).
//!
//! On the ground she stands on three wheels - two mains ahead of her centre
//! and a castering tail wheel - cast like the car's suspension, gripping
//! sideways at the mains, rolling with a little drag, and braking. Where
//! they stand, how far the tail wheel reaches and where her centre of mass
//! is are the record's (#1617), so a drawn taildragger stands tail-down on
//! her own wheels ([`wheels`], [`gear_spring`]); however far out her mains
//! stand, they damp her roll no harder than one fixed step can follow, and
//! stopping, never turn her the other way ([`roll_damping`],
//! [`stop_share`], #1625). How much of her weight they carry is how much of
//! her control is the ground's: taking off, the air's fades in as her lift
//! unloads them.
//!
//! # What is passive
//!
//! The keys are read by [`read_airplane_keys`], which stands down while the
//! owner types (#821); everything else - the held throttle, the lift and
//! drag, the gear, the levelling - is [`fly_airplane`]'s, which does not.
//! So a chat message typed in flight leaves her flying level at her power.
//! A flipped airplane on the ground rights itself
//! ([`apply_airplane_uprighting`], #1240 f162).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::{AirplaneParams, LocomotionConfig};
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{AirplanePreset, VisualsEditFreeze};

/// Gravity, as the physics runs it: avian's default, which the game keeps.
const G: f32 = 9.81;

/// How fast the controller closes the gap between the turn rate asked for
/// and the one she has (1/s): about an eighth of a second.
const RESPONSE: f32 = 8.0;

/// The steepest bank A and D take her to (degrees): a 1.4 g turn, which
/// her levelling holds her height through. At 60 the turn asked for a
/// pitch rate past her own, and she spiralled down into the ground.
pub(crate) const MAX_BANK_DEG: f32 = 45.0;

/// How hard the controller pulls toward the bank asked for, per radian
/// short of it (1/s), before the record's `roll_rate` caps it.
const BANK_GAIN: f32 = 3.0;

/// How hard, with no pitch key, it levels her flight path, per radian of
/// climb or descent (1/s), before `pitch_rate` caps it.
const PATH_GAIN: f32 = 1.5;

/// How hard, with no rudder, it brings her nose onto her path, per radian
/// of sideslip (1/s).
const SLIP_GAIN: f32 = 3.0;

/// The steepest angle of attack her wings take before they stall (degrees),
/// how far past it the lift falls away over, and the share of it left once
/// it has.
const STALL_ANGLE_DEG: f32 = 15.0;
const STALL_BLEND_DEG: f32 = 4.0;
const STALL_LIFT: f32 = 0.55;

/// Drag the lift costs, per unit of lift, at the stall's lift coefficient.
const INDUCED_DRAG: f32 = 0.12;

/// How fast the keel turns her path toward her nose, per second of
/// sideslip at `cruise_speed`.
const KEEL_RATE: f32 = 1.5;

/// The fastest the air may change her motion (1/s): half the rate of the
/// game's 64 Hz fixed step. A force that grows steeply with her angle of
/// attack or her speed - a stall speed far under her cruise, whose stall
/// angle lifts many times what no angle does, or a cruise far under the
/// speed she flies - overshot in one step, fed itself, and sent her to NaN
/// (the #1615 review: a stall of 2 m/s under a cruise of 20). Held to
/// this, it settles. The default record's steepest, flat out, is a sixth
/// of it.
const AIR_RESPONSE_MAX: f32 = 32.0;

/// How fast Space and Shift move the throttle (a full sweep in two
/// seconds).
const THROTTLE_RATE: f32 = 0.5;

/// Her steering on the ground, as a multiple of her rudder's rate - and the
/// most sideways acceleration it may turn her at as she rolls faster
/// (m/s^2): turning at the full rate at 5 m/s leaned her 10 degrees onto
/// one main, and at a take-off's speed would roll her over.
const GROUND_STEER: f32 = 2.4;
const GROUND_LATERAL: f32 = 3.0;

/// The gear: how far past its reach a wheel still counts as down (m), how
/// far her weight compresses it as a share of its reach, how damped it is
/// (of critical), how hard the mains grip sideways (1/s), the rolling drag
/// (m/s^2), and how far past upright she may lean before her wheels are no
/// longer under her (degrees). The sag was 0.35 (#1615) until a drawn
/// airplane stood on it (#1617): her drawn wheels hang where they stand
/// loaded, and the physics' wheel meets the ground its sag sooner - at
/// 0.35, 26 cm before the drawn one on a touchdown.
const GEAR_SLACK_M: f32 = 0.05;
const GEAR_SAG: f32 = 0.15;
const GEAR_DAMPING_RATIO: f32 = 0.8;
const GEAR_GRIP: f32 = 10.0;
const ROLLING_DRAG: f32 = 0.4;
const GEAR_TILT_LIMIT_DEG: f32 = 50.0;

/// The most of her roll her mains' dampers may take out in one fixed step
/// (#1625): all of it, and never more, which the step would turn round into
/// a roll the other way. Each damper is sized for the load its spring
/// carries ([`gear_spring`]), and far out from a narrow box - all her roll
/// inertia, which avian reads off her collider - the two took out three
/// times her roll in a step: the seeded twin, her mains under her nacelles,
/// rocked on them without end, and so did any airplane whose mains were
/// set far enough out. Her heave and pitch keep their damping whole.
const GEAR_ROLL_STEP: f32 = 1.0;

/// The most of her yaw her mains may take out in one fixed step as their
/// rolling drag and brakes bring them to a stop (#1625): half of it. Each
/// stops no more than its own wheel, the load its spring carries, and that
/// is all of her that rolls straight ahead; but far out from a narrow box
/// the two turn her about her middle as well, against a box's yaw inertia,
/// and took out more than all of her yaw in a step - she shivered from side
/// to side at rest, through her roll too, standing tail-down. Half leaves
/// her roll's damping ([`GEAR_ROLL_STEP`]) the rest of the step.
const GEAR_YAW_STEP: f32 = 0.5;

/// The throttle she holds, from 0 (closed) to 1 (full) - on her chassis,
/// moved only by Space and Shift and closed by every set-down
/// ([`set_down`]): a spawned airplane stays parked.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Throttle(pub(crate) f32);

/// The keys this fixed step, as [`read_airplane_keys`] read them: each axis
/// -1, 0 or +1. [`fly_airplane`] takes them and leaves none behind, so a
/// step the reader stood down on - the owner typing - is a step with no key.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stick {
    /// +1 banks right (D), -1 left (A).
    roll: i8,
    /// +1 nose up (S), -1 down (W).
    pitch: i8,
    /// +1 nose left (Q), -1 right (E).
    yaw: i8,
    /// Space.
    up: bool,
    /// Shift.
    down: bool,
}

/// Close the throttle of a body being set down - spawned, respawned, moved
/// or arrived (#1615), as the rotorcraft's landed state is reset there: a
/// set-down airplane stays where it is put. A no-op for a body that is not
/// an airplane, which has no throttle.
pub(crate) fn set_down(throttle: Option<Mut<Throttle>>) {
    if let Some(mut throttle) = throttle {
        throttle.set_if_neq(Throttle::default());
    }
}

/// Her wings' two speeds and what they make of an angle of attack.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Wings {
    /// The slowest her wings carry her weight, at the stall angle (m/s).
    stall: f32,
    /// The speed her wings carry her weight at with the nose on her path.
    cruise: f32,
    /// Her lift coefficient at the stall angle, over the one at none:
    /// `(cruise / stall)^2`, at least 1.
    c_max: f32,
}

impl Wings {
    fn of(p: &AirplaneParams) -> Self {
        let stall = p.min_airspeed.0.max(0.5);
        let cruise = p.cruise_speed.0.max(0.5);
        Self {
            stall,
            cruise,
            c_max: (cruise / stall).powi(2).max(1.0),
        }
    }
}

/// Smoothstep on `[0, 1]`.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Her lift coefficient at angle of attack `alpha` (radians), as a multiple
/// of the one at no angle: 1 at none, rising straight to `c_max` at the
/// stall angle - the same line through negative angles - and past either
/// stall falling away to [`STALL_LIFT`] of its edge over [`STALL_BLEND_DEG`].
fn lift_coefficient(alpha: f32, c_max: f32) -> f32 {
    let stall = STALL_ANGLE_DEG.to_radians();
    let blend = STALL_BLEND_DEG.to_radians();
    let line = |a: f32| 1.0 + (c_max - 1.0) * a / stall;
    if alpha > stall {
        let edge = line(stall);
        edge + (edge * STALL_LIFT - edge) * smooth((alpha - stall) / blend)
    } else if alpha < -stall {
        let edge = line(-stall);
        edge + (edge * STALL_LIFT - edge) * smooth((-stall - alpha) / blend)
    } else {
        line(alpha)
    }
}

/// What the air does to her this step: the force (lift, drag and keel) and
/// her angle of attack (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Air {
    force: Vec3,
    alpha: f32,
}

impl Air {
    /// Whether her wings have stalled.
    fn stalled(self) -> bool {
        self.alpha > STALL_ANGLE_DEG.to_radians()
    }
}

/// The air's force on a body of `mass` moving at `velocity` with its nose
/// along `forward`, its right wing along `right` and its up along `up`.
fn air_force(
    velocity: Vec3,
    [forward, right, up]: [Vec3; 3],
    mass: f32,
    drag_coefficient: f32,
    wings: Wings,
) -> Air {
    let speed = velocity.length();
    if speed < 1e-3 {
        return Air {
            force: Vec3::ZERO,
            alpha: 0.0,
        };
    }
    let along = velocity.dot(forward);
    // Dynamic pressure is the whole airspeed's: at the stall angle the
    // forward share alone left her `cos^2 15` short of her weight at her
    // own stall speed.
    let dynamic = mass * G * (speed / wings.cruise).powi(2);
    // The air turns her path and slows her no faster than a step settles
    // ([`AIR_RESPONSE_MAX`]): no force past `most`, and no lift that climbs
    // with her angle of attack faster than that - a wing too steep for the
    // step flies as the steepest one it can.
    let most = mass * speed * AIR_RESPONSE_MAX;
    let c_max = wings
        .c_max
        .min(1.0 + most * STALL_ANGLE_DEG.to_radians() / dynamic);
    // The angle the air meets her wings at: positive with the nose over the
    // path. Flying backwards, her wings lift nothing.
    let (alpha, coefficient) = if along > 0.1 {
        let alpha = (-velocity.dot(up)).atan2(along);
        (alpha, lift_coefficient(alpha, c_max))
    } else {
        (0.0, 0.0)
    };
    let lift = (dynamic * coefficient).clamp(-most, most);
    let drag = (drag_coefficient * speed * speed
        + INDUCED_DRAG * lift.abs() * coefficient.abs() / c_max)
        .min(most);
    let keel = mass * KEEL_RATE * (speed / wings.cruise).min(2.0) * velocity.dot(right);
    // Lift stands square to the air, in the plane of her wings - so it
    // leans with her bank - rather than along her up: tipped back with her
    // nose, it took `lift sin(alpha)` out of her speed as phantom drag, and
    // a long turn bled her from 16 m/s to 12.
    let toward = velocity / speed;
    let square = (up - toward * up.dot(toward)).normalize_or(up);
    Air {
        force: square * lift - toward * drag - right * keel,
        alpha,
    }
}

/// Her bank (radians): positive with her right wing down, past a quarter
/// turn when she is past her side.
fn bank_of(right: Vec3, up: Vec3) -> f32 {
    (-right.y).atan2(up.y)
}

/// Her flight path's climb (radians) - or, under half her stall speed
/// along her nose, her nose's: a plane dropped onto her wheels at a spawn
/// is not diving, and levelling that "path" would pitch her up as she
/// falls.
fn path_of(velocity: Vec3, forward: Vec3, wings: Wings) -> f32 {
    let toward = if velocity.dot(forward) > 0.5 * wings.stall {
        velocity.normalize()
    } else {
        forward
    };
    toward.y.clamp(-1.0, 1.0).asin()
}

/// Turn rates (rad/s) about her body's axes: roll about her nose (positive
/// takes the right wing down), pitch about her right wing (positive lifts
/// the nose), yaw about her up (positive swings the nose left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rates {
    roll: f32,
    pitch: f32,
    yaw: f32,
}

/// How she is flying this step, as her levelling reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Attitude {
    /// Her bank (radians): positive with her right wing down ([`bank_of`]).
    bank: f32,
    /// Her flight path's climb (radians, [`path_of`]).
    path: f32,
    /// Her sideslip (radians): positive with her path to the right of her
    /// nose.
    slip: f32,
    /// Her speed along her nose (m/s).
    forward_speed: f32,
    /// Her wings have stalled.
    stalled: bool,
}

/// The rates she is asked for in the air: each key's own, and with none
/// held the levelling's - wings to level, path to level, nose onto the
/// path. A stalled wing drops her nose whatever is held.
fn air_rates(stick: Stick, p: &AirplaneParams, flying: Attitude, wings: Wings) -> Rates {
    let Attitude {
        bank,
        path,
        slip,
        forward_speed,
        stalled,
    } = flying;
    let roll_rate = p.roll_rate.0.to_radians();
    let pitch_rate = p.pitch_rate.0.to_radians();
    let yaw_rate = p.yaw_rate.0.to_radians();
    let want_bank = f32::from(stick.roll) * MAX_BANK_DEG.to_radians();
    let roll = (BANK_GAIN * (want_bank - bank)).clamp(-roll_rate, roll_rate);
    let speed = forward_speed.max(wings.stall);
    // A banked turn's own pitch rate - her nose comes round the turn as
    // her path does - so a level turn needs no climb or sink to ask for it.
    let banked = bank.clamp(-MAX_BANK_DEG.to_radians(), MAX_BANK_DEG.to_radians());
    let turning = G * banked.sin() * banked.tan() / speed;
    let pitch = if stalled {
        -pitch_rate
    } else if stick.pitch != 0 {
        f32::from(stick.pitch) * pitch_rate
    } else {
        (turning - PATH_GAIN * path).clamp(-pitch_rate, pitch_rate)
    };
    let yaw = if stick.yaw != 0 {
        f32::from(stick.yaw) * yaw_rate
    } else {
        // Onto her path, and round with her bank's turn as it begins.
        let turn = G * banked.sin() / speed;
        (-SLIP_GAIN * slip - turn).clamp(-2.0 * yaw_rate, 2.0 * yaw_rate)
    };
    Rates { roll, pitch, yaw }
}

/// The torque (N m, along the axis) that drives a body of `inertia` about
/// one axis from turning at `now` toward `want` (rad/s): the controller's
/// pull, and a feed-forward of the body's own angular `damping` so the rate
/// it settles at is the one asked for.
fn rate_torque(inertia: f32, want: f32, now: f32, damping: f32) -> f32 {
    inertia * (RESPONSE * (want - now) + damping * want)
}

/// A uniform box's moments of inertia about its x (pitch), y (yaw) and z
/// (roll) axes - avian's own for a body whose only collider is the box.
fn box_inertia(mass: f32, [hx, hy, hz]: [f32; 3]) -> Vec3 {
    Vec3::new(
        mass * (hy * hy + hz * hz) / 3.0,
        mass * (hx * hx + hz * hz) / 3.0,
        mass * (hx * hx + hy * hy) / 3.0,
    )
}

/// One of her three wheels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Wheel {
    /// Where its leg meets the box's floor, in her own frame (nose at -Z).
    at: Vec3,
    /// How far it reaches under that, unloaded (m).
    reach: f32,
    /// The share of her weight its spring is sized for: what it carries
    /// standing level.
    share: f32,
    /// A main, which grips sideways and brakes - or the castering tail
    /// wheel, which does neither.
    main: bool,
}

/// Her wheels, from the record's layout (#1617): two mains `gear_track` of
/// the half-width out and `gear_ahead` of the half-length ahead of the box's
/// middle, and a tail wheel `tail_wheel_aft` behind it reaching
/// `tail_gear_share` of the mains' reach. What each is sized to carry is its
/// lever about her centre of mass along her - the tail wheel the mains' arm
/// over both - so standing level each spring sinks the same [`GEAR_SAG`] of
/// its own reach; standing pitched, her legs swing under her and the
/// shares move ([`reach_for_stance`]).
fn wheels(p: &AirplaneParams) -> [Wheel; 3] {
    let [hx, hy, hz] = p.chassis_half_extents.0;
    let ahead = p.gear_ahead.0 * hz;
    let aft = p.tail_wheel_aft.0 * hz;
    let mass_ahead = p.center_of_mass_ahead.0 * hz;
    // A wheel on her centre of mass would carry everything; held a twentieth
    // of the half-length off it, none is left carrying nothing.
    let to_mains = (ahead - mass_ahead).max(0.05 * hz);
    let to_tail = (aft + mass_ahead).max(0.05 * hz);
    let tail = to_mains / (to_mains + to_tail);
    let main = (1.0 - tail) * 0.5;
    let x = p.gear_track.0 * hx;
    let reach = p.gear_height.0;
    [
        Wheel {
            at: Vec3::new(-x, -hy, -ahead),
            reach,
            share: main,
            main: true,
        },
        Wheel {
            at: Vec3::new(x, -hy, -ahead),
            reach,
            share: main,
            main: true,
        },
        Wheel {
            at: Vec3::new(0.0, -hy, aft),
            reach: reach * p.tail_gear_share.0,
            share: tail,
            main: false,
        },
    ]
}

/// Her wheels' reaches, `(gear_height, tail_gear_share)`, to stand her
/// `pitch` nose-up (radians) on level ground with her box's floor `mains`
/// metres over it above her mains and `tail` above her tail wheel, plumb:
/// what a drawn airplane's record carries so that the wheels she draws
/// stand on the ground (#1617). Her layout and centre of mass are read,
/// never moved.
///
/// Her rays fall plumb whatever her pitch, so the heights are plumb ones.
/// Each spring sinks [`GEAR_SAG`] of its reach under the share of her
/// weight it is sized for ([`wheels`]: its lever about her centre of mass,
/// along her) and in proportion under what it carries - and standing
/// pitched, the two differ. Her legs meet her box's floor, under her centre
/// of mass, so a nose-up stance swings them forward under her and moves
/// weight off her mains onto her tail wheel. Reached in her own frame, the
/// seeded airplane's mains stood a centimetre over the ground at ten
/// degrees (the #1617 review).
pub(crate) fn reach_for_stance(
    p: &AirplaneParams,
    pitch: f32,
    mains: f32,
    tail: f32,
) -> (f32, f32) {
    let [main, _, tail_wheel] = wheels(p);
    let centre = Vec3::new(
        0.0,
        0.0,
        -p.center_of_mass_ahead.0 * p.chassis_half_extents.0[2],
    );
    // Nose-up turns her nose, -Z, toward +Y: a turn about +X.
    let standing = Quat::from_rotation_x(pitch);
    // How far ahead of her centre of mass a leg's top stands, along the
    // ground.
    let ahead = |wheel: &Wheel| -(standing * (wheel.at - centre)).z;
    let (to_mains, to_tail) = (ahead(&main), -ahead(&tail_wheel));
    let on_tail = (to_mains / (to_mains + to_tail)).clamp(0.0, 1.0);
    // What a spring leaves of its reach carrying `carried` of her weight,
    // sized for `share` of it - never under a tenth, whatever the layout.
    let leaves = |carried: f32, share: f32| (1.0 - GEAR_SAG * carried / share).max(0.1);
    let main_reach = mains / leaves(1.0 - on_tail, 2.0 * main.share);
    let tail_reach = tail / leaves(on_tail, tail_wheel.share);
    (main_reach, tail_reach / main_reach)
}

/// A wheel's spring rate (N/m) and damping (N s/m), carrying `load_mass`
/// (kg) at rest on a leg `reach` long: it sinks [`GEAR_SAG`] of its reach
/// under it, damped at [`GEAR_DAMPING_RATIO`] of critical.
fn gear_spring(load_mass: f32, reach: f32) -> (f32, f32) {
    let reach = reach.max(0.02);
    let stiffness = load_mass * G / (GEAR_SAG * reach);
    let damping = 2.0 * GEAR_DAMPING_RATIO * (stiffness * load_mass).sqrt();
    (stiffness, damping)
}

/// How much of a wheel's last step to a stop it may take - what is left of
/// its roll along the ground, taken out at once: all of it, unless the two
/// mains, `lever` metres out to her sides each carrying `carries` kg on
/// `grip`, would take out more than [`GEAR_YAW_STEP`] of her yaw in that
/// step against her `yaw_inertia` (kg m^2). The tail wheel, on her
/// centreline, never would.
fn stop_share(carries: f32, grip: f32, yaw_inertia: f32, lever: f32) -> f32 {
    let arm = 2.0 * carries * grip * lever * lever;
    if arm > 0.0 {
        (GEAR_YAW_STEP * yaw_inertia / arm).min(1.0)
    } else {
        1.0
    }
}

/// How hard a wheel's damper may resist her roll (N s/m), its leg `lever`
/// metres out to her side: its own `damping`, unless the two mains together
/// would take out more than [`GEAR_ROLL_STEP`] of her roll in one fixed
/// step of `dt` seconds against her `roll_inertia` (kg m^2). The tail wheel,
/// on her centreline, never would.
fn roll_damping(damping: f32, roll_inertia: f32, lever: f32, dt: f32) -> f32 {
    let arm = 2.0 * lever * lever * dt;
    if arm > 0.0 {
        damping.min(GEAR_ROLL_STEP * roll_inertia / arm)
    } else {
        damping
    }
}

/// How far behind her bank her path's turn answers it (s), as a pilot
/// reckons a roll-out ([`Handling::roll_out_turn`]): her lift is still the
/// steeper bank's for a moment as her wings come level, and her nose leads
/// her path round, which her keel follows. Measured on the bench: from her
/// steepest bank she turns about 40 % further than the bank alone turns
/// her, and with this the reckoning is within a degree of the turn made.
#[cfg(unix)]
const PATH_LAG_SECS: f32 = 0.15;

/// What a pilot who lets go of a key before her turn is done needs to know
/// of her handling - the agent's, which steers at its own frame rather
/// than the physics' (#1615): how long a turn takes to die once its key is
/// let go, and how much further a bank turns her as her wings level.
/// Unix-only like the agent, the one reader: ungated, it is dead code on
/// wasm32.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Handling {
    /// The record's `roll_rate` (rad/s).
    roll_rate: f32,
    /// How fast the controller and her own angular damping together close
    /// on the turn rate asked for (1/s): the damping's feed-forward in
    /// [`rate_torque`] makes the two one lag.
    response: f32,
    /// Her stall speed (m/s).
    stall: f32,
}

#[cfg(unix)]
impl Handling {
    pub(crate) fn of(p: &AirplaneParams) -> Self {
        Self {
            roll_rate: p.roll_rate.0.to_radians(),
            response: RESPONSE + p.angular_damping.0,
            stall: Wings::of(p).stall,
        }
    }

    /// Her stall speed as her wings fly it (m/s): the record's, floored.
    pub(crate) fn stall(self) -> f32 {
        self.stall
    }

    /// How long a turn takes to die once its key is let go with nothing
    /// else asked of that axis (s) - as a steer on the ground does.
    pub(crate) fn settle_secs(self) -> f32 {
        1.0 / self.response
    }

    /// How far her path still turns (radians, clockwise seen from above)
    /// once A and D are let go: banked `bank` (radians, right wing down),
    /// rolling at `rolling` (rad/s, the same way) and flying at `speed`
    /// (m/s). Her wings level as [`air_rates`] levels them - at the roll
    /// rate's cap, then easing in - and her path turns as a bank
    /// [`PATH_LAG_SECS`] behind hers does, so it is run at the physics' own
    /// step rather than solved.
    pub(crate) fn roll_out_turn(self, bank: f32, rolling: f32, speed: f32) -> f32 {
        const STEP: f32 = 1.0 / 64.0;
        let most = MAX_BANK_DEG.to_radians();
        let speed = speed.max(self.stall);
        // A lag of a roll is that roll's rate times the lag behind it.
        let mut lagged = bank - PATH_LAG_SECS * rolling;
        let (mut bank, mut rolling, mut turned) = (bank, rolling, 0.0);
        // Ten seconds is the roll-out from the steepest bank at 5 degrees
        // a second; a record that rolls slower than that is close enough.
        for _ in 0..640 {
            let want = (-BANK_GAIN * bank).clamp(-self.roll_rate, self.roll_rate);
            rolling += self.response * (want - rolling) * STEP;
            bank += rolling * STEP;
            lagged += (bank - lagged) * STEP / PATH_LAG_SECS;
            turned += G * lagged.clamp(-most, most).tan() / speed * STEP;
            if bank.abs().max(lagged.abs()).max(rolling.abs()) < 1e-4 {
                break;
            }
        }
        turned
    }
}

/// Read the keys into her [`Stick`] - the one airplane system the owner's
/// typing stands down (#821).
pub(super) fn read_airplane_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Stick, (With<LocalPlayer>, With<AirplanePreset>)>,
) {
    let Ok(mut stick) = query.single_mut() else {
        return;
    };
    let held = |keys: &[KeyCode]| keys.iter().any(|key| keyboard.pressed(*key));
    let axis = |plus: bool, minus: bool| i8::from(plus) - i8::from(minus);
    let next = Stick {
        roll: axis(
            held(&[KeyCode::KeyD, KeyCode::ArrowRight]),
            held(&[KeyCode::KeyA, KeyCode::ArrowLeft]),
        ),
        pitch: axis(
            held(&[KeyCode::KeyS, KeyCode::ArrowDown]),
            held(&[KeyCode::KeyW, KeyCode::ArrowUp]),
        ),
        yaw: axis(held(&[KeyCode::KeyQ]), held(&[KeyCode::KeyE])),
        up: held(&[KeyCode::Space]),
        down: held(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    };
    // Guarded: a `&mut` through a query stamps a change tick even when the
    // value is the same.
    if *stick != next {
        *stick = next;
    }
}

/// The flight model, every fixed step: the throttle, the gear, the air,
/// the thrust and the control - see the module docs.
///
/// Ungated on the owner's typing (#821); parked during the editor's freeze
/// (#814), which holds the chassis wholesale - its keys spent all the same,
/// so none waits for the step after it. During travel no key counts and
/// the engine is cut - power would fly her away from the portal while the
/// destination loads - but the air and the gear still carry her.
#[allow(clippy::type_complexity, clippy::too_many_lines)]
pub(super) fn fly_airplane(
    live: Res<LiveAvatarRecord>,
    time: Res<Time>,
    traveling: Option<Res<TravelingTo>>,
    sensors: Query<Entity, With<Sensor>>,
    spatial: SpatialQuery,
    mut query: Query<
        (
            Entity,
            Forces,
            &mut Throttle,
            &mut Stick,
            Has<VisualsEditFreeze>,
        ),
        (With<LocalPlayer>, With<AirplanePreset>),
    >,
) {
    let LocomotionConfig::Airplane(p) = &live.0.locomotion else {
        return;
    };
    let Ok((body, mut forces, mut throttle, mut stick, frozen)) = query.single_mut() else {
        return;
    };
    // This step's keys - and none left for the next unless they are read
    // again.
    let keys = if traveling.is_some() {
        Stick::default()
    } else {
        *stick
    };
    if *stick != Stick::default() {
        *stick = Stick::default();
    }
    if frozen {
        return;
    }
    let dt = time.delta_secs().max(1e-4);

    // The throttle holds where the keys leave it; closed and still held
    // down, Shift is the brakes.
    let mut setting = throttle.0;
    if keys.up {
        setting += THROTTLE_RATE * dt;
    }
    if keys.down {
        setting -= THROTTLE_RATE * dt;
    }
    let setting = setting.clamp(0.0, 1.0);
    if setting != throttle.0 {
        throttle.0 = setting;
    }
    let braking = keys.down && setting <= 0.0;

    let pose = super::physics_pose(forces.position(), forces.rotation());
    let (forward, right, up) = (
        pose.forward().as_vec3(),
        pose.right().as_vec3(),
        pose.up().as_vec3(),
    );
    let velocity = forces.linear_velocity();
    let spin = forces.angular_velocity();
    let mass = p.mass.0;
    let half = p.chassis_half_extents.0;
    let wings = Wings::of(p);
    let inertia = box_inertia(mass, half);

    // The gear: three wheels, cast like the car's springs.
    let mut carried = 0.0;
    if up.y >= GEAR_TILT_LIMIT_DEG.to_radians().cos() {
        let filter = super::ground_ray_filter(body, sensors.iter());
        let ahead = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let aside = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
        let local = pose.compute_transform();
        // Her centre of mass, and her roll about it alone.
        let centre =
            local.transform_point(Vec3::new(0.0, 0.0, -p.center_of_mass_ahead.0 * half[2]));
        let roll = forward * spin.dot(forward);
        for wheel in wheels(p) {
            let at = local.transform_point(wheel.at);
            let Some(hit) =
                spatial.cast_ray(at, Dir3::NEG_Y, wheel.reach + GEAR_SLACK_M, true, &filter)
            else {
                continue;
            };
            let carries = wheel.share * mass;
            let (stiffness, damping) = gear_spring(carries, wheel.reach);
            // About her centre of mass, which is not the box's middle when
            // the record moves it.
            let moving = forces.velocity_at_point(at);
            // What her roll alone sinks the leg's top by is damped no harder
            // than one step can follow ([`GEAR_ROLL_STEP`]).
            let held = roll_damping(damping, inertia.z, wheel.at.x, dt);
            let sinking = if held < damping {
                let rocking = roll.cross(at - centre).y;
                moving.y - rocking * (1.0 - held / damping)
            } else {
                moving.y
            };
            let compression = (wheel.reach - hit.distance).max(0.0);
            let load = if compression > 0.0 {
                (stiffness * compression - damping * sinking).max(0.0)
            } else {
                0.0
            };
            carried += load;
            // What the wheel does along the ground scales with how much of
            // her it carries: nothing, lifting off.
            let grip = (load / (carries * G)).clamp(0.0, 1.5);
            let rolling = moving.dot(ahead);
            let slowing = ROLLING_DRAG
                + if braking && wheel.main {
                    p.brake.0
                } else {
                    0.0
                };
            // No more than stops the wheel, so a brake never backs her up -
            // nor, far out to her side, turns her the other way
            // ([`GEAR_YAW_STEP`]).
            let stop = stop_share(carries, grip, inertia.y, wheel.at.x);
            let along = -rolling.signum() * slowing.min(stop * rolling.abs() / dt);
            let mut push = Vec3::Y * load + ahead * (carries * grip * along);
            if wheel.main {
                // Two mains share her sideways grip; the tail wheel castors.
                push -= aside * (0.5 * mass * GEAR_GRIP * grip * moving.dot(aside));
            }
            forces.apply_force_at_point(push, at);
        }
    }
    // The share of her weight on her wheels: all of it parked, none flying.
    let ground = (carried / (mass * G)).clamp(0.0, 1.0);

    // The air.
    let air = air_force(
        velocity,
        [forward, right, up],
        mass,
        p.drag_coefficient.0,
        wings,
    );
    forces.apply_force(air.force);

    // The engine.
    let speed = velocity.length();
    if traveling.is_none() && thrust_applies(up, speed, wings) {
        forces.apply_force(forward * p.thrust.0 * setting);
    }

    // The control.
    let damping = p.angular_damping.0;
    let forward_speed = velocity.dot(forward).max(0.0);
    let slip = velocity.dot(right).atan2(forward_speed.max(1e-3));
    let flying = forward_speed > 0.5 * wings.stall;
    let rates = air_rates(
        keys,
        p,
        Attitude {
            bank: bank_of(right, up),
            path: path_of(velocity, forward, wings),
            slip: if flying { slip } else { 0.0 },
            forward_speed,
            stalled: air.stalled(),
        },
        wings,
    );
    // The control surfaces answer the air flowing over them: fully at her
    // stall speed and over, and never less than a quarter in the air, so a
    // stall can always be flown out of.
    let flow = (forward_speed / wings.stall).powi(2);
    let in_air = (1.0 - ground) * flow.clamp(0.25, 1.0);
    let mut torque =
        forward * rate_torque(inertia.z, rates.roll, spin.dot(forward), damping) * in_air
            + up * rate_torque(inertia.y, rates.yaw, spin.dot(up), damping) * in_air;
    let pitch_now = spin.dot(right);
    let pitch_torque = rate_torque(inertia.x, rates.pitch, pitch_now, damping);
    torque += right
        * if keys.pitch != 0 {
            // A held pitch key works on the ground too, as her speed lets
            // it - how she rotates to take off.
            pitch_torque * (in_air + ground * flow.min(1.0))
        } else {
            pitch_torque * in_air
        };
    // On her wheels she steers about the vertical, A and D with the rudder.
    let steer = (f32::from(keys.yaw) - f32::from(keys.roll)).clamp(-1.0, 1.0);
    let steer_rate =
        steer * (p.yaw_rate.0.to_radians() * GROUND_STEER).min(GROUND_LATERAL / speed.max(0.1));
    torque += Vec3::Y * rate_torque(inertia.y, steer_rate, spin.y, damping) * ground;
    forces.apply_torque(torque);
}

/// Restoring torque for an airplane that has ended up on its back
/// (#1240 f162), modelled directly on the car's `upright_assist_torque`.
///
/// `None` while the tilt is inside
/// [`cfg::AIRPLANE_UPRIGHT_ENGAGE_TILT_DEGREES`] - a plane banks and rolls
/// as ordinary flight, and an assist that fought that would be flying the
/// aircraft. Past it, rotate `up` toward world-up, falling back to the
/// roll axis when the chassis is DEAD inverted (where `up × world-up`
/// degenerates and the plane could otherwise perch on its back forever),
/// minus a spin-damping term so it settles level rather than oscillating.
fn upright_assist_torque(up: Vec3, right: Vec3, ang_vel: Vec3, mass: f32) -> Option<Vec3> {
    if up.dot(Vec3::Y) >= cfg::AIRPLANE_UPRIGHT_ENGAGE_TILT_DEGREES.to_radians().cos() {
        return None;
    }
    let mut axis = up.cross(Vec3::Y);
    if axis.length_squared() < cfg::CAR_UPRIGHT_DEGENERATE_SQ {
        axis = right;
    }
    let restoring = axis.normalize_or_zero() * (mass * cfg::AIRPLANE_UPRIGHT_ASSIST_ACCEL);
    let damping = -ang_vel * (mass * cfg::AIRPLANE_UPRIGHT_ASSIST_DAMPING);
    Some(restoring + damping)
}

/// Whether her engine pushes at this attitude and speed (#1240 f162).
/// Pure so the boundary is testable.
///
/// An inverted plane on the ground is being driven INTO the terrain by its
/// own engine, and `respawn_if_fallen` cannot rescue it: it is lying ON
/// the ground, not 20 m below it. So the engine is cut there - but only
/// under her stall speed: inverted at the top of a loop she is flying, and
/// needs it.
fn thrust_applies(up: Vec3, speed: f32, wings: Wings) -> bool {
    speed >= wings.stall
        || up.dot(Vec3::Y) > cfg::AIRPLANE_THRUST_CUT_TILT_DEGREES.to_radians().cos()
}

/// Right an airplane that has flipped onto its back. Registered ungated
/// beside the other three presets' assists (#1240 f162) - the airplane was
/// simply missed, and a crashed plane is exactly the state where the owner
/// is least able to do anything about it. It stands down while she flies
/// (#1615): inverted over the top of a loop is flying, and her own
/// levelling is what rights her in the air.
#[allow(clippy::type_complexity)]
pub(super) fn apply_airplane_uprighting(
    live: Res<LiveAvatarRecord>,
    mut query: Query<Forces, (With<LocalPlayer>, With<AirplanePreset>)>,
) {
    let LocomotionConfig::Airplane(p) = &live.0.locomotion else {
        return;
    };
    let Ok(mut forces) = query.single_mut() else {
        return;
    };
    if forces.linear_velocity().length() >= Wings::of(p).stall {
        return;
    }
    let global_tf = &super::physics_pose(forces.position(), forces.rotation());
    let up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();
    let ang_vel = forces.angular_velocity();
    if let Some(torque) = upright_assist_torque(up, right, ang_vel, p.mass.0) {
        forces.apply_torque(torque);
    }
}

#[cfg(test)]
mod tests;
