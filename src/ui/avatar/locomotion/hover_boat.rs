//! Hover-boat locomotion panel.

use bevy_egui::egui;

use super::{LocomotionPanel, fp_slider, fp3_extents};
use crate::pds::HoverBoatParams;

impl LocomotionPanel for HoverBoatParams {
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
                fp_slider(ui, &mut self.mass, 5.0..=200.0, 1.0, dirty);
                ui.label("Linear damping");
                fp_slider(ui, &mut self.linear_damping, 0.0..=10.0, 0.1, dirty);
                ui.label("Angular damping");
                fp_slider(ui, &mut self.angular_damping, 0.0..=20.0, 0.1, dirty);
            });

        egui::CollapsingHeader::new("Suspension & Drive")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Suspension rest length");
                fp_slider(ui, &mut self.suspension_rest_length, 0.2..=2.0, 0.05, dirty);
                ui.label("Suspension stiffness");
                fp_slider(
                    ui,
                    &mut self.suspension_stiffness,
                    500.0..=15_000.0,
                    50.0,
                    dirty,
                );
                ui.label("Suspension damping");
                fp_slider(ui, &mut self.suspension_damping, 10.0..=500.0, 5.0, dirty);
                ui.label("Drive force");
                fp_slider(ui, &mut self.drive_force, 500.0..=10_000.0, 50.0, dirty);
                ui.label("Turn torque");
                fp_slider(ui, &mut self.turn_torque, 200.0..=6_000.0, 50.0, dirty);
                ui.label("Lateral grip");
                fp_slider(ui, &mut self.lateral_grip, 500.0..=15_000.0, 100.0, dirty);
                ui.label("Jump force");
                fp_slider(ui, &mut self.jump_force, 500.0..=8_000.0, 50.0, dirty);
                ui.label("Uprighting torque");
                fp_slider(
                    ui,
                    &mut self.uprighting_torque,
                    100.0..=3_000.0,
                    50.0,
                    dirty,
                );
            });

        egui::CollapsingHeader::new("Buoyancy")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Water rest length (m)");
                fp_slider(ui, &mut self.water_rest_length, 0.0..=3.0, 0.05, dirty);
                ui.label("Strength (N/m)");
                fp_slider(ui, &mut self.buoyancy_strength, 0.0..=10_000.0, 50.0, dirty);
                ui.label("Damping (N·s/m)");
                fp_slider(ui, &mut self.buoyancy_damping, 0.0..=2_000.0, 10.0, dirty);
                ui.label("Max depth (m)");
                fp_slider(ui, &mut self.buoyancy_max_depth, 0.1..=5.0, 0.05, dirty);
            });
    }
}
