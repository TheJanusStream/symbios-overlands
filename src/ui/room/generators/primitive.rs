//! Per-primitive detail editors. Each owns the shape-specific drag
//! widgets, the solid checkbox, the torture triple, and the material
//! panel.

use bevy_egui::egui;

use crate::pds::generator::{
    BlobElement, BlobShape, FaceKey, FaceOverride, GeneratorKind, LathePoint, PrimCommon,
    SpinePoint, UvMapping,
};
use crate::pds::sanitize::limits::{MAX_BLOB_ELEMENTS, MAX_FACE_OVERRIDES, MAX_SWEEP_POINTS};
use crate::pds::{Fp, Fp2, Fp3, SovereignMaterialSettings};

use super::super::construct::{draw_torture, draw_universal_material};
use super::super::widgets::{drag_u32, euler_rotation_row, fp_slider};

pub(super) fn draw_primitive_cuboid(ui: &mut egui::Ui, size: &mut Fp3, edit: PrimEdit<'_, '_>) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
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
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_sphere(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        drag_u32(ui, "Ico Res", resolution, 0, 6, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_cylinder(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_capsule(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    length: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Length", length, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Lats", latitudes, 2, 64, dirty);
        drag_u32(ui, "Lons", longitudes, 4, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_cone(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_torus(
    ui: &mut egui::Ui,
    minor_radius: &mut Fp,
    major_radius: &mut Fp,
    minor_resolution: &mut u32,
    major_resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Minor R", minor_radius, 0.01, 50.0, dirty);
        fp_slider(ui, "Major R", major_radius, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Minor Res", minor_resolution, 3, 64, dirty);
        drag_u32(ui, "Major Res", major_resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_plane(
    ui: &mut egui::Ui,
    size: &mut Fp2,
    subdivisions: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
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
    // The Plane has no revolve axis — its mesher ignores the topology cuts,
    // so don't offer them.
    draw_common_primitive(ui, common, faces, salt, false, dirty);
}

pub(super) fn draw_primitive_tetrahedron(ui: &mut egui::Ui, size: &mut Fp, edit: PrimEdit<'_, '_>) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    fp_slider(ui, "Size", size, 0.01, 100.0, dirty);
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_tube(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    inner_radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Outer R", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Inner R", inner_radius, 0.0, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_bevel(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    bevel: &mut Fp,
    bevel_segments: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
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
    ui.horizontal(|ui| {
        fp_slider(ui, "Bevel", bevel, 0.0, 50.0, dirty);
        drag_u32(ui, "Segments", bevel_segments, 1, 16, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_helix(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    tube_radius: &mut Fp,
    pitch: &mut Fp,
    turns: &mut Fp,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Tube", tube_radius, 0.01, 50.0, dirty);
    });
    ui.horizontal(|ui| {
        fp_slider(ui, "Pitch", pitch, 0.0, 100.0, dirty);
        fp_slider(ui, "Turns", turns, 0.05, 16.0, dirty);
        drag_u32(ui, "Res/turn", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_superellipsoid(
    ui: &mut egui::Ui,
    half_extents: &mut Fp3,
    exponent_ns: &mut Fp,
    exponent_ew: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.horizontal(|ui| {
        ui.label("Half-extents X/Y/Z:");
        let mut v = half_extents.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *half_extents = Fp3(v);
            *dirty = true;
        }
    });
    // The two exponents are the shape: ~0.2 = box, 0.5 = pillow, 1.0 =
    // sphere/ellipsoid, 2.0 = octahedral, 2.5 = pinched star.
    ui.horizontal(|ui| {
        fp_slider(ui, "Exp N-S", exponent_ns, 0.2, 2.5, dirty);
        fp_slider(ui, "Exp E-W", exponent_ew, 0.2, 2.5, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Lats", latitudes, 4, 64, dirty);
        drag_u32(ui, "Lons", longitudes, 4, 128, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_spine(
    ui: &mut egui::Ui,
    points: &mut Vec<SpinePoint>,
    resolution: &mut u32,
    samples_per_segment: &mut u32,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.label("Spine points (X/Y/Z, radius):");
    let mut remove: Option<usize> = None;
    for (i, p) in points.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{i}"));
            let mut v = p.position.0;
            let mut changed = false;
            for axis in v.iter_mut() {
                changed |= ui
                    .add(egui::DragValue::new(axis).speed(0.05).range(-100.0..=100.0))
                    .changed();
            }
            if changed {
                p.position = Fp3(v);
                *dirty = true;
            }
            let mut r = p.radius.0;
            if ui
                .add(egui::DragValue::new(&mut r).speed(0.01).range(0.01..=100.0))
                .changed()
            {
                p.radius = Fp(r);
                *dirty = true;
            }
            if crate::ui::affordances::remove_button(ui, "Remove point").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && points.len() > 2
    {
        points.remove(i);
        *dirty = true;
    }
    if points.len() < MAX_SWEEP_POINTS && ui.button("+ Add point").clicked() {
        // Extend past the current end, continuing the last segment's
        // direction so the new point doesn't fold the spline back.
        let last = points[points.len() - 1];
        let prev = points[points.len() - 2];
        let step = [
            last.position.0[0] * 2.0 - prev.position.0[0],
            last.position.0[1] * 2.0 - prev.position.0[1],
            last.position.0[2] * 2.0 - prev.position.0[2],
        ];
        points.push(SpinePoint {
            position: Fp3(step),
            radius: last.radius,
        });
        *dirty = true;
    }
    ui.horizontal(|ui| {
        drag_u32(ui, "Ring segs", resolution, 3, 64, dirty);
        drag_u32(ui, "Samples/seg", samples_per_segment, 2, 32, dirty);
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_lathe(
    ui: &mut egui::Ui,
    points: &mut Vec<LathePoint>,
    resolution: &mut u32,
    smooth: &mut bool,
    edit: PrimEdit<'_, '_>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.label("Profile (radius, height — bottom to top):");
    let mut remove: Option<usize> = None;
    for (i, p) in points.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{i}"));
            let mut r = p.radius.0;
            if ui
                .add(egui::DragValue::new(&mut r).speed(0.01).range(0.0..=100.0))
                .changed()
            {
                p.radius = Fp(r);
                *dirty = true;
            }
            let mut h = p.height.0;
            if ui
                .add(
                    egui::DragValue::new(&mut h)
                        .speed(0.05)
                        .range(-100.0..=100.0),
                )
                .changed()
            {
                p.height = Fp(h);
                *dirty = true;
            }
            if crate::ui::affordances::remove_button(ui, "Remove station").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && points.len() > 2
    {
        points.remove(i);
        *dirty = true;
    }
    if points.len() < MAX_SWEEP_POINTS && ui.button("+ Add station").clicked() {
        let last = points[points.len() - 1];
        points.push(LathePoint {
            radius: last.radius,
            height: Fp(last.height.0 + 0.25),
        });
        *dirty = true;
    }
    ui.horizontal(|ui| {
        drag_u32(ui, "Revolve segs", resolution, 3, 128, dirty);
        if ui.checkbox(smooth, "Smooth (spline)").changed() {
            *dirty = true;
        }
    });
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

pub(super) fn draw_primitive_blob_group(
    ui: &mut egui::Ui,
    elements: &mut Vec<BlobElement>,
    resolution: &mut u32,
    edit: PrimEdit<'_, '_>,
    // In-scene edit selection (#705): which element carries the 3D gizmo.
    // Mirrors `editor_gizmo::BlobEditContext::selected_element` — a row
    // click here and a proxy click in the scene land in the same slot.
    selected_element: &mut Option<usize>,
) {
    let PrimEdit {
        common,
        faces,
        salt,
        dirty,
    } = edit;
    ui.label("Blob elements (evaluated top to bottom):");
    ui.label(
        egui::RichText::new(
            "Click an element's number (or its red/green ghost in the scene) \
             to sculpt it with the gizmo. Esc returns to the whole prim.",
        )
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
    let mut remove: Option<usize> = None;
    let mut duplicate: Option<usize> = None;
    for (i, e) in elements.iter_mut().enumerate() {
        ui.push_id((salt, "blob_el", i), |ui| {
            ui.horizontal(|ui| {
                let is_selected = *selected_element == Some(i);
                if ui
                    .selectable_label(is_selected, format!("{i}"))
                    .on_hover_text("Select for in-scene gizmo editing")
                    .clicked()
                {
                    *selected_element = if is_selected { None } else { Some(i) };
                }
                let shapes = [
                    (BlobShape::Sphere, "Sphere"),
                    (BlobShape::Capsule, "Capsule"),
                    (BlobShape::Ellipsoid, "Ellipsoid"),
                    (BlobShape::Box, "Box"),
                    (BlobShape::Cylinder, "Cylinder"),
                    (BlobShape::Torus, "Torus"),
                    (BlobShape::Cone, "Cone"),
                ];
                let current = shapes
                    .iter()
                    .find(|(v, _)| *v == e.shape)
                    .map(|(_, n)| *n)
                    .unwrap_or("Unknown");
                egui::ComboBox::from_id_salt("shape")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (v, n) in shapes {
                            if ui.selectable_label(e.shape == v, n).clicked() && e.shape != v {
                                e.shape = v;
                                *dirty = true;
                            }
                        }
                    });
                if ui.checkbox(&mut e.subtract, "Carve").changed() {
                    *dirty = true;
                }
                let mut b = e.blend.0;
                ui.label("Blend");
                if ui
                    .add(egui::DragValue::new(&mut b).speed(0.01).range(0.0..=10.0))
                    .changed()
                {
                    e.blend = Fp(b);
                    *dirty = true;
                }
                if ui.button("⎘").on_hover_text("Duplicate").clicked() {
                    duplicate = Some(i);
                }
                if crate::ui::affordances::remove_button(ui, "Remove this element").clicked() {
                    remove = Some(i);
                }
            });
            ui.horizontal(|ui| {
                ui.label("  Pos");
                let mut v = e.position.0;
                let mut changed = false;
                for c in v.iter_mut() {
                    changed |= ui
                        .add(egui::DragValue::new(c).speed(0.05).range(-100.0..=100.0))
                        .changed();
                }
                if changed {
                    e.position = Fp3(v);
                    *dirty = true;
                }
                // Sphere: radii[0]. Ellipsoid: semi-axes. Capsule: radius +
                // half-length.
                //
                // A sphere's SDF only reads radii[0], so its three boxes all
                // show that one radius: editing the first resizes it
                // uniformly (stays a sphere), while editing the Y or Z box
                // stretches one axis and promotes it to an ellipsoid so
                // per-axis size works from the GUI too (#707).
                if e.shape == BlobShape::Sphere {
                    ui.label("Radius").on_hover_text(
                        "Edit X to resize the sphere; edit Y or Z to stretch it into an ellipsoid.",
                    );
                    let r0 = e.radii.0[0];
                    let (mut rx, mut ry, mut rz) = (r0, r0, r0);
                    let size = |ui: &mut egui::Ui, v: &mut f32| {
                        ui.add(egui::DragValue::new(v).speed(0.02).range(0.01..=100.0))
                            .changed()
                    };
                    let cx = size(ui, &mut rx);
                    let cy = size(ui, &mut ry);
                    let cz = size(ui, &mut rz);
                    if cy || cz {
                        e.shape = BlobShape::Ellipsoid;
                        e.radii = Fp3([rx, ry, rz]);
                        *dirty = true;
                    } else if cx {
                        e.radii = Fp3([rx, rx, rx]);
                        *dirty = true;
                    }
                } else {
                    // The three boxes mean different things per shape; the
                    // hover hint keeps the row compact without a per-shape
                    // widget fork.
                    let hint = match e.shape {
                        BlobShape::Ellipsoid => "Semi-axes X / Y / Z.",
                        BlobShape::Capsule => "Tube radius / half-length / (unused).",
                        BlobShape::Box => "Half-extents X / Y / Z.",
                        BlobShape::Cylinder => "Radius / half-height / (unused).",
                        BlobShape::Cone => "Base radius / half-height / tip radius.",
                        BlobShape::Torus => "Ring radius / tube radius / (unused).",
                        BlobShape::Sphere | BlobShape::Unknown => "Radius / (unused) / (unused).",
                    };
                    ui.label("Size").on_hover_text(hint);
                    let mut r = e.radii.0;
                    let mut changed = false;
                    for c in r.iter_mut() {
                        changed |= ui
                            .add(egui::DragValue::new(c).speed(0.02).range(0.01..=100.0))
                            .changed();
                    }
                    if changed {
                        e.radii = Fp3(r);
                        *dirty = true;
                    }
                }
            });
            // Orientation as yaw/pitch/roll DEGREE drags, stored as a
            // quaternion — the shared #826 row, so every rotation editor
            // in the app speaks the same units.
            euler_rotation_row(ui, "  Rot", &mut e.rotation, dirty);
        });
    }
    if let Some(i) = remove
        && elements.len() > 1
    {
        elements.remove(i);
        // Keep the in-scene selection pointing at the same element as
        // the list shifts (or drop it if it was the removed row).
        match selected_element {
            Some(s) if *s == i => *selected_element = None,
            Some(s) if *s > i => *s -= 1,
            _ => {}
        }
        *dirty = true;
    }
    if let Some(i) = duplicate
        && elements.len() < MAX_BLOB_ELEMENTS
    {
        let copy = elements[i];
        elements.insert(i + 1, copy);
        if let Some(s) = selected_element
            && *s > i
        {
            *s += 1;
        }
        *dirty = true;
    }
    if elements.len() < MAX_BLOB_ELEMENTS && ui.button("+ Add element").clicked() {
        elements.push(BlobElement::default());
        *dirty = true;
    }
    drag_u32(ui, "Grid res", resolution, 8, 48, dirty);
    draw_common_primitive(ui, common, faces, salt, true, dirty);
}

/// The UV projection modes an author can pick, with the hover copy that
/// explains each. Shared by the whole-prim picker ([`draw_uv_mapping`]) and
/// the per-face one ([`draw_face_uv_mapping`]) so a mode's description is
/// written once.
const UV_MODES: [(UvMapping, &str, &str); 7] = [
    (
        UvMapping::Box,
        "Box (tri-planar)",
        "Projects each face along its dominant axis at uniform \
         density — the default. Strong patterns show faint seams \
         where the projection axis changes.",
    ),
    (
        UvMapping::Fit,
        "Fit (span once)",
        "Keeps the mesher's own layout, spanning the surface \
         exactly once. Required for alpha cards — window glazing, \
         foliage billboards — which upload clamp-to-edge and would \
         otherwise tile. The default on Plane and on the revolved \
         family (sphere, cylinder, torus, lathe, …), whose meshers \
         wrap the shape analytically better than any projection.",
    ),
    (
        UvMapping::Spherical,
        "Spherical",
        "Wraps once around the mass from its centre. Reads well on \
         roundish blobs; stretches on elongated ones and repeats \
         the texture on concave regions.",
    ),
    (
        UvMapping::Cylindrical,
        "Cylindrical (Y)",
        "Wraps around the prim's local Y axis (the cut axis), in \
         metres of arc. Suits limbs, trunks and columns; \
         up/down-facing surface swirls.",
    ),
    (
        UvMapping::PlanarX,
        "Planar X",
        "Flat projection along local X. Back side mirrors.",
    ),
    (
        UvMapping::PlanarY,
        "Planar Y",
        "Flat top-down projection — slabs and ground masses. \
         Underside mirrors.",
    ),
    (
        UvMapping::PlanarZ,
        "Planar Z",
        "Flat projection along local Z. Back side mirrors.",
    ),
];

/// The display name of a projection mode, or `"Unknown"` for a value from
/// a newer client (the open union's whole point).
fn uv_mode_name(mapping: UvMapping) -> &'static str {
    UV_MODES
        .iter()
        .find(|(v, _, _)| *v == mapping)
        .map(|(_, n, _)| *n)
        .unwrap_or("Unknown")
}

/// The shared UV-projection picker (#937). All sixteen primitive kinds
/// carry a `uv_mapping` field and all sixteen show this same control, so a
/// mode means the same thing wherever it appears.
///
/// The revolved family got the row late (#963): its field only started
/// meshing differently when `projection_for` generalised in #959, and until
/// then a picker would have been dead UI. Their default stays `Fit` — their
/// meshers' analytic wrap — so nothing that already existed moved.
pub(super) fn draw_uv_mapping(
    ui: &mut egui::Ui,
    common: &mut PrimCommon,
    family: UvMapping,
    salt: &str,
    dirty: &mut bool,
) {
    // The record stores `None` for the family's own projection (#1188);
    // the picker shows the resolved mode and folds a pick of the family
    // default back to `None`, so the wire never gains a key an older
    // client would have elided.
    let current = common.uv_mapping.unwrap_or(family);
    ui.horizontal(|ui| {
        ui.label("UV mapping").on_hover_text(
            "How the texture is projected onto the meshed surface. Every \
             mode but Fit measures in metres, so `uv_scale` reads as tiles \
             per metre and one material looks the same on prims of any size.",
        );
        egui::ComboBox::from_id_salt((salt, "uv_mapping"))
            .selected_text(uv_mode_name(current))
            .show_ui(ui, |ui| {
                for (v, n, hint) in UV_MODES {
                    if ui
                        .selectable_label(current == v, n)
                        .on_hover_text(hint)
                        .clicked()
                        && current != v
                    {
                        common.uv_mapping = (v != family).then_some(v);
                        *dirty = true;
                    }
                }
            });
    });
}

/// Everything a primitive editor needs beyond its own dimensional knobs
/// (#1188): the shared block it edits in place, the Faces panel's context,
/// the egui salt and the dirty flag. One parameter, so the fifteen
/// per-kind editors are `(ui, own knobs…, edit)` and a field added to
/// [`PrimCommon`] reaches every one of them without a signature moving.
pub(super) struct PrimEdit<'a, 'u> {
    /// The prim's shared block — solid, projection, material, faces,
    /// torture — edited in place.
    pub common: &'a mut PrimCommon,
    pub faces: FacePanel<'a, 'u>,
    /// This node's egui ID salt, threaded to every nested widget.
    pub salt: &'a str,
    pub dirty: &'a mut bool,
}

/// Everything the shared Faces panel (#960) needs besides the override
/// list itself, which lives in the [`PrimCommon`] the editor already holds.
pub(super) struct FacePanel<'a, 'u> {
    /// The whole node as of this frame — what the panel enumerates faces
    /// from, and where an inherited projection comes from. `None` for a
    /// kind with no faces at all.
    ///
    /// A *snapshot* because the dispatch match holds the node's `common`
    /// mutably: the panel cannot borrow the node again. It is taken
    /// before the match and costs one clone per frame for the selected
    /// node only.
    pub snapshot: Option<&'a GeneratorKind>,
    /// Undo-history label sink, so a face edit reads as one in the ⌘Z list
    /// instead of the generic "edit".
    pub undo_label: &'a mut crate::ui::undo::LabelSlot<'u>,
    /// Click-to-pick channel (#961): the arm flag this panel's button
    /// toggles, plus any face the last viewport click resolved *for this
    /// node*.
    pub pick: FacePickUi<'a>,
}

/// The Faces panel's half of click-to-pick (#961) — the
/// [`FacePick`](crate::editor_gizmo::FacePick) resource narrowed to what
/// the panel may touch, with the addressing already resolved by the caller.
pub(super) struct FacePickUi<'a> {
    /// Armed by this panel's button, disarmed by the click that resolves a
    /// face (or by pressing the button again).
    pub armed: &'a mut bool,
    /// A face the last scene click resolved on the node being drawn, to
    /// focus exactly once — creating its override first if it is new.
    pub picked: Option<FaceKey>,
}

/// Shared tail for every primitive editor: the UV-mapping picker, solid
/// checkbox, torture triple, collapsible material panel and per-face
/// overrides — the whole [`PrimCommon`] block. Factored out so each
/// per-primitive editor only owns its shape-specific parameter widgets, and
/// so all sixteen kinds — plus the avatar editor, which routes through the
/// same dispatch — get the Faces panel from one place. `show_cuts` gates the
/// topology-cut widgets for kinds whose mesher ignores them (Plane).
fn draw_common_primitive(
    ui: &mut egui::Ui,
    common: &mut PrimCommon,
    faces: FacePanel<'_, '_>,
    salt: &str,
    show_cuts: bool,
    dirty: &mut bool,
) {
    // The family default the picker folds to comes from the snapshot,
    // which is the node itself; a faceless kind never reaches here.
    let family = faces
        .snapshot
        .and_then(GeneratorKind::family_uv_mapping)
        .unwrap_or_default();
    draw_uv_mapping(ui, common, family, salt, dirty);
    if ui.checkbox(&mut common.solid, "Solid (collider)").changed() {
        *dirty = true;
    }
    ui.add_space(2.0);
    draw_torture(ui, &mut common.torture, show_cuts, dirty);
    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_universal_material(ui, &mut common.material, salt, dirty);
        });
    // A new override starts as a copy of the base material, so adding one
    // changes nothing until it is edited (see `draw_face_overrides`).
    let base = common.material.clone();
    draw_face_overrides(ui, &mut common.faces, faces, &base, salt, dirty);
}

/// The per-face override list (#960): the SL "select a face, give it its own
/// texture" model, one collapsible row per overridden face.
///
/// Two behaviours are load-bearing rather than cosmetic:
///
/// * **Dormant overrides stay.** The list is the *record's* overrides, never
///   filtered by `live` — a face the current path-cut removed keeps its row
///   (greyed, still editable) because the record keeps the override and
///   restoring the cut brings it back. Filtering here would quietly teach
///   authors that cutting destroys their work.
/// * **Adding is a no-op.** A fresh override copies the prim's base material
///   and inherits its projection, so it plans into the same material group
///   (`FacePlan::is_whole` stays true) and the prim does not split until the
///   author actually changes something.
///
/// A scene pick (#961) arrives here as `pick.picked`: it adds the face if it
/// is new, opens its row, and forces this whole section open — the picked
/// node is usually one the user has never expanded, so a silently-added row
/// inside a collapsed header would look like nothing happened.
fn draw_face_overrides(
    ui: &mut egui::Ui,
    overrides: &mut Vec<FaceOverride>,
    faces: FacePanel<'_, '_>,
    base: &SovereignMaterialSettings,
    salt: &str,
    dirty: &mut bool,
) {
    let FacePanel {
        snapshot,
        undo_label,
        pick,
    } = faces;
    let FacePickUi {
        armed,
        picked: just_picked,
    } = pick;
    // Consume the pick before the header: a new face has to be in the list
    // *this* draw, or its row can't be opened until the next one.
    if let Some(face) = just_picked
        && !overrides.iter().any(|ov| ov.face == face)
        && overrides.len() < MAX_FACE_OVERRIDES
    {
        overrides.push(new_face_override(face, base));
        undo_label.set("face override");
        *dirty = true;
    }
    let title = if overrides.is_empty() {
        "Faces".to_string()
    } else {
        format!("Faces ({})", overrides.len())
    };
    // Local dirty flag: any edit anywhere in this panel names the undo entry
    // once, instead of every widget repeating the label.
    let mut edited = false;
    egui::CollapsingHeader::new(title)
        .id_salt(format!("{}_faces", salt))
        .default_open(false)
        // A pick opens the section for the frame it lands on; egui keeps it
        // open afterwards, so the override is visible where it was made.
        .open(just_picked.map(|_| true))
        .show(ui, |ui| {
            // Inside the body: `face_census` meshes the prim, so a closed
            // panel must not pay for it, and a selected BlobGroup must not
            // re-run surface nets every frame just for a face list.
            let live = snapshot
                .map(|k| face_census(ui, salt, k))
                .unwrap_or_default();
            // One frame stale after an edit to the whole-prim UV combo above
            // — it only names what "Inherit" resolves to.
            let base_mapping = snapshot.and_then(|k| k.uv_mapping()).unwrap_or_default();
            let weak = crate::ui::theme::current(ui.ctx()).text_weak;
            ui.label(
                egui::RichText::new(
                    "A listed face carries its own complete material — the \
                     Material above no longer paints it. Faces the current \
                     cuts don't produce stay listed but greyed; restoring \
                     the cut brings the override back.",
                )
                .small()
                .color(weak),
            );
            let mut remove: Option<usize> = None;
            for (i, ov) in overrides.iter_mut().enumerate() {
                let dormant = !live.contains(&ov.face);
                // Keyed by face rather than index so removing one row does
                // not hand its open/closed state to its neighbour.
                let id = ui.make_persistent_id((salt, "face_row", ov.face.label()));
                let mut row = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    false,
                );
                // The picked face opens so the click lands on an editor, not
                // on a row the user still has to find and expand. Built here
                // rather than outside the section: the row's id comes from
                // this `Ui`'s id stack, which only exists inside the body.
                if just_picked == Some(ov.face) {
                    row.set_open(true);
                }
                row.show_header(ui, |ui| {
                    let c = ov.material.base_color.0;
                    egui::color_picker::show_color(
                        ui,
                        egui::Rgba::from_rgb(c[0], c[1], c[2]),
                        egui::vec2(14.0, 14.0),
                    )
                    .on_hover_text("This face's base colour");
                    let mut text = egui::RichText::new(ov.face.label());
                    if dormant {
                        text = text.color(weak);
                    }
                    let label = ui.label(text);
                    if dormant {
                        ui.label(egui::RichText::new("(dormant)").small().color(weak))
                            .on_hover_text(
                                "The current cuts don't produce this face, so \
                                 nothing renders with it. The override is kept \
                                 — undo the cut and it paints again.",
                            );
                        label
                            .on_hover_text("Dormant: not emitted by the prim's current cut state.");
                    }
                    if crate::ui::affordances::remove_button(ui, "Drop this face override")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                })
                .body(|ui| {
                    let face_salt = format!("{}_face_{}", salt, i);
                    draw_face_uv_mapping(
                        ui,
                        &mut ov.uv_mapping,
                        base_mapping,
                        &face_salt,
                        &mut edited,
                    );
                    draw_universal_material(ui, &mut ov.material, &face_salt, &mut edited);
                });
            }
            if let Some(i) = remove {
                overrides.remove(i);
                edited = true;
            }
            // Pick from the scene (#961) — the reason the face vocabulary
            // ("Side −X", "Slice start") never has to be decoded against the
            // camera. Sits with the dropdown because they are the two ways
            // to reach the same list.
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(*armed, "Pick from scene")
                    .on_hover_text(
                        "Click a face in the 3D view to give it its own \
                         material. The click also selects whatever prim it \
                         lands on, so this works across the whole room — not \
                         only on the prim shown here.",
                    )
                    .clicked()
                {
                    *armed = !*armed;
                }
                if *armed {
                    ui.label(
                        egui::RichText::new("click a face in the view — or here again to cancel")
                            .small()
                            .color(weak),
                    );
                }
            });
            // The picker offers only faces this prim emits *now* and does not
            // yet override — the sanitizer drops duplicate keys (first wins),
            // so offering one twice would silently discard the second.
            let addable: Vec<FaceKey> = addable_faces(&live, overrides);
            if overrides.len() >= MAX_FACE_OVERRIDES {
                ui.label(
                    egui::RichText::new(format!(
                        "All {MAX_FACE_OVERRIDES} override slots are used."
                    ))
                    .small()
                    .color(weak),
                );
            } else if addable.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Every face this prim currently emits already has an override.",
                    )
                    .small()
                    .color(weak),
                );
            } else {
                egui::ComboBox::from_id_salt((salt, "face_add"))
                    .selected_text("+ Add face")
                    .show_ui(ui, |ui| {
                        for face in addable {
                            if ui.selectable_label(false, face.label()).clicked() {
                                overrides.push(new_face_override(face, base));
                                edited = true;
                            }
                        }
                    });
            }
        });
    if edited {
        undo_label.set("face override");
        *dirty = true;
    }
}

/// A newly picked face's override: the prim's own material, copied whole,
/// with the projection inherited.
///
/// Adding a face must not *change* anything — the author has only said
/// "this face is mine now". Copying the base rather than defaulting is what
/// makes that true: the override plans into the base material's group
/// (`a_no_op_override_does_not_split` in
/// [`world_builder::prim`](crate::world_builder)), so the prim keeps its
/// single mesh and single draw call until an edit actually diverges.
fn new_face_override(face: FaceKey, base: &SovereignMaterialSettings) -> FaceOverride {
    FaceOverride {
        face,
        material: base.clone(),
        uv_mapping: None,
    }
}

/// The faces a prim emits that have no override yet — what the add-picker
/// offers, in mesh-emission order.
fn addable_faces(live: &[FaceKey], overrides: &[FaceOverride]) -> Vec<FaceKey> {
    live.iter()
        .copied()
        .filter(|f| !overrides.iter().any(|ov| ov.face == *f))
        .collect()
}

/// A single face's projection override: [`draw_uv_mapping`]'s modes plus an
/// "Inherit" entry for `None`, which is the default and the reason a plain
/// recolour never re-meshes the prim.
fn draw_face_uv_mapping(
    ui: &mut egui::Ui,
    mapping: &mut Option<UvMapping>,
    base_mapping: UvMapping,
    salt: &str,
    dirty: &mut bool,
) {
    let inherit = format!("Inherit ({})", uv_mode_name(base_mapping));
    ui.horizontal(|ui| {
        ui.label("UV mapping").on_hover_text(
            "How this face's texture is projected. Inherit uses the prim's \
             own projection — the only part of an override that is a delta \
             rather than a complete value.",
        );
        let current = match mapping {
            None => inherit.as_str(),
            Some(m) => uv_mode_name(*m),
        };
        egui::ComboBox::from_id_salt((salt, "face_uv_mapping"))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(mapping.is_none(), &inherit).clicked() && mapping.is_some() {
                    *mapping = None;
                    *dirty = true;
                }
                for (v, n, hint) in UV_MODES {
                    if ui
                        .selectable_label(*mapping == Some(v), n)
                        .on_hover_text(hint)
                        .clicked()
                        && *mapping != Some(v)
                    {
                        *mapping = Some(v);
                        *dirty = true;
                    }
                }
            });
    });
}

/// The faces a primitive currently emits, memoized per node in egui's
/// frame memory.
///
/// [`enumerate_faces`](crate::world_builder::enumerate_faces) answers by
/// *building the mesh* — the only honest answer, since the census depends on
/// the whole torture block — so calling it every frame would re-mesh a
/// 128-segment helix at frame rate while the panel is merely open. The
/// geometry fingerprint (already the primitive mesh cache's key) is a cheap
/// enough per-frame check to gate that on, and it changes exactly when the
/// census can.
///
/// Returns empty for a non-primitive kind, which has no faces to override.
pub(super) fn face_census(ui: &egui::Ui, salt: &str, kind: &GeneratorKind) -> Vec<FaceKey> {
    if kind.faces().is_none() {
        return Vec::new();
    }
    let fingerprint = crate::world_builder::prim_cache::prim_geometry_fingerprint(kind);
    let id = egui::Id::new((salt, "face_census"));
    if let Some((cached, faces)) = ui.data(|d| d.get_temp::<(u64, Vec<FaceKey>)>(id))
        && cached == fingerprint
    {
        return faces;
    }
    let faces = crate::world_builder::enumerate_faces(kind);
    ui.data_mut(|d| d.insert_temp(id, (fingerprint, faces.clone())));
    faces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn override_of(face: FaceKey) -> FaceOverride {
        FaceOverride {
            face,
            material: SovereignMaterialSettings::default(),
            uv_mapping: None,
        }
    }

    /// The picker offers the live faces that are still free, in emission
    /// order — re-offering an overridden face would hand the author a
    /// duplicate the sanitizer then silently drops (first entry wins).
    #[test]
    fn add_picker_offers_only_unclaimed_live_faces() {
        let live = [FaceKey::Wall, FaceKey::Top, FaceKey::Bottom];
        let overrides = [override_of(FaceKey::Top)];
        assert_eq!(
            addable_faces(&live, &overrides),
            vec![FaceKey::Wall, FaceKey::Bottom]
        );
    }

    /// A dormant override — its face is no longer emitted — occupies no
    /// picker slot, because the picker is fed by the *live* faces. The row
    /// itself survives: the panel's list is the record's own `overrides`,
    /// never filtered by the census (#955's dormancy contract).
    #[test]
    fn a_dormant_override_claims_nothing_from_the_picker() {
        let live = [FaceKey::Wall, FaceKey::Top];
        // `PathCutStart` exists only while a path-cut is open.
        let overrides = [override_of(FaceKey::PathCutStart)];
        assert_eq!(
            addable_faces(&live, &overrides),
            vec![FaceKey::Wall, FaceKey::Top]
        );
    }

    /// Adding a face is a no-op until it is edited: the new override carries
    /// a copy of the prim's own material and inherits its projection, which
    /// is exactly the shape `FacePlan` folds back into the base group.
    #[test]
    fn a_new_override_starts_identical_to_the_base_material() {
        let base = SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.4, 0.9]),
            uv_scale: Fp(3.0),
            ..Default::default()
        };
        let ov = new_face_override(FaceKey::Top, &base);
        assert_eq!(ov.face, FaceKey::Top);
        assert_eq!(ov.material, base);
        assert_eq!(
            ov.uv_mapping, None,
            "a fresh override inherits the prim's projection"
        );
    }

    /// Every face key the record can hold has a display name — the panel
    /// lists overrides straight from the record, including keys a future
    /// client invented, so an unnamed one would render as a blank row.
    #[test]
    fn every_listed_face_has_a_label() {
        for tag in ["Cuboid", "Sphere", "Cylinder", "Wedge", "Tetrahedron"] {
            let kind = GeneratorKind::default_primitive_for_tag(tag).unwrap();
            for face in crate::world_builder::enumerate_faces(&kind) {
                assert!(!face.label().is_empty(), "{tag} emits an unnamed face");
            }
        }
        assert!(!FaceKey::Unknown.label().is_empty());
    }
}
