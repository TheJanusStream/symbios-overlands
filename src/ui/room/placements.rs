//! Placements tab — persistent master-detail (#825 / W4): a left list
//! panel with the Add actions ABOVE it, and a right detail panel for the
//! selected `Absolute`, `Scatter`, or `Grid` placement, plus the
//! `ScatterBounds` and `BiomeFilter` sub-widgets. Same split-panel
//! layout as the Region Assets tab, so the window's tabs share one
//! navigation model.
//!
//! New placements spawn AT THE PLAYER'S POSITION (snap-to-terrain on, so
//! they land on the ground where the owner is standing) instead of at
//! the world origin — an origin add 500 m away read as "the button is
//! broken".

use bevy_egui::egui;

use crate::pds::{
    BiomeFilter, Fp, Fp2, Fp3, GeneratorKind, Placement, RoomRecord, ScatterBounds, TransformData,
    WaterRelation,
};

use super::environment::PlayerPose;
use super::widgets::{drag_u32, drag_u64, draw_transform_no_scale, fp_slider, generator_combo};

/// One-line list/heading label for a placement row.
fn placement_label(index: usize, placement: &Placement) -> String {
    match placement {
        Placement::Absolute { generator_ref, .. } => {
            format!("#{index} Absolute → {generator_ref}")
        }
        Placement::Scatter {
            generator_ref,
            count,
            ..
        } => {
            format!("#{index} Scatter × {count} → {generator_ref}")
        }
        Placement::Grid {
            generator_ref,
            counts,
            ..
        } => {
            format!(
                "#{index} Grid {}x{}x{} → {generator_ref}",
                counts[0], counts[1], counts[2]
            )
        }
        Placement::Unknown => format!("#{index} (unknown)"),
    }
}

/// The player's ground position as an anchor for a fresh placement —
/// `[x, z]` when the pose is known, world origin otherwise.
fn anchor_xz(player_pose: Option<PlayerPose>) -> [f32; 2] {
    player_pose.map(|p| [p.x, p.z]).unwrap_or([0.0, 0.0])
}

/// Fresh `Absolute` at the player's feet: snapped, so Y is a surface
/// offset and 0 lands it exactly on the ground at the anchor.
fn new_absolute_placement(target: String, anchor: [f32; 2]) -> Placement {
    Placement::Absolute {
        generator_ref: target,
        transform: TransformData {
            translation: Fp3([anchor[0], 0.0, anchor[1]]),
            ..TransformData::default()
        },
        snap_to_terrain: true,
        avoid_water: false,
        avoid_water_clearance: Fp(0.0),
    }
}

/// Fresh `Scatter` whose bounds circle is centred on the anchor.
fn new_scatter_placement(target: String, anchor: [f32; 2]) -> Placement {
    let mut bounds = ScatterBounds::default();
    match &mut bounds {
        ScatterBounds::Circle { center, .. } | ScatterBounds::Rect { center, .. } => {
            *center = Fp2(anchor);
        }
    }
    Placement::Scatter {
        generator_ref: target,
        bounds,
        count: 16,
        local_seed: 1,
        biome_filter: BiomeFilter::default(),
        snap_to_terrain: true,
        random_yaw: true,
        avoid_urban: false,
    }
}

/// Fresh `Grid` anchored at the player's feet (snapped: the compile
/// replaces Y with the terrain height).
fn new_grid_placement(target: String, anchor: [f32; 2]) -> Placement {
    Placement::Grid {
        generator_ref: target,
        transform: TransformData {
            translation: Fp3([anchor[0], 0.0, anchor[1]]),
            ..TransformData::default()
        },
        counts: [2, 1, 2],
        gaps: Fp3([2.0, 2.0, 2.0]),
        snap_to_terrain: true,
        random_yaw: false,
    }
}

