//! Humanoid locomotion panel.

use bevy_egui::egui;

use super::{LocomotionPanel, fp_slider};
use crate::pds::HumanoidParams;

impl LocomotionPanel for HumanoidParams {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool) {
        egui::CollapsingHeader::new("Capsule")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Radius (m)");
                fp_slider(ui, &mut self.capsule_radius, 0.05..=1.0, 0.01, dirty);
                ui.label("Cylinder length (m)");
                fp_slider(ui, &mut self.capsule_length, 0.1..=4.0, 0.05, dirty);
                ui.label(
                    egui::RichText::new(format!(
                        "Total height ≈ {:.2} m (length + 2·radius)",
                        self.total_height()
                    ))
                    .small()
                    .weak(),
                );
                ui.label("Mass (kg)");
                fp_slider(ui, &mut self.mass, 30.0..=150.0, 1.0, dirty);
                ui.label("Linear damping");
                fp_slider(ui, &mut self.linear_damping, 0.0..=3.0, 0.05, dirty);
            });

        egui::CollapsingHeader::new("Locomotion")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Walk speed (m/s)");
                fp_slider(ui, &mut self.walk_speed, 1.0..=10.0, 0.1, dirty);
                ui.label("Acceleration (1/s)");
                fp_slider(ui, &mut self.acceleration, 2.0..=30.0, 0.5, dirty);
                ui.label("Jump impulse (N·s)");
                fp_slider(ui, &mut self.jump_impulse, 100.0..=1500.0, 10.0, dirty);
            });

        egui::CollapsingHeader::new("Water")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Wading speed factor");
                fp_slider(ui, &mut self.wading_speed_factor, 0.0..=1.0, 0.05, dirty);
                ui.label("Swim speed (m/s)");
                fp_slider(ui, &mut self.swim_speed, 0.5..=8.0, 0.1, dirty);
                ui.label("Swim vertical speed (m/s)");
                fp_slider(ui, &mut self.swim_vertical_speed, 0.2..=5.0, 0.1, dirty);
            });
    }
}
