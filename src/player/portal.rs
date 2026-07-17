//! Portal travel: reading the local player's collision-sensor set and
//! driving the async ATProto room-record fetch that carries them to a new
//! world.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::EventPayload;
use crate::pds::{FetchError, RoomRecord, fetch_room_record};
use crate::state::{
    CurrentRoomDid, LiveRoomRecord, LocalPlayer, RemotePeer, RoomRecordRecovery, TravelingTo,
};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};
use crate::world_builder::PortalMarker;

#[derive(Component)]
pub(super) struct PortalTravelTask(
    pub(super) bevy::tasks::Task<Result<Option<RoomRecord>, FetchError>>,
);

/// Suppresses portal interactions for a brief window after a successful
/// teleport. Without this, a portal whose `target_pos` lands the player
/// inside any portal collider (the source itself or one at the
/// destination) would re-fire `handle_portal_interaction` every frame:
/// each invocation snaps the player back to `target_pos` and zeros their
/// velocity, preventing the player from ever moving out of the overlap
/// — a permanent soft-lock until logout.
///
/// `until_secs` is taken against `Time::elapsed_secs_f64()`; the cooldown
/// is "long enough for a humanoid to walk out of a typical portal
/// collider, short enough that deliberate re-entry feels responsive."
#[derive(Resource)]
pub struct PortalCooldown {
    pub until_secs: f64,
}

const PORTAL_COOLDOWN_SECS: f64 = 0.75;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_portal_interaction(
    mut commands: Commands,
    mut players: Query<
        (
            &CollidingEntities,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<LocalPlayer>,
    >,
    portals: Query<&PortalMarker>,
    current_room: Option<Res<CurrentRoomDid>>,
    traveling: Option<Res<TravelingTo>>,
    guard: Option<Res<UnsavedGuard>>,
    cooldown: Option<Res<PortalCooldown>>,
    time: Res<Time>,
) {
    // Guard against re-entry: once a travel task is in flight, the player
    // keeps coasting through the portal collider for several frames. Without
    // this check the Update system would spawn a fresh IoTaskPool fetch each
    // frame, flooding the pool and stalling every other background task.
    if traveling.is_some() {
        return;
    }
    // An unsaved-edits dialog is already pending (for this portal or for
    // a logout) — don't stack another action behind it.
    if guard.is_some() {
        return;
    }
    // Post-teleport cooldown: keeps a portal-overlapping arrival from
    // immediately re-firing this system and locking the player into a
    // single point. See [`PortalCooldown`].
    let now = time.elapsed_secs_f64();
    if let Some(cd) = cooldown.as_deref()
        && now < cd.until_secs
    {
        return;
    }

    let Ok((collisions, mut tf, mut lv, mut av)) = players.single_mut() else {
        return;
    };

    for entity in collisions.iter() {
        let Ok(portal) = portals.get(*entity) else {
            continue;
        };

        let same_room = current_room
            .as_deref()
            .map(|r| r.0 == portal.target_did)
            .unwrap_or(false);
        if same_room {
            tf.translation = portal.target_pos;
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
            // Engage the cooldown so a portal whose target_pos lies
            // inside its own (or another portal's) collider can't
            // soft-lock the player by re-firing each frame.
            commands.insert_resource(PortalCooldown {
                until_secs: now + PORTAL_COOLDOWN_SECS,
            });
        } else {
            // Inter-room portal: Freeze the player and hand the travel to
            // the unsaved-edits guard. Zero momentum so the player doesn't
            // re-collide with the portal on the next frame.
            //
            // The guard owns the dirty decision: when the local user has
            // no unpublished room edits it calls [`begin_portal_travel`]
            // on the very next frame without showing anything; when they
            // do, it offers Publish / Discard / Stay first. Starting the
            // fetch directly here would bypass that choice and silently
            // overwrite the live record.
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
            commands.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
                target_did: portal.target_did.clone(),
                target_pos: Some(portal.target_pos),
            }));
        }
        break;
    }
}

