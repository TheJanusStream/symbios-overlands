//! Sovereign room editor — tabbed Master/Detail view.
//!
//! Rendered only when `session.did == current_room.0` (the signed-in user
//! owns the room they are visiting). The editor keeps an in-flight
//! `pending_record` clone so the owner can stage changes across frames
//! without clobbering the live `ResMut<RoomRecord>` that `world_builder`
//! recompiles from; hitting "Apply Local Preview" commits the pending
//! record into the resource, broadcasts a `RoomStateUpdate` to guests, and
//! (when "Publish to Noosphere" is used) pushes it to the owner's PDS via
//! `com.atproto.repo.putRecord`.
//!
//! The editor is intentionally forgiving: any field it doesn't yet expose
//! as a widget still round-trips via the Raw JSON tab, so L-system code,
//! prop mappings, traits, etc. stay editable while the visual UI catches
//! up to the full schema.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::pds::{
    self, Environment, Fp, Fp2, Fp3, Fp4, Fp64, Generator, Placement, RoomRecord, ScatterBounds,
    SovereignGeneratorKind, SovereignGroundConfig, SovereignMaterialConfig,
    SovereignMaterialSettings, SovereignRockConfig, SovereignSplatRule, SovereignTerrainConfig,
    TransformData,
};
use crate::protocol::OverlandsMessage;
use crate::state::CurrentRoomDid;

/// Async task for publishing the room record to the owner's PDS.
#[derive(Component)]
pub struct PublishRoomTask(pub bevy::tasks::Task<Result<(), String>>);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum EditorTab {
    #[default]
    Environment,
    Generators,
    Placements,
    Raw,
}

