//! Avatar editor — tabbed split view.
//!
//! The avatar window now has two tabs:
//!
//!   * **Visuals** — embeds the same tree-view + detail-panel widget that
//!     drives the room editor's Generators tab, fed by an
//!     [`AvatarVisualsTreeSource`] adapter so the avatar's single
//!     `visuals` root is editable through the unified vocabulary
//!     (primitives only in v1).
//!   * **Locomotion** — picker for the [`LocomotionConfig`] preset
//!     (HoverBoat / Humanoid / Airplane / Helicopter / Car) plus a
//!     per-preset slider panel for collider dimensions and physics
//!     tuning.
//!
//! Live UX is preserved: every widget mutates [`LiveAvatarRecord`] in
//! place, the player module rebuilds visuals or swaps locomotion the same
//! frame the resource changes, and `network::broadcast_avatar_state`
//! pushes a preview update to peers so they see the edit before the
//! author commits. Three explicit buttons drive persistence and discard
//! flows:
//!
//!   * **Publish to PDS** writes the current `LiveAvatarRecord` to the
//!     owner's PDS via `com.atproto.repo.putRecord` and then syncs the
//!     value into [`StoredAvatarRecord`] on success.
//!   * **Load from PDS** drops all in-flight edits by copying
//!     [`StoredAvatarRecord`] back into `LiveAvatarRecord`.
//!   * **Reset to default** replaces `LiveAvatarRecord` with the canonical
//!     [`AvatarRecord::default_for_did`] seed.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{
    self, AirplaneParams, AvatarRecord, CarParams, Fp, HelicopterParams, HoverBoatParams,
    HumanoidParams, LocomotionConfig,
};
use crate::state::{
    LiveAvatarRecord, LiveInventoryRecord, LocalSettings, PublishFeedback, StoredAvatarRecord,
};
use crate::ui::room::generators::{AvatarVisualsTreeSource, GenNodeId, draw_generators_tab};

/// Async task for publishing the avatar record to the owner's PDS.
#[derive(Component)]
pub struct PublishAvatarTask(pub bevy::tasks::Task<Result<(), String>>);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum AvatarTab {
    #[default]
    Visuals,
    Locomotion,
}

