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
    BiomeFilter, Fp, Fp2, Fp3, GeneratorKind, Placement, RoomRecord, ScatterBounds,
    ScatterNaturalness, TransformData, WaterRelation,
};

use super::environment::PlayerPose;
use super::widgets::{drag_u32, drag_u64, draw_transform_no_scale, fp_slider, generator_combo};

/// One-line list/heading label for a placement row.
fn placement_label(index: usize, placement: &Placement) -> String {
    match placement {
        Placement::Absolute { generator_ref, .. } => {
            format!("#{index} Absolute › {generator_ref}")
        }
        Placement::Scatter {
            generator_ref,
            count,
            ..
        } => {
            format!("#{index} Scatter × {count} › {generator_ref}")
        }
        Placement::Grid {
            generator_ref,
            counts,
            ..
        } => {
            format!(
                "#{index} Grid {}x{}x{} › {generator_ref}",
                counts[0], counts[1], counts[2]
            )
        }
        Placement::Unknown => format!("#{index} (unknown)"),
    }
}

/// The Placements list's own state, bundled (#1244 f414 / f415).
///
/// A seeded settlement hands the owner several hundred machine-authored
/// rows on arrival, interleaved with their own, in raw record order with
/// no filter, no sort and no grouping — and the single control that looked
/// like a filter (`draw_biome_filter`) edits a RECORD field on the
/// selected scatter, filtering terrain layers at compile time rather than
/// the list.
pub(super) struct PlacementList<'a> {
    pub filter: &'a mut String,
    pub sort: &'a mut crate::ui::room::PlacementSort,
    /// Rows selected BESIDE the anchor. The anchor stays the gizmo target.
    pub extra: &'a mut Vec<usize>,
    /// The shared confirm the bulk delete parks behind, carrying the rows
    /// it will remove.
    pub bulk_delete: &'a mut crate::ui::confirm::ConfirmState<Vec<usize>>,
}

/// The rows to draw, in display order (#1244 f414).
///
/// Returns record INDICES, never a reordered record: the index is the
/// placement's identity to the gizmo, the visualiser, the detail panel and
/// the delete path, so display order and storage order have to stay
/// separate things. Pure, which is the only way this is testable.
pub(super) fn visible_rows(
    placements: &[Placement],
    filter: &str,
    sort: crate::ui::room::PlacementSort,
) -> Vec<usize> {
    use crate::ui::room::PlacementSort;
    let needle = filter.trim().to_lowercase();
    let mut rows: Vec<usize> = placements
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            // Filter on the row's OWN text — which already embeds the
            // generator name — so "where are this asset's placements" is
            // answered for free.
            needle.is_empty() || placement_label(*i, p).to_lowercase().contains(&needle)
        })
        .map(|(i, _)| i)
        .collect();
    match sort {
        PlacementSort::Order => {}
        PlacementSort::Generator => {
            rows.sort_by_key(|&i| (placement_target(&placements[i]).to_lowercase(), i))
        }
        PlacementSort::Kind => rows.sort_by_key(|&i| (placement_kind_rank(&placements[i]), i)),
    }
    rows
}

/// The anchor plus its sidecar rows, in ascending order (#1244 f415).
fn selected_rows(anchor: Option<usize>, extra: &[usize]) -> Vec<usize> {
    let mut all: Vec<usize> = anchor.into_iter().chain(extra.iter().copied()).collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// Point a placement at a different generator (#1244 f415). `Unknown` is
/// left alone: the editor refuses to write into a schema it cannot read.
fn retarget_placement(placement: &mut Placement, target: &str) {
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => *generator_ref = target.to_string(),
        Placement::Unknown => {}
    }
}

/// The generator a placement points at, or `""` for an unknown variant.
fn placement_target(placement: &Placement) -> &str {
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => generator_ref,
        Placement::Unknown => "",
    }
}

/// Stable rank for the Kind sort: Absolute, Scatter, Grid, Unknown.
fn placement_kind_rank(placement: &Placement) -> u8 {
    match placement {
        Placement::Absolute { .. } => 0,
        Placement::Scatter { .. } => 1,
        Placement::Grid { .. } => 2,
        Placement::Unknown => 3,
    }
}

