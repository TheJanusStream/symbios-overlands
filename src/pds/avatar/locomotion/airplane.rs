//! Airplane preset - a light airplane's flight model (#1615): a throttle
//! that holds, lift along the wings that grows with speed and angle of
//! attack and stalls, control as rates the plane's own inertia is driven
//! at, wings and flight path that level when the keys are let go, and
//! landing gear to taxi, steer and brake on. See `player::airplane` for the
//! physics, and its module docs for the keys.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos, clamp_unit};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Airplane preset: W/S pitch, A/D bank, Q/E rudder, Space/Shift throttle.
///
/// # Five fields no system reads since #1615
///
/// `pitch_torque`, `roll_torque`, `yaw_torque`, `lift_per_speed` and
/// `cruise_throttle` belonged to the arcade model this replaced, and are
/// still written, with their old defaults, because a client built before
/// it requires them: a record without them would not decode there. The
/// controls they drove are rates now (`roll_rate`, `pitch_rate`,
/// `yaw_rate`), lift is `cruise_speed`'s, and the throttle holds where it
/// is set rather than idling at a cruise fraction. The editor no longer
/// shows them.
///
/// A record published before #1615 flies the new model with the numbers it
/// was saved with: its 1500 N of thrust on 40 kg makes it a rocket, until
/// its owner sets the Locomotion tab's sliders or picks the preset afresh.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AirplaneParams {
    /// Fuselage cuboid collider half-extents (m). The landing gear stands
    /// under it ([`Self::gear_height`]); the wings, like a rotor's disc,
    /// are drawn outside it.
    pub chassis_half_extents: Fp3,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    /// Full-throttle thrust along the nose (N).
    pub thrust: Fp,
    /// Retired (#1615): see the struct docs.
    pub pitch_torque: Fp,
    /// Retired (#1615): see the struct docs.
    pub roll_torque: Fp,
    /// Retired (#1615): see the struct docs.
    pub yaw_torque: Fp,
    /// Retired (#1615): see the struct docs.
    pub lift_per_speed: Fp,
    /// Air resistance along the velocity (N per (m/s)^2): with `thrust` it
    /// sets her top speed - a little under `sqrt(thrust / drag_coefficient)`
    /// in level flight, her damping and the drag her lift costs taking the
    /// rest: 25 m/s for the default.
    pub drag_coefficient: Fp,
    /// Her STALL speed (m/s): the slowest her wings can carry her weight,
    /// at the steepest angle of attack they take before they stall. Since
    /// #1615; before it, the speed under which lift simply stopped.
    pub min_airspeed: Fp,
    /// Retired (#1615): see the struct docs.
    #[serde(default = "default_cruise_throttle")]
    pub cruise_throttle: Fp,
    /// The speed her wings carry her weight at with the nose on the flight
    /// path (m/s) - level flight at no angle of attack. Slower, she flies
    /// level nose-high; faster, nose-low. With `min_airspeed` it sets how
    /// much lift a steeper angle buys: `(cruise / stall)^2` times as much
    /// at the stall.
    #[serde(default = "default_cruise_speed")]
    pub cruise_speed: Fp,
    /// How fast A and D roll her toward her bank, and back level when let
    /// go (degrees per second).
    #[serde(default = "default_roll_rate")]
    pub roll_rate: Fp,
    /// How fast W and S pitch her nose (degrees per second).
    #[serde(default = "default_pitch_rate")]
    pub pitch_rate: Fp,
    /// How fast Q and E yaw her nose (degrees per second); on the ground
    /// the steering is a multiple of it.
    #[serde(default = "default_yaw_rate")]
    pub yaw_rate: Fp,
    /// How far her wheels reach under the fuselage box, unloaded (m): at
    /// least 5 cm, or her wheels would never meet the ground and she would
    /// have no brakes or steering on it.
    #[serde(default = "default_gear_height")]
    pub gear_height: Fp,
    /// How hard full brakes slow her on the ground (m/s^2): Shift held with
    /// the throttle closed.
    #[serde(default = "default_brake")]
    pub brake: Fp,
    /// How far out her main wheels stand either side, as a share of the
    /// box's half-width - past 1 on legs that splay out of it (#1617).
    #[serde(default = "default_gear_track")]
    pub gear_track: Fp,
    /// How far ahead of the box's middle her main wheels stand, as a share
    /// of its half-length (#1617).
    #[serde(default = "default_gear_ahead")]
    pub gear_ahead: Fp,
    /// How far behind the box's middle her tail wheel stands, as a share of
    /// its half-length (#1617).
    #[serde(default = "default_tail_wheel_aft")]
    pub tail_wheel_aft: Fp,
    /// Her tail wheel's reach, as a share of [`Self::gear_height`] (#1617):
    /// 1 stands her level on all three wheels; shorter, she sits tail-down
    /// as a taildragger does, and flies level.
    #[serde(default = "default_tail_gear_share")]
    pub tail_gear_share: Fp,
    /// How far ahead of the box's middle her centre of mass is, as a share
    /// of its half-length (#1617): forward over her wing, her tail wheel
    /// carries a smaller share of her weight.
    #[serde(default)]
    pub center_of_mass_ahead: Fp,
}

