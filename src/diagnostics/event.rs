//! Session-event data model — the taxonomy every subsystem records into the
//! single append-only diagnostic stream (Pillar A of the diagnostic suite).
//!
//! One [`SessionEvent`] is emitted per notable thing that happens between app
//! launch and exit. The stream has three consumers, all reading the *same*
//! records: the in-game Diagnostics event log (a bounded tail view), the
//! native NDJSON file a coding agent reads for a post-mortem, and the offline
//! `--analyze-session` analyzer. One model means the GUI and the file can
//! never disagree.
//!
//! This module is deliberately free of gameplay types — peer ids, DIDs and
//! positions are stored as plain strings / arrays so it depends only on
//! `serde` and round-trips losslessly through JSON on both native and wasm.
//! Call sites format their domain values into these fields when they record.

use serde::{Deserialize, Serialize};

/// Which subsystem an event originated in — one of the three filter axes
/// (`subsystem` × [`Category`] × [`Severity`]) the analyzer slices on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Subsystem {
    /// App-state machine + the loading gate (Login → Loading → InGame).
    Loading,
    /// Peer-to-peer networking and multiuser presence.
    Network,
    /// Async work offloaded to task pools / web workers.
    Offload,
    /// Frame time, assets, physics, memory — live-session health.
    Runtime,
    /// Session-level bookkeeping (snapshots, segment resets, exit, anomalies).
    Session,
}

/// Severity of an event — drives log level, GUI badge colour, and the
/// analyzer's verdict tally. `Ord` so the GUI can pick the worst active.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Fine-grained / high-frequency; usually rate-limited before recording.
    Trace,
    /// Normal lifecycle progress.
    Info,
    /// Something recoverable but worth noticing.
    Warn,
    /// A failure that degraded behaviour.
    Error,
    /// A failure that blocks or breaks the session.
    Critical,
}

/// A coarse topical grouping, the middle filter axis between [`Subsystem`] and
/// the fine-grained payload `kind`. Derived from the payload via
/// [`EventPayload::category`] so callers never have to pass it by hand.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Lifecycle,
    Fetch,
    Generation,
    Audio,
    Peer,
    Transport,
    Offer,
    Chat,
    Social,
    Job,
    Physics,
    Asset,
    Perf,
    Portal,
    Anomaly,
    Snapshot,
}

/// Which PDS-backed record a fetch event refers to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordKind {
    Room,
    Avatar,
    Inventory,
}

/// Terminal disposition of a record fetch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FetchStatus {
    /// Record decoded and installed.
    Ok,
    /// PDS returned 404 → fell back to the DID-seeded default.
    NotFound,
    /// Response body failed to decode against the current lexicon.
    DecodeError,
    /// A transient error (DNS / timeout / 5xx) that will be retried.
    TransientError,
    /// The retry budget was exhausted and the default was installed.
    Exhausted,
    /// Best-effort fetch (inventory) fell back without retrying.
    BestEffortFallback,
}

/// Which phase a [`StartupInfo`] snapshot was taken in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapshotPhase {
    /// Emitted at app build, before login — the DID is not yet known.
    Boot,
    /// Emitted on Login → Loading, with the authenticated DID/relay filled in.
    Session,
}

/// The first record of every session: enough build/environment context to key
/// a log to a DID and correlate it across runs. Built by
/// `crate::diagnostics::snapshot` (Pillar A-4); the type lives here because it
/// is part of the event taxonomy. Boxed inside [`EventPayload`] so the enum
/// stays small.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StartupInfo {
    pub phase: SnapshotPhase,
    /// `CARGO_PKG_VERSION` of the overlands crate.
    pub version: String,
    /// Short git sha (or `"unknown"` when built outside a git checkout).
    pub git_sha: String,
    /// `target_arch` the binary was compiled for.
    pub target_arch: String,
    /// `"debug"` or `"release"`.
    pub profile: String,
    /// True on the wasm32 web build.
    pub wasm: bool,
    /// Boot params (see `crate::boot_params`), if any were supplied.
    pub boot_target_did: Option<String>,
    pub boot_pos: Option<[f32; 3]>,
    pub boot_yaw_deg: Option<f32>,
    pub pds: Option<String>,
    pub relay: Option<String>,
    /// The authenticated session DID — `None` in the `Boot` phase snapshot.
    pub session_did: Option<String>,
}

