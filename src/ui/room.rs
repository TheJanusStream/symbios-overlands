//! Sovereign room editor — tabbed Master/Detail view.
//!
//! Rendered only when `session.did == current_room.0` (the signed-in user
//! owns the room they are visiting). Follows the same **Live UX** paradigm
//! as the avatar editor: every widget mutates the live `ResMut<RoomRecord>`
//! in place, so the world recompiles and remote peers mirror the edit the
//! same frame the slider moves — the peer broadcast is driven by the
//! `network::broadcast_room_state` system watching `Res::is_changed`. Three
//! explicit buttons drive persistence and discard flows:
//!
//! - **Publish to PDS** pushes the current `RoomRecord` to the owner's PDS
//!   via `com.atproto.repo.putRecord` and syncs the value into
//!   [`StoredRoomRecord`] on success.
//! - **Load from PDS** drops all in-flight edits by copying
//!   [`StoredRoomRecord`] back into the live `RoomRecord`.
//! - **Reset to default** replaces `RoomRecord` with the canonical
//!   `RoomRecord::default_for_did` seed — useful after a botched edit or
//!   when starting from scratch.
//!
//! The editor is intentionally forgiving: any field it doesn't yet expose
//! as a widget still round-trips via the Raw JSON tab, so L-system code,
//! prop mappings, traits, etc. stay editable while the visual UI catches
//! up to the full schema.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{
    self, Environment, Fp, Fp2, Fp3, Fp4, Fp64, Generator, Placement, PrimNode, PrimShape,
    PropMeshType, RoomRecord, ScatterBounds, SovereignAshlarConfig, SovereignAsphaltConfig,
    SovereignBarkConfig, SovereignBrickConfig, SovereignCobblestoneConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignEncausticConfig, SovereignGeneratorKind,
    SovereignGroundConfig, SovereignIronGrilleConfig, SovereignLeafConfig, SovereignMarbleConfig,
    SovereignMaterialConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPaversConfig, SovereignPlankConfig, SovereignRockConfig, SovereignShingleConfig,
    SovereignSplatRule, SovereignStainedGlassConfig, SovereignStuccoConfig, SovereignTerrainConfig,
    SovereignTextureConfig, SovereignThatchConfig, SovereignTwigConfig, SovereignWainscotingConfig,
    SovereignWindowConfig, TransformData,
};
use crate::state::{CurrentRoomDid, PublishFeedback, RoomRecordRecovery, StoredRoomRecord};

/// Async task for publishing the room record to the owner's PDS.
#[derive(Component)]
pub struct PublishRoomTask(pub bevy::tasks::Task<Result<(), String>>);

/// Async task for the hard-reset publish path (delete-then-put). Separate
/// from `PublishRoomTask` only for logging clarity — the two share the same
/// result type and poll system.
#[derive(Component)]
pub struct ResetRoomTask(pub bevy::tasks::Task<Result<(), String>>);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorTab {
    #[default]
    Environment,
    Generators,
    Placements,
    Raw,
}

/// Persistent editor state kept across frames. Promoted to a `Resource` so
/// the 3D gizmo controller in `editor_gizmo` can observe which placement the
/// owner has selected in the UI panel.
#[derive(Resource, Default)]
pub struct RoomEditorState {
    pub selected_tab: EditorTab,
    pub selected_generator: Option<String>,
    pub selected_placement: Option<usize>,
    raw_text: String,
    raw_text_initialised: bool,
    raw_error: Option<String>,
    /// True once a widget mutates the live record relative to the last
    /// committed / loaded / reset state — drives the Publish button
    /// colouring.
    is_dirty: bool,
    /// Seconds remaining before a pending widget change is flushed into
    /// the live `RoomRecord`'s change tick. Dragging a slider resets
    /// this to `MENU_DEBOUNCE_SECS`; the downstream terrain rebuild,
    /// world-compiler pass, and peer `RoomStateUpdate` broadcast fire
    /// exactly once when the timer drains rather than every frame the
    /// slider moves.
    pending_flush_secs: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn room_admin_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut room_record: Option<ResMut<RoomRecord>>,
    stored: Option<Res<StoredRoomRecord>>,
    recovery: Option<Res<RoomRecordRecovery>>,
    mut editor: ResMut<RoomEditorState>,
    mut publish_feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    let (Some(session), Some(room_did), Some(record)) = (session, room_did, room_record.as_mut())
    else {
        return;
    };

