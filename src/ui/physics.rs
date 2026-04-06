use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::state::LocalPhysicsParams;

pub fn physics_ui(mut contexts: EguiContexts, mut pp: ResMut<LocalPhysicsParams>) {
    egui::Window::new("Physics Tuning")
        .default_open(false)
        .default_pos([650.0, 10.0])
        .default_width(320.0)
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let p = &mut *pp;

            // --- Suspension ------------------------------------------------
            egui::CollapsingHeader::new("Suspension")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Rest length (m)");
                    ui.add(
                        egui::Slider::new(&mut p.suspension_rest_length, 0.2..=2.0).step_by(0.05),
                    );

                    ui.label("Stiffness");
                    ui.add(
                        egui::Slider::new(&mut p.suspension_stiffness, 500.0..=15_000.0)
                            .step_by(50.0),
                    );

                    ui.label("Damping");
                    ui.add(egui::Slider::new(&mut p.suspension_damping, 10.0..=500.0).step_by(5.0));
                });

            // --- Drive -----------------------------------------------------
            egui::CollapsingHeader::new("Drive")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Drive force");
                    ui.add(egui::Slider::new(&mut p.drive_force, 500.0..=10_000.0).step_by(50.0));

                    ui.label("Turn torque");
                    ui.add(egui::Slider::new(&mut p.turn_torque, 200.0..=6_000.0).step_by(50.0));

                    ui.label("Lateral grip");
                    ui.add(egui::Slider::new(&mut p.lateral_grip, 500.0..=15_000.0).step_by(100.0));

                    ui.label("Jump force");
                    ui.add(egui::Slider::new(&mut p.jump_force, 500.0..=8_000.0).step_by(50.0));

                    ui.label("Uprighting torque");
                    ui.add(
                        egui::Slider::new(&mut p.uprighting_torque, 100.0..=3_000.0).step_by(50.0),
                    );
                });

            // --- Chassis ---------------------------------------------------
            egui::CollapsingHeader::new("Chassis")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Linear damping");
                    ui.add(egui::Slider::new(&mut p.linear_damping, 0.0..=10.0).step_by(0.1));

                    ui.label("Angular damping");
                    ui.add(egui::Slider::new(&mut p.angular_damping, 0.0..=20.0).step_by(0.1));

                    ui.label("Mass (kg)");
                    ui.add(egui::Slider::new(&mut p.mass, 5.0..=200.0).step_by(1.0));
                });

            // --- Buoyancy (swimming) --------------------------------------
            egui::CollapsingHeader::new("Buoyancy")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Water rest length (m)");
                    ui.add(egui::Slider::new(&mut p.water_rest_length, 0.0..=3.0).step_by(0.05));

                    ui.label("Strength (N/m)");
                    ui.add(
                        egui::Slider::new(&mut p.buoyancy_strength, 0.0..=10_000.0).step_by(50.0),
                    );

                    ui.label("Damping (N·s/m)");
                    ui.add(egui::Slider::new(&mut p.buoyancy_damping, 0.0..=2_000.0).step_by(10.0));

                    ui.label("Max depth (m)");
                    ui.add(egui::Slider::new(&mut p.buoyancy_max_depth, 0.1..=5.0).step_by(0.05));
                });

            ui.separator();

            if ui.button("Reset to defaults").clicked() {
                *p = LocalPhysicsParams::default();
            }
        });
}
