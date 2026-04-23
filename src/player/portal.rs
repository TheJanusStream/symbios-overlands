//! Portal travel: reading the local player's collision-sensor set and
//! driving the async ATProto room-record fetch that carries them to a new
//! world.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{FetchError, RoomRecord, fetch_room_record};
use crate::state::{CurrentRoomDid, LocalPlayer, RemotePeer, TravelingTo};
use crate::world_builder::PortalMarker;

#[derive(Component)]
pub(super) struct PortalTravelTask(
    pub(super) bevy::tasks::Task<Result<Option<RoomRecord>, FetchError>>,
);

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
) {
    // Guard against re-entry: once a travel task is in flight, the player
    // keeps coasting through the portal collider for several frames. Without
    // this check the Update system would spawn a fresh IoTaskPool fetch each
    // frame, flooding the pool and stalling every other background task.
    if traveling.is_some() {
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
    relay_host: Option<Res<crate::state::RelayHost>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut players: Query<
        (&mut Transform, &mut LinearVelocity, &mut AngularVelocity),
        With<LocalPlayer>,
    >,
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

        // 1. Resolve the new record (or default if 404)
        let mut new_record = match result {
            Ok(Some(r)) => r,
            Ok(None) | Err(_) => RoomRecord::default_for_did(&travel_data.target_did),
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
    }
}
