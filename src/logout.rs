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

use crate::protocol::OverlandsMessage;
use crate::state::{AppState, ChatHistory, DiagnosticsLog, LocalPlayer, RelayHost, RemotePeer};

pub struct LogoutPlugin;

impl Plugin for LogoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(AppState::InGame), cleanup_on_logout);
    }
}

fn cleanup_on_logout(
    mut commands: Commands,
    players: Query<Entity, With<LocalPlayer>>,
    peers: Query<Entity, With<RemotePeer>>,
    mut chat: ResMut<ChatHistory>,
    mut diagnostics: ResMut<DiagnosticsLog>,
) {
    // Despawn game-world entities (recursive by default in Bevy 0.18).
    for e in &players {
        commands.entity(e).despawn();
    }
    for e in &peers {
        commands.entity(e).despawn();
    }

    // Tear down session + networking resources. Removing SymbiosMultiuserConfig
    // triggers bevy_symbios_multiuser to close the matchbox socket next frame.
    commands.remove_resource::<AtprotoSession>();
    commands.remove_resource::<TokenSourceRes>();
    commands.remove_resource::<SymbiosMultiuserConfig<OverlandsMessage>>();
    commands.remove_resource::<RelayHost>();

    // Reset in-memory buffers so the next session starts fresh.
    chat.messages.clear();
    *diagnostics = DiagnosticsLog::default();
}
