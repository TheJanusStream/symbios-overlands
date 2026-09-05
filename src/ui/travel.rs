//! Travel visibility (#842): the in-flight overlay and the portal
//! approach prompt.
//!
//! Portal travel used to be invisible — [`TravelingTo`] suppressed every
//! drive system with zero on-screen sign, and classic portals never said
//! WHERE they lead while committing travel on mere collider contact.
//! This module adds the two read-only surfaces:
//!
//! * [`travel_overlay_ui`] — a card while a travel is in flight:
//!   destination name, spinner, elapsed seconds. It covers BOTH halves of
//!   the journey (#1231 f20) — the record fetch, which a *Cancel travel*
//!   button can give up on, and the terrain regen plus world compile that
//!   follow it, behind a veil, because the alternative was watching the
//!   destination assemble from inside the ground.
//! * [`portal_prompt_ui`] — a bottom-center line while the player is
//!   NEAR (not yet touching) an inter-room portal, naming the
//!   destination before contact commits the travel.
//!
//! Destination naming goes through [`travel_label`]: the name the surface
//! that started the travel already had, else the bsky profile cache, else
//! the DID's head — all of it spelled by [`PeerLabel`], the app's one
//! naming ladder.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::avatar::BskyProfileCache;
use crate::network::presence::PeerLabel;
use crate::state::{CurrentRoomDid, LocalPlayer, TravelPhase, TravelingTo};
use crate::ui::unsaved_guard::UnsavedGuard;
use crate::world_builder::PortalMarker;

/// How close (m) the player must be to a portal for the approach prompt.
/// Comfortably outside the portal colliders the themes build (~1-2 m
/// half-extents), so the prompt appears before contact commits.
const PORTAL_PROMPT_RADIUS_M: f32 = 7.0;

/// Opacity of the veil painted over the scene while the destination is
/// still being built. Not fully opaque on purpose — the world coming up
/// underneath is the progress cue the compile itself cannot give.
const ARRIVAL_VEIL_ALPHA: f32 = 0.88;

/// Human display name for a DID, through the app's one naming ladder
/// ([`PeerLabel`]).
///
/// `carried` is the name the surface that started this travel already had
/// (#1231 f27). It matters because [`BskyProfileCache`] is filled only by
/// peer-driven fetches — `trigger_avatar_fetches` walks `RemotePeer`
/// entities — so a mutual the viewer has never shared a room with is never
/// in it. The gateway row rendered "@alice" from the mutuals list and threw
/// the handle away, and the overlay one click later said
/// `did:plc:abcdefgh…` for the same person.
pub(crate) fn travel_label(cache: &BskyProfileCache, did: &str, carried: Option<&str>) -> String {
    if let Some(name) = carried.filter(|n| !n.is_empty()) {
        return name.to_owned();
    }
    let handle = cache.get(did).and_then(|p| p.handle.clone());
    PeerLabel::new(handle.as_deref(), Some(did)).addressed()
}

/// What the overlay says for a travel in `phase` heading to `name`.
///
/// Pure so the two halves' wording is testable, and separate sentences
/// because they are separate waits: one is a request to somebody else's
/// server, the other is this machine building a world.
pub(crate) fn travel_headline(phase: TravelPhase, name: &str) -> String {
    match phase {
        TravelPhase::Fetching => format!("Traveling to {name}'s world…"),
        TravelPhase::Building => format!("Building {name}'s world…"),
    }
}

