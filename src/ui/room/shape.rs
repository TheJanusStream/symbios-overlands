//! CGA Shape Grammar generator tab — multi-rule source editor, footprint
//! and root-rule controls, seed for stochastic variants, and the
//! string-keyed material slot table.
//!
//! The forge mirrors `lsystem` in shape and tone: a top-level source code
//! editor, a `Turtle`-equivalent parameter group ("Lot" — root rule, seed,
//! footprint), and a collapsible "Material slots" panel keyed on the
//! `Mat("...")` slot names emitted by the upstream interpreter.

use bevy_egui::egui;

use crate::pds::{Fp3, SovereignMaterialSettings};

use super::material::draw_texture_bridge;
use super::widgets::{color_picker, drag_u64, fp_slider};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_shape_forge(
    ui: &mut egui::Ui,
    grammar_source: &mut String,
    root_rule: &mut String,
    footprint: &mut Fp3,
    seed: &mut u64,
    materials: &mut std::collections::HashMap<String, SovereignMaterialSettings>,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Grammar")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "One named rule per line: `Name --> ops`. Stochastic variants \
                     use `weight%` prefixes (e.g. `Facade --> 70% Brick | 30% Glass`). \
                     Lines beginning with `//` are skipped.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(grammar_source)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(12)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                *dirty = true;
            }
        });

    egui::CollapsingHeader::new("Lot")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Root rule");
                if ui
                    .add(
                        egui::TextEdit::singleline(root_rule)
                            .desired_width(120.0)
                            .hint_text("Lot"),
                    )
                    .changed()
                {
                    *dirty = true;
                }
            });
            drag_u64(ui, "Seed", seed, dirty);
            ui.label("Footprint (X / Y / Z, world units)");
            ui.horizontal(|ui| {
                let mut v = footprint.0;
                let mut changed = false;
                ui.label("x");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[0])
                            .speed(0.5)
                            .range(0.001..=1000.0),
                    )
                    .changed();
                ui.label("y");
                // Y is allowed to be 0 — most grammars `Extrude` the
                // initial flat plot themselves. The sanitiser clamps it to
                // [0.0, 1000.0]; keep the widget range matching.
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
                            .speed(0.5)
                            .range(0.0..=1000.0),
                    )
                    .changed();
                ui.label("z");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[2])
                            .speed(0.5)
                            .range(0.001..=1000.0),
                    )
                    .changed();
                if changed {
                    *footprint = Fp3(v);
                    *dirty = true;
                }
            });
        });

    egui::CollapsingHeader::new("Material slots")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Slot name matches the literal passed to `Mat(\"...\")` in the \
                     grammar. Terminals with no matching slot fall back to a default \
                     grey material.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );

            // Sort by name so the editor order is stable across frames —
            // a `HashMap` iterator's order would otherwise reshuffle after
            // every insert/remove and disorient the user.
            let mut slot_names: Vec<String> = materials.keys().cloned().collect();
            slot_names.sort();
            let mut to_remove: Option<String> = None;
            // Pending rename: `(old_name, new_name)`. Applied after the
            // iteration so we never mutate the map while walking it.
            let mut to_rename: Option<(String, String)> = None;
            for name in &slot_names {
                let Some(m) = materials.get_mut(name) else {
                    continue;
                };
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        let mut draft = name.clone();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut draft)
                                    .desired_width(150.0)
                                    .hint_text("slot name"),
                            )
                            .changed()
                        {
                            // Defer the actual rename: we still need the
                            // map's current key to fetch & display the
                            // settings on this frame.
                            to_rename = Some((name.clone(), draft));
                            *dirty = true;
                        }
                        if ui
                            .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                            .clicked()
                        {
                            to_remove = Some(name.clone());
                        }
                    });
                    color_picker(ui, "Base color", &mut m.base_color, dirty);
                    color_picker(ui, "Emission", &mut m.emission_color, dirty);
                    fp_slider(
                        ui,
                        "Emission strength",
                        &mut m.emission_strength,
                        0.0,
                        20.0,
                        dirty,
                    );
                    fp_slider(ui, "Roughness", &mut m.roughness, 0.0, 1.0, dirty);
                    fp_slider(ui, "Metallic", &mut m.metallic, 0.0, 1.0, dirty);
                    fp_slider(ui, "UV scale", &mut m.uv_scale, 0.1, 10.0, dirty);

                    let salt = format!("shape_mat_{}", name);
                    draw_texture_bridge(ui, &mut m.texture, &salt, dirty);
                });
            }
            if let Some(name) = to_remove {
                materials.remove(&name);
                *dirty = true;
            }
            if let Some((old, new)) = to_rename
                && old != new
                && !new.is_empty()
                && !materials.contains_key(&new)
                && let Some(settings) = materials.remove(&old)
            {
                materials.insert(new, settings);
                *dirty = true;
            }
            if ui.button("+ Add material slot").clicked() {
                // Pick a fresh `SlotN` key so the same default doesn't
                // conflict with an already-defined slot. This stays inside
                // the per-rule identifier cap enforced by the sanitiser.
                let mut n = materials.len();
                let key = loop {
                    let candidate = format!("Slot{}", n);
                    if !materials.contains_key(&candidate) {
                        break candidate;
                    }
                    n += 1;
                };
                materials.insert(key, SovereignMaterialSettings::default());
                *dirty = true;
            }
        });
}