/// What a click on a placement row does, given the modifiers (#1244 f415).
///
/// Pure so the range arithmetic is testable: the shift-range is taken over
/// the DISPLAY order, not the record order, or a filtered or re-sorted
/// list would select rows the user cannot see.
pub(super) fn click_selection(
    rows: &[usize],
    clicked: usize,
    anchor: Option<usize>,
    shift: bool,
    ctrl: bool,
    extra: &[usize],
) -> (Option<usize>, Vec<usize>) {
    if shift
        && let Some(anchor) = anchor
        && let (Some(from), Some(to)) = (
            rows.iter().position(|&i| i == anchor),
            rows.iter().position(|&i| i == clicked),
        )
    {
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        // The anchor stays the anchor: extending a range must not move the
        // gizmo, or a 40-row shift-click would re-target it 40 times.
        let extra = rows[lo..=hi]
            .iter()
            .copied()
            .filter(|&i| i != anchor)
            .collect();
        return (Some(anchor), extra);
    }
    if ctrl {
        let mut extra = extra.to_vec();
        match anchor {
            Some(a) if a == clicked => {
                // Ctrl-clicking the anchor promotes the next selected row.
                let next = extra.first().copied();
                extra.retain(|&i| Some(i) != next);
                return (next, extra);
            }
            _ => {
                if let Some(pos) = extra.iter().position(|&i| i == clicked) {
                    extra.remove(pos);
                    return (anchor, extra);
                }
                if let Some(a) = anchor {
                    extra.push(a);
                }
                return (Some(clicked), extra);
            }
        }
    }
    (Some(clicked), Vec::new())
}

/// Remove several placements at once, highest index first so the earlier
/// removals cannot shift the later ones (#1244 f415).
pub(super) fn remove_placements(placements: &mut Vec<Placement>, mut rows: Vec<usize>) {
    rows.sort_unstable();
    rows.dedup();
    for index in rows.into_iter().rev() {
        if index < placements.len() {
            placements.remove(index);
        }
    }
}

/// The player's ground position as an anchor for a fresh placement —
/// `[x, z]` when the pose is known, world origin otherwise.
pub(super) fn anchor_xz(player_pose: Option<PlayerPose>) -> [f32; 2] {
    player_pose.map(|p| [p.x, p.z]).unwrap_or([0.0, 0.0])
}