/// Persistent editor state kept across frames.
#[derive(Default)]
pub struct RoomEditorState {
    /// True after `pending_record` has been initialised from the live
    /// `RoomRecord` resource. Prevents stomping in-flight edits on every
    /// frame.
    initialised: bool,
    /// Clone of the live record that absorbs widget changes. Only written
    /// back to `ResMut<RoomRecord>` when the owner hits Apply.
    pending_record: Option<RoomRecord>,
    selected_tab: EditorTab,
    selected_generator: Option<String>,
    selected_placement: Option<usize>,
    raw_text: String,
    raw_error: Option<String>,
    /// True once the user has changed the pending record relative to the
    /// live one — drives the colour of the Apply/Publish buttons.
    is_dirty: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn room_admin_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<RoomRecord>>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut editor: Local<RoomEditorState>,
) {
    let (Some(session), Some(room_did), Some(record)) = (session, room_did, room_record.as_mut())
    else {
        return;
    };

    // Security gate — only the owner may edit their own room.
    if session.did != room_did.0 {
        return;
    }

    if !editor.initialised {
        editor.pending_record = Some(record.as_ref().clone());
        editor.raw_text = serde_json::to_string_pretty(record.as_ref())
            .unwrap_or_else(|e| format!("// serialize error: {}", e));
        editor.initialised = true;
    }

    // Destructure the Local into independent field borrows so the
    // borrow-checker can see that the tab-body closure and the commit-row
    // closure each touch *disjoint* subsets of the editor state. Without
    // this, re-borrowing `editor` inside nested egui closures trips E0499.
    let RoomEditorState {
        pending_record,
        selected_tab,
        selected_generator,
        selected_placement,
        raw_text,
        raw_error,
        is_dirty,
        ..
    } = &mut *editor;

    let ctx = contexts.ctx_mut().unwrap();

    egui::Window::new("World Editor")
        .collapsible(true)
        .resizable(true)
        .default_width(560.0)
        .default_height(620.0)
        .default_pos([10.0, 220.0])
        .show(ctx, |ui| {
            // Tab bar
            ui.horizontal(|ui| {
                let tabs = [
                    (EditorTab::Environment, "Environment"),
                    (EditorTab::Generators, "Generators"),
                    (EditorTab::Placements, "Placements"),
                    (EditorTab::Raw, "Raw JSON"),
                ];
                for (tab, label) in tabs {
                    if ui.selectable_label(*selected_tab == tab, label).clicked() {
                        *selected_tab = tab;
                    }
                }
            });
            ui.separator();

            // Tab body — bail if pending_record somehow got dropped
            let Some(pending) = pending_record.as_mut() else {
                ui.label("No pending record loaded.");
                return;
            };

            egui::ScrollArea::vertical()
                .max_height(460.0)
                .show(ui, |ui| match *selected_tab {
                    EditorTab::Environment => {
                        draw_environment_tab(ui, &mut pending.environment, is_dirty);
                    }
                    EditorTab::Generators => {
                        draw_generators_tab(ui, pending, selected_generator, is_dirty);
                    }
                    EditorTab::Placements => {
                        draw_placements_tab(ui, pending, selected_placement, is_dirty);
                    }
                    EditorTab::Raw => {
                        draw_raw_tab(ui, raw_text, raw_error, pending, is_dirty);
                    }
                });

            ui.separator();

            // Commit row
            ui.horizontal(|ui| {
                let apply = ui.add_enabled(*is_dirty, egui::Button::new("Apply Local Preview"));
                if apply.clicked() {
                    let new_record = pending.clone();
                    **record = new_record.clone();
                    writer.write(Broadcast {
                        payload: OverlandsMessage::RoomStateUpdate(new_record),
                        channel: ChannelKind::Reliable,
                    });
                    *is_dirty = false;
                    *raw_text = serde_json::to_string_pretty(record.as_ref()).unwrap_or_default();
                    *raw_error = None;
                }

                if ui.button("Publish to Noosphere").clicked() {
                    let new_record = pending.clone();
                    **record = new_record.clone();
                    writer.write(Broadcast {
                        payload: OverlandsMessage::RoomStateUpdate(new_record.clone()),
                        channel: ChannelKind::Reliable,
                    });
                    *is_dirty = false;
                    spawn_publish_task(&mut commands, &session, new_record);
                }

                if ui.button("Revert").clicked() {
                    *pending = record.as_ref().clone();
                    *raw_text = serde_json::to_string_pretty(record.as_ref()).unwrap_or_default();
                    *raw_error = None;
                    *is_dirty = false;
                    *selected_generator = None;
                    *selected_placement = None;
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Tab: Environment
// ---------------------------------------------------------------------------

fn draw_environment_tab(ui: &mut egui::Ui, env: &mut Environment, dirty: &mut bool) {
    ui.heading("Environment");
    ui.add_space(4.0);
    ui.label("Directional sunlight colour — applied to the scene's main light.");
    ui.add_space(4.0);
    let mut rgb = env.sun_color.0;
    if ui.color_edit_button_rgb(&mut rgb).changed() {
        env.sun_color = Fp3(rgb);
        *dirty = true;
    }
}

// ---------------------------------------------------------------------------
// Tab: Generators (master list + detail)
// ---------------------------------------------------------------------------

fn draw_generators_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<String>,
    dirty: &mut bool,
) {
    ui.columns(2, |cols| {
        // Left column — master list
        cols[0].heading("Generators");
        cols[0].add_space(4.0);

        let mut names: Vec<String> = record.generators.keys().cloned().collect();
        names.sort();

        let mut to_remove: Option<String> = None;
        for name in &names {
            let is_selected = selected.as_ref() == Some(name);
            cols[0].horizontal(|ui| {
                if ui.selectable_label(is_selected, name).clicked() {
                    *selected = Some(name.clone());
                }
                if ui.small_button("−").clicked() {
                    to_remove = Some(name.clone());
                }
            });
        }
        if let Some(name) = to_remove {
            record.generators.remove(&name);
            if selected.as_ref() == Some(&name) {
                *selected = None;
            }
            *dirty = true;
        }

        cols[0].add_space(6.0);
        cols[0].separator();
        cols[0].label("Add new generator:");
        cols[0].horizontal(|ui| {
            if ui.small_button("+ Terrain").clicked() {
                let name = unique_key(&record.generators, "terrain");
                record
                    .generators
                    .insert(name.clone(), Generator::Terrain(Default::default()));
                *selected = Some(name);
                *dirty = true;
            }
            if ui.small_button("+ Water").clicked() {
                let name = unique_key(&record.generators, "water");
                record.generators.insert(
                    name.clone(),
                    Generator::Water {
                        level_offset: Fp(0.0),
                    },
                );
                *selected = Some(name);
                *dirty = true;
            }
            if ui.small_button("+ LSystem").clicked() {
                let name = unique_key(&record.generators, "lsystem");
                record
                    .generators
                    .insert(name.clone(), default_lsystem_generator());
                *selected = Some(name);
                *dirty = true;
            }
        });

        // Right column — detail
        cols[1].heading("Detail");
        cols[1].add_space(4.0);
        if let Some(name) = selected.clone() {
            if let Some(g) = record.generators.get_mut(&name) {
                draw_generator_detail(&mut cols[1], &name, g, dirty);
            } else {
                cols[1].label("(Selection no longer exists.)");
            }
        } else {
            cols[1].label("Select a generator on the left to edit it.");
        }
    });
}

fn draw_generator_detail(
    ui: &mut egui::Ui,
    name: &str,
    generator: &mut Generator,
    dirty: &mut bool,
) {
    ui.label(format!("Generator: `{}`", name));
    ui.add_space(4.0);
    match generator {
        Generator::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        Generator::Water { level_offset } => {
            fp_slider(ui, "Level offset", level_offset, -20.0, 20.0, dirty);
        }
        Generator::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_scale,
            mesh_resolution,
            ..
        } => draw_lsystem_forge(
            ui,
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_scale,
            mesh_resolution,
            dirty,
        ),
        Generator::Shape { style, floors } => {
            if ui
                .add(egui::TextEdit::singleline(style).hint_text("style"))
                .changed()
            {
                *dirty = true;
            }
            if ui
                .add(egui::DragValue::new(floors).speed(1.0).range(0..=64))
                .changed()
            {
                *dirty = true;
            }
        }
        Generator::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

fn draw_terrain_forge(ui: &mut egui::Ui, cfg: &mut SovereignTerrainConfig, dirty: &mut bool) {
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

fn draw_material_forge(ui: &mut egui::Ui, mat: &mut SovereignMaterialConfig, dirty: &mut bool) {
    drag_u32(ui, "Texture size", &mut mat.texture_size, 16, 4096, dirty);
    fp_slider(ui, "Tile scale", &mut mat.tile_scale, 1.0, 500.0, dirty);

    let labels = ["Grass", "Dirt", "Rock", "Snow"];
    for (i, rule) in mat.rules.iter_mut().enumerate() {
        egui::CollapsingHeader::new(format!("{} rule", labels[i]))
            .default_open(false)
            .show(ui, |ui| {
                draw_splat_rule(ui, rule, dirty);
            });
    }

    egui::CollapsingHeader::new("Grass texture")
        .default_open(false)
        .show(ui, |ui| draw_ground_config(ui, &mut mat.grass, dirty));
    egui::CollapsingHeader::new("Dirt texture")
        .default_open(false)
        .show(ui, |ui| draw_ground_config(ui, &mut mat.dirt, dirty));
    egui::CollapsingHeader::new("Rock texture")
        .default_open(false)
        .show(ui, |ui| draw_rock_config(ui, &mut mat.rock, dirty));
    egui::CollapsingHeader::new("Snow texture")
        .default_open(false)
        .show(ui, |ui| draw_ground_config(ui, &mut mat.snow, dirty));
}

fn draw_splat_rule(ui: &mut egui::Ui, rule: &mut SovereignSplatRule, dirty: &mut bool) {
    fp_slider(ui, "Height min", &mut rule.height_min, 0.0, 1.0, dirty);
    fp_slider(ui, "Height max", &mut rule.height_max, 0.0, 1.0, dirty);
    fp_slider(ui, "Slope min", &mut rule.slope_min, 0.0, 1.0, dirty);
    fp_slider(ui, "Slope max", &mut rule.slope_max, 0.0, 1.0, dirty);
    fp_slider(ui, "Sharpness", &mut rule.sharpness, 0.05, 8.0, dirty);
}

fn draw_ground_config(ui: &mut egui::Ui, g: &mut SovereignGroundConfig, dirty: &mut bool) {
    drag_u32_wide(ui, "Seed", &mut g.seed, dirty);
    fp64_slider(ui, "Macro scale", &mut g.macro_scale, 0.1, 20.0, dirty);
    drag_u32(ui, "Macro octaves", &mut g.macro_octaves, 1, 12, dirty);
    fp64_slider(ui, "Micro scale", &mut g.micro_scale, 0.1, 40.0, dirty);
    drag_u32(ui, "Micro octaves", &mut g.micro_octaves, 1, 12, dirty);
    fp64_slider(ui, "Micro weight", &mut g.micro_weight, 0.0, 1.0, dirty);
    color_picker(ui, "Dry", &mut g.color_dry, dirty);
    color_picker(ui, "Moist", &mut g.color_moist, dirty);
    fp_slider(
        ui,
        "Normal strength",
        &mut g.normal_strength,
        0.0,
        10.0,
        dirty,
    );
}

fn draw_rock_config(ui: &mut egui::Ui, r: &mut SovereignRockConfig, dirty: &mut bool) {
    drag_u32_wide(ui, "Seed", &mut r.seed, dirty);
    fp64_slider(ui, "Scale", &mut r.scale, 0.1, 20.0, dirty);
    drag_u32(ui, "Octaves", &mut r.octaves, 1, 16, dirty);
    fp64_slider(ui, "Attenuation", &mut r.attenuation, 0.1, 8.0, dirty);
    color_picker(ui, "Light", &mut r.color_light, dirty);
    color_picker(ui, "Dark", &mut r.color_dark, dirty);
    fp_slider(
        ui,
        "Normal strength",
        &mut r.normal_strength,
        0.0,
        10.0,
        dirty,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_lsystem_forge(
    ui: &mut egui::Ui,
    source_code: &mut String,
    finalization_code: &mut String,
    iterations: &mut u32,
    seed: &mut u64,
    angle: &mut Fp,
    step: &mut Fp,
    width: &mut Fp,
    elasticity: &mut Fp,
    tropism: &mut Option<Fp3>,
    materials: &mut std::collections::HashMap<u8, SovereignMaterialSettings>,
    prop_scale: &mut Fp,
    mesh_resolution: &mut u32,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Source code")
        .default_open(true)
        .show(ui, |ui| {
            if ui
                .add(
                    egui::TextEdit::multiline(source_code)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(10)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                *dirty = true;
            }
        });
    egui::CollapsingHeader::new("Finalization code")
        .default_open(false)
        .show(ui, |ui| {
            if ui
                .add(
                    egui::TextEdit::multiline(finalization_code)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                *dirty = true;
            }
        });

    egui::CollapsingHeader::new("Turtle")
        .default_open(true)
        .show(ui, |ui| {
            drag_u32(ui, "Iterations", iterations, 0, 12, dirty);
            drag_u64(ui, "Seed", seed, dirty);
            fp_slider(ui, "Angle (deg)", angle, 0.0, 180.0, dirty);
            fp_slider(ui, "Step", step, 0.0, 10.0, dirty);
            fp_slider(ui, "Width", width, 0.0, 5.0, dirty);
            fp_slider(ui, "Elasticity", elasticity, 0.0, 4.0, dirty);
            fp_slider(ui, "Prop scale", prop_scale, 0.0, 10.0, dirty);
            drag_u32(ui, "Mesh resolution", mesh_resolution, 3, 32, dirty);

            let mut has_tropism = tropism.is_some();
            if ui.checkbox(&mut has_tropism, "Tropism").changed() {
                *tropism = if has_tropism {
                    Some(Fp3([0.0, -1.0, 0.0]))
                } else {
                    None
                };
                *dirty = true;
            }
            if let Some(t) = tropism.as_mut() {
                let mut v = t.0;
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("x");
                    changed |= ui
                        .add(egui::DragValue::new(&mut v[0]).speed(0.05))
                        .changed();
                    ui.label("y");
                    changed |= ui
                        .add(egui::DragValue::new(&mut v[1]).speed(0.05))
                        .changed();
                    ui.label("z");
                    changed |= ui
                        .add(egui::DragValue::new(&mut v[2]).speed(0.05))
                        .changed();
                });
                if changed {
                    *t = Fp3(v);
                    *dirty = true;
                }
            }
        });

    egui::CollapsingHeader::new("Material slots")
        .default_open(false)
        .show(ui, |ui| {
            let mut slot_ids: Vec<u8> = materials.keys().copied().collect();
            slot_ids.sort_unstable();
            let mut to_remove: Option<u8> = None;
            for id in slot_ids {
                let Some(m) = materials.get_mut(&id) else {
                    continue;
                };
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("Slot {}", id));
                        if ui.small_button("−").clicked() {
                            to_remove = Some(id);
                        }
                    });
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
                });
            }
            if let Some(id) = to_remove {
                materials.remove(&id);
                *dirty = true;
            }
            if ui.button("+ Add material slot").clicked() {
                let next = (0u8..=255).find(|k| !materials.contains_key(k));
                if let Some(k) = next {
                    materials.insert(k, SovereignMaterialSettings::default());
                    *dirty = true;
                }
            }
        });
}

// ---------------------------------------------------------------------------
// Tab: Placements
// ---------------------------------------------------------------------------

fn draw_placements_tab(
    ui: &mut egui::Ui,
    record: &mut RoomRecord,
    selected: &mut Option<usize>,
    dirty: &mut bool,
) {
    ui.columns(2, |cols| {
        cols[0].heading("Placements");
        cols[0].add_space(4.0);

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
                } => format!("#{i} Scatter × {count} → {generator_ref}"),
                Placement::Unknown => format!("#{i} (unknown)"),
            };
            let is_selected = *selected == Some(i);
            cols[0].horizontal(|ui| {
                if ui.selectable_label(is_selected, label).clicked() {
                    *selected = Some(i);
                }
                if ui.small_button("−").clicked() {
                    to_remove = Some(i);
                }
            });
        }
        if let Some(idx) = to_remove {
            record.placements.remove(idx);
            if *selected == Some(idx) {
                *selected = None;
            }
            *dirty = true;
        }

        cols[0].add_space(6.0);
        cols[0].separator();
        cols[0].label("Add placement:");
        cols[0].horizontal(|ui| {
            if ui.small_button("+ Absolute").clicked() {
                record.placements.push(Placement::Absolute {
                    generator_ref: record.generators.keys().next().cloned().unwrap_or_default(),
                    transform: TransformData::default(),
                });
                *selected = Some(record.placements.len() - 1);
                *dirty = true;
            }
            if ui.small_button("+ Scatter").clicked() {
                record.placements.push(Placement::Scatter {
                    generator_ref: record.generators.keys().next().cloned().unwrap_or_default(),
                    bounds: ScatterBounds::default(),
                    count: 16,
                    local_seed: 1,
                    biome_filter: None,
                });
                *selected = Some(record.placements.len() - 1);
                *dirty = true;
            }
        });

        cols[1].heading("Detail");
        cols[1].add_space(4.0);
        if let Some(idx) = *selected {
            if let Some(p) = record.placements.get_mut(idx) {
                let gen_names: Vec<String> = record.generators.keys().cloned().collect();
                draw_placement_detail(&mut cols[1], p, &gen_names, dirty);
            } else {
                cols[1].label("(Selection no longer exists.)");
            }
        } else {
            cols[1].label("Select a placement on the left to edit it.");
        }
    });
}