/// Always-on-top in-flight card while [`TravelingTo`] exists: where we
/// are going, a spinner, and how long the journey has been running. The
/// `Local` start stamp arms on the rising edge and clears when the
/// travel resolves either way.
pub fn travel_overlay_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    traveling: Option<Res<TravelingTo>>,
    tasks: Query<Entity, With<crate::player::PortalTravelTask>>,
    profile_cache: Res<BskyProfileCache>,
    time: Res<Time>,
    mut started_at: Local<Option<f64>>,
) {
    let Some(traveling) = traveling.as_deref() else {
        *started_at = None;
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let started = *started_at.get_or_insert(now);

    // The veil (#1231 f20). Until the arrival gate existed the player was
    // released at the landing pose the frame the RECORD landed — `y = 0`
    // for a gateway hop, frequently below the terrain still standing where
    // they left — and the destination materialised around them over
    // several seconds. Freezing them there without a veil would only trade
    // a moving underground camera for a still one.
    if traveling.phase == TravelPhase::Building {
        let theme = crate::ui::theme::current(ctx);
        ctx.layer_painter(egui::LayerId::background()).rect_filled(
            ctx.viewport_rect(),
            0.0,
            theme.backdrop_bottom.gamma_multiply(ARRIVAL_VEIL_ALPHA),
        );
    }

    let destination = travel_label(
        &profile_cache,
        &traveling.target_did,
        traveling.target_label.as_deref(),
    );
    egui::Window::new("travel-overlay")
        .title_bar(false)
        .resizable(false)
        // Interactable for the Cancel button (#1231 f25). It claims only
        // this small card's rect for pointer input; the avatar is parked
        // for the duration either way, so the camera gating that costs is
        // not gating anything the player could be doing.
        .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "{} ({:.0}s)",
                    travel_headline(traveling.phase, &destination),
                    (now - started).max(0.0)
                ));
            });
            match traveling.phase {
                // A fetch can outlast a minute — `fetch_room_record`
                // resolves the DID and then reads the record, each under
                // its own 30 s bound — and the player sits motionless for
                // all of it. #1129 established that a stalled network
                // operation must be escapable; this was the one that
                // wasn't, and the target-match guard in
                // `poll_portal_travel_tasks` already makes a late-arriving
                // result harmless.
                TravelPhase::Fetching => {
                    if ui
                        .button("Cancel travel")
                        .on_hover_text("Stay where you are and keep playing")
                        .clicked()
                    {
                        for entity in tasks.iter() {
                            commands.entity(entity).despawn();
                        }
                        commands.remove_resource::<TravelingTo>();
                        // The same brief cooldown the failure arm sets, so
                        // the portal the player is still standing in does
                        // not immediately pull them back through.
                        commands.insert_resource(crate::player::PortalCooldown {
                            until_secs: now + crate::player::PORTAL_COOLDOWN_SECS,
                        });
                    }
                }
                // Nothing to cancel: the record is installed and the world
                // being left no longer exists. Saying what the wait is for
                // is all that is owed, and it is what the loading screen
                // says for the same work.
                TravelPhase::Building => {
                    ui.label(
                        egui::RichText::new("Terrain and props — this can pause for a moment.")
                            .small()
                            .color(crate::ui::theme::current(ui.ctx()).text_weak),
                    );
                }
            }
        });
}

/// Why "Travel to my world" is unavailable, or `None` (#1232 f251).
///
/// Pure, and it exists because the account chip is the ONLY route home
/// that does not require walking into a gateway collider. The gateway
/// picker's home row lives inside a window that exists only while the
/// player overlaps a `GatewayMarker` sensor — and a landmark link can put
/// the arrival anywhere, with no map, compass or marker pointing at the
/// gate. A first-time visitor who could not find their way back had one
/// exit, and it was Log out.
///
/// Disabled with a reason rather than hidden, the #851 idiom the World
/// Editor button and the People *Visit* button already follow: an absent
/// control cannot carry a tooltip, and "you are already home" is a
/// perfectly good thing to say out loud.
pub fn home_travel_blocked(
    already_home: bool,
    traveling: bool,
    guard_open: bool,
) -> Option<&'static str> {
    if already_home {
        return Some("You are already in your own world");
    }
    if traveling {
        return Some("Finish or cancel the current travel first");
    }
    if guard_open {
        return Some("Answer the unsaved-edits prompt first");
    }
    None
}

/// Verified names for the inter-room portals this room contains (#1231
/// f27).
///
/// The approach prompt is the safety affordance that lets a player decide
/// whether to keep walking, and for anybody they have not already met it
/// read as a hex string: a classic portal carries a DID and nothing else,
/// and `BskyProfileCache` is filled only by peer-driven fetches, so it was
/// never going to have the answer.
///
/// The lookup is [`crate::pds::resolve_did_handle`] — the bidirectionally
/// verified one #1227 built, not `getProfile`'s claim — because this is a
/// prompt about whether to enter a stranger's world, and a prompt that can
/// be made to print somebody else's name is worse than one that prints an
/// identifier.
///
/// A key's presence means "asked"; its value is the answer so far. A
/// lookup that fails settles as `None` and is never retried, exactly as
/// [`crate::ui::login::entry::resolve_boot_destination`] settles: retrying
/// in a loop against somebody else's directory service is not a thing a
/// prompt should do.
#[derive(Resource, Default)]
pub struct PortalNames {
    names: std::collections::HashMap<String, Option<String>>,
}

/// How many portal destinations one session will name. A room's portals
/// are authored by its owner and bounded by the record's placement cap,
/// but travel accumulates rooms — so the map is bounded like the profile
/// cache beside it, and past the cap the prompt falls back to the DID.
const MAX_PORTAL_NAMES: usize = 64;

