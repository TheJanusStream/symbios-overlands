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
use crate::state::{ChatHistory, RemotePeer, SocialResonance};

/// Edge length (px) of the avatar icons rendered next to each author's
/// handle in the chat HUD. Same value used by the People panel so the
/// two layouts line up visually.
pub(crate) const AVATAR_ICON_PX: f32 = 18.0;

/// One-shot request to focus the chat input (#836): the global Enter
/// shortcut sets it (alongside opening the panel) and [`chat_ui`]
/// consumes it on its next render, so a reply is Enter → type → Enter.
#[derive(Resource, Default)]
pub struct ChatFocusRequest(pub bool);

#[allow(clippy::too_many_arguments)]
pub fn chat_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut focus_request: ResMut<ChatFocusRequest>,
    mut was_open: Local<bool>,
    session: Option<Res<AtprotoSession>>,
    mut chat: ResMut<ChatHistory>,
    profile_cache: Res<BskyProfileCache>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut input: Local<String>,
    peers: Query<(&RemotePeer, Option<&SocialResonance>)>,
) {
    use crate::config::ui::chat as cfg;

    // Autofocus on open (#846): however the window opened — toolbar
    // toggle, Enter shortcut, unread-badge click — the input grabs focus
    // on the rising edge, so "open chat → type" needs no extra click.
    // Reuses the #836 one-shot request the input widget already consumes.
    let just_opened = panels.chat && !*was_open;
    *was_open = panels.chat;
    if just_opened {
        focus_request.0 = true;
    }

    // DIDs of peers the local user mutually follows — their chat author
    // tag gets the same warm-gold ★ as their People-panel row. Built
    // once per frame from the live peer set; `SocialResonance` is absent
    // until the async getRelationships query lands, so a brand-new peer
    // simply renders un-highlighted until then. The local user is not a
    // peer entity, so their own messages never match (you are not your
    // own mutual).
    let mutual_dids: std::collections::HashSet<&str> = peers
        .iter()
        .filter(|(_, r)| matches!(r, Some(SocialResonance::Mutual)))
        .filter_map(|(p, _)| p.did.as_deref())
        .collect();

    let ctx = contexts.ctx_mut().unwrap();
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Chat, ctx);
    let response = egui::Window::new("Chat")
        .open(&mut panels.chat)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(ctx.available_rect())
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
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
                            // Local wall-clock HH:MM (#846) — the old stamp
                            // was minutes-since-app-launch, meaningless
                            // across peers and sessions.
                            ui.colored_label(
                                crate::ui::theme::current(ui.ctx()).text_weak,
                                format!("[{}]", crate::state::clock_hhmm(entry.at_epoch_secs)),
                            );
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
                            let is_mutual = entry
                                .did
                                .as_deref()
                                .is_some_and(|d| mutual_dids.contains(d));
                            // Accent star for mutuals, info-blue author
                            // tag (#856) — same roles the People window
                            // uses, formerly bespoke config golds/blues.
                            let th = crate::ui::theme::current(ui.ctx());
                            let (tag_color, tag_text) = if is_mutual {
                                (th.accent, format!("★ [{}]", entry.author))
                            } else {
                                (th.status.info, format!("[{}]", entry.author))
                            };
                            let tag = ui.colored_label(tag_color, tag_text);
                            if is_mutual {
                                tag.on_hover_text("You and this peer follow each other");
                            }
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
                    // Global Enter shortcut (#836): consume the one-shot
                    // focus request so typing starts immediately.
                    if focus_request.0 {
                        response.request_focus();
                        focus_request.0 = false;
                    }
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
                        let clipped = if trimmed.len() <= max {
                            trimmed.to_string()
                        } else {
                            let mut end = max;
                            while end > 0 && !trimmed.is_char_boundary(end) {
                                end -= 1;
                            }
                            trimmed[..end].to_string()
                        };
                        // Strip ASCII control characters (newlines,
                        // carriage returns, form feeds, …) before either
                        // pushing to our own HUD or broadcasting. The
                        // receiver runs the same filter defensively, so
                        // skipping it here previously left the local
                        // sender's row showing a multi-line paste while
                        // every remote peer saw a single-line version —
                        // a permanent visual desync on the sender's HUD.
                        let text: String = clipped
                            .chars()
                            .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
                            .collect();
                        input.clear();
                        response.request_focus();

                        let (did, author) = match session.as_ref() {
                            Some(s) => (Some(s.did.clone()), s.handle.clone()),
                            None => (None, "me".to_owned()),
                        };
                        // Capped + wall-clock-stamped (#846): local sends
                        // used to push uncapped with a session-relative
                        // stamp.
                        chat.push(did, author, text.clone());

                        writer.write(Broadcast {
                            payload: OverlandsMessage::Chat { text },
                            channel: ChannelKind::Reliable,
                        });
                    }
                });
            });
        });
    if let Some(response) = response {
        chrome.remember(crate::ui::layout::UiWindow::Chat, response.response.rect);
    }
}