/// Persistent avatar-editor state across frames.
#[derive(Default)]
pub struct AvatarEditorState {
    selected_tab: AvatarTab,
    /// Tree-view selection mirrors the room editor's RoomEditorState.
    /// `selected_generator` is always `Some(AvatarVisualsTreeSource::ROOT_NAME)`
    /// once a node has been picked; `selected_prim_path` is the child
    /// chain into the visuals tree.
    selected_generator: Option<String>,
    selected_prim_path: Option<Vec<usize>>,
    tree_view_state: egui_ltreeview::TreeViewState<GenNodeId>,
    /// Unused for the avatar (single-root sources have no rename) but
    /// required by [`draw_generators_tab`]'s signature. Holding a
    /// `Local`-owned `Option` lets us hand a `&mut` to the callee
    /// without conditionally constructing a stack reference each frame.
    renaming_unused: Option<(String, String)>,
    /// Seconds remaining before a pending widget change is flushed into
    /// `LiveAvatarRecord`'s change tick. The downstream player rebuild
    /// and `network::broadcast_avatar_state` peer broadcast fire once
    /// when the timer drains rather than every frame.
    pending_flush_secs: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn avatar_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut live: ResMut<LiveAvatarRecord>,
    stored: Option<Res<StoredAvatarRecord>>,
    mut settings: ResMut<LocalSettings>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut feedback: ResMut<PublishFeedback>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    mut editor: Local<AvatarEditorState>,
    time: Res<Time>,
) {
    use crate::config::ui::airship as cfg;

    // `ResMut::deref_mut` unconditionally flips the change tick, so
    // mutating `live.0` inside the egui closure would otherwise mark the
    // resource changed every frame the editor is visible — and
    // `network::broadcast_avatar_state` turns that into a peer broadcast
    // storm. Route UI access through `bypass_change_detection` and call
    // `live.set_changed()` explicitly below, only after the debounce
    // timer drains.
    let mut widget_changed = false;
    {
        let live_mut = live.bypass_change_detection();
        let before = live_mut.0.clone();

        egui::Window::new("Avatar")
            .default_open(false)
            .default_pos(cfg::WINDOW_DEFAULT_POS)
            .default_width(cfg::WINDOW_DEFAULT_WIDTH)
            .resizable(true)
            .collapsible(true)
            .show(contexts.ctx_mut().unwrap(), |ui| {
                // --- Tab bar ----------------------------------------------
                ui.horizontal(|ui| {
                    let tabs = [
                        (AvatarTab::Visuals, "Visuals"),
                        (AvatarTab::Locomotion, "Locomotion"),
                    ];
                    for (tab, label) in tabs {
                        if ui
                            .selectable_label(editor.selected_tab == tab, label)
                            .clicked()
                        {
                            editor.selected_tab = tab;
                        }
                    }
                });
                ui.separator();

                // Reserve room below the tab body for the separator +
                // Publish/Load/Reset row + feedback line; the scroll
                // area then fills the rest of the window so dragging the
                // window taller actually grows the tab body.
                const FOOTER_RESERVE: f32 = 110.0;
                const BODY_MIN_HEIGHT: f32 = 200.0;
                let body_height = (ui.available_height() - FOOTER_RESERVE).max(BODY_MIN_HEIGHT);

                let AvatarEditorState {
                    selected_tab,
                    selected_generator,
                    selected_prim_path,
                    tree_view_state,
                    renaming_unused,
                    ..
                } = &mut *editor;

                match *selected_tab {
                    AvatarTab::Visuals => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let mut source = AvatarVisualsTreeSource::new(&mut live_mut.0.visuals);
                            draw_generators_tab(
                                ui,
                                &mut source,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                renaming_unused,
                                inventory.as_deref_mut(),
                                &mut widget_changed,
                            );
                        });
                    }
                    AvatarTab::Locomotion => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                draw_locomotion_tab(
                                    ui,
                                    &mut live_mut.0.locomotion,
                                    &mut widget_changed,
                                );
                            });
                    }
                }

                ui.separator();

                // --- Networking (local-only, not broadcast) ---------------
                egui::CollapsingHeader::new("Networking")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.checkbox(
                            &mut settings.smooth_kinematics,
                            "Smooth remote peers (Hermite spline + 100 ms buffer)",
                        );
                        ui.label(
                            egui::RichText::new(
                                "Uncheck to snap to the latest packet and expose raw jitter.",
                            )
                            .small()
                            .weak(),
                        );
                    });

                ui.separator();

                // --- Publish / Load from PDS / Reset to default -----------
                let is_dirty = stored.as_ref().is_some_and(|s| s.0 != live_mut.0);

                ui.horizontal(|ui| {
                    let publish_button = egui::Button::new(
                        egui::RichText::new("Publish to PDS").color(if is_dirty {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::GRAY
                        }),
                    );
                    let publish_enabled = is_dirty && session.is_some() && refresh_ctx.is_some();
                    if ui.add_enabled(publish_enabled, publish_button).clicked()
                        && let (Some(session), Some(refresh)) =
                            (session.as_ref(), refresh_ctx.as_ref())
                    {
                        *feedback = PublishFeedback::Publishing;
                        spawn_publish_avatar_task(
                            &mut commands,
                            session,
                            refresh,
                            live_mut.0.clone(),
                        );
                    }

                    if ui
                        .add_enabled(is_dirty, egui::Button::new("Load from PDS"))
                        .clicked()
                        && let Some(stored) = &stored
                    {
                        live_mut.0 = stored.0.clone();
                    }

                    let default_record = session
                        .as_ref()
                        .map(|s| AvatarRecord::default_for_did(&s.did));
                    let reset_enabled = default_record.as_ref().is_some_and(|d| *d != live_mut.0);
                    if ui
                        .add_enabled(reset_enabled, egui::Button::new("Reset to default"))
                        .clicked()
                        && let Some(default_record) = default_record
                    {
                        live_mut.0 = default_record;
                    }
                });

                // --- Status line ------------------------------------------
                match &*feedback {
                    PublishFeedback::Idle => {}
                    PublishFeedback::Publishing => {
                        ui.label(egui::RichText::new("Publishing…").italics().weak());
                    }
                    PublishFeedback::Success { .. } => {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Published ✓");
                    }
                    PublishFeedback::Failed { message, .. } => {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("Publish failed: {message}"),
                        );
                    }
                }
            });

        if live_mut.0 != before {
            widget_changed = true;
        }
    }

    if widget_changed {
        editor.pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
    }
    if editor.pending_flush_secs > 0.0 {
        editor.pending_flush_secs = (editor.pending_flush_secs - time.delta_secs()).max(0.0);
        if editor.pending_flush_secs <= 0.0 {
            // Debounce drained — publish the accumulated edit to player
            // (visual rebuild) and `broadcast_avatar_state` (peer
            // preview) in a single change tick.
            live.set_changed();
        }
    }
}

