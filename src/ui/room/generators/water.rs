//! Per-volume water editor: the single `level_offset` slider plus the full
//! [`WaterSurface`] knob set grouped into colour / wave / material / flow
//! sub-panels.

use bevy_egui::egui;

use crate::pds::{Fp, WaterSurface};

use super::super::widgets::{color_picker_rgba, fp_slider};

pub(super) fn draw_water_editor(
    ui: &mut egui::Ui,
    level_offset: &mut Fp,
    surface: &mut WaterSurface,
    dirty: &mut bool,
) {
    fp_slider(ui, "Level offset", level_offset, -20.0, 20.0, dirty);
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Colour")
        .default_open(true)
        .show(ui, |ui| {
            color_picker_rgba(ui, "Shallow (head-on)", &mut surface.shallow_color, dirty);
            color_picker_rgba(ui, "Deep (grazing)", &mut surface.deep_color, dirty);
            ui.label(
                egui::RichText::new(
                    "Alpha controls the opacity at each viewing extreme — shallow is typically \
                     low (transparent looking down), deep is high (opaque at grazing).",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });

    egui::CollapsingHeader::new("Waves")
        .default_open(true)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Scale (amplitude)",
                &mut surface.wave_scale,
                0.0,
                4.0,
                dirty,
            );
            fp_slider(ui, "Speed", &mut surface.wave_speed, 0.0, 4.0, dirty);
            fp_slider(
                ui,
                "Choppiness",
                &mut surface.wave_choppiness,
                0.0,
                1.0,
                dirty,
            );
            ui.label("Wave direction (X / Z)");
            ui.horizontal(|ui| {
                let mut v = surface.wave_direction.0;
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[0])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                if changed {
                    surface.wave_direction = crate::pds::Fp2(v);
                    *dirty = true;
                }
            });
            fp_slider(ui, "Foam amount", &mut surface.foam_amount, 0.0, 1.0, dirty);
        });

    egui::CollapsingHeader::new("Material")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Roughness", &mut surface.roughness, 0.0, 1.0, dirty);
            fp_slider(ui, "Metallic", &mut surface.metallic, 0.0, 1.0, dirty);
            fp_slider(
                ui,
                "Reflectance (F0)",
                &mut surface.reflectance,
                0.0,
                1.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Flow (physics)")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Force per metre submerged applied along the surface's downhill \
                 tangent. Zero on flat water; ~9.81 ≈ free-fall along the slope.",
            );
            fp_slider(
                ui,
                "Flow strength",
                &mut surface.flow_strength,
                0.0,
                30.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Flow (visual)")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Blends the surface look from classic Gerstner standing waves \
                 (0) toward a river-style flow map (1). Independent of the \
                 physics push — set this without flow strength for a glassy \
                 \"infinity-pool\" effect, or both for a flowing river.",
            );
            fp_slider(ui, "Flow amount", &mut surface.flow_amount, 0.0, 1.0, dirty);
        });
}