fn draw_placement_detail(
    ui: &mut egui::Ui,
    placement: &mut Placement,
    gen_names: &[String],
    dirty: &mut bool,
) {
    match placement {
        Placement::Absolute {
            generator_ref,
            transform,
        } => {
            generator_combo(ui, "Generator", generator_ref, gen_names, dirty);
            draw_transform(ui, transform, dirty);
        }
        Placement::Scatter {
            generator_ref,
            bounds,
            count,
            local_seed,
            biome_filter,
        } => {
            generator_combo(ui, "Generator", generator_ref, gen_names, dirty);
            drag_u32(ui, "Count", count, 0, 100_000, dirty);
            drag_u64(ui, "Seed", local_seed, dirty);
            draw_scatter_bounds(ui, bounds, dirty);
            draw_biome_filter(ui, biome_filter, dirty);
        }
        Placement::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown placement type — editable only via Raw JSON.",
            );
        }
    }
}

fn draw_transform(ui: &mut egui::Ui, t: &mut TransformData, dirty: &mut bool) {
    ui.label("Translation");
    let mut tr = t.translation.0;
    ui.horizontal(|ui| {
        for v in tr.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.5)).changed() {
                *dirty = true;
            }
        }
    });
    t.translation = Fp3(tr);

    ui.label("Scale");
    let mut sc = t.scale.0;
    ui.horizontal(|ui| {
        for v in sc.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(0.01..=1000.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    t.scale = Fp3(sc);

    ui.label("Rotation (quaternion xyzw)");
    let mut rot = t.rotation.0;
    ui.horizontal(|ui| {
        for v in rot.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.01)).changed() {
                *dirty = true;
            }
        }
    });
    t.rotation = Fp4(rot);
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
        };
        *dirty = true;
    }
    match bounds {
        ScatterBounds::Circle { center, radius } => {
            let mut c = center.0;
            ui.horizontal(|ui| {
                ui.label("Center");
                for v in c.iter_mut() {
                    if ui.add(egui::DragValue::new(v).speed(1.0)).changed() {
                        *dirty = true;
                    }
                }
            });
            *center = Fp2(c);
            fp_slider(ui, "Radius", radius, 1.0, 1024.0, dirty);
        }
        ScatterBounds::Rect { center, extents } => {
            let mut c = center.0;
            ui.horizontal(|ui| {
                ui.label("Center");
                for v in c.iter_mut() {
                    if ui.add(egui::DragValue::new(v).speed(1.0)).changed() {
                        *dirty = true;
                    }
                }
            });
            *center = Fp2(c);
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
        }
    }
}

