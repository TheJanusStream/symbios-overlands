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
use crate::pds::{InventoryRecord, publish_inventory_record};
use crate::protocol::OverlandsMessage;
use crate::state::{
    DiagnosticsLog, IncomingOfferDialog, InventoryPublishFeedback, LiveInventoryRecord, RemotePeer,
};
use crate::ui::chat::AVATAR_ICON_PX;
use crate::ui::inventory::{
    PeerDropTarget, PendingGeneratorDrop, PublishInventoryTask, choose_inventory_gift_key,
};

#[allow(clippy::too_many_arguments)]
pub fn people_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    mut peers: Query<&mut RemotePeer>,
    profile_cache: Res<BskyProfileCache>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
) {
    use crate::config::ui::people as cfg;

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
    egui::Window::new("People")
        .default_open(false)
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
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
                        let [r, g, b] = crate::config::ui::chat::AUTHOR_COLOR;
                        let self_color = egui::Color32::from_rgb(r, g, b);
                        ui.horizontal(|ui| {
                            ui.colored_label(self_color, "●");
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
                    for mut peer in peers.iter_mut() {
                        let handle = peer.handle.as_deref().unwrap_or("identifying…").to_owned();
                        let dot_color = if peer.muted {
                            egui::Color32::GRAY
                        } else {
                            egui::Color32::GREEN
                        };
                        let mut muted = peer.muted;

                        // Render the row inside an egui Response we can
                        // interrogate for hover state so we can highlight
                        // valid drop targets while a drag is armed. Peers
                        // without a resolved DID cannot receive offers
                        // (the recipient authenticates by DID), so their
                        // row stays inert for the drag.
                        let can_receive_gift = drag_active && !peer.muted && peer.did.is_some();
                        let row = ui.horizontal(|ui| {
                            ui.colored_label(dot_color, "●");
                            draw_avatar_icon(
                                ui,
                                peer.did.as_deref(),
                                &profile_cache,
                                AVATAR_ICON_PX,
                            );
                            ui.monospace(format!("@{}", handle));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut muted, "Mute");
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
                                egui::Color32::from_rgba_unmultiplied(80, 160, 255, 40),
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
                        }
                    }

                    if peer_count == 0 && session.is_none() {
                        ui.colored_label(egui::Color32::GRAY, "(empty)");
                    } else if peer_count == 0 {
                        ui.colored_label(egui::Color32::GRAY, "(no other peers)");
                    }
                });
        });
}

/// Renders the incoming-offer modal when [`IncomingOfferDialog`] is set
/// and drives the Accept / Decline / Mute & Decline actions. On accept,
/// the item is copied into the owner's live inventory under a
/// collision-safe key (see [`choose_inventory_gift_key`]) and a publish
/// task is spawned immediately so the new item is on the PDS before the
/// user closes the window — the user explicitly opted into "auto-publish
/// on accept" for less-likely-to-lose-items behaviour.
#[allow(clippy::too_many_arguments)]
pub fn incoming_offer_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    dialog: Option<Res<IncomingOfferDialog>>,
    mut live_inventory: Option<ResMut<LiveInventoryRecord>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut peers: Query<&mut RemotePeer>,
    mut writer: MessageWriter<Broadcast<OverlandsMessage>>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    mut inventory_feedback: ResMut<InventoryPublishFeedback>,
    time: Res<Time>,
) {
    let Some(dialog) = dialog else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut action: Option<OfferAction> = None;
    egui::Window::new("Incoming Item Offer")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "@{} wants to gift you \"{}\".",
                dialog.sender_handle, dialog.item_name
            ));
            ui.monospace(
                egui::RichText::new(&dialog.sender_did)
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.separator();
            if let Some(live) = live_inventory.as_deref() {
                let cap = crate::config::state::MAX_INVENTORY_ITEMS;
                let len = live.0.generators.len();
                ui.label(format!("Your stash: {len}/{cap}"));
                if len >= cap {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 90, 90),
                        "Inventory full — accepting will not work.",
                    );
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
                            egui::RichText::new("Accept").color(egui::Color32::LIGHT_GREEN),
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
                            .color(egui::Color32::from_rgb(220, 90, 90)),
                    ))
                    .clicked()
                {
                    action = Some(OfferAction::MuteAndDecline);
                }
            });
        });

    let Some(action) = action else {
        return;
    };

    let now = time.elapsed_secs_f64();
    let accepted = matches!(action, OfferAction::Accept);

    // Flip the mute flag on the sender's `RemotePeer` before we send the
    // response so any subsequent offer this frame (unlikely but possible
    // if the attacker double-sent) is already auto-declined as muted.
    if matches!(action, OfferAction::MuteAndDecline) {
        for mut peer in peers.iter_mut() {
            if peer.peer_id == dialog.sender_peer_id && !peer.muted {
                peer.muted = true;
                diagnostics.push(
                    now,
                    format!(
                        "Muted @{} after declining their offer",
                        dialog.sender_handle
                    ),
                );
                break;
            }
        }
    }

    if accepted {
        if let Some(live) = live_inventory.as_mut() {
            let key = choose_inventory_gift_key(&live.0.generators, &dialog.item_name);
            live.0
                .generators
                .insert(key.clone(), dialog.generator.clone());
            diagnostics.push(
                now,
                format!(
                    "Accepted \"{}\" from @{} (stored as \"{}\"), publishing…",
                    dialog.item_name, dialog.sender_handle, key
                ),
            );

            // Auto-publish the updated inventory immediately. The user
            // explicitly chose "publish on accept" over "mark dirty" so
            // accepted items are persistent even if the session ends
            // before they click the Inventory's Publish button. The
            // `poll_publish_inventory_tasks` system (already in the
            // Update schedule) drains the task and flips
            // `StoredInventoryRecord` + `InventoryPublishFeedback` on
            // completion, so we only kick off the I/O here.
            if let (Some(sess), Some(refresh)) = (session.as_deref(), refresh_ctx.as_deref()) {
                *inventory_feedback = InventoryPublishFeedback::Publishing;
                spawn_inventory_publish_task(
                    &mut commands,
                    sess.clone(),
                    refresh.clone(),
                    live.0.clone(),
                );
            }
        } else {
            // Live inventory resource absent — should not happen in
            // `AppState::InGame`, but decline rather than drop the
            // response and leave the sender hanging.
            diagnostics.push(
                now,
                format!(
                    "Could not accept \"{}\" from @{}: inventory not loaded",
                    dialog.item_name, dialog.sender_handle
                ),
            );
        }
    } else {
        diagnostics.push(
            now,
            format!(
                "Declined \"{}\" from @{}",
                dialog.item_name, dialog.sender_handle
            ),
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

fn spawn_inventory_publish_task(
    commands: &mut Commands,
    session: AtprotoSession,
    refresh: crate::oauth::OauthRefreshCtx,
    record: InventoryRecord,
) {
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            publish_inventory_record(&client, &session, &refresh, &record).await
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
    commands.spawn(PublishInventoryTask(task));
}
