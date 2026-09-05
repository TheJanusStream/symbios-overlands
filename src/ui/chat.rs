//! In-game chat window.
//!
//! Renders `ChatHistory` into a scroll area and exposes a single-line input
//! that broadcasts `OverlandsMessage::Chat` over the Reliable channel.  The
//! sender holds the typist to `MAX_MESSAGE_CHARS` while the receiver
//! enforces `MAX_MESSAGE_BYTES` on whatever actually arrives, so a
//! misbehaving peer who bypasses this UI still gets its payload clipped on
//! every other client in the room. Two units on purpose (#1264 f362): the
//! wire limit exists to bound rendering cost and the composer limit exists
//! to be fair to every script, and a single byte cap could not be both.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::{BskyProfileCache, draw_avatar_icon};
use crate::network::presence::PeerLabel;
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

/// Whether a chat row's author is muted (#1219 f130).
///
/// Two sources, because they answer different questions: `live` is every
/// peer in the room whose `RemotePeer::muted` is set, and `durable` is the
/// DID-keyed list that outlives them — a harasser who has already left, or
/// one muted in a previous session, is not in the room to carry a flag but
/// their words are still in the scrollback.
fn is_muted(
    did: &str,
    live: &std::collections::HashSet<String>,
    durable: &crate::state::MutedDids,
) -> bool {
    live.contains(did) || durable.0.contains(did)
}

/// The name to print for a chat row right now (#1218 f300).
///
/// [`crate::state::ChatEntry::author`] is stamped once, at push time, from
/// whatever was known then — so a message that beat the sender's
/// `getProfile` into the room kept its fallback name for the rest of the
/// scrollback and one speaker appeared under two names in one conversation.
/// The entry's DID is the stable identity, so the label is re-derived from
/// the live peer set every frame; the stamped author remains the floor for a
/// speaker who has since left the room, and for the system and local rows,
/// which carry no peer.
fn author_now<'a>(
    entry: &'a crate::state::ChatEntry,
    names: &'a std::collections::HashMap<String, String>,
) -> &'a str {
    entry
        .did
        .as_deref()
        .and_then(|did| names.get(did))
        .map(String::as_str)
        .unwrap_or(entry.author.as_str())
}

/// The composer's remaining-length readout, or `None` while the limit is
/// far enough away not to be worth saying (#1264 f362).
///
/// A counter that is always on is noise on every ordinary message; one
/// that appears only at the moment of amputation is the defect this
/// closes, restated. So it arrives with a fifth of the budget left, which
/// is enough warning to finish a sentence or start trimming one.
///
/// Counted in characters, like the limit itself — the whole point is that
/// a CJK writer sees the same number of characters as everyone else.
fn composer_counter(draft: &str) -> Option<String> {
    let max = crate::config::ui::chat::MAX_MESSAGE_CHARS;
    let used = draft.chars().count();
    (used * 5 >= max * 4).then(|| format!("{used}/{max}"))
}

/// Everything the chat window reads that is not the conversation itself.
///
/// Bundled for the same reason `people::RosterDeps` is (#1223): the window
/// was at 14 parameters and the per-row mute action (#1222 f296) needs the
/// mute list, the session log and a clock. Bevy's ceiling is 16 and an
/// over-ceiling system fails at app build with a trait error that names
/// none of this.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ChatDeps<'w> {
    profile_cache: Res<'w, BskyProfileCache>,
    link: Res<'w, crate::network::LinkState>,
    muted_dids: ResMut<'w, crate::state::MutedDids>,
    session_log: ResMut<'w, crate::diagnostics::SessionLog>,
    time: Res<'w, Time>,
}