pub(super) fn draw_placements_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<usize>,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
) {
    // Sorted: `record.generators` is a HashMap, so unsorted keys would put
    // the combos in nondeterministic hash order (varying between sessions).
    let mut all_names: Vec<String> = record.generators.keys().cloned().collect();
    all_names.sort();
    // Targets valid for Scatter/Grid: any root generator that is neither
    // a Terrain (unique by design — duplicating it would spawn
    // conflicting heightfield colliders) nor a Water (water is
    // child-only, so it can never legally be a root). Absolute is
    // unrestricted.
    let mut eligible_names: Vec<String> = record
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
    eligible_names.sort();

    // Drop a selection whose row vanished (delete, Load-from-PDS shrink).
    if selected.is_some_and(|i| i >= record.placements.len()) {
        *selected = None;
    }

    egui::SidePanel::left("placements_list_panel")
        .resizable(true)
        .default_width(260.0)
        .min_width(180.0)
        .show_inside(ui, |ui| {
            // Add actions ABOVE the list (#825).
            let anchor = anchor_xz(player_pose);
            ui.horizontal(|ui| {
                if ui
                    .small_button("+ Absolute")
                    .on_hover_text("Add a single placement at your position")
                    .clicked()
                {
                    record.placements.push(new_absolute_placement(
                        all_names.first().cloned().unwrap_or_default(),
                        anchor,
                    ));
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
                    .on_hover_text("Scatter instances in a region centred on you")
                    .clicked()
                {
                    record.placements.push(new_scatter_placement(
                        eligible_names.first().cloned().unwrap_or_default(),
                        anchor,
                    ));
                    *selected = Some(record.placements.len() - 1);
                    *dirty = true;
                }
                if ui
                    .add_enabled(has_eligible, egui::Button::new("+ Grid").small())
                    .on_hover_text("Add a grid of instances anchored at your position")
                    .clicked()
                {
                    record.placements.push(new_grid_placement(
                        eligible_names.first().cloned().unwrap_or_default(),
                        anchor,
                    ));
                    *selected = Some(record.placements.len() - 1);
                    *dirty = true;
                }
            });
            ui.separator();

            let mut to_remove: Option<usize> = None;
            egui::ScrollArea::vertical()
                .id_salt("placements_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if record.placements.is_empty() {
                        ui.label(
                            egui::RichText::new("(no placements — click + Absolute above)")
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                    }
                    for (i, p) in record.placements.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(*selected == Some(i), placement_label(i, p))
                                .clicked()
                            {
                                *selected = Some(i);
                            }
                            if ui
                                .add(
                                    egui::Button::new("−")
                                        .fill(crate::ui::theme::current(ui.ctx()).danger_fill),
                                )
                                .clicked()
                            {
                                to_remove = Some(i);
                            }
                        });
                    }
                });
            if let Some(idx) = to_remove {
                record.placements.remove(idx);
                // Indices above the removal shifted down — keep the same
                // ROW selected where possible, clear if it was the one
                // removed.
                *selected = match *selected {
                    Some(s) if s == idx => None,
                    Some(s) if s > idx => Some(s - 1),
                    other => other,
                };
                *dirty = true;
            }
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("placement_detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let Some(idx) = *selected else {
                    ui.label(
                        egui::RichText::new(
                            "Select a placement on the left — or click an object in \
                             the world.",
                        )
                        .small()
                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                    );
                    return;
                };
                let Some(p) = record.placements.get_mut(idx) else {
                    return;
                };
                ui.heading(placement_label(idx, p));
                ui.add_space(4.0);
                draw_placement_detail(ui, p, &all_names, &eligible_names, heightmap, dirty);
            });
    });
}

