//! Environment tab — directional sun, ambient, sky, fog, and room-wide water
//! widgets. Per-volume water appearance (colour, choppiness) lives on the
//! Water generator detail editor instead; the fields here are the ones that
//! should match the room's mood rather than varying between water bodies.

use bevy_egui::egui;

use crate::pds::Environment;

use super::widgets::{color_picker, color_picker_rgba, fp_slider};

pub(super) fn draw_environment_tab(ui: &mut egui::Ui, env: &mut Environment, dirty: &mut bool) {
    ui.heading("Environment");
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Lighting & Sky")
        .default_open(true)
        .show(ui, |ui| {
            color_picker(ui, "Sun colour", &mut env.sun_color, dirty);
            color_picker(ui, "Sky colour", &mut env.sky_color, dirty);
            fp_slider(
                ui,
                "Sun illuminance",
                &mut env.sun_illuminance,
                0.0,
                50_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Ambient brightness",
                &mut env.ambient_brightness,
                0.0,
                2_000.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Distance Fog")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Visibility (m)",
                &mut env.fog_visibility,
                50.0,
                2_000.0,
                dirty,
            );
            color_picker_rgba(ui, "Fog colour", &mut env.fog_color, dirty);
            color_picker(ui, "Extinction", &mut env.fog_extinction, dirty);
            color_picker(ui, "Inscattering", &mut env.fog_inscattering, dirty);
            color_picker_rgba(ui, "Sun glow", &mut env.fog_sun_color, dirty);
            fp_slider(
                ui,
                "Sun glow exponent",
                &mut env.fog_sun_exponent,
                1.0,
                100.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Water (room-wide)")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Detail normal — near tile",
                &mut env.water_normal_scale_near,
                0.0,
                4.0,
                dirty,
            );
            fp_slider(
                ui,
                "Detail normal — far tile",
                &mut env.water_normal_scale_far,
                0.0,
                1.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Near + far tiles blend by distance so the repeating-grid look \
                     disappears on long sightlines.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Sun glitter",
                &mut env.water_sun_glitter,
                0.0,
                8.0,
                dirty,
            );
            color_picker(
                ui,
                "Crest scatter tint",
                &mut env.water_scatter_color,
                dirty,
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Refraction strength",
                &mut env.water_refraction_strength,
                0.0,
                2.0,
                dirty,
            );
            fp_slider(
                ui,
                "Shoreline foam width (m)",
                &mut env.water_shore_foam_width,
                0.0,
                8.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Refraction + shoreline foam are reserved for a future depth-prepass pass; \
                     the fields round-trip through the record so rooms authored now will \
                     light up once the wiring lands.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });
}