impl PortalNames {
    /// The verified handle for `did`, if one has landed.
    pub fn get(&self, did: &str) -> Option<&str> {
        self.names.get(did).and_then(Option::as_deref)
    }

    fn asked(&self, did: &str) -> bool {
        self.names.contains_key(did)
    }

    /// Forget every answer. Called on logout for the reason
    /// [`BskyProfileCache`] is: these are DID-keyed lookups this account's
    /// session went and made, about rooms the next account may never see.
    pub fn clear(&mut self) {
        self.names.clear();
    }
}

/// In-flight DID → verified handle lookup for a portal destination.
#[derive(Component)]
pub struct ResolvePortalNameTask {
    did: String,
    task: bevy::tasks::Task<Option<String>>,
}

/// Name the portal the player is walking towards, before they reach it.
///
/// Deliberately NOT part of [`portal_prompt_ui`]: a UI system that spawns
/// network tasks is how a render path acquires a fetch storm. One lookup
/// per DID per session, started only for a portal already inside the
/// prompt radius — so a room full of portals costs nothing until somebody
/// walks up to one.
pub fn resolve_portal_names(
    mut commands: Commands,
    players: Query<&GlobalTransform, With<LocalPlayer>>,
    portals: Query<(&PortalMarker, &GlobalTransform)>,
    current_room: Option<Res<CurrentRoomDid>>,
    mut names: ResMut<PortalNames>,
    mut tasks: Query<(Entity, &mut ResolvePortalNameTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        // Overwrites the `None` placed at spawn time; a failure leaves it,
        // which is what stops the retry.
        names.names.insert(task.did.clone(), result);
    }

    let Ok(player_tf) = players.single() else {
        return;
    };
    let Some(did) = nearest_portal_did(&portals, current_room.as_deref(), player_tf.translation())
    else {
        return;
    };
    if names.asked(&did) || !did.starts_with("did:") || names.names.len() >= MAX_PORTAL_NAMES {
        return;
    }
    if !crate::pds::xrpc::is_resolvable_did(&did) {
        names.names.insert(did, None);
        return;
    }
    // Marked asked at spawn time, not at completion: this system runs every
    // frame the player stands near the portal.
    names.names.insert(did.clone(), None);
    let lookup_did = did.clone();
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            crate::pds::resolve_did_handle(&client, &lookup_did).await
        };
        crate::config::http::run_or(fut, None).await
    });
    commands.spawn(ResolvePortalNameTask { did, task });
}

/// The DID of the nearest inter-room portal inside the prompt radius.
///
/// Shared by the prompt and the name lookup so the two can never disagree
/// about which portal is being approached.
fn nearest_portal_did(
    portals: &Query<(&PortalMarker, &GlobalTransform)>,
    current_room: Option<&CurrentRoomDid>,
    player_pos: Vec3,
) -> Option<String> {
    portals
        .iter()
        .filter(|(marker, _)| current_room.is_none_or(|room| room.0 != marker.target_did))
        .map(|(marker, tf)| (marker, tf.translation().distance(player_pos)))
        .filter(|(_, distance)| *distance <= PORTAL_PROMPT_RADIUS_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(marker, _)| marker.target_did.clone())
}

