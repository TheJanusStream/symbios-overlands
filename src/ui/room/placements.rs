//! Placements tab — master list + detail editor for `Absolute`, `Scatter`,
//! and `Grid` placements, plus the `ScatterBounds` and `BiomeFilter`
//! sub-widgets.

use bevy_egui::egui;

use crate::pds::{
    BiomeFilter, Fp, Fp2, Fp3, GeneratorKind, Placement, RoomRecord, ScatterBounds, TransformData,
    WaterRelation,
};

use super::widgets::{drag_u32, drag_u64, draw_transform_no_scale, fp_slider, generator_combo};

pub(super) fn draw_placements_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<usize>,
    dirty: &mut bool,
) {
    // Single-column master/detail — see `draw_generators_tab` for the
    // rationale; logic mirrors it with index-based selection.
    let selected_exists = selected.is_some_and(|i| i < record.placements.len());

    let all_names: Vec<String> = record.generators.keys().cloned().collect();
    // Targets valid for Scatter/Grid: any root generator that is neither
    // a Terrain (unique by design — duplicating it would spawn
    // conflicting heightfield colliders) nor a Water (water is
    // child-only, so it can never legally be a root). Absolute is
    // unrestricted.
    let eligible_names: Vec<String> = record
        .generators
        .iter()
        .filter(|(_, g)| {
            !matches!(
                g.kind,
                GeneratorKind::Terrain(_) | GeneratorKind::Water { .. }
            )
        })
        .map(|(name, _)| name.clone())
        .collect();

    if selected_exists {
        let idx = selected.expect("selected_exists implies Some");
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                *selected = None;
            }
            ui.heading(format!("Detail — #{idx}"));
        });
        ui.add_space(4.0);
        if let Some(p) = record.placements.get_mut(idx) {
            draw_placement_detail(ui, p, &all_names, &eligible_names, dirty);
        }
        return;
    }

    *selected = None;

    ui.heading("Placements");
    ui.add_space(4.0);

    let mut to_remove: Option<usize> = None;
    for (i, p) in record.placements.iter().enumerate() {
        let label = match p {
            Placement::Absolute { generator_ref, .. } => {
                format!("#{i} Absolute → {generator_ref}")
            }
            Placement::Scatter {
                generator_ref,
                count,
                ..
            } => {
                format!("#{i} Scatter × {count} → {generator_ref}")
            }
            Placement::Grid {
                generator_ref,
                counts,
                ..
            } => {
                format!(
                    "#{i} Grid {}x{}x{} → {generator_ref}",
                    counts[0], counts[1], counts[2]
                )
            }
            Placement::Unknown => format!("#{i} (unknown)"),
        };
        ui.horizontal(|ui| {
            if ui.selectable_label(false, label).clicked() {
                *selected = Some(i);
            }
            if ui
                .add(egui::Button::new("−").fill(egui::Color32::from_rgb(180, 50, 50)))
                .clicked()
            {
                to_remove = Some(i);
            }
        });
    }
    if let Some(idx) = to_remove {
        record.placements.remove(idx);
        *dirty = true;
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label("Add placement:");
    ui.horizontal(|ui| {
        if ui.small_button("+ Absolute").clicked() {
            record.placements.push(Placement::Absolute {
                generator_ref: all_names.first().cloned().unwrap_or_default(),
                transform: TransformData::default(),
                snap_to_terrain: true,
            });
            *selected = Some(record.placements.len() - 1);
            *dirty = true;
        }
        // Scatter and Grid require an eligible target — disable the
        // buttons when every generator in the record is a Terrain or
        // Water root, so the user can't seed an immediately-invalid
        // placement that the sanitiser would just drop on next save.
        let has_eligible = !eligible_names.is_empty();
        if ui
            .add_enabled(has_eligible, egui::Button::new("+ Scatter").small())
            .clicked()
        {
            record.placements.push(Placement::Scatter {
                generator_ref: eligible_names.first().cloned().unwrap_or_default(),
                bounds: ScatterBounds::default(),
                count: 16,
                local_seed: 1,
                biome_filter: BiomeFilter::default(),
                snap_to_terrain: true,
                random_yaw: true,
            });
            *selected = Some(record.placements.len() - 1);
            *dirty = true;
        }
        if ui
            .add_enabled(has_eligible, egui::Button::new("+ Grid").small())
            .clicked()
        {
            record.placements.push(Placement::Grid {
                generator_ref: eligible_names.first().cloned().unwrap_or_default(),
                transform: TransformData::default(),
                counts: [2, 1, 2],
                gaps: Fp3([2.0, 2.0, 2.0]),
                snap_to_terrain: true,
                random_yaw: false,
            });
            *selected = Some(record.placements.len() - 1);
            *dirty = true;
        }
    });
}

