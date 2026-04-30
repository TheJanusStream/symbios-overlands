//! Locomotion-tab UI: per-preset slider panel plus the central preset
//! picker. Each preset's [`LocomotionPanel`] impl lives in its own
//! submodule so adding a new preset is one new file + one match arm in
//! [`draw_locomotion_tab`].

mod airplane;
mod car;
mod helicopter;
mod hover_boat;
mod humanoid;

use bevy_egui::egui;

use crate::pds::{Fp, LocomotionConfig};

/// Egui detail panel for one locomotion preset. Implemented on each
/// `*Params` struct in this module's siblings — `draw_locomotion_tab`
/// dispatches to whichever variant the live `LocomotionConfig` carries.
pub trait LocomotionPanel {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool);
}

/// Render the picker row (one selectable label per preset, switching
/// preset replaces `*locomotion` with the new variant's default-tuned
/// instance) followed by the per-preset detail panel.
pub fn draw_locomotion_tab(ui: &mut egui::Ui, locomotion: &mut LocomotionConfig, dirty: &mut bool) {
    let current_kind = locomotion.kind_tag();

    ui.horizontal_wrapped(|ui| {
        ui.label("Preset:");
        for (kind, label, ctor) in LocomotionConfig::pickers() {
            if ui.selectable_label(current_kind == *kind, *label).clicked() && current_kind != *kind
            {
                *locomotion = ctor();
                *dirty = true;
            }
        }
    });
    ui.separator();

    match locomotion {
        LocomotionConfig::HoverBoat(p) => p.draw(ui, dirty),
        LocomotionConfig::Humanoid(p) => p.draw(ui, dirty),
        LocomotionConfig::Airplane(p) => p.draw(ui, dirty),
        LocomotionConfig::Helicopter(p) => p.draw(ui, dirty),
        LocomotionConfig::Car(p) => p.draw(ui, dirty),
        LocomotionConfig::Unknown => {
            ui.colored_label(
                egui::Color32::ORANGE,
                "This avatar's locomotion preset was authored against a newer schema — \
                 pick a preset above to replace it.",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shared widgets — narrower than `ui::room::widgets::fp_slider` (this one
// takes a step size and emits no inline label, leaving the caller to draw
// labels next to a stack of related sliders).
// ---------------------------------------------------------------------------

pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    dirty: &mut bool,
) {
    if ui
        .add(egui::Slider::new(&mut value.0, range).step_by(step))
        .changed()
    {
        *dirty = true;
    }
}

/// Three-component drag editor for `Fp3` half-extents (or any other
/// vec3-shaped numeric triple). Edits land in the underlying `[f32; 3]`
/// directly so the caller's `Fp3` wrapper picks up the change without an
/// intermediate copy.
pub(super) fn fp3_extents(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        for axis in value.iter_mut() {
            if ui
                .add(egui::DragValue::new(axis).speed(0.05).range(0.05..=20.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
}
