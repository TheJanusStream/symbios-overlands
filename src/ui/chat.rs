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
use crate::state::ChatHistory;

pub fn chat_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    mut chat: ResMut<ChatHistory>,
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
            ui.separator();

            let scroll_height =
                (ui.available_height() - cfg::INPUT_RESERVE_HEIGHT).max(cfg::SCROLL_MIN_HEIGHT);

            egui::ScrollArea::vertical()
                .id_salt("chat_scroll")
                .max_height(scroll_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    //ui.set_max_width(ui.available_width());
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
