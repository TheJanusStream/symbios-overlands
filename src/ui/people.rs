//! Room roster window. Lists the signed-in user plus every remote peer
//! currently in the room, with a per-peer Mute toggle. The mute flag writes
//! straight into `RemotePeer.muted`; audio-mix / visibility code keys off the
//! same component. Diagnostics still renders its own copy of the roster
//! (with DIDs) — this window is the user-facing social view, Diagnostics is
//! the debug view.
//!
//! Drag-to-gift: peer rows double as drop targets for the inventory /
//! world-editor drag. While a generator drag is armed, hovering a peer row
//! writes a [`crate::ui::inventory::PeerDropTarget`] into
//! [`crate::ui::inventory::PendingGeneratorDrop`]; releasing there routes
//! the drop into an [`crate::protocol::OverlandsMessage::ItemOffer`]
//! instead of a ground placement. See [`crate::ui::inventory::handle_generator_drop`].
//!
//! Incoming offer modal: when [`crate::state::IncomingOfferDialog`] is set,
//! [`incoming_offer_ui`] renders the Accept / Decline / Mute & Decline
//! prompt. Exactly one dialog is ever active — concurrent offers are
//! auto-declined with "busy" at the network layer, see
//! [`crate::network`].

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::*;

use crate::avatar::{BskyProfileCache, draw_avatar_icon};
use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::InventoryRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{
    IncomingOfferDialog, LiveInventoryRecord, PublishFeedback, PublishStatus, RemotePeer,
    SocialResonance,
};
use crate::ui::chat::AVATAR_ICON_PX;
use crate::ui::inventory::{PeerDropTarget, PendingGeneratorDrop, choose_inventory_gift_key};

/// Log a `PeerMuteToggled` event (#635b). Shared by the three mute controls
/// (this roster panel, the diagnostics-panel roster, and the offer dialog) so
/// the event's shape can't drift between them. Call it only inside the
/// change-guard, so both mute *and* unmute are captured and a no-op write logs
/// nothing.
pub(crate) fn log_peer_mute_toggled(
    session_log: &mut crate::diagnostics::SessionLog,
    now: f64,
    peer: String,
    muted: bool,
) {
    session_log.info(
        now,
        crate::diagnostics::event::EventPayload::PeerMuteToggled { peer, muted },
    );
}

