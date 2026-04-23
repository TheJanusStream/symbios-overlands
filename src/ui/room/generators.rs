//! Generators tab — master list of named generators, add/remove/rename
//! flows, and the per-generator detail editor that dispatches to the
//! Terrain / Construct / LSystem / Shape / Water / Portal sub-editors.

use bevy_egui::egui;

use crate::pds::{Fp, Fp3, Generator, PrimNode, RoomRecord};
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::{DropSource, PendingGeneratorDrop, is_drop_placeable};

use super::construct::draw_construct_forge;
use super::lsystem::draw_lsystem_forge;
use super::terrain::draw_terrain_forge;
use super::widgets::{default_lsystem_generator, fp_slider, unique_key};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_generators_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    renaming_generator: &mut Option<(String, String)>,
    mut inventory: Option<&mut LiveInventoryRecord>,
    pending_drop: Option<&mut PendingGeneratorDrop>,
    can_drag_place: bool,
    dirty: &mut bool,
) {
    // Single-column master/detail: when a generator is selected and still
    // exists, render the detail view full-width with a back button;
    // otherwise render the master list full-width.
    let selected_exists = selected
        .as_ref()
        .is_some_and(|n| record.generators.contains_key(n));

    if selected_exists {
        let name = selected.clone().expect("selected_exists implies Some");
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                *selected = None;
            }
            ui.heading("Detail");
            if ui.button("Rename").clicked() {
                *renaming_generator = Some((name.clone(), name.clone()));
            }
        });
        ui.add_space(4.0);
        if *selected == Some(name.clone())
            && let Some(g) = record.generators.get_mut(&name)
        {
            draw_generator_detail(
                ui,
                &name,
                g,
                selected_prim_path,
                inventory.as_deref_mut(),
                dirty,
            );
        }
        return;
    }

    // Drop any stale selection so a later re-select of the same name starts
    // cleanly and the "(Selection no longer exists.)" state never renders.
    *selected = None;

    ui.heading("Generators");
    ui.add_space(4.0);

    let mut names: Vec<String> = record.generators.keys().cloned().collect();
    names.sort();

    let mut to_remove: Option<String> = None;
    let mut pending_drop = pending_drop;
    for name in &names {
        ui.horizontal(|ui| {
            // Mirror the inventory's drag-to-place affordance: only arm a
            // drag when (a) the viewer owns this room and (b) the generator
            // kind is point-placeable (terrain + water are room-scoped, so
            // spawning them at a ground hit makes no sense).
            let is_placeable = record
                .generators
                .get(name)
                .map(is_drop_placeable)
                .unwrap_or(false);
            let drag_armable = can_drag_place && is_placeable && pending_drop.is_some();
            let resp = if drag_armable {
                ui.selectable_label(false, name)
                    .interact(egui::Sense::click_and_drag())
            } else {
                ui.selectable_label(false, name)
            };
            if resp.clicked() {
                *selected = Some(name.clone());
            }
            if drag_armable
                && resp.drag_started()
                && let Some(pd) = pending_drop.as_deref_mut()
            {
                pd.generator_name = Some(name.clone());
                pd.source = DropSource::RoomGenerators;
            }
            if drag_armable
                && resp.dragged()
                && pending_drop
                    .as_deref()
                    .and_then(|pd| pd.generator_name.as_deref())
                    == Some(name.as_str())
            {
                // Follow-the-cursor tooltip so the owner can see what
                // they're about to drop once the pointer leaves the row.
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    egui::Id::new(("gen_drag_tip", name)),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.label(format!("Place “{name}”"));
                });
            }
            if ui
                .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                .clicked()
            {
                to_remove = Some(name.clone());
            }
        });
    }
    if let Some(name) = to_remove {
        record.generators.remove(&name);
        *dirty = true;
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label("Add new generator:");
    ui.horizontal(|ui| {
        if ui.small_button("+ Terrain").clicked() {
            let name = unique_key(&record.generators, "terrain");
            record
                .generators
                .insert(name.clone(), Generator::Terrain(Default::default()));
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Water").clicked() {
            let name = unique_key(&record.generators, "water");
            record.generators.insert(
                name.clone(),
                Generator::Water {
                    level_offset: Fp(0.0),
                },
            );
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ LSystem").clicked() {
            let name = unique_key(&record.generators, "lsystem");
            record
                .generators
                .insert(name.clone(), default_lsystem_generator());
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Portal").clicked() {
            let name = unique_key(&record.generators, "portal");
            record.generators.insert(
                name.clone(),
                Generator::Portal {
                    target_did: String::new(),
                    target_pos: Fp3([0.0, 0.0, 0.0]),
                },
            );
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Construct").clicked() {
            let name = unique_key(&record.generators, "construct");
            record.generators.insert(
                name.clone(),
                Generator::Construct {
                    root: PrimNode::default(),
                },
            );
            *selected = Some(name);
            *dirty = true;
        }
        if let Some(inv) = inventory.as_deref()
            && !inv.0.generators.is_empty()
        {
            let mut insert: Option<(String, Generator)> = None;
            egui::ComboBox::from_id_salt("from_inventory")
                .selected_text("From Inventory...")
                .show_ui(ui, |ui| {
                    let mut inv_names: Vec<&String> = inv.0.generators.keys().collect();
                    inv_names.sort();
                    for inv_name in inv_names {
                        if ui.selectable_label(false, inv_name).clicked()
                            && let Some(g) = inv.0.generators.get(inv_name)
                        {
                            insert = Some((inv_name.clone(), g.clone()));
                        }
                    }
                });
            if let Some((inv_name, g)) = insert {
                let new_name = unique_key(&record.generators, &inv_name);
                record.generators.insert(new_name.clone(), g);
                *selected = Some(new_name);
                *dirty = true;
            }
        }
    });
}