/// The payload of a [`SessionEvent`]. Internally tagged (`"kind": "…"`) so each
/// JSONL line self-describes; every variant is a unit variant, a struct
/// variant, or a newtype wrapping a struct — the shapes serde internal tagging
/// supports (bare tuple variants are forbidden). The union is drawn from the four
/// priority subsystems surveyed for the suite plus session-level records.
///
/// Fields carry only serde-friendly scalars/strings — domain values (peer ids,
/// DIDs, positions) are pre-formatted to strings/arrays at the call site.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum EventPayload {
    // ---- Session-level -----------------------------------------------------
    /// First record of a session (see [`StartupInfo`]).
    StartupSnapshot(Box<StartupInfo>),
    /// A logout / room-change started a fresh session segment.
    SessionSegmentReset {
        reason: String,
    },
    /// The session ended (clean exit, logout, or a captured crash).
    SessionEnd {
        reason: String,
    },
    /// An invariant/anomaly rule fired (Pillar D routes these in). The event's
    /// own `severity` carries the rule severity.
    InvariantViolation {
        rule: String,
        detail: String,
    },
    /// A periodic flat snapshot of the metrics registry (E-5), so a post-mortem
    /// can chart metric trends over the session. Boxed to keep the enum small.
    MetricsSnapshot(Box<crate::diagnostics::registry::MetricSnapshot>),

    // ---- Loading / state machine ------------------------------------------
    /// Entered `AppState::Loading`.
    LoadingPhaseStarted,
    RecordFetchInitiated {
        record: RecordKind,
        did: String,
        attempt: u32,
    },
    RecordFetchRetrying {
        record: RecordKind,
        did: String,
        attempt: u32,
        backoff_secs: u64,
        reason: String,
    },
    RecordFetchCompleted {
        record: RecordKind,
        did: String,
        status: FetchStatus,
        duration_secs: f64,
    },
    /// A PDS record *write* succeeded — e.g. saving the edited room to the
    /// owner's PDS (`putRecord`, or an `applyWrites` batch for the
    /// split-format room / per-item inventory). The write counterpart of
    /// [`RecordFetchCompleted`](EventPayload::RecordFetchCompleted); makes an
    /// in-game save visible in the analyzer timeline.
    RecordWriteCompleted {
        record: RecordKind,
        did: String,
        duration_secs: f64,
    },
    /// A PDS record write failed (a `putRecord` / `applyWrites` / delete
    /// error).
    RecordWriteFailed {
        record: RecordKind,
        did: String,
        reason: String,
    },
    /// Serialized record payload size measured at a publish attempt (for
    /// split-format rooms, the largest single record the publish writes) —
    /// the single-record-boundary watch (#694). Severity encodes the budget
    /// classification: info under the soft budget, warn past it, error past
    /// the hard ceiling (where the publish was refused pre-flight).
    RecordSizeMeasured {
        record: RecordKind,
        bytes: u64,
        soft_budget_bytes: u64,
        hard_ceiling_bytes: u64,
    },
    /// The room record could not be decoded; the recovery banner was raised.
    RoomRecoveryBannerRaised {
        reason: String,
    },
    HeightmapGenStarted {
        seed: u64,
    },
    HeightmapGenCompleted {
        duration_secs: f64,
        width: u32,
        height: u32,
    },
    SplatTexturesStarted {
        layer_count: u32,
    },
    SplatTexturesCompleted {
        layer_count: u32,
        duration_secs: f64,
    },
    AmbientBakeStarted {
        variant: String,
    },
    AmbientBakeCompleted {
        bytes: u64,
        duration_secs: f64,
    },
    AmbientBakeFallback {
        reason: String,
    },
    WorldCompileStarted {
        placement_count: u32,
    },
    WorldCompileCompleted {
        entity_count: u32,
        duration_secs: f64,
    },
    /// The local player re-seeded their avatar in the editor (a `Reroll(seed)`),
    /// regenerating the avatar visuals. Grouped with the other in-game
    /// regeneration events (region re-seed → heightmap/world-compile) so an
    /// avatar re-seed is visible in the analyzer timeline rather than inferable
    /// only from asset-handle churn.
    AvatarReseeded {
        seed: u64,
    },
    /// All nine loading-gate resources are present.
    LoadingGateReady {
        elapsed_secs: f64,
    },
    /// Transitioned Loading → InGame.
    LoadingGateTransitionToInGame {
        elapsed_secs: f64,
    },
    LoadingGateWarning {
        stage: String,
        message: String,
    },
    LoadingGateTimeout {
        stage: String,
        elapsed_secs: f64,
        expected_max_secs: f64,
    },
    LoginFeedFetchInitiated,
    LoginFeedFetchCompleted {
        post_count: u32,
        duration_secs: f64,
        success: bool,
    },
    AmbientSettleCompleted {
        settled_at_secs: f64,
    },

    // ---- Network / multiuser ----------------------------------------------
    /// The relay's `peer_list` welcome named `count` peers already present in
    /// the room when we joined. Emitted once per (re)connect that finds a
    /// non-empty room, BEFORE any WebRTC data channel opens — so a session log
    /// can tell "joined a populated room" apart from "genuinely alone". A
    /// `SocketPeerListReceived { count >= 1 }` with no following `PeerJoined` is
    /// the fingerprint of a stalled / glared handshake (the app only logs
    /// `PeerJoined` on a *completed* connection, which glare never reaches).
    SocketPeerListReceived {
        count: u64,
    },
    /// The relay refused our WebSocket handshake and the signaller gave up: an
    /// HTTP 4xx (`status`, chiefly `401` from an expired/invalid service-auth
    /// token) or a wasm blind-retry exhaustion (`status == 0`, unknown). The
    /// socket never opens, so this is the *only* trace of an auth-reject —
    /// there is no `peer_list`/`PeerJoined` to follow. `total` is the
    /// session-cumulative rejection count.
    RelayAuthRejected {
        status: u64,
        total: u64,
    },
    PeerJoined {
        peer: String,
    },
    PeerLeft {
        peer: String,
        label: String,
    },
    PeerIdentityVerified {
        peer: String,
        did: String,
        handle: String,
    },
    PeerIdentitySpoofRejected {
        peer: String,
        claimed_did: String,
        authenticated_did: String,
    },
    AvatarFetchStarted {
        peer: String,
        did: String,
    },
    AvatarFetchSucceeded {
        peer: String,
        did: String,
        from_cache: bool,
    },
    AvatarFetchFailed {
        peer: String,
        did: String,
        error: String,
    },
    AvatarStateDecodeFailed {
        peer: String,
        reason: String,
    },
    TransformSampleRejected {
        peer: String,
        reason: String,
    },
    RoomStateRejected {
        sender_did: String,
        reason: String,
    },
    RoomStateDecodeFailed {
        sender_did: String,
        error: String,
    },
    RoomStateApplied,
    ChatReceived {
        sender_did: String,
        text_len: u32,
        muted: bool,
    },
    ChatDroppedMuted {
        sender_did: String,
    },
    /// The local player sent a gift offer to a peer (outbound side of
    /// [`ItemOfferReceived`](EventPayload::ItemOfferReceived)).
    ItemOfferSent {
        offer_id: u64,
        target_did: String,
        item_name: String,
    },
    ItemOfferReceived {
        offer_id: u64,
        sender_did: String,
        item_name: String,
    },
    ItemOfferAutoDeclinedMuted {
        offer_id: u64,
    },
    ItemOfferAutoDeclinedBusy {
        offer_id: u64,
    },
    ItemOfferDecodeFailed {
        reason: String,
    },
    /// An inbound offer decoded cleanly but was rejected before the dialog
    /// was shown (e.g. the item kind is not giftable). Distinct from
    /// [`ItemOfferDecodeFailed`](EventPayload::ItemOfferDecodeFailed), which
    /// is a parse failure.
    ItemOfferRejected {
        offer_id: u64,
        reason: String,
    },
    ItemOfferDialogShown {
        offer_id: u64,
        item_name: String,
    },
    ItemOfferDialogAutoDeclinedTimeout {
        offer_id: u64,
    },
    ItemOfferUserResponded {
        offer_id: u64,
        accepted: bool,
    },
    ItemOfferResponseReceived {
        offer_id: u64,
        accepted: bool,
    },
    PendingOfferTimedOut {
        offer_id: u64,
    },
    PeerMuteToggled {
        peer: String,
        muted: bool,
    },
    SocialResonanceCompleted {
        peer: String,
        resonance: String,
    },
    SocialResonanceFailed {
        peer: String,
        error: String,
    },
    /// A reliable broadcast was refused before send because its serialized
    /// size exceeded [`crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES`]
    /// (#716). `kind` names the message variant (e.g. `"RoomStateUpdate"`).
    /// This is the visible replacement for the fire-and-forget SCTP
    /// `ErrOutboundPacketTooLarge` the app cannot otherwise observe — the
    /// guest did NOT receive this update.
    OutboundMessageOversize {
        message_kind: String,
        bytes: u64,
        ceiling_bytes: u64,
    },

    // ---- Async / offload ---------------------------------------------------
    OffloadJobStarted {
        job: String,
    },
    OffloadJobCompleted {
        job: String,
        duration_secs: f64,
    },
    OffloadJobFailed {
        job: String,
        reason: String,
    },
    OffloadTaskTimeout {
        job: String,
        elapsed_secs: f64,
    },
    WorkerSpawnFailed {
        reason: String,
    },

    // ---- Runtime health ----------------------------------------------------
    RespawnTriggered {
        fell_to_y: f32,
        ground_y: f32,
    },
    PortalTravelInitiated {
        target_did: String,
    },
    PortalTravelCompleted {
        target_did: String,
    },
    /// A portal hop aborted because the destination room record could not be
    /// fetched (transient PDS failure) — the player stays in the current room.
    PortalTravelFailed {
        target_did: String,
        reason: String,
    },
}