#[allow(clippy::too_many_arguments)]
pub fn people_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    session: Option<Res<AtprotoSession>>,
    mut peers: Query<(&mut RemotePeer, Option<&SocialResonance>)>,
    profile_cache: Res<BskyProfileCache>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    pending_offers: Res<crate::state::PendingOutgoingOffers>,
    mut muted_dids: ResMut<crate::state::MutedDids>,
    mut commands: Commands,
    current_room: Option<Res<crate::state::CurrentRoomDid>>,
    traveling: Option<Res<crate::state::TravelingTo>>,
    guard: Option<Res<crate::ui::unsaved_guard::UnsavedGuard>>,
) {
    let now = time.elapsed_secs_f64();

    // Drag-to-gift hover snapshot lives in `pending_drop.peer_target`. We
    // rebuild it from scratch each frame because a peer that was hovered
    // last frame may no longer be under the cursor this frame; leaving the
    // stale value in place would let the drop handler target a peer the
    // user had already moved away from. Only overwrite when a drag is
    // armed, so the resource is untouched outside a drag (and a later
    // non-drag interaction can't accidentally poke it).
    let drag_active = pending_drop.generator_name.is_some();
    if drag_active {
        pending_drop.peer_target = None;
    }

    let ctx = contexts.ctx_mut().unwrap();
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::People, ctx);
    let response = egui::Window::new("People")
        .open(&mut panels.people)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(ctx.available_rect())
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let peer_count = peers.iter().count();
            let total = peer_count + session.is_some() as usize;
            ui.label(format!("In room ({})", total));
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([true, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    // Self entry at the top. Same blue dot the chat uses
                    // for the local author tag, so the visual "you" cue
                    // carries across both windows. No Mute button on self.
                    if let Some(s) = session.as_deref() {
                        // Same info-blue the chat uses for the local
                        // author tag (#856) — the "you" cue carries across.
                        let self_color = crate::ui::theme::current(ui.ctx()).status.info;
                        ui.horizontal(|ui| {
                            crate::ui::affordances::status_dot(ui, self_color);
                            draw_avatar_icon(
                                ui,
                                Some(s.did.as_str()),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            ui.monospace(format!("@{} (you)", s.handle));
                        });
                    }

                    // Remote peers. Handshake-in-progress peers show as
                    // "identifying…" so their presence is visible before
                    // the handle resolves.
                    //
                    // Sorted (#844): bare query iteration follows archetype
                    // order, so rows JUMPED when `SocialResonance` resolved
                    // (a component insert moves the entity). Mutuals first,
                    // then case-insensitive handle — the same deliberate
                    // order the gateway picker uses; a stable list also
                    // de-risks drag-to-gift aim.
                    let mut rows: Vec<_> = peers.iter_mut().collect();
                    rows.sort_by_key(|(peer, resonance)| {
                        (
                            !matches!(resonance, Some(SocialResonance::Mutual)),
                            peer.handle.as_deref().unwrap_or("~").to_lowercase(),
                        )
                    });
                    for (mut peer, resonance) in rows {
                        let handle = peer.handle.as_deref().unwrap_or("identifying…").to_owned();
                        let th = crate::ui::theme::current(ui.ctx());
                        let dot_color = if peer.muted { th.text_faint } else { th.status.ok };
                        let mut muted = peer.muted;

                        // Render the row inside an egui Response we can
                        // interrogate for hover state so we can highlight
                        // valid drop targets while a drag is armed. Peers
                        // without a resolved DID cannot receive offers
                        // (the recipient authenticates by DID), so their
                        // row stays inert for the drag.
                        let can_receive_gift = drag_active && !peer.muted && peer.did.is_some();
                        let row = ui.horizontal(|ui| {
                            crate::ui::affordances::status_dot(ui, dot_color);
                            draw_avatar_icon(
                                ui,
                                peer.did.as_deref(),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            // A peer the local user mutually follows gets
                            // the shared warm-gold name + a ★ so the cue
                            // also survives a colour-blind / greyscale
                            // viewer. `SocialResonance` is absent until the
                            // async getRelationships query lands; treat
                            // missing / Unknown / None as "not a mutual".
                            if matches!(resonance, Some(SocialResonance::Mutual)) {
                                // Accent, not gold (#856): the old
                                // (240,190,70) star sat in the warn-amber
                                // family — a friend must not read as a
                                // caution. Brand highlight = accent.
                                ui.colored_label(
                                    crate::ui::theme::current(ui.ctx()).accent,
                                    egui::RichText::new(format!("★ @{handle}")).monospace(),
                                )
                                .on_hover_text("You and this peer follow each other");
                            } else {
                                ui.monospace(format!("@{}", handle));
                            }
                            // Outgoing-gift badge (#843): while an offer to
                            // this peer awaits their answer, say so on the
                            // row — the sender used to have no trace at all.
                            let offers_pending = peer
                                .did
                                .as_deref()
                                .map(|did| {
                                    pending_offers
                                        .by_id
                                        .values()
                                        .filter(|o| o.target_did == did)
                                        .count()
                                })
                                .unwrap_or(0);
                            if offers_pending > 0 {
                                let text = if offers_pending == 1 {
                                    "🎁 offer pending".to_owned()
                                } else {
                                    format!("🎁 {offers_pending} offers pending")
                                };
                                ui.label(
                                    egui::RichText::new(text).small().color(crate::ui::theme::current(ui.ctx()).text_weak),
                                )
                                .on_hover_text("Waiting for this peer to accept or decline");
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut muted, "Mute").on_hover_text(
                                        "Hides their avatar, chat, audio and gift \
                                         offers. Persists across sessions.",
                                    );
                                    // "Meet someone → visit their overland"
                                    // finally has a UI path (#845). Routed
                                    // through the unsaved-edits guard exactly
                                    // like gateway travel; `target_pos: None`
                                    // arrives at their default landing. Not
                                    // offered for muted peers, unresolved
                                    // DIDs, the room we're already in, or
                                    // while a travel/guard is in flight.
                                    let already_here = peer.did.as_deref().is_some_and(|did| {
                                        current_room.as_deref().is_some_and(|room| room.0 == did)
                                    });
                                    if !peer.muted
                                        && !already_here
                                        && traveling.is_none()
                                        && guard.is_none()
                                        && let Some(did) = peer.did.as_deref()
                                        && ui
                                            .small_button("Visit")
                                            .on_hover_text(format!(
                                                "Travel to @{handle}'s overland"
                                            ))
                                            .clicked()
                                    {
                                        commands.insert_resource(
                                            crate::ui::unsaved_guard::UnsavedGuard::new(
                                                crate::ui::unsaved_guard::GuardedAction::PortalTravel {
                                                    target_did: did.to_owned(),
                                                    target_pos: None,
                                                },
                                            ),
                                        );
                                    }
                                },
                            );
                        });
                        let row_rect = row.response.rect;
                        let hovered = can_receive_gift && ui.rect_contains_pointer(row_rect);
                        if hovered {
                            // Soft highlight so the user has visual
                            // feedback that a release here is a gift and
                            // not a mis-click. Painted on the foreground
                            // layer so it overlays the default row bg
                            // without disturbing text layout.
                            ui.painter().rect_filled(
                                row_rect,
                                4.0,
                                {
                                    let a = crate::ui::theme::current(ui.ctx()).accent;
                                    egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 40)
                                },
                            );
                            if let Some(did) = peer.did.clone() {
                                pending_drop.peer_target = Some(PeerDropTarget {
                                    peer_id: peer.peer_id,
                                    did,
                                    handle: handle.clone(),
                                });
                            }
                        }

                        // Guard the write so Bevy's change-detection flag is
                        // only raised when the mute state actually flips —
                        // an unconditional assignment would mark
                        // `RemotePeer` as `Changed` every frame and
                        // invalidate any `Changed<RemotePeer>` filter
                        // downstream.
                        if peer.muted != muted {
                            peer.muted = muted;
                            // Mirror into the durable DID-keyed list (#844)
                            // so the mute survives reconnects and relogs.
                            if let Some(did) = peer.did.as_deref() {
                                muted_dids.set(did, muted);
                            }
                            log_peer_mute_toggled(
                                &mut session_log,
                                now,
                                peer.peer_id.to_string(),
                                muted,
                            );
                        }
                    }

                    if peer_count == 0 && session.is_none() {
                        ui.colored_label(crate::ui::theme::current(ui.ctx()).text_weak, "(empty)");
                    } else if peer_count == 0 {
                        ui.colored_label(crate::ui::theme::current(ui.ctx()).text_weak, "(no other peers)");
                    }
                });
        });
    if let Some(response) = response {
        chrome.remember(crate::ui::layout::UiWindow::People, response.response.rect);
    }
}

