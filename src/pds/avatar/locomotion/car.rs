//! Car preset - ground vehicle with raycast suspension + steering + handbrake,
//! and an air model for when its wheels leave the ground.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Car preset: ground vehicle. 4-corner raycast suspension (same approach
/// as the hover-boat, no buoyancy), W/S throttle/reverse, A/D steer, Space
/// handbrake (no forward force, and lateral grip multiplied by
/// `handbrake_grip_factor` - less grip at the default 0.25, so the rear
/// slides). Sinks in water.
///
/// Throttle, steering and grip act through the wheels, so each is scaled
/// by the share of the four suspension corners on the ground (#1524): a
/// car in the air neither accelerates nor steers, and flies ballistically.
/// In their place, while any wheel is off the ground, the air model below
/// takes over - W/S pitch, A/D yaw and Q/E roll the chassis (W/S/A/D only
/// when pressed in the air: see `air_control_accel`), it levels itself in
/// pitch and roll while no counted key holds that axis, and it swaps the
/// ground dampings for the lighter air ones.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CarParams {
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    /// Lateral-grip multiplier applied while Space (handbrake) is held.
    /// Below 1 it is LESS grip, which is what lets the rear slide out.
    pub handbrake_grip_factor: Fp,
    /// Tilt (degrees from upright) beyond which the uprighting assist
    /// engages (#804). Below it the assist stays silent so cornering lean
    /// and slope driving are never fought. Promoted from
    /// `CAR_UPRIGHT_ASSIST_COS` by #876 (60° ↔ cos 0.5); field-level
    /// serde default keeps pre-#876 records at the historical feel.
    #[serde(default = "default_upright_engage_tilt")]
    pub upright_engage_tilt_degrees: Fp,
    /// Mass-normalised righting acceleration (rad/s²-equivalent) applied
    /// past the engage tilt, near the ground. Since #1524 at least 1.5 times
    /// what gravity needs to tip the lying box back over the edge it pivots
    /// on, so a flat roof or a tall box's side cannot hold a car down; 0
    /// turns the assist off, that floor included, and nothing else rights a
    /// car lying on the ground.
    #[serde(default = "default_upright_accel")]
    pub upright_assist_accel: Fp,
    /// Mass-normalised spin damping while righting, so the chassis
    /// settles level instead of oscillating.
    #[serde(default = "default_upright_damping")]
    pub upright_assist_damping: Fp,
    /// Centre-of-mass drop as a fraction of the chassis half-height,
    /// below the body origin (#804's anti-rollover lever). Applied when
    /// the chassis is (re)built, like the collider dimensions.
    #[serde(default = "default_center_of_mass_drop")]
    pub center_of_mass_drop: Fp,
    /// Linear damping (1/s) while no wheel is on the ground (#1524). With
    /// all four down the chassis uses `linear_damping`, and in between the
    /// two blend by the share of wheels down. Small, so a jump flies nearly
    /// ballistically: the ground drag used to act in the air too, holding a
    /// fall to about g / `linear_damping`.
    #[serde(default = "default_air_linear_damping")]
    pub air_linear_damping: Fp,
    /// Angular damping (1/s) while no wheel is on the ground, blended with
    /// `angular_damping` the same way. Light, so a spin carries through a
    /// flight instead of dying in a fraction of a second.
    #[serde(default = "default_air_angular_damping")]
    pub air_angular_damping: Fp,
    /// Mass-normalised air-control torque: W/S pitch the nose down/up, A/D
    /// yaw and Q/E roll left/right, each as a torque of mass times this
    /// about the chassis' own axes, scaled by the share of wheels OFF the
    /// ground. W/S/A/D count only when pressed with no wheel down: held
    /// while a wheel touches they are throttle and steering, and count once
    /// let go and pressed again - so a driver holding W or a turn off a ramp
    /// does not tip the car, and uneven ground never pitches it. Q/E do
    /// nothing on the ground and count whenever held. No key counts while
    /// the car lies on the ground - on its roof, a side or its nose - which
    /// is the uprighting assist's to right.
    #[serde(default = "default_air_control_accel")]
    pub air_control_accel: Fp,
    /// Mass-normalised stiffness of the air self-levelling: while a wheel
    /// is off the ground and no counted air key (see `air_control_accel`)
    /// holds that axis, a torque of mass times this times the sine of the
    /// tilt turns the chassis back toward level in pitch and roll (never
    /// yaw), damped so it settles instead of swinging. Scaled by the share of
    /// wheels off the ground; 0 disables. Passive, so it keeps on while the
    /// player types; for the air only, so a car lying on the ground is left
    /// to the uprighting assist.
    #[serde(default = "default_air_level_accel")]
    pub air_level_accel: Fp,
}

