//! Shared ECS state: the `AppState` enum driving the login/loading/ingame
//! state machine, marker components for the local player and remote peers,
//! the per-peer transform jitter buffer, rolling chat and diagnostics logs,
//! and the live/stored avatar + room + inventory record resources backing
//! the "Live UX" editor paradigm.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::pds::{AvatarRecord, Generator, InventoryRecord, RoomRecord};

/// Application state machine. `Loading` waits on the async heightmap
/// generation task, the ATProto PDS room-record fetch, the local
/// avatar-record fetch, *and* the local inventory-record fetch before
/// handing off to `InGame`, so the terrain collider is solid and every
/// recipe (room + avatar + inventory) is resident when the first gameplay
/// frame runs.
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
    /// Last-applied avatar record from this peer (used to detect changes and
    /// hot-swap archetypes). `None` until the async PDS fetch completes.
    pub avatar: Option<AvatarRecord>,
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

/// Inserted when the player touches an inter-room portal. Freezes local
/// movement and triggers an async fetch of the target room record while
/// keeping the player in AppState::InGame.
#[derive(Resource, Clone)]
pub struct TravelingTo {
    pub target_did: String,
    pub target_pos: Vec3,
}

/// Most recent result of a "Publish to PDS" attempt from the World or
/// Avatar editor. The UI watches this resource to render a status line
/// beside the Publish/Load/Reset buttons so the owner gets visual confirmation
/// that the PDS round-trip actually succeeded instead of relying on the
/// console log.
#[derive(Resource, Clone, Debug, Default)]
pub enum PublishFeedback {
    #[default]
    Idle,
    Publishing,
    Success {
        at_secs: f64,
    },
    Failed {
        at_secs: f64,
        message: String,
    },
}

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

/// The local player's **live** avatar record — what the editor sliders
/// mutate in real time and what gets broadcast to peers. Diverges from
/// `StoredAvatarRecord` until the owner presses "Publish" (or reverts).
#[derive(Resource, Clone)]
pub struct LiveAvatarRecord(pub AvatarRecord);

/// The last known PDS-persisted avatar record. Populated by the loading
/// fetch and replaced on a successful publish; used by the "Load from PDS" button
/// to restore the sliders to the committed state.
#[derive(Resource, Clone)]
pub struct StoredAvatarRecord(pub AvatarRecord);

/// The last known PDS-persisted room record. The live `RoomRecord`
/// resource is mutated immediately by the world editor; this one stays
/// pinned to the committed state so "Load from PDS" can discard
/// uncommitted edits and the Publish button's dirty indicator has a
/// reference point to diff against.
#[derive(Resource, Clone)]
pub struct StoredRoomRecord(pub RoomRecord);

/// Local-only UX preferences that are *not* stored on the PDS (they
/// describe how this client renders the world, not the world itself).
#[derive(Resource)]
pub struct LocalSettings {
    /// When true, remote peer transforms are smoothed with a Hermite spline
    /// applied to a delayed jitter buffer.  When false, peers snap to the
    /// latest received packet (useful for debugging raw network latency).
    pub smooth_kinematics: bool,
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            smooth_kinematics: true,
        }
    }
}

/// The owner's **live** inventory record — the in-memory copy the Inventory
/// window mutates in place. Divergence from [`StoredInventoryRecord`] drives
/// the "Publish to PDS" button's dirty indicator.
#[derive(Resource, Clone)]
pub struct LiveInventoryRecord(pub InventoryRecord);

/// Last known PDS-persisted inventory record. Populated by the loading
/// fetch and replaced on a successful publish; nothing else should mutate
/// it so the dirty check against `LiveInventoryRecord` stays meaningful.
#[derive(Resource, Clone)]
pub struct StoredInventoryRecord(pub InventoryRecord);

/// Most recent outcome of an inventory publish attempt. Mirrors
/// [`PublishFeedback`] but is kept separate so publishing the inventory
/// doesn't clobber the status line rendered next to the room editor's own
/// Publish button.
#[derive(Resource, Clone, Debug, Default)]
pub enum InventoryPublishFeedback {
    #[default]
    Idle,
    Publishing,
    Success {
        at_secs: f64,
    },
    Failed {
        at_secs: f64,
        message: String,
    },
}

/// A currently-displayed incoming item-offer modal. Exactly one can be
/// active at a time — this is an explicit anti-spam measure: concurrent
/// offers from other peers are auto-declined with a "busy" reply so a
/// malicious client cannot flood a victim with request dialogs or tie
/// their client up answering queued prompts.
///
/// Muted senders never reach this resource — see
/// `handle_incoming_messages` in `network.rs`, which short-circuits
/// muted-peer offers into a silent auto-decline before the dialog is
/// constructed.
#[derive(Resource, Clone, Debug)]
pub struct IncomingOfferDialog {
    pub offer_id: u64,
    pub sender_peer_id: bevy_symbios_multiuser::prelude::PeerId,
    pub sender_did: String,
    pub sender_handle: String,
    pub item_name: String,
    pub generator: Generator,
    /// Session-relative seconds the offer arrived; diagnostics entries and
    /// any future timeout logic key off this.
    pub arrived_at_secs: f64,
}

/// A gift that the local user has sent to a peer but hasn't yet received
/// a response for. Keyed by `offer_id` (the sender-chosen token echoed by
/// the recipient). The resource is a thin map because a single user may
/// fire off several offers to different peers before any response comes
/// back; the per-entry target DID lets us authenticate responses and drop
/// spoofed replies from unrelated peers.
#[derive(Resource, Default, Debug)]
pub struct PendingOutgoingOffers {
    pub by_id: std::collections::HashMap<u64, PendingOutgoingOffer>,
    /// Monotonic counter for generating fresh offer_ids. Scoped per-client
    /// so the ids only have to be unique within this session — the peer
    /// echoes the value back unchanged, and we correlate by id alone.
    pub next_id: u64,
}

#[derive(Clone, Debug)]
pub struct PendingOutgoingOffer {
    pub target_did: String,
    pub target_handle: String,
    pub item_name: String,
    pub sent_at_secs: f64,
}

impl PendingOutgoingOffers {
    /// Allocate a fresh `offer_id` and insert the pending record. Returns
    /// the allocated id so the caller can ship it in the [`OverlandsMessage::ItemOffer`].
    pub fn register(
        &mut self,
        target_did: String,
        target_handle: String,
        item_name: String,
        sent_at_secs: f64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.by_id.insert(
            id,
            PendingOutgoingOffer {
                target_did,
                target_handle,
                item_name,
                sent_at_secs,
            },
        );
        id
    }
}
