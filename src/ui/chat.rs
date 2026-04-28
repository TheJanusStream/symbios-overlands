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

use crate::avatar::{BskyProfileCache, draw_avatar_icon};
use crate::protocol::OverlandsMessage;
use crate::state::{ChatEntry, ChatHistory};

/// Edge length (px) of the avatar icons rendered next to each author's
/// handle in the chat HUD. Same value used by the People panel so the
/// two layouts line up visually.
pub(crate) const AVATAR_ICON_PX: f32 = 18.0;

#[allow(clippy::too_many_arguments)]
pub fn chat_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    mut chat: ResMut<ChatHistory>,
    profile_cache: Res<BskyProfileCache>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut input: Local<String>,
    time: Res<Time>,
) {
    use crate::config::ui::chat as cfg;

    egui::Window::new("Chat")
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            // Reserve vertical space for the bottom separator + input row
            // and let the scroll area fill everything above.
            // `auto_shrink([true, false])` combined with the dynamic
            // `max_height` is what makes the window vertically resizable —
            // as the user drags the window taller, `ui.available_height()`
            // grows, `scroll_height` grows with it, and the scroll area
            // fills the extra space.
            const INPUT_RESERVE_HEIGHT: f32 = 44.0;
            const SCROLL_MIN_HEIGHT: f32 = 60.0;
            let scroll_height =
                (ui.available_height() - INPUT_RESERVE_HEIGHT).max(SCROLL_MIN_HEIGHT);

            egui::ScrollArea::vertical()
                .id_salt("chat_scroll")
                .auto_shrink([true, false])
                .max_height(scroll_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for entry in &chat.messages {
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(egui::Color32::GRAY, format!("[{}]", entry.timestamp));
                            // Profile icon by DID, or a same-sized
                            // placeholder spacer so the row layout
                            // doesn't shift between cache-miss and
                            // cache-hit frames.
                            draw_avatar_icon(
                                ui,
                                entry.did.as_deref(),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            let [r, g, b] = cfg::AUTHOR_COLOR;
                            ui.colored_label(
                                egui::Color32::from_rgb(r, g, b),
                                format!("[{}]", entry.author),
                            );
                            ui.label(&entry.text);
                        });
                    }
                });

            ui.separator();

            // Right-to-left layout: Send first (pinned to the right edge),
            // then the TextEdit whose `desired_width` is set to whatever
            // horizontal space remains — so widening the window stretches
            // the field instead of leaving dead space beside it.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let send = ui.button("Send");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut *input).desired_width(ui.available_width()),
                    );
                    let submit = send.clicked()
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                    if submit && !input.trim().is_empty() {
                        // Enforce a strict per-message length cap *before*
                        // the text is broadcast. Otherwise a peer could
                        // paste an 800 KiB junk string (well under the 1
                        // MiB packet limit) and every guest would try to
                        // word-wrap it in egui on every frame — an instant
                        // room-wide DoS.
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

                        let (did, author) = match session.as_ref() {
                            Some(s) => (Some(s.did.clone()), s.handle.clone()),
                            None => (None, "me".into()),
                        };
                        let ts = crate::format_elapsed_ts(time.elapsed_secs_f64());
                        chat.messages.push(ChatEntry {
                            did,
                            author,
                            text: text.clone(),
                            timestamp: ts,
                        });

                        writer.write(Broadcast {
                            payload: OverlandsMessage::Chat { text },
                            channel: ChannelKind::Reliable,
                        });
                    }
                });
            });
        });
}
