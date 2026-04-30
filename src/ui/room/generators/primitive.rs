//! Per-primitive detail editors. Each owns the shape-specific drag
//! widgets, the solid checkbox, the torture triple, and the material
//! panel.

use bevy_egui::egui;

use crate::pds::{Fp, Fp2, Fp3, SovereignMaterialSettings};

use super::super::construct::{draw_torture, draw_universal_material};
use super::super::widgets::{drag_u32, fp_slider};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_primitive_cuboid(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_sphere(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_cylinder(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_capsule(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    length: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_cone(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_torus(
    ui: &mut egui::Ui,
    minor_radius: &mut Fp,
    major_radius: &mut Fp,
    minor_resolution: &mut u32,
    major_resolution: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_plane(
    ui: &mut egui::Ui,
    size: &mut Fp2,
    subdivisions: &mut u32,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
pub(super) fn draw_primitive_tetrahedron(
    ui: &mut egui::Ui,
    size: &mut Fp,
    solid: &mut bool,
    material: &mut SovereignMaterialSettings,
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
    material: &mut SovereignMaterialSettings,
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
