//! L-system generator tab — source/finalization code editors, rewrite-rule
//! tuning, material slots, and the `PropMeshType` mapping table.

use bevy_egui::egui;

use crate::pds::{Fp, Fp3, PropMeshType, SovereignMaterialSettings};

use super::material::{draw_texture_bridge, draw_uv_transform_rows};
use super::widgets::{color_picker, drag_u32, drag_u64, fp_slider, grammar_status_line};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_lsystem_forge(
    ui: &mut egui::Ui,
    grammar_status: Option<&crate::world_builder::grammar_diag::GrammarStatus>,
    source_code: &mut String,
    finalization_code: &mut String,
    iterations: &mut u32,
    seed: &mut u64,
    angle: &mut Fp,
    step: &mut Fp,
    width: &mut Fp,
    elasticity: &mut Fp,
    tropism: &mut Option<Fp3>,
    materials: &mut std::collections::HashMap<u16, SovereignMaterialSettings>,
    prop_mappings: &mut std::collections::HashMap<u16, PropMeshType>,
    prop_scale: &mut Fp,
    mesh_resolution: &mut u32,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    egui::CollapsingHeader::new("Source code")
        .default_open(true)
        .show(ui, |ui| {
            if crate::ui::affordances::text_edit(
                ui,
                egui::TextEdit::multiline(source_code)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            )
            .changed()
            {
                *dirty = true;
            }
        });
    egui::CollapsingHeader::new("Finalization code")
        .default_open(false)
        .show(ui, |ui| {
            if crate::ui::affordances::text_edit(
                ui,
                egui::TextEdit::multiline(finalization_code)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            )
            .changed()
            {
                *dirty = true;
            }
        });

    // Latest compile outcome (#829) — a grammar typo used to mean "the
    // world silently stops updating"; now the parser's line-numbered
    // error lands right under the code that caused it.
    grammar_status_line(ui, grammar_status);

    egui::CollapsingHeader::new("Turtle")
        .default_open(true)
        .show(ui, |ui| {
            drag_u32(ui, "Iterations", iterations, 0, 12, dirty);
            drag_u64(ui, "Seed", seed, dirty);
            fp_slider(ui, "Angle (deg)", angle, 0.0, 180.0, dirty);
            fp_slider(ui, "Step", step, 0.0, 10.0, dirty);
            fp_slider(ui, "Width", width, 0.0, 5.0, dirty);
            fp_slider(ui, "Elasticity", elasticity, 0.0, 4.0, dirty);
            fp_slider(ui, "Prop scale", prop_scale, 0.0, 10.0, dirty);
            drag_u32(ui, "Mesh resolution", mesh_resolution, 3, 32, dirty);

            let mut has_tropism = tropism.is_some();
            if ui.checkbox(&mut has_tropism, "Tropism").changed() {
                *tropism = if has_tropism {
                    Some(Fp3([0.0, -1.0, 0.0]))
                } else {
                    None
                };
                *dirty = true;
            }
            if let Some(t) = tropism.as_mut() {
                let mut v = t.0;
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("x");
                    changed |= ui
                        .add(crate::ui::num::drag(&mut v[0]).speed(0.05))
                        .changed();
                    ui.label("y");
                    changed |= ui
                        .add(crate::ui::num::drag(&mut v[1]).speed(0.05))
                        .changed();
                    ui.label("z");
                    changed |= ui
                        .add(crate::ui::num::drag(&mut v[2]).speed(0.05))
                        .changed();
                });
                if changed {
                    *t = Fp3(v);
                    *dirty = true;
                }
            }
        });

    egui::CollapsingHeader::new("Material slots")
        .default_open(false)
        .show(ui, |ui| {
            // The slot table is the bridge between the grammar text and
            // what you see, and it was a set of numbered colour pickers
            // whose effect could only be found by trial (#1250 f94). The
            // sibling Shape forge already explains its own bridge; this is
            // the same sentence for this one.
            ui.label(
                egui::RichText::new(
                    "A slot number matches the number the grammar passes to \
                     `Mat(n)`. Branches with no matching slot fall back to a \
                     default. See docs/lsystem-playbook.md for the tokens.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            let mut slot_ids: Vec<u16> = materials.keys().copied().collect();
            slot_ids.sort_unstable();
            let mut to_remove: Option<u16> = None;
            for id in slot_ids {
                let Some(m) = materials.get_mut(&id) else {
                    continue;
                };
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("Slot {}", id)).on_hover_text(format!(
                            "Painted on whatever the grammar marks `Mat({id})`."
                        ));
                        // Removing a slot the grammar still names changes
                        // the render with no stated cause (#1250 f94), so
                        // the button says which case this is.
                        let referenced = source_code.contains(&format!("Mat({id})"));
                        let hover = if referenced {
                            format!(
                                "Remove slot {id}. The grammar still names `Mat({id})` — \
                                 those branches will fall back to the default material."
                            )
                        } else {
                            format!("Remove slot {id}. The grammar does not name it.")
                        };
                        if crate::ui::affordances::remove_button(ui, &hover).clicked() {
                            to_remove = Some(id);
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

                    let salt = format!("mat_{}", id);
                    draw_texture_bridge(ui, &mut m.texture, &salt, dirty, assets);
                });
            }
            if let Some(id) = to_remove {
                materials.remove(&id);
                *dirty = true;
            }
            if ui.button("+ Add material slot").clicked() {
                let next = (0u16..=u16::MAX).find(|k| !materials.contains_key(k));
                if let Some(k) = next {
                    materials.insert(k, SovereignMaterialSettings::default());
                    *dirty = true;
                }
            }
        });

    egui::CollapsingHeader::new("Prop mappings")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "`~n` is the token the grammar writes to place a prop: this \
                     table says which shape each one draws. See \
                     docs/lsystem-playbook.md.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            let mut ids: Vec<u16> = prop_mappings.keys().copied().collect();
            ids.sort_unstable();
            let mut to_remove: Option<u16> = None;
            for id in ids {
                ui.horizontal(|ui| {
                    ui.label(format!("~{}", id))
                        .on_hover_text(format!("Drawn wherever the grammar writes `~{id}`."));
                    if let Some(current) = prop_mappings.get_mut(&id) {
                        egui::ComboBox::from_id_salt(format!("prop_map_{}", id))
                            .selected_text(current.label())
                            .show_ui(ui, |ui| {
                                let types = [
                                    PropMeshType::Leaf,
                                    PropMeshType::Twig,
                                    PropMeshType::Sphere,
                                    PropMeshType::Cone,
                                    PropMeshType::Cylinder,
                                    PropMeshType::Cube,
                                ];
                                for t in types {
                                    if ui.selectable_value(current, t, t.label()).changed() {
                                        *dirty = true;
                                    }
                                }
                            });
                    }
                    let referenced = source_code.contains(&format!("~{id}"));
                    let hover = if referenced {
                        format!(
                            "Remove the `~{id}` mapping. The grammar still writes it — \
                             those props will fall back to a leaf."
                        )
                    } else {
                        format!("Remove the `~{id}` mapping. The grammar does not write it.")
                    };
                    if crate::ui::affordances::remove_button(ui, &hover).clicked() {
                        to_remove = Some(id);
                    }
                });
            }
            if let Some(id) = to_remove {
                prop_mappings.remove(&id);
                *dirty = true;
            }
            if ui.button("+ Add mapping").clicked() {
                let next = (0u16..=255).find(|k| !prop_mappings.contains_key(k));
                if let Some(k) = next {
                    prop_mappings.insert(k, PropMeshType::Leaf);
                    *dirty = true;
                }
            }
        });
}
