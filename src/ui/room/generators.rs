//! Generators tab — master list of named generators, add/remove/rename
//! flows, and the per-kind detail editor that dispatches to the
//! Terrain / LSystem / Water / Portal / Primitive sub-editors.

use bevy_egui::egui;

use crate::pds::{Fp, Fp2, Fp3, Generator, GeneratorKind, RoomRecord, WaterSurface};
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::{DropSource, PendingGeneratorDrop, is_drop_placeable};

use super::construct::{draw_generator_tree, draw_torture, draw_universal_material};
use super::lsystem::draw_lsystem_forge;
use super::shape::draw_shape_forge;
use super::terrain::draw_terrain_forge;
use super::widgets::{
    color_picker_rgba, default_lsystem_generator, default_shape_generator, drag_u32, fp_slider,
    unique_key,
};

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
        });
        ui.add_space(4.0);
        if *selected == Some(name.clone())
            && let Some(g) = record.generators.get_mut(&name)
        {
            ui.label(format!("Generator: `{}`", name));
            ui.add_space(4.0);
            // Re-borrow as shared so nested tree editors can offer an "add
            // child from inventory" picker without fighting the outer `&mut`
            // that the master-list code still needs for the "Save to
            // Inventory" and "From Inventory..." buttons further down.
            let inv_shared: Option<&LiveInventoryRecord> = inventory.as_deref();
            // Every named generator opens into the universal tree editor —
            // that draws the root's transform + kind picker + per-kind
            // detail (via `draw_generator_detail`) + child tree.
            draw_generator_tree(ui, g, selected_prim_path, inv_shared, dirty);
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                    .clicked()
                {
                    to_remove = Some(name.clone());
                }
                if let Some(inv) = inventory.as_deref_mut()
                    && ui.small_button("Save to Inventory").clicked()
                    && let Some(g) = record.generators.get(name).cloned()
                {
                    let safe_name = unique_key(&inv.0.generators, name);
                    inv.0.generators.insert(safe_name, g);
                }
                if ui.small_button("Rename").clicked() {
                    *renaming_generator = Some((name.clone(), name.clone()));
                }
            });
        });
    }
    if let Some(name) = to_remove {
        record.generators.remove(&name);
        *dirty = true;
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label("Add new generator:");
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("+ Terrain").clicked() {
            let name = unique_key(&record.generators, "terrain");
            record.generators.insert(
                name.clone(),
                Generator::from_kind(GeneratorKind::Terrain(Default::default())),
            );
            *selected = Some(name);
            *dirty = true;
        }
        // No "+ Water" button: Water is child-only. Add it inside a
        // generator's tree via the per-node "+ Add child" affordance.
        if ui.small_button("+ LSystem").clicked() {
            let name = unique_key(&record.generators, "lsystem");
            record
                .generators
                .insert(name.clone(), default_lsystem_generator());
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Shape").clicked() {
            let name = unique_key(&record.generators, "shape");
            record
                .generators
                .insert(name.clone(), default_shape_generator());
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Portal").clicked() {
            let name = unique_key(&record.generators, "portal");
            record.generators.insert(
                name.clone(),
                Generator::from_kind(GeneratorKind::Portal {
                    target_did: String::new(),
                    target_pos: Fp3([0.0, 0.0, 0.0]),
                }),
            );
            *selected = Some(name);
            *dirty = true;
        }
        // Top-level parametric primitives. Each gets a sensible default
        // from `Generator::default_primitive_for_tag`. They all carry an
        // empty `children` list by default and can be promoted into a
        // hierarchy by adding children inside the detail view — there is
        // no separate "+ Construct" button anymore.
        for (label, tag) in [
            ("+ Cuboid", "Cuboid"),
            ("+ Sphere", "Sphere"),
            ("+ Cylinder", "Cylinder"),
            ("+ Capsule", "Capsule"),
            ("+ Cone", "Cone"),
            ("+ Torus", "Torus"),
            ("+ Plane", "Plane"),
            ("+ Tetrahedron", "Tetrahedron"),
        ] {
            if ui.small_button(label).clicked()
                && let Some(prim) = Generator::default_primitive_for_tag(tag)
            {
                let name = unique_key(&record.generators, &tag.to_lowercase());
                record.generators.insert(name.clone(), prim);
                *selected = Some(name);
                *dirty = true;
            }
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

/// Per-kind variant detail editor. Dispatches into the per-variant forges
/// for Terrain / LSystem, owns the inline Water / Portal widgets, and uses
/// a shared primitive editor for every parametric shape. Does NOT render
/// the local transform or the child tree — those belong to the wrapping
/// [`Generator`] node and are drawn by [`super::construct::draw_generator_tree`].
///
/// `salt` uniquely identifies this node in egui's ID stack — it's passed
/// through to nested material widgets so collapsing one sibling never
/// affects another when the same widget type repeats in a recursive tree.
pub(super) fn draw_generator_detail(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &mut GeneratorKind,
    dirty: &mut bool,
) {
    match kind {
        GeneratorKind::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        GeneratorKind::Water {
            level_offset,
            surface,
        } => {
            draw_water_editor(ui, level_offset, surface, dirty);
        }
        GeneratorKind::LSystem {
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
        GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
        } => draw_shape_forge(
            ui,
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
            dirty,
        ),
        GeneratorKind::Portal {
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
        GeneratorKind::Cuboid {
            size,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cuboid(ui, size, solid, material, twist, taper, bend, salt, dirty),
        GeneratorKind::Sphere {
            radius,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_sphere(
            ui, radius, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cylinder(
            ui, radius, height, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_capsule(
            ui, radius, length, latitudes, longitudes, solid, material, twist, taper, bend, salt,
            dirty,
        ),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cone(
            ui, radius, height, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_torus(
            ui,
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            twist,
            taper,
            bend,
            salt,
            dirty,
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_plane(
            ui,
            size,
            subdivisions,
            solid,
            material,
            twist,
            taper,
            bend,
            salt,
            dirty,
        ),
        GeneratorKind::Tetrahedron {
            size,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_tetrahedron(ui, size, solid, material, twist, taper, bend, salt, dirty),
        GeneratorKind::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Per-primitive detail editors. Each one owns the shape-specific drag
// widgets, the solid checkbox, the torture triple, and the material panel.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cuboid(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Size X/Y/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp3(v);
            *dirty = true;
        }
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_sphere(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        drag_u32(ui, "Ico Res", resolution, 0, 10, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cylinder(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_capsule(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    length: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Length", length, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Lats", latitudes, 2, 64, dirty);
        drag_u32(ui, "Lons", longitudes, 4, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cone(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_torus(
    ui: &mut egui::Ui,
    minor_radius: &mut Fp,
    major_radius: &mut Fp,
    minor_resolution: &mut u32,
    major_resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Minor R", minor_radius, 0.01, 50.0, dirty);
        fp_slider(ui, "Major R", major_radius, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Minor Res", minor_resolution, 3, 64, dirty);
        drag_u32(ui, "Major Res", major_resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_plane(
    ui: &mut egui::Ui,
    size: &mut Fp2,
    subdivisions: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Size X/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp2(v);
            *dirty = true;
        }
        drag_u32(ui, "Subdivs", subdivisions, 0, 32, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_tetrahedron(
    ui: &mut egui::Ui,
    size: &mut Fp,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    fp_slider(ui, "Size", size, 0.01, 100.0, dirty);
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

/// Shared tail for every primitive editor: solid checkbox, torture triple,
/// collapsible material panel. Factored out so each per-primitive editor
/// only owns its shape-specific parameter widgets.
#[allow(clippy::too_many_arguments)]
fn draw_common_primitive(
    ui: &mut egui::Ui,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    if ui.checkbox(solid, "Solid (collider)").changed() {
        *dirty = true;
    }
    ui.add_space(2.0);
    draw_torture(ui, twist, taper, bend, dirty);
    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_universal_material(ui, material, salt, dirty);
        });
}

/// Per-volume water editor: the single `level_offset` slider plus the full
/// [`WaterSurface`] knob set grouped into colour / wave / material sub-panels.
/// Room-wide water parameters (detail-normal tiling, sun glitter, scatter
/// tint) live on the Environment tab instead — the split follows the "one
/// room, many water bodies" intuition: different ponds can have different
/// colour and choppiness, but they share the room's sky and atmosphere.
fn draw_water_editor(
    ui: &mut egui::Ui,
    level_offset: &mut Fp,
    surface: &mut WaterSurface,
    dirty: &mut bool,
) {
    fp_slider(ui, "Level offset", level_offset, -20.0, 20.0, dirty);
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Colour")
        .default_open(true)
        .show(ui, |ui| {
            color_picker_rgba(ui, "Shallow (head-on)", &mut surface.shallow_color, dirty);
            color_picker_rgba(ui, "Deep (grazing)", &mut surface.deep_color, dirty);
            ui.label(
                egui::RichText::new(
                    "Alpha controls the opacity at each viewing extreme — shallow is typically \
                     low (transparent looking down), deep is high (opaque at grazing).",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });

    egui::CollapsingHeader::new("Waves")
        .default_open(true)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Scale (amplitude)",
                &mut surface.wave_scale,
                0.0,
                4.0,
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
                        egui::DragValue::new(&mut v[0])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
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
}
