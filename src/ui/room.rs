use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::pds::{self, RoomRecord};
use crate::protocol::OverlandsMessage;
use crate::state::CurrentRoomDid;
use crate::terrain::WaterVolume;

/// Async task for publishing the room record to the owner's PDS.
#[derive(Component)]
pub struct PublishRoomTask(pub bevy::tasks::Task<Result<(), String>>);

/// "God mode" admin panel — only rendered when the authenticated user owns the
/// room (i.e. `session.did == current_room.0`).
#[allow(clippy::too_many_arguments)]
pub fn room_admin_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    room_record: Option<ResMut<RoomRecord>>,
    mut water: Query<&mut Transform, With<WaterVolume>>,
    mut dir_lights: Query<&mut DirectionalLight>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
) {
    let (Some(session), Some(room_did), Some(mut room_record)) = (session, room_did, room_record)
    else {
        return;
    };

    // Security gate: only the room owner sees this panel.
    if session.did != room_did.0 {
        return;
    }

    let ctx = contexts.ctx_mut().unwrap();

    egui::Window::new("Room Settings")
        .collapsible(true)
        .resizable(false)
        .default_pos([10.0, 500.0])
        .show(ctx, |ui| {
            ui.label("You own this overland. Customise it below.");
            ui.add_space(6.0);

            // --- Water level offset slider ---
            let prev_water = room_record.water_level_offset;
            ui.horizontal(|ui| {
                ui.label("Water Level Offset:");
                ui.add(egui::Slider::new(
                    &mut room_record.water_level_offset,
                    -5.0..=15.0,
                ));
            });

            // Live-update the water volume transform when the slider moves.
            if room_record.water_level_offset != prev_water {
                let base_wl = (crate::config::terrain::water::LEVEL_FACTOR
                    * crate::config::terrain::HEIGHT_SCALE)
                    .max(0.001);
                let wl = (base_wl + room_record.water_level_offset).max(0.001);
                for mut tf in water.iter_mut() {
                    tf.translation.y = wl / 2.0;
                    tf.scale.y = wl;
                }
            }

            // --- Sun colour picker ---
            ui.horizontal(|ui| {
                ui.label("Sun Color:");
                let mut c = room_record.sun_color;
                ui.color_edit_button_rgb(&mut c);
                if c != room_record.sun_color {
                    room_record.sun_color = c;
                    for mut light in dir_lights.iter_mut() {
                        light.color = Color::srgb(c[0], c[1], c[2]);
                    }
                }
            });

            ui.add_space(8.0);

            // --- Save button ---
            if ui.button("Save to PDS").clicked() {
                let record = room_record.clone();
                let session_clone = session.clone();
                // Use the IO pool for the blocking putRecord HTTP call so it
                // never contends with CPU-bound compute workers.
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

                // Broadcast the updated room state to all connected peers so
                // guests see the change instantly without refreshing.
                writer.write(Broadcast {
                    payload: OverlandsMessage::RoomStateUpdate(room_record.clone()),
                    channel: ChannelKind::Reliable,
                });
            }
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
