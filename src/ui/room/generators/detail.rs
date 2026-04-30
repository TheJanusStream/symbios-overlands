//! Right-side detail panel: a header naming the selected node + its
//! kind picker + transform editor, followed by the per-kind detail
//! editor (delegated to [`primitive`], [`sign`], [`particles`],
//! [`water`], or the Terrain / LSystem / Shape forges in sibling
//! modules of the room editor).

use bevy_egui::egui;

use crate::pds::GeneratorKind;

use super::super::construct::generator_kind_picker;
use super::super::lsystem::draw_lsystem_forge;
use super::super::shape::draw_shape_forge;
use super::super::terrain::draw_terrain_forge;
use super::super::widgets::draw_transform;
use super::GeneratorTreeSource;
use super::particles::draw_generator_particles;
use super::primitive::{
    draw_primitive_capsule, draw_primitive_cone, draw_primitive_cuboid, draw_primitive_cylinder,
    draw_primitive_plane, draw_primitive_sphere, draw_primitive_tetrahedron, draw_primitive_torus,
};
use super::sign::draw_generator_sign;
use super::tree::{current_id, find_node, find_node_mut, node_salt, path_string};
use super::water::draw_water_editor;

/// Renders only the *content* of the selected node — kind picker,
/// transform, per-kind detail editor — plus a header that names the node
/// and shows its path. Every structural operation (Add child / Add child
/// from Inventory / Rename / Save to Inventory / Delete) lives in the
/// per-row context menu on the tree panel; this function never mutates
/// the tree shape.
pub(super) fn draw_detail_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    dirty: &mut bool,
) {
    let Some(id) = current_id(selected_generator, selected_prim_path) else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("Select a generator from the tree to edit.")
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Right-click any tree row for: + Add child / Rename / Save to Inventory / − Delete.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
        return;
    };

    let is_root = id.path.is_empty();
    // Snapshot the kind tag and choose the kind-picker vocabulary up
    // front so the immutable borrow used for the header is released
    // before we re-enter the source mutably for the editor body.
    let kind_tag = match find_node(&*source, &id) {
        Some(snapshot) => snapshot.kind_tag(),
        None => {
            // The selection points at a node that just disappeared (e.g.
            // its parent was kind-changed to a no-children variant). The
            // tree panel will sync the selection to None on the next
            // frame; show a brief placeholder for this frame.
            ui.label("(selected node no longer exists)");
            return;
        }
    };
    let allowed_kinds: &'static [&'static str] = if is_root {
        source.allowed_kinds_for_root()
    } else {
        source.allowed_kinds_for_child()
    };

    ui.horizontal(|ui| {
        if is_root {
            ui.heading(&id.root);
            ui.label(egui::RichText::new(format!("({})", kind_tag)).color(egui::Color32::GRAY));
        } else {
            ui.heading(kind_tag);
            ui.label(
                egui::RichText::new(format!("path: /{}", path_string(&id.path)))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    });

    ui.separator();

    let salt = node_salt(&id);

    if let Some(node) = find_node_mut(source, &id) {
        ui.horizontal(|ui| {
            ui.label("Kind:");
            generator_kind_picker(ui, &mut node.kind, allowed_kinds, &salt, dirty);
        });

        ui.add_space(4.0);
        draw_transform(ui, &mut node.transform, dirty);
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt(("gen_detail_scroll", &salt))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                draw_generator_detail(ui, &salt, &mut node.kind, dirty);
            });
    }
}

/// Per-kind variant detail editor. Dispatches into the per-variant forges
/// for Terrain / LSystem / Shape, owns the inline Water / Portal widgets,
/// and uses a shared primitive editor for every parametric shape. Does NOT
/// render the local transform — that's drawn separately in the detail
/// panel header.
///
/// `salt` uniquely identifies this node in egui's ID stack — it's passed
/// through to nested material widgets so collapsing one node never
/// affects another when the same widget type repeats across the tree.
fn draw_generator_detail(
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
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
        } => draw_generator_sign(
            ui,
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
            salt,
            dirty,
        ),
        GeneratorKind::ParticleSystem {
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            looping,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            blend_mode,
            billboard,
            simulation_space,
            inherit_velocity,
            collide_terrain,
            collide_water,
            collide_colliders,
            bounce,
            friction,
            seed,
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
        } => draw_generator_particles(
            ui,
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            looping,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            blend_mode,
            billboard,
            simulation_space,
            inherit_velocity,
            collide_terrain,
            collide_water,
            collide_colliders,
            bounce,
            friction,
            seed,
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
            salt,
            dirty,
        ),
        GeneratorKind::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}