impl EventPayload {
    /// The subsystem this payload naturally belongs to. Used as the default
    /// when recording; a caller (e.g. the anomaly router) may override the
    /// event's `subsystem` field for cross-cutting events.
    pub fn subsystem(&self) -> Subsystem {
        use EventPayload::*;
        match self {
            StartupSnapshot(_)
            | SessionSegmentReset { .. }
            | SessionEnd { .. }
            | InvariantViolation { .. }
            | MetricsSnapshot(_) => Subsystem::Session,

            LoadingPhaseStarted
            | RecordFetchInitiated { .. }
            | RecordFetchRetrying { .. }
            | RecordFetchCompleted { .. }
            | RecordWriteCompleted { .. }
            | RecordWriteFailed { .. }
            | RecordSizeMeasured { .. }
            | RoomRecoveryBannerRaised { .. }
            | HeightmapGenStarted { .. }
            | HeightmapGenCompleted { .. }
            | SplatTexturesStarted { .. }
            | SplatTexturesCompleted { .. }
            | AmbientBakeStarted { .. }
            | AmbientBakeCompleted { .. }
            | AmbientBakeFallback { .. }
            | WorldCompileStarted { .. }
            | WorldCompileCompleted { .. }
            | AvatarReseeded { .. }
            | LoadingGateReady { .. }
            | LoadingGateTransitionToInGame { .. }
            | LoadingGateWarning { .. }
            | LoadingGateTimeout { .. }
            | LoginFeedFetchInitiated
            | LoginFeedFetchCompleted { .. }
            | AmbientSettleCompleted { .. } => Subsystem::Loading,

            SocketPeerListReceived { .. }
            | RelayAuthRejected { .. }
            | PeerJoined { .. }
            | PeerLeft { .. }
            | PeerIdentityVerified { .. }
            | PeerIdentitySpoofRejected { .. }
            | AvatarFetchStarted { .. }
            | AvatarFetchSucceeded { .. }
            | AvatarFetchFailed { .. }
            | AvatarStateDecodeFailed { .. }
            | TransformSampleRejected { .. }
            | RoomStateRejected { .. }
            | RoomStateDecodeFailed { .. }
            | RoomStateApplied
            | ChatReceived { .. }
            | ChatDroppedMuted { .. }
            | ItemOfferSent { .. }
            | ItemOfferReceived { .. }
            | ItemOfferAutoDeclinedMuted { .. }
            | ItemOfferAutoDeclinedBusy { .. }
            | ItemOfferDecodeFailed { .. }
            | ItemOfferRejected { .. }
            | ItemOfferDialogShown { .. }
            | ItemOfferDialogAutoDeclinedTimeout { .. }
            | ItemOfferUserResponded { .. }
            | ItemOfferResponseReceived { .. }
            | PendingOfferTimedOut { .. }
            | PeerMuteToggled { .. }
            | SocialResonanceCompleted { .. }
            | SocialResonanceFailed { .. }
            | OutboundMessageOversize { .. } => Subsystem::Network,

            OffloadJobStarted { .. }
            | OffloadJobCompleted { .. }
            | OffloadJobFailed { .. }
            | OffloadTaskTimeout { .. }
            | WorkerSpawnFailed { .. } => Subsystem::Offload,

            RespawnTriggered { .. }
            | PortalTravelInitiated { .. }
            | PortalTravelCompleted { .. }
            | PortalTravelFailed { .. } => Subsystem::Runtime,
        }
    }