#[allow(clippy::too_many_arguments)]
pub fn chat_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut focus_request: ResMut<ChatFocusRequest>,
    mut was_open: Local<bool>,
    session: Option<Res<AtprotoSession>>,
    mut chat: ResMut<ChatHistory>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut peers: Query<(&mut RemotePeer, Option<&SocialResonance>)>,
    local: Query<Entity, With<crate::state::LocalPlayer>>,
    mut emotes: MessageWriter<crate::player::emote::EmoteRequest>,
    mut deps: ChatDeps,
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
    // Owned, not borrowed: the render loop below can raise a mute, which
    // needs `peers` mutably, and a set holding `&str` into the query would
    // keep the immutable borrow alive across it.
    let mutual_dids: std::collections::HashSet<String> = peers
        .iter()
        .filter(|(_, r)| matches!(r, Some(SocialResonance::Mutual)))
        .filter_map(|(p, _)| p.did.clone())
        .collect();

    // Everyone the user has muted, live flag OR durable list (#1219 f130).
    // The inbound filter honours "hides their chat" only for messages that
    // arrive AFTER the flip — so the moment a user reaches for mute, the
    // abuse that made them reach for it is still sitting on screen. Filtering
    // at RENDER makes the mute retroactive and the unmute non-destructive:
    // nothing is deleted, so unticking the box brings the history back.
    let muted_here: std::collections::HashSet<String> = peers
        .iter()
        .filter(|(peer, _)| peer.muted)
        .filter_map(|(peer, _)| peer.did.clone())
        .collect();

    // DIDs whose relationship query could not be answered (#1218 f297) —
    // rendered as a neutral "?" rather than as the un-highlighted state a
    // genuine non-mutual gets, which is what the failure used to look like.
    let unknown_dids: std::collections::HashSet<String> = peers
        .iter()
        .filter(|(_, r)| matches!(r, Some(SocialResonance::Failed)))
        .filter_map(|(p, _)| p.did.clone())
        .collect();

    // DID → the name that peer goes by RIGHT NOW (#1218 f300).
    //
    // `ChatEntry.author` is stamped once, at push time, from whatever was
    // known then — so a message that arrived before `getProfile` landed kept
    // its fallback name for the rest of the scrollback and one speaker
    // appeared under two names in one conversation. The entry's DID is the
    // stable identity; the label is derived from it every frame, off the
    // same ladder every other surface uses. The stored `author` remains the
    // floor for a peer who has since left.
    let names: std::collections::HashMap<String, String> = peers
        .iter()
        .filter_map(|(peer, _)| {
            let did = peer.did.clone()?;
            let label = PeerLabel::new(peer.handle.as_deref(), Some(&did)).name();
            Some((did, label))
        })
        .collect();

    // What would become of a message sent right now (#1213). Upstream's
    // `transmit_messages` drains the broadcast reader and returns without
    // sending when `connected_peers()` is empty, and the whole transmit
    // chain is `run_if(resource_exists::<MatchboxSocket>)` — so with no
    // peers, or no socket, a send is dropped and nothing feeds that back.
    // Both facts are decided in ONE place so the note above the input and
    // the suffix on the pushed line cannot tell different stories.
    let peer_count = peers.iter().count();
    let delivery = deps.link.delivery(peer_count);

    // The half-typed line lives on `ChatHistory` (#1140), not in a
    // `Local<String>`: a Local is unreachable from every teardown path, so
    // a draft typed before logout was still sitting in the box for whoever
    // logged in next. Worked on a frame-local copy and written back only
    // when it actually changed — the guarded-dirty rule (#879), so an open
    // Chat window does not flag the resource every frame.
    let mut input = chat.draft.clone();
    let mut cleared = false;
    // The DID a row's context menu asked to mute (#1222 f296). Applied
    // after the window closes: the render loop holds `chat.messages`
    // immutably and the write needs `peers` mutably.
    let mut mute_request: Option<String> = None;

    let ctx = contexts.ctx_mut().unwrap();
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Chat, ctx);
    // Guarded-dirty (#879): `.open(&mut panels.chat)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce — local copy in, write back only on the ✕
    // click (the Settings window's idiom).
    let mut open = panels.chat;
    let response = egui::Window::new("Chat")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(chrome.available_rect(ctx))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // The footer is laid out FIRST, bottom-up, so its height is
            // MEASURED rather than guessed, and the scrollback gets exactly
            // what is left. This used to reserve a constant 44 pt for
            // "the separator + input row" and hand the scroll area
            // `available_height() - 44` with `auto_shrink([true, false])`,
            // which claims that height whether or not the content fills it.
            // Two lines have been added below the input since — #1141's
            // emote hint and #1213's composer note — so the measured content
            // ran taller than the window every frame, and egui's `Resize`
            // never shrinks on its own: the window climbed to the full
            // screen height within a second of being opened (#1280). The
            // footer keeps ordinary reading order and is anchored by an
            // `egui::Panel::bottom` (#1282, #1285).
            crate::ui::layout::footer(ui, "chat_footer", |ui| {
                // Right-to-left layout: Send first (pinned to the right edge),
                // then the TextEdit whose `desired_width` is set to whatever
                // horizontal space remains — so widening the window stretches
                // the field instead of leaving dead space beside it.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let send = ui.button("Send");
                        // The rest of what mute cannot reach (#1219 f130): a
                        // history that only rolls off after 500 entries, which in
                        // a quiet room is a very long time to sit with something
                        // you did not want to read.
                        if ui
                            .button("Clear")
                            .on_hover_text("Empty this window. Nobody else is affected.")
                            .clicked()
                        {
                            cleared = true;
                        }
                        // The remaining-length readout (#1264 f362),
                        // between Clear and the field so it sits
                        // against the right-hand controls. Silent
                        // until the limit is close enough to matter —
                        // a counter on every message would be noise
                        // on the 99% of lines nowhere near it — and
                        // tinted once there is nothing left.
                        if let Some(count) = composer_counter(&input) {
                            let th = crate::ui::theme::current(ui.ctx());
                            let colour = if input.chars().count() >= cfg::MAX_MESSAGE_CHARS {
                                th.status.warn
                            } else {
                                th.text_weak
                            };
                            ui.colored_label(colour, count);
                        }
                        let response = crate::ui::affordances::text_edit(
                            ui,
                            egui::TextEdit::singleline(&mut input)
                                // The limit made visible before it is
                                // hit, rather than as an amputation
                                // after Send (#1264 f362).
                                .char_limit(cfg::MAX_MESSAGE_CHARS)
                                .desired_width(ui.available_width()),
                        );
                        // Global Enter shortcut (#836): consume the one-shot
                        // focus request so typing starts immediately.
                        if focus_request.0 {
                            response.request_focus();
                            focus_request.0 = false;
                        }
                        // Enter through the shared IME guard (#1263
                        // f372): under an input method the first
                        // Enter accepts the candidate, and this room
                        // has no edit and no delete for what it
                        // would otherwise have sent.
                        let submit =
                            send.clicked() || crate::ui::shortcuts::enter_submitted(ui, &response);

                        if submit && !input.trim().is_empty() {
                            // Enforce a strict per-message length cap *before*
                            // the text is broadcast. Otherwise a peer could
                            // paste an 800 KiB junk string (well under the 1
                            // MiB packet limit) and every guest would try to
                            // word-wrap it in egui on every frame — an instant
                            // room-wide DoS.
                            //
                            // CHARACTERS, not bytes (#1264 f362): the
                            // old cap gave a CJK writer a third of
                            // everyone else's message length. The
                            // field's `char_limit` below means this
                            // clip cannot fire on anything a person
                            // typed or pasted into it; it stays as
                            // the invariant's enforcement, because
                            // what is broadcast must be what the
                            // limit says however the draft got here.
                            let trimmed = input.trim();
                            let clipped: String =
                                trimmed.chars().take(cfg::MAX_MESSAGE_CHARS).collect();
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
                            // Stamped with the delivery outcome resolved above
                            // (#1213) — the sender's HUD used to render a
                            // message that reached nobody exactly like one that
                            // was delivered.
                            chat.push_sent(did, author, text.clone(), delivery);

                            // Chat-keyword emotes (#1068): my own body plays what
                            // I just said, exactly as every peer's does. Without
                            // this the sender is the one person in the room who
                            // never sees their own gesture, which reads as the
                            // feature being broken rather than as an omission.
                            // Same `request_for` the inbound path uses, so the two
                            // cannot drift.
                            if let Ok(chassis) = local.single()
                                && let Some(request) =
                                    crate::player::emote::request_for(chassis, &text)
                            {
                                emotes.write(request);
                            }

                            writer.write(Broadcast {
                                payload: OverlandsMessage::Chat { text },
                                channel: ChannelKind::Reliable,
                            });
                        }
                    });
                });

                // The persistent "this is going nowhere" note (#1213). Above
                // the input, not a toast: it is a standing condition, and the
                // moment it matters is the moment before the user types.
                if let Some(note) = deps.link.composer_note(peer_count) {
                    ui.colored_label(crate::ui::theme::current(ui.ctx()).status.warn, note);
                }

                // The keyword emotes have no command syntax to discover and,
                // until this line, no surface anywhere in the UI (#1141) —
                // #1068 shipped a feature findable only by typing one of its
                // words by accident. Sourced from the keyword table so the
                // examples cannot name a word that no longer gestures.
                ui.small(crate::player::emote::Emote::hint_line());
            });

            crate::ui::layout::fill_above(ui, |ui| {
                // No `max_height`: `fill_above` already handed us
                // exactly the space the footer left, and setting one
                // here is what put the guess back (#1280).
                egui::ScrollArea::vertical()
                    .id_salt("chat_scroll")
                    .auto_shrink([true, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &chat.messages {
                            if entry
                                .did
                                .as_deref()
                                .is_some_and(|did| is_muted(did, &muted_here, &deps.muted_dids))
                            {
                                continue;
                            }
                            ui.horizontal_wrapped(|ui| {
                                // Local wall-clock HH:MM (#846) — the old stamp
                                // was minutes-since-app-launch, meaningless
                                // across peers and sessions.
                                ui.colored_label(
                                    crate::ui::theme::current(ui.ctx()).text_weak,
                                    format!("[{}]", crate::state::clock_hhmm(entry.at_epoch_secs)),
                                );
                                let author = author_now(entry, &names);
                                // Profile icon by DID, or a same-sized tile
                                // carrying the author's initial (#1225 f351) so
                                // the row layout doesn't shift between
                                // cache-miss and cache-hit frames AND the miss
                                // still says whose row this is.
                                draw_avatar_icon(
                                    ui,
                                    entry.did.as_deref(),
                                    Some(author),
                                    &deps.profile_cache,
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
                                let unknown = entry
                                    .did
                                    .as_deref()
                                    .is_some_and(|d| unknown_dids.contains(d));
                                let (tag_color, tag_text) = if is_mutual {
                                    (th.accent, format!("★ [{author}]"))
                                } else if unknown {
                                    (th.status.info, format!("? [{author}]"))
                                } else {
                                    (th.status.info, format!("[{author}]"))
                                };
                                // The author tag is the mute affordance (#1222
                                // f296). The remedy for a flood used to be two
                                // windows away — leave the chat, open People,
                                // find the row among a dozen, tick a box — and it
                                // arrived after the damage was permanent. The
                                // action belongs on the message in front of you.
                                // Own lines are not offered it: you are not a
                                // peer, and muting yourself is not a thing.
                                let can_mute = entry.did.as_deref().is_some_and(|did| {
                                    session.as_deref().is_none_or(|s| s.did != did)
                                });
                                let tag = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(tag_text).color(tag_color),
                                    )
                                    .sense(if can_mute {
                                        egui::Sense::click()
                                    } else {
                                        egui::Sense::hover()
                                    }),
                                );
                                let tag = if is_mutual {
                                    tag.on_hover_text("You and this peer follow each other")
                                } else if unknown {
                                    tag.on_hover_text(
                                        "Couldn't check whether you follow each other. \
                                             Trying again shortly.",
                                    )
                                } else if can_mute {
                                    tag.on_hover_text("Right-click to mute this person")
                                } else {
                                    tag
                                };
                                if can_mute {
                                    tag.context_menu(|ui| {
                                        if ui
                                            .button(format!("Mute {author}"))
                                            .on_hover_text(
                                                "Hides their avatar, chat, audio and gift \
                                                     offers — including what they have already \
                                                     said. Persists across sessions.",
                                            )
                                            .clicked()
                                            && let Some(did) = entry.did.clone()
                                        {
                                            mute_request = Some(did);
                                            ui.close();
                                        }
                                    });
                                }
                                ui.label(&entry.text);
                                // A line of ours that reached nobody says so,
                                // in weak text so a normal conversation is not
                                // visually noisy (#1213).
                                if let Some(suffix) = entry.delivery.suffix() {
                                    ui.colored_label(
                                        crate::ui::theme::current(ui.ctx()).text_weak,
                                        suffix,
                                    );
                                }
                            });
                        }
                    });
            });
        });
    if chat.draft != input {
        chat.draft = input;
    }
    if cleared {
        chat.messages.clear();
        chat.unread = 0;
    }
    if let Some(did) = mute_request {
        // Through the ONE funnel (#1219), so this third control cannot
        // drift from the roster checkbox and the offer dialog: it owns the
        // change guard, the durable DID-keyed list and the log line. The
        // speaker may have left the room, in which case only the durable
        // half lands — which is exactly right, and exactly what the
        // hit-and-run case needs.
        let live = peers
            .iter_mut()
            .find(|(peer, _)| peer.did.as_deref() == Some(did.as_str()));
        let peer_id = live.as_ref().map(|(peer, _)| peer.peer_id);
        crate::network::presence::set_peer_mute(
            live.map(|(peer, _)| peer.into_inner()),
            Some(did.as_str()),
            true,
            &mut deps.muted_dids,
            &mut deps.session_log,
            peer_id,
            deps.time.elapsed_secs_f64(),
        );
    }
    if let Some(response) = response {
        chrome.remember(crate::ui::layout::UiWindow::Chat, response.response.rect);
    }
    if panels.chat && !open {
        panels.chat = false;
    }
}

