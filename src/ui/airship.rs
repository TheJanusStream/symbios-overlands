use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::protocol::PontoonShape;
use crate::state::LocalAirshipParams;

pub fn airship_ui(mut contexts: EguiContexts, mut ap: ResMut<LocalAirshipParams>) {
    use crate::config::ui::airship as cfg;

    egui::Window::new("Airship Design")
        .default_open(false)
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_width(cfg::WINDOW_DEFAULT_WIDTH)
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            // Reborrow once so we can split-borrow individual fields of
            // `LocalAirshipParams` independently (egui needs `&mut params` for
            // the construction sliders *and* `&mut smooth_kinematics` for the
            // networking checkbox below).
            let ap = ap.as_mut();
            let p = &mut ap.params;
            let mut changed = false;

            // --- Hull ------------------------------------------------------
            egui::CollapsingHeader::new("Hull")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label("Length (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.hull_length, 1.0..=5.0).step_by(0.05))
                        .changed();

                    ui.label("Width (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.hull_width, 0.6..=3.0).step_by(0.05))
                        .changed();

                    ui.label("Keel depth (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.hull_depth, 0.1..=1.5).step_by(0.05))
                        .changed();
                });

            // --- Pontoons --------------------------------------------------
            egui::CollapsingHeader::new("Pontoons")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label("Shape");
                    let shape_label = match p.pontoon_shape {
                        PontoonShape::Capsule => "Capsule",
                        PontoonShape::VHull => "V-Hull",
                    };
                    egui::ComboBox::from_id_salt("pontoon_shape")
                        .selected_text(shape_label)
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut p.pontoon_shape,
                                    PontoonShape::Capsule,
                                    "Capsule",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut p.pontoon_shape,
                                    PontoonShape::VHull,
                                    "V-Hull",
                                )
                                .changed();
                        });

                    ui.label("Spread (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.pontoon_spread, 0.5..=2.5).step_by(0.05))
                        .changed();

                    ui.label("Length (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.pontoon_length, 0.5..=3.5).step_by(0.05))
                        .changed();

                    ui.label("Width (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.pontoon_width, 0.05..=1.0).step_by(0.01))
                        .changed();

                    ui.label("Height (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.pontoon_height, 0.05..=1.0).step_by(0.01))
                        .changed();

                    ui.label("Strut drop (% keel)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.strut_drop, 0.0..=1.0).step_by(0.01))
                        .changed();
                });

            // --- Mast & Sail -----------------------------------------------
            egui::CollapsingHeader::new("Mast & Sail")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label("Mast height (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.mast_height, 0.3..=2.5).step_by(0.05))
                        .changed();

                    ui.label("Mast radius (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.mast_radius, 0.01..=0.2).step_by(0.005))
                        .changed();

                    ui.label("Mast offset X (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.mast_offset[0], -1.5..=1.5).step_by(0.05))
                        .changed();

                    ui.label("Mast offset Z (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.mast_offset[1], -2.0..=2.0).step_by(0.05))
                        .changed();

                    ui.label("Sail size (m)");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.sail_size, 0.3..=2.5).step_by(0.05))
                        .changed();
                });

            // --- Materials -------------------------------------------------
            egui::CollapsingHeader::new("Materials")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("Metallic");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.metallic, 0.0..=1.0).step_by(0.01))
                        .changed();

                    ui.label("Roughness");
                    changed |= ui
                        .add(egui::Slider::new(&mut p.roughness, 0.0..=1.0).step_by(0.01))
                        .changed();

                    ui.collapsing("Hull colour", |ui| {
                        changed |= color_sliders(ui, &mut p.hull_color);
                    });

                    ui.collapsing("Pontoon colour", |ui| {
                        changed |= color_sliders(ui, &mut p.pontoon_color);
                    });

                    ui.collapsing("Mast colour", |ui| {
                        changed |= color_sliders(ui, &mut p.mast_color);
                    });

                    ui.collapsing("Strut colour", |ui| {
                        changed |= color_sliders(ui, &mut p.strut_color);
                    });
                });

            ui.separator();

            // --- Networking (local-only, not broadcast) -------------------
            egui::CollapsingHeader::new("Networking")
                .default_open(false)
                .show(ui, |ui| {
                    ui.checkbox(
                        &mut ap.smooth_kinematics,
                        "Smooth remote peers (Hermite spline + 50 ms buffer)",
                    );
                    ui.label(
                        egui::RichText::new(
                            "Uncheck to snap to the latest packet and expose raw jitter.",
                        )
                        .small()
                        .weak(),
                    );
                });

            ui.separator();

            if ui.button("Reset to defaults").clicked() {
                ap.params = crate::protocol::AirshipParams::default();
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
        changed |= ui
            .add(egui::Slider::new(&mut rgb[0], 0.0..=1.0).step_by(0.01))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("G").color(egui::Color32::from_rgb(80, 200, 80)));
        changed |= ui
            .add(egui::Slider::new(&mut rgb[1], 0.0..=1.0).step_by(0.01))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(80, 130, 220)));
        changed |= ui
            .add(egui::Slider::new(&mut rgb[2], 0.0..=1.0).step_by(0.01))
            .changed();
    });

    // Live colour preview swatch
    let swatch = egui::Color32::from_rgb(
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
    );
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, swatch);

    changed
}
