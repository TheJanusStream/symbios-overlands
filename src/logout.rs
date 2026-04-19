//! Logout cleanup: despawn game entities and remove session/game resources
//! when transitioning from [`AppState::InGame`](crate::state::AppState::InGame)
//! back to [`AppState::Login`](crate::state::AppState::Login).
//!
//! Runs on `OnExit(AppState::InGame)`. Removing the
//! [`SymbiosMultiuserConfig`](bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig)
//! resource tears down the existing matchbox socket on the next frame
//! (see bevy_symbios_multiuser docs).

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use bevy_symbios_multiuser::prelude::SymbiosMultiuserConfig;
use bevy_symbios_multiuser::signaller::TokenSourceRes;

use crate::pds::RoomRecord;
use crate::protocol::OverlandsMessage;
use crate::state::{
    AppState, ChatHistory, DiagnosticsLog, LiveAvatarRecord, LocalPlayer, RelayHost, RemotePeer,
    RoomRecordRecovery, StoredAvatarRecord, StoredRoomRecord, TravelRequest,
};
use crate::world_builder::RoomEntity;

pub struct LogoutPlugin;

impl Plugin for LogoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::InGame), cleanup_on_logout);
    }
}

#[allow(clippy::too_many_arguments)]
fn cleanup_on_logout(
    mut commands: Commands,
    players: Query<Entity, With<LocalPlayer>>,
    peers: Query<Entity, With<RemotePeer>>,
    room_entities: Query<Entity, With<RoomEntity>>,
    mut chat: ResMut<ChatHistory>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    travel_req: Option<Res<TravelRequest>>,
    relay_host: Option<Res<RelayHost>>,
) {
    // Despawn game-world entities (recursive by default in Bevy 0.18).
    //
    // `try_despawn` swallows the `EntityMutableFetchError` that fires
    // when an entity has already been despawned this frame — which can
    // happen when a parent's recursive despawn reaches a child before
    // the child's own queue entry runs, or when a deferred closure
    // queued by a gameplay system (e.g. `commands.queue(...)` in the
    // avatar paint pipeline) lands in the same apply pass. The warnings
    // are harmless but noisy; using `try_despawn` keeps the log clean
    // without masking genuine lifecycle bugs elsewhere.
    for e in &players {
        commands.entity(e).try_despawn();
    }
    for e in &peers {
        commands.entity(e).try_despawn();
    }
    // Also drop every world-compiler output (L-systems, scatter props,
    // water volumes). `terrain.rs` despawns the heightfield on its own
    // `OnExit(InGame)` hook, but the world builder does not — without
    // this loop, trees and shapes from the previous room would sit
    // orphaned in the ECS until the next room loaded.
    for e in &room_entities {
        commands.entity(e).try_despawn();
    }

    // Drop the active recipe so a later login does not compile the old
    // room's contents into the new session's scene graph.
    commands.remove_resource::<RoomRecord>();
    commands.remove_resource::<StoredRoomRecord>();
    commands.remove_resource::<LiveAvatarRecord>();
    commands.remove_resource::<StoredAvatarRecord>();
    // Clear any recovery marker from this session so a fresh login does
    // not start with the "incompatible record" banner still showing.
    commands.remove_resource::<RoomRecordRecovery>();

    if let Some(req) = travel_req {
        // Seamless portal travel: keep the authenticated session and the
        // relay host, only swap the matchbox socket config so the client
        // reconnects to the target room's URL on the next frame. Without
        // this branch, a portal jump would drop the user back to the login
        // screen (losing credentials) instead of streaming directly into
        // the destination overland.
        commands.remove_resource::<SymbiosMultiuserConfig<OverlandsMessage>>();
        if let Some(host) = relay_host {
            commands.insert_resource(SymbiosMultiuserConfig::<OverlandsMessage> {
                room_url: format!("wss://{}/overlands/{}", host.0, req.target_did),
                ice_servers: None,
                _marker: std::marker::PhantomData,
            });
        }
    } else {
        // Hard logout path: tear down every session + networking resource.
        // Removing SymbiosMultiuserConfig triggers bevy_symbios_multiuser to
        // close the matchbox socket on the next frame.
        commands.remove_resource::<AtprotoSession>();
        commands.remove_resource::<TokenSourceRes>();
        commands.remove_resource::<SymbiosMultiuserConfig<OverlandsMessage>>();
        commands.remove_resource::<RelayHost>();
    }

    // Reset in-memory buffers so the next session starts fresh.
    chat.messages.clear();
    *diagnostics = DiagnosticsLog::default();
}
