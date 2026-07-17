//! Travel visibility (#842): the in-flight overlay and the portal
//! approach prompt.
//!
//! Portal travel used to be invisible — [`TravelingTo`] suppressed every
//! drive system with zero on-screen sign, and classic portals never said
//! WHERE they lead while committing travel on mere collider contact.
//! This module adds the two read-only surfaces:
//!
//! * [`travel_overlay_ui`] — a small always-on-top card while a travel
//!   fetch is in flight: destination name, spinner, elapsed seconds.
//! * [`portal_prompt_ui`] — a bottom-center line while the player is
//!   NEAR (not yet touching) an inter-room portal, naming the
//!   destination before contact commits the travel.
//!
//! Destination naming resolves the DID through the bsky profile cache
//! when the owner has crossed paths with us before; otherwise the DID is
//! shown shortened — honest, and the cache warms as sessions accumulate.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::avatar::BskyProfileCache;
use crate::state::{CurrentRoomDid, LocalPlayer, TravelingTo};
use crate::ui::unsaved_guard::UnsavedGuard;
use crate::world_builder::PortalMarker;

/// How close (m) the player must be to a portal for the approach prompt.
/// Comfortably outside the portal colliders the themes build (~1-2 m
/// half-extents), so the prompt appears before contact commits.
const PORTAL_PROMPT_RADIUS_M: f32 = 7.0;

/// Human display name for a DID: the profile-verified `@handle` when the
/// bsky cache has one, otherwise the DID shortened to its tail — long
/// enough to recognise, short enough for a one-line prompt.
pub(crate) fn display_name_for_did(cache: &BskyProfileCache, did: &str) -> String {
    if let Some(handle) = cache.get(did).and_then(|p| p.handle.as_deref()) {
        return format!("@{handle}");
    }
    // `did:plc:` + 24 chars is the common shape; keep scheme + head.
    if did.chars().count() > 16 {
        let head: String = did.chars().take(16).collect();
        format!("{head}…")
    } else {
        did.to_owned()
    }
}

/// Always-on-top in-flight card while [`TravelingTo`] exists: where we
/// are going, a spinner, and how long the fetch has been running. The
/// `Local` start stamp arms on the rising edge and clears when the
/// travel resolves either way.
pub fn travel_overlay_ui(
    mut contexts: EguiContexts,
    traveling: Option<Res<TravelingTo>>,
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

    let destination = display_name_for_did(&profile_cache, &traveling.target_did);
    egui::Window::new("travel-overlay")
        .title_bar(false)
        .resizable(false)
        .interactable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Traveling to {destination}'s world… ({:.0}s)",
                    (now - started).max(0.0)
                ));
            });
        });
}

/// Bottom-center approach prompt naming an inter-room portal's
/// destination before collider contact commits the travel. Same-room
/// teleporters are skipped (they act instantly and stay local), as is
/// everything while a travel or guard dialog is already in progress.
#[allow(clippy::type_complexity)]
pub fn portal_prompt_ui(
    mut contexts: EguiContexts,
    players: Query<&GlobalTransform, With<LocalPlayer>>,
    portals: Query<(&PortalMarker, &GlobalTransform)>,
    current_room: Option<Res<CurrentRoomDid>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
    profile_cache: Res<BskyProfileCache>,
) {
    if traveling.is_some() || guard.is_some() {
        return;
    }
    let Ok(player_tf) = players.single() else {
        return;
    };
    let player_pos = player_tf.translation();

    // Nearest inter-room portal within the prompt radius.
    let nearest = portals
        .iter()
        .filter(|(marker, _)| {
            current_room
                .as_deref()
                .is_none_or(|room| room.0 != marker.target_did)
        })
        .map(|(marker, tf)| (marker, tf.translation().distance(player_pos)))
        .filter(|(_, distance)| *distance <= PORTAL_PROMPT_RADIUS_M)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let Some((marker, _)) = nearest else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let destination = display_name_for_did(&profile_cache, &marker.target_did);
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
    fn display_name_prefers_handle_and_shortens_dids() {
        let cache = BskyProfileCache::default();
        // No cache entry → shortened DID with an ellipsis.
        let long = "did:plc:abcdefghijklmnopqrstuvwx";
        let shown = display_name_for_did(&cache, long);
        assert!(shown.ends_with('…'));
        assert!(shown.starts_with("did:plc:"));
        assert!(shown.chars().count() <= 17);
        // Short identifiers pass through untouched.
        assert_eq!(display_name_for_did(&cache, "did:web:x"), "did:web:x");
    }
}
