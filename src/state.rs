use bevy::prelude::*;

use crate::protocol::AirshipParams;

/// Application state machine. Terrain must be solid before entering InGame.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Login,
    Loading,
    InGame,
}

/// Marks the local player's chassis entity.
#[derive(Component)]
pub struct LocalPlayer;

/// Marks a remote peer's visual entity.
#[derive(Component)]
pub struct RemotePeer {
    pub peer_id: bevy_symbios_multiuser::prelude::PeerId,
    pub did: Option<String>,
    pub handle: Option<String>,
    /// When true: chat messages are ignored and the vessel is hidden.
    pub muted: bool,
    /// Last-received vessel design from this peer (used to detect changes).
    pub airship: Option<AirshipParams>,
}

/// Rolling chat history shown in the HUD.
#[derive(Resource, Default)]
pub struct ChatHistory {
    pub messages: Vec<(String, String)>,
}

/// Rolling diagnostic event log.
#[derive(Resource, Default)]
pub struct DiagnosticsLog {
    entries: std::collections::VecDeque<String>,
}

impl DiagnosticsLog {
    pub fn push(&mut self, entry: String) {
        self.entries.push_back(entry);
        if self.entries.len() > crate::config::state::MAX_DIAGNOSTICS_ENTRIES {
            self.entries.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.entries.iter()
    }
}

/// Relay hostname captured at login, used when building the room URL.
#[derive(Resource, Clone)]
pub struct RelayHost(pub String);

/// Local player's current airship construction / material parameters.
/// Edited via the airship GUI and broadcast inside every Identity message.
/// Set `needs_rebuild = true` after changing `params` to trigger a mesh rebuild.
#[derive(Resource, Default)]
pub struct LocalAirshipParams {
    pub params: AirshipParams,
    /// Signals `rebuild_local_rover` to regenerate the visual children this frame.
    pub needs_rebuild: bool,
}
