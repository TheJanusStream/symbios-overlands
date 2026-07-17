//! Shared ECS state: the `AppState` enum driving the login/loading/ingame
//! state machine, marker components for the local player and remote peers,
//! the rolling chat log, and the live/stored avatar + room + inventory
//! record resources backing the "Live UX" editor paradigm. (The per-peer
//! transform jitter buffer lives in `network`, and the diagnostics session
//! log lives in `diagnostics`.)
//!
//! Peer-to-peer item-offer bookkeeping also lives here:
//! [`IncomingOfferDialog`] is the single active "someone sent you a gift"
//! modal (concurrent offers are auto-declined with "busy" at the network
//! layer), and [`PendingOutgoingOffers`] tracks offers the local user has
//! sent but not yet received a response for, keyed by a session-unique
//! `offer_id` the recipient echoes back in its reply.

use std::marker::PhantomData;

use bevy::prelude::*;

use crate::pds::{AvatarRecord, Generator, InventoryRecord, RoomRecord};

/// Application state machine. `Loading` waits on all six loading tasks —
/// the async heightmap generation task, the ATProto PDS room-record fetch,
/// the local avatar-record fetch, the local inventory-record fetch, the
/// seeded ambient-audio bake, *and* the room compile (`WorldCompiled`) —
/// before handing off to `InGame`, so the terrain collider is solid, every
/// recipe (room + avatar + inventory) is resident, the ambient bed is ready
/// to play, and the world's entities exist when the first gameplay frame
/// runs.
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

/// One row in the rolling chat HUD. The optional `did` is filled in when
/// the message originated from a known peer entity (or from the local
/// session) so the chat panel can look up the author's bsky profile
/// picture in [`crate::avatar::BskyProfileCache`] and render it as a
/// small icon next to the handle. Messages with no DID — e.g. an
/// unauthenticated test write — render text-only with a placeholder.
#[derive(Clone, Debug)]
pub struct ChatEntry {
    pub did: Option<String>,
    pub author: String,
    pub text: String,
    /// Wall-clock arrival time as Unix seconds (#846). The old field was
    /// a pre-formatted minutes-since-app-launch string — meaningless
    /// across peers and sessions. Raw epoch here; the HUD renders local
    /// HH:MM via [`clock_hhmm`].
    pub at_epoch_secs: i64,
}

/// Current wall-clock time as Unix seconds. `chrono`'s clock is backed
/// by JS `Date` on wasm (`wasmbind`), so this is safe on both targets —
/// `std::time::SystemTime` panics on wasm32 (known gotcha).
pub fn now_epoch_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Render an epoch stamp as the viewer's local `HH:MM` (#846).
pub fn clock_hhmm(epoch_secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(epoch_secs, 0)
        .map(|utc| {
            utc.with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "--:--".to_owned())
}

/// Rolling chat history shown in the HUD.
#[derive(Resource, Default)]
pub struct ChatHistory {
    pub messages: Vec<ChatEntry>,
    /// Messages that arrived while the Chat window was closed — drives
    /// the toolbar's "Chat (n)" badge (#835), which is the only way an
    /// incoming message is visible at all with the window shut. Cleared
    /// by the toolbar whenever the window is open.
    pub unread: usize,
}

impl ChatHistory {
    /// Append a wall-clock-stamped entry, enforcing the rolling cap
    /// (#846). The cap used to live only on the inbound path — local
    /// sends and the presence/system lines grew the history unboundedly.
    /// EVERY writer routes through here now.
    pub fn push(
        &mut self,
        did: Option<String>,
        author: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.messages.push(ChatEntry {
            did,
            author: author.into(),
            text: text.into(),
            at_epoch_secs: now_epoch_secs(),
        });
        let cap = crate::config::ui::chat::MAX_HISTORY_ENTRIES;
        if self.messages.len() > cap {
            let drop = self.messages.len() - cap;
            self.messages.drain(..drop);
        }
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
    /// Arrival position. `Some` for a classic portal with a baked target;
    /// `None` (#745, gateway travel) defers to the destination record's
    /// `default_landing` — resolved when the fetched record lands, falling
    /// back to the legacy origin scatter when the destination has none.
    pub target_pos: Option<Vec3>,
}

/// Spawn-pose handoff from the login pipeline (fresh login or resume) into
/// `spawn_local_player`. Inserted by the login completion / resume systems
/// when the URL/CLI boot params asked for a non-default pose; consumed and
/// removed by `spawn_local_player` on the first `InGame` frame so a later
/// portal travel or `respawn_if_fallen` can't retroactively reapply it.
#[derive(Resource, Clone, Debug)]
pub struct PendingSpawnPlacement {
    pub pos: Option<crate::boot_params::TargetPos>,
    pub yaw_deg: Option<f32>,
}

/// Outcome of the most recent "Save to PDS" round-trip for one
/// editable record. Carried inside the per-record [`PublishFeedback`]
/// resource and rendered verbatim by the shared
/// [`crate::ui::editable::publish_status_line`], so every editor's
/// status line looks and counts identically (the same `(Ns ago)` timer
/// for both Success *and* Failed).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PublishStatus {
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

/// Per-record publish-status resource. Generic over the record type so
/// the Room, Avatar and Inventory editors each get their **own**
/// instance: publishing one record can no longer overwrite another
/// editor's status line — the bug that came from a single shared
/// `PublishFeedback`. One is registered per record in [`crate::run`]
/// (`PublishFeedback<RoomRecord>`, `<AvatarRecord>`,
/// `<InventoryRecord>`).
pub struct PublishFeedback<R: Send + Sync + 'static> {
    pub status: PublishStatus,
    /// Throttled cache of the live record's serialized size, feeding the
    /// shared row's budget readout (#694). Refreshed by each editor at
    /// [`crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS`] cadence
    /// while its window is open — a full serialize per frame would be
    /// wasted work. `None` until first measured (window never opened).
    pub live_bytes: Option<usize>,
    /// When `live_bytes` was last refreshed (`Time::elapsed_secs_f64`).
    pub live_bytes_at: Option<f64>,
    _record: PhantomData<fn() -> R>,
}

// Hand-written (not derived): `#[derive(Default)]` would wrongly demand
// `R: Default`, but no record type implements `Default` the same way
// (Room/Avatar are DID-seeded). `PhantomData<fn() -> R>` is
// `Send + Sync` for *any* `R`, so the resource bound only needs
// `R: 'static`.
impl<R: Send + Sync + 'static> Default for PublishFeedback<R> {
    fn default() -> Self {
        Self {
            status: PublishStatus::Idle,
            live_bytes: None,
            live_bytes_at: None,
            _record: PhantomData,
        }
    }
}