/// Start the inter-room travel: pin [`TravelingTo`] (which suppresses
/// further portal interaction until the swap completes or fails) and
/// dispatch the async destination room-record fetch. Called by the
/// unsaved-edits guard once any dirty-record question is settled.
/// `target_pos: None` (#745) arrives at the destination record's
/// `default_landing` — see [`TravelingTo`].
pub(crate) fn begin_portal_travel(
    commands: &mut Commands,
    target_did: String,
    target_pos: Option<Vec3>,
) {
    commands.insert_resource(TravelingTo {
        target_did: target_did.clone(),
        target_pos,
    });

    let pool = bevy::tasks::IoTaskPool::get();
    // `reqwest` spawns internal timer/IO futures the moment it issues
    // a request, which panics with "there is no reactor running"
    // unless the future is driven inside a tokio runtime. The
    // `IoTaskPool` is a plain async-executor, so on native we build
    // a per-task single-threaded runtime (same pattern as every
    // other HTTP-spawning site in the crate — see
    // `network::spawn_peer_avatar_fetch` /
    // `lib::spawn_avatar_record_fetch`). wasm32 has no tokio; the
    // browser's JS runtime backs `fetch`, so the bare future works.
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            fetch_room_record(&client, &target_did).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(PortalTravelTask(task));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn poll_portal_travel_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalTravelTask)>,
    traveling: Option<Res<TravelingTo>>,
    mut room_record: Option<ResMut<LiveRoomRecord>>,
    mut stored_room: Option<ResMut<crate::state::StoredRoomRecord>>,
    mut current_did: Option<ResMut<CurrentRoomDid>>,
    mut chat: ResMut<crate::state::ChatHistory>,
    mut session_log: ResMut<SessionLog>,
    relay_host: Option<Res<crate::state::RelayHost>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut players: Query<
        (&mut Transform, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
    time: Res<Time>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    profile_cache: Res<crate::avatar::BskyProfileCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut task.0),
        ) else {
            continue;
        };

        commands.entity(entity).despawn();
        let Some(travel_data) = traveling.as_deref() else {
            continue;
        };

        // 1. Resolve the new record. The four arms preserve the original
        // owner's PDS contents — substituting the default on a transient
        // network failure would silently destroy the destination user's
        // real room as soon as they (or any autosave hook) clicked
        // "Save to PDS". Mirrors the loading-pipeline policy in
        // `loading::poll_room_record_task`.
        let elapsed = time.elapsed_secs_f64();
        let mut new_record = match result {
            // Owner has saved a record — install it. A clean install also
            // clears any recovery banner a previous room raised (#840):
            // carrying it into this room would offer a "Reset PDS to
            // default" against a HEALTHY record.
            Ok(Some(r)) => {
                commands.remove_resource::<RoomRecordRecovery>();
                r
            }
            // 404: the destination owner has never customised their
            // overland. Synthesising the default is safe — and clean, so
            // the stale-banner sweep applies here too (#840).
            Ok(None) => {
                commands.remove_resource::<RoomRecordRecovery>();
                RoomRecord::default_for_did(&travel_data.target_did)
            }
            // Schema-incompatible record on the PDS: not transient, so
            // retrying won't help. Fall through to the default and raise
            // a recovery banner the destination owner can clear by
            // re-publishing.
            Err(FetchError::Decode(msg)) => {
                session_log.warn(
                    elapsed,
                    EventPayload::RoomRecoveryBannerRaised {
                        reason: msg.clone(),
                    },
                );
                warn!(
                    "Portal travel decode error ({}) — installing default + recovery marker",
                    msg
                );
                commands.insert_resource(RoomRecordRecovery {
                    reason: msg.clone(),
                });
                RoomRecord::default_for_did(&travel_data.target_did)
            }
            // Transient failure (DNS, timeout, 5xx, DID resolution
            // hiccup): abort the travel and keep the player in the
            // current room. The destination owner's real record stays
            // safe; the user can walk into the portal again to retry.
            Err(err) => {
                session_log.warn(
                    elapsed,
                    EventPayload::PortalTravelFailed {
                        target_did: travel_data.target_did.clone(),
                        reason: format!("{err:?}"),
                    },
                );
                warn!("Portal travel fetch failed: {:?} — aborting travel", err);
                // The player just unfreezes with zero explanation without
                // this — "the portal did nothing" (#842).
                toasts.error(
                    format!(
                        "Couldn't reach {}'s world — walk into the portal again to retry.",
                        crate::ui::travel::display_name_for_did(
                            &profile_cache,
                            &travel_data.target_did,
                        )
                    ),
                    elapsed,
                );
                commands.remove_resource::<TravelingTo>();
                // Brief cooldown so the player isn't immediately
                // re-pulled into the same failing fetch by their own
                // collision overlap with the source portal.
                commands.insert_resource(PortalCooldown {
                    until_secs: elapsed + PORTAL_COOLDOWN_SECS,
                });
                continue;
            }
        };
        new_record.sanitize();
        // Captured before the record moves into the resources below: a
        // travel with no baked target (gateway, #745) arrives at the
        // destination owner's configured landing pose.
        let destination_landing = new_record.default_landing;

        // 2. Hot-swap the ECS Resources (Triggers `world_builder` automatically!)
        if let Some(rec) = room_record.as_mut() {
            rec.0 = new_record.clone();
        }
        if let Some(stored) = stored_room.as_mut() {
            **stored = crate::state::StoredRoomRecord(new_record);
        }
        if let Some(did) = current_did.as_mut() {
            did.0 = travel_data.target_did.clone();
        }

        // 3. Hot-swap the WebRTC Socket
        commands.remove_resource::<bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig<
            crate::protocol::OverlandsMessage,
        >>();
        if let Some(host) = relay_host.as_deref() {
            commands.insert_resource(bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig::<
                crate::protocol::OverlandsMessage,
            > {
                room_url: format!("wss://{}/overlands/{}", host.0, travel_data.target_did),
                ice_servers: crate::config::network::ice_servers(),
                _marker: std::marker::PhantomData,
            });
        }

        // 3a. Despawn the origin-region's remote peers. Tearing down the
        // multiuser socket above *should* surface `Disconnected` events for
        // each peer, but those events fan through the plugin's own systems
        // next frame and are not guaranteed to sweep the ECS entities —
        // leaving mute, frozen chassis sitting at the origin's last
        // broadcast transform in the new region. The fresh socket's
        // `Connected` events will re-spawn each peer we still share a
        // room with. `try_despawn` tolerates the case where a parent
        // despawn (e.g. an in-flight avatar rebuild queued this frame)
        // already took a child down.
        for peer_entity in &peers {
            commands.entity(peer_entity).try_despawn();
        }

        // 4. Teleport player and clear momentum. A baked portal target is
        // used verbatim (classic portals, translation only — facing is
        // left alone, as ever). Without one, the destination's
        // `default_landing` supplies position *and* facing; without that,
        // the legacy origin scatter. Landing heights are provisional when
        // the pose is drop-pin (`y: None`) — the destination heightmap
        // doesn't exist yet at this point (the record swap above only
        // *queued* the terrain rebuild), so we park at y = 0 and let
        // `lift_player_above_new_ground` snap the chassis onto the new
        // ground the frame the heightmap lands, exactly as it already does
        // for re-seeds and stale baked targets.
        if let Ok((mut tf, mut lv, mut av)) = players.single_mut() {
            let (arrival, yaw_deg) = match travel_data.target_pos {
                Some(pos) => (pos, None),
                None => match destination_landing {
                    Some(landing) => (
                        Vec3::new(
                            landing.pos.0[0],
                            landing.y.map(|y| y.0).unwrap_or(0.0),
                            landing.pos.0[1],
                        ),
                        Some(landing.yaw_deg.0),
                    ),
                    None => {
                        let (x, z) = super::random_spawn_xz();
                        (Vec3::new(x, 0.0, z), None)
                    }
                },
            };
            tf.translation = arrival;
            if let Some(deg) = yaw_deg {
                tf.rotation = Quat::from_rotation_y(deg.to_radians());
            }
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
        }

        // 5. Clean up state
        chat.messages.clear();
        // Arrival line (#842): the hard cut wipes the world AND the chat
        // history in the same frame — say where we landed and why the
        // scrollback vanished.
        chat.push(
            None,
            "system",
            format!(
                "Arrived in {}'s world — chat history starts fresh here.",
                crate::ui::travel::display_name_for_did(&profile_cache, &travel_data.target_did)
            ),
        );
        commands.remove_resource::<TravelingTo>();
        // Engage the post-arrival cooldown so a portal at the
        // destination whose collider envelops the spawn position can't
        // immediately yank the player back through.
        commands.insert_resource(PortalCooldown {
            until_secs: elapsed + PORTAL_COOLDOWN_SECS,
        });
    }
}
