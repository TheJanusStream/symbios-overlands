//! Construct generator tab — hierarchical `PrimNode` tree editor with
//! per-shape parametric controls and per-node material/transform widgets.

use bevy_egui::egui;

use crate::pds::{Fp2, Fp3, PrimNode, PrimShape, SovereignMaterialSettings};

use super::material::draw_texture_bridge;
use super::widgets::{color_picker, drag_u32, draw_transform, fp_slider};

pub(super) fn draw_construct_forge(
    ui: &mut egui::Ui,
    root: &mut PrimNode,
    selected_prim_path: &mut Option<Vec<usize>>,
    dirty: &mut bool,
) {
    ui.label(
        "Hierarchical primitive tree. Root anchors to the world; \
        children inherit transform, and every solid node contributes a collider.",
    );
    ui.add_space(4.0);
    draw_prim_node_ui(ui, root, true, dirty, "root", &[], selected_prim_path);
}

/// Recursive node editor. `is_root` suppresses the delete button for the tree
/// root. `path_salt` makes every egui ID unique across the recursive tree so
/// collapsing one sibling never affects another. `current_path` carries the
/// child-index chain from the blueprint root to this node; the "🎯 Target"
/// toggle writes that path into `selected_path` so `editor_gizmo` can find
/// the matching live entity.
fn draw_prim_node_ui(
    ui: &mut egui::Ui,
    node: &mut PrimNode,
    is_root: bool,
    dirty: &mut bool,
    path_salt: &str,
    current_path: &[usize],
    selected_path: &mut Option<Vec<usize>>,
) -> PrimNodeAction {
    let header = format!("{:?}", node.shape);
    let mut action = PrimNodeAction::None;
    egui::CollapsingHeader::new(header)
        .id_salt(path_salt)
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let is_targeted = selected_path.as_ref().is_some_and(|p| p == current_path);
                let mut toggle = is_targeted;
                if ui.toggle_value(&mut toggle, "🎯 Target").clicked() {
                    if toggle {
                        *selected_path = Some(current_path.to_vec());
                    } else {
                        *selected_path = None;
                    }
                    // Bump `is_dirty` so `sync_gizmo_selection` observes the
                    // change-tick flip and re-evaluates which prim entity
                    // should own the `GizmoTarget` component.
                    *dirty = true;
                }
                shape_combo(ui, &mut node.shape, path_salt, dirty);
            });

            if ui.checkbox(&mut node.solid, "Solid (collider)").changed() {
                *dirty = true;
            }

            ui.add_space(4.0);
            draw_transform(ui, &mut node.transform, dirty);

            egui::CollapsingHeader::new("Material")
                .id_salt(format!("{}_mat", path_salt))
                .default_open(false)
                .show(ui, |ui| {
                    draw_universal_material(ui, &mut node.material, path_salt, dirty);
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.small_button("+ Add child").clicked() {
                    node.children.push(PrimNode::default());
                    *dirty = true;
                }
                if !is_root
                    && ui
                        .add(
                            egui::Button::new("− Delete")
                                .fill(egui::Color32::from_rgb(180, 50, 50)),
                        )
                        .clicked()
                {
                    action = PrimNodeAction::Delete;
                }
            });

            ui.add_space(4.0);
            let mut to_remove: Option<usize> = None;
            for (i, child) in node.children.iter_mut().enumerate() {
                let child_salt = format!("{}_c{}", path_salt, i);
                let mut child_path = current_path.to_vec();
                child_path.push(i);
                let child_action = draw_prim_node_ui(
                    ui,
                    child,
                    false,
                    dirty,
                    &child_salt,
                    &child_path,
                    selected_path,
                );
                if matches!(child_action, PrimNodeAction::Delete) {
                    to_remove = Some(i);
                }
            }
            if let Some(i) = to_remove {
                node.children.remove(i);
                *dirty = true;
                // Clear the gizmo target if we just removed the targeted
                // node or any ancestor of it — its entity is about to be
                // despawned on the next compile, and leaving the stale path
                // in place would point at a hole.
                let mut deleted_path = current_path.to_vec();
                deleted_path.push(i);
                if let Some(sel) = selected_path.as_ref()
                    && sel.starts_with(&deleted_path)
                {
                    *selected_path = None;
                }
            }
        });
    action
}

