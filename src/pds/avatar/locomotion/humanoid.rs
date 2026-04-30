//! Humanoid preset — vertical capsule with locked-axes uprighting,
//! velocity-driven walk controller, jump impulse, swim/wading modes.

use super::{LocomotionConfig, LocomotionPreset, clamp_pos, clamp_unit};
use crate::pds::types::Fp;
use serde::{Deserialize, Serialize};

/// Humanoid preset: vertical capsule rigid body with `LockedAxes` keeping
/// it upright, velocity-driven walk controller, jump impulse, swim/wading
/// modes triggered by water depth.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidParams {
    /// Capsule collider radius (m).
    pub capsule_radius: Fp,
    /// Capsule cylindrical length (m). Total height ≈ length + 2·radius.
    pub capsule_length: Fp,
    pub mass: Fp,
    pub linear_damping: Fp,
    pub walk_speed: Fp,
    pub acceleration: Fp,
    pub jump_impulse: Fp,
    pub swim_speed: Fp,
    pub swim_vertical_speed: Fp,
    pub wading_speed_factor: Fp,
}

impl Default for HumanoidParams {
    fn default() -> Self {
        Self {
            capsule_radius: Fp(0.28),
            capsule_length: Fp(1.24),
            mass: Fp(80.0),
            linear_damping: Fp(0.3),
            walk_speed: Fp(4.0),
            acceleration: Fp(12.0),
            jump_impulse: Fp(450.0),
            swim_speed: Fp(2.5),
            swim_vertical_speed: Fp(1.8),
            wading_speed_factor: Fp(0.5),
        }
    }
}

impl HumanoidParams {
    /// Total standing height (m). Convenience for systems that need to
    /// know whether the player's head/feet cross a water surface — the
    /// avatar's vertical extent comes purely from the capsule collider.
    pub fn total_height(&self) -> f32 {
        self.capsule_length.0 + 2.0 * self.capsule_radius.0
    }
}

impl LocomotionPreset for HumanoidParams {
    const KIND_TAG: &'static str = "humanoid";
    const DISPLAY_LABEL: &'static str = "Humanoid";

    fn sanitize(&mut self) {
        self.capsule_radius = clamp_pos(self.capsule_radius, 0.05, 2.0);
        self.capsule_length = clamp_pos(self.capsule_length, 0.1, 8.0);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.walk_speed = clamp_pos(self.walk_speed, 0.0, 50.0);
        self.acceleration = clamp_pos(self.acceleration, 0.0, 200.0);
        self.jump_impulse = clamp_pos(self.jump_impulse, 0.0, 50_000.0);
        self.swim_speed = clamp_pos(self.swim_speed, 0.0, 50.0);
        self.swim_vertical_speed = clamp_pos(self.swim_vertical_speed, 0.0, 50.0);
        self.wading_speed_factor = clamp_unit(self.wading_speed_factor);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Humanoid(Box::new(self))
    }
}
