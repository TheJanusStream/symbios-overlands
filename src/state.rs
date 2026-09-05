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

use crate::config;
use crate::network::ChatDelivery;
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
    /// What wire protocol this peer announced (#1121), from its
    /// [`crate::protocol::OverlandsMessage::Hello`]. `None` means no `Hello`
    /// has arrived — which after
    /// [`crate::config::network::PROTOCOL_ANNOUNCE_GRACE_SECS`] is itself the
    /// answer: the peer is running a build from before the handshake existed,
    /// and its wire layout is unknowable.
    pub build: Option<PeerBuild>,
    /// `Time::elapsed_secs_f64` at the moment this peer connected — the clock
    /// the `Hello` grace period runs against.
    pub connected_at: f64,
}

/// A peer's self-declared wire protocol and build string (#1121).
///
/// Both fields are peer-supplied over an unauthenticated data channel and
/// nothing is gated on them: the protocol number only drives a chip on the
/// peer's own row and a session-log line, and the build string is only ever
/// displayed. A peer that lies mislabels itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerBuild {
    /// The peer's [`crate::protocol::PROTOCOL_VERSION`].
    pub protocol: u16,
    /// Human-readable version+sha, for naming the two builds in a bug report.
    pub build: String,
}

/// How this peer's wire compatibility reads right now (#1121). Derived, not
/// stored: [`RemotePeer::compatibility`] computes it from the announcement and
/// the elapsed grace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerCompatibility {
    /// A `Hello` arrived and its protocol equals ours.
    Compatible,
    /// A `Hello` arrived announcing a different protocol.
    Mismatched(u16),
    /// No `Hello` yet, and the grace period has not elapsed.
    Pending,
    /// No `Hello`, and the grace period has elapsed — a pre-handshake build.
    Unannounced,
}

impl RemotePeer {
    /// Read this peer's wire compatibility as of `now`.
    ///
    /// The `Unannounced` arm is the case that matters in practice and the one
    /// a version field alone would miss: every build that shipped before
    /// #1121 announces nothing, so silence — not a number — is how the live
    /// incompatibility presents itself.
    pub fn compatibility(&self, now: f64) -> PeerCompatibility {
        match &self.build {
            Some(b) if b.protocol == crate::protocol::PROTOCOL_VERSION => {
                PeerCompatibility::Compatible
            }
            Some(b) => PeerCompatibility::Mismatched(b.protocol),
            None if now - self.connected_at >= config::network::PROTOCOL_ANNOUNCE_GRACE_SECS => {
                PeerCompatibility::Unannounced
            }
            None => PeerCompatibility::Pending,
        }
    }
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
    /// The query could not be answered — a non-2xx response, a transport
    /// error, a JSON decode failure or the wrapping timeout (#1218 f297).
    ///
    /// This arm exists because the three failure paths used to return
    /// [`None`](Self::None), whose doc comment asserts a fact about the
    /// social graph that a failed lookup cannot support. The ★ is the only
    /// trust signal the social UI carries — it is what a user leans on
    /// deciding whether to accept a gift or follow a stranger through a
    /// portal — and a signal that fails closed with no distinction between
    /// "no" and "couldn't ask" quietly under-reports friends.
    ///
    /// Retried on the shared doubling backoff, so a transient AppView hiccup
    /// does not settle a relationship for the session.
    Failed,
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
    /// What became of this line if WE sent it (#1213). Inbound chat,
    /// presence lines and system notices carry
    /// [`ChatDelivery::NotApplicable`] and render no annotation; a local
    /// send that reached nobody renders a weak suffix, because pushing it
    /// into the HUD byte-identically to a delivered one made the sender's
    /// own window the lie.
    pub delivery: ChatDelivery,
}