    // Security gate — only the owner may edit their own room.
    if session.did != room_did.0 {
        return;
    }

    if !editor.raw_text_initialised {
        editor.raw_text = serde_json::to_string_pretty(record.as_ref())
            .unwrap_or_else(|e| format!("// serialize error: {}", e));
        editor.raw_text_initialised = true;
    }

    // Destructure the Local into independent field borrows so the
    // borrow-checker can see that the tab-body closure and the commit-row
    // closure each touch *disjoint* subsets of the editor state. Without
    // this, re-borrowing `editor` inside nested egui closures trips E0499.
    let RoomEditorState {
        selected_tab,
        selected_generator,
        selected_placement,
        raw_text,
        raw_error,
        is_dirty,
        pending_flush_secs,
        ..
    } = &mut *editor;

    let ctx = contexts.ctx_mut().unwrap();

    // `ResMut::deref_mut` unconditionally flips the change tick, so any
    // `&mut record.field` access taken while the window is open would mark
    // the resource as changed every frame — which in turn spams peers with
    // `RoomStateUpdate` broadcasts even when nothing was actually edited.
    // Route all UI access through `bypass_change_detection` and call
    // `record.set_changed()` explicitly at the bottom only when a widget or
    // Load/Reset click actually mutated the record.
    let mut widget_change = false;
    let mut needs_broadcast = false;