/// Bottom-center approach prompt naming an inter-room portal's
/// destination before collider contact commits the travel. Same-room
/// teleporters are skipped (they act instantly and stay local), as is
/// everything while a travel or guard dialog is already in progress.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn portal_prompt_ui(
    mut contexts: EguiContexts,
    players: Query<&GlobalTransform, With<LocalPlayer>>,
    portals: Query<(&PortalMarker, &GlobalTransform)>,
    current_room: Option<Res<CurrentRoomDid>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
    profile_cache: Res<BskyProfileCache>,
    names: Res<PortalNames>,
    // A gateway underfoot outranks a portal nearby (#1261 f35) — see the
    // gate below.
    gateway_dismissed: Option<Res<crate::ui::gateway::GatewayDismissed>>,
) {
    if traveling.is_some() || guard.is_some() {
        return;
    }
    // #1261 f35: the gateway re-open chip anchors at exactly this point —
    // `CENTER_BOTTOM` with a -24 offset — so standing in a gateway zone
    // within 7 m of an owner-placed portal drew two opaque cards of
    // different widths through each other. This window is
    // `.interactable(false)`, so nothing was stealing the chip's clicks;
    // the damage was that neither card could be read.
    //
    // The chip wins because it is about where the player IS STANDING and
    // it is the one with a control on it. The picker, which the chip
    // replaces, sits at -64 and never collided — that offset is the
    // evidence the stacking was reasoned about for two of the three
    // surfaces and not the third.
    if gateway_dismissed.is_some() {
        return;
    }
    let Ok(player_tf) = players.single() else {
        return;
    };
    let Some(target_did) =
        nearest_portal_did(&portals, current_room.as_deref(), player_tf.translation())
    else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // The verified name if `resolve_portal_names` has landed one, else the
    // ladder's next rung (#1231 f27). A portal carries a DID and nothing
    // else, so this is the one travel surface with no label to carry.
    let destination = travel_label(&profile_cache, &target_did, names.get(&target_did));
    egui::Window::new("portal-prompt")
        .title_bar(false)
        .resizable(false)
        .interactable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Portal to {destination}'s world — keep walking to travel"
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_prefers_a_handle_and_shortens_dids() {
        let cache = BskyProfileCache::default();
        // No cache entry → shortened DID with an ellipsis.
        let long = "did:plc:abcdefghijklmnopqrstuvwx";
        let shown = travel_label(&cache, long, None);
        assert!(shown.ends_with('…'));
        assert!(shown.starts_with("did:plc:"));
        assert!(shown.chars().count() <= 17);
        // Short identifiers pass through untouched.
        assert_eq!(travel_label(&cache, "did:web:x", None), "did:web:x");
    }

    /// THE SEQUENCE (#1231 f27): the visitor clicks *Go* beside "@alice" in
    /// the gateway picker and the overlay that follows says "Traveling to
    /// did:plc:abcdefgh…'s world". The refuter is right that this function
    /// already walks handle → DID head → raw; what it cannot do is fill the
    /// cache. `BskyProfileCache` is populated by peer-driven fetches over
    /// `RemotePeer` entities, so a mutual the viewer has never shared a
    /// room with is never in it — and the row that just rendered her handle
    /// threw it away. The fix is to carry the label the row already had.
    #[test]
    fn a_carried_label_beats_a_cache_that_was_never_going_to_have_the_name() {
        let cache = BskyProfileCache::default();
        let did = "did:plc:abcdefghijklmnopqrstuvwx";
        assert_eq!(
            travel_label(&cache, did, Some("@alice.bsky.social")),
            "@alice.bsky.social",
            "the gateway row's own name survives the click"
        );
        // An empty carried label is not a name; fall through the ladder.
        assert_eq!(
            travel_label(&cache, did, Some("")),
            travel_label(&cache, did, None),
            "an empty carried label is not a name"
        );
        assert!(travel_label(&cache, did, None).ends_with('…'));
    }

    /// THE SEQUENCE (#1232 f251): a visitor arrives in a stranger's world
    /// through a landmark link, which can drop them anywhere, and wants to
    /// go back to their own. The only home affordance in the app was a row
    /// inside a window that exists solely while standing inside the host's
    /// gate — with no map, compass or marker pointing at it. Their one
    /// remaining exit was Log out, which is the action the app itself
    /// guards as destructive.
    #[test]
    fn the_route_home_says_why_it_is_unavailable_rather_than_vanishing() {
        assert_eq!(home_travel_blocked(false, false, false), None);
        // The most useful reason of the three: standing in your own world
        // is the one state with no cue anywhere else on screen.
        assert!(
            home_travel_blocked(true, false, false)
                .expect("already home")
                .contains("already"),
        );
        assert!(home_travel_blocked(false, true, false).is_some());
        assert!(home_travel_blocked(false, false, true).is_some());
        // Ordered most-fundamental-first, like `wear_blocked_reason`: an
        // owner standing at home mid-travel is told the thing that will
        // still be true when the travel ends.
        assert_eq!(
            home_travel_blocked(true, true, true),
            home_travel_blocked(true, false, false)
        );
    }

    /// #1231 f20. The two halves of a journey are two different waits and
    /// say so: one is a request to somebody else's server, the other is
    /// this machine building a world.
    #[test]
    fn the_overlay_names_the_half_of_the_journey_it_is_in() {
        let fetching = travel_headline(TravelPhase::Fetching, "@alice.bsky.social");
        let building = travel_headline(TravelPhase::Building, "@alice.bsky.social");
        assert!(fetching.contains("Traveling to"), "{fetching}");
        assert!(building.contains("Building"), "{building}");
        assert_ne!(fetching, building);
        for line in [&fetching, &building] {
            assert!(line.contains("@alice.bsky.social"), "{line}");
        }
    }
}
