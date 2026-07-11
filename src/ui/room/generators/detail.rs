//! Right-side detail panel: a header naming the selected node + its
//! kind picker + transform editor, followed by the per-kind detail
//! editor (delegated to [`primitive`](super::primitive),
//! [`sign`](super::sign), [`particles`](super::particles),
//! [`water`](super::water), or the Terrain / LSystem / Shape forges in
//! sibling modules of the room editor).

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
    draw_primitive_bevel, draw_primitive_blob_group, draw_primitive_capsule, draw_primitive_cone,
    draw_primitive_cuboid, draw_primitive_cylinder, draw_primitive_helix, draw_primitive_lathe,
    draw_primitive_plane, draw_primitive_sphere, draw_primitive_spine,
    draw_primitive_superellipsoid, draw_primitive_tetrahedron, draw_primitive_torus,
    draw_primitive_tube,
};
use super::reparent::{current_id, find_node, find_node_mut};
use super::sign::draw_generator_sign;
use super::tree::{node_salt, path_string};
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
    audio_editor: &mut super::super::audio::AudioEditorState,
    dirty: &mut bool,
    // In-scene blob element selection (#705); see `draw_primitive_blob_group`.
    blob_selected_element: &mut Option<usize>,
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
                draw_generator_detail(ui, &salt, &mut node.kind, dirty, blob_selected_element);

                // Per-construct audio slot (#314). The bridge writes back
                // any committed pop-out edit and offers the variant picker
                // + "Edit audio…" button, salted by node so each
                // construct keeps its own slot in egui's id stack.
                ui.add_space(6.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("Audio")
                        .strong()
                        .color(egui::Color32::LIGHT_GRAY),
                );
                super::super::audio::draw_audio_bridge(
                    ui,
                    &mut node.audio,
                    &salt,
                    dirty,
                    audio_editor,
                );
            });
    }
}

/// Inline editor for a [`crate::pds::generator::RoadConfig`] (the RoadNetwork
/// generator). Exposes the authorable street knobs; the terrain plugin
/// recomputes the road mesh from the heightmap on any change. Geometry-only
/// rendering constants (UV tile, ribbon step) stay in code.
fn draw_road_editor(
    ui: &mut egui::Ui,
    config: &mut crate::pds::generator::RoadConfig,
    dirty: &mut bool,
) {
    if ui.checkbox(&mut config.enabled, "Roads enabled").changed() {
        *dirty = true;
    }
    if ui
        .checkbox(&mut config.populate_lots, "Grow buildings on lots")
        .on_hover_text(
            "Fill the network's enclosed blocks with themed buildings at load. \
             Re-roll the layout to re-seed them.",
        )
        .changed()
    {
        *dirty = true;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(format!("Layout seed: {}", config.seed));
        if ui.button("Re-roll").clicked() {
            // Deterministic LCG step → a fresh street layout, terrain untouched.
            config.seed = config
                .seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *dirty = true;
        }
    });
    ui.add_space(4.0);
    let mut row = |v: &mut f32, lo: f32, hi: f32, label: &str| {
        if ui.add(egui::Slider::new(v, lo..=hi).text(label)).changed() {
            *dirty = true;
        }
    };
    row(
        &mut config.district_half_extent.0,
        50.0,
        512.0,
        "District ½-extent (m)",
    );
    row(
        &mut config.major_spacing.0,
        30.0,
        300.0,
        "Major spacing (m)",
    );
    row(
        &mut config.minor_spacing.0,
        20.0,
        200.0,
        "Minor spacing (m)",
    );
    row(
        &mut config.major_half_width.0,
        1.0,
        8.0,
        "Major ½-width (m)",
    );
    row(
        &mut config.minor_half_width.0,
        0.5,
        6.0,
        "Minor ½-width (m)",
    );
    row(&mut config.curb_height.0, 0.0, 0.5, "Curb height (m)");
    row(&mut config.chamfer_width.0, 0.0, 1.0, "Curb chamfer (m)");
    row(&mut config.skirt_depth.0, 1.0, 15.0, "Skirt depth (m)");
}

/// Inline editor for a [`GeneratorKind::Portal`]: the destination room's
/// DID plus the world-space exit position in that room.
fn draw_portal_editor(
    ui: &mut egui::Ui,
    target_did: &mut String,
    target_pos: &mut crate::pds::Fp3,
    dirty: &mut bool,
) {
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
        for (label, axis) in ["X", "Y", "Z"].iter().zip(target_pos.0.iter_mut()) {
            ui.label(*label);
            if ui.add(egui::DragValue::new(axis).speed(0.1)).changed() {
                *dirty = true;
            }
        }
    });
}

/// Per-kind variant detail editor — a thin dispatch: every arm is a
/// single delegation into a per-kind editor fn (the Terrain / LSystem /
/// Shape forges, the shared primitive editors, or the inline-widget
/// helpers above).
/// Does NOT render the local transform — that's drawn separately in the detail
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
    blob_selected_element: &mut Option<usize>,
) {
    match kind {
        GeneratorKind::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        GeneratorKind::Water { surface } => {
            draw_water_editor(ui, surface, dirty);
        }
        GeneratorKind::RoadNetwork(config) => draw_road_editor(ui, config, dirty),
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
        } => draw_portal_editor(ui, target_did, target_pos, dirty),
        GeneratorKind::Cuboid {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_cuboid(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Sphere {
            radius,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_sphere(
            ui, radius, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_cylinder(
            ui, radius, height, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
        } => draw_primitive_capsule(
            ui, radius, length, latitudes, longitudes, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_cone(
            ui, radius, height, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            torture,
        } => draw_primitive_torus(
            ui,
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            solid,
            material,
            torture,
        } => draw_primitive_plane(
            ui,
            size,
            subdivisions,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Tetrahedron {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_tetrahedron(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_tube(
            ui,
            radius,
            inner_radius,
            height,
            resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            solid,
            material,
            torture,
        } => draw_primitive_bevel(
            ui,
            size,
            bevel,
            bevel_segments,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        // A wedge carries the same fields as a cuboid (a bounding box); reuse
        // the cuboid editor.
        GeneratorKind::Wedge {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_cuboid(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_helix(
            ui,
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Superellipsoid {
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
        } => draw_primitive_superellipsoid(
            ui,
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Spine {
            points,
            resolution,
            samples_per_segment,
            solid,
            material,
            torture,
        } => draw_primitive_spine(
            ui,
            points,
            resolution,
            samples_per_segment,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Lathe {
            points,
            resolution,
            smooth,
            solid,
            material,
            torture,
        } => draw_primitive_lathe(
            ui, points, resolution, smooth, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::BlobGroup {
            elements,
            resolution,
            solid,
            uv_mapping,
            material,
            torture,
        } => draw_primitive_blob_group(
            ui,
            elements,
            resolution,
            solid,
            uv_mapping,
            material,
            torture,
            salt,
            dirty,
            blob_selected_element,
        ),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
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
            texture_filter,
            salt,
            dirty,
        ),
        GeneratorKind::ParticleSystem(params) => draw_generator_particles(ui, params, salt, dirty),
        GeneratorKind::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}
