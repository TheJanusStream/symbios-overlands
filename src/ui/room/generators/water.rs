//! Per-volume water editor: the full [`WaterSurface`] knob set
//! grouped into colour / wave / material / flow / depth sub-panels. Vertical
//! position is set via the generator's placement transform - there's
//! no separate level-offset slider any more.

use bevy_egui::egui;

use crate::pds::WaterSurface;
use crate::pds::sanitize::limits::{MAX_WATER_DEPTH, MIN_WATER_DEPTH};

use super::super::widgets::{color_picker_rgba, fp_slider, fp_slider_log};

/// What the Depth row says on hover (#1949).
const DEPTH_HOVER: &str = "How deep the water is, in metres under its surface. Below its \
     bottom nothing is in the water: a pool on a roof is no water to the hall under it. \
     Unset, it reaches down to the floor of the piece that holds it - the bottom of its \
     parent's box - but never more than 2.5 m, and 2.5 m where the parent has no box \
     or its box does not reach under the water. So parent the water to the slab under \
     it, or set its depth here: a pool deeper than 2.5 m needs it. A depth set here is \
     world metres and does not grow with a scaled copy. Only water inside a built piece \
     has a bottom: the world's own water layers, its sea and lakes, reach down to the \
     ground and ignore this.";

pub(super) fn draw_water_editor(ui: &mut egui::Ui, surface: &mut WaterSurface, dirty: &mut bool) {
    egui::CollapsingHeader::new("Colour")
        .default_open(false)
        .show(ui, |ui| {
            color_picker_rgba(ui, "Shallow (head-on)", &mut surface.shallow_color, dirty);
            color_picker_rgba(ui, "Deep (grazing)", &mut surface.deep_color, dirty);
            ui.label(
                egui::RichText::new(
                    "Alpha controls the opacity at each viewing extreme - shallow is typically \
                     low (transparent looking down), deep is high (opaque at grazing).",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        });

    egui::CollapsingHeader::new("Waves")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Scale (amplitude)",
                &mut surface.wave_scale,
                0.0,
                0.4,
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
                        crate::ui::num::drag(&mut v[0])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        crate::ui::num::drag(&mut v[1])
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
                 physics push - set this without flow strength for a glassy \
                 \"infinity-pool\" effect, or both for a flowing river.",
            );
            fp_slider(ui, "Flow amount", &mut surface.flow_amount, 0.0, 1.0, dirty);
        });

    egui::CollapsingHeader::new("Depth")
        .default_open(false)
        .show(ui, |ui| draw_depth(ui, surface, dirty));

    egui::CollapsingHeader::new("Avatar wakes")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Per-avatar ripples shed by every avatar currently in contact \
                 with this water plane (local player + remote peers). Strength \
                 0 disables the effect entirely so existing rooms render \
                 unchanged.",
            );
            fp_slider(ui, "Strength", &mut surface.wake_strength, 0.0, 5.0, dirty);
            fp_slider(
                ui,
                "Ripple wavelength (m)",
                &mut surface.wake_ripple_wavelength,
                0.05,
                10.0,
                dirty,
            );
            fp_slider(
                ui,
                "Decay radius (m)",
                &mut surface.wake_decay_radius,
                0.1,
                50.0,
                dirty,
            );
        });
}

/// The Depth row (#1949): ticked, a depth in metres; unticked, unset - down
/// to the holding piece's floor. Ticking it starts from
/// [`WaterSurface::FALLBACK_DEPTH`], which the wire holds exactly.
fn draw_depth(ui: &mut egui::Ui, surface: &mut WaterSurface, dirty: &mut bool) {
    let mut set = surface.depth.is_some();
    if ui
        .checkbox(&mut set, "Set its depth")
        .on_hover_text(DEPTH_HOVER)
        .changed()
    {
        surface.depth = set.then_some(crate::pds::Fp(WaterSurface::FALLBACK_DEPTH));
        *dirty = true;
    }
    match surface.depth.as_mut() {
        Some(depth) => {
            fp_slider_log(
                ui,
                "Depth (m)",
                depth,
                MIN_WATER_DEPTH,
                MAX_WATER_DEPTH,
                dirty,
            )
            .on_hover_text(DEPTH_HOVER);
        }
        None => {
            ui.label(
                egui::RichText::new(
                    "Unset: down to the floor of the piece that holds it, 2.5 m at most.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            )
            .on_hover_text(DEPTH_HOVER);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1949: the hover names the fallback the depth rule uses.
    #[test]
    fn the_depth_hover_names_the_fallback() {
        let fallback = format!("{} m", WaterSurface::FALLBACK_DEPTH);
        assert!(DEPTH_HOVER.contains(&fallback), "{fallback}");
    }
}