#[cfg(test)]
mod author_tests {
    use super::*;
    use crate::network::ChatDelivery;
    use crate::state::ChatEntry;

    fn entry(did: Option<&str>, author: &str) -> ChatEntry {
        ChatEntry {
            did: did.map(str::to_owned),
            author: author.to_owned(),
            text: String::from("hi"),
            at_epoch_secs: 0,
            delivery: ChatDelivery::NotApplicable,
        }
    }

    /// #1218 f300. The sequence: someone's first two messages land before
    /// their `getProfile` does, so they were stamped with the fallback and
    /// their later ones with `@sam.bsky.social` — the same speaker under two
    /// names in one scrollback, neither matchable to a roster row.
    #[test]
    fn a_speaker_wears_one_name_for_the_whole_scrollback() {
        let mut names = std::collections::HashMap::new();
        names.insert(String::from("did:plc:sam"), String::from("sam.bsky.social"));

        let early = entry(Some("did:plc:sam"), "did:plc:sam…");
        let late = entry(Some("did:plc:sam"), "sam.bsky.social");
        assert_eq!(author_now(&early, &names), "sam.bsky.social");
        assert_eq!(author_now(&late, &names), "sam.bsky.social");
    }

    /// A speaker who has left the room has no live peer to resolve against;
    /// their rows keep the name they were stamped with rather than becoming
    /// anonymous.
    #[test]
    fn a_departed_speaker_keeps_the_name_they_were_stamped_with() {
        let names = std::collections::HashMap::new();
        let gone = entry(Some("did:plc:gone"), "gone.bsky.social");
        assert_eq!(author_now(&gone, &names), "gone.bsky.social");
    }