/// Fresh `Absolute` at the player's feet: snapped, so Y is a surface
/// offset and 0 lands it exactly on the ground at the anchor.
pub(super) fn new_absolute_placement(target: String, anchor: [f32; 2]) -> Placement {
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
        float_on_water: false,
        // Flat uniform by default: a fresh scatter should do the plainly
        // predictable thing, and the Naturalness section is right there.
        naturalness: ScatterNaturalness::default(),
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

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_placements_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<usize>,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
    // Undo-entry label channel (#865): add/remove name themselves;
    // detail-panel widget edits fall back to the tab-level label.
    label: &mut crate::ui::undo::LabelSlot,
    // Finding a row, and operating on more than one of them (#1244 f414 /
    // f415): the substring filter, the display order, the extra selected
    // rows, and the confirm the bulk delete parks behind.
    list: PlacementList<'_>,
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

    egui::Panel::left("placements_list_panel")
        .resizable(true)
        .default_size(260.0)
        .min_size(180.0)
        .show(ui, |ui| {
            // Add actions ABOVE the list (#825). Every add stands down at
            // the placement cap with the reason (#1210) — the 1025th used
            // to be pushed, selected, and truncated by the next flush.
            let anchor = anchor_xz(player_pose);
            let cap = crate::ui::room::caps::Cap::Placements;
            let full = cap.is_full(record.placements.len());
            let full_reason = cap.full_reason();
            let (count_text, tone) = cap.readout(record.placements.len());
            ui.label(
                egui::RichText::new(count_text)
                    .small()
                    .color(crate::ui::room::caps::tone_color(ui, tone)),
            );
            ui.horizontal(|ui| {
                // Refused with no region asset to point at (#1239 f71).
                // The cap gate was already here; what was missing is the
                // one its two SIBLINGS have always had — `+ Scatter` and
                // `+ Grid` refuse an ineligible target for exactly this
                // reason. Unconditionally enabled, `+ Absolute` took
                // `all_names.first().unwrap_or_default()` — the EMPTY
                // string — and minted a row reading "#0 Absolute › " that
                // spawns nothing, survives `sanitize` (only Scatter/Grid
                // with ineligible targets are dropped), and is published;
                // its Generator dropdown is empty, so it cannot be
                // repaired either.
                let absolute_refusal = if full {
                    full_reason.as_str()
                } else {
                    "Add a region asset first — a placement has to point at one"
                };
                if ui
                    .add_enabled(
                        !full && !all_names.is_empty(),
                        egui::Button::new("+ Absolute").small(),
                    )
                    .on_hover_text("Add a single placement at your position")
                    .on_disabled_hover_text(absolute_refusal)
                    .clicked()
                {
                    label.set("add of absolute placement");
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
                let scatter_refusal = if full {
                    full_reason.as_str()
                } else {
                    "No scatterable generator in this world yet"
                };
                if ui
                    .add_enabled(
                        has_eligible && !full,
                        egui::Button::new("+ Scatter").small(),
                    )
                    .on_hover_text("Scatter instances in a region centred on you")
                    .on_disabled_hover_text(scatter_refusal)
                    .clicked()
                {
                    label.set("add of scatter placement");
                    record.placements.push(new_scatter_placement(
                        eligible_names.first().cloned().unwrap_or_default(),
                        anchor,
                    ));
                    *selected = Some(record.placements.len() - 1);
                    *dirty = true;
                }
                if ui
                    .add_enabled(has_eligible && !full, egui::Button::new("+ Grid").small())
                    .on_hover_text("Add a grid of instances anchored at your position")
                    .on_disabled_hover_text(scatter_refusal)
                    .clicked()
                {
                    label.set("add of grid placement");
                    record.placements.push(new_grid_placement(
                        eligible_names.first().cloned().unwrap_or_default(),
                        anchor,
                    ));
                    *selected = Some(record.placements.len() - 1);
                    *dirty = true;
                }
            });
            ui.separator();

            // Find a row (#1244 f414). The filter runs over the row's own
            // label, which already embeds the generator name.
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(list.filter)
                        .desired_width(120.0)
                        .hint_text("Filter…"),
                )
                .on_hover_text("Show only rows whose label contains this text");
                egui::ComboBox::from_id_salt("placement_sort")
                    .selected_text(list.sort.label())
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for option in crate::ui::room::PlacementSort::ALL {
                            ui.selectable_value(list.sort, option, option.label());
                        }
                    });
            });

            let rows = visible_rows(&record.placements, list.filter, *list.sort);
            if !list.filter.trim().is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "{} of {} rows",
                        rows.len(),
                        record.placements.len()
                    ))
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                );
            }

            // Bulk actions (#1244 f415). Rendered only with more than one
            // row selected — a single selection has its own per-row
            // controls and the detail panel beside it.
            let mut selection = selected_rows(*selected, list.extra);
            let mut retarget: Option<String> = None;
            if selection.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Selected ({})", selection.len())).strong(),
                    );
                    if ui
                        .add(crate::ui::confirm::danger_button(
                            "Delete",
                            &crate::ui::theme::current(ui.ctx()),
                        ))
                        .clicked()
                    {
                        list.bulk_delete.request(
                            "Delete these placements?",
                            format!(
                                "{} placements will be removed from this world. \
                                 The region assets they point at are kept.",
                                selection.len()
                            ),
                            "Delete",
                            selection.clone(),
                        );
                    }
                    ui.label("Retarget to:");
                    egui::ComboBox::from_id_salt("placement_retarget")
                        .selected_text("Pick an asset…")
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for name in &all_names {
                                if ui.button(name).clicked() {
                                    retarget = Some(name.clone());
                                }
                            }
                        });
                });
                ui.separator();
            }

            let mut to_remove: Option<usize> = None;
            let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
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
                    } else if rows.is_empty() {
                        ui.label(
                            egui::RichText::new("(no rows match the filter)")
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                    }
                    for i in &rows {
                        let i = *i;
                        let p = &record.placements[i];
                        ui.horizontal(|ui| {
                            let picked = *selected == Some(i) || list.extra.contains(&i);
                            if ui
                                .selectable_label(picked, {
                                    let label = placement_label(i, p);
                                    // Carries an authored generator name
                                    // (#1262 f359); the room record is not
                                    // one of the detector's ECS arms.
                                    crate::ui::fonts::note_drawn_text(ui.ctx(), &label);
                                    label
                                })
                                .on_hover_text(
                                    "Shift-click to extend the selection · Ctrl-click to \
                                     add or remove one row",
                                )
                                .clicked()
                            {
                                let (anchor, extra) =
                                    click_selection(&rows, i, *selected, shift, ctrl, list.extra);
                                *selected = anchor;
                                *list.extra = extra;
                            }
                            if crate::ui::affordances::remove_button(ui, "Delete this placement")
                                .clicked()
                            {
                                to_remove = Some(i);
                            }
                        });
                    }
                });

            if let Some(target) = retarget {
                selection = selected_rows(*selected, list.extra);
                for &index in &selection {
                    if let Some(p) = record.placements.get_mut(index) {
                        retarget_placement(p, &target);
                    }
                }
                label.set(format!(
                    "retarget of {} placements to {target}",
                    selection.len()
                ));
                *dirty = true;
            }
            if let Some(rows) = list.bulk_delete.show(ui.ctx(), "placements-bulk") {
                label.set(format!("remove of {} placements", rows.len()));
                remove_placements(&mut record.placements, rows);
                // Every remaining index above a removal shifted; nothing
                // survives the renumbering meaningfully, so the selection
                // goes rather than pointing somewhere arbitrary.
                *selected = None;
                list.extra.clear();
                *dirty = true;
            }
            if let Some(idx) = to_remove {
                label.set(format!(
                    "remove of {}",
                    placement_label(idx, &record.placements[idx])
                ));
                record.placements.remove(idx);
                // Indices above the removal shifted down — keep the same
                // ROW selected where possible, clear if it was the one
                // removed.
                *selected = match *selected {
                    Some(s) if s == idx => None,
                    Some(s) if s > idx => Some(s - 1),
                    other => other,
                };
                // The sidecar selection is renumbered by the same rule.
                list.extra.retain(|&s| s != idx);
                for s in list.extra.iter_mut() {
                    if *s > idx {
                        *s -= 1;
                    }
                }
                *dirty = true;
            }
        });

    egui::CentralPanel::default().show(ui, |ui| {
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
            if snap_toggle(
                ui,
                snap_to_terrain,
                heightmap,
                "Snapped: the anchor sits ON the terrain, and Y is an \
                 offset from that surface (drag the gizmo vertically or \
                 edit Y to float/sink it). Turning snap ON drops the \
                 object onto the surface; turning it OFF keeps it where \
                 it is (Y becomes absolute).",
            ) {
                // Compile semantics for Absolute: snapped world Y =
                // terrain(x, z) + authored Y; unsnapped world Y =
                // authored Y.
                if *snap_to_terrain {
                    // ON: drop onto the surface — zero the offset (#701).
                    transform.translation.0[1] = 0.0;
                } else if let Some(hm) = heightmap {
                    // OFF: stay in place — bake the ground height into the
                    // now-absolute Y (#700). Read through the shared
                    // resolver so the object does not move when the flag
                    // flips: a seeded structure is rendered at its
                    // footprint's high point, not its centre (#1008/#1011).
                    transform.translation.0[1] += crate::world_builder::snapped_ground_y(
                        &hm.0,
                        transform.translation.0[0],
                        transform.translation.0[2],
                        // `avoid_water` is untouched by this toggle, so the
                        // radius reads exactly as it did while snapped.
                        crate::world_builder::snap_radius_of(
                            *avoid_water,
                            avoid_water_clearance.0,
                            transform.scale.0[0],
                        ),
                    );
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
                        .add(crate::ui::num::drag(&mut avoid_water_clearance.0).range(0.0..=100.0))
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
            float_on_water,
            naturalness,
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
            if ui
                .checkbox(float_on_water, "Float on water")
                .on_hover_text(
                    "Spawn instances at the water surface instead of on the \
                     terrain under it — floating cover like lily pads. \
                     Instances on dry ground keep their terrain height.",
                )
                .changed()
            {
                *dirty = true;
            }
            drag_u32(ui, "Count", count, 0, 100_000, dirty);
            drag_u64(ui, "Seed", local_seed, dirty);
            draw_scatter_bounds(ui, bounds, dirty);
            draw_biome_filter(ui, biome_filter, dirty);
            draw_naturalness(ui, naturalness, dirty);
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
            if snap_toggle(
                ui,
                snap_to_terrain,
                heightmap,
                "Snapped: the grid anchor sits at the terrain height under \
                 it (its Y is ignored). Toggling writes that height into Y \
                 so the grid stays where it is.",
            ) {
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
                    .add(crate::ui::num::drag(&mut counts[0]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(crate::ui::num::drag(&mut counts[1]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(crate::ui::num::drag(&mut counts[2]).speed(1).range(1..=100))
                    .changed()
                {
                    *dirty = true;
                }
            });

            ui.label("Grid Gaps (X, Y, Z)");
            ui.horizontal(|ui| {
                if ui
                    .add(
                        crate::ui::num::drag(&mut gaps.0[0])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        crate::ui::num::drag(&mut gaps.0[1])
                            .speed(0.1)
                            .range(0.01..=100.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        crate::ui::num::drag(&mut gaps.0[2])
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

/// The "Snap to Terrain" checkbox, refused with a reason while there is no
/// heightmap to snap against (#1238 f60).
///
/// Both snap rebases depend on `FinishedHeightMap`, and the resource is
/// REMOVED for the whole duration of a terrain regeneration — i.e. exactly
/// after any terrain edit, which is the most likely moment to be
/// re-seating placements. Without it, un-snapping an `Absolute` left the
/// authored offset (usually 0) as an absolute world Y and the object
/// dropped to sea level, while `Grid` left its anchor Y stale in the other
/// direction; `*dirty` fired either way, so the wrong pose was committed
/// and broadcast. The checkbox's own hover text promised the opposite —
/// "turning it OFF keeps it where it is".
///
/// Returns true on the frame the value changed, so the callers keep their
/// existing `if … { rebase }` shape.
fn snap_toggle(
    ui: &mut egui::Ui,
    snap_to_terrain: &mut bool,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    hover: &str,
) -> bool {
    let mut value = *snap_to_terrain;
    let response = ui
        .add_enabled(
            heightmap.is_some(),
            egui::Checkbox::new(&mut value, "Snap to Terrain"),
        )
        .on_hover_text(hover)
        .on_disabled_hover_text(
            "Waiting for the terrain rebuild — snapping needs a heightmap, and \
             toggling without one would move this object.",
        );
    if response.changed() {
        *snap_to_terrain = value;
        return true;
    }
    false
}

fn draw_scatter_bounds(ui: &mut egui::Ui, bounds: &mut ScatterBounds, dirty: &mut bool) {
    ui.label("Bounds");
    let is_circle = matches!(bounds, ScatterBounds::Circle { .. });
    let mut circle = is_circle;
    // A shape change is not a move (#1238 f64). Both arms used to build
    // the new bounds with a hard-coded `center: [0, 0]`, so squaring off a
    // scatter placed 400 m from spawn teleported the whole stand to the
    // world origin — and the coordinates it had were gone from the UI, so
    // recovery meant noticing and undoing. Radius↔extents carries too: a
    // circle's radius becomes the rect's half-extents and back, so the
    // patch keeps roughly the ground it covered.
    let (center, span) = match bounds {
        ScatterBounds::Circle { center, radius } => (*center, Fp2([radius.0, radius.0])),
        ScatterBounds::Rect {
            center, extents, ..
        } => (*center, *extents),
    };
    if ui.radio_value(&mut circle, true, "Circle").clicked() && !is_circle {
        *bounds = ScatterBounds::Circle {
            center,
            radius: Fp(((span.0[0] + span.0[1]) * 0.5).clamp(1.0, 1024.0)),
        };
        *dirty = true;
    }
    if ui.radio_value(&mut circle, false, "Rect").clicked() && is_circle {
        *bounds = ScatterBounds::Rect {
            center,
            extents: span,
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
                        .add(crate::ui::num::drag(v).speed(1.0).range(0.0..=4096.0))
                        .changed()
                    {
                        *dirty = true;
                    }
                }
            });
            *extents = Fp2(e);

            let mut deg = rotation.0.to_degrees();
            if ui
                .add(crate::ui::num::slider(&mut deg, -180.0..=180.0).text("Rotation (deg)"))
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
        ui.label("Centre X / Z");
        for v in center.0.iter_mut() {
            if ui
                .add(crate::ui::num::drag(v).speed(1.0))
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

/// Placement-naturalness section (#912) — the dials that turn a uniform
/// sprinkle into something that reads as grown.
///
/// Every one of these is safe to drag mid-session: none of them consumes a
/// draw from the placement RNG, so moving a slider re-poses the instances
/// without relocating them, and the slope cutoff only ever removes
/// instances rather than reshuffling the stand. That is the reason the
/// hover texts can promise what they promise.
fn draw_naturalness(ui: &mut egui::Ui, n: &mut ScatterNaturalness, dirty: &mut bool) {
    ui.separator();
    ui.label("Naturalness");

    // `fp_slider` hands back no response, so these spell the slider out to
    // hang a hover text off it — the knobs are not self-explanatory from
    // their labels alone.
    let mut slider = |label: &str, value: &mut Fp, hi: f32, hint: &str| {
        let mut v = value.0;
        if ui
            .add(crate::ui::num::slider(&mut v, 0.0..=hi).text(label))
            .on_hover_text(hint)
            .changed()
        {
            *value = Fp(v);
            *dirty = true;
        }
    };
    slider(
        "Clumping",
        &mut n.clumping,
        0.95,
        "Pulls instances toward seeded cluster centres, so the stand grows \
         in patches with clearings between them. 0 is a flat sprinkle.",
    );
    slider(
        "Edge falloff",
        &mut n.edge_falloff,
        4.0,
        "Thins the stand toward its boundary instead of ending in a mown \
         circular edge. Higher values concentrate on the middle.",
    );
    slider(
        "Scale jitter",
        &mut n.scale_jitter,
        0.6,
        "Per-instance uniform scale spread, as a half-width in log space: \
         0.18 gives roughly 0.84×–1.20×. The mesh stays shared.",
    );
    slider(
        "Tilt jitter (rad)",
        &mut n.tilt_jitter,
        0.6,
        "Per-instance lean off vertical, in a random direction. 0.12 ≈ 7°, \
         about right for ground cover; trees want far less.",
    );

    // Microbiome bands (#913). Each is a two-ended range, so they get a
    // checkbox plus a pair of drags rather than a slider.
    let mut band = |label: &str,
                    hint: &str,
                    value: &mut Option<Fp2>,
                    default: [f32; 2],
                    range: std::ops::RangeInclusive<f32>| {
        let mut on = value.is_some();
        if ui.checkbox(&mut on, label).on_hover_text(hint).changed() {
            *value = on.then_some(Fp2(default));
            *dirty = true;
        }
        if let Some(Fp2([lo, hi])) = value {
            ui.horizontal(|ui| {
                ui.label("    min / max");
                for v in [lo, hi] {
                    if ui
                        .add(crate::ui::num::drag(v).speed(0.5).range(range.clone()))
                        .changed()
                    {
                        *dirty = true;
                    }
                }
            });
        }
    };
    band(
        "Limit by height above water",
        "Reject samples outside this band above the room's water line — the \
         moisture proxy. [0, 4] is a riparian shoreline band. Needs a water \
         generator; without one the scatter places nothing.",
        &mut n.above_water_band,
        [0.0, 6.0],
        -100.0..=1000.0,
    );
    band(
        "Limit by altitude",
        "Reject samples outside this world-Y band — a treeline, or an alpine \
         floor. Absolute metres, so it depends on the terrain's height scale.",
        &mut n.altitude_band,
        [0.0, 100.0],
        -1000.0..=1000.0,
    );

    let mut limited = n.max_slope_deg.is_some();
    if ui
        .checkbox(&mut limited, "Limit by slope")
        .on_hover_text(
            "Reject samples on ground steeper than the limit. Needs a \
             heightmap — a scatter with this on places nothing without one.",
        )
        .changed()
    {
        // 30° is where a canopy tree stops being plausible, which is the
        // most common reason to reach for this.
        n.max_slope_deg = limited.then_some(Fp(30.0));
        *dirty = true;
    }
    if let Some(deg) = &mut n.max_slope_deg {
        fp_slider(ui, "Max slope (°)", deg, 0.0, 90.0, dirty);
    }
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

    /// #1238 f64. Sequence: place a scatter around a clearing 400 m from
    /// spawn, then switch its bounds from Circle to Rect to square it off
    /// — the whole stand teleports to the world origin, and the
    /// coordinates it had are gone from the UI. Both arms built the new
    /// bounds with a hard-coded `center: [0, 0]`, three lines from
    /// `new_scatter_placement`, which uses the very same centre-binding
    /// pattern to preserve one.
    ///
    /// Exercises the pure half of the swap — the same expression the radio
    /// arms read — rather than driving egui radio buttons.
    #[test]
    fn a_bounds_shape_change_keeps_the_patch_where_it_is() {
        fn carry(bounds: &ScatterBounds) -> (Fp2, Fp2) {
            match bounds {
                ScatterBounds::Circle { center, radius } => (*center, Fp2([radius.0, radius.0])),
                ScatterBounds::Rect {
                    center, extents, ..
                } => (*center, *extents),
            }
        }
        let circle = ScatterBounds::Circle {
            center: Fp2([-400.0, 120.0]),
            radius: Fp(30.0),
        };
        let (center, span) = carry(&circle);
        assert_eq!(center.0, [-400.0, 120.0], "a shape change is not a move");
        assert_eq!(span.0, [30.0, 30.0], "and the patch keeps its ground");

        let rect = ScatterBounds::Rect {
            center: Fp2([12.0, -8.0]),
            extents: Fp2([50.0, 10.0]),
            rotation: Fp(30.0),
        };
        let (center, span) = carry(&rect);
        assert_eq!(center.0, [12.0, -8.0]);
        let radius = ((span.0[0] + span.0[1]) * 0.5).clamp(1.0, 1024.0);
        assert!((radius - 30.0).abs() < 1e-6, "{radius}");
    }

    fn abs(target: &str) -> Placement {
        new_absolute_placement(target.to_string(), [0.0, 0.0])
    }

    /// #1244 f414. Sequence: a seeded settlement hands the owner several
    /// hundred machine-authored placements on arrival, interleaved with
    /// their own, in raw record order with no filter, no sort and no
    /// grouping — and the only control that looked like a filter
    /// (`draw_biome_filter`) edits a RECORD field on the selected scatter.
    /// The filter runs over the row's own label, which already embeds the
    /// generator name, so "where are this asset's placements" is answered
    /// for free.
    #[test]
    fn the_list_can_be_filtered_and_reordered_without_touching_the_record() {
        use crate::ui::room::PlacementSort;
        let placements = vec![
            abs("oak_17"),
            new_scatter_placement("birch".into(), [0.0, 0.0]),
            abs("oak_2"),
            new_grid_placement("aspen".into(), [0.0, 0.0]),
        ];
        // Unfiltered, unsorted: every row, in record order.
        assert_eq!(
            visible_rows(&placements, "", PlacementSort::Order),
            vec![0, 1, 2, 3]
        );
        // Substring, case-insensitively, over the row label.
        assert_eq!(
            visible_rows(&placements, "OAK", PlacementSort::Order),
            vec![0, 2]
        );
        assert!(visible_rows(&placements, "nothing", PlacementSort::Order).is_empty());
        // Grouped by target — the question the finding leads with.
        assert_eq!(
            visible_rows(&placements, "", PlacementSort::Generator),
            vec![3, 1, 0, 2]
        );
        // …and by kind: Absolute, Scatter, Grid.
        assert_eq!(
            visible_rows(&placements, "", PlacementSort::Kind),
            vec![0, 2, 1, 3]
        );
        // The rows are INDICES into the untouched record: the index is the
        // placement's identity to the gizmo, the visualiser and the
        // delete path, so display order and storage order stay separate.
        assert_eq!(placements.len(), 4);
    }

    /// #1244 f415. Sequence: select forty placements and delete them. The
    /// selection was a single `Option<usize>` and every removal was one
    /// red "−" per row, so a 200-row cleanup was 200 clicks. The ANCHOR
    /// never moves while a range is extended — otherwise a 40-row
    /// shift-click would re-target the gizmo forty times.
    #[test]
    fn shift_and_ctrl_click_build_a_selection_without_moving_the_anchor() {
        let rows = vec![0, 1, 2, 3, 4];
        // A plain click replaces everything.
        let (anchor, extra) = click_selection(&rows, 2, Some(0), false, false, &[1]);
        assert_eq!(anchor, Some(2));
        assert!(extra.is_empty());

        // Shift extends from the anchor, in DISPLAY order, and the anchor
        // stays where it was.
        let (anchor, mut extra) = click_selection(&rows, 4, Some(1), true, false, &[]);
        assert_eq!(anchor, Some(1));
        extra.sort_unstable();
        assert_eq!(extra, vec![2, 3, 4]);

        // …including backwards.
        let (anchor, mut extra) = click_selection(&rows, 0, Some(3), true, false, &[]);
        assert_eq!(anchor, Some(3));
        extra.sort_unstable();
        assert_eq!(extra, vec![0, 1, 2]);

        // Ctrl adds one row, and Ctrl again on the same row removes it.
        let (anchor, extra) = click_selection(&rows, 4, Some(1), false, true, &[]);
        assert_eq!(anchor, Some(4));
        assert_eq!(extra, vec![1]);
        let (anchor, extra) = click_selection(&rows, 1, Some(4), false, true, &[1]);
        assert_eq!(anchor, Some(4));
        assert!(
            extra.is_empty(),
            "ctrl-clicking a selected row deselects it"
        );
    }

    /// The shift-range follows the DISPLAY order, not the record order —
    /// or a filtered or re-sorted list would select rows the user cannot
    /// see (#1244 f414 + f415 meeting).
    #[test]
    fn a_range_over_a_reordered_list_selects_only_visible_rows() {
        let rows = vec![7, 2, 9];
        let (anchor, mut extra) = click_selection(&rows, 9, Some(7), true, false, &[]);
        assert_eq!(anchor, Some(7));
        extra.sort_unstable();
        assert_eq!(extra, vec![2, 9], "the hidden rows between them stay out");
    }

    /// #1244 f415. Removing several rows at once must not let an earlier
    /// removal shift a later index — the classic way a bulk delete takes
    /// the wrong things with it.
    #[test]
    fn a_bulk_delete_removes_exactly_the_chosen_rows() {
        let mut placements = vec![abs("a"), abs("b"), abs("c"), abs("d"), abs("e")];
        remove_placements(&mut placements, vec![1, 3, 3]);
        let names: Vec<&str> = placements.iter().map(placement_target).collect();
        assert_eq!(names, vec!["a", "c", "e"]);
        // An out-of-range index is ignored rather than panicking.
        remove_placements(&mut placements, vec![99]);
        assert_eq!(placements.len(), 3);
    }

    /// #1244 f415. Retargeting after an asset revision is the commonest
    /// large-world edit and had no supported route at all short of the
    /// Raw JSON tab. `Unknown` is left alone — the editor refuses to write
    /// into a schema it cannot read.
    #[test]
    fn retarget_rewrites_every_known_variant_and_skips_the_unknown() {
        let mut rows = [
            abs("shed"),
            new_scatter_placement("shed".into(), [0.0, 0.0]),
            new_grid_placement("shed".into(), [0.0, 0.0]),
            Placement::Unknown,
        ];
        for p in rows.iter_mut() {
            retarget_placement(p, "shed_v2");
        }
        assert_eq!(placement_target(&rows[0]), "shed_v2");
        assert_eq!(placement_target(&rows[1]), "shed_v2");
        assert_eq!(placement_target(&rows[2]), "shed_v2");
        assert!(matches!(rows[3], Placement::Unknown));
    }
}