impl<R: Send + Sync + 'static> Resource for PublishFeedback<R> {}

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

/// Present when the avatar-record fetch fell back to the DID default for
/// an unrecoverable reason — decode failure or an exhausted retry budget
/// (#840). Live and Stored are both the default, so "dirty" reads clean
/// while the real record still sits on the PDS: the Avatar editor shows
/// a banner and the first publish asks for confirmation before it
/// overwrites the stored copy. Cleared by a successful fetch, by that
/// confirmed publish, and on logout.
#[derive(Resource, Debug, Clone)]
pub struct AvatarRecordRecovery {
    /// Human-readable fetch/decode error, shown in the banner.
    pub reason: String,
}

/// Present when the inventory fetch fell back to the empty default after
/// its (short) retry budget (#840) — the session is "degraded": the
/// stash shows empty while items may still exist on the PDS, and an
/// unconfirmed publish would wipe them. Same lifecycle as
/// [`AvatarRecordRecovery`].
#[derive(Resource, Debug, Clone)]
pub struct InventoryRecordRecovery {
    /// Human-readable fetch error, shown in the banner.
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

/// The local **live** room record — what the World Editor's widgets,
/// the 3D gizmo commit and the inventory drag-drop mutate in place, and
/// what the world compiler / terrain / network broadcast read each
/// frame. Diverges from [`StoredRoomRecord`] until the owner Publishes
/// (or reverts via Load / Reset). This is the same Live/Stored split
/// [`LiveAvatarRecord`] and [`LiveInventoryRecord`] use, so all three
/// editors share one mental model and one Save/Load/Reset
/// implementation ([`crate::ui::editable`]).
#[derive(Resource, Clone)]
pub struct LiveRoomRecord(pub RoomRecord);

/// The last known PDS-persisted room record. [`LiveRoomRecord`] is
/// mutated immediately by the world editor; this one stays pinned to
/// the committed state so "Load from PDS" can discard uncommitted edits
/// and the derived dirty indicator (`records_differ(live, stored)`) has
/// a reference point to diff against. Only the publish-poll system
/// repins it (on success), so the dirty check stays meaningful.
#[derive(Resource, Clone)]
pub struct StoredRoomRecord(pub RoomRecord);

/// Local-only UX preferences that are *not* stored on the PDS (they
/// describe how this client renders the world, not the world itself).
/// Persisted machine-locally by [`crate::prefs`] (#820); grow it only
/// with `#[serde(default)]`-compatible fields so old prefs files keep
/// loading.
#[derive(Resource, Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
/// the "Save to PDS" button's dirty indicator.
#[derive(Resource, Clone)]
pub struct LiveInventoryRecord(pub InventoryRecord);

/// Last known PDS-persisted inventory record. Populated by the loading
/// fetch and replaced on a successful publish; nothing else should mutate
/// it so the dirty check against `LiveInventoryRecord` stays meaningful.
#[derive(Resource, Clone)]
pub struct StoredInventoryRecord(pub InventoryRecord);

/// One editable, PDS-backed record. Lets the Room / Avatar / Inventory
/// editors share a single Save / Load / Reset implementation
/// ([`crate::ui::editable`]) instead of three subtly-divergent
/// hand-rolled variants.
pub trait EditableRecord: Clone + serde::Serialize + Send + Sync + 'static {
    /// The canonical default record for a DID. Room and Avatar seed
    /// deterministic content from the DID; Inventory ignores it (its
    /// default is an empty stash).
    fn default_for_did(did: &str) -> Self;
    /// Lower-case noun used by the shared status line ("room" → "✓
    /// Saved …"). The editor window title carries the fuller context.
    const NOUN: &'static str;
}

