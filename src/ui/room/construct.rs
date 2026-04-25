//! Universal generator-tree editor.
//!
//! Every named generator is hierarchical: a root node carrying variant-
//! specific parameters plus a `Vec<Generator>` of children. This module owns
//! the recursive node UI — local transform editor, kind picker, child
//! add/delete buttons, "🎯 Target" toggle that flags a node for the 3D
//! gizmo — that the Generators tab draws above the per-kind detail editor
//! provided by [`super::generators::draw_generator_detail`].

use bevy_egui::egui;

use crate::pds::{Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, WaterSurface};
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::is_drop_placeable;

use super::generators::draw_generator_detail;
use super::material::draw_texture_bridge;
use super::widgets::{color_picker, draw_transform, fp_slider};

pub(super) fn draw_generator_tree(
    ui: &mut egui::Ui,
    root: &mut Generator,
    selected_prim_path: &mut Option<Vec<usize>>,
    inventory: Option<&LiveInventoryRecord>,
    dirty: &mut bool,
) {
    ui.label(
        "Hierarchical generator. Root anchors to the placement; children \
        inherit transform. Every solid node contributes a collider.",
    );
    ui.add_space(4.0);
    draw_generator_node_ui(
        ui,
        root,
        true,
        dirty,
        "root",
        &[],
        selected_prim_path,
        inventory,
    );
}

/// Whether a node at this position is allowed to carry children. Terrain
/// is root-only and a region anchor (children allowed). Water is a leaf
/// (no children). Unknown can't be edited so child management is hidden.
fn allows_children(kind: &GeneratorKind) -> bool {
    !matches!(kind, GeneratorKind::Water { .. } | GeneratorKind::Unknown)
}

/// Recursive node editor. `is_root` suppresses the delete button for the
/// tree root. `path_salt` makes every egui ID unique across the recursive
/// tree so collapsing one sibling never affects another. `current_path`
/// carries the child-index chain from the named generator's root to this
/// node; the "🎯 Target" toggle writes that path into `selected_path` so
/// `editor_gizmo` can find the matching live entity.
#[allow(clippy::too_many_arguments)]
fn draw_generator_node_ui(
    ui: &mut egui::Ui,
    node: &mut Generator,
    is_root: bool,
    dirty: &mut bool,
    path_salt: &str,
    current_path: &[usize],
    selected_path: &mut Option<Vec<usize>>,
    inventory: Option<&LiveInventoryRecord>,
) -> NodeAction {
    let header = node.kind_tag();
    let mut action = NodeAction::None;
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
                    // change-tick flip and re-evaluates which entity should
                    // own the `GizmoTarget` component.
                    *dirty = true;
                }
                generator_kind_picker(ui, &mut node.kind, is_root, path_salt, dirty);
            });

            ui.add_space(4.0);
            draw_transform(ui, &mut node.transform, dirty);

            ui.add_space(4.0);
            ui.separator();
            // Per-kind detail editor — every primitive, L-system, portal,
            // water, terrain renders its variant-specific widgets here.
            draw_generator_detail(ui, path_salt, &mut node.kind, dirty);

            ui.add_space(4.0);
            // Water is a leaf and can't carry children (the spawner
            // ignores them and the sanitizer strips them); Unknown is
            // un-editable. Terrain *can* now carry children — that's the
            // region-blueprint shape. Hide the child UI on the leaf-only
            // kinds so we don't promise something the spawner won't honor.
            let allows_children = allows_children(&node.kind);

            ui.horizontal(|ui| {
                if allows_children {
                    if ui.small_button("+ Add child").clicked() {
                        node.children.push(Generator::default());
                        *dirty = true;
                    }
                    if let Some(inv) = inventory
                        && !inv.0.generators.is_empty()
                    {
                        draw_inventory_child_picker(ui, node, path_salt, inv, dirty);
                    }
                }
                if !is_root
                    && ui
                        .add(
                            egui::Button::new("− Delete")
                                .fill(egui::Color32::from_rgb(180, 50, 50)),
                        )
                        .clicked()
                {
                    action = NodeAction::Delete;
                }
            });

            if allows_children {
                ui.add_space(4.0);
                let mut to_remove: Option<usize> = None;
                for (i, child) in node.children.iter_mut().enumerate() {
                    let child_salt = format!("{}_c{}", path_salt, i);
                    let mut child_path = current_path.to_vec();
                    child_path.push(i);
                    let child_action = draw_generator_node_ui(
                        ui,
                        child,
                        false,
                        dirty,
                        &child_salt,
                        &child_path,
                        selected_path,
                        inventory,
                    );
                    if matches!(child_action, NodeAction::Delete) {
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
            }
        });
    action
}

