//! Shared ECS state: the `AppState` enum driving the login/loading/ingame
//! state machine, marker components for the local player and remote peers,
//! the per-peer transform jitter buffer, rolling chat and diagnostics logs,
//! and the GUI-editable physics and airship parameter resources.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::protocol::AirshipParams;

/// Application state machine. `Loading` waits on *both* the async heightmap
/// generation task and the ATProto PDS room-record fetch before handing off
/// to `InGame`, so the terrain collider is solid and the world-builder has a
/// recipe to compile the moment the first gameplay frame runs.
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

/// Social-graph resonance state derived from the unauthenticated ATProto
/// `getRelationships` lexicon call.  Updated asynchronously after the peer's
/// Identity arrives so the game loop is never blocked on network I/O.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SocialResonance {
    /// State not yet queried or in flight.
    #[default]
    Unknown,
    /// Query finished: the local actor and remote peer do **not** follow each
    /// other bidirectionally.
    None,
    /// Query finished: both `following` and `followedBy` were present.
    Mutual,
}

/// Per-peer transform sample captured off the network.
#[derive(Clone, Copy, Debug)]
pub struct TransformSample {
    pub position: Vec3,
    pub rotation: Quat,
    /// Seconds since application start, taken from `Time::elapsed_secs_f64`.
    pub timestamp: f64,
}

/// Ring buffer of incoming transform samples, used by the kinematic-smoothing
/// system to hide single-packet drops with Hermite interpolation.
#[derive(Component, Default)]
pub struct TransformBuffer {
    pub samples: VecDeque<TransformSample>,
}

/// Rolling chat history shown in the HUD.
/// Each entry is `(author, text, timestamp_label)`.
#[derive(Resource, Default)]
pub struct ChatHistory {
    pub messages: Vec<(String, String, String)>,
}

/// Rolling diagnostic event log with session-relative timestamps.
#[derive(Resource, Default)]
pub struct DiagnosticsLog {
    entries: std::collections::VecDeque<(String, String)>,
}

impl DiagnosticsLog {
    /// Push a new entry. `elapsed_secs` comes from `Time::elapsed_secs_f64`.
    pub fn push(&mut self, elapsed_secs: f64, entry: String) {
        let total = elapsed_secs as u64;
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        let ts = if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m:02}:{s:02}")
        };
        self.entries.push_back((ts, entry));
        if self.entries.len() > crate::config::state::MAX_DIAGNOSTICS_ENTRIES {
            self.entries.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.entries.iter()
    }
}

/// Relay hostname captured at login, used when building the room URL.
#[derive(Resource, Clone)]
pub struct RelayHost(pub String);

/// The DID of the room (overland) we are currently visiting.
/// If the user leaves the login field blank, this defaults to their own DID
/// (i.e. "home").
#[derive(Resource, Clone)]
pub struct CurrentRoomDid(pub String);

/// Present when the room-record fetch fell through to the default homeworld
/// because the PDS response could not be decoded against the current
/// `RoomRecord` schema (e.g. an old record saved against a since-changed
/// lexicon). The world editor shows a recovery banner and a "Reset PDS to
/// default" button while this resource is set, so the owner can deliberately
/// overwrite the incompatible stored record instead of being stuck in a
/// retry loop during Loading.
#[derive(Resource, Debug, Clone)]
pub struct RoomRecordRecovery {
    /// Human-readable decode error reported by `serde_json` / reqwest, shown
    /// in the banner so the owner understands why recovery is active.
    pub reason: String,
}

/// Local player's runtime physics tuning parameters.
/// Initialised from `config::rover` defaults and editable via the physics GUI.
#[derive(Resource)]
pub struct LocalPhysicsParams {
    // --- Suspension ---
    pub suspension_rest_length: f32,
    pub suspension_stiffness: f32,
    pub suspension_damping: f32,
    // --- Drive ---
    pub drive_force: f32,
    pub turn_torque: f32,
    pub lateral_grip: f32,
    pub jump_force: f32,
    pub uprighting_torque: f32,
    // --- Chassis ---
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub mass: f32,
    // --- Buoyancy (swimming) ---
    pub water_rest_length: f32,
    pub buoyancy_strength: f32,
    pub buoyancy_damping: f32,
    pub buoyancy_max_depth: f32,
}

impl Default for LocalPhysicsParams {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            suspension_rest_length: cfg::SUSPENSION_REST_LENGTH,
            suspension_stiffness: cfg::SUSPENSION_STIFFNESS,
            suspension_damping: cfg::SUSPENSION_DAMPING,
            drive_force: cfg::DRIVE_FORCE,
            turn_torque: cfg::TURN_TORQUE,
            lateral_grip: cfg::LATERAL_GRIP,
            jump_force: cfg::JUMP_FORCE,
            uprighting_torque: cfg::UPRIGHTING_TORQUE,
            linear_damping: cfg::LINEAR_DAMPING,
            angular_damping: cfg::ANGULAR_DAMPING,
            mass: cfg::MASS,
            water_rest_length: cfg::WATER_REST_LENGTH,
            buoyancy_strength: cfg::BUOYANCY_STRENGTH,
            buoyancy_damping: cfg::BUOYANCY_DAMPING,
            buoyancy_max_depth: cfg::BUOYANCY_MAX_DEPTH,
        }
    }
}

/// Local player's current airship construction / material parameters.
/// Edited via the airship GUI and broadcast inside every Identity message.
/// Set `needs_rebuild = true` after changing `params` to trigger a mesh rebuild.
#[derive(Resource)]
pub struct LocalAirshipParams {
    pub params: AirshipParams,
    /// Signals `rebuild_local_rover` to regenerate the visual children this frame.
    pub needs_rebuild: bool,
    /// When true, remote peer transforms are smoothed with a Hermite spline
    /// applied to a delayed jitter buffer.  When false, peers snap to the
    /// latest received packet (useful for debugging raw network latency).
    pub smooth_kinematics: bool,
}

impl Default for LocalAirshipParams {
    fn default() -> Self {
        Self {
            params: AirshipParams::default(),
            needs_rebuild: false,
            smooth_kinematics: true,
        }
    }
}
