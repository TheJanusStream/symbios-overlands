//! Gateway destination picker (#748). Walking into a gateway zone
//! ([`GatewayMarker`]) opens a window listing the **room owner's** mutual
//! follows — visitors browse the owner's social neighbourhood, not their
//! own — with a search filter and a home row. Picking a destination
//! routes through the same [`UnsavedGuard`] flow as classic portals, with
//! `target_pos: None` so arrival resolves the destination record's
//! `default_landing` (#745).
//!
//! Lifecycle: [`watch_gateway_zone`] opens the picker on zone entry and
//! closes it on exit; the window's Close button instead sets
//! [`GatewayDismissed`], which suppresses re-opening until the player
//! leaves the zone once — otherwise the standing overlap would pop the
//! window right back the next frame.

use avian3d::prelude::CollidingEntities;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::avatar::{BskyProfileCache, draw_avatar_icon};
use crate::social::{MutualsCache, MutualsState, request_mutuals};
use crate::state::{CurrentRoomDid, LocalPlayer, TravelingTo};
use crate::ui::chat::AVATAR_ICON_PX;
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};
use crate::world_builder::GatewayMarker;

/// Present while the destination picker is open. Carries the live search
/// text; the mutuals themselves stay in [`MutualsCache`].
#[derive(Resource, Default)]
pub struct GatewayPicker {
    pub search: String,
}

/// Present after the user closed the picker while still standing in the
/// zone. Cleared by [`watch_gateway_zone`] the moment they step out, so
/// the next walk-in opens the picker again.
#[derive(Resource)]
pub struct GatewayDismissed;

/// Open/close the picker from the player's overlap with gateway zones.
pub fn watch_gateway_zone(
    mut commands: Commands,
    players: Query<&CollidingEntities, With<LocalPlayer>>,
    gateways: Query<(), With<GatewayMarker>>,
    picker: Option<Res<GatewayPicker>>,
    dismissed: Option<Res<GatewayDismissed>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
) {
    let Ok(collisions) = players.single() else {
        return;
    };
    let in_zone = collisions.iter().any(|e| gateways.contains(*e));

    if !in_zone {
        if picker.is_some() {
            commands.remove_resource::<GatewayPicker>();
        }
        if dismissed.is_some() {
            commands.remove_resource::<GatewayDismissed>();
        }
        return;
    }
    // In the zone: open unless suppressed — an in-flight travel, a pending
    // unsaved-edits prompt, or an explicit dismissal that hasn't been
    // walked off yet.
    if picker.is_none() && dismissed.is_none() && traveling.is_none() && guard.is_none() {
        commands.insert_resource(GatewayPicker::default());
    }
}

/// Render the destination picker. Registered behind
/// `resource_exists::<GatewayPicker>`.
#[allow(clippy::too_many_arguments)]
pub fn gateway_picker_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut picker: ResMut<GatewayPicker>,
    mut mutuals: ResMut<MutualsCache>,
    current_room: Option<Res<CurrentRoomDid>>,
    session: Option<Res<AtprotoSession>>,
    profile_cache: Res<BskyProfileCache>,
    time: Res<Time>,
) {
    let Some(room) = current_room.as_deref() else {
        return;
    };
    let owner_did = room.0.clone();
    request_mutuals(
        &mut commands,
        &mut mutuals,
        &owner_did,
        time.elapsed_secs_f64(),
    );

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // One deferred travel per frame at most; resolved after the window
    // closure so the borrow of `picker` stays simple.
    let mut travel_to: Option<String> = None;
    let mut close = false;

    egui::Window::new("Gateway")
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -64.0])
        .resizable(false)
        .collapsible(false)
        .fixed_size([380.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Travel to a mutual follow of this room's owner:");
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut picker.search)
                    .hint_text("Search handle or name…")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);

            // Home row — always available when away from home, above the
            // scroll list so it never has to be searched for.
            if let Some(s) = session.as_deref()
                && s.did != owner_did
            {
                ui.horizontal(|ui| {
                    draw_avatar_icon(ui, Some(s.did.as_str()), &profile_cache, AVATAR_ICON_PX);
                    ui.monospace(format!("@{} — home", s.handle));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Go").clicked() {
                            travel_to = Some(s.did.clone());
                        }
                    });
                });
                ui.separator();
            }

            match mutuals.get(&owner_did).map(|c| &c.state) {
                None | Some(MutualsState::Loading) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking up mutuals…");
                    });
                }
                Some(MutualsState::Failed(reason)) => {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("Could not load mutuals — {reason}"),
                    );
                    ui.label("Retrying shortly…");
                }
                Some(MutualsState::Ready(list)) => {
                    let query = picker.search.to_lowercase();
                    let self_did = session.as_deref().map(|s| s.did.as_str());
                    let rows: Vec<_> = list
                        .mutuals
                        .iter()
                        // The owner's own room and your home row are
                        // already covered; drop them from the list.
                        .filter(|m| m.did != owner_did && Some(m.did.as_str()) != self_did)
                        .filter(|m| {
                            query.is_empty()
                                || m.handle.to_lowercase().contains(&query)
                                || m.display_name
                                    .as_deref()
                                    .is_some_and(|n| n.to_lowercase().contains(&query))
                        })
                        .collect();
                    if list.truncated {
                        ui.small("Large following — list may be incomplete.");
                    }
                    if rows.is_empty() {
                        ui.label(if query.is_empty() {
                            "No mutual follows found."
                        } else {
                            "No matches."
                        });
                    }
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for m in rows {
                                ui.horizontal(|ui| {
                                    draw_avatar_icon(
                                        ui,
                                        Some(m.did.as_str()),
                                        &profile_cache,
                                        AVATAR_ICON_PX,
                                    );
                                    match &m.display_name {
                                        Some(name) => {
                                            ui.label(name);
                                            ui.monospace(
                                                egui::RichText::new(format!("@{}", m.handle))
                                                    .weak(),
                                            );
                                        }
                                        None => {
                                            ui.monospace(format!("@{}", m.handle));
                                        }
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("Go").clicked() {
                                                travel_to = Some(m.did.clone());
                                            }
                                        },
                                    );
                                });
                            }
                        });
                }
            }

            ui.add_space(6.0);
            if ui.button("Close").clicked() {
                close = true;
            }
        });

    if let Some(target_did) = travel_to {
        // Same guard flow as walking into a classic portal: the guard owns
        // the unsaved-edits question and then calls `begin_portal_travel`.
        // `target_pos: None` = arrive at the destination's default landing.
        commands.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
            target_did,
            target_pos: None,
        }));
        commands.remove_resource::<GatewayPicker>();
        // Suppress re-opening while still overlapping this gate (e.g. the
        // guard's "Stay here" path); walking out clears it.
        commands.insert_resource(GatewayDismissed);
    } else if close {
        commands.remove_resource::<GatewayPicker>();
        commands.insert_resource(GatewayDismissed);
    }
}
