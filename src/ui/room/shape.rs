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

use super::material::{draw_texture_bridge, draw_uv_transform_rows};
use super::widgets::{color_picker, drag_u64, fp_slider, grammar_status_line};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_shape_forge(
    ui: &mut egui::Ui,
    grammar_status: Option<&crate::world_builder::grammar_diag::GrammarStatus>,
    grammar_source: &mut String,
    root_rule: &mut String,
    footprint: &mut Fp3,
    seed: &mut u64,
    materials: &mut std::collections::HashMap<String, SovereignMaterialSettings>,
    round_meshes: &mut Vec<String>,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    egui::CollapsingHeader::new("Grammar")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "One statement per line. Rules are `Name --> ops`; stochastic \
                     variants use `weight%` prefixes (`Facade --> 70% Brick | 30% Glass`) \
                     and guarded ones use `when(cond): ops | else: ops`. Lines may also \
                     declare `attr Name = value`, `const Name = value`, or \
                     `style Name { Attr = value }`. Lines beginning with `//` are skipped.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            if crate::ui::affordances::text_edit(
                ui,
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

    // Latest compile outcome (#829) — parser errors land right under the
    // grammar instead of vanishing into the log.
    grammar_status_line(ui, grammar_status);

    egui::CollapsingHeader::new("Lot")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Root rule");
                if crate::ui::affordances::text_edit(
                    ui,
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
                        crate::ui::num::drag(&mut v[0])
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
                        crate::ui::num::drag(&mut v[1])
                            .speed(0.5)
                            .range(0.0..=1000.0),
                    )
                    .changed();
                ui.label("z");
                changed |= ui
                    .add(
                        crate::ui::num::drag(&mut v[2])
                            .speed(0.5)
                            .range(0.001..=1000.0),
                    )
                    .changed();
                if changed {
                    *footprint = Fp3(v);
                    *dirty = true;
                }
            });

            // Turned terminals: a comma-separated list of the mesh ids
            // (`I("...")` literals) that render as elliptical prisms
            // rather than boxes. Keyed on mesh id, not material, because a
            // colonnade's shafts and its flat entablature normally share
            // one stone.
            ui.label("Turned terminals (round cross-section)");
            // Deferred commit (#1238 f77). This used to regenerate the
            // buffer from the record every frame and re-parse on every
            // change: a typed comma produced an empty entry, the empty was
            // filtered out, and the next frame re-rendered the field
            // WITHOUT the comma — so the second name could never be
            // started and the documented multi-id feature was reachable
            // only by pasting the whole string at once.
            let joined = round_meshes.join(", ");
            let out = crate::ui::room::widgets::text_draft_row(
                ui,
                "shape_round_meshes",
                &joined,
                f32::INFINITY,
                "Comma-separated mesh ids. Press Enter (or click away) to apply.",
                |_| None,
            );
            if let Some(text) = out.committed {
                *round_meshes = text
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                *dirty = true;
            }
            ui.label(
                egui::RichText::new(
                    "Mesh ids listed here bake as cylinders (or cones, with `Taper`) \
                     inscribed in their scope. The grammar still derives boxes, so \
                     splits and occlusion are unaffected.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
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
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );

            // Sort by name so the editor order is stable across frames —
            // a `HashMap` iterator's order would otherwise reshuffle after
            // every insert/remove and disorient the user.
            let mut slot_names: Vec<String> = materials.keys().cloned().collect();
            slot_names.sort();
            // Every key currently in use, for the rename refusal — read
            // once, because `materials` is borrowed mutably inside the
            // loop below.
            let taken: std::collections::HashSet<String> = slot_names.iter().cloned().collect();
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
                        // Commit on focus loss, not per keystroke (#1238
                        // f80). Per keystroke the building flashed grey
                        // through every intermediate name (none of which
                        // matches a `Mat("…")` in the grammar), a full
                        // recompile was armed per digit, an empty or
                        // colliding draft was reverted with no
                        // explanation, and the row jumped position
                        // mid-word because the list re-sorts by name every
                        // frame — taking the focused field's egui id with
                        // it. The refusal is the generator rename modal's
                        // own, so the two say the same thing.
                        let out = crate::ui::room::widgets::text_draft_row(
                            ui,
                            ("shape_slot_name", name),
                            name,
                            150.0,
                            "Slot name — press Enter (or click away) to rename",
                            |draft| {
                                crate::ui::confirm::validate_new_key(draft, name, |candidate| {
                                    taken.contains(candidate)
                                })
                                .err()
                            },
                        );
                        if let Some(new_name) = out.committed {
                            // Deferred: we still need the map's current key
                            // to fetch & display the settings this frame.
                            to_rename = Some((name.clone(), new_name));
                            *dirty = true;
                        }
                        if crate::ui::affordances::remove_button(ui, "Remove this material slot")
                            .clicked()
                        {
                            to_remove = Some(name.clone());
                        }
                    });
                    color_picker(ui, "Base colour", &mut m.base_color, dirty);
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
                    draw_uv_transform_rows(ui, m, "m", dirty);

                    let salt = format!("shape_mat_{}", name);
                    draw_texture_bridge(ui, &mut m.texture, &salt, dirty, assets);
                });
            }
            if let Some(name) = to_remove {
                materials.remove(&name);
                *dirty = true;
            }
            // The draft row already refused an empty or taken name with a
            // visible reason, so by the time a rename arrives here it is
            // valid; the guards stay as a belt-and-braces invariant on the
            // map rather than as the (silent) user-facing rule they were.
            if let Some((old, new)) = to_rename
                && old != new
                && !new.is_empty()
                && !materials.contains_key(&new)
                && let Some(settings) = materials.remove(&old)
            {
                materials.insert(new, settings);
                *dirty = true;
            }
            let cap = crate::ui::room::caps::Cap::MaterialSlots;
            if ui
                .add_enabled(
                    !cap.is_full(materials.len()),
                    egui::Button::new("+ Add material slot"),
                )
                .on_disabled_hover_text(cap.full_reason())
                .clicked()
            {
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
