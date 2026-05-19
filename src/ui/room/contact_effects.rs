//! Room-editor tab for authored avatar-world contact effects (#246).
//!
//! Edits [`crate::pds::ContactEffects`] in place; any change flips the
//! shared `dirty` flag so the room re-publishes and the world
//! compiler's `apply_contact_recipes` rebuilds the runtime registry on
//! the next save — no recompile, no relog.

use bevy_egui::egui;

use crate::pds::contact_effects::{
    ContactEffectRecord, ContactEffects, ContactPhaseKind, ContactSurfaceKind,
};
use crate::pds::generator::EmitterShape;
use crate::pds::types::{Fp, Fp3};

use super::widgets::{color_picker_rgba, drag_u32, fp_slider};

/// A reasonable starting point for a freshly-added recipe (a copy of
/// the canonical splash, renamed) so "Add" yields something that
/// already works.
fn new_recipe(existing: usize) -> ContactEffectRecord {
    let mut r = crate::pds::default_contact_effects().recipes.swap_remove(0);
    r.name = format!("effect_{existing}");
    r
}

pub(super) fn draw_contact_effects_tab(
    ui: &mut egui::Ui,
    effects: &mut ContactEffects,
    dirty: &mut bool,
) {
    ui.heading("Contact effects");
    ui.label(
        egui::RichText::new(
            "Particle bursts triggered when an avatar contacts a surface \
             (e.g. a boat hitting water). Edits take effect on the next \
             Publish.",
        )
        .small()
        .color(egui::Color32::GRAY),
    );
    ui.add_space(4.0);

    let mut per_frame = effects.max_particles_per_frame;
    drag_u32(
        ui,
        "Max particles / frame (all recipes)",
        &mut per_frame,
        0,
        4096,
        dirty,
    );
    effects.max_particles_per_frame = per_frame;

    ui.separator();

    let mut remove: Option<usize> = None;
    for (i, r) in effects.recipes.iter_mut().enumerate() {
        let header = format!(
            "{}  ({}, {})",
            r.name,
            surface_label(r.surface),
            phase_label(r.phase),
        );
        egui::CollapsingHeader::new(header)
            .id_salt(("contact_recipe", i))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui.text_edit_singleline(&mut r.name).changed() {
                        *dirty = true;
                    }
                });
                if ui.checkbox(&mut r.enabled, "Enabled").changed() {
                    *dirty = true;
                }

                ui.collapsing("Trigger", |ui| {
                    surface_combo(ui, i, &mut r.surface, dirty);
                    phase_combo(ui, i, &mut r.phase, dirty);
                    fp_slider(ui, "Min speed (m/s)", &mut r.min_speed, 0.0, 50.0, dirty);
                    fp_slider(ui, "Min intensity", &mut r.min_intensity, 0.0, 1.0, dirty);
                });

                ui.collapsing("Count = clamp(speed·gain + base, min, max)", |ui| {
                    fp_slider(ui, "Gain", &mut r.count.gain, 0.0, 40.0, dirty);
                    fp_slider(ui, "Base", &mut r.count.base, 0.0, 40.0, dirty);
                    drag_u32(ui, "Min", &mut r.count.min, 0, 512, dirty);
                    drag_u32(ui, "Max", &mut r.count.max, 0, 512, dirty);
                });

                fp_slider(ui, "Radius scale", &mut r.radius_scale, 0.0, 8.0, dirty);
                fp_slider(
                    ui,
                    "Velocity inherit",
                    &mut r.velocity_inherit,
                    0.0,
                    2.0,
                    dirty,
                );
                fp_slider(ui, "Cooldown (s)", &mut r.cooldown, 0.0, 5.0, dirty);

                ui.collapsing("Particle", |ui| {
                    shape_combo(ui, i, &mut r.particle.shape, dirty);
                    fp_slider(
                        ui,
                        "Lifetime min (s)",
                        &mut r.particle.lifetime_min,
                        0.0,
                        5.0,
                        dirty,
                    );
                    fp_slider(
                        ui,
                        "Lifetime max (s)",
                        &mut r.particle.lifetime_max,
                        0.0,
                        5.0,
                        dirty,
                    );
                    fp_slider(ui, "Speed min", &mut r.particle.speed_min, 0.0, 20.0, dirty);
                    fp_slider(ui, "Speed max", &mut r.particle.speed_max, 0.0, 20.0, dirty);
                    fp_slider(
                        ui,
                        "Gravity ×",
                        &mut r.particle.gravity_multiplier,
                        -2.0,
                        2.0,
                        dirty,
                    );
                    fp_slider(
                        ui,
                        "Linear drag",
                        &mut r.particle.linear_drag,
                        0.0,
                        5.0,
                        dirty,
                    );
                    fp_slider(
                        ui,
                        "Start size",
                        &mut r.particle.start_size,
                        0.0,
                        1.0,
                        dirty,
                    );
                    fp_slider(ui, "End size", &mut r.particle.end_size, 0.0, 1.0, dirty);
                    color_picker_rgba(ui, "Start colour", &mut r.particle.start_color, dirty);
                    color_picker_rgba(ui, "End colour", &mut r.particle.end_color, dirty);
                    if ui
                        .checkbox(&mut r.particle.billboard, "Billboard")
                        .changed()
                    {
                        *dirty = true;
                    }
                    drag_u32(
                        ui,
                        "Max particles",
                        &mut r.particle.max_particles,
                        0,
                        512,
                        dirty,
                    );
                });

                if ui
                    .button(egui::RichText::new("Remove recipe").color(egui::Color32::LIGHT_RED))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
    }

    if let Some(i) = remove {
        effects.recipes.remove(i);
        *dirty = true;
    }

    ui.add_space(4.0);
    if ui.button("➕ Add recipe").clicked() {
        let n = effects.recipes.len();
        effects.recipes.push(new_recipe(n));
        *dirty = true;
    }
}

