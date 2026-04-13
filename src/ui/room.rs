//! Sovereign room editor — advanced JSON mode.
//!
//! Rendered only when `session.did == current_room.0` (i.e. the signed-in
//! user owns the room they are visiting).  The new recipe-style
//! `RoomRecord` carries arbitrarily nested `generators`, `placements` and
//! `traits`, which is impractical to expose as a flat slider panel; this
//! editor instead shows the record's pretty-printed JSON and lets the
//! owner edit it directly.
//!
//! On "Apply & Save to PDS":
//!   1. Parse the text as a `RoomRecord`. On failure, show the serde error
//!      in red and abort.
//!   2. Overwrite `ResMut<RoomRecord>` — `world_builder::compile_room_record`
//!      picks up the change and rebuilds every compiled entity in one pass.
//!   3. Broadcast the new recipe as `RoomStateUpdate` on the Reliable
//!      channel so connected guests see the change without reloading.
//!   4. Publish to the owner's PDS via `com.atproto.repo.putRecord`.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::pds::{self, RoomRecord};
use crate::protocol::OverlandsMessage;
use crate::state::CurrentRoomDid;

/// Async task for publishing the room record to the owner's PDS.
#[derive(Component)]
pub struct PublishRoomTask(pub bevy::tasks::Task<Result<(), String>>);

/// Local editor state kept across frames — the user's in-flight text and
/// the last parse error (if any). Cleared when the user hits "Reset".
#[derive(Default)]
pub struct RoomEditorState {
    /// True after the editor text has been initialised from the record.
    /// Prevents re-syncing on every frame and clobbering in-progress edits.
    pub initialised: bool,
    pub text: String,
    pub error: Option<String>,
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

    // Security gate: only the room owner sees this panel.
    if session.did != room_did.0 {
        return;
    }

    // One-time sync: initialise the editor buffer from the current record
    // the first frame we render. Further record updates (e.g. from a
    // `RoomStateUpdate` broadcast) do NOT overwrite in-progress edits.
    if !editor.initialised {
        editor.text = serde_json::to_string_pretty(record.as_ref())
            .unwrap_or_else(|e| format!("// serialize error: {}", e));
        editor.initialised = true;
    }

    let ctx = contexts.ctx_mut().unwrap();

    egui::Window::new("Room Settings (Advanced)")
        .collapsible(true)
        .resizable(true)
        .default_width(520.0)
        .default_pos([10.0, 500.0])
        .show(ctx, |ui| {
            ui.label("Raw Lexicon JSON (RoomRecord):");
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(18)
                            .desired_width(f32::INFINITY),
                    );
                });

            if let Some(err) = &editor.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Apply & Save to PDS").clicked() {
                    match serde_json::from_str::<RoomRecord>(&editor.text) {
                        Ok(new_record) => {
                            editor.error = None;

                            // `**record` swaps the resource in place; Bevy
                            // marks it changed so `world_builder` will
                            // despawn + recompile on the next frame.
                            **record = new_record.clone();

                            // Broadcast to guests so they see the change
                            // without a reconnect.
                            writer.write(Broadcast {
                                payload: OverlandsMessage::RoomStateUpdate(new_record.clone()),
                                channel: ChannelKind::Reliable,
                            });

                            // Publish to PDS. Same IO-pool pattern as the
                            // old slider-based editor — the blocking HTTP
                            // call must not contend with CPU workers.
                            let session_clone = session.clone();
                            let pool = bevy::tasks::IoTaskPool::get();
                            let task = pool.spawn(async move {
                                let fut = async {
                                    let client = reqwest::Client::new();
                                    pds::publish_room_record(&client, &session_clone, &new_record)
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
                            commands.spawn(PublishRoomTask(task));
                        }
                        Err(e) => {
                            editor.error = Some(format!("Invalid JSON schema: {}", e));
                        }
                    }
                }

                if ui.button("Reset from Record").clicked() {
                    editor.text = serde_json::to_string_pretty(record.as_ref())
                        .unwrap_or_else(|e| format!("// serialize error: {}", e));
                    editor.error = None;
                }
            });
        });
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