/// Current wall-clock time as Unix seconds. `chrono`'s clock is backed
/// by JS `Date` on wasm (`wasmbind`), so this is safe on both targets —
/// `std::time::SystemTime` panics on wasm32 (known gotcha).
pub fn now_epoch_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Real seconds elapsed since a wall-clock stamp taken by
/// [`now_epoch_secs`] (#1216).
///
/// The clock every deadline the OTHER end is also counting has to run on.
/// Bevy's `Res<Time>` is the virtual clock, which clamps each frame's delta
/// to `DEFAULT_MAX_DELTA` (250 ms): a suspended laptop or a backgrounded tab
/// advances `elapsed` by a quarter second, not by the wall-clock gap. Two
/// machines that were asleep for different lengths therefore accumulate
/// different amounts of "time", and a gift offer can expire on the sender
/// while the recipient still has a live dialog — the sender toasted that
/// nobody answered while the recipient was accepting.
///
/// Clamped at zero: a wall clock can step backwards (NTP, a timezone-naive
/// user setting the date), and a negative age would read as a deadline that
/// has receded. Erring toward "just arrived" delays an expiry rather than
/// firing one early, which is the safe direction for a countdown a person
/// is watching.
///
/// Keep `Res<Time>` for anything that should pause with the game; a
/// deadline shared with a peer is not one of those.
pub fn real_secs_since(epoch_secs: i64) -> f64 {
    (now_epoch_secs() - epoch_secs).max(0) as f64
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
    /// The half-typed line sitting in the chat input (#1140). It lived in
    /// a `Local<String>` on `chat_ui`, which no teardown can reach — so a
    /// sentence typed before logging out was still in the box when the
    /// NEXT user logged in on the same machine. Anything session-scoped
    /// has to live somewhere logout can scrub, and the history it belongs
    /// to is already that place.
    pub draft: String,
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
        self.push_with(did, author, text, ChatDelivery::NotApplicable);
    }

    /// Append a line the LOCAL user just sent, stamped with what actually
    /// became of it (#1213). Only `chat_ui` calls this — every other writer
    /// is reporting something that already arrived, and uses [`push`].
    ///
    /// [`push`]: ChatHistory::push
    pub fn push_sent(
        &mut self,
        did: Option<String>,
        author: impl Into<String>,
        text: impl Into<String>,
        delivery: ChatDelivery,
    ) {
        self.push_with(did, author, text, delivery);
    }

    /// The single funnel both public writers route through, so the rolling
    /// cap and the wall-clock stamp cannot be bypassed by either.
    fn push_with(
        &mut self,
        did: Option<String>,
        author: impl Into<String>,
        text: impl Into<String>,
        delivery: ChatDelivery,
    ) {
        self.messages.push(ChatEntry {
            did,
            author: author.into(),
            text: text.into(),
            at_epoch_secs: now_epoch_secs(),
            delivery,
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

/// Which half of a journey is running (#1231 f20).
///
/// The two halves look nothing alike from the player's seat and had to be
/// told apart: the first is a network wait that can be given up on, the
/// second is a local build that cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelPhase {
    /// Fetching the destination's room record. Cancellable — nothing has
    /// been swapped yet, and the fetch can outlast a minute on a bad
    /// network (#1231 f25).
    Fetching,
    /// The record has landed and been installed; the terrain is
    /// regenerating and the world is compiling a time-slice at a time.
    /// Nothing to cancel — the world being left no longer exists.
    Building,
}

/// Inserted when the player touches an inter-room portal. Freezes local
/// movement and triggers an async fetch of the target room record while
/// keeping the player in AppState::InGame.
///
/// It stays through the [`Building`](TravelPhase::Building) half too
/// (#1231 f20). It used to be removed the frame the *record* landed, which
/// is several seconds before the destination exists: the freeze released
/// and the card vanished while terrain regen had not started and the
/// time-sliced compile had not run, so the player was dropped at the
/// landing pose — `y = 0` for a gateway hop, frequently below the ground
/// still under their feet — to watch the world assemble around them with
/// no overlay, spinner or line. Every suppressor in the app already keys
/// off `TravelingTo.is_some()`, so carrying a phase on it rather than
/// adding a second marker is what makes it impossible to miss one.
#[derive(Resource, Clone)]
pub struct TravelingTo {
    pub target_did: String,
    /// Arrival position. `Some` for a classic portal with a baked target;
    /// `None` (#745, gateway travel) defers to the destination record's
    /// `default_landing` — resolved when the fetched record lands, falling
    /// back to the legacy origin scatter when the destination has none.
    pub target_pos: Option<Vec3>,
    /// The name the surface that started this travel already had for the
    /// destination (#1231 f27), or `None` when it had none.
    ///
    /// `BskyProfileCache` is filled by peer-driven fetches only, so a
    /// gateway row that just rendered "@alice" does NOT put her in it —
    /// and the overlay one click later fell back to `did:plc:abcdefgh…`
    /// for the same person. The row had the handle; it just threw it away.
    pub target_label: Option<String>,
    pub phase: TravelPhase,
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
    /// A save is in flight; `since_secs` is when it was dispatched, so the
    /// status line can count up against the task deadline (#1206).
    Publishing {
        since_secs: f64,
    },
    Success {
        at_secs: f64,
    },
    Failed {
        at_secs: f64,
        message: String,
        /// Retrying this save cannot work (#1214): the OAuth refresh token
        /// is expired or revoked, so the identical failure follows every
        /// click. The Save row reads it as a refusal, and
        /// `report_publish_failure` skips the panel force-open that would
        /// otherwise point the owner back at the button that cannot succeed.
        terminal: bool,
    },
}

/// Per-record publish-status resource. Generic over the record type so
/// the Room, Avatar and Inventory editors each get their **own**
/// instance: publishing one record can no longer overwrite another
/// editor's status line — the bug that came from a single shared
/// `PublishFeedback`. One is registered per record in [`crate::run`]
/// (`PublishFeedback<RoomRecord>`, `<AvatarRecord>`,
/// `<InventoryRecord>`).
#[derive(Resource)]
pub struct PublishFeedback<R: Send + Sync + 'static> {
    pub status: PublishStatus,
    /// Throttled cache of what the live record's next save would write,
    /// feeding the shared row's budget readout (#694, #1207). Refreshed by
    /// each editor at
    /// [`crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS`] cadence
    /// while its window is open — a full serialize per frame would be
    /// wasted work. Default (unmeasured) until the window first opens.
    pub live_size: crate::pds::record_size::SizeReadout,
    /// When `live_size` was last refreshed (`Time::elapsed_secs_f64`).
    pub live_bytes_at: Option<f64>,
    _record: PhantomData<fn() -> R>,
}

impl PublishStatus {
    /// Whether a save is in flight — the one state the Save row disables
    /// every button in.
    pub fn is_publishing(&self) -> bool {
        matches!(self, Self::Publishing { .. })
    }
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
            live_size: crate::pds::record_size::SizeReadout::default(),
            live_bytes_at: None,
            _record: PhantomData,
        }
    }
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
#[derive(Resource, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LocalSettings {
    /// When true, remote peer transforms are smoothed with a Hermite spline
    /// applied to a delayed jitter buffer.  When false, peers snap to the
    /// latest received packet (useful for debugging raw network latency).
    pub smooth_kinematics: bool,
    /// Which shipped UI palette this machine uses (#857). Applied by
    /// `ui::theme::sync_theme_from_settings`; old prefs files without
    /// the field default to Dark via the struct-level `serde(default)`.
    pub theme: crate::ui::theme::UserTheme,
    /// Camera ground-avoidance mode (#872): whether (and how) the orbit
    /// camera is pulled in to keep terrain out of the shot. Old prefs
    /// files default to the camera-position-only check.
    pub camera_ground_avoidance: crate::camera::CameraGroundAvoidance,
    /// Headroom the ground avoidance keeps between the camera and the
    /// terrain surface, in metres (#872).
    pub camera_ground_clearance_m: f32,
    /// Build and slowly orbit a live seeded demo world behind the login
    /// screen (#897). Off ⇒ the login screen keeps its sky-gradient
    /// backdrop and skips the pre-login world build entirely.
    pub login_world_backdrop: bool,
    /// How loudly the room's contact effects are allowed to play (#1221
    /// f308). The visual counterpart to the app-wide audio mute, and the
    /// accessibility control the app lacked for flashing and motion.
    pub effects_intensity: EffectsIntensity,
    /// Hang each remote peer's name over their body (#1226 f325). On by
    /// default: without it there is no in-world identity at all, and every
    /// social action the product ships — Mute, Visit, drag-to-gift — is
    /// addressed to a roster row that nothing connects to a body. Off is
    /// for the user who would rather have an uncluttered view of a busy
    /// room than a name over everybody in it.
    pub show_peer_nametags: bool,
    /// Interface scale, applied as egui's `zoom_factor` (#1259 f239).
    ///
    /// The app shipped with no text-size control of any kind, so the
    /// three-palette picker was the whole of its accessibility surface —
    /// and egui's built-in Ctrl+plus / Ctrl+minus, which has always
    /// worked, was documented nowhere and forgotten at every launch
    /// because nothing serialises egui's `Options`. This field is the
    /// durable home for it: [`crate::ui::theme::sync_ui_scale`] pushes it
    /// into the context AND reads the keyboard zoom back out, so the two
    /// controls are one setting.
    ///
    /// Clamped to
    /// [`crate::config::ui::UI_SCALE_MIN`]..=[`crate::config::ui::UI_SCALE_MAX`]
    /// on the way in: a prefs file naming 0.05 would leave the UI
    /// unreadable with no way to reach the slider that fixes it.
    pub ui_scale: f32,
    /// Follow `Url` asset references — images and sounds a record names by
    /// web address — as opposed to the ATProto ones (`AtprotoBlob`,
    /// `DidPfp`), which stay inside Bluesky infrastructure (#1248 f298).
    ///
    /// **What this is really about.** A room reached through a portal or a
    /// gateway is a stranger's, and its record can name any host it likes.
    /// Standing in it makes your client fetch from that host, which discloses
    /// your IP address, roughly where you are, and — for a contact cue, which
    /// fires on touch — when you arrived and when you left. #1127 removed the
    /// crude half of that (http, loopback, private ranges); what remains is
    /// inherent to following an address somebody else chose, and until now it
    /// was neither disclosed nor refusable.
    ///
    /// On by default, because most authored imagery in the product is a URL
    /// and defaulting off would make every visited world worse without the
    /// visitor understanding why. The point is that the choice exists and is
    /// stated.
    pub load_external_assets: bool,
}