    {
        let record_mut: &mut RoomRecord = record.bypass_change_detection();

        egui::Window::new("World Editor")
            .collapsible(true)
            .resizable(true)
            .default_width(560.0)
            .default_height(620.0)
            .default_pos([10.0, 220.0])
            .show(ctx, |ui| {
                // Recovery banner — shown when the stored PDS record failed
                // to decode and we're running on the synthesised default.
                // Offers a one-click "Reset PDS to default" so the owner can
                // deliberately overwrite the incompatible record instead of
                // being stuck.
                if let Some(rec) = recovery.as_deref() {
                    let banner = egui::Frame::new()
                        .fill(egui::Color32::from_rgb(90, 30, 30))
                        .inner_margin(6.0)
                        .corner_radius(4.0);
                    banner.show(ui, |ui| {
                        ui.colored_label(
                            egui::Color32::WHITE,
                            "⚠ Stored room record is incompatible with this build.",
                        );
                        ui.label(format!("Decode error: {}", rec.reason));
                        ui.label(
                            "You are currently editing the default homeworld. Click below \
                             to overwrite the stored record on your PDS with this default \
                             so the next login loads cleanly.",
                        );
                        if ui.button("Reset PDS to default").clicked() {
                            let default_record = pds::RoomRecord::default_for_did(&room_did.0);
                            *record_mut = default_record.clone();
                            *raw_text =
                                serde_json::to_string_pretty(&default_record).unwrap_or_default();
                            *raw_error = None;
                            *is_dirty = false;
                            needs_broadcast = true;
                            // Use the delete-then-put reset path. The vanilla
                            // putRecord upsert can return 500 when the stored
                            // record is incompatible with the current lexicon;
                            // hard-deleting first sidesteps that failure mode.
                            spawn_reset_task(&mut commands, &session, default_record);
                            commands.remove_resource::<RoomRecordRecovery>();
                        }
                    });
                    ui.add_space(6.0);
                }

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
                            // Refresh the JSON text when the user arrives at
                            // the Raw tab so it reflects any edits made in
                            // the other tabs since the last time it was
                            // viewed.
                            if tab == EditorTab::Raw && *selected_tab != EditorTab::Raw {
                                *raw_text =
                                    serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                                *raw_error = None;
                            }
                            *selected_tab = tab;
                        }
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(460.0)
                    .show(ui, |ui| match *selected_tab {
                        EditorTab::Environment => {
                            draw_environment_tab(
                                ui,
                                &mut record_mut.environment,
                                &mut widget_change,
                            );
                        }
                        EditorTab::Generators => {
                            draw_generators_tab(
                                ui,
                                record_mut,
                                selected_generator,
                                &mut widget_change,
                            );
                        }
                        EditorTab::Placements => {
                            draw_placements_tab(
                                ui,
                                record_mut,
                                selected_placement,
                                &mut widget_change,
                            );
                        }
                        EditorTab::Raw => {
                            draw_raw_tab(ui, raw_text, raw_error, record_mut, &mut widget_change);
                        }
                    });

                ui.separator();

                // Publish / Load from PDS / Reset to default
                ui.horizontal(|ui| {
                    let publish_button = egui::Button::new(
                        egui::RichText::new("Publish to PDS").color(if *is_dirty {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::GRAY
                        }),
                    );
                    if ui.add_enabled(*is_dirty, publish_button).clicked() {
                        let new_record = record_mut.clone();
                        *is_dirty = false;
                        *publish_feedback = PublishFeedback::Publishing;
                        spawn_publish_task(&mut commands, &session, new_record);
                    }

                    let can_load = stored.is_some() && *is_dirty;
                    if ui
                        .add_enabled(can_load, egui::Button::new("Load from PDS"))
                        .clicked()
                        && let Some(stored) = stored.as_ref()
                    {
                        *record_mut = stored.0.clone();
                        *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                        *raw_error = None;
                        *is_dirty = false;
                        *selected_generator = None;
                        *selected_placement = None;
                        needs_broadcast = true;
                    }

                    if ui.button("Reset to default").clicked() {
                        *record_mut = pds::RoomRecord::default_for_did(&room_did.0);
                        *raw_text = serde_json::to_string_pretty(&*record_mut).unwrap_or_default();
                        *raw_error = None;
                        *is_dirty = stored
                            .as_ref()
                            .map(|s| {
                                serde_json::to_value(&s.0).ok()
                                    != serde_json::to_value(&*record_mut).ok()
                            })
                            .unwrap_or(true);
                        *selected_generator = None;
                        *selected_placement = None;
                        needs_broadcast = true;
                    }
                });

                // Publish status indicator. `Idle` stays silent; other states
                // render a coloured one-liner so the owner knows whether the
                // PDS round-trip actually landed without having to tail the
                // console.
                match publish_feedback.as_ref() {
                    PublishFeedback::Idle => {}
                    PublishFeedback::Publishing => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 200, 80),
                            "⟳ Publishing to PDS…",
                        );
                    }
                    PublishFeedback::Success { at_secs } => {
                        let ago = (time.elapsed_secs_f64() - at_secs).max(0.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 120),
                            format!("✓ Published to PDS ({:.0}s ago)", ago),
                        );
                    }
                    PublishFeedback::Failed { at_secs, message } => {
                        let ago = (time.elapsed_secs_f64() - at_secs).max(0.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 90, 90),
                            format!("✗ Publish failed ({:.0}s ago): {}", ago, message),
                        );
                    }
                }
            });
    }

    if widget_change {
        *is_dirty = true;
        *pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
    }
    // Drain the debounce timer and flip `needs_broadcast` on the frame it
    // reaches zero. A slider drag keeps resetting `pending_flush_secs`
    // above, so the flush only fires once the user pauses — collapsing a
    // ~60 Hz storm of RoomStateUpdate broadcasts and terrain rebuilds into
    // one event per edit burst.
    if *pending_flush_secs > 0.0 {
        *pending_flush_secs = (*pending_flush_secs - time.delta_secs()).max(0.0);
        if *pending_flush_secs <= 0.0 {
            needs_broadcast = true;
        }
    }
    if needs_broadcast {
        // Explicit Load / Reset / recovery clicks land here too; zero the
        // timer so a concurrently-debounced slider flush cannot double-fire
        // set_changed() on the very next frame.
        *pending_flush_secs = 0.0;
        record.set_changed();
    }
}

// ---------------------------------------------------------------------------
// Tab: Environment
// ---------------------------------------------------------------------------