    /// The topical category of this payload (middle filter axis).
    pub fn category(&self) -> Category {
        use EventPayload::*;
        match self {
            StartupSnapshot(_) => Category::Snapshot,
            SessionSegmentReset { .. } | SessionEnd { .. } => Category::Lifecycle,
            InvariantViolation { .. } => Category::Anomaly,
            MetricsSnapshot(_) => Category::Snapshot,

            LoadingPhaseStarted
            | LoadingGateReady { .. }
            | LoadingGateTransitionToInGame { .. }
            | LoadingGateWarning { .. }
            | LoadingGateTimeout { .. }
            | AmbientSettleCompleted { .. } => Category::Lifecycle,

            RecordFetchInitiated { .. }
            | RecordFetchRetrying { .. }
            | RecordFetchCompleted { .. }
            | RecordWriteCompleted { .. }
            | RecordWriteFailed { .. }
            | RecordSizeMeasured { .. }
            | RoomRecoveryBannerRaised { .. }
            | LoginFeedFetchInitiated
            | LoginFeedFetchCompleted { .. } => Category::Fetch,

            HeightmapGenStarted { .. }
            | HeightmapGenCompleted { .. }
            | SplatTexturesStarted { .. }
            | SplatTexturesCompleted { .. }
            | WorldCompileStarted { .. }
            | WorldCompileCompleted { .. }
            | AvatarReseeded { .. } => Category::Generation,

            AmbientBakeStarted { .. }
            | AmbientBakeCompleted { .. }
            | AmbientBakeFallback { .. } => Category::Audio,

            SocketPeerListReceived { .. }
            | RelayAuthRejected { .. }
            | PeerJoined { .. }
            | PeerLeft { .. }
            | PeerIdentityVerified { .. }
            | PeerIdentitySpoofRejected { .. }
            | AvatarFetchStarted { .. }
            | AvatarFetchSucceeded { .. }
            | AvatarFetchFailed { .. }
            | AvatarStateDecodeFailed { .. }
            | PeerMuteToggled { .. } => Category::Peer,

            TransformSampleRejected { .. }
            | RoomStateRejected { .. }
            | RoomStateDecodeFailed { .. }
            | RoomStateApplied
            | OutboundMessageOversize { .. } => Category::Transport,

            ChatReceived { .. } | ChatDroppedMuted { .. } => Category::Chat,

            ItemOfferSent { .. }
            | ItemOfferReceived { .. }
            | ItemOfferAutoDeclinedMuted { .. }
            | ItemOfferAutoDeclinedBusy { .. }
            | ItemOfferDecodeFailed { .. }
            | ItemOfferRejected { .. }
            | ItemOfferDialogShown { .. }
            | ItemOfferDialogAutoDeclinedTimeout { .. }
            | ItemOfferUserResponded { .. }
            | ItemOfferResponseReceived { .. }
            | PendingOfferTimedOut { .. } => Category::Offer,

            SocialResonanceCompleted { .. } | SocialResonanceFailed { .. } => Category::Social,

            OffloadJobStarted { .. }
            | OffloadJobCompleted { .. }
            | OffloadJobFailed { .. }
            | OffloadTaskTimeout { .. }
            | WorkerSpawnFailed { .. } => Category::Job,

            RespawnTriggered { .. } => Category::Physics,

            PortalTravelInitiated { .. }
            | PortalTravelCompleted { .. }
            | PortalTravelFailed { .. } => Category::Portal,
        }
    }

