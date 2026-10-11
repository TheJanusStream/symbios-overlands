//! Airplane locomotion panel - the flight model's live fields (#1615). The
//! five the arcade model read and nothing reads now (`pitch_torque`,
//! `roll_torque`, `yaw_torque`, `lift_per_speed`, `cruise_throttle`) are
//! still written for older clients, and are not shown: a slider that moves
//! nothing is a promise the panel cannot keep.

use bevy_egui::egui;

use super::{LocomotionPanel, fp_slider, fp3_extents};
use crate::pds::AirplaneParams;
use crate::player::LocalMovement;

impl LocomotionPanel for AirplaneParams {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool, _facts: &LocalMovement) {
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
                ui.label(
                    "Centre of mass ahead of the middle (share of half-length; \
                     held behind the main wheels)",
                );
                let range = self.center_of_mass_range();
                fp_slider(ui, &mut self.center_of_mass_ahead, range, 0.01, dirty);
            });

        egui::CollapsingHeader::new("Engine & wings")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Thrust at full throttle (N)");
                fp_slider(ui, &mut self.thrust, 0.0..=10_000.0, 10.0, dirty);
                ui.label("Cruise speed - level with the nose on her path (m/s)");
                fp_slider(ui, &mut self.cruise_speed, 1.0..=80.0, 0.5, dirty);
                ui.label("Stall speed (m/s)");
                fp_slider(ui, &mut self.min_airspeed, 0.0..=40.0, 0.5, dirty);
                ui.label("Drag coefficient");
                fp_slider(ui, &mut self.drag_coefficient, 0.0..=5.0, 0.01, dirty);
            });

        egui::CollapsingHeader::new("Control rates")
            .default_open(true)
            .show(ui, |ui| {
                ui.label("Roll rate - A / D (deg/s)");
                fp_slider(ui, &mut self.roll_rate, 0.0..=360.0, 5.0, dirty);
                ui.label("Pitch rate - W / S (deg/s)");
                fp_slider(ui, &mut self.pitch_rate, 0.0..=180.0, 5.0, dirty);
                ui.label("Yaw rate - Q / E (deg/s)");
                fp_slider(ui, &mut self.yaw_rate, 0.0..=180.0, 5.0, dirty);
            });

        egui::CollapsingHeader::new("Landing gear")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Gear height under the box (m)");
                fp_slider(ui, &mut self.gear_height, 0.05..=2.0, 0.01, dirty);
                ui.label("Brakes - Shift at closed throttle (m/s²)");
                fp_slider(ui, &mut self.brake, 0.0..=20.0, 0.5, dirty);
                ui.label("Main wheels' track (share of half-width)");
                fp_slider(ui, &mut self.gear_track, 0.0..=4.0, 0.01, dirty);
                ui.label("Main wheels ahead of the middle (share of half-length)");
                fp_slider(ui, &mut self.gear_ahead, 0.0..=1.0, 0.01, dirty);
                ui.label("Tail wheel behind the middle (share of half-length)");
                fp_slider(ui, &mut self.tail_wheel_aft, 0.0..=1.0, 0.01, dirty);
                ui.label("Tail wheel's reach (share of gear height; under 1 sits her tail-down)");
                fp_slider(ui, &mut self.tail_gear_share, 0.0..=1.0, 0.01, dirty);
            });
    }
}