/// Serde fallbacks for records published before #876 - the values the
/// uprighting/centre-of-mass code hard-coded (formerly the
/// `config::rover::CAR_UPRIGHT_*` constants). Shared with `Default` so an
/// old record and a fresh preset agree.
fn default_upright_engage_tilt() -> Fp {
    Fp(60.0)
}
fn default_upright_accel() -> Fp {
    Fp(2.5)
}
fn default_upright_damping() -> Fp {
    Fp(0.8)
}
fn default_center_of_mass_drop() -> Fp {
    Fp(0.6)
}

/// Serde fallbacks for records published before the air model (#1524),
/// which carry none of its four fields - so these are what every published
/// skiff flies with. Shared with `Default` so an old record and a fresh
/// preset agree. Measured on the drive bench (`player::sim::DriveBench`);
/// the numbers behind them are on #1524.
fn default_air_linear_damping() -> Fp {
    Fp(0.05)
}
fn default_air_angular_damping() -> Fp {
    Fp(1.0)
}
fn default_air_control_accel() -> Fp {
    Fp(4.0)
}
fn default_air_level_accel() -> Fp {
    Fp(6.0)
}

impl Default for CarParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            chassis_half_extents: Fp3([0.8, 0.4, 1.6]),
            mass: Fp(900.0),
            linear_damping: Fp(0.8),
            angular_damping: Fp(4.0),
            suspension_rest_length: Fp(0.6),
            // Stiffer than hover-boat: cars need quick response on terrain.
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS * 4.0),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING * 2.5),
            drive_force: Fp(8_000.0),
            turn_torque: Fp(1_800.0),
            lateral_grip: Fp(20_000.0),
            handbrake_grip_factor: Fp(0.25),
            upright_engage_tilt_degrees: default_upright_engage_tilt(),
            upright_assist_accel: default_upright_accel(),
            upright_assist_damping: default_upright_damping(),
            center_of_mass_drop: default_center_of_mass_drop(),
            air_linear_damping: default_air_linear_damping(),
            air_angular_damping: default_air_angular_damping(),
            air_control_accel: default_air_control_accel(),
            air_level_accel: default_air_level_accel(),
        }
    }
}

impl LocomotionPreset for CarParams {
    const KIND_TAG: &'static str = "car";
    const DISPLAY_LABEL: &'static str = "Car";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 50_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.suspension_rest_length = clamp_pos(self.suspension_rest_length, 0.001, 5.0);
        self.suspension_stiffness = clamp_pos(self.suspension_stiffness, 0.0, 200_000.0);
        self.suspension_damping = clamp_pos(self.suspension_damping, 0.0, 20_000.0);
        self.drive_force = clamp_pos(self.drive_force, 0.0, 200_000.0);
        self.turn_torque = clamp_pos(self.turn_torque, 0.0, 50_000.0);
        self.lateral_grip = clamp_pos(self.lateral_grip, 0.0, 200_000.0);
        self.handbrake_grip_factor = clamp_pos(self.handbrake_grip_factor, 0.0, 100.0);
        // Floor of 15°: an assist that engages inside ordinary cornering
        // lean would fight normal driving every turn.
        self.upright_engage_tilt_degrees = clamp_pos(self.upright_engage_tilt_degrees, 15.0, 90.0);
        self.upright_assist_accel = clamp_pos(self.upright_assist_accel, 0.0, 50.0);
        self.upright_assist_damping = clamp_pos(self.upright_assist_damping, 0.0, 20.0);
        self.center_of_mass_drop = clamp_pos(self.center_of_mass_drop, 0.0, 1.0);
        // The air model (#1524). The damping ceilings sit well above the
        // ground ones a record carries (0.45-0.9 and 3-5 across the fleet),
        // so an owner can hand the air the ground's feel back; the two
        // accelerations share the upright assist's ceiling.
        self.air_linear_damping = clamp_pos(self.air_linear_damping, 0.0, 10.0);
        self.air_angular_damping = clamp_pos(self.air_angular_damping, 0.0, 20.0);
        self.air_control_accel = clamp_pos(self.air_control_accel, 0.0, 50.0);
        self.air_level_accel = clamp_pos(self.air_level_accel, 0.0, 50.0);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Car(Box::new(self))
    }
}
