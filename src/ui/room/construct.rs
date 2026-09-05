//! Helpers shared by the Generators tab's tree-view sidebar and detail
//! pane: the child-allowance predicate, the variant picker, the inventory
//! child picker, the universal material editor, and the vertex-torture
//! triple. The recursive node UI that used to live here has been replaced
//! by the [`super::generators`] split-panel layout — every node is now
//! edited in the right-hand detail panel after being selected in the tree.

use bevy_egui::egui;

use crate::pds::{
    Fp2, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, TortureParams, WaterSurface,
};

use super::material::{draw_texture_bridge, draw_uv_transform_rows};
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
                    let losses = kind_change_losses(current, is_pristine, child_count, k);
                    if losses.is_empty() {
                        *kind = make_default_for_kind(k);
                        *dirty = true;
                    } else {
                        confirm.request(
                            format!("Change kind to {k}?"),
                            format!(
                                "Switching this node to {k} discards {}. Undo \
                                 (Ctrl+Z) can restore it this session.",
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

/// Whether switching a node to `kind_tag` leaves its children with no
/// parent that can carry them — true only for the leaf-only kinds.
fn kind_strands_children(kind_tag: &str) -> bool {
    !allows_children(&make_default_for_kind(kind_tag))
}

/// What a kind change from `current` to `target` discards, as the
/// confirm's clauses; empty means nothing is lost and the switch applies
/// on the click. The child clause is computed from the TARGET kind
/// (#1209): it used to fire on `child_count > 0` alone, warning "discards
/// 3 child nodes" for the 23 of 24 targets that keep them — a scary
/// confirm on a harmless switch, which trains click-through on the one
/// that is real.
pub(super) fn kind_change_losses(
    current: &str,
    is_pristine: bool,
    child_count: usize,
    target: &str,
) -> Vec<String> {
    let mut losses: Vec<String> = Vec::new();
    if !is_pristine {
        losses.push(format!("this node's {current} settings"));
    }
    if child_count > 0 && kind_strands_children(target) {
        losses.push(format!(
            "{child_count} child node{} ({target} cannot carry children)",
            if child_count == 1 { "" } else { "s" }
        ));
    }
    losses
}

/// Apply a kind change exactly as the confirm described it (#1209): the
/// new default replaces the kind, and children the new kind cannot carry
/// go NOW — into the same undo entry — rather than sitting invisibly in
/// the record until the next sanitize flush deletes them a quarter
/// second later with no message.
pub(super) fn apply_kind_change(node: &mut Generator, kind_tag: &'static str) {
    node.kind = make_default_for_kind(kind_tag);
    if !allows_children(&node.kind) {
        node.children.clear();
    }
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
    assets: &mut super::assets::AssetPanel<'_>,
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
    draw_uv_transform_rows(ui, m, "m", dirty);

    draw_texture_bridge(ui, &mut m.texture, salt, dirty, assets);
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
    // "Vertex torture" is engine language on the most-repeated panel in the
    // product — it appears on all sixteen primitives in both editors — and
    // until #1250 f89 it was the only block in the editor with no hover text
    // at all. The explanations existed, as source comments two lines above
    // each row; they are the hover copy now, the way `UV_MODES` writes its
    // description once as data beside the value.
    ui.label("Deform")
        .on_hover_text("Bend, taper, twist and cut the shape after it is built.");
    fp_slider(
        ui,
        "Twist (rad)",
        &mut torture.twist,
        -4.0 * std::f32::consts::PI,
        4.0 * std::f32::consts::PI,
        dirty,
    )
    .on_hover_text("Rotate the top against the base, in radians. A whole turn is about 6.28.");
    // Per-axis taper (X / Z): equal = cone/frustum, unequal = wedge/fin.
    let mut tp = torture.taper.0;
    ui.horizontal(|ui| {
        ui.label("Taper top (X/Z)").on_hover_text("Narrow (or widen) the top on each axis. Equal on both is a cone; unequal is a wedge or fin. 0 leaves it alone.");
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
        ui.label("Taper bottom (X/Z)").on_hover_text(
            "The same at the bottom. Tapering both ends gives a lens or a spearhead.",
        );
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
        ui.label("Bulge (X/Z)").on_hover_text(
            "Swell (+) or pinch (−) the middle, strongest at half height — muscle, belly or waist.",
        );
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
        ui.label("Bend (X/Y/Z)").on_hover_text("Lean the top away from the base, in metres of travel. The Y value lengthens or shortens instead.");
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
        ui.label("S-bend (X/Z)").on_hover_text(
            "A serpentine wave up the height: the middle goes one way and the top comes back.",
        );
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
        ui.label("Shear (X/Z)").on_hover_text(
            "Slide the top sideways over the base, keeping the height — a leaning stack.",
        );
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
    ui.label("Cuts")
        .on_hover_text("Remove part of the shape rather than deforming it.");
    // Path-cut (begin/end, kept angular fraction of the sweep).
    let mut pc = torture.path_cut.0;
    ui.horizontal(|ui| {
        ui.label("Path-cut (begin/end)").on_hover_text("Keep only part of the way around: 0 to 1 is the whole turn. Begin past end keeps nothing.");
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
        ui.label("Profile-cut (begin/end)").on_hover_text(
            "Keep only a band of the profile from bottom (0) to top (1) — a dimple or a bowl.",
        );
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
    fp_slider(ui, "Hollow", &mut torture.hollow, 0.0, 0.95, dirty).on_hover_text(
        "Bore the middle out, as a fraction of the outer size. 0.5 leaves walls half as thick as the radius.",
    );
}

#[cfg(test)]
mod kind_change_tests {
    use super::*;

    /// #1209, finding 82. Sequence: switch a Cuboid with three children to
    /// Sphere. The confirm warned it "discards 3 child nodes" — it never
    /// did; only Water (and Unknown) refuse children, so the clause was
    /// false for 23 of the 24 targets. A false warning on a harmless
    /// switch trains click-through on the delete confirm that is real.
    #[test]
    fn the_child_clause_comes_from_the_target_kind() {
        // Pristine Cuboid → Sphere keeps its children: nothing to confirm.
        assert!(kind_change_losses("Cuboid", true, 3, "Sphere").is_empty());
        // → Water strands them, and the clause says why.
        let losses = kind_change_losses("Cuboid", true, 3, "Water");
        assert_eq!(losses.len(), 1);
        assert!(losses[0].contains("3 child nodes"), "{losses:?}");
        assert!(losses[0].contains("Water"), "{losses:?}");
        // Tuned settings are still a loss whatever the target.
        let losses = kind_change_losses("Cuboid", false, 3, "Sphere");
        assert_eq!(losses, vec![String::from("this node's Cuboid settings")]);
    }

    /// The one genuinely lossy switch used to strand the children in the
    /// record until the next 0.25 s sanitize flush deleted them — outside
    /// the undo entry, with no message. They go at apply time now.
    #[test]
    fn a_switch_to_a_leaf_kind_clears_the_children_it_was_said_to_discard() {
        let mut node = Generator::default_cuboid();
        node.children = vec![Generator::default_cuboid(); 3];
        apply_kind_change(&mut node, "Sphere");
        assert_eq!(node.children.len(), 3, "a container keeps its children");
        apply_kind_change(&mut node, "Water");
        assert!(node.children.is_empty(), "a leaf kind cannot carry them");
    }
}