fn draw_environment_tab(ui: &mut egui::Ui, env: &mut Environment, dirty: &mut bool) {
    ui.heading("Environment");
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Lighting & Sky")
        .default_open(true)
        .show(ui, |ui| {
            color_picker(ui, "Sun colour", &mut env.sun_color, dirty);
            color_picker(ui, "Sky colour", &mut env.sky_color, dirty);
            fp_slider(
                ui,
                "Sun illuminance",
                &mut env.sun_illuminance,
                0.0,
                50_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Ambient brightness",
                &mut env.ambient_brightness,
                0.0,
                2_000.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Distance Fog")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Visibility (m)",
                &mut env.fog_visibility,
                50.0,
                2_000.0,
                dirty,
            );
            color_picker_rgba(ui, "Fog colour", &mut env.fog_color, dirty);
            color_picker(ui, "Extinction", &mut env.fog_extinction, dirty);
            color_picker(ui, "Inscattering", &mut env.fog_inscattering, dirty);
            color_picker_rgba(ui, "Sun glow", &mut env.fog_sun_color, dirty);
            fp_slider(
                ui,
                "Sun glow exponent",
                &mut env.fog_sun_exponent,
                1.0,
                100.0,
                dirty,
            );
        });
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
    // Single-column master/detail: when a generator is selected and still
    // exists, render the detail view full-width with a back button;
    // otherwise render the master list full-width.
    let selected_exists = selected
        .as_ref()
        .is_some_and(|n| record.generators.contains_key(n));

    if selected_exists {
        let name = selected.clone().expect("selected_exists implies Some");
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                *selected = None;
            }
            ui.heading("Detail");
        });
        ui.add_space(4.0);
        if *selected == Some(name.clone())
            && let Some(g) = record.generators.get_mut(&name)
        {
            draw_generator_detail(ui, &name, g, dirty);
        }
        return;
    }

    // Drop any stale selection so a later re-select of the same name starts
    // cleanly and the "(Selection no longer exists.)" state never renders.
    *selected = None;

    ui.heading("Generators");
    ui.add_space(4.0);

    let mut names: Vec<String> = record.generators.keys().cloned().collect();
    names.sort();

    let mut to_remove: Option<String> = None;
    for name in &names {
        ui.horizontal(|ui| {
            if ui.selectable_label(false, name).clicked() {
                *selected = Some(name.clone());
            }
            if ui.small_button("−").clicked() {
                to_remove = Some(name.clone());
            }
        });
    }
    if let Some(name) = to_remove {
        record.generators.remove(&name);
        *dirty = true;
    }

    ui.add_space(6.0);
    ui.separator();
    ui.label("Add new generator:");
    ui.horizontal(|ui| {
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
        if ui.small_button("+ Portal").clicked() {
            let name = unique_key(&record.generators, "portal");
            record.generators.insert(
                name.clone(),
                Generator::Portal {
                    target_did: String::new(),
                    target_pos: Fp3([0.0, 0.0, 0.0]),
                },
            );
            *selected = Some(name);
            *dirty = true;
        }
        if ui.small_button("+ Construct").clicked() {
            let name = unique_key(&record.generators, "construct");
            record.generators.insert(
                name.clone(),
                Generator::Construct {
                    root: PrimNode::default(),
                },
            );
            *selected = Some(name);
            *dirty = true;
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
            prop_mappings,
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
            prop_mappings,
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
        Generator::Portal {
            target_did,
            target_pos,
        } => {
            ui.label("Target DID (destination room)");
            if ui
                .add(egui::TextEdit::singleline(target_did).hint_text("did:plc:…"))
                .changed()
            {
                *dirty = true;
            }
            ui.add_space(4.0);
            ui.label("Exit position (world space in the target room)");
            // Drag the raw f32s directly — wrapping each axis in a temporary
            // `Fp` to reuse `fp_slider` wouldn't round-trip the edit back to
            // the underlying `[f32; 3]`.
            ui.horizontal(|ui| {
                ui.label("X");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[0]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Y");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[1]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Z");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[2]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        Generator::Construct { root } => {
            draw_construct_forge(ui, root, dirty);
        }
        Generator::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Construct forge — recursive hierarchical primitive editor
// ---------------------------------------------------------------------------

fn draw_construct_forge(ui: &mut egui::Ui, root: &mut PrimNode, dirty: &mut bool) {
    ui.label(
        "Hierarchical primitive tree. Root anchors to the world; \
        children inherit transform, and every solid node contributes a collider.",
    );
    ui.add_space(4.0);
    draw_prim_node_ui(ui, root, true, dirty, "root");
}

/// Recursive node editor. `is_root` suppresses the delete button for the tree
/// root. `path_salt` makes every egui ID unique across the recursive tree so
/// collapsing one sibling never affects another.
fn draw_prim_node_ui(
    ui: &mut egui::Ui,
    node: &mut PrimNode,
    is_root: bool,
    dirty: &mut bool,
    path_salt: &str,
) -> PrimNodeAction {
    let header = format!("{:?}", node.shape);
    let mut action = PrimNodeAction::None;
    egui::CollapsingHeader::new(header)
        .id_salt(path_salt)
        .default_open(true)
        .show(ui, |ui| {
            shape_combo(ui, &mut node.shape, path_salt, dirty);

            if ui.checkbox(&mut node.solid, "Solid (collider)").changed() {
                *dirty = true;
            }

            ui.add_space(4.0);
            draw_transform(ui, &mut node.transform, dirty);

            egui::CollapsingHeader::new("Material")
                .id_salt(format!("{}_mat", path_salt))
                .default_open(false)
                .show(ui, |ui| {
                    draw_prim_material(ui, &mut node.material, path_salt, dirty);
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.small_button("+ Add child").clicked() {
                    node.children.push(PrimNode::default());
                    *dirty = true;
                }
                if !is_root && ui.small_button("− Delete").clicked() {
                    action = PrimNodeAction::Delete;
                }
            });

            ui.add_space(4.0);
            let mut to_remove: Option<usize> = None;
            for (i, child) in node.children.iter_mut().enumerate() {
                let child_salt = format!("{}_c{}", path_salt, i);
                let child_action = draw_prim_node_ui(ui, child, false, dirty, &child_salt);
                if matches!(child_action, PrimNodeAction::Delete) {
                    to_remove = Some(i);
                }
            }
            if let Some(i) = to_remove {
                node.children.remove(i);
                *dirty = true;
            }
        });
    action
}

/// Signal returned by `draw_prim_node_ui` so the parent can remove a child
/// that asked to be deleted. Keeping the delete state out of the child's own
/// mutation avoids borrow conflicts with the recursive `iter_mut`.
enum PrimNodeAction {
    None,
    Delete,
}

fn shape_combo(ui: &mut egui::Ui, shape: &mut PrimShape, salt: &str, dirty: &mut bool) {
    egui::ComboBox::from_id_salt(format!("{}_shape", salt))
        .selected_text(format!("{:?}", shape))
        .show_ui(ui, |ui| {
            let shapes = [
                PrimShape::Cube,
                PrimShape::Sphere,
                PrimShape::Cylinder,
                PrimShape::Capsule,
                PrimShape::Cone,
                PrimShape::Torus,
            ];
            for s in shapes {
                if ui.selectable_value(shape, s, format!("{:?}", s)).changed() {
                    *dirty = true;
                }
            }
        });
}

/// Slim material editor for a single Prim node. Mirrors the L-system slot
/// UI but scoped to a single `SovereignMaterialSettings` with `salt` making
/// every internal egui id unique across the recursive tree.
fn draw_prim_material(
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
    prop_mappings: &mut std::collections::HashMap<u16, PropMeshType>,
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
                    fp_slider(ui, "UV scale", &mut m.uv_scale, 0.1, 10.0, dirty);

                    let salt = format!("mat_{}", id);
                    draw_texture_bridge(ui, &mut m.texture, &salt, dirty);
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

    egui::CollapsingHeader::new("Prop mappings")
        .default_open(false)
        .show(ui, |ui| {
            let mut ids: Vec<u16> = prop_mappings.keys().copied().collect();
            ids.sort_unstable();
            let mut to_remove: Option<u16> = None;
            for id in ids {
                ui.horizontal(|ui| {
                    ui.label(format!("~{}", id));
                    if let Some(current) = prop_mappings.get_mut(&id) {
                        egui::ComboBox::from_id_salt(format!("prop_map_{}", id))
                            .selected_text(format!("{:?}", current))
                            .show_ui(ui, |ui| {
                                let types = [
                                    PropMeshType::Leaf,
                                    PropMeshType::Twig,
                                    PropMeshType::Sphere,
                                    PropMeshType::Cone,
                                    PropMeshType::Cylinder,
                                    PropMeshType::Cube,
                                ];
                                for t in types {
                                    if ui
                                        .selectable_value(current, t, format!("{:?}", t))
                                        .changed()
                                    {
                                        *dirty = true;
                                    }
                                }
                            });
                    }
                    if ui.small_button("−").clicked() {
                        to_remove = Some(id);
                    }
                });
            }
            if let Some(id) = to_remove {
                prop_mappings.remove(&id);
                *dirty = true;
            }
            if ui.button("+ Add mapping").clicked() {
                let next = (0u16..=255).find(|k| !prop_mappings.contains_key(k));
                if let Some(k) = next {
                    prop_mappings.insert(k, PropMeshType::Leaf);
                    *dirty = true;
                }
            }
        });
}

/// Unified texture picker + upstream config editor bridge. Renders a combo
/// box for the 24 [`SovereignTextureConfig`] variants, then hands the
/// active variant's config to the corresponding
/// `bevy_symbios_texture::ui::*_config_editor`, round-tripping through the
/// native type so edits persist back into the DAG-CBOR-safe mirror.
fn draw_texture_bridge(
    ui: &mut egui::Ui,
    texture: &mut SovereignTextureConfig,
    salt: &str,
    dirty: &mut bool,
) {
    egui::ComboBox::from_id_salt(format!("{}_tex_ty", salt))
        .selected_text(texture.label())
        .show_ui(ui, |ui| {
            macro_rules! opt {
                ($label:literal, $expr:expr) => {{
                    let selected = texture.label() == $label;
                    if ui.selectable_label(selected, $label).clicked() && !selected {
                        *texture = $expr;
                        *dirty = true;
                    }
                }};
            }
            opt!("None", SovereignTextureConfig::None);
            opt!("Leaf", SovereignTextureConfig::Leaf(Default::default()));
            opt!("Twig", SovereignTextureConfig::Twig(Default::default()));
            opt!("Bark", SovereignTextureConfig::Bark(Default::default()));
            opt!("Window", SovereignTextureConfig::Window(Default::default()));
            opt!(
                "Stained Glass",
                SovereignTextureConfig::StainedGlass(Default::default())
            );
            opt!(
                "Iron Grille",
                SovereignTextureConfig::IronGrille(Default::default())
            );
            opt!("Ground", SovereignTextureConfig::Ground(Default::default()));
            opt!("Rock", SovereignTextureConfig::Rock(Default::default()));
            opt!("Brick", SovereignTextureConfig::Brick(Default::default()));
            opt!("Plank", SovereignTextureConfig::Plank(Default::default()));
            opt!(
                "Shingle",
                SovereignTextureConfig::Shingle(Default::default())
            );
            opt!("Stucco", SovereignTextureConfig::Stucco(Default::default()));
            opt!(
                "Concrete",
                SovereignTextureConfig::Concrete(Default::default())
            );
            opt!("Metal", SovereignTextureConfig::Metal(Default::default()));
            opt!("Pavers", SovereignTextureConfig::Pavers(Default::default()));
            opt!("Ashlar", SovereignTextureConfig::Ashlar(Default::default()));
            opt!(
                "Cobblestone",
                SovereignTextureConfig::Cobblestone(Default::default())
            );
            opt!("Thatch", SovereignTextureConfig::Thatch(Default::default()));
            opt!("Marble", SovereignTextureConfig::Marble(Default::default()));
            opt!(
                "Corrugated",
                SovereignTextureConfig::Corrugated(Default::default())
            );
            opt!(
                "Asphalt",
                SovereignTextureConfig::Asphalt(Default::default())
            );
            opt!(
                "Wainscoting",
                SovereignTextureConfig::Wainscoting(Default::default())
            );
            opt!(
                "Encaustic",
                SovereignTextureConfig::Encaustic(Default::default())
            );
        });

    let id = egui::Id::new(salt);
    macro_rules! run {
        ($c:expr, $sov:ty, $editor:path) => {{
            let mut native = $c.to_native();
            let (wb, _regen) = $editor(ui, &mut native, id);
            if wb {
                *$c = <$sov>::from_native(&native);
                *dirty = true;
            }
        }};
    }

    match texture {
        SovereignTextureConfig::None | SovereignTextureConfig::Unknown => {}
        SovereignTextureConfig::Leaf(c) => run!(
            c,
            SovereignLeafConfig,
            bevy_symbios_texture::ui::leaf_config_editor
        ),
        SovereignTextureConfig::Twig(c) => run!(
            c,
            SovereignTwigConfig,
            bevy_symbios_texture::ui::twig_config_editor
        ),
        SovereignTextureConfig::Bark(c) => run!(
            c,
            SovereignBarkConfig,
            bevy_symbios_texture::ui::bark_config_editor
        ),
        SovereignTextureConfig::Window(c) => run!(
            c,
            SovereignWindowConfig,
            bevy_symbios_texture::ui::window_config_editor
        ),
        SovereignTextureConfig::StainedGlass(c) => run!(
            c,
            SovereignStainedGlassConfig,
            bevy_symbios_texture::ui::stained_glass_config_editor
        ),
        SovereignTextureConfig::IronGrille(c) => run!(
            c,
            SovereignIronGrilleConfig,
            bevy_symbios_texture::ui::iron_grille_config_editor
        ),
        SovereignTextureConfig::Ground(c) => run!(
            c,
            SovereignGroundConfig,
            bevy_symbios_texture::ui::ground_config_editor
        ),
        SovereignTextureConfig::Rock(c) => run!(
            c,
            SovereignRockConfig,
            bevy_symbios_texture::ui::rock_config_editor
        ),
        SovereignTextureConfig::Brick(c) => run!(
            c,
            SovereignBrickConfig,
            bevy_symbios_texture::ui::brick_config_editor
        ),
        SovereignTextureConfig::Plank(c) => run!(
            c,
            SovereignPlankConfig,
            bevy_symbios_texture::ui::plank_config_editor
        ),
        SovereignTextureConfig::Shingle(c) => run!(
            c,
            SovereignShingleConfig,
            bevy_symbios_texture::ui::shingle_config_editor
        ),
        SovereignTextureConfig::Stucco(c) => run!(
            c,
            SovereignStuccoConfig,
            bevy_symbios_texture::ui::stucco_config_editor
        ),
        SovereignTextureConfig::Concrete(c) => run!(
            c,
            SovereignConcreteConfig,
            bevy_symbios_texture::ui::concrete_config_editor
        ),
        SovereignTextureConfig::Metal(c) => run!(
            c,
            SovereignMetalConfig,
            bevy_symbios_texture::ui::metal_config_editor
        ),
        SovereignTextureConfig::Pavers(c) => run!(
            c,
            SovereignPaversConfig,
            bevy_symbios_texture::ui::pavers_config_editor
        ),
        SovereignTextureConfig::Ashlar(c) => run!(
            c,
            SovereignAshlarConfig,
            bevy_symbios_texture::ui::ashlar_config_editor
        ),
        SovereignTextureConfig::Cobblestone(c) => run!(
            c,
            SovereignCobblestoneConfig,
            bevy_symbios_texture::ui::cobblestone_config_editor
        ),
        SovereignTextureConfig::Thatch(c) => run!(
            c,
            SovereignThatchConfig,
            bevy_symbios_texture::ui::thatch_config_editor
        ),
        SovereignTextureConfig::Marble(c) => run!(
            c,
            SovereignMarbleConfig,
            bevy_symbios_texture::ui::marble_config_editor
        ),
        SovereignTextureConfig::Corrugated(c) => run!(
            c,
            SovereignCorrugatedConfig,
            bevy_symbios_texture::ui::corrugated_config_editor
        ),
        SovereignTextureConfig::Asphalt(c) => run!(
            c,
            SovereignAsphaltConfig,
            bevy_symbios_texture::ui::asphalt_config_editor
        ),
        SovereignTextureConfig::Wainscoting(c) => run!(
            c,
            SovereignWainscotingConfig,
            bevy_symbios_texture::ui::wainscoting_config_editor
        ),
        SovereignTextureConfig::Encaustic(c) => run!(
            c,
            SovereignEncausticConfig,
            bevy_symbios_texture::ui::encaustic_config_editor
        ),
    }
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
    // Single-column master/detail — see `draw_generators_tab` for the
    // rationale; logic mirrors it with index-based selection.
    let selected_exists = selected.is_some_and(|i| i < record.placements.len());

    if selected_exists {
        let idx = selected.expect("selected_exists implies Some");
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                *selected = None;
            }
            ui.heading(format!("Detail — #{idx}"));
        });
        ui.add_space(4.0);
        let gen_names: Vec<String> = record.generators.keys().cloned().collect();
        if let Some(p) = record.placements.get_mut(idx) {
            draw_placement_detail(ui, p, &gen_names, dirty);
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
            } => format!("#{i} Scatter × {count} → {generator_ref}"),
            Placement::Unknown => format!("#{i} (unknown)"),
        };
        ui.horizontal(|ui| {
            if ui.selectable_label(false, label).clicked() {
                *selected = Some(i);
            }
            if ui.small_button("−").clicked() {
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
                Ok(mut parsed) => {
                    // Enforce the same bounds the network-ingress path
                    // applies — the raw JSON tab otherwise lets the owner
                    // bypass `sanitize()` and hand a 2 GiB grid_size or
                    // unbounded L-system iterations straight to the world
                    // compiler.
                    parsed.sanitize();
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

/// RGBA colour picker — mirrors [`color_picker`] but for [`Fp4`] fields
/// where the alpha channel carries renderer-relevant information (fog
/// opacity, sun-glow strength). Uses the unmultiplied variant so the
/// alpha edits independently of RGB rather than being pre-scaled.
fn color_picker_rgba(ui: &mut egui::Ui, label: &str, value: &mut Fp4, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = value.0;
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *value = Fp4(rgba);
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

/// "Ternary Tree (+Props +Materials +Variations)" preset, ported verbatim
/// from `lsystem-explorer`. Ships with three material slots (bark / twig /
/// leaf) pre-wired to procedural textures, plus a prop-mapping table so the
/// `B` terminals become leaf billboards and `~(0)` props become twig cards.
fn default_lsystem_generator() -> Generator {
    let mut materials = std::collections::HashMap::new();

    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.35, 0.2, 0.08]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig::default()),
            ..Default::default()
        },
    );

    let mut prop_mappings = std::collections::HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);
    prop_mappings.insert(1, PropMeshType::Leaf);

    Generator::LSystem {
        source_code: "#define d1 180\n#define th 3.5\n#define d2 252\n#define a 36\n#define lr 1.12\n#define vr 1.532\n#define ps 60.0\n#define s 50.0\n#define ir 10.0\nomega: C(0.0)!(th)F(4*s)/(45)A[B]\np0: A : 0.7 -> !(th*vr)F(s)[&(a)F(s)A[B]]/(d1)[&(a)F(s)A[B]]/(d2)[&(a)F(s)A[B]]\np1: A : 0.3 -> !(th*vr)F(s)A[B]\np2: F(l) : * -> F(l*lr)\np3: !(w) : * -> !(w*vr)\np4: B : * -> \np5: B -> \np6: C(x) : 0.7 -> C(x)\np7: C(x) : 0.3 -> C(x-ir)".to_string(),
        finalization_code: "p8: B : * -> ,(1)~(0,ps)\np9: C(x) : * -> /(x)".to_string(),
        iterations: 6,
        seed: 1,
        angle: Fp(36.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.05),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
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
            let client = crate::config::http::default_client();
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

/// Spawn the hard-reset publish task — delete the stored record first, then
/// create a fresh one. Used by the recovery banner's "Reset PDS to default"
/// button, which has to work around PDS implementations that return 500 on
/// `putRecord` when the prior blob is schema-incompatible.
fn spawn_reset_task(commands: &mut Commands, session: &AtprotoSession, record: RoomRecord) {
    let session_clone = session.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::reset_room_record(&client, &session_clone, &record).await
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
    commands.spawn(ResetRoomTask(task));
}

/// Poll outstanding publish and reset tasks and log results. On success,
/// pin `StoredRoomRecord` to the live `RoomRecord` so subsequent "Load from
/// PDS" presses restore the now-committed state and the dirty indicator
/// resets.
pub fn poll_publish_tasks(
    mut commands: Commands,
    mut publish_tasks: Query<(Entity, &mut PublishRoomTask)>,
    mut reset_tasks: Query<(Entity, &mut ResetRoomTask)>,
    live: Option<Res<RoomRecord>>,
    mut stored: Option<ResMut<StoredRoomRecord>>,
    mut publish_feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    for (entity, mut task) in publish_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Room record saved to PDS");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.as_ref().clone();
                }
                *publish_feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save room record: {}", e);
                *publish_feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
    for (entity, mut task) in reset_tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Room record reset on PDS (delete + put)");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.as_ref().clone();
                }
                *publish_feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to reset room record: {}", e);
                *publish_feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
