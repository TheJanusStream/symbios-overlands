//! In-game chat window.
//!
//! Renders `ChatHistory` into a scroll area and exposes a single-line input
//! that broadcasts `OverlandsMessage::Chat` over the Reliable channel.  The
//! sender enforces the same `MAX_MESSAGE_LEN` cap as the receiver so a
//! misbehaving peer who bypasses this UI still gets its payload clipped on
//! every other client in the room.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::protocol::OverlandsMessage;
use crate::state::{ChatHistory, RemotePeer};

/// Per-UI state for the chat window. Held in a `Local` so the expanded /
/// collapsed state of the roster column survives frame-to-frame without
/// leaking into the global resource table.
pub struct ChatUiState {
    /// Whether the roster column is currently expanded. Defaults to open so
    /// a freshly-landed visitor immediately sees who else is in the room.
    pub roster_expanded: bool,
}

impl Default for ChatUiState {
    fn default() -> Self {
        Self {
            roster_expanded: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn chat_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    peers: Query<&RemotePeer>,
    mut chat: ResMut<ChatHistory>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut input: Local<String>,
    mut state: Local<ChatUiState>,
    time: Res<Time>,
) {
    use crate::config::ui::chat as cfg;

    egui::Window::new("Chat")
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.separator();

            // Build the room roster — self plus every remote peer. Peers
            // whose Identity handshake is still in flight render as
            // "identifying…" so the row count matches `diagnostics_ui` and
            // the user can see someone has joined even before the handle
            // resolves.
            let self_handle = session.as_ref().map(|s| s.handle.clone());
            let mut peer_handles: Vec<String> = peers
                .iter()
                .map(|p| {
                    p.handle
                        .clone()
                        .unwrap_or_else(|| "identifying…".to_string())
                })
                .collect();
            peer_handles.sort();

            let scroll_height =
                (ui.available_height() - cfg::INPUT_RESERVE_HEIGHT).max(cfg::SCROLL_MIN_HEIGHT);

            ui.horizontal(|ui| {
                // Collapsed: a single narrow toggle column so the chat keeps
                // most of the horizontal space. Expanded: the same toggle
                // plus the handle list.
                let toggle_label = if state.roster_expanded { "◂" } else { "▸" };
                if ui
                    .add(egui::Button::new(toggle_label).small())
                    .on_hover_text(if state.roster_expanded {
                        "Hide room roster"
                    } else {
                        "Show room roster"
                    })
                    .clicked()
                {
                    state.roster_expanded = !state.roster_expanded;
                }

                if state.roster_expanded {
                    ui.separator();
                    ui.vertical(|ui| {
                        let total = peer_handles.len() + self_handle.is_some() as usize;
                        ui.label(
                            egui::RichText::new(format!("In room ({})", total))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("chat_roster")
                            .max_height(scroll_height)
                            .show(ui, |ui| {
                                let [r, g, b] = cfg::AUTHOR_COLOR;
                                let author_color = egui::Color32::from_rgb(r, g, b);
                                if let Some(h) = self_handle.as_ref() {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(author_color, "●");
                                        ui.monospace(format!("@{} (you)", h));
                                    });
                                }
                                for h in &peer_handles {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(egui::Color32::GREEN, "●");
                                        ui.monospace(format!("@{}", h));
                                    });
                                }
                                if self_handle.is_none() && peer_handles.is_empty() {
                                    ui.colored_label(egui::Color32::GRAY, "(empty)");
                                }
                            });
                    });
                    ui.separator();
                }

                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("chat_scroll")
                        .max_height(scroll_height)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for (author, text, ts) in &chat.messages {
                                ui.horizontal_wrapped(|ui| {
                                    ui.colored_label(egui::Color32::GRAY, format!("[{}]", ts));
                                    let [r, g, b] = cfg::AUTHOR_COLOR;
                                    ui.colored_label(
                                        egui::Color32::from_rgb(r, g, b),
                                        format!("[{}]", author),
                                    );
                                    ui.label(text);
                                });
                            }
                        });
                });
            });

            ui.separator();

            ui.horizontal(|ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut *input));
                let send = ui.button("Send");
                let submit = send.clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                if submit && !input.trim().is_empty() {
                    // Enforce a strict per-message length cap *before* the
                    // text is broadcast.  Otherwise a peer could paste an
                    // 800 KiB junk string (well under the 1 MiB packet limit)
                    // and every guest would try to word-wrap it in egui on
                    // every frame — an instant room-wide denial of service.
                    let trimmed = input.trim();
                    let max = cfg::MAX_MESSAGE_LEN;
                    let text = if trimmed.len() <= max {
                        trimmed.to_string()
                    } else {
                        let mut end = max;
                        while end > 0 && !trimmed.is_char_boundary(end) {
                            end -= 1;
                        }
                        trimmed[..end].to_string()
                    };
                    input.clear();
                    response.request_focus();

                    let author = session
                        .as_ref()
                        .map(|s| s.handle.clone())
                        .unwrap_or_else(|| "me".into());
                    let ts = crate::format_elapsed_ts(time.elapsed_secs_f64());
                    chat.messages.push((author, text.clone(), ts));

                    writer.write(Broadcast {
                        payload: OverlandsMessage::Chat { text },
                        channel: ChannelKind::Reliable,
                    });
                }
            });
        });
}