    /// A one-line human string for the in-game event log (the tail view keeps
    /// rendering the same terse one-line-per-event view as before).
    pub fn short_line(&self) -> String {
        use EventPayload::*;
        match self {
            StartupSnapshot(s) => format!(
                "startup {:?}: v{} ({}) {}{}",
                s.phase,
                s.version,
                s.git_sha,
                s.target_arch,
                s.session_did
                    .as_deref()
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default()
            ),
            SessionSegmentReset { reason } => format!("session segment reset ({reason})"),
            SessionEnd { reason } => format!("session end ({reason})"),
            InvariantViolation { rule, detail } => format!("⚠ invariant {rule}: {detail}"),
            MetricsSnapshot(s) => format!(
                "metrics snapshot ({} gauges, {} counters, {} hists)",
                s.gauges.len(),
                s.counters.len(),
                s.histograms.len()
            ),

            LoadingPhaseStarted => "loading started".to_string(),
            RecordFetchInitiated {
                record, attempt, ..
            } => {
                format!("{record:?} fetch (attempt {attempt})")
            }
            RecordFetchRetrying {
                record,
                attempt,
                backoff_secs,
                reason,
                ..
            } => {
                format!("{record:?} fetch retry #{attempt} in {backoff_secs}s ({reason})")
            }
            RecordFetchCompleted {
                record,
                status,
                duration_secs,
                ..
            } => {
                format!("{record:?} fetch {status:?} in {duration_secs:.1}s")
            }
            RecordWriteCompleted {
                record,
                duration_secs,
                ..
            } => {
                format!("{record:?} saved to PDS in {duration_secs:.1}s")
            }
            RecordWriteFailed { record, reason, .. } => {
                format!("{record:?} save FAILED ({reason})")
            }
            RecordSizeMeasured {
                record,
                bytes,
                soft_budget_bytes,
                hard_ceiling_bytes,
            } => {
                format!(
                    "{record:?} record size {bytes} B (soft budget {soft_budget_bytes} B, \
                     hard ceiling {hard_ceiling_bytes} B)"
                )
            }
            OutboundMessageOversize {
                message_kind,
                bytes,
                ceiling_bytes,
            } => {
                format!(
                    "⚠ {message_kind} broadcast dropped: {bytes} B over the {ceiling_bytes} B \
                     reliable-payload ceiling (guest did not receive it)"
                )
            }
            RoomRecoveryBannerRaised { reason } => format!("room recovery banner ({reason})"),
            HeightmapGenStarted { seed } => format!("heightmap gen started (seed {seed})"),
            HeightmapGenCompleted {
                duration_secs,
                width,
                height,
            } => {
                format!("heightmap gen done {width}×{height} in {duration_secs:.1}s")
            }
            SplatTexturesStarted { layer_count } => {
                format!("splat textures started ({layer_count} layers)")
            }
            SplatTexturesCompleted {
                layer_count,
                duration_secs,
            } => {
                format!("splat textures done ({layer_count} layers) in {duration_secs:.1}s")
            }
            AmbientBakeStarted { variant } => format!("ambient bake started ({variant})"),
            AmbientBakeCompleted {
                bytes,
                duration_secs,
            } => {
                format!("ambient bake done ({bytes} B) in {duration_secs:.1}s")
            }
            AmbientBakeFallback { reason } => format!("ambient bake fallback ({reason})"),
            WorldCompileStarted { placement_count } => {
                format!("world compile started ({placement_count} placements)")
            }
            WorldCompileCompleted {
                entity_count,
                duration_secs,
            } => {
                format!("world compile done ({entity_count} entities) in {duration_secs:.1}s")
            }
            AvatarReseeded { seed } => format!("avatar reseeded (seed {seed})"),
            LoadingGateReady { elapsed_secs } => {
                format!("loading gate ready ({elapsed_secs:.1}s)")
            }
            LoadingGateTransitionToInGame { elapsed_secs } => {
                format!("→ InGame ({elapsed_secs:.1}s)")
            }
            LoadingGateWarning { stage, message } => format!("gate warning [{stage}]: {message}"),
            LoadingGateTimeout {
                stage,
                elapsed_secs,
                expected_max_secs,
            } => {
                format!("gate TIMEOUT [{stage}] {elapsed_secs:.1}s > {expected_max_secs:.1}s")
            }
            LoginFeedFetchInitiated => "login feed fetch".to_string(),
            LoginFeedFetchCompleted {
                post_count,
                duration_secs,
                success,
            } => {
                format!("login feed {post_count} posts in {duration_secs:.1}s (ok={success})")
            }
            AmbientSettleCompleted { settled_at_secs } => {
                format!("ambient settled ({settled_at_secs:.1}s)")
            }

            SocketPeerListReceived { count } => {
                format!("relay peer_list: {count} peer(s) already in room")
            }
            RelayAuthRejected { status, total } => {
                let code = if *status == 0 {
                    "auth, status unknown".to_string()
                } else {
                    format!("HTTP {status}")
                };
                format!("relay rejected connection ({code}); {total} this session")
            }
            PeerJoined { peer } => format!("peer joined: {peer}"),
            PeerLeft { peer, label } => format!("peer left: {label} ({peer})"),
            PeerIdentityVerified { did, handle, .. } => format!("identity: @{handle} {did}"),
            PeerIdentitySpoofRejected {
                claimed_did,
                authenticated_did,
                ..
            } => {
                format!("SPOOF rejected: claimed {claimed_did} ≠ {authenticated_did}")
            }
            AvatarFetchStarted { did, .. } => format!("avatar fetch: {did}"),
            AvatarFetchSucceeded {
                did, from_cache, ..
            } => {
                format!("avatar ok: {did} (cache={from_cache})")
            }
            AvatarFetchFailed { did, error, .. } => format!("avatar FAILED: {did} ({error})"),
            AvatarStateDecodeFailed { peer, reason } => {
                format!("avatar state decode failed [{peer}]: {reason}")
            }
            TransformSampleRejected { peer, reason } => {
                format!("transform rejected [{peer}]: {reason}")
            }
            RoomStateRejected { sender_did, reason } => {
                format!("room-state rejected from {sender_did} ({reason})")
            }
            RoomStateDecodeFailed { sender_did, error } => {
                format!("room-state decode failed from {sender_did}: {error}")
            }
            RoomStateApplied => "room-state applied".to_string(),
            ChatReceived {
                sender_did,
                text_len,
                muted,
            } => {
                format!("chat from {sender_did} ({text_len} B, muted={muted})")
            }
            ChatDroppedMuted { sender_did } => format!("chat dropped (muted): {sender_did}"),
            ItemOfferSent {
                offer_id,
                target_did,
                item_name,
            } => {
                format!("offer #{offer_id} '{item_name}' sent to {target_did}")
            }
            ItemOfferReceived {
                offer_id,
                sender_did,
                item_name,
            } => {
                format!("offer #{offer_id} '{item_name}' from {sender_did}")
            }
            ItemOfferAutoDeclinedMuted { offer_id } => {
                format!("offer #{offer_id} auto-declined (muted)")
            }
            ItemOfferAutoDeclinedBusy { offer_id } => {
                format!("offer #{offer_id} auto-declined (busy)")
            }
            ItemOfferDecodeFailed { reason } => format!("offer decode failed ({reason})"),
            ItemOfferRejected { offer_id, reason } => {
                format!("offer #{offer_id} rejected ({reason})")
            }
            ItemOfferDialogShown {
                offer_id,
                item_name,
            } => {
                format!("offer #{offer_id} '{item_name}' shown")
            }
            ItemOfferDialogAutoDeclinedTimeout { offer_id } => {
                format!("offer #{offer_id} dialog timed out")
            }
            ItemOfferUserResponded { offer_id, accepted } => {
                format!(
                    "offer #{offer_id} {}",
                    if *accepted { "accepted" } else { "declined" }
                )
            }
            ItemOfferResponseReceived { offer_id, accepted } => {
                format!("offer #{offer_id} response: accepted={accepted}")
            }
            PendingOfferTimedOut { offer_id } => format!("pending offer #{offer_id} timed out"),
            PeerMuteToggled { peer, muted } => format!("peer {peer} muted={muted}"),
            SocialResonanceCompleted { peer, resonance } => {
                format!("resonance [{peer}]: {resonance}")
            }
            SocialResonanceFailed { peer, error } => {
                format!("resonance failed [{peer}]: {error}")
            }

            OffloadJobStarted { job } => format!("offload '{job}' started"),
            OffloadJobCompleted { job, duration_secs } => {
                format!("offload '{job}' done in {duration_secs:.2}s")
            }
            OffloadJobFailed { job, reason } => format!("offload '{job}' FAILED ({reason})"),
            OffloadTaskTimeout { job, elapsed_secs } => {
                format!("offload '{job}' TIMEOUT ({elapsed_secs:.1}s)")
            }
            WorkerSpawnFailed { reason } => format!("worker spawn FAILED ({reason})"),

            RespawnTriggered {
                fell_to_y,
                ground_y,
            } => {
                format!("respawn: fell to y={fell_to_y:.1} (ground {ground_y:.1})")
            }
            PortalTravelInitiated { target_did } => format!("portal → {target_did}"),
            PortalTravelCompleted { target_did } => format!("portal arrived {target_did}"),
            PortalTravelFailed { target_did, reason } => {
                format!("portal → {target_did} FAILED ({reason})")
            }
        }
    }
}