/// How much of a room's authored contact effects a visitor accepts
/// (#1221 f308).
///
/// Portals and gateways invite visitors into rooms authored by strangers,
/// and the recipe driving those effects arrives over the live-preview
/// broadcast. The engine bounded resource use carefully and never bounded
/// GRIEFING: it caps the decal pile at 64 quads while permitting each to be
/// 64 m across at full opacity with a zero cooldown, so a visitor's screen
/// filled with solid colour the moment their feet touched the ground — and
/// the only escape was to leave the room, which is exactly what a griefer
/// wants. There was an app-wide audio mute and no visual equivalent at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EffectsIntensity {
    /// Play what the room authored.
    #[default]
    Full,
    /// Bound the worst of it: decals shrink and fade, and a zero cooldown
    /// gets a floor so a Dwell recipe cannot stamp once per frame.
    Reduced,
    /// None at all — the same early-return the empty-registry path already
    /// takes.
    Off,
}

impl EffectsIntensity {
    /// Whether contact effects run at all.
    pub fn plays(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Multiplier on a decal's authored size and opacity.
    pub fn decal_scale(self) -> f32 {
        match self {
            Self::Full => 1.0,
            // A quarter of 64 m is 16 m: present, and no longer the whole
            // view. Applied to alpha as well, so a "Reduced" screen can
            // always be seen through.
            Self::Reduced => 0.25,
            Self::Off => 0.0,
        }
    }

    /// Least seconds between two stamps of the same recipe on the same
    /// avatar, whatever the room authored.
    pub fn cooldown_floor(self) -> f32 {
        match self {
            Self::Full => 0.0,
            Self::Reduced => crate::config::interaction::REDUCED_EFFECT_COOLDOWN_SECS,
            Self::Off => f32::INFINITY,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Reduced => "Reduced",
            Self::Off => "Off",
        }
    }
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            smooth_kinematics: true,
            theme: crate::ui::theme::UserTheme::Dark,
            camera_ground_avoidance: crate::camera::CameraGroundAvoidance::default(),
            camera_ground_clearance_m: crate::config::camera::TERRAIN_CLEARANCE,
            login_world_backdrop: true,
            effects_intensity: EffectsIntensity::default(),
            show_peer_nametags: true,
            ui_scale: 1.0,
            load_external_assets: true,
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
    /// The best name we have for the sender, off the ONE ladder
    /// ([`crate::network::PeerLabel`], #1218 f299).
    ///
    /// A `String` here used to fall back to the raw DID, which the modal
    /// then rendered inside an `@`-prefixed sentence — presenting
    /// `@did:plc:z72i7hdynmk6r22z27h6tvur` as the identity the user must
    /// judge, twice, in the app's one blocking dialog, on a countdown. The
    /// typed label is what lets the modal say "we don't know who this is"
    /// instead of guessing.
    pub sender_label: crate::network::PeerLabel,
    pub item_name: String,
    pub generator: Generator,
    /// The item's wear metadata (#1108) when the sender gifted a wearable,
    /// already sanitised at the wire seam; `None` lands the gift as decor.
    pub wear: Option<crate::pds::inventory::WearMeta>,
    /// Session-relative seconds the offer arrived. Kept for the session
    /// log, whose other stamps are all on this clock.
    pub arrived_at_secs: f64,
    /// Wall-clock seconds the offer arrived (#1216). The TTL and the
    /// on-screen countdown both run on THIS one — see [`real_secs_since`]
    /// for why a deadline the sender is also counting cannot use the
    /// suspend-clamped virtual clock.
    pub arrived_at_epoch: i64,
}

/// An incoming offer the recipient set aside to make room for (#1220 f288).
///
/// The dialog is a true `egui::Modal`: it paints topmost and blocks
/// background pointer input. So its own "Open Inventory" button — offered on
/// the one failure path it exists for, a full stash — raised the Inventory
/// window UNDER a modal that swallowed every click on it, while the
/// countdown declined the gift out from under the user. The only way to
/// reach the Inventory was to Decline first, which is the outcome the button
/// exists to avoid.
///
/// Holding the offer instead lets the dialog close, the Inventory work, and
/// the offer come back the moment a slot frees. The clock keeps running on
/// [`IncomingOfferDialog::arrived_at_epoch`], so a hold cannot be used to
/// extend an offer past the TTL the sender is also counting.
#[derive(Resource, Clone, Debug)]
pub struct HeldOffer(pub IncomingOfferDialog);

/// DIDs the SIGNED-IN user has muted, persisted across sessions via the
/// prefs layer (#820/#844). The live cache stays `RemotePeer::muted` —
/// this set is the durable source: applied when a peer's DID resolves,
/// updated by every mute toggle. Without it a mute lived on the
/// session-scoped peer entity, so a harasser reset the block by simply
/// reconnecting.
///
/// Scoped to the account, not the machine (#1223 f292). It is stored in
/// machine-local prefs like everything else, but under the owner's DID in
/// [`MutedByOwner`] — a block list is a statement about who *you* will not
/// hear, and a shared computer used to hand one user's list to the next,
/// invisibly, since a muted peer renders as a hidden body and a faint dot.
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