    /// The presence and system lines carry no DID at all and must be left
    /// alone.
    #[test]
    fn a_system_line_is_not_re_authored() {
        let names = std::collections::HashMap::new();
        assert_eq!(author_now(&entry(None, "system"), &names), "system");
    }

    /// #1219 f130. The sequence: you are being harassed, you tick Mute, and
    /// the abuse that made you reach for it is still sitting in the window —
    /// the inbound filter only ever applied to messages that had not arrived
    /// yet, and a 500-entry history rolls off very slowly in a quiet room.
    /// Filtering at render is also what makes an unmute non-destructive:
    /// nothing was deleted.
    #[test]
    fn muting_someone_hides_what_they_already_said() {
        let mut live = std::collections::HashSet::new();
        live.insert(String::from("did:plc:inroom"));
        let mut durable = crate::state::MutedDids::default();
        durable.0.insert(String::from("did:plc:leftalready"));

        assert!(is_muted("did:plc:inroom", &live, &durable));
        assert!(
            is_muted("did:plc:leftalready", &live, &durable),
            "a harasser who has already gone is not in the room to carry a \
             flag, but their words are still in the scrollback"
        );
        assert!(!is_muted("did:plc:friend", &live, &durable));

        // Unticking the box: the live flag goes and the row comes back.
        let empty = std::collections::HashSet::new();
        assert!(!is_muted(
            "did:plc:inroom",
            &empty,
            &crate::state::MutedDids::default()
        ));
    }
}

