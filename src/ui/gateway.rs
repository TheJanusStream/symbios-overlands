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

use crate::avatar::BskyProfileCache;
use crate::social::{MutualsCache, MutualsState, request_mutuals};
use crate::state::{CurrentRoomDid, LocalPlayer, TravelingTo};
use crate::ui::avatar::draw_avatar_icon;
use crate::ui::chat::AVATAR_ICON_PX;
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};
use crate::world_builder::GatewayMarker;

/// Present while the destination picker is open. Carries the live search
/// text; the mutuals themselves stay in [`MutualsCache`].
#[derive(Resource, Default)]
pub struct GatewayPicker {
    pub search: String,
    /// Free-text destination (#1232 f24). The picker's reachable set is a
    /// function of a THIRD PARTY's follow graph — the room owner's mutuals
    /// — while its only text box filtered that list in memory under a
    /// "Search handle or name…" hint that promised a network lookup it
    /// never performed. A new account standing on its own gateway, which
    /// is precisely the user with no mutuals, saw a search box, an empty
    /// list and a Close button: nothing that could be travelled through.
    pub destination: String,
    /// Why the last attempt at `destination` was refused.
    pub destination_error: Option<String>,
}

/// In-flight `@handle` → DID resolve for the destination row.
///
/// A DID typed in full needs no lookup and goes straight to the guard; a
/// handle costs one `resolveHandle` round trip, which is what the login
/// form already spends before starting an OAuth dance for the same reason
/// — a typo fails in one request with a spelling hint instead of burning
/// the post-arrival record-fetch retry budget.
#[derive(Component)]
pub struct GatewayDestinationTask {
    /// What the user typed, kept as the label the travel carries (#1231
    /// f27) — they wrote the name, so it is the name they will recognise.
    typed: String,
    task: bevy::tasks::Task<Result<String, String>>,
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

/// Small re-open chip while the player stands in a gateway zone with the
/// picker dismissed (#842): the only way back in used to be walking out
/// of the zone and back ([`GatewayDismissed`] clears on exit only).
/// Registered behind `resource_exists::<GatewayDismissed>`, whose
/// presence already implies "still in the zone".
pub fn gateway_reopen_chip_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    picker: Option<Res<GatewayPicker>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
) {
    if picker.is_some() || traveling.is_some() || guard.is_some() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("gateway-reopen-chip")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
        .show(ctx, |ui| {
            if ui.button("⌖ Gateway — choose a destination").clicked() {
                // The zone watcher sees the dismissal gone and reopens
                // the picker on its next run.
                commands.remove_resource::<GatewayDismissed>();
            }
        });
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
    resolve_tasks: Query<(), With<GatewayDestinationTask>>,
) {
    // Guarded-dirty (#879, generalised by #1274 f177): the widgets below take
    // `&mut` fields of this resource, and `ResMut::deref_mut` stamps the change
    // tick on ACCESS — so drawing the window marked it changed on every frame
    // whether or not anybody typed. Nothing reads this resource's change tick
    // today, and copying its strings in and out each frame to find that out
    // would cost more than the tick does. If a consumer is ever added, call
    // `set_changed()` on a real edit rather than deleting this line.
    let picker = picker.bypass_change_detection();
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
    // The DID AND the name the row rendered (#1231 f27): the profile
    // cache is filled by peer-driven fetches only, so a mutual the viewer
    // has never shared a room with is not in it — and the overlay one
    // click after "@alice" said `did:plc:abcdefgh…` for the same person.
    let mut travel_to: Option<(String, String)> = None;
    /// Reserved width of the destination row's Go button, so the text
    /// field beside it does not resize as the button enables.
    const GO_BUTTON_WIDTH: f32 = 44.0;
    let mut go_to: Option<String> = None;
    let mut retry_mutuals = false;
    let mut close = false;

    egui::Window::new("Gateway")
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -64.0])
        .resizable(false)
        .collapsible(false)
        .fixed_size([380.0, 0.0])
        .show(ctx, |ui| {
            // The free-text row FIRST (#1232 f24): it is the only control
            // on this window that can reach somebody outside the room
            // owner's follow graph, and it used to not exist.
            ui.label("Go to a handle or DID:");
            let resolving = !resolve_tasks.is_empty();
            ui.horizontal(|ui| {
                let entry = crate::ui::affordances::text_edit_enabled(
                    ui,
                    !resolving,
                    "Looking up that handle — the field unlocks when it resolves",
                    egui::TextEdit::singleline(&mut picker.destination)
                        .hint_text("@alice.bsky.social")
                        .desired_width(ui.available_width() - GO_BUTTON_WIDTH),
                );
                let entered =
                    entry.lost_focus() && entry.ctx.input(|i| i.key_pressed(egui::Key::Enter));
                // Two different reasons disable this, so the hover has to
                // say WHICH (#1289) — "enter a handle" while a lookup is in
                // flight would be wrong, and the spinner below is the only
                // other cue for the resolving case.
                let go_blocked = if resolving {
                    Some("Looking up that handle — this will enable when it resolves")
                } else if picker.destination.trim().is_empty() {
                    Some("Enter a handle or DID to travel to")
                } else {
                    None
                };
                let go = ui
                    .add_enabled(go_blocked.is_none(), egui::Button::new("Go"))
                    .on_disabled_hover_text(go_blocked.unwrap_or_default());
                if go.clicked() || entered {
                    go_to = Some(picker.destination.trim().to_owned());
                }
            });
            if resolving {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Looking up that handle…");
                });
            }
            if let Some(error) = &picker.destination_error {
                ui.colored_label(crate::ui::theme::current(ui.ctx()).status.error, error);
            }

            ui.separator();
            ui.label("…or a mutual follow of this world's owner:");
            ui.add_space(4.0);
            crate::ui::affordances::text_edit(
                ui,
                egui::TextEdit::singleline(&mut picker.search)
                    // "Filter this list", not "Search" (#1232 f24): it has
                    // only ever matched the rows already fetched, and the
                    // old hint promised a lookup that never happened.
                    .hint_text("Filter this list…")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);

            // Home row — always available when away from home, above the
            // scroll list so it never has to be searched for.
            if let Some(s) = session.as_deref()
                && s.did != owner_did
            {
                ui.horizontal(|ui| {
                    draw_avatar_icon(
                        ui,
                        Some(s.did.as_str()),
                        Some(s.handle.as_str()),
                        &profile_cache,
                        AVATAR_ICON_PX,
                    );
                    ui.monospace(format!("@{} — home", s.handle));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Go").clicked() {
                            travel_to = Some((s.did.clone(), format!("@{}", s.handle)));
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
                    // Plain language, and the raw chain stays in the log
                    // (#1232 f284): `app.bsky.graph.getFollows => 429` is a
                    // lexicon name on a control an ordinary visitor operates.
                    ui.colored_label(
                        crate::ui::theme::current(ui.ctx()).status.error,
                        crate::social::mutuals_error(reason),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Retrying shortly…");
                        if ui.small_button("Retry now").clicked() {
                            retry_mutuals = true;
                        }
                    });
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
                        // Both empty states name the rule (#1232 f24). The
                        // reachable set here is a function of somebody
                        // else's follow graph, which is not a thing a
                        // visitor can be expected to infer from "No
                        // matches." — and the row above is the way out.
                        ui.label(if query.is_empty() {
                            "This world's owner has no mutual follows yet — \
                             enter a handle above to go somewhere."
                        } else {
                            "No mutual follows match that filter — enter a \
                             handle above to go somewhere else."
                        });
                    }
                    // Scale the list cap with the screen instead of a
                    // flat 240px (#834): ~1/3 of the height keeps the
                    // bottom-anchored window clear of the scene on a
                    // laptop while letting a tall monitor show more
                    // mutuals; the old 240px stays as the floor.
                    let list_cap = (ui.ctx().content_rect().height() / 3.0).max(240.0);
                    egui::ScrollArea::vertical()
                        .max_height(list_cap)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for m in rows {
                                ui.horizontal(|ui| {
                                    draw_avatar_icon(
                                        ui,
                                        Some(m.did.as_str()),
                                        Some(m.handle.as_str()),
                                        &profile_cache,
                                        AVATAR_ICON_PX,
                                    );
                                    // Handle first, display name after
                                    // (#1222 f295). The handle is verified by
                                    // the network; the display name is
                                    // whatever its owner typed, and rendering
                                    // the spoofable field large with the
                                    // verified one greyed beside it inverted
                                    // the trust order the rest of the app
                                    // gets right — a display name set to
                                    // somebody else's handle produced a row
                                    // that read as that person. Chat and the
                                    // People roster show only the verified
                                    // handle; this surface is the one
                                    // strangers browse.
                                    ui.monospace(format!("@{}", m.handle));
                                    if let Some(name) = &m.display_name {
                                        ui.label(egui::RichText::new(name).weak());
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("Go").clicked() {
                                                travel_to =
                                                    Some((m.did.clone(), format!("@{}", m.handle)));
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

    if retry_mutuals {
        // Dropping the slot is the whole retry: `request_mutuals` runs at
        // the top of this system and dispatches whenever there is no slot.
        mutuals.forget(&owner_did);
    }

    // The free-text destination (#1232 f24). Validated by the login form's
    // own `validate_destination` — the two surfaces that ask "where do you
    // want to go" agree on what an answer looks like — and a handle costs
    // one `resolveHandle` round trip before the guard flow starts.
    if let Some(typed) = go_to {
        picker.destination_error = None;
        match crate::ui::login::validation::validate_destination(&typed) {
            Err(message) => picker.destination_error = Some(message),
            // Blank is "your own world", which the home row above already
            // offers by name; the Go button is disabled on empty input, so
            // this arm is only reachable via an all-whitespace entry.
            Ok(crate::ui::login::validation::Destination::Home) => {
                picker.destination_error = Some(String::from(
                    "Enter a handle or DID — the home row above goes home.",
                ));
            }
            Ok(crate::ui::login::validation::Destination::Did(did)) => {
                travel_to = Some((did.clone(), did));
            }
            Ok(crate::ui::login::validation::Destination::Handle(handle)) => {
                let lookup = handle.clone();
                let task = bevy::tasks::IoTaskPool::get().spawn(async move {
                    let fut = async {
                        let http = crate::config::http::default_client();
                        crate::pds::resolve_handle(&http, &lookup).await
                    };
                    crate::config::http::run_or(
                        fut,
                        Err(crate::config::http::timed_out("handle lookup")),
                    )
                    .await
                });
                commands.spawn(GatewayDestinationTask {
                    typed: format!("@{handle}"),
                    task,
                });
            }
        }
    }

    if let Some((target_did, target_label)) = travel_to {
        // Same guard flow as walking into a classic portal: the guard owns
        // the unsaved-edits question and then calls `begin_portal_travel`.
        // `target_pos: None` = arrive at the destination's default landing.
        commands.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
            via: crate::ui::unsaved_guard::TravelVia::Gateway,
            target_did,
            target_label: Some(target_label),
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

/// Drain a finished destination lookup into the same guard flow every
/// other travel takes (#1232 f24).
///
/// A picker that has closed under the task — the player walked out of the
/// zone, or picked a mutual instead — takes the result nowhere: a travel
/// nobody is still asking for must not fire.
pub fn poll_gateway_destination(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut GatewayDestinationTask)>,
    picker: Option<ResMut<GatewayPicker>>,
) {
    let mut picker = picker;
    for (entity, mut lookup) in tasks.iter_mut() {
        let Some(result) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut lookup.task),
        ) else {
            continue;
        };
        commands.entity(entity).despawn();
        let Some(picker) = picker.as_deref_mut() else {
            continue;
        };
        match result {
            Ok(did) => {
                commands.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
                    via: crate::ui::unsaved_guard::TravelVia::Gateway,
                    target_did: did,
                    // What the user typed, which is the name they will
                    // recognise on the overlay that follows (#1231 f27).
                    target_label: Some(lookup.typed.clone()),
                    target_pos: None,
                }));
                commands.remove_resource::<GatewayPicker>();
                commands.insert_resource(GatewayDismissed);
            }
            Err(reason) => {
                warn!("gateway destination lookup failed: {reason}");
                picker.destination_error = Some(destination_lookup_error(&reason));
            }
        }
    }
}

/// Plain language for a failed destination lookup (#1232 f24).
///
/// `resolve_handle`'s errors are XRPC chains, and this row is on the same
/// surface as the mutuals list whose lexicon leak #1232 f284 is about —
/// so it does not get to grow one of its own.
pub fn destination_lookup_error(raw: &str) -> String {
    if raw.contains("timed out") {
        return String::from("That lookup timed out — check your connection and try again.");
    }
    String::from("No account with that handle — check the spelling.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::login::validation::{Destination, validate_destination};

    /// THE SEQUENCE (#1232 f24): a visitor stands on a gateway, types a
    /// friend's handle into "Search handle or name…" and gets "No
    /// matches." The friend exists — they are simply not a mutual follow
    /// of whoever owns this room, and the box never performed a lookup at
    /// all. A new account on its own gateway, which is exactly the user
    /// with no mutuals, saw a search box, an empty list and a Close
    /// button: nothing that could be travelled through.
    ///
    /// The row asks the same question the login form asks, through the
    /// same validator, so the two cannot drift on what an answer is.
    #[test]
    fn the_destination_row_takes_the_same_answers_the_login_form_takes() {
        assert_eq!(
            validate_destination("@alice.bsky.social"),
            Ok(Destination::Handle("alice.bsky.social".into())),
            "a handle is resolved before the guard flow starts"
        );
        assert_eq!(
            validate_destination("did:plc:abcdefgh"),
            Ok(Destination::Did("did:plc:abcdefgh".into())),
            "a DID typed in full costs no lookup"
        );
        // A typo fails here with a spelling hint rather than becoming a
        // travel to nowhere.
        assert!(validate_destination("not a handle").is_err());
        assert_eq!(validate_destination("   "), Ok(Destination::Home));
    }

    /// And a lookup that fails says so in English. This row shares a
    /// surface with the mutuals list whose lexicon leak #1232 f284 is
    /// about, so it does not get to grow one of its own.
    #[test]
    fn a_failed_destination_lookup_does_not_leak_the_chain() {
        for raw in [
            "resolveHandle => 400 Bad Request",
            "handle lookup timed out after 30s",
        ] {
            let shown = destination_lookup_error(raw);
            assert!(
                !shown.contains("=>") && !shown.contains("resolveHandle"),
                "{shown}"
            );
            assert!(shown.ends_with('.'), "{shown}");
        }
        assert_ne!(
            destination_lookup_error("resolveHandle => 400"),
            destination_lookup_error("handle lookup timed out after 30s")
        );
    }
}