/// Serde fallback for records published before #876 - the constant the
/// throttle table hard-coded. Shared with `Default`. Retired since #1615.
fn default_cruise_throttle() -> Fp {
    Fp(0.5)
}

/// Serde fallbacks for the fields #1615 added - and the defaults a fresh
/// airplane takes: a 3 m light airplane that stalls at 11 m/s, flies level
/// with her nose on her path at 20 and tops out about 25.
fn default_cruise_speed() -> Fp {
    Fp(20.0)
}
fn default_roll_rate() -> Fp {
    Fp(90.0)
}
fn default_pitch_rate() -> Fp {
    Fp(45.0)
}
fn default_yaw_rate() -> Fp {
    Fp(25.0)
}
fn default_gear_height() -> Fp {
    Fp(0.25)
}
fn default_brake() -> Fp {
    Fp(4.0)
}

/// Serde fallbacks for the wheel layout #1617 put on the record - and the
/// layout every airplane had before it: mains three quarters out and three
/// tenths of the half-length ahead, the tail wheel nine tenths behind and
/// as tall as the mains, her centre of mass in the box's middle.
fn default_gear_track() -> Fp {
    Fp(0.75)
}
fn default_gear_ahead() -> Fp {
    Fp(0.30)
}
fn default_tail_wheel_aft() -> Fp {
    Fp(0.90)
}
fn default_tail_gear_share() -> Fp {
    Fp(1.0)
}

impl Default for AirplaneParams {
    fn default() -> Self {
        Self {
            chassis_half_extents: Fp3([0.6, 0.3, 1.5]),
            mass: Fp(40.0),
            linear_damping: Fp(0.05),
            angular_damping: Fp(2.0),
            // 0.75 g: a short take-off run and a steep climb, and with the
            // drag below a top speed of about 25 m/s.
            thrust: Fp(300.0),
            pitch_torque: Fp(900.0),
            roll_torque: Fp(900.0),
            yaw_torque: Fp(450.0),
            lift_per_speed: Fp(45.0),
            drag_coefficient: Fp(0.38),
            min_airspeed: Fp(11.0),
            cruise_throttle: default_cruise_throttle(),
            cruise_speed: default_cruise_speed(),
            roll_rate: default_roll_rate(),
            pitch_rate: default_pitch_rate(),
            yaw_rate: default_yaw_rate(),
            gear_height: default_gear_height(),
            brake: default_brake(),
            gear_track: default_gear_track(),
            gear_ahead: default_gear_ahead(),
            tail_wheel_aft: default_tail_wheel_aft(),
            tail_gear_share: default_tail_gear_share(),
            center_of_mass_ahead: Fp::ZERO,
        }
    }
}

impl AirplaneParams {
    /// Where her centre of mass may stand, ahead of the box's middle as a
    /// share of its half-length (#1617): never further ahead than a
    /// twentieth of the half-length behind her mains - with no wheel ahead
    /// of it, she would sit on her nose - nor more than nine tenths behind.
    /// The sanitiser holds the record to it and the editor's slider spans it.
    pub fn center_of_mass_range(&self) -> std::ops::RangeInclusive<f32> {
        -0.9..=(self.gear_ahead.0 - 0.05).max(-0.9)
    }
}