#[cfg(test)]
mod length_tests {
    use super::*;
    use crate::config::ui::chat as cfg;

    /// The wire ceiling can never clip a message the composer permitted
    /// (#1264 f362).
    ///
    /// The two limits are in different units on purpose — bytes bound
    /// peer-side rendering cost, characters are what a person is held to —
    /// and the whole arrangement only works if the byte one is the looser.
    /// Set the char limit above a quarter of the byte limit and a CJK
    /// sentence the composer accepted arrives amputated, which is the
    /// defect this closes, restored with extra steps.
    #[test]
    fn the_wire_ceiling_cannot_truncate_a_permitted_message() {
        const {
            assert!(cfg::MAX_MESSAGE_BYTES >= 4 * cfg::MAX_MESSAGE_CHARS);
        }

        // Demonstrated, not just asserted: the longest thing the composer
        // will hand over, in the widest encoding UTF-8 has.
        let widest: String = std::iter::repeat_n('\u{1F600}', cfg::MAX_MESSAGE_CHARS).collect();
        assert_eq!(widest.chars().count(), cfg::MAX_MESSAGE_CHARS);
        assert!(widest.len() <= cfg::MAX_MESSAGE_BYTES);

        // And the old cap really did clip it — the control, so this test
        // is not describing a coincidence.
        assert!(widest.len() > 512, "512 bytes was the old ceiling");
    }

