//! HoverBoat preset — 4-corner raycast suspension + buoyancy + WASD drive.

use super::{LocomotionConfig, LocomotionPreset, clamp_half_extents, clamp_pos};
use crate::pds::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// Hover-boat preset: 4-corner raycast suspension + buoyancy + WASD drive,
/// matching the legacy hover-rover physics. Collider is a chassis cuboid
/// with author-tunable half-extents.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HoverBoatParams {
    /// Chassis collider half-extents (m). Avian's `Collider::cuboid` takes
    /// full extents, so the spawner doubles each component before insert.
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
    pub jump_force: Fp,
    pub uprighting_torque: Fp,
    pub water_rest_length: Fp,
    pub buoyancy_strength: Fp,
    pub buoyancy_damping: Fp,
    pub buoyancy_max_depth: Fp,
}

impl Default for HoverBoatParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            chassis_half_extents: Fp3([cfg::CHASSIS_X, cfg::CHASSIS_Y, cfg::CHASSIS_Z]),
            mass: Fp(cfg::MASS),
            linear_damping: Fp(cfg::LINEAR_DAMPING),
            angular_damping: Fp(cfg::ANGULAR_DAMPING),
            suspension_rest_length: Fp(cfg::SUSPENSION_REST_LENGTH),
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING),
            drive_force: Fp(cfg::DRIVE_FORCE),
            turn_torque: Fp(cfg::TURN_TORQUE),
            lateral_grip: Fp(cfg::LATERAL_GRIP),
            jump_force: Fp(cfg::JUMP_FORCE),
            uprighting_torque: Fp(cfg::UPRIGHTING_TORQUE),
            water_rest_length: Fp(cfg::WATER_REST_LENGTH),
            buoyancy_strength: Fp(cfg::BUOYANCY_STRENGTH),
            buoyancy_damping: Fp(cfg::BUOYANCY_DAMPING),
            buoyancy_max_depth: Fp(cfg::BUOYANCY_MAX_DEPTH),
        }
    }
}

impl LocomotionPreset for HoverBoatParams {
    const KIND_TAG: &'static str = "hover_boat";
    const DISPLAY_LABEL: &'static str = "Hover-Boat";

    fn sanitize(&mut self) {
        clamp_half_extents(&mut self.chassis_half_extents);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.angular_damping = clamp_pos(self.angular_damping, 0.0, 100.0);
        self.suspension_rest_length = clamp_pos(self.suspension_rest_length, 0.0, 5.0);
        self.suspension_stiffness = clamp_pos(self.suspension_stiffness, 0.0, 50_000.0);
        self.suspension_damping = clamp_pos(self.suspension_damping, 0.0, 5_000.0);
        self.drive_force = clamp_pos(self.drive_force, 0.0, 50_000.0);
        self.turn_torque = clamp_pos(self.turn_torque, 0.0, 50_000.0);
        self.lateral_grip = clamp_pos(self.lateral_grip, 0.0, 50_000.0);
        self.jump_force = clamp_pos(self.jump_force, 0.0, 50_000.0);
        self.uprighting_torque = clamp_pos(self.uprighting_torque, 0.0, 50_000.0);
        self.water_rest_length = clamp_pos(self.water_rest_length, 0.0, 10.0);
        self.buoyancy_strength = clamp_pos(self.buoyancy_strength, 0.0, 100_000.0);
        self.buoyancy_damping = clamp_pos(self.buoyancy_damping, 0.0, 10_000.0);
        self.buoyancy_max_depth = clamp_pos(self.buoyancy_max_depth, 0.001, 50.0);
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::HoverBoat(Box::new(self))
    }
}