fn draw_biome_filter(ui: &mut egui::Ui, biome: &mut Option<u8>, dirty: &mut bool) {
    let mut enabled = biome.is_some();
    if ui.checkbox(&mut enabled, "Biome filter").changed() {
        *biome = if enabled { Some(0) } else { None };
        *dirty = true;
    }
    if let Some(idx) = biome.as_mut() {
        let labels = ["Grass", "Dirt", "Rock", "Snow"];
        ui.horizontal(|ui| {
            for (i, label) in labels.iter().enumerate() {
                if ui.radio_value(idx, i as u8, *label).changed() {
                    *dirty = true;
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tab: Raw JSON (fallback + forward-compat escape hatch)
// ---------------------------------------------------------------------------

fn draw_raw_tab(
    ui: &mut egui::Ui,
    text: &mut String,
    error: &mut Option<String>,
    pending: &mut RoomRecord,
    dirty: &mut bool,
) {
    ui.heading("Raw JSON");
    ui.add_space(4.0);
    ui.label("Advanced escape hatch. Parse errors abort the commit.");
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::multiline(text)
            .font(egui::TextStyle::Monospace)
            .code_editor()
            .desired_rows(18)
            .desired_width(f32::INFINITY),
    );
    if let Some(err) = error.as_ref() {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
    }
    ui.horizontal(|ui| {
        if ui.button("Parse into pending record").clicked() {
            match serde_json::from_str::<RoomRecord>(text) {
                Ok(parsed) => {
                    *pending = parsed;
                    *error = None;
                    *dirty = true;
                }
                Err(e) => *error = Some(format!("Invalid JSON schema: {}", e)),
            }
        }
        if ui.button("Refresh from pending").clicked() {
            *text = serde_json::to_string_pretty(pending).unwrap_or_default();
            *error = None;
        }
    });
}

// ---------------------------------------------------------------------------
// Widget helpers
// ---------------------------------------------------------------------------

fn fp_slider(ui: &mut egui::Ui, label: &str, value: &mut Fp, lo: f32, hi: f32, dirty: &mut bool) {
    let mut v = value.0;
    if ui
        .add(egui::Slider::new(&mut v, lo..=hi).text(label))
        .changed()
    {
        *value = Fp(v);
        *dirty = true;
    }
}

fn fp64_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp64,
    lo: f64,
    hi: f64,
    dirty: &mut bool,
) {
    let mut v = value.0;
    if ui
        .add(egui::Slider::new(&mut v, lo..=hi).text(label))
        .changed()
    {
        *value = Fp64(v);
        *dirty = true;
    }
}

fn drag_u32(ui: &mut egui::Ui, label: &str, value: &mut u32, lo: u32, hi: u32, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value).range(lo..=hi)).changed() {
            *dirty = true;
        }
    });
}