impl EditableRecord for RoomRecord {
    fn default_for_did(did: &str) -> Self {
        RoomRecord::default_for_did(did)
    }
    const NOUN: &'static str = "room";
}

impl EditableRecord for AvatarRecord {
    fn default_for_did(did: &str) -> Self {
        AvatarRecord::default_for_did(did)
    }
    const NOUN: &'static str = "avatar";
}

impl EditableRecord for InventoryRecord {
    fn default_for_did(_did: &str) -> Self {
        InventoryRecord::default()
    }
    const NOUN: &'static str = "inventory";
}

/// Canonical equality for the dirty check. `RoomRecord` and
/// `InventoryRecord` deliberately don't derive `PartialEq` (the
/// `Generator` tree carries data the editor never compares
/// structurally), so all three editors diff through the same serde
/// model that is already DAG-CBOR / round-trip tested: "dirty" means
/// "would serialise differently to the stored record" — exactly what a
/// Publish would change. Used uniformly so Avatar no longer behaves
/// differently from Room/Inventory just because it happens to derive
/// `PartialEq`.
pub fn records_differ<R: serde::Serialize>(a: &R, b: &R) -> bool {
    serde_json::to_value(a).ok() != serde_json::to_value(b).ok()
}

/// A currently-displayed incoming item-offer modal. Exactly one can be
/// active at a time — this is an explicit anti-spam measure: concurrent
/// offers from other peers are auto-declined with a "busy" reply so a
/// malicious client cannot flood a victim with request dialogs or tie
/// their client up answering queued prompts.
///
/// Muted senders never reach this resource — see
/// [`crate::network`]'s `inbound::handle_incoming_messages`, which
/// short-circuits muted-peer offers into a silent auto-decline before the
/// dialog is constructed.
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

/// DIDs the local user has muted, persisted across sessions via the
/// prefs layer (#820/#844). The live cache stays `RemotePeer::muted` —
/// this set is the durable source: applied when a peer's DID resolves,
/// updated by every mute toggle. Without it a mute lived on the
/// session-scoped peer entity, so a harasser reset the block by simply
/// reconnecting. Machine-local like the rest of the prefs.
#[derive(Resource, Default, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct MutedDids(pub std::collections::HashSet<String>);

impl MutedDids {
    /// Record a mute-flag change for `did`. Returns true when the set
    /// actually changed (drives prefs change detection honestly).
    pub fn set(&mut self, did: &str, muted: bool) -> bool {
        if muted {
            self.0.insert(did.to_owned())
        } else {
            self.0.remove(did)
        }
    }
}

/// Offers auto-declined by the busy-gate while the current
/// [`IncomingOfferDialog`] sat on screen (#843). The network layer's
/// single-dialog anti-spam invariant silently declines them; this counter
/// lets the UI say "N more offers arrived while you decided" when the
/// dialog closes, instead of peers' gifts vanishing without a trace.
/// Reset whenever a dialog is answered or evicted.
#[derive(Resource, Default, Debug)]
pub struct BusyAutoDeclines(pub u32);

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
    /// the allocated id so the caller can ship it in the
    /// [`crate::protocol::OverlandsMessage::ItemOffer`].
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
