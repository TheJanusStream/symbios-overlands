//! L-system generator tab — source/finalization code editors, rewrite-rule
//! tuning, material slots, and the `PropMeshType` mapping table.

use bevy_egui::egui;

use crate::pds::{Fp, Fp3, PropMeshType, SovereignMaterialSettings};

use super::material::draw_texture_bridge;
use super::widgets::{color_picker, drag_u32, drag_u64, fp_slider};

pub(super) fn draw_lsystem_forge(
    ui: &mut egui::Ui,
    source_code: &mut String,
    finalization_code: &mut String,
    iterations: &mut u32,
    seed: &mut u64,
    angle: &mut Fp,
    step: &mut Fp,
    width: &mut Fp,
    elasticity: &mut Fp,
    tropism: &mut Option<Fp3>,
    materials: &mut std::collections::HashMap<u8, SovereignMaterialSettings>,
    prop_mappings: &mut std::collections::HashMap<u16, PropMeshType>,
    prop_scale: &mut Fp,
    mesh_resolution: &mut u32,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Source code")
        .default_open(true)
        .show(ui, |ui| {
            if ui
                .add(
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
            if ui
                .add(
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
                        .add(egui::DragValue::new(&mut v[0]).speed(0.05))
                        .changed();
                    ui.label("y");
                    changed |= ui
                        .add(egui::DragValue::new(&mut v[1]).speed(0.05))
                        .changed();
                    ui.label("z");
                    changed |= ui
                        .add(egui::DragValue::new(&mut v[2]).speed(0.05))
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
            let mut slot_ids: Vec<u8> = materials.keys().copied().collect();
            slot_ids.sort_unstable();
            let mut to_remove: Option<u8> = None;
            for id in slot_ids {
                let Some(m) = materials.get_mut(&id) else {
                    continue;
                };
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("Slot {}", id));
                        if ui
                            .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                            .clicked()
                        {
                            to_remove = Some(id);
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

                    let salt = format!("mat_{}", id);
                    draw_texture_bridge(ui, &mut m.texture, &salt, dirty);
                });
            }
            if let Some(id) = to_remove {
                materials.remove(&id);
                *dirty = true;
            }
            if ui.button("+ Add material slot").clicked() {
                let next = (0u8..=255).find(|k| !materials.contains_key(k));
                if let Some(k) = next {
                    materials.insert(k, SovereignMaterialSettings::default());
                    *dirty = true;
                }
            }
        });

    egui::CollapsingHeader::new("Prop mappings")
        .default_open(false)
        .show(ui, |ui| {
            let mut ids: Vec<u16> = prop_mappings.keys().copied().collect();
            ids.sort_unstable();
            let mut to_remove: Option<u16> = None;
            for id in ids {
                ui.horizontal(|ui| {
                    ui.label(format!("~{}", id));
                    if let Some(current) = prop_mappings.get_mut(&id) {
                        egui::ComboBox::from_id_salt(format!("prop_map_{}", id))
                            .selected_text(format!("{:?}", current))
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
                                    if ui
                                        .selectable_value(current, t, format!("{:?}", t))
                                        .changed()
                                    {
                                        *dirty = true;
                                    }
                                }
                            });
                    }
                    if ui
                        .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                        .clicked()
                    {
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
