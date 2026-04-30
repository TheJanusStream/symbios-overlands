//! Portal travel: reading the local player's collision-sensor set and
//! driving the async ATProto room-record fetch that carries them to a new
//! world.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{FetchError, RoomRecord, fetch_room_record};
use crate::state::{
    CurrentRoomDid, DiagnosticsLog, LocalPlayer, RemotePeer, RoomRecordRecovery, TravelingTo,
};
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
pub(super) struct PortalCooldown {
    pub(super) until_secs: f64,
}

const PORTAL_COOLDOWN_SECS: f64 = 0.75;

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
            // Inter-room portal: Freeze the player and start the async fetch.
            // Zero momentum so the player doesn't re-collide with the portal
            // on the next frame before the travel task resolves.
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
            commands.insert_resource(TravelingTo {
                target_did: portal.target_did.clone(),
                target_pos: portal.target_pos,
            });

            let did_clone = portal.target_did.clone();
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
                    fetch_room_record(&client, &did_clone).await
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
            commands.spawn(PortalTravelTask(task));
        }
        break;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn poll_portal_travel_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalTravelTask)>,
    traveling: Option<Res<TravelingTo>>,
    mut room_record: Option<ResMut<RoomRecord>>,
    mut stored_room: Option<ResMut<crate::state::StoredRoomRecord>>,
    mut current_did: Option<ResMut<CurrentRoomDid>>,
    mut chat: ResMut<crate::state::ChatHistory>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    relay_host: Option<Res<crate::state::RelayHost>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut players: Query<
        (&mut Transform, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
    time: Res<Time>,
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
        // "Publish to PDS". Mirrors the loading-pipeline policy in
        // `loading::poll_room_record_task`.
        let elapsed = time.elapsed_secs_f64();
        let mut new_record = match result {
            // Owner has saved a record — install it.
            Ok(Some(r)) => r,
            // 404: the destination owner has never customised their
            // overland. Synthesising the default is safe.
            Ok(None) => RoomRecord::default_for_did(&travel_data.target_did),
            // Schema-incompatible record on the PDS: not transient, so
            // retrying won't help. Fall through to the default and raise
            // a recovery banner the destination owner can clear by
            // re-publishing.
            Err(FetchError::Decode(msg)) => {
                diagnostics.push(
                    elapsed,
                    format!(
                        "Portal travel: stored room record incompatible ({msg}) — using default"
                    ),
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
                diagnostics.push(
                    elapsed,
                    format!("Portal travel failed ({err:?}) — staying put. Try again."),
                );
                warn!("Portal travel fetch failed: {:?} — aborting travel", err);
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

        // 2. Hot-swap the ECS Resources (Triggers world_builder.rs automatically!)
        if let Some(rec) = room_record.as_mut() {
            **rec = new_record.clone();
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
                ice_servers: None,
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

        // 4. Teleport player and clear momentum
        if let Ok((mut tf, mut lv, mut av)) = players.single_mut() {
            tf.translation = travel_data.target_pos;
            lv.0 = Vec3::ZERO;
            av.0 = Vec3::ZERO;
        }

        // 5. Clean up state
        chat.messages.clear();
        commands.remove_resource::<TravelingTo>();
        // Engage the post-arrival cooldown so a portal at the
        // destination whose collider envelops the spawn position can't
        // immediately yank the player back through.
        commands.insert_resource(PortalCooldown {
            until_secs: elapsed + PORTAL_COOLDOWN_SECS,
        });
    }
}