fn draw_generator_detail(
    ui: &mut egui::Ui,
    name: &str,
    generator: &mut Generator,
    selected_prim_path: &mut Option<Vec<usize>>,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(format!("Generator: `{}`", name));
        if let Some(inv) = inventory
            && ui.button("Save to Inventory").clicked()
        {
            let safe_name = unique_key(&inv.0.generators, name);
            inv.0.generators.insert(safe_name, generator.clone());
        }
    });
    ui.add_space(4.0);
    match generator {
        Generator::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        Generator::Water { level_offset } => {
            fp_slider(ui, "Level offset", level_offset, -20.0, 20.0, dirty);
        }
        Generator::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_mappings,
            prop_scale,
            mesh_resolution,
            ..
        } => draw_lsystem_forge(
            ui,
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_mappings,
            prop_scale,
            mesh_resolution,
            dirty,
        ),
        Generator::Shape { style, floors } => {
            if ui
                .add(egui::TextEdit::singleline(style).hint_text("style"))
                .changed()
            {
                *dirty = true;
            }
            if ui
                .add(egui::DragValue::new(floors).speed(1.0).range(0..=64))
                .changed()
            {
                *dirty = true;
            }
        }
        Generator::Portal {
            target_did,
            target_pos,
        } => {
            ui.label("Target DID (destination room)");
            if ui
                .add(egui::TextEdit::singleline(target_did).hint_text("did:plc:…"))
                .changed()
            {
                *dirty = true;
            }
            ui.add_space(4.0);
            ui.label("Exit position (world space in the target room)");
            // Drag the raw f32s directly — wrapping each axis in a temporary
            // `Fp` to reuse `fp_slider` wouldn't round-trip the edit back to
            // the underlying `[f32; 3]`.
            ui.horizontal(|ui| {
                ui.label("X");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[0]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Y");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[1]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Z");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[2]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        Generator::Construct { root } => {
            draw_construct_forge(ui, root, selected_prim_path, dirty);
        }
        Generator::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Construct forge — recursive hierarchical primitive editor
// ---------------------------------------------------------------------------
