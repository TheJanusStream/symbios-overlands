//! Helpers shared by the Generators tab's tree-view sidebar and detail
//! pane: the child-allowance predicate, the variant picker, the inventory
//! child picker, the universal material editor, and the vertex-torture
//! triple. The recursive node UI that used to live here has been replaced
//! by the [`super::generators`] split-panel layout — every node is now
//! edited in the right-hand detail panel after being selected in the tree.

use bevy_egui::egui;

use crate::pds::{Fp2, Fp3, GeneratorKind, SovereignMaterialSettings, TortureParams, WaterSurface};

use super::material::draw_texture_bridge;
use super::widgets::{color_picker, fp_slider};

/// Whether a node carrying this kind is allowed to own children. Water and
/// Unknown are leaf-only — the spawner ignores their children and the
/// sanitizer strips them, so the tree-view widget hides the expand arrow
/// for those rows. Every other variant can carry children, including
/// Terrain at the root (region-blueprint shape).
pub(super) fn allows_children(kind: &GeneratorKind) -> bool {
    !matches!(kind, GeneratorKind::Water { .. } | GeneratorKind::Unknown)
}

/// Variant-picker combo box for a node's [`GeneratorKind`]. `kinds` is the
/// allowed kind-tag set for this node's position — supplied by the caller's
/// [`super::generators::GeneratorTreeSource`] so the room editor and the
/// avatar editor can offer different vocabularies (rooms allow
/// Terrain/Water/Portal; avatars exclude them).
///
/// Switching to a different primitive builds a fresh default for that
/// shape; switching to a non-primitive (Terrain/Water/LSystem/Portal)
/// constructs a reasonable starter so the owner has something to edit.
///
/// #838: a kind change discards the node's tuned params and (when the new
/// kind refuses children) strands its subtree — so when the node carries
/// children or non-default params, the switch parks behind the shared
/// confirm (answered in `draw_generators_tab`, which re-resolves
/// `node_id`) instead of applying on the click.
#[allow(clippy::too_many_arguments)]
pub(super) fn generator_kind_picker(
    ui: &mut egui::Ui,
    kind: &mut GeneratorKind,
    kinds: &[&'static str],
    salt: &str,
    dirty: &mut bool,
    node_id: &super::generators::GenNodeId,
    child_count: usize,
    confirm: &mut crate::ui::confirm::ConfirmState<(super::generators::GenNodeId, &'static str)>,
) {
    let current = kind.kind_tag();
    // "Has the user tuned anything?" — the same value a fresh switch to
    // this kind would install. Unknown has no constructor, so switching
    // away from it always warns (it discards data this build can't read).
    let is_pristine = *kind == make_default_for_kind(current);
    egui::ComboBox::from_id_salt(format!("{}_kind", salt))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for k in kinds {
                if ui.selectable_label(current == *k, *k).clicked() && current != *k {
                    if child_count == 0 && is_pristine {
                        *kind = make_default_for_kind(k);
                        *dirty = true;
                    } else {
                        let mut losses: Vec<String> = Vec::new();
                        if !is_pristine {
                            losses.push(format!("this node's {current} settings"));
                        }
                        if child_count > 0 {
                            losses.push(format!(
                                "{child_count} child node{}",
                                if child_count == 1 { "" } else { "s" }
                            ));
                        }
                        confirm.request(
                            format!("Change kind to {k}?"),
                            format!(
                                "Switching this node to {k} discards {}. This \
                                 cannot be undone.",
                                losses.join(" and ")
                            ),
                            format!("Change to {k}"),
                            (node_id.clone(), *k),
                        );
                    }
                }
            }
        });
}

/// Kind tags eligible at the **root** of a room generator tree: every
/// primitive plus LSystem / Shape / Portal / Terrain. Water is excluded
/// (child-only). Terrain *is* offered at root — promoting an existing root
/// to Terrain turns the named generator into a region blueprint.
pub(crate) const ROOM_ROOT_KINDS: &[&str] = &[
    "Cuboid",
    "Sphere",
    "Cylinder",
    "Capsule",
    "Cone",
    "Torus",
    "Plane",
    "Tetrahedron",
    "Tube",
    "Bevel",
    "Wedge",
    "Helix",
    "Superellipsoid",
    "Spine",
    "Lathe",
    "BlobGroup",
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
    "Portal",
    "Terrain",
];

/// Kind tags eligible as a **child** anywhere in a room generator tree:
/// every primitive plus LSystem / Shape / Portal / Water / RoadNetwork.
/// Terrain is excluded (root-only). RoadNetwork is only *meaningful* as a
/// Terrain child (the terrain plugin reads it there) but, like Water, is
/// offered as a generic child — misplacement simply grows no roads.
pub(super) const ROOM_CHILD_KINDS: &[&str] = &[
    "Cuboid",
    "Sphere",
    "Cylinder",
    "Capsule",
    "Cone",
    "Torus",
    "Plane",
    "Tetrahedron",
    "Tube",
    "Bevel",
    "Wedge",
    "Helix",
    "Superellipsoid",
    "Spine",
    "Lathe",
    "BlobGroup",
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
    "Portal",
    "Gateway",
    "Water",
    "RoadNetwork",
];