/// Renders the incoming-offer modal when [`IncomingOfferDialog`] is set
/// and drives the Accept / Decline / Mute & Decline actions. On accept,
/// the item is copied into the owner's live inventory under a
/// collision-safe key (see [`choose_inventory_gift_key`]) and a publish
/// task is spawned immediately so the new item is on the PDS before the
/// user closes the window — the user explicitly opted into "auto-publish
/// on accept" for less-likely-to-lose-items behaviour.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn incoming_offer_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    dialog: Option<Res<IncomingOfferDialog>>,
    mut live_inventory: Option<ResMut<LiveInventoryRecord>>,
    stored_inventory: Option<Res<crate::state::StoredInventoryRecord>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut peers: Query<&mut RemotePeer>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut session_log: ResMut<SessionLog>,
    mut inventory_feedback: ResMut<PublishFeedback<InventoryRecord>>,
    // Bundled to stay under Bevy's 16-parameter ceiling (#843/#844).
    (time, mut metrics, mut busy_declines, mut toasts, mut offer_size, mut muted_dids): (
        Res<Time>,
        ResMut<crate::diagnostics::MetricsRegistry>,
        ResMut<crate::state::BusyAutoDeclines>,
        ResMut<crate::ui::toast::Toasts>,
        Local<Option<(u64, Option<usize>)>>,
        ResMut<crate::state::MutedDids>,
    ),
) {
    let Some(dialog) = dialog else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut action: Option<OfferAction> = None;
    // A true `egui::Modal`, matching the unsaved-edits guard — the
    // app's one modality pattern (#834). It always paints topmost and
    // blocks background input, so it can never end up buried under the
    // (previously also center-anchored) Controls sheet with its
    // buttons unreachable.
    let modal = egui::Modal::new(egui::Id::new("incoming-item-offer")).show(ctx, |ui| {
        ui.heading("Incoming item offer");
        ui.add_space(4.0);
        ui.label(format!(
            "@{} wants to gift you \"{}\".",
            dialog.sender_handle, dialog.item_name
        ));
        ui.monospace(
            egui::RichText::new(&dialog.sender_did)
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        // What's actually being offered (#843): kind + rough serialized
        // size. The generator arrives decoded + sanitized before the
        // dialog opens; the size is measured once per offer (cached by
        // offer_id — serializing per frame would be wasted work).
        let bytes = match *offer_size {
            Some((id, bytes)) if id == dialog.offer_id => bytes,
            _ => {
                let bytes = crate::pds::record_size::serialized_record_bytes(&dialog.generator);
                *offer_size = Some((dialog.offer_id, bytes));
                bytes
            }
        };
        let size_text = bytes
            .map(crate::pds::record_size::human_bytes)
            .unwrap_or_else(|| "size unknown".to_owned());
        ui.label(
            egui::RichText::new(format!("{} · {}", dialog.generator.kind_tag(), size_text))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        ui.separator();
        if let Some(live) = live_inventory.as_deref() {
            let cap = crate::config::state::MAX_INVENTORY_ITEMS;
            let len = live.0.generators.len();
            ui.label(format!("Your stash: {len}/{cap}"));
            if len >= cap {
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.error,
                    "Inventory full — remove an item to accept.",
                );
                if ui.button("Open Inventory").clicked() {
                    panels.inventory = true;
                }
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let can_accept = live_inventory
                .as_deref()
                .map(|l| l.0.generators.len() < crate::config::state::MAX_INVENTORY_ITEMS)
                .unwrap_or(false);
            if ui
                .add_enabled(
                    can_accept,
                    egui::Button::new(
                        egui::RichText::new("Accept")
                            .color(crate::ui::theme::current(ui.ctx()).status.ok),
                    ),
                )
                .clicked()
            {
                action = Some(OfferAction::Accept);
            }
            if ui.button("Decline").clicked() {
                action = Some(OfferAction::Decline);
            }
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Mute & Decline")
                        .color(crate::ui::theme::current(ui.ctx()).status.error),
                ))
                .clicked()
            {
                action = Some(OfferAction::MuteAndDecline);
            }
        });
        ui.add_space(4.0);
        // The lifecycle sweep auto-declines the dialog after the TTL
        // (`config::network::OFFER_DIALOG_TIMEOUT_SECS`) — surface that
        // instead of letting the offer vanish invisibly mid-decision.
        let remaining = (crate::config::network::OFFER_DIALOG_TIMEOUT_SECS
            - (time.elapsed_secs_f64() - dialog.arrived_at_secs))
            .max(0.0)
            .ceil() as u64;
        ui.small(format!(
            "Declines automatically in {remaining}s — Esc to decline now."
        ));
    });
    // Esc (or a click on the dimmed backdrop) = Decline: the safe,
    // non-destructive dismissal — the sender gets an honest response
    // instead of a dialog that lingers until the TTL sweep.
    if action.is_none() && modal.should_close() {
        action = Some(OfferAction::Decline);
    }

    let Some(action) = action else {
        return;
    };

    let now = time.elapsed_secs_f64();
    // The dialog is closing (#843): report offers the busy-gate silently
    // turned away while the user decided, then reset for the next one.
    if busy_declines.0 > 0 {
        toasts.info(
            format!(
                "{} more offer{} arrived while you decided and {} auto-declined.",
                busy_declines.0,
                if busy_declines.0 == 1 { "" } else { "s" },
                if busy_declines.0 == 1 { "was" } else { "were" },
            ),
            now,
        );
        busy_declines.0 = 0;
    }
    let accepted = matches!(action, OfferAction::Accept);
    // Count the local user's offer disposition (E-4) — accept vs any decline.
    if accepted {
        crate::diagnostics::samplers::offer_accepted(&mut metrics);
    } else {
        crate::diagnostics::samplers::offer_declined(&mut metrics);
    }

    // Flip the mute flag on the sender's `RemotePeer` before we send the
    // response so any subsequent offer this frame (unlikely but possible
    // if the attacker double-sent) is already auto-declined as muted.
    if matches!(action, OfferAction::MuteAndDecline) {
        for mut peer in peers.iter_mut() {
            if peer.peer_id == dialog.sender_peer_id && !peer.muted {
                peer.muted = true;
                // Durable DID-keyed mute (#844). The dialog's sender DID is
                // relay-authenticated, so it is safe to key on even if the
                // peer entity's own `did` hasn't resolved yet.
                muted_dids.set(&dialog.sender_did, true);
                log_peer_mute_toggled(&mut session_log, now, peer.peer_id.to_string(), true);
                break;
            }
        }
    }

    if accepted {
        if let Some(live) = live_inventory.as_mut() {
            let key = choose_inventory_gift_key(&live.0.generators, &dialog.item_name);
            live.0.generators.insert(key, dialog.generator.clone());
            session_log.info(
                now,
                EventPayload::ItemOfferUserResponded {
                    offer_id: dialog.offer_id,
                    accepted: true,
                },
            );

            // Auto-publish the updated inventory immediately. The user
            // explicitly chose "publish on accept" over "mark dirty" so
            // accepted items are persistent even if the session ends
            // before they click the Inventory's Publish button. The
            // `poll_publish_inventory_tasks` system (already in the
            // Update schedule) drains the task and flips
            // `StoredInventoryRecord` + `PublishFeedback<InventoryRecord>`
            // on completion, so we only kick off the I/O here.
            if let (Some(sess), Some(refresh)) = (session.as_deref(), refresh_ctx.as_deref()) {
                inventory_feedback.status = PublishStatus::Publishing;
                crate::ui::inventory::spawn_publish_inventory_task(
                    &mut commands,
                    sess,
                    refresh,
                    live.0.clone(),
                    stored_inventory
                        .as_deref()
                        .map(|s| s.0.clone())
                        .unwrap_or_default(),
                    time.elapsed_secs_f64(),
                );
            }
        } else {
            // Live inventory resource absent — should not happen in
            // `AppState::InGame`, but decline rather than drop the
            // response and leave the sender hanging. The user's response was
            // an accept, so it records as such but at Warn severity because
            // the item could not actually be stored.
            warn!(
                "Could not store accepted offer \"{}\" from @{}: inventory not loaded",
                dialog.item_name, dialog.sender_handle
            );
            session_log.warn(
                now,
                EventPayload::ItemOfferUserResponded {
                    offer_id: dialog.offer_id,
                    accepted: true,
                },
            );
        }
    } else {
        session_log.info(
            now,
            EventPayload::ItemOfferUserResponded {
                offer_id: dialog.offer_id,
                accepted: false,
            },
        );
    }

    // Fire the response back to the sender. Broadcast-with-address: the
    // `target_did` field is the *sender's* DID so only they pick it up.
    writer.write(Broadcast {
        payload: OverlandsMessage::ItemOfferResponse {
            offer_id: dialog.offer_id,
            target_did: dialog.sender_did.clone(),
            accepted,
        },
        channel: ChannelKind::Reliable,
    });

    commands.remove_resource::<IncomingOfferDialog>();
}

#[derive(Clone, Copy)]
enum OfferAction {
    Accept,
    Decline,
    MuteAndDecline,
}