impl LocomotionPreset for AirplaneParams {
    const KIND_TAG: &'static str = "airplane";
    const DISPLAY_LABEL: &'static str = "Airplane";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.thrust = clamp_pos(self.thrust, 0.0, 100_000.0);
        self.pitch_torque = clamp_pos(self.pitch_torque, 0.0, 50_000.0);
        self.roll_torque = clamp_pos(self.roll_torque, 0.0, 50_000.0);
        self.yaw_torque = clamp_pos(self.yaw_torque, 0.0, 50_000.0);
        self.lift_per_speed = clamp_pos(self.lift_per_speed, 0.0, 1_000.0);
        self.drag_coefficient = clamp_pos(self.drag_coefficient, 0.0, 100.0);
        self.min_airspeed = clamp_pos(self.min_airspeed, 0.0, 100.0);
        self.cruise_throttle = clamp_unit(self.cruise_throttle);
        self.cruise_speed = clamp_pos(self.cruise_speed, 1.0, 200.0);
        self.roll_rate = clamp_pos(self.roll_rate, 0.0, 720.0);
        self.pitch_rate = clamp_pos(self.pitch_rate, 0.0, 360.0);
        self.yaw_rate = clamp_pos(self.yaw_rate, 0.0, 360.0);
        self.gear_height = clamp_pos(self.gear_height, 0.05, 5.0);
        self.brake = clamp_pos(self.brake, 0.0, 50.0);
        self.gear_track = clamp_pos(self.gear_track, 0.0, 4.0);
        self.gear_ahead = clamp_pos(self.gear_ahead, 0.0, 1.0);
        self.tail_wheel_aft = clamp_pos(self.tail_wheel_aft, 0.0, 1.0);
        self.tail_gear_share = clamp_pos(self.tail_gear_share, 0.0, 1.0);
        // A value that is no number puts it in the middle - or as near it
        // as her mains let it stand.
        let range = self.center_of_mass_range();
        let ahead = self.center_of_mass_ahead.0;
        let ahead = if ahead.is_finite() { ahead } else { 0.0 };
        self.center_of_mass_ahead = Fp(ahead.clamp(*range.start(), *range.end()));
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Airplane(Box::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record published before #1615 has none of its fields, nor
    /// #1617's wheel layout, and decodes to the defaults a fresh airplane
    /// takes for them.
    #[test]
    fn a_record_from_before_the_flight_model_decodes() {
        let mut old = serde_json::to_value(AirplaneParams::default()).expect("serializes");
        let map = old.as_object_mut().expect("an object");
        for field in [
            "cruise_speed",
            "roll_rate",
            "pitch_rate",
            "yaw_rate",
            "gear_height",
            "brake",
            "gear_track",
            "gear_ahead",
            "tail_wheel_aft",
            "tail_gear_share",
            "center_of_mass_ahead",
        ] {
            assert!(map.remove(field).is_some(), "{field} is written");
        }
        let back: AirplaneParams = serde_json::from_value(old).expect("decodes");
        assert_eq!(back, AirplaneParams::default());
    }

    /// And the retired fields are still written, so a client from before
    /// #1615 - which requires them - decodes a record written after it.
    #[test]
    fn the_retired_fields_are_still_written() {
        let json = serde_json::to_value(AirplaneParams::default()).expect("serializes");
        for field in [
            "pitch_torque",
            "roll_torque",
            "yaw_torque",
            "lift_per_speed",
            "cruise_throttle",
        ] {
            assert!(json.get(field).is_some(), "{field} is not written");
        }
    }

    /// The defaults survive the sanitiser unchanged.
    #[test]
    fn the_defaults_are_inside_the_sanitiser() {
        let mut p = AirplaneParams::default();
        p.sanitize();
        assert_eq!(p, AirplaneParams::default());
    }

    /// Her centre of mass is held behind her mains and inside the box, and
    /// one that is no number goes as near the middle as her mains let it -
    /// inside the span the editor's slider offers, where a second pass
    /// moves nothing (#1617).
    #[test]
    fn her_centre_of_mass_is_held_behind_her_mains() {
        for (gear_ahead, asked, held) in [
            (0.30, 0.9, 0.25),
            (0.30, -2.0, -0.9),
            (0.30, f32::NAN, 0.0),
            (0.0, f32::NAN, -0.05),
            (0.0, 0.5, -0.05),
        ] {
            let mut p = AirplaneParams {
                gear_ahead: Fp(gear_ahead),
                center_of_mass_ahead: Fp(asked),
                ..AirplaneParams::default()
            };
            p.sanitize();
            let ahead = p.center_of_mass_ahead.0;
            assert!(
                (ahead - held).abs() < 1e-6,
                "{asked} with her mains {gear_ahead} ahead was held at {ahead}"
            );
            assert!(p.center_of_mass_range().contains(&ahead), "{ahead}");
            let once = p.clone();
            p.sanitize();
            assert_eq!(p, once, "a second pass moved her");
        }
    }
}
