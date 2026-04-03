use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::state::LocalAirshipParams;

pub fn airship_ui(mut contexts: EguiContexts, mut ap: ResMut<LocalAirshipParams>) {
    use crate::config::ui::airship as cfg;

    egui::Window::new("Airship Design")
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_width(cfg::WINDOW_DEFAULT_WIDTH)
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let p = &mut ap.params;
            let mut changed = false;

            // --- Construction parameters ------------------------------------
            ui.heading("Construction");

            ui.label("Hull length (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.hull_length, 1.0..=5.0).step_by(0.05))
                .changed();

            ui.label("Hull width (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.hull_width, 0.6..=3.0).step_by(0.05))
                .changed();

            ui.label("Pontoon spread (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.pontoon_spread, 0.5..=2.5).step_by(0.05))
                .changed();

            ui.label("Pontoon length (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.pontoon_length, 0.5..=3.5).step_by(0.05))
                .changed();

            ui.label("Mast height (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.mast_height, 0.3..=2.5).step_by(0.05))
                .changed();

            ui.label("Sail size (m)");
            changed |= ui
                .add(egui::Slider::new(&mut p.sail_size, 0.3..=2.5).step_by(0.05))
                .changed();

            ui.separator();

            // --- Material properties ----------------------------------------
            ui.heading("Materials");

            ui.label("Metallic");
            changed |= ui
                .add(egui::Slider::new(&mut p.metallic, 0.0..=1.0).step_by(0.01))
                .changed();

            ui.label("Roughness");
            changed |= ui
                .add(egui::Slider::new(&mut p.roughness, 0.0..=1.0).step_by(0.01))
                .changed();

            ui.separator();

            // --- Colour pickers (RGB sliders) --------------------------------
            ui.collapsing("Hull colour", |ui| {
                changed |= color_sliders(ui, &mut p.hull_color);
            });

            ui.collapsing("Pontoon colour", |ui| {
                changed |= color_sliders(ui, &mut p.pontoon_color);
            });

            ui.separator();

            if ui.button("Reset to defaults").clicked() {
                *p = crate::protocol::AirshipParams::default();
                changed = true;
            }

            if changed {
                ap.needs_rebuild = true;
            }
        });
}

/// Three labelled 0–1 sliders for an sRGB colour.
fn color_sliders(ui: &mut egui::Ui, rgb: &mut [f32; 3]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("R").color(egui::Color32::from_rgb(220, 80, 80)));
        changed |= ui.add(egui::Slider::new(&mut rgb[0], 0.0..=1.0).step_by(0.01)).changed();
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("G").color(egui::Color32::from_rgb(80, 200, 80)));
        changed |= ui.add(egui::Slider::new(&mut rgb[1], 0.0..=1.0).step_by(0.01)).changed();
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(80, 130, 220)));
        changed |= ui.add(egui::Slider::new(&mut rgb[2], 0.0..=1.0).step_by(0.01)).changed();
    });

    // Live colour preview swatch
    let swatch = egui::Color32::from_rgb(
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
    );
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, swatch);

    changed
}
