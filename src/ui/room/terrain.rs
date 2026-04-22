//! Terrain tab (sub-tab of Generators) — `SovereignTerrainConfig` widgets for
//! algorithm choice, noise octaves, erosion, and texture-size.

use bevy_egui::egui;

use crate::pds::{SovereignGeneratorKind, SovereignTerrainConfig};

use super::material::draw_material_forge;
use super::widgets::{drag_u32, drag_u64, fp_slider, kind_combo};

pub(super) fn draw_terrain_forge(
    ui: &mut egui::Ui,
    cfg: &mut SovereignTerrainConfig,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Grid")
        .default_open(true)
        .show(ui, |ui| {
            drag_u32(ui, "Grid size", &mut cfg.grid_size, 32, 2048, dirty);
            fp_slider(ui, "Cell scale", &mut cfg.cell_scale, 0.1, 16.0, dirty);
            fp_slider(ui, "Height scale", &mut cfg.height_scale, 1.0, 500.0, dirty);
        });

    egui::CollapsingHeader::new("Algorithm")
        .default_open(true)
        .show(ui, |ui| {
            if kind_combo(ui, &mut cfg.generator_kind) {
                *dirty = true;
            }
            drag_u64(ui, "Seed", &mut cfg.seed, dirty);
            match cfg.generator_kind {
                SovereignGeneratorKind::FbmNoise => {
                    drag_u32(ui, "Octaves", &mut cfg.octaves, 1, 32, dirty);
                    fp_slider(ui, "Persistence", &mut cfg.persistence, 0.0, 1.0, dirty);
                    fp_slider(ui, "Lacunarity", &mut cfg.lacunarity, 1.0, 4.0, dirty);
                    fp_slider(
                        ui,
                        "Base frequency",
                        &mut cfg.base_frequency,
                        0.1,
                        32.0,
                        dirty,
                    );
                }
                SovereignGeneratorKind::DiamondSquare => {
                    fp_slider(ui, "Roughness", &mut cfg.ds_roughness, 0.0, 1.0, dirty);
                }
                SovereignGeneratorKind::VoronoiTerracing => {
                    drag_u32(
                        ui,
                        "Num seeds",
                        &mut cfg.voronoi_num_seeds,
                        1,
                        10_000,
                        dirty,
                    );
                    drag_u32(
                        ui,
                        "Num terraces",
                        &mut cfg.voronoi_num_terraces,
                        1,
                        64,
                        dirty,
                    );
                }
            }
        });

    egui::CollapsingHeader::new("Hydraulic Erosion")
        .default_open(false)
        .show(ui, |ui| {
            if ui.checkbox(&mut cfg.erosion_enabled, "Enabled").changed() {
                *dirty = true;
            }
            drag_u32(ui, "Drops", &mut cfg.erosion_drops, 0, 500_000, dirty);
            fp_slider(ui, "Inertia", &mut cfg.inertia, 0.0, 1.0, dirty);
            fp_slider(ui, "Erosion rate", &mut cfg.erosion_rate, 0.0, 1.0, dirty);
            fp_slider(
                ui,
                "Deposition rate",
                &mut cfg.deposition_rate,
                0.0,
                1.0,
                dirty,
            );
            fp_slider(
                ui,
                "Evaporation",
                &mut cfg.evaporation_rate,
                0.0,
                1.0,
                dirty,
            );
            fp_slider(
                ui,
                "Capacity factor",
                &mut cfg.capacity_factor,
                0.1,
                64.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Thermal Erosion")
        .default_open(false)
        .show(ui, |ui| {
            if ui.checkbox(&mut cfg.thermal_enabled, "Enabled").changed() {
                *dirty = true;
            }
            drag_u32(ui, "Iterations", &mut cfg.thermal_iterations, 0, 500, dirty);
            fp_slider(
                ui,
                "Talus angle",
                &mut cfg.thermal_talus_angle,
                0.0,
                0.5,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Material")
        .default_open(false)
        .show(ui, |ui| {
            draw_material_forge(ui, &mut cfg.material, dirty);
        });
}