fn drag_u32_wide(ui: &mut egui::Ui, label: &str, value: &mut u32, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value)).changed() {
            *dirty = true;
        }
    });
}

fn drag_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value)).changed() {
            *dirty = true;
        }
    });
}

fn color_picker(ui: &mut egui::Ui, label: &str, value: &mut Fp3, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgb = value.0;
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *value = Fp3(rgb);
            *dirty = true;
        }
    });
}

fn kind_combo(ui: &mut egui::Ui, kind: &mut SovereignGeneratorKind) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label("Kind")
        .selected_text(match kind {
            SovereignGeneratorKind::FbmNoise => "FBM Noise",
            SovereignGeneratorKind::DiamondSquare => "Diamond Square",
            SovereignGeneratorKind::VoronoiTerracing => "Voronoi Terracing",
        })
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(kind, SovereignGeneratorKind::FbmNoise, "FBM Noise")
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::DiamondSquare,
                    "Diamond Square",
                )
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::VoronoiTerracing,
                    "Voronoi Terracing",
                )
                .changed();
        });
    changed
}

fn generator_combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    names: &[String],
    dirty: &mut bool,
) {
    egui::ComboBox::from_label(label)
        .selected_text(value.clone())
        .show_ui(ui, |ui| {
            for n in names {
                if ui.selectable_value(value, n.clone(), n).changed() {
                    *dirty = true;
                }
            }
        });
}