/// One record in the append-only session stream. `t_mono_secs` is
/// session-relative (`Time::elapsed_secs_f64`, the same source the current
/// session-log timestamps use); `wall_ms` is an absolute unix-epoch stamp
/// (web-time on wasm, std on native) for cross-run correlation, `None` when no
/// clock is available. `seq` is a gap-free per-process counter so the analyzer
/// can detect a truncated/torn tail.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionEvent {
    pub seq: u64,
    pub t_mono_secs: f64,
    pub wall_ms: Option<u64>,
    pub subsystem: Subsystem,
    pub category: Category,
    pub severity: Severity,
    pub payload: EventPayload,
}

impl SessionEvent {
    /// Build an event, deriving `subsystem` and `category` from the payload so
    /// call sites only pass the payload + severity (+ the two stamps). The
    /// derived subsystem can be overridden afterward for cross-cutting events.
    pub fn new(
        seq: u64,
        t_mono_secs: f64,
        wall_ms: Option<u64>,
        severity: Severity,
        payload: EventPayload,
    ) -> Self {
        SessionEvent {
            seq,
            t_mono_secs,
            wall_ms,
            subsystem: payload.subsystem(),
            category: payload.category(),
            severity,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One representative event per subsystem group + a unit variant + the
    /// boxed snapshot, so the round-trip test exercises the tag machinery
    /// across every shape (struct / unit / newtype-of-struct).
    fn samples() -> Vec<SessionEvent> {
        let payloads = vec![
            EventPayload::StartupSnapshot(Box::new(StartupInfo {
                phase: SnapshotPhase::Session,
                version: "0.1.0".into(),
                git_sha: "deadbee".into(),
                target_arch: "x86_64".into(),
                profile: "debug".into(),
                wasm: false,
                boot_target_did: Some("did:plc:abc".into()),
                boot_pos: Some([1.0, 2.0, 3.0]),
                boot_yaw_deg: Some(90.0),
                pds: Some("https://pds.example".into()),
                relay: None,
                session_did: Some("did:plc:me".into()),
            })),
            EventPayload::SessionEnd {
                reason: "app_exit".into(),
            },
            EventPayload::LoadingPhaseStarted,
            EventPayload::RecordFetchCompleted {
                record: RecordKind::Room,
                did: "did:plc:me".into(),
                status: FetchStatus::Ok,
                duration_secs: 1.5,
            },
            EventPayload::RecordWriteCompleted {
                record: RecordKind::Room,
                did: "did:plc:me".into(),
                duration_secs: 0.4,
            },
            EventPayload::RecordSizeMeasured {
                record: RecordKind::Room,
                bytes: 123_456,
                soft_budget_bytes: 102_400,
                hard_ceiling_bytes: 921_600,
            },
            EventPayload::AvatarReseeded { seed: 42 },
            EventPayload::PeerIdentitySpoofRejected {
                peer: "peer:7".into(),
                claimed_did: "did:plc:evil".into(),
                authenticated_did: "did:plc:real".into(),
            },
            EventPayload::OffloadJobFailed {
                job: "heightmap".into(),
                reason: "worker gone".into(),
            },
            EventPayload::RespawnTriggered {
                fell_to_y: -30.0,
                ground_y: 4.0,
            },
            EventPayload::InvariantViolation {
                rule: "LoadingGateStall".into(),
                detail: "125s in Loading".into(),
            },
            EventPayload::ItemOfferSent {
                offer_id: 7,
                target_did: "did:plc:friend".into(),
                item_name: "Lantern".into(),
            },
            EventPayload::ItemOfferRejected {
                offer_id: 8,
                reason: "item kind not giftable".into(),
            },
            EventPayload::PortalTravelFailed {
                target_did: "did:plc:elsewhere".into(),
                reason: "PDS timeout".into(),
            },
            EventPayload::OutboundMessageOversize {
                message_kind: "RoomStateUpdate".into(),
                bytes: 950_272,
                ceiling_bytes: 921_600,
            },
        ];
        payloads
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                SessionEvent::new(
                    i as u64,
                    i as f64 * 0.5,
                    Some(1_700_000_000_000 + i as u64),
                    Severity::Info,
                    p,
                )
            })
            .collect()
    }

    #[test]
    fn round_trips_as_ndjson() {
        for ev in samples() {
            let line = serde_json::to_string(&ev).expect("serialize");
            assert!(!line.contains('\n'), "one event must be one line");
            assert!(
                line.contains("\"kind\":"),
                "internally-tagged payload: {line}"
            );
            let back: SessionEvent = serde_json::from_str(&line).expect("deserialize");
            assert_eq!(ev, back, "lossless round-trip for {line}");
        }
    }

    #[test]
    fn subsystem_and_category_are_derived_consistently() {
        for ev in samples() {
            assert_eq!(ev.subsystem, ev.payload.subsystem());
            assert_eq!(ev.category, ev.payload.category());
        }
    }

    #[test]
    fn every_sample_renders_a_short_line() {
        for ev in samples() {
            assert!(!ev.payload.short_line().is_empty());
        }
    }

    #[test]
    fn spoof_rejection_maps_to_network() {
        let p = EventPayload::PeerIdentitySpoofRejected {
            peer: "p".into(),
            claimed_did: "a".into(),
            authenticated_did: "b".into(),
        };
        assert_eq!(p.subsystem(), Subsystem::Network);
        assert_eq!(p.category(), Category::Peer);
    }
}