// ---------------------------------------------------------------------------
// Locomotion tab
// ---------------------------------------------------------------------------

fn draw_locomotion_tab(ui: &mut egui::Ui, locomotion: &mut LocomotionConfig, dirty: &mut bool) {
    let current_kind = locomotion.kind_tag();

    ui.horizontal_wrapped(|ui| {
        ui.label("Preset:");
        for (kind, label, ctor) in LocomotionConfig::pickers() {
            if ui.selectable_label(current_kind == *kind, *label).clicked() && current_kind != *kind
            {
                *locomotion = ctor();
                *dirty = true;
            }
        }
    });
    ui.separator();

    match locomotion {
        LocomotionConfig::HoverBoat(p) => hover_boat_panel(ui, p, dirty),
        LocomotionConfig::Humanoid(p) => humanoid_panel(ui, p, dirty),
        LocomotionConfig::Airplane(p) => airplane_panel(ui, p, dirty),
        LocomotionConfig::Helicopter(p) => helicopter_panel(ui, p, dirty),
        LocomotionConfig::Car(p) => car_panel(ui, p, dirty),
        LocomotionConfig::Unknown => {
            ui.colored_label(
                egui::Color32::ORANGE,
                "This avatar's locomotion preset was authored against a newer schema — \
                 pick a preset above to replace it.",
            );
        }
    }
}

fn hover_boat_panel(ui: &mut egui::Ui, p: &mut HoverBoatParams, dirty: &mut bool) {
    egui::CollapsingHeader::new("Chassis")
        .default_open(true)
        .show(ui, |ui| {
            fp3_extents(
                ui,
                "Half-extents (X/Y/Z, m)",
                &mut p.chassis_half_extents.0,
                dirty,
            );
            ui.label("Mass (kg)");
            fp_slider(ui, &mut p.mass, 5.0..=200.0, 1.0, dirty);
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=10.0, 0.1, dirty);
            ui.label("Angular damping");
            fp_slider(ui, &mut p.angular_damping, 0.0..=20.0, 0.1, dirty);
        });

    egui::CollapsingHeader::new("Suspension & Drive")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Suspension rest length");
            fp_slider(ui, &mut p.suspension_rest_length, 0.2..=2.0, 0.05, dirty);
            ui.label("Suspension stiffness");
            fp_slider(
                ui,
                &mut p.suspension_stiffness,
                500.0..=15_000.0,
                50.0,
                dirty,
            );
            ui.label("Suspension damping");
            fp_slider(ui, &mut p.suspension_damping, 10.0..=500.0, 5.0, dirty);
            ui.label("Drive force");
            fp_slider(ui, &mut p.drive_force, 500.0..=10_000.0, 50.0, dirty);
            ui.label("Turn torque");
            fp_slider(ui, &mut p.turn_torque, 200.0..=6_000.0, 50.0, dirty);
            ui.label("Lateral grip");
            fp_slider(ui, &mut p.lateral_grip, 500.0..=15_000.0, 100.0, dirty);
            ui.label("Jump force");
            fp_slider(ui, &mut p.jump_force, 500.0..=8_000.0, 50.0, dirty);
            ui.label("Uprighting torque");
            fp_slider(ui, &mut p.uprighting_torque, 100.0..=3_000.0, 50.0, dirty);
        });

    egui::CollapsingHeader::new("Buoyancy")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Water rest length (m)");
            fp_slider(ui, &mut p.water_rest_length, 0.0..=3.0, 0.05, dirty);
            ui.label("Strength (N/m)");
            fp_slider(ui, &mut p.buoyancy_strength, 0.0..=10_000.0, 50.0, dirty);
            ui.label("Damping (N·s/m)");
            fp_slider(ui, &mut p.buoyancy_damping, 0.0..=2_000.0, 10.0, dirty);
            ui.label("Max depth (m)");
            fp_slider(ui, &mut p.buoyancy_max_depth, 0.1..=5.0, 0.05, dirty);
        });
}