/// Signal returned by `draw_prim_node_ui` so the parent can remove a child
/// that asked to be deleted. Keeping the delete state out of the child's own
/// mutation avoids borrow conflicts with the recursive `iter_mut`.
enum PrimNodeAction {
    None,
    Delete,
}

fn shape_combo(ui: &mut egui::Ui, shape: &mut PrimShape, salt: &str, dirty: &mut bool) {
    let current_variant = shape.kind_tag();

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(format!("{}_shape", salt))
            .selected_text(current_variant)
            .show_ui(ui, |ui| {
                let variants = [
                    "Cuboid",
                    "Sphere",
                    "Cylinder",
                    "Capsule",
                    "Cone",
                    "Torus",
                    "Plane",
                    "Tetrahedron",
                ];
                for v in variants {
                    if ui.selectable_label(current_variant == v, v).clicked() {
                        *shape = PrimShape::default_for_tag(v);
                        *dirty = true;
                    }
                }
            });
    });

    ui.add_space(2.0);

    match shape {
        PrimShape::Cuboid { size } => {
            ui.horizontal(|ui| {
                ui.label("Size X/Y/Z:");
                let mut v = size.0;
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[0])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[2])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed();
                if changed {
                    *size = Fp3(v);
                    *dirty = true;
                }
            });
        }
        PrimShape::Sphere { radius, resolution } => {
            ui.horizontal(|ui| {
                fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
                drag_u32(ui, "Ico Res", resolution, 0, 10, dirty);
            });
        }
        PrimShape::Cylinder {
            radius,
            height,
            resolution,
        } => {
            ui.horizontal(|ui| {
                fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
                fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
                drag_u32(ui, "Res", resolution, 3, 128, dirty);
            });
        }
        PrimShape::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
        } => {
            ui.horizontal(|ui| {
                fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
                fp_slider(ui, "Length", length, 0.01, 100.0, dirty);
            });
            ui.horizontal(|ui| {
                drag_u32(ui, "Lats", latitudes, 2, 64, dirty);
                drag_u32(ui, "Lons", longitudes, 4, 128, dirty);
            });
        }
        PrimShape::Cone {
            radius,
            height,
            resolution,
        } => {
            ui.horizontal(|ui| {
                fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
                fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
                drag_u32(ui, "Res", resolution, 3, 128, dirty);
            });
        }
        PrimShape::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
        } => {
            ui.horizontal(|ui| {
                fp_slider(ui, "Minor R", minor_radius, 0.01, 50.0, dirty);
                fp_slider(ui, "Major R", major_radius, 0.01, 100.0, dirty);
            });
            ui.horizontal(|ui| {
                drag_u32(ui, "Minor Res", minor_resolution, 3, 64, dirty);
                drag_u32(ui, "Major Res", major_resolution, 3, 128, dirty);
            });
        }
        PrimShape::Plane { size, subdivisions } => {
            ui.horizontal(|ui| {
                ui.label("Size X/Z:");
                let mut v = size.0;
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[0])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed();
                if changed {
                    *size = Fp2(v);
                    *dirty = true;
                }
                drag_u32(ui, "Subdivs", subdivisions, 0, 32, dirty);
            });
        }
        PrimShape::Tetrahedron { size } => {
            fp_slider(ui, "Size", size, 0.01, 100.0, dirty);
        }
    }
}

/// Slim material editor for a single Prim node. Mirrors the L-system slot
/// UI but scoped to a single `SovereignMaterialSettings` with `salt` making
/// every internal egui id unique across the recursive tree.
pub(crate) fn draw_universal_material(
    ui: &mut egui::Ui,
    m: &mut SovereignMaterialSettings,
    salt: &str,
    dirty: &mut bool,
) {
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

    draw_texture_bridge(ui, &mut m.texture, salt, dirty);
}
