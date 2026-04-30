//! Helicopter locomotion panel.

use bevy_egui::egui;

use super::{LocomotionPanel, fp3_extents, fp_slider};
use crate::pds::HelicopterParams;

impl LocomotionPanel for HelicopterParams {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
        egui::CollapsingHeader::new("Chassis")
            .default_open(true)
            .show(ui, |ui| {
                fp3_extents(
                    ui,
                    "Half-extents (X/Y/Z, m)",
                    &mut self.chassis_half_extents.0,
                    dirty,
                );
                ui.label("Mass (kg)");
                fp_slider(ui, &mut self.mass, 5.0..=500.0, 1.0, dirty);
                ui.label("Linear damping");
                fp_slider(ui, &mut self.linear_damping, 0.0..=10.0, 0.05, dirty);
                ui.label("Angular damping");
                fp_slider(ui, &mut self.angular_damping, 0.0..=20.0, 0.1, dirty);
            });

        egui::CollapsingHeader::new("Hover & cyclic")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Hover thrust (N)");
                fp_slider(ui, &mut self.hover_thrust, 0.0..=10_000.0, 25.0, dirty);
                ui.label("Vertical speed (m/s)");
                fp_slider(ui, &mut self.vertical_speed, 0.0..=20.0, 0.25, dirty);
                ui.label("Cyclic force (N)");
                fp_slider(ui, &mut self.cyclic_force, 0.0..=5_000.0, 25.0, dirty);
                ui.label("Strafe force (N)");
                fp_slider(ui, &mut self.strafe_force, 0.0..=5_000.0, 25.0, dirty);
                ui.label("Yaw torque (N·m)");
                fp_slider(ui, &mut self.yaw_torque, 0.0..=5_000.0, 25.0, dirty);
            });
    }
}
