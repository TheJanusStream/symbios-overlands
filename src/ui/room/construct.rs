//! Helpers shared by the Generators tab's tree-view sidebar and detail
//! pane: the child-allowance predicate, the variant picker, the inventory
//! child picker, the universal material editor, and the vertex-torture
//! triple. The recursive node UI that used to live here has been replaced
//! by the [`super::generators`] split-panel layout — every node is now
//! edited in the right-hand detail panel after being selected in the tree.

use bevy_egui::egui;

use crate::pds::{Fp, Fp3, GeneratorKind, SovereignMaterialSettings, WaterSurface};

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
pub(super) fn generator_kind_picker(
    ui: &mut egui::Ui,
    kind: &mut GeneratorKind,
    kinds: &[&'static str],
    salt: &str,
    dirty: &mut bool,
) {
    let current = kind.kind_tag();
    egui::ComboBox::from_id_salt(format!("{}_kind", salt))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for k in kinds {
                if ui.selectable_label(current == *k, *k).clicked() && current != *k {
                    *kind = make_default_for_kind(k);
                    *dirty = true;
                }
            }
        });
}

/// Kind tags eligible at the **root** of a room generator tree: every
/// primitive plus LSystem / Shape / Portal / Terrain. Water is excluded
/// (child-only). Terrain *is* offered at root — promoting an existing root
/// to Terrain turns the named generator into a region blueprint.
pub(super) const ROOM_ROOT_KINDS: &[&str] = &[
    "Cuboid",
    "Sphere",
    "Cylinder",
    "Capsule",
    "Cone",
    "Torus",
    "Plane",
    "Tetrahedron",
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
    "Portal",
    "Terrain",
];

/// Kind tags eligible as a **child** anywhere in a room generator tree:
/// every primitive plus LSystem / Shape / Portal / Water. Terrain is
/// excluded (root-only).
pub(super) const ROOM_CHILD_KINDS: &[&str] = &[
    "Cuboid",
    "Sphere",
    "Cylinder",
    "Capsule",
    "Cone",
    "Torus",
    "Plane",
    "Tetrahedron",
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
    "Portal",
    "Water",
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
    "Sign",
    "ParticleSystem",
    "LSystem",
    "Shape",
];

pub(super) fn make_default_for_kind(kind: &str) -> GeneratorKind {
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
        "Terrain" => GeneratorKind::Terrain(Default::default()),
        "Water" => GeneratorKind::Water {
            level_offset: Fp(0.0),
            surface: WaterSurface::default(),
        },
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