fn surface_label(s: ContactSurfaceKind) -> &'static str {
    match s {
        ContactSurfaceKind::Water => "water",
        ContactSurfaceKind::Terrain => "terrain",
        ContactSurfaceKind::Unknown => "unknown",
    }
}

fn phase_label(p: ContactPhaseKind) -> &'static str {
    match p {
        ContactPhaseKind::Enter => "enter",
        ContactPhaseKind::Dwell => "dwell",
        ContactPhaseKind::Exit => "exit",
        ContactPhaseKind::Unknown => "unknown",
    }
}

fn surface_combo(ui: &mut egui::Ui, salt: usize, s: &mut ContactSurfaceKind, dirty: &mut bool) {
    // Water and terrain are the modelled surfaces (terrain landed in
    // Phase 3, #245). `Unknown` is intentionally not offered — it's a
    // forward-compat deserialize fallback, not an authorable choice.
    egui::ComboBox::from_id_salt(("surface", salt))
        .selected_text(surface_label(*s))
        .show_ui(ui, |ui| {
            for opt in [ContactSurfaceKind::Water, ContactSurfaceKind::Terrain] {
                if ui.selectable_value(s, opt, surface_label(opt)).clicked() {
                    *dirty = true;
                }
            }
        });
    ui.label("Surface");
}

fn phase_combo(ui: &mut egui::Ui, salt: usize, p: &mut ContactPhaseKind, dirty: &mut bool) {
    egui::ComboBox::from_id_salt(("phase", salt))
        .selected_text(phase_label(*p))
        .show_ui(ui, |ui| {
            for opt in [
                ContactPhaseKind::Enter,
                ContactPhaseKind::Dwell,
                ContactPhaseKind::Exit,
            ] {
                if ui.selectable_value(p, opt, phase_label(opt)).clicked() {
                    *dirty = true;
                }
            }
        });
    ui.label("Phase");
}

fn shape_combo(ui: &mut egui::Ui, salt: usize, shape: &mut EmitterShape, dirty: &mut bool) {
    let label = match shape {
        EmitterShape::Point => "point",
        EmitterShape::Sphere { .. } => "sphere",
        EmitterShape::Box { .. } => "box",
        EmitterShape::Cone { .. } => "cone",
        EmitterShape::Unknown => "unknown",
    };
    egui::ComboBox::from_id_salt(("shape", salt))
        .selected_text(label)
        .show_ui(ui, |ui| {
            // Switching variant resets to that variant's sane default;
            // the per-variant sliders below then tune it.
            if ui.selectable_label(false, "point").clicked() {
                *shape = EmitterShape::Point;
                *dirty = true;
            }
            if ui.selectable_label(false, "sphere").clicked() {
                *shape = EmitterShape::Sphere { radius: Fp(0.2) };
                *dirty = true;
            }
            if ui.selectable_label(false, "box").clicked() {
                *shape = EmitterShape::Box {
                    half_extents: Fp3([0.2, 0.2, 0.2]),
                };
                *dirty = true;
            }
            if ui.selectable_label(false, "cone").clicked() {
                *shape = EmitterShape::Cone {
                    half_angle: Fp(0.7),
                    height: Fp(0.4),
                };
                *dirty = true;
            }
        });
    ui.label("Emitter shape");

    match shape {
        EmitterShape::Sphere { radius } => {
            fp_slider(ui, "Radius", radius, 0.0, 8.0, dirty);
        }
        EmitterShape::Box { half_extents } => {
            let mut e = half_extents.0;
            let mut changed = false;
            for (axis, v) in ["X", "Y", "Z"].iter().zip(e.iter_mut()) {
                let mut f = Fp(*v);
                fp_slider(ui, axis, &mut f, 0.0, 8.0, &mut changed);
                *v = f.0;
            }
            if changed {
                half_extents.0 = e;
                *dirty = true;
            }
        }
        EmitterShape::Cone { half_angle, height } => {
            fp_slider(
                ui,
                "Half angle (rad)",
                half_angle,
                0.0,
                std::f32::consts::PI,
                dirty,
            );
            fp_slider(ui, "Height", height, 0.0, 8.0, dirty);
        }
        EmitterShape::Point | EmitterShape::Unknown => {}
    }
}