fn draw_placement_detail(
    ui: &mut egui::Ui,
    placement: &mut Placement,
    all_names: &[String],
    eligible_names: &[String],
    dirty: &mut bool,
) {
    match placement {
        Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain,
        } => {
            generator_combo(ui, "Generator", generator_ref, all_names, dirty);
            if ui.checkbox(snap_to_terrain, "Snap to Terrain").changed() {
                *dirty = true;
            }
            draw_transform_no_scale(ui, transform, dirty);
        }
        Placement::Scatter {
            generator_ref,
            bounds,
            count,
            local_seed,
            biome_filter,
            snap_to_terrain,
            random_yaw,
        } => {
            generator_combo(ui, "Generator", generator_ref, eligible_names, dirty);
            if ui.checkbox(snap_to_terrain, "Snap to Terrain").changed() {
                *dirty = true;
            }
            if ui.checkbox(random_yaw, "Random Yaw").changed() {
                *dirty = true;
            }
            drag_u32(ui, "Count", count, 0, 100_000, dirty);
            drag_u64(ui, "Seed", local_seed, dirty);
            draw_scatter_bounds(ui, bounds, dirty);
            draw_biome_filter(ui, biome_filter, dirty);
        }
        Placement::Grid {
            generator_ref,
            transform,
            counts,
            gaps,
            snap_to_terrain,
            random_yaw,
        } => {
            generator_combo(ui, "Generator", generator_ref, eligible_names, dirty);
            if ui.checkbox(snap_to_terrain, "Snap to Terrain").changed() {
                *dirty = true;
            }
            if ui.checkbox(random_yaw, "Random Yaw").changed() {
                *dirty = true;
            }

            ui.label("Grid Counts (X, Y, Z)");
            ui.horizontal(|ui| {
                if ui
                    .add(egui::DragValue::new(&mut counts[0]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(egui::DragValue::new(&mut counts[1]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(egui::DragValue::new(&mut counts[2]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
            });

            ui.label("Grid Gaps (X, Y, Z)");
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::DragValue::new(&mut gaps.0[0])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut gaps.0[1])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut gaps.0[2])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
            });

            draw_transform_no_scale(ui, transform, dirty);
        }
        Placement::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown placement type — editable only via Raw JSON.",
            );
        }
    }
}

fn draw_scatter_bounds(ui: &mut egui::Ui, bounds: &mut ScatterBounds, dirty: &mut bool) {
    ui.label("Bounds");
    let is_circle = matches!(bounds, ScatterBounds::Circle { .. });
    let mut circle = is_circle;
    if ui.radio_value(&mut circle, true, "Circle").clicked() && !is_circle {
        *bounds = ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(64.0),
        };
        *dirty = true;
    }
    if ui.radio_value(&mut circle, false, "Rect").clicked() && is_circle {
        *bounds = ScatterBounds::Rect {
            center: Fp2([0.0, 0.0]),
            extents: Fp2([64.0, 64.0]),
            rotation: Fp(0.0),
        };
        *dirty = true;
    }
    match bounds {
        ScatterBounds::Circle { center, radius } => {
            ui.label(
                egui::RichText::new(format!(
                    "Center: X {:.1}, Z {:.1} (Use Gizmo to move)",
                    center.0[0], center.0[1]
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            fp_slider(ui, "Radius", radius, 1.0, 1024.0, dirty);
        }
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
            ui.label(
                egui::RichText::new(format!(
                    "Center: X {:.1}, Z {:.1} (Use Gizmo to move)",
                    center.0[0], center.0[1]
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            let mut e = extents.0;
            ui.horizontal(|ui| {
                ui.label("Extents");
                for v in e.iter_mut() {
                    if ui
                        .add(egui::DragValue::new(v).speed(1.0).range(0.0..=4096.0))
                        .changed()
                    {
                        *dirty = true;
                    }
                }
            });
            *extents = Fp2(e);

            let mut deg = rotation.0.to_degrees();
            if ui
                .add(egui::Slider::new(&mut deg, -180.0..=180.0).text("Rotation (deg)"))
                .changed()
            {
                rotation.0 = deg.to_radians();
                *dirty = true;
            }
        }
    }
}

fn draw_biome_filter(ui: &mut egui::Ui, filter: &mut BiomeFilter, dirty: &mut bool) {
    ui.label("Biome filter (allowed layers — none checked = any)");
    let labels = ["Grass", "Dirt", "Rock", "Snow"];
    ui.horizontal(|ui| {
        for (i, label) in labels.iter().enumerate() {
            let id = i as u8;
            let mut on = filter.biomes.contains(&id);
            if ui.checkbox(&mut on, *label).changed() {
                if on {
                    if !filter.biomes.contains(&id) {
                        filter.biomes.push(id);
                        filter.biomes.sort();
                    }
                } else {
                    filter.biomes.retain(|b| *b != id);
                }
                *dirty = true;
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label("Water:");
        let options = [
            (WaterRelation::Both, "Both"),
            (WaterRelation::Above, "Above"),
            (WaterRelation::Below, "Below"),
        ];
        for (value, label) in options {
            if ui.radio_value(&mut filter.water, value, label).changed() {
                *dirty = true;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tab: Raw JSON (fallback + forward-compat escape hatch)
// ---------------------------------------------------------------------------
