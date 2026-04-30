//! Airplane locomotion panel.

use bevy_egui::egui;

use super::{LocomotionPanel, fp_slider, fp3_extents};
use crate::pds::AirplaneParams;

impl LocomotionPanel for AirplaneParams {
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
                fp_slider(ui, &mut self.linear_damping, 0.0..=5.0, 0.05, dirty);
                ui.label("Angular damping");
                fp_slider(ui, &mut self.angular_damping, 0.0..=20.0, 0.1, dirty);
            });

        egui::CollapsingHeader::new("Thrust & control surfaces")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Thrust (N)");
                fp_slider(ui, &mut self.thrust, 0.0..=10_000.0, 50.0, dirty);
                ui.label("Pitch torque (N·m)");
                fp_slider(ui, &mut self.pitch_torque, 0.0..=5_000.0, 25.0, dirty);
                ui.label("Roll torque (N·m)");
                fp_slider(ui, &mut self.roll_torque, 0.0..=5_000.0, 25.0, dirty);
                ui.label("Yaw / rudder torque (N·m)");
                fp_slider(ui, &mut self.yaw_torque, 0.0..=5_000.0, 25.0, dirty);
            });

        egui::CollapsingHeader::new("Aerodynamics")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Lift per (m/s) airspeed");
                fp_slider(ui, &mut self.lift_per_speed, 0.0..=200.0, 1.0, dirty);
                ui.label("Drag coefficient");
                fp_slider(ui, &mut self.drag_coefficient, 0.0..=5.0, 0.05, dirty);
                ui.label("Min airspeed (m/s)");
                fp_slider(ui, &mut self.min_airspeed, 0.0..=30.0, 0.5, dirty);
            });
    }
}