/// Kind tags eligible at every position inside an avatar visuals tree:
/// primitives + LSystem + Shape. Terrain / Water / Portal are excluded
/// (see [`crate::pds::sanitize_avatar_visuals`] for the rationale on
/// each), and the sanitiser overwrites any record that smuggles them
/// in to a default cuboid.
pub(crate) const AVATAR_KINDS: &[&str] = &[
    "Cuboid",
    "Sphere",
    "Cylinder",
    "Capsule",
    "Cone",
    "Torus",
    "Plane",
    "Tetrahedron",
    "Tube",
    "Bevel",
    "Wedge",
    "Helix",
    "Superellipsoid",
    "Spine",
    "Lathe",
    "BlobGroup",
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
];

pub(crate) fn make_default_for_kind(kind: &str) -> GeneratorKind {
    if let Some(prim) = GeneratorKind::default_primitive_for_tag(kind) {
        return prim;
    }
    match kind {
        "LSystem" => super::widgets::default_lsystem_kind(),
        "Shape" => super::widgets::default_shape_kind(),
        "Portal" => GeneratorKind::Portal {
            target_did: String::new(),
            target_pos: Fp3([0.0, 0.0, 0.0]),
        },
        "Gateway" => GeneratorKind::Gateway {
            size: Fp3([2.5, 3.0, 2.5]),
        },
        "Terrain" => GeneratorKind::Terrain(Default::default()),
        "Water" => GeneratorKind::Water {
            surface: WaterSurface::default(),
        },
        "RoadNetwork" => GeneratorKind::RoadNetwork(crate::pds::generator::RoadConfig::default()),
        "Sign" => GeneratorKind::default_sign(),
        "ParticleSystem" => GeneratorKind::default_particles(),
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

/// Vertex-torture editor for the [`TortureParams`] every primitive carries:
/// twist, per-axis taper (X/Z), a three-axis bend, the S-bend wave, and
/// top-shear; plus the SL-style topology cuts (path-cut / profile-cut /
/// hollow). Ranges mirror `pds::sanitize::limits::*`. `show_cuts` hides the
/// cuts block for the one kind whose mesher ignores it (Plane — no revolve
/// axis), so the GUI never offers dead sliders.
pub(super) fn draw_torture(
    ui: &mut egui::Ui,
    torture: &mut TortureParams,
    show_cuts: bool,
    dirty: &mut bool,
) {
    ui.label("Vertex torture");
    fp_slider(
        ui,
        "Twist (rad)",
        &mut torture.twist,
        -4.0 * std::f32::consts::PI,
        4.0 * std::f32::consts::PI,
        dirty,
    );
    // Per-axis taper (X / Z): equal = cone/frustum, unequal = wedge/fin.
    let mut tp = torture.taper.0;
    ui.horizontal(|ui| {
        ui.label("Taper top (X/Z)");
        for v in tp.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.02).range(-0.99..=0.99))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.taper = Fp2(tp);
    // Mirrored bottom taper: composes with the top taper so a prim can narrow
    // at both ends (lens / spearhead) without upside-down authoring.
    let mut tb = torture.taper_bottom.0;
    ui.horizontal(|ui| {
        ui.label("Taper bottom (X/Z)");
        for v in tb.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.02).range(-0.99..=0.99))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.taper_bottom = Fp2(tb);
    // Mid-profile bulge (+) / pinch (−): a sin(π·height) swell that peaks at
    // mid-height — muscle / belly / waist in one slider pair.
    let mut bu = torture.bulge.0;
    ui.horizontal(|ui| {
        ui.label("Bulge (X/Z)");
        for v in bu.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.02).range(-2.0..=2.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.bulge = Fp2(bu);
    // Three-axis bend (the Y component lengthens / shortens the top).
    let mut b = torture.bend.0;
    ui.horizontal(|ui| {
        ui.label("Bend (X/Y/Z)");
        for v in b.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(-10.0..=10.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.bend = Fp3(b);
    // S-bend amplitude (X / Z): a sin(2π·height) serpentine wave.
    let mut s = torture.s_bend.0;
    ui.horizontal(|ui| {
        ui.label("S-bend (X/Z)");
        for v in s.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(-10.0..=10.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.s_bend = Fp2(s);
    // Top-shear (X / Z): a linear lateral lean of the top vs the base.
    let mut sh = torture.shear.0;
    ui.horizontal(|ui| {
        ui.label("Shear (X/Z)");
        for v in sh.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(-10.0..=10.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.shear = Fp2(sh);

    // --- Topology cuts (every prim except Plane, which is gated off via
    // `show_cuts`) ---
    if !show_cuts {
        return;
    }
    ui.label("Cuts");
    // Path-cut (begin/end, kept angular fraction of the sweep).
    let mut pc = torture.path_cut.0;
    ui.horizontal(|ui| {
        ui.label("Path-cut (begin/end)");
        for v in pc.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.01).range(0.0..=1.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.path_cut = Fp2(pc);
    // Profile-cut / dimple (begin/end, kept latitude band on a revolved profile).
    let mut prc = torture.profile_cut.0;
    ui.horizontal(|ui| {
        ui.label("Profile-cut (begin/end)");
        for v in prc.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.01).range(0.0..=1.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    torture.profile_cut = Fp2(prc);
    // Hollow (bore as a fraction of the outer radius).
    fp_slider(ui, "Hollow", &mut torture.hollow, 0.0, 0.95, dirty);
}
