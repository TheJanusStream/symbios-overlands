//! Personal inventory stash UI.
//!
//! The Inventory window lists every `Generator` the owner has tucked aside
//! across editing sessions. Each entry can be renamed in place via a modal,
//! removed, or kept — and once the stash diverges from the PDS-persisted
//! copy, "Publish to PDS" pushes the new state through
//! `com.atproto.repo.putRecord`. The stash is loaded during `AppState::Loading`
//! (see `main.rs::start_inventory_record_fetch`) so a freshly-logged-in
//! owner always sees the previously-saved items the moment they land in-game.
//!
//! `InventoryRecord` does not derive `PartialEq` (the underlying `Generator`
//! enum doesn't either, because its variants carry types that themselves
//! would need full equality), so the dirty check round-trips through
//! `serde_json` — same pattern the world editor uses for its Reset button.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::state::{InventoryPublishFeedback, LiveInventoryRecord, StoredInventoryRecord};

/// Persistent UI-only state for the Inventory window. Held in a `Local` so
/// it lives for the lifetime of the system without polluting the global
/// resource table.
#[derive(Default)]
pub struct InventoryEditorState {
    /// Active rename modal: `(original_key, draft_key)`.
    pub renaming_generator: Option<(String, String)>,
}

/// Async task for publishing the inventory record to the owner's PDS.
#[derive(Component)]
pub struct PublishInventoryTask(pub bevy::tasks::Task<Result<(), String>>);

#[allow(clippy::too_many_arguments)]
pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    mut feedback: ResMut<InventoryPublishFeedback>,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
) {
    let (Some(live), Some(stored), Some(session)) = (live.as_mut(), stored, session) else {
        return;
    };

    // `InventoryRecord` lacks `PartialEq`, so we diff through serde_json —
    // identical JSON means identical contents for our purposes. The two
    // values are small (at most 50 generators) so this is cheap per frame.
    let mut is_dirty = serde_json::to_value(&live.0).ok() != serde_json::to_value(&stored.0).ok();
    let ctx = contexts.ctx_mut().unwrap();

    // Rename modal — independent top-level egui Window so it floats above
    // the Inventory window. Same shape as the world editor's rename flow.
    if let Some((old_name, mut new_name)) = state.renaming_generator.clone() {
        let mut close = false;
        let mut apply = false;
        egui::Window::new("Rename Inventory Item")
            .collapsible(false)
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut new_name).request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        if apply
            && !new_name.is_empty()
            && !live.0.generators.contains_key(&new_name)
            && let Some(g) = live.0.generators.remove(&old_name)
        {
            live.0.generators.insert(new_name.clone(), g);
            is_dirty = true;
        }
        if close {
            state.renaming_generator = None;
        } else {
            state.renaming_generator = Some((old_name, new_name));
        }
    }

    egui::Window::new("Inventory")
        .default_pos([580.0, 220.0])
        .default_size([300.0, 400.0])
        .collapsible(true)
        .show(ctx, |ui| {
            ui.label(format!("Stored Generators: {}/50", live.0.generators.len()));
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    let mut to_remove: Option<String> = None;
                    let mut names: Vec<String> = live.0.generators.keys().cloned().collect();
                    names.sort();

                    for name in names {
                        ui.horizontal(|ui| {
                            ui.label(&name);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new("−")
                                                .fill(egui::Color32::from_rgb(180, 50, 50)),
                                        )
                                        .clicked()
                                    {
                                        to_remove = Some(name.clone());
                                    }
                                    if ui.small_button("Rename").clicked() {
                                        state.renaming_generator =
                                            Some((name.clone(), name.clone()));
                                    }
                                },
                            );
                        });
                    }
                    if let Some(name) = to_remove {
                        live.0.generators.remove(&name);
                        is_dirty = true;
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                let publish_button =
                    egui::Button::new(egui::RichText::new("Publish to PDS").color(if is_dirty {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GRAY
                    }));

                if ui.add_enabled(is_dirty, publish_button).clicked() {
                    *feedback = InventoryPublishFeedback::Publishing;

                    let session_clone = session.clone();
                    let record_clone = live.0.clone();
                    let pool = bevy::tasks::IoTaskPool::get();
                    let task = pool.spawn(async move {
                        let fut = async {
                            let client = crate::config::http::default_client();
                            crate::pds::publish_inventory_record(
                                &client,
                                &session_clone,
                                &record_clone,
                            )
                            .await
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
                    commands.spawn(PublishInventoryTask(task));
                }
            });

            match feedback.as_ref() {
                InventoryPublishFeedback::Idle => {}
                InventoryPublishFeedback::Publishing => {
                    ui.colored_label(egui::Color32::from_rgb(220, 200, 80), "⟳ Publishing…");
                }
                InventoryPublishFeedback::Success { at_secs } => {
                    ui.colored_label(
                        egui::Color32::from_rgb(80, 200, 120),
                        format!(
                            "✓ Saved ({:.0}s ago)",
                            (time.elapsed_secs_f64() - at_secs).max(0.0)
                        ),
                    );
                }
                InventoryPublishFeedback::Failed { at_secs, message } => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 90, 90),
                        format!(
                            "✗ Failed ({:.0}s ago): {}",
                            (time.elapsed_secs_f64() - at_secs).max(0.0),
                            message
                        ),
                    );
                }
            }
        });
}

pub fn poll_publish_inventory_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishInventoryTask)>,
    live: Option<Res<LiveInventoryRecord>>,
    mut stored: Option<ResMut<StoredInventoryRecord>>,
    mut feedback: ResMut<InventoryPublishFeedback>,
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
                info!("Inventory record saved to PDS");
                if let (Some(live), Some(stored)) = (live.as_ref(), stored.as_mut()) {
                    stored.0 = live.0.clone();
                }
                *feedback = InventoryPublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save inventory record: {}", e);
                *feedback = InventoryPublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
