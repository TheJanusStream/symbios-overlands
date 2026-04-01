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
) {
    use crate::config::ui::chat as cfg;
    egui::SidePanel::right("chat")
        .resizable(true)
        .width_range(cfg::PANEL_WIDTH_MIN..=cfg::PANEL_WIDTH_MAX)
        .default_width(cfg::PANEL_DEFAULT_WIDTH)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.heading("Chat");
            ui.separator();

            let scroll_height = (ui.available_height() - cfg::INPUT_RESERVE_HEIGHT).max(cfg::SCROLL_MIN_HEIGHT);

            egui::ScrollArea::vertical()
                .id_salt("chat_scroll")
                .max_height(scroll_height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    for (author, text) in &chat.messages {
                        ui.horizontal_wrapped(|ui| {
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
                let response =
                    ui.add(egui::TextEdit::singleline(&mut *input).desired_width(f32::INFINITY));
                let send = ui.button("Send");
                let submit = send.clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                if submit && !input.trim().is_empty() {
                    let text = input.trim().to_string();
                    input.clear();
                    response.request_focus();

                    let author = session
                        .as_ref()
                        .map(|s| s.handle.clone())
                        .unwrap_or_else(|| "me".into());
                    chat.messages.push((author, text.clone()));

                    writer.write(Broadcast {
                        payload: OverlandsMessage::Chat { text },
                        channel: ChannelKind::Reliable,
                    });
                }
            });
        });
}