fn humanoid_panel(ui: &mut egui::Ui, p: &mut HumanoidParams, dirty: &mut bool) {
    egui::CollapsingHeader::new("Capsule")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Radius (m)");
            fp_slider(ui, &mut p.capsule_radius, 0.05..=1.0, 0.01, dirty);
            ui.label("Cylinder length (m)");
            fp_slider(ui, &mut p.capsule_length, 0.1..=4.0, 0.05, dirty);
            ui.label(
                egui::RichText::new(format!(
                    "Total height ≈ {:.2} m (length + 2·radius)",
                    p.total_height()
                ))
                .small()
                .weak(),
            );
            ui.label("Mass (kg)");
            fp_slider(ui, &mut p.mass, 30.0..=150.0, 1.0, dirty);
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=3.0, 0.05, dirty);
        });

    egui::CollapsingHeader::new("Locomotion")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Walk speed (m/s)");
            fp_slider(ui, &mut p.walk_speed, 1.0..=10.0, 0.1, dirty);
            ui.label("Acceleration (1/s)");
            fp_slider(ui, &mut p.acceleration, 2.0..=30.0, 0.5, dirty);
            ui.label("Jump impulse (N·s)");
            fp_slider(ui, &mut p.jump_impulse, 100.0..=1500.0, 10.0, dirty);
        });

    egui::CollapsingHeader::new("Water")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Wading speed factor");
            fp_slider(ui, &mut p.wading_speed_factor, 0.0..=1.0, 0.05, dirty);
            ui.label("Swim speed (m/s)");
            fp_slider(ui, &mut p.swim_speed, 0.5..=8.0, 0.1, dirty);
            ui.label("Swim vertical speed (m/s)");
            fp_slider(ui, &mut p.swim_vertical_speed, 0.2..=5.0, 0.1, dirty);
        });
}

fn airplane_panel(ui: &mut egui::Ui, p: &mut AirplaneParams, dirty: &mut bool) {
    egui::CollapsingHeader::new("Chassis")
        .default_open(true)
        .show(ui, |ui| {
            fp3_extents(
                ui,
                "Half-extents (X/Y/Z, m)",
                &mut p.chassis_half_extents.0,
                dirty,
            );
            ui.label("Mass (kg)");
            fp_slider(ui, &mut p.mass, 5.0..=500.0, 1.0, dirty);
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=5.0, 0.05, dirty);
            ui.label("Angular damping");
            fp_slider(ui, &mut p.angular_damping, 0.0..=20.0, 0.1, dirty);
        });

    egui::CollapsingHeader::new("Thrust & control surfaces")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Thrust (N)");
            fp_slider(ui, &mut p.thrust, 0.0..=10_000.0, 50.0, dirty);
            ui.label("Pitch torque (N·m)");
            fp_slider(ui, &mut p.pitch_torque, 0.0..=5_000.0, 25.0, dirty);
            ui.label("Roll torque (N·m)");
            fp_slider(ui, &mut p.roll_torque, 0.0..=5_000.0, 25.0, dirty);
            ui.label("Yaw / rudder torque (N·m)");
            fp_slider(ui, &mut p.yaw_torque, 0.0..=5_000.0, 25.0, dirty);
        });

    egui::CollapsingHeader::new("Aerodynamics")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Lift per (m/s) airspeed");
            fp_slider(ui, &mut p.lift_per_speed, 0.0..=200.0, 1.0, dirty);
            ui.label("Drag coefficient");
            fp_slider(ui, &mut p.drag_coefficient, 0.0..=5.0, 0.05, dirty);
            ui.label("Min airspeed (m/s)");
            fp_slider(ui, &mut p.min_airspeed, 0.0..=30.0, 0.5, dirty);
        });
}

