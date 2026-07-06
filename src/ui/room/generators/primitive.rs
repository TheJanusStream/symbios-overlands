//! Per-primitive detail editors. Each owns the shape-specific drag
//! widgets, the solid checkbox, the torture triple, and the material
//! panel.

use bevy_egui::egui;

use crate::pds::generator::{BlobElement, BlobShape, LathePoint, SpinePoint};
use crate::pds::sanitize::limits::{MAX_BLOB_ELEMENTS, MAX_SWEEP_POINTS};
use crate::pds::types::Fp4;
use crate::pds::{Fp, Fp2, Fp3, SovereignMaterialSettings, TortureParams};

use super::super::construct::{draw_torture, draw_universal_material};
use super::super::widgets::{drag_u32, fp_slider};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_cuboid(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_sphere(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        drag_u32(ui, "Ico Res", resolution, 0, 6, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_cylinder(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_capsule(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    length: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_cone(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_torus(
    ui: &mut egui::Ui,
    minor_radius: &mut Fp,
    major_radius: &mut Fp,
    minor_resolution: &mut u32,
    major_resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_plane(
    ui: &mut egui::Ui,
    size: &mut Fp2,
    subdivisions: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_tetrahedron(
    ui: &mut egui::Ui,
    size: &mut Fp,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    fp_slider(ui, "Size", size, 0.01, 100.0, dirty);
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_tube(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    inner_radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Outer R", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Inner R", inner_radius, 0.0, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_bevel(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    bevel: &mut Fp,
    bevel_segments: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
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
    ui.horizontal(|ui| {
        fp_slider(ui, "Bevel", bevel, 0.0, 50.0, dirty);
        drag_u32(ui, "Segments", bevel_segments, 1, 16, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_helix(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    tube_radius: &mut Fp,
    pitch: &mut Fp,
    turns: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Tube", tube_radius, 0.01, 50.0, dirty);
    });
    ui.horizontal(|ui| {
        fp_slider(ui, "Pitch", pitch, 0.0, 100.0, dirty);
        fp_slider(ui, "Turns", turns, 0.05, 16.0, dirty);
        drag_u32(ui, "Res/turn", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_superellipsoid(
    ui: &mut egui::Ui,
    half_extents: &mut Fp3,
    exponent_ns: &mut Fp,
    exponent_ew: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_spine(
    ui: &mut egui::Ui,
    points: &mut Vec<SpinePoint>,
    resolution: &mut u32,
    samples_per_segment: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
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
            if ui.button("−").on_hover_text("Remove point").clicked() {
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
    if points.len() < MAX_SWEEP_POINTS && ui.button("+ point").clicked() {
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_lathe(
    ui: &mut egui::Ui,
    points: &mut Vec<LathePoint>,
    resolution: &mut u32,
    smooth: &mut bool,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
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
            if ui.button("−").on_hover_text("Remove station").clicked() {
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
    if points.len() < MAX_SWEEP_POINTS && ui.button("+ station").clicked() {
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
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_blob_group(
    ui: &mut egui::Ui,
    elements: &mut Vec<BlobElement>,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
    // In-scene edit selection (#705): which element carries the 3D gizmo.
    // Mirrors `editor_gizmo::BlobEditContext::selected_element` — a row
    // click here and a proxy click in the scene land in the same slot.
    selected_element: &mut Option<usize>,
) {
    ui.label("Blob elements (evaluated top to bottom):");
    ui.label(
        egui::RichText::new(
            "Click an element's number (or its red/green ghost in the scene) \
             to sculpt it with the gizmo. Esc returns to the whole prim.",
        )
        .small()
        .color(egui::Color32::GRAY),
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
                if ui.button("⧉").on_hover_text("Duplicate").clicked() {
                    duplicate = Some(i);
                }
                if ui.button("−").on_hover_text("Remove").clicked() {
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
                ui.label("Size");
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
            });
            // Orientation as yaw/pitch/roll drags, stored as a quaternion.
            ui.horizontal(|ui| {
                ui.label("  Rot (Y/P/R)");
                let q = bevy::math::Quat::from_array(e.rotation.0);
                let (mut yaw, mut pitch, mut roll) = q.to_euler(bevy::math::EulerRot::YXZ);
                let mut changed = false;
                for a in [&mut yaw, &mut pitch, &mut roll] {
                    changed |= ui
                        .add(egui::DragValue::new(a).speed(0.02).range(-3.15..=3.15))
                        .changed();
                }
                if changed {
                    e.rotation = Fp4(bevy::math::Quat::from_euler(
                        bevy::math::EulerRot::YXZ,
                        yaw,
                        pitch,
                        roll,
                    )
                    .to_array());
                    *dirty = true;
                }
            });
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
    if elements.len() < MAX_BLOB_ELEMENTS && ui.button("+ element").clicked() {
        elements.push(BlobElement::default());
        *dirty = true;
    }
    drag_u32(ui, "Grid res", resolution, 8, 48, dirty);
    draw_common_primitive(ui, solid, material, torture, salt, dirty);
}

/// Shared tail for every primitive editor: solid checkbox, torture triple,
/// collapsible material panel. Factored out so each per-primitive editor
/// only owns its shape-specific parameter widgets.
#[allow(clippy::too_many_arguments)]
fn draw_common_primitive(
    ui: &mut egui::Ui,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
    torture: &mut TortureParams,
    salt: &str,
    dirty: &mut bool,
) {
    if ui.checkbox(solid, "Solid (collider)").changed() {
        *dirty = true;
    }
    ui.add_space(2.0);
    draw_torture(ui, torture, dirty);
    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_universal_material(ui, material, salt, dirty);
        });
}