fn draw_placement_detail(
    ui: &mut egui::Ui,
    placement: &mut Placement,
    all_names: &[String],
    eligible_names: &[String],
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    dirty: &mut bool,
) {
    match placement {
        Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain,
            avoid_water,
            avoid_water_clearance,
        } => {
            generator_combo(ui, "Generator", generator_ref, all_names, dirty);
            if ui
                .checkbox(snap_to_terrain, "Snap to Terrain")
                .on_hover_text(
                    "Snapped: the anchor sits ON the terrain, and Y is an \
                     offset from that surface (drag the gizmo vertically or \
                     edit Y to float/sink it). Turning snap ON drops the \
                     object onto the surface; turning it OFF keeps it where \
                     it is (Y becomes absolute).",
                )
                .changed()
            {
                // Compile semantics for Absolute: snapped world Y =
                // terrain(x, z) + authored Y; unsnapped world Y =
                // authored Y.
                if *snap_to_terrain {
                    // ON: drop onto the surface — zero the offset (#701).
                    transform.translation.0[1] = 0.0;
                } else if let Some(hm) = heightmap {
                    // OFF: stay in place — bake the terrain height into
                    // the now-absolute Y (#700).
                    transform.translation.0[1] +=
                        hm.world_height_at(transform.translation.0[0], transform.translation.0[2]);
                }
                *dirty = true;
            }
            if ui
                .checkbox(avoid_water, "Avoid Water")
                .on_hover_text(
                    "When snapped, slide the anchor along its bearing to the \
                     nearest ground above the room's water line.",
                )
                .changed()
            {
                *dirty = true;
            }
            if *avoid_water {
                ui.horizontal(|ui| {
                    ui.label("Clearance (m)");
                    if ui
                        .add(egui::DragValue::new(&mut avoid_water_clearance.0).range(0.0..=100.0))
                        .on_hover_text(
                            "Dry-land radius the walk must clear — roughly the \
                             structure's footprint radius. 0 checks the centre only.",
                        )
                        .changed()
                    {
                        *dirty = true;
                    }
                });
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
            avoid_urban,
        } => {
            generator_combo(ui, "Generator", generator_ref, eligible_names, dirty);
            if ui.checkbox(snap_to_terrain, "Snap to Terrain").changed() {
                *dirty = true;
            }
            if ui.checkbox(random_yaw, "Random Yaw").changed() {
                *dirty = true;
            }
            if ui
                .checkbox(avoid_urban, "Avoid urban district")
                .on_hover_text(
                    "Skip scatter points inside the road network's district \
                     (keeps wild scatter out of the built-up area).",
                )
                .changed()
            {
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
            if ui
                .checkbox(snap_to_terrain, "Snap to Terrain")
                .on_hover_text(
                    "Snapped: the grid anchor sits at the terrain height under \
                     it (its Y is ignored). Toggling writes that height into Y \
                     so the grid stays where it is.",
                )
                .changed()
            {
                // Compile semantics for Grid REPLACE the anchor Y with the
                // terrain height while snapped, so the stay-in-place rebase
                // is the same in both directions: store the ground height
                // (#700). Turning snap OFF then keeps the grid exactly
                // where it rendered; turning it ON makes the record agree
                // with what the compiler will do anyway.
                if let Some(hm) = heightmap {
                    transform.translation.0[1] =
                        hm.world_height_at(transform.translation.0[0], transform.translation.0[2]);
                }
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
                crate::ui::theme::current(ui.ctx()).status.warn,
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
            scatter_center_row(ui, center, dirty);
            fp_slider(ui, "Radius", radius, 1.0, 1024.0, dirty);
        }
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
            scatter_center_row(ui, center, dirty);
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

/// Numeric X/Z entry for a scatter bounds centre (#825) — precise
/// placement no longer needs the gizmo, which stays available as the
/// coarse channel.
fn scatter_center_row(ui: &mut egui::Ui, center: &mut Fp2, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Center X / Z");
        for v in center.0.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(1.0))
                .on_hover_text("Type exact coordinates — or drag the gizmo in the scene")
                .changed()
            {
                *dirty = true;
            }
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    const POSE: PlayerPose = PlayerPose {
        x: 120.5,
        y: 8.0,
        z: -44.25,
        yaw_deg: 90.0,
    };

    #[test]
    fn anchor_is_the_player_position_or_origin() {
        assert_eq!(anchor_xz(Some(POSE)), [120.5, -44.25]);
        assert_eq!(anchor_xz(None), [0.0, 0.0]);
    }

    #[test]
    fn new_absolute_lands_snapped_at_the_players_feet() {
        let p = new_absolute_placement("tree".into(), anchor_xz(Some(POSE)));
        match p {
            Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                ..
            } => {
                assert_eq!(generator_ref, "tree");
                // Snapped semantics: Y is a surface offset, so 0 puts the
                // anchor exactly ON the ground at the player's X/Z.
                assert_eq!(transform.translation.0, [120.5, 0.0, -44.25]);
                assert!(snap_to_terrain);
            }
            other => panic!("expected Absolute, got {other:?}"),
        }
    }

    #[test]
    fn new_scatter_centres_its_bounds_on_the_player() {
        let p = new_scatter_placement("tree".into(), anchor_xz(Some(POSE)));
        match p {
            Placement::Scatter { bounds, .. } => match bounds {
                ScatterBounds::Circle { center, .. } | ScatterBounds::Rect { center, .. } => {
                    assert_eq!(center.0, [120.5, -44.25]);
                }
            },
            other => panic!("expected Scatter, got {other:?}"),
        }
    }

    #[test]
    fn new_grid_anchors_at_the_player() {
        let p = new_grid_placement("tree".into(), anchor_xz(Some(POSE)));
        match p {
            Placement::Grid {
                transform,
                snap_to_terrain,
                ..
            } => {
                assert_eq!(transform.translation.0, [120.5, 0.0, -44.25]);
                assert!(snap_to_terrain);
            }
            other => panic!("expected Grid, got {other:?}"),
        }
    }
}