fn helicopter_panel(ui: &mut egui::Ui, p: &mut HelicopterParams, dirty: &mut bool) {
    egui::CollapsingHeader::new("Chassis")
        .default_open(true)
        .show(ui, |ui| {
            fp3_extents(
                ui,
                "Half-extents (X/Y/Z, m)",
                &mut p.chassis_half_extents.0,
                dirty,
            );
            ui.label("Mass (kg)");
            fp_slider(ui, &mut p.mass, 5.0..=500.0, 1.0, dirty);
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=10.0, 0.05, dirty);
            ui.label("Angular damping");
            fp_slider(ui, &mut p.angular_damping, 0.0..=20.0, 0.1, dirty);
        });

    egui::CollapsingHeader::new("Hover & cyclic")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Hover thrust (N)");
            fp_slider(ui, &mut p.hover_thrust, 0.0..=10_000.0, 25.0, dirty);
            ui.label("Vertical speed (m/s)");
            fp_slider(ui, &mut p.vertical_speed, 0.0..=20.0, 0.25, dirty);
            ui.label("Cyclic force (N)");
            fp_slider(ui, &mut p.cyclic_force, 0.0..=5_000.0, 25.0, dirty);
            ui.label("Strafe force (N)");
            fp_slider(ui, &mut p.strafe_force, 0.0..=5_000.0, 25.0, dirty);
            ui.label("Yaw torque (N·m)");
            fp_slider(ui, &mut p.yaw_torque, 0.0..=5_000.0, 25.0, dirty);
        });
}

fn car_panel(ui: &mut egui::Ui, p: &mut CarParams, dirty: &mut bool) {
    egui::CollapsingHeader::new("Chassis")
        .default_open(true)
        .show(ui, |ui| {
            fp3_extents(
                ui,
                "Half-extents (X/Y/Z, m)",
                &mut p.chassis_half_extents.0,
                dirty,
            );
            ui.label("Mass (kg)");
            fp_slider(ui, &mut p.mass, 100.0..=5_000.0, 10.0, dirty);
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=10.0, 0.05, dirty);
            ui.label("Angular damping");
            fp_slider(ui, &mut p.angular_damping, 0.0..=20.0, 0.1, dirty);
        });

    egui::CollapsingHeader::new("Suspension")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Rest length (m)");
            fp_slider(ui, &mut p.suspension_rest_length, 0.1..=2.0, 0.025, dirty);
            ui.label("Stiffness");
            fp_slider(
                ui,
                &mut p.suspension_stiffness,
                500.0..=50_000.0,
                100.0,
                dirty,
            );
            ui.label("Damping");
            fp_slider(ui, &mut p.suspension_damping, 10.0..=2_000.0, 10.0, dirty);
        });

    egui::CollapsingHeader::new("Drive & steering")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Drive force (N)");
            fp_slider(ui, &mut p.drive_force, 500.0..=20_000.0, 50.0, dirty);
            ui.label("Turn torque (N·m)");
            fp_slider(ui, &mut p.turn_torque, 200.0..=10_000.0, 50.0, dirty);
            ui.label("Lateral grip");
            fp_slider(ui, &mut p.lateral_grip, 500.0..=50_000.0, 100.0, dirty);
            ui.label("Handbrake grip factor");
            fp_slider(ui, &mut p.handbrake_grip_factor, 0.0..=2.0, 0.01, dirty);
        });
}

fn fp_slider(
    ui: &mut egui::Ui,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    dirty: &mut bool,
) {
    if ui
        .add(egui::Slider::new(&mut value.0, range).step_by(step))
        .changed()
    {
        *dirty = true;
    }
}

/// Three-component drag editor for `Fp3` half-extents (or any other
/// vec3-shaped numeric triple). Edits land in the underlying `[f32; 3]`
/// directly so the caller's `Fp3` wrapper picks up the change without an
/// intermediate copy.
fn fp3_extents(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        for axis in value.iter_mut() {
            if ui
                .add(egui::DragValue::new(axis).speed(0.05).range(0.05..=20.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
}

fn spawn_publish_avatar_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: AvatarRecord,
) {
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_avatar_record(&client, &session_clone, &refresh_clone, &record).await
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
    commands.spawn(PublishAvatarTask(task));
}

/// Poll outstanding avatar publish tasks. On success, sync `LiveAvatarRecord`
/// into `StoredAvatarRecord` so the "Load from PDS" button is disabled until the
/// next edit.
pub fn poll_publish_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishAvatarTask)>,
    live: Res<LiveAvatarRecord>,
    mut stored: Option<ResMut<StoredAvatarRecord>>,
    mut feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Avatar record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = live.0.clone();
                }
                *feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save avatar record: {}", e);
                *feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