    /// The list in a stable order for display (#1223 f292). A `HashSet`
    /// iterates arbitrarily, and a settings list whose rows reshuffle under
    /// the cursor is one you cannot click an Unmute button in.
    pub fn sorted(&self) -> Vec<&str> {
        let mut dids: Vec<&str> = self.0.iter().map(String::as_str).collect();
        dids.sort_unstable();
        dids
    }
}

/// Every account's mute list on this machine, keyed by the owner's DID
/// (#1223 f292).
///
/// The persisted shape behind [`MutedDids`], which holds only the signed-in
/// owner's entry. Kept as a separate resource rather than folded into
/// `MutedDids` because every reader in the crate wants "the list that
/// applies to me right now", and exactly two places — login and the prefs
/// save — care whose it is.
#[derive(Resource, Default, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct MutedByOwner(pub std::collections::HashMap<String, std::collections::HashSet<String>>);

impl MutedByOwner {
    /// The list belonging to `owner`, empty when they have muted nobody.
    pub fn for_owner(&self, owner: &str) -> MutedDids {
        MutedDids(self.0.get(owner).cloned().unwrap_or_default())
    }

    /// Store `list` as `owner`'s. An empty list is REMOVED rather than
    /// stored empty, so unmuting everyone leaves no trace of who was
    /// signed in on this machine.
    pub fn set_owner(&mut self, owner: &str, list: &MutedDids) {
        if list.0.is_empty() {
            self.0.remove(owner);
        } else {
            self.0.insert(owner.to_owned(), list.0.clone());
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
    /// How to name the recipient in the sender's own toasts, off the ONE
    /// ladder ([`crate::network::PeerLabel`], #1218 f299) and already
    /// carrying its `@` when — and only when — it is a real handle.
    ///
    /// This used to be a bare handle string with `@` glued on at four call
    /// sites, so a recipient whose profile had not resolved was toasted at
    /// as `@identifying…`, and the DID-head fallback would have read
    /// `@did:plc:z72i7hdy…`. The sigil belongs to the name tier alone.
    pub target_label: String,
    pub item_name: String,
    pub sent_at_secs: f64,
    /// Wall-clock seconds the offer was sent (#1216). The 180 s sweep runs
    /// on this, so a sender who slept and a recipient who did not cannot
    /// settle on opposite outcomes for one `offer_id`.
    pub sent_at_epoch: i64,
}

impl PendingOutgoingOffers {
    /// The id the next [`Self::register`] will use, without consuming it.
    ///
    /// The gift path needs an `offer_id` to build the
    /// [`crate::protocol::OverlandsMessage::ItemOffer`] with, but must not
    /// arm the offer's expiry timer until that message has actually been
    /// handed to the transport (#1123). Registering first meant a send the
    /// chunker refused still logged `ItemOfferSent` and still expired
    /// minutes later as "no answer" — blaming a recipient who was never
    /// asked. Peeking costs nothing if the send is refused: the id is not
    /// consumed and the next gift reuses it.
    pub fn peek_next_id(&self) -> u64 {
        self.next_id
    }

    /// Insert the pending record under an id from [`Self::peek_next_id`],
    /// consuming it.
    pub fn register(
        &mut self,
        id: u64,
        target_did: String,
        target_label: String,
        item_name: String,
        sent_at_secs: f64,
    ) {
        self.next_id = id.wrapping_add(1);
        self.by_id.insert(
            id,
            PendingOutgoingOffer {
                target_did,
                target_label,
                item_name,
                sent_at_secs,
                // Stamped here rather than passed in: every caller would
                // otherwise have to remember to read the same clock, and
                // the deadline is this type's own business.
                sent_at_epoch: now_epoch_secs(),
            },
        );
    }
}

#[cfg(test)]
mod effects_intensity_tests {
    use super::*;

    /// #1221 f308. The sequence: you portal into a stranger's room and your
    /// screen fills with solid colour the moment your feet touch the ground
    /// — a Dwell decal recipe at 64 m, alpha 1.0, cooldown 0, stamping once
    /// per frame per avatar. The engine bounded resource use (64 live
    /// quads) and never bounded griefing, and the only exit was to leave.
    #[test]
    fn each_level_bounds_a_different_half_of_the_attack() {
        // Full is what the room authored — the setting must not change the
        // product for people who never touch it.
        assert!(EffectsIntensity::Full.plays());
        assert_eq!(EffectsIntensity::Full.decal_scale(), 1.0);
        assert_eq!(
            EffectsIntensity::Full.cooldown_floor(),
            0.0,
            "an authored cooldown of zero is legitimate at Full"
        );

        // Reduced bounds BOTH factors: a 64 m quad becomes 16 m, its alpha
        // drops with it, and the per-frame case is gone.
        assert!(EffectsIntensity::Reduced.plays());
        let scaled = 64.0 * EffectsIntensity::Reduced.decal_scale();
        assert!(scaled <= 16.0, "a reduced decal is still {scaled} m across");
        assert!(
            EffectsIntensity::Reduced.decal_scale() < 1.0,
            "opacity scales with size so a Reduced screen can be seen through"
        );
        assert!(EffectsIntensity::Reduced.cooldown_floor() > 0.0);

        // Off is the early return.
        assert!(!EffectsIntensity::Off.plays());
    }

    /// The default is Full: a setting that quietly degraded everybody's
    /// rooms to fix a griefing case would be the wrong trade.
    #[test]
    fn the_default_changes_nothing_for_anyone() {
        assert_eq!(EffectsIntensity::default(), EffectsIntensity::Full);
        assert_eq!(
            LocalSettings::default().effects_intensity,
            EffectsIntensity::Full
        );
    }

    /// Every level has a label, because the control is three buttons and an
    /// unlabelled one is unusable.
    #[test]
    fn every_level_names_itself() {
        for level in [
            EffectsIntensity::Full,
            EffectsIntensity::Reduced,
            EffectsIntensity::Off,
        ] {
            assert!(!level.label().is_empty());
        }
    }
}

#[cfg(test)]
mod mute_list_tests {
    use super::*;

    /// #1223 f292. A `HashSet` iterates arbitrarily, and a settings list
    /// whose rows reshuffle between frames is one you cannot reliably click
    /// an Unmute button in — the click lands on whoever moved into that row.
    #[test]
    fn the_mute_list_renders_in_a_stable_order() {
        let mut muted = MutedDids::default();
        for did in ["did:plc:charlie", "did:plc:alice", "did:plc:bob"] {
            muted.set(did, true);
        }
        assert_eq!(
            muted.sorted(),
            vec!["did:plc:alice", "did:plc:bob", "did:plc:charlie"],
        );
    }
}

#[cfg(test)]
mod peer_compatibility_tests {
    use super::*;
    use crate::config::network::PROTOCOL_ANNOUNCE_GRACE_SECS;

    /// Mint a `PeerId` without naming the `uuid` crate — it wraps a `Uuid`,
    /// which deserializes from its hyphenated string form.
    fn any_peer_id() -> bevy_symbios_multiuser::prelude::PeerId {
        serde_json::from_value(serde_json::Value::String(String::from(
            "00000000-0000-0000-0000-000000000001",
        )))
        .expect("valid uuid")
    }

    fn peer(build: Option<PeerBuild>) -> RemotePeer {
        RemotePeer {
            peer_id: any_peer_id(),
            did: None,
            handle: None,
            muted: false,
            avatar: None,
            build,
            connected_at: 100.0,
        }
    }

    /// #1121. The sequence this reproduces is the one that shipped: a peer
    /// running a build from before `ItemOffer` gained `wear_json` (59ff989)
    /// connects, and every message it exchanges with us that touches a moved
    /// variant fails to decode inside the transport and is dropped. Before
    /// this reading existed there was nothing anywhere — not on the wire, not
    /// on the peer, not in the log — that distinguished that peer from a
    /// healthy one, so a failed gift was indistinguishable from a slow one.
    ///
    /// The grace period is the whole subtlety: such a peer sends no `Hello`,
    /// and neither does a compatible peer during its first frames. Only the
    /// elapsed time separates them, which is why the reading takes `now`.
    #[test]
    fn a_peer_that_never_announces_reads_as_incompatible_only_after_the_grace() {
        let old_build = peer(None);
        assert_eq!(
            old_build.compatibility(100.0),
            PeerCompatibility::Pending,
            "a peer that just connected has not had a chance to announce"
        );
        assert_eq!(
            old_build.compatibility(100.0 + PROTOCOL_ANNOUNCE_GRACE_SECS - 0.5),
            PeerCompatibility::Pending
        );
        assert_eq!(
            old_build.compatibility(100.0 + PROTOCOL_ANNOUNCE_GRACE_SECS),
            PeerCompatibility::Unannounced,
            "silence past the grace is the pre-handshake build reporting itself"
        );
    }

    /// A peer that announces is read on its number alone, with no grace: the
    /// answer is already known, so waiting would only delay the chip.
    #[test]
    fn an_announced_protocol_is_read_immediately_and_either_way() {
        let ours = peer(Some(PeerBuild {
            protocol: crate::protocol::PROTOCOL_VERSION,
            build: String::from("0.7.0+abc"),
        }));
        assert_eq!(ours.compatibility(100.0), PeerCompatibility::Compatible);
        assert_eq!(
            ours.compatibility(100.0 + PROTOCOL_ANNOUNCE_GRACE_SECS * 10.0),
            PeerCompatibility::Compatible,
            "a compatible peer never ages into a warning"
        );

        let theirs = peer(Some(PeerBuild {
            protocol: crate::protocol::PROTOCOL_VERSION + 7,
            build: String::from("9.9.9+def"),
        }));
        assert_eq!(
            theirs.compatibility(100.0),
            PeerCompatibility::Mismatched(crate::protocol::PROTOCOL_VERSION + 7),
            "a declared disagreement needs no grace — it is already the answer"
        );
    }
}

#[cfg(test)]
mod wall_clock_tests {
    use super::*;

    /// THE SEQUENCE (#1216): a peer offers a gift and shuts the laptop lid.
    /// Both ends are counting the same TTL, and the virtual clock stops on
    /// whichever machine slept — so the two disagreed about whether the
    /// offer was still alive, and the sender was told the recipient never
    /// answered while the recipient was accepting.
    #[test]
    fn a_stamp_ages_in_real_seconds() {
        let now = now_epoch_secs();
        assert!(real_secs_since(now) < 2.0, "a fresh stamp is ~0s old");
        assert!(
            (real_secs_since(now - 200) - 200.0).abs() < 2.0,
            "a stamp from 200 real seconds ago is 200s old, however many \
             frames were rendered in between"
        );
        // Past both offer TTLs, which is the decision the sweep makes.
        assert!(real_secs_since(now - 200) > crate::config::network::PENDING_OFFER_TIMEOUT_SECS);
        assert!(real_secs_since(now - 100) > crate::config::network::OFFER_DIALOG_TIMEOUT_SECS);
        assert!(real_secs_since(now - 60) < crate::config::network::OFFER_DIALOG_TIMEOUT_SECS);
    }

    /// A wall clock can step backwards (NTP, a user setting the date). A
    /// negative age would read as a deadline that has receded; clamping to
    /// zero delays an expiry rather than firing one early, which is the
    /// safe direction for a countdown a person is watching and a decision
    /// a peer is mirroring.
    #[test]
    fn a_clock_that_steps_backwards_never_ages_a_stamp_negatively() {
        let future = now_epoch_secs() + 10_000;
        assert_eq!(real_secs_since(future), 0.0);
    }

    /// The offer's wall-clock stamp is taken by `register` itself, not
    /// passed in — every caller reading its own clock is how the two
    /// stamps would drift apart.
    #[test]
    fn registering_an_offer_stamps_the_wall_clock_itself() {
        let mut pending = PendingOutgoingOffers::default();
        let id = pending.peek_next_id();
        pending.register(
            id,
            String::from("did:plc:bob"),
            String::from("bob"),
            String::from("Lamp"),
            12.0,
        );
        let entry = &pending.by_id[&id];
        assert_eq!(entry.sent_at_secs, 12.0, "the virtual stamp is untouched");
        assert!(
            real_secs_since(entry.sent_at_epoch) < 2.0,
            "and the deadline's own clock was read at registration"
        );
    }
}