    /// Every script gets the same message length (#1264 f362).
    ///
    /// The finding in one assertion: under a byte cap a Japanese sentence
    /// was worth a third of a Latin one.
    #[test]
    fn the_composer_limit_is_the_same_in_every_script() {
        for sample in ['a', '\u{3042}', '\u{05D0}', '\u{1F600}'] {
            let full: String = std::iter::repeat_n(sample, cfg::MAX_MESSAGE_CHARS).collect();
            let clipped: String = full.chars().take(cfg::MAX_MESSAGE_CHARS).collect();
            assert_eq!(
                clipped.chars().count(),
                cfg::MAX_MESSAGE_CHARS,
                "{sample:?} must get the full allowance"
            );
            // The old rule, for contrast: bytes, so anything above U+007F
            // lost most of the message.
            let old: usize = full.char_indices().take_while(|(i, _)| *i < 512).count();
            if sample.is_ascii() {
                assert_eq!(old, cfg::MAX_MESSAGE_CHARS);
            } else {
                // At most half, and for CJK a third: 512 bytes bought
                // 256 Hebrew or Arabic characters, 170 CJK, 128 emoji.
                assert!(old <= cfg::MAX_MESSAGE_CHARS / 2);
            }
        }
    }

    /// The counter appears with enough left to act on, and not before
    /// (#1264 f362).
    #[test]
    fn the_counter_arrives_before_the_limit_does() {
        assert_eq!(composer_counter(""), None);
        assert_eq!(composer_counter("hello"), None);

        // The threshold is a fifth remaining, so 410 of 512 is the first
        // draft that shows one.
        let near: String = "x".repeat(cfg::MAX_MESSAGE_CHARS * 4 / 5 + 1);
        assert_eq!(
            composer_counter(&near),
            Some(format!("{}/{}", near.len(), cfg::MAX_MESSAGE_CHARS))
        );

        let just_under: String = "x".repeat(cfg::MAX_MESSAGE_CHARS * 4 / 5);
        assert_eq!(just_under.len(), near.len() - 1);
        assert_eq!(
            composer_counter(&just_under),
            None,
            "quiet until it matters"
        );

        // Counted in characters, so a CJK draft at the same character
        // count shows the same number — the point of the whole change.
        let cjk: String =
            std::iter::repeat_n('\u{3042}', cfg::MAX_MESSAGE_CHARS * 4 / 5 + 1).collect();
        assert_eq!(composer_counter(&cjk), composer_counter(&near));
    }
}
