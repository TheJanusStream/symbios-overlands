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
    // README's second sentence tells a new user to edit the terrain
    // (#1233 f264). The path from that promise to a visible change ran
    // through a tab called Region Assets to a tree node to Octaves,
    // Persistence and Lacunarity — fractal-noise vocabulary, with no
    // hover text on any of it — while the friendlier control that already
    // exists sits above and was never pointed at.
    ui.label(
        egui::RichText::new(
            "These shape the landform in detail. For a different landscape entirely, use this window's Random seed row.",
        )
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Grid")
        .default_open(true)
        .show(ui, |ui| {
            drag_u32(ui, "Grid size", &mut cfg.grid_size, 32, 2048, dirty).on_hover_text(
                "How many points the landscape is sampled at. Higher is more detailed and slower to build.",
            );
            fp_slider(ui, "Cell scale", &mut cfg.cell_scale, 0.1, 16.0, dirty).on_hover_text(
                "Metres between those points — how far the land stretches. Bigger means a wider world with the same amount of detail.",
            );
            fp_slider(ui, "Height scale", &mut cfg.height_scale, 1.0, 500.0, dirty)
                .on_hover_text("How tall the hills are. Everything gets steeper together.");
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
                    drag_u32(ui, "Octaves", &mut cfg.octaves, 1, 32, dirty).on_hover_text(
                        "How much fine detail: more octaves adds smaller bumps on top of the big shapes.",
                    );
                    fp_slider(ui, "Persistence", &mut cfg.persistence, 0.0, 1.0, dirty)
                        .on_hover_text(
                            "How strong those small bumps are. Low is smooth and rolling; high is rough and broken.",
                        );
                    fp_slider(ui, "Lacunarity", &mut cfg.lacunarity, 1.0, 4.0, dirty)
                        .on_hover_text(
                            "How much smaller each layer of detail is than the one before it. Higher packs finer detail in faster.",
                        );
                    fp_slider(
                        ui,
                        "Base frequency",
                        &mut cfg.base_frequency,
                        0.1,
                        32.0,
                        dirty,
                    )
                    .on_hover_text(
                        "The size of the big shapes. Low gives a few broad mountains; high gives many small hills.",
                    );
                }
                SovereignGeneratorKind::DiamondSquare => {
                    fp_slider(ui, "Roughness", &mut cfg.ds_roughness, 0.0, 1.0, dirty)
                        .on_hover_text(
                            "How jagged the land is. Low is smooth dunes; high is broken, cliff-like ground.",
                        );
                }
                // No knobs for an algorithm this build has never compiled
                // (#1119) — it has no parameters here to show. The combo
                // above names it; the Kind picker is the way out.
                SovereignGeneratorKind::Unknown => {
                    ui.colored_label(
                        crate::ui::theme::current(ui.ctx()).status.warn,
                        "Terrain algorithm from a newer version of Overlands — \
                         pick a Kind above to replace it.",
                    );
                }
                SovereignGeneratorKind::VoronoiTerracing => {
                    drag_u32(
                        ui,
                        "Num seeds",
                        &mut cfg.voronoi_num_seeds,
                        1,
                        10_000,
                        dirty,
                    )
                    .on_hover_text(
                        "How many plateaus the land is broken into. More seeds means smaller, more crowded ones.",
                    );
                    drag_u32(
                        ui,
                        "Num terraces",
                        &mut cfg.voronoi_num_terraces,
                        1,
                        64,
                        dirty,
                    )
                    .on_hover_text(
                        "How many height steps those plateaus climb through — the number of visible ledges.",
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
            drag_u32(ui, "Drops", &mut cfg.erosion_drops, 0, 500_000, dirty).on_hover_text(
                "How many raindrops run down the land carving it. More is deeper valleys and a longer build.",
            );
            fp_slider(ui, "Inertia", &mut cfg.inertia, 0.0, 1.0, dirty).on_hover_text(
                "How readily a drop turns. Low water follows every dip; high water cuts straight and carves gorges.",
            );
            fp_slider(ui, "Erosion rate", &mut cfg.erosion_rate, 0.0, 1.0, dirty)
                .on_hover_text("How much ground each drop picks up as it runs.");
            fp_slider(
                ui,
                "Deposition rate",
                &mut cfg.deposition_rate,
                0.0,
                1.0,
                dirty,
            )
            .on_hover_text("How readily it drops that ground again, building up flats and deltas.");
            fp_slider(
                ui,
                "Evaporation",
                &mut cfg.evaporation_rate,
                0.0,
                1.0,
                dirty,
            )
            .on_hover_text("How quickly a drop dries up. Higher makes shorter, more local valleys.");
            fp_slider(
                ui,
                "Capacity factor",
                &mut cfg.capacity_factor,
                0.1,
                64.0,
                dirty,
            )
            .on_hover_text("How much ground a drop can carry before it starts dropping it again.");
        });

    egui::CollapsingHeader::new("Thermal Erosion")
        .default_open(false)
        .show(ui, |ui| {
            if ui.checkbox(&mut cfg.thermal_enabled, "Enabled").changed() {
                *dirty = true;
            }
            drag_u32(ui, "Iterations", &mut cfg.thermal_iterations, 0, 500, dirty)
                .on_hover_text("How long loose material is allowed to slide downhill.");
            fp_slider(
                ui,
                "Talus angle",
                &mut cfg.thermal_talus_angle,
                0.0,
                0.5,
                dirty,
            )
            .on_hover_text(
                "The steepest slope that holds. Lower settles the land into gentler scree.",
            );
        });

    egui::CollapsingHeader::new("Material")
        .default_open(false)
        .show(ui, |ui| {
            draw_material_forge(ui, &mut cfg.material, dirty);
        });
}

#[cfg(test)]
mod tests {
    /// THE SEQUENCE (#1233 f264): README's second sentence tells a new user
    /// to edit the terrain. They open the World Editor, find no terrain
    /// tab, eventually find terrain nested inside "Region Assets", and are
    /// shown Persistence, Lacunarity and Base frequency with no idea which
    /// one makes hills — and nothing on the surface pointed at the seed
    /// re-roll, which is the friendlier control for wanting a different
    /// landform rather than a tuned one.
    ///
    /// Source-scanning, because "does every knob explain itself" is a
    /// question about the code: a new algorithm arm added without hover
    /// text would otherwise re-open the gap silently.
    #[test]
    fn every_terrain_knob_says_what_it_does_to_the_landscape() {
        let source = include_str!("terrain.rs");
        let body = source
            .split_once("pub(super) fn draw_terrain_forge(")
            .expect("the forge is in this file")
            .1
            .split_once("\n}\n")
            .expect("a brace-balanced body")
            .0;

        // Every parameter widget is followed by its own explanation. The
        // seed drag is exempt: it is an identifier, not a dial, and the
        // lead-in below already says what re-rolling it does.
        let knobs = body.matches("fp_slider(").count() + body.matches("drag_u32(").count();
        let explained = body.matches(".on_hover_text(").count();
        assert_eq!(
            knobs, explained,
            "{knobs} terrain knobs, {explained} explanations — a slider with \
             no hover text is fractal-noise vocabulary and nothing else"
        );
        assert!(
            knobs >= 9,
            "only {knobs} knobs found — the scan missed some"
        );

        // And the way out for somebody who wants a different landscape
        // rather than a tuned one is named, not left to be discovered.
        assert!(
            body.contains("Random seed row"),
            "nothing points at the re-roll"
        );
    }
}
