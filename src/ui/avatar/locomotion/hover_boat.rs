//! Hover-boat locomotion panel.
//!
//! # Six ranges that could not hold the fleet (#1390)
//!
//! Until #1390 the sliders here rewrote a seeded boat on sight, and the
//! ranges hid behind that: a value the track could not reach was quietly
//! pulled to the bound, so nobody saw that six of them could not hold
//! what `default_visuals::boat_locomotion` derives. Now that a drawn
//! panel leaves the record alone (`ui::num::slider`), an out-of-range
//! value survives being looked at - but the owner still cannot DRAG it,
//! because the track only spans the range. So the ranges have to reach.
//!
//! Measured over seeds 0..4000 by `inert_panel_tests::audit_every_slider_
//! range_against_what_the_seeds_derive`, with each new ceiling set at the
//! derivation's own ceiling rounded up, and under the sanitiser's cap so
//! a dragged value still round-trips:
//!
//! | field | seeds derive | was | now | sanitiser cap |
//! |---|---|---|---|---|
//! | Mass | 101.9 - 474.1 | 5..200 | 5..500 | 10 000 |
//! | Suspension stiffness | 8 557 - 39 824 | 500..15 000 | 500..50 000 | 50 000 |
//! | Suspension damping | 357 - 1 659 | 10..500 | 10..2 000 | 5 000 |
//! | Lateral grip | 12 224 - 48 000 | 500..15 000 | 500..50 000 | 50 000 |
//! | Buoyancy strength | 5 093 - 23 705 | 0..10 000 | 0..25 000 | 100 000 |
//! | Buoyancy damping | 815 - 3 793 | 0..2 000 | 0..4 000 | 10 000 |
//!
//! The mass bound is `boat_locomotion`'s own clamp (80..480) rounded out;
//! the four scaled support fields are `stock x (480 / BOAT_REF_MASS)`,
//! which is what `scaled()` can reach before its cap. The three the car
//! panel also has now carry the car's bounds and steps, so the same knob
//! feels the same on both families.

use bevy_egui::egui;

use super::{LocomotionPanel, fp_slider, fp3_extents};
use crate::pds::HoverBoatParams;
use crate::player::LocalMovement;

impl LocomotionPanel for HoverBoatParams {
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
                fp_slider(ui, &mut self.linear_damping, 0.0..=10.0, 0.1, dirty);
                ui.label("Angular damping");
                fp_slider(ui, &mut self.angular_damping, 0.0..=20.0, 0.1, dirty);
            });

        egui::CollapsingHeader::new("Suspension & drive")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Suspension rest length");
                fp_slider(ui, &mut self.suspension_rest_length, 0.2..=2.0, 0.05, dirty);
                ui.label("Suspension stiffness");
                fp_slider(
                    ui,
                    &mut self.suspension_stiffness,
                    500.0..=50_000.0,
                    100.0,
                    dirty,
                );
                ui.label("Suspension damping");
                fp_slider(
                    ui,
                    &mut self.suspension_damping,
                    10.0..=2_000.0,
                    10.0,
                    dirty,
                );
                ui.label("Drive force");
                fp_slider(ui, &mut self.drive_force, 500.0..=10_000.0, 50.0, dirty);
                ui.label("Turn torque");
                fp_slider(ui, &mut self.turn_torque, 200.0..=6_000.0, 50.0, dirty);
                ui.label("Lateral grip");
                fp_slider(ui, &mut self.lateral_grip, 500.0..=50_000.0, 100.0, dirty);
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
                fp_slider(ui, &mut self.buoyancy_strength, 0.0..=25_000.0, 50.0, dirty);
                ui.label("Damping (N·s/m)");
                fp_slider(ui, &mut self.buoyancy_damping, 0.0..=4_000.0, 10.0, dirty);
                ui.label("Max depth (m)");
                fp_slider(ui, &mut self.buoyancy_max_depth, 0.1..=5.0, 0.05, dirty);
            });
    }
}
