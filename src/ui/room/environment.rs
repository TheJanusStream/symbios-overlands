//! Environment tab — directional sun, ambient, sky, and fog widgets.

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
}