/// Signal returned by `draw_generator_node_ui` so the parent can remove a
/// child that asked to be deleted. Keeping the delete state out of the
/// child's own mutation avoids borrow conflicts with the recursive
/// `iter_mut`.
enum NodeAction {
    None,
    Delete,
}

/// Combo box that lists every placeable generator in the owner's inventory
/// and, on click, clones the chosen entry into a fresh child appended to
/// `node.children`. Filters through [`is_drop_placeable`] so Terrain /
/// Water / Unknown — which the sanitizer would overwrite anyway — never
/// appear in the menu.
fn draw_inventory_child_picker(
    ui: &mut egui::Ui,
    node: &mut Generator,
    salt: &str,
    inventory: &LiveInventoryRecord,
    dirty: &mut bool,
) {
    let mut picked: Option<Generator> = None;
    egui::ComboBox::from_id_salt(format!("{}_inv_child", salt))
        .selected_text("+ From Inventory…")
        .show_ui(ui, |ui| {
            let mut names: Vec<&String> = inventory
                .0
                .generators
                .iter()
                .filter(|(_, g)| is_drop_placeable(g))
                .map(|(k, _)| k)
                .collect();
            names.sort();
            if names.is_empty() {
                ui.label("(no placeable inventory items)");
                return;
            }
            for name in names {
                if ui.selectable_label(false, name).clicked()
                    && let Some(g) = inventory.0.generators.get(name)
                {
                    picked = Some(g.clone());
                }
            }
        });
    if let Some(generator) = picked {
        node.children.push(generator);
        *dirty = true;
    }
}

/// Variant-picker combo box for a node's [`GeneratorKind`]. The kind set
/// depends on the node's position in the tree:
///
/// * **Root** of a named generator: every kind except Water (Water is
///   child-only; the sanitizer would overwrite a Water root anyway).
///   Terrain *is* offered at root — promoting an existing root to Terrain
///   turns the named generator into a region blueprint.
/// * **Child** node: every kind except Terrain (Terrain is root-only).
///   Water *is* offered as a child option here.
///
/// Switching to a different primitive builds a fresh default for that
/// shape; switching to a non-primitive (Terrain/Water/LSystem/Portal)
/// constructs a reasonable starter so the owner has something to edit.
fn generator_kind_picker(
    ui: &mut egui::Ui,
    kind: &mut GeneratorKind,
    is_root: bool,
    salt: &str,
    dirty: &mut bool,
) {
    const PRIMITIVES: &[&str] = &[
        "Cuboid",
        "Sphere",
        "Cylinder",
        "Capsule",
        "Cone",
        "Torus",
        "Plane",
        "Tetrahedron",
    ];
    let mut kinds: Vec<&'static str> = PRIMITIVES.to_vec();
    kinds.push("LSystem");
    kinds.push("Portal");
    if is_root {
        kinds.push("Terrain");
    } else {
        kinds.push("Water");
    }

    let current = kind.kind_tag();
    egui::ComboBox::from_id_salt(format!("{}_kind", salt))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for k in &kinds {
                if ui.selectable_label(current == *k, *k).clicked() && current != *k {
                    *kind = make_default_for_kind(k);
                    *dirty = true;
                }
            }
        });
}

fn make_default_for_kind(kind: &str) -> GeneratorKind {
    if let Some(prim) = GeneratorKind::default_primitive_for_tag(kind) {
        return prim;
    }
    match kind {
        "LSystem" => super::widgets::default_lsystem_kind(),
        "Portal" => GeneratorKind::Portal {
            target_did: String::new(),
            target_pos: Fp3([0.0, 0.0, 0.0]),
        },
        "Terrain" => GeneratorKind::Terrain(Default::default()),
        "Water" => GeneratorKind::Water {
            level_offset: Fp(0.0),
            surface: WaterSurface::default(),
        },
        _ => GeneratorKind::default_cuboid(),
    }
}

/// Slim material editor for a single primitive's `SovereignMaterialSettings`.
/// Mirrors the L-system slot UI but scoped to a single material with `salt`
/// making every internal egui id unique across the recursive tree.
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

/// Vertex-torture editor for the three fields every primitive carries.
/// Ranges mirror `pds::limits::MAX_TORTURE_*`.
pub(super) fn draw_torture(
    ui: &mut egui::Ui,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    dirty: &mut bool,
) {
    ui.label("Vertex torture");
    fp_slider(
        ui,
        "Twist (rad)",
        twist,
        -4.0 * std::f32::consts::PI,
        4.0 * std::f32::consts::PI,
        dirty,
    );
    fp_slider(ui, "Taper", taper, -0.99, 0.99, dirty);
    ui.label("Bend (X/Y/Z)");
    let mut b = bend.0;
    ui.horizontal(|ui| {
        for v in b.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(-10.0..=10.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    *bend = Fp3(b);
}