fn unique_key<T>(map: &std::collections::HashMap<String, T>, prefix: &str) -> String {
    let mut n = 0;
    loop {
        let key = if n == 0 {
            prefix.to_string()
        } else {
            format!("{prefix}_{n}")
        };
        if !map.contains_key(&key) {
            return key;
        }
        n += 1;
    }
}

fn default_lsystem_generator() -> Generator {
    Generator::LSystem {
        source_code: String::from("omega: F\nF -> F[+F]F[-F]F\n"),
        finalization_code: String::new(),
        iterations: 3,
        seed: 1,
        angle: Fp(22.5),
        step: Fp(1.0),
        width: Fp(0.2),
        elasticity: Fp(0.0),
        tropism: None,
        materials: std::collections::HashMap::new(),
        prop_mappings: std::collections::HashMap::new(),
        prop_scale: Fp(1.0),
        mesh_resolution: 8,
    }
}

// ---------------------------------------------------------------------------
// Publish pipeline
// ---------------------------------------------------------------------------

fn spawn_publish_task(commands: &mut Commands, session: &AtprotoSession, record: RoomRecord) {
    let session_clone = session.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = reqwest::Client::new();
            pds::publish_room_record(&client, &session_clone, &record).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(PublishRoomTask(task));
}

/// Poll outstanding publish tasks and log results.
pub fn poll_publish_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishRoomTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();
        match result {
            Ok(()) => info!("Room record saved to PDS"),
            Err(e) => warn!("Failed to save room record: {}", e),
        }
    }
}
