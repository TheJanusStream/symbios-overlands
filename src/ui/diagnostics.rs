//! Diagnostics HUD: local identity, current room DID, peer roster with per-
//! peer mute toggles, a scrolling event log, and the log-out button that
//! transitions the app back to `AppState::Login`.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::boot_params::{build_landmark_link, write_to_clipboard};
use crate::player::HumanoidVisualRoot;
use crate::state::{AppState, CurrentRoomDid, DiagnosticsLog, LocalPlayer, RemotePeer};

#[allow(clippy::too_many_arguments)]
pub fn diagnostics_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut peers: Query<&mut RemotePeer>,
    diagnostics: Res<DiagnosticsLog>,
    mut next_state: ResMut<NextState<AppState>>,
    mut landmark_status: Local<Option<(String, f64)>>,
    time: Res<Time>,
    local_player_q: Query<&Transform, With<LocalPlayer>>,
    visual_root_q: Query<&Transform, With<HumanoidVisualRoot>>,
) {
    use crate::config::ui::diagnostics as cfg;

    egui::Window::new("Diagnostics")
        .default_open(false)
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.label("Local Identity");
            match &session {
                Some(sess) => {
                    ui.monospace(format!("@{}", sess.handle));
                    ui.monospace(
                        egui::RichText::new(&sess.did)
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
                None => {
                    ui.colored_label(egui::Color32::GRAY, "(not authenticated)");
                }
            }

            if ui.button("Log out").clicked() {
                next_state.set(AppState::Login);
            }

            ui.separator();

            if let Some(room) = &room_did {
                ui.label("Room");
                ui.monospace(
                    egui::RichText::new(&room.0)
                        .small()
                        .color(egui::Color32::GRAY),
                );

                // "Copy Landmark Link" — emits a shareable URL pointing at
                // the WASM build with the local player's current room DID,
                // exact world position, and yaw in degrees. Visible to
                // visitors as well as owners (any player in the room can
                // share where they are).
                let player_tf = local_player_q.single().ok().copied();
                let visual_tf = visual_root_q.single().ok().copied();
                let can_copy = player_tf.is_some();
                if ui
                    .add_enabled(can_copy, egui::Button::new("Copy Landmark Link"))
                    .clicked()
                    && let Some(tf) = player_tf
                {
                    // Humanoid locks rotation on the rigid body and yaws a
                    // child visual root instead, so prefer the visual root
                    // when present. HoverRover's chassis transform carries
                    // its own yaw directly.
                    let yaw_rad = visual_tf
                        .map(|v| v.rotation)
                        .unwrap_or(tf.rotation)
                        .to_euler(EulerRot::YXZ)
                        .0;
                    let yaw_deg = yaw_rad.to_degrees();
                    let link = build_landmark_link(&room.0, tf.translation, yaw_deg);
                    let now = time.elapsed_secs_f64();
                    *landmark_status = Some(match write_to_clipboard(&link) {
                        Ok(()) => (format!("Copied: {link}"), now),
                        Err(e) => (format!("Copy failed ({e}); {link}"), now),
                    });
                }
                if let Some((msg, at)) = landmark_status.as_ref() {
                    let ago = (time.elapsed_secs_f64() - at).max(0.0);
                    if ago < 6.0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(160, 200, 160),
                            egui::RichText::new(msg).small(),
                        );
                    }
                }
                ui.separator();
            }

            let peer_count = peers.iter().count();
            ui.label(format!("Peers ({})", peer_count));

            for mut peer in peers.iter_mut() {
                let handle = peer.handle.as_deref().unwrap_or("identifying…").to_owned();
                let did = peer.did.as_deref().unwrap_or("unknown").to_owned();
                let dot_color = if peer.muted {
                    egui::Color32::GRAY
                } else {
                    egui::Color32::GREEN
                };
                let mut muted = peer.muted;
                ui.horizontal(|ui| {
                    ui.colored_label(dot_color, "●");
                    ui.vertical(|ui| {
                        ui.monospace(format!("@{}", handle));
                        ui.monospace(egui::RichText::new(&did).small().color(egui::Color32::GRAY));
                    });
                    ui.checkbox(&mut muted, "Mute");
                });
                // Guard the write so Bevy's change-detection flag is only
                // raised when the mute state actually flips.  An unconditional
                // assignment marks `RemotePeer` as `Changed` every frame and
                // invalidates any `Changed<RemotePeer>` filters downstream.
                if peer.muted != muted {
                    peer.muted = muted;
                }
            }

            if peer_count == 0 {
                ui.colored_label(egui::Color32::GRAY, "(no peers)");
            }

            ui.separator();

            ui.label("Event Log");
            let log_height = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("diag_log")
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .max_height(log_height)
                .show(ui, |ui| {
                    for (ts, entry) in diagnostics.iter() {
                        ui.horizontal(|ui| {
                            ui.monospace(
                                egui::RichText::new(ts).small().color(egui::Color32::GRAY),
                            );
                            ui.monospace(
                                egui::RichText::new(entry)
                                    .small()
                                    .color(egui::Color32::LIGHT_GRAY),
                            );
                        });
                    }
                });
        });
}
