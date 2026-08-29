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
///
/// f32 fields whose values can go non-finite (physics state) must route
/// through [`finite_or_sentinel`] at the emit site: `serde_json` writes
/// NaN/±Inf as `null`, silently breaking the NDJSON schema — that is how
/// the offline analyzer dropped 1,461 lines of the #867 meltdown as
/// "unparseable" (#868).
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
    HeightmapGenCompleted {
        duration_secs: f64,
        width: u32,
        height: u32,
        /// Content digest of the sample grid (#1146) — see
        /// [`crate::world_digest`]. Recorded so two captured logs from two
        /// peers of the same room can be compared offline even when neither
        /// peer was running when the other was.
        digest: u64,
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
    WorldCompileCompleted {
        entity_count: u32,
        duration_secs: f64,
        /// Content digest of the compile (#1146) — placement fingerprints in
        /// index order plus `entity_count`. See [`crate::world_digest`].
        digest: u64,
    },
    /// The local player re-seeded their avatar in the editor (a `Reroll(seed)`),
    /// regenerating the avatar visuals. Grouped with the other in-game
    /// regeneration events (region re-seed → heightmap/world-compile) so an
    /// avatar re-seed is visible in the analyzer timeline rather than inferable
    /// only from asset-handle churn.
    AvatarReseeded {
        seed: u64,
    },
    /// One rigged avatar build landed (#1078): the kick-to-land wall time,
    /// which atlas rung it ran at (#1059's draft/full ladder), and whether the
    /// engine produced a body at all.
    RiggedBuildCompleted {
        atlas: u32,
        duration_secs: f64,
        ok: bool,
    },
    /// Transitioned Loading → InGame.
    LoadingGateTransitionToInGame {
        elapsed_secs: f64,
    },
    LoadingGateWarning {
        stage: String,
        message: String,
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
    PeerIdentitySpoofRejected {
        peer: String,
        claimed_did: String,
        authenticated_did: String,
    },
    /// A peer announced a wire protocol that is not ours, or announced none
    /// at all within the grace period (#1121). `theirs` is `None` for the
    /// silent case — a build from before the handshake existed, which is what
    /// every peer running a bundle cached before this change looks like.
    ///
    /// Advisory: nothing is refused on the strength of it. It exists so that
    /// "my gift never arrived" stops being an anecdote — the two builds that
    /// disagreed are named in the log of both ends.
    PeerProtocolMismatch {
        peer: String,
        ours: u16,
        theirs: Option<u16>,
        build: String,
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
    /// One peer's rigged body finished resolving against its owner's PDS
    /// (#1144): the wardrobe record plus `requested` attachment records, of
    /// which `resolved` installed. `body_ok == false` means the peer is
    /// standing as a bare chassis.
    WardrobeResolved {
        did: String,
        requested: u32,
        resolved: u32,
        body_ok: bool,
    },
    /// One worn prop could not be fetched while resolving a peer's body
    /// (#1144). Emitted per skipped rkey, so "the third of five props 5xx'd"
    /// is answerable from a captured log.
    AttachmentFetchFailed {
        did: String,
        rkey: String,
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
    /// An owner's room broadcast was accepted and replaced the live record
    /// (#1146). Declared since the diagnostics suite was built and never
    /// emitted until now, which is why the two historical desyncs (#51, #882)
    /// had no record of WHICH record each peer was deriving from — the apply
    /// path was the one inbound outcome that left no trace at all.
    ///
    /// `bytes` is the received JSON payload size; `digest_of_record` is the
    /// fingerprint of the record as applied, so two peers' logs can be lined
    /// up on "were we even building the same recipe" before anyone argues
    /// about the world.
    RoomStateApplied {
        bytes: u64,
        digest_of_record: u64,
    },
    /// A peer reported a world digest that differs from ours for the SAME
    /// record (#1146). The first captured evidence this project has ever had
    /// that two clients expanded one record into two different worlds.
    ///
    /// Advisory in both directions: the digest arrives over an
    /// unauthenticated data channel and nothing is refused on the strength of
    /// it, so a hostile peer can provoke this event in our log and nothing
    /// else.
    PeerWorldDigestMismatch {
        peer: String,
        record_fp: u64,
        ours: u64,
        theirs: u64,
    },
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
    /// One allocation ≥ 16 MiB landed on the wasm heap (#811). The exact
    /// size is the fingerprint that identifies the owning collection — a ×2
    /// sequence across events is a `Vec` doubling caught red-handed.
    GiantAllocation {
        bytes: u64,
    },

    // ---- Runtime health ----------------------------------------------------
    RespawnTriggered {
        // `deserialize_with` (#868): pre-sentinel builds wrote non-finite
        // values as `null`; mapping those back to NaN keeps old incident
        // logs analyzable instead of dropping the exact lines that
        // describe a NaN cascade as "unparseable".
        #[serde(deserialize_with = "f32_null_as_nan")]
        fell_to_y: f32,
        #[serde(deserialize_with = "f32_null_as_nan")]
        ground_y: f32,
    },
    /// The respawn safety net escalated (#867): repeated respawns inside
    /// the thrash window, or a non-finite body state, mean plain
    /// teleports are not recovering — the whole physics body (collider,
    /// mass, preset) is stripped and rebuilt to shed corrupted solver
    /// state.
    PhysicsBodyRebuilt {
        respawns_recent: u32,
        non_finite: bool,
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
    /// The subsystem this payload belongs to, stamped by [`SessionEvent::new`]
    /// at every recording site. The field is public, so a cross-cutting caller
    /// *could* rewrite it after construction, but none does today — note in
    /// particular that an anomaly fire is logged as an `InvariantViolation`
    /// and therefore lands under [`Subsystem::Session`], not under the firing
    /// rule's own `RuleHeader` subsystem.
    pub fn subsystem(&self) -> Subsystem {
        use EventPayload::*;
        match self {
            StartupSnapshot(_)
            | SessionSegmentReset { .. }
            | SessionEnd { .. }
            | InvariantViolation { .. }
            | MetricsSnapshot(_) => Subsystem::Session,

            LoadingPhaseStarted
            | RecordFetchRetrying { .. }
            | RecordFetchCompleted { .. }
            | RecordWriteCompleted { .. }
            | RecordWriteFailed { .. }
            | RecordSizeMeasured { .. }
            | RoomRecoveryBannerRaised { .. }
            | HeightmapGenCompleted { .. }
            | AmbientBakeStarted { .. }
            | AmbientBakeCompleted { .. }
            | AmbientBakeFallback { .. }
            | WorldCompileCompleted { .. }
            | AvatarReseeded { .. }
            | RiggedBuildCompleted { .. }
            | LoadingGateTransitionToInGame { .. }
            | LoadingGateWarning { .. }
            | AmbientSettleCompleted { .. } => Subsystem::Loading,

            SocketPeerListReceived { .. }
            | RelayAuthRejected { .. }
            | PeerJoined { .. }
            | PeerLeft { .. }
            | PeerIdentitySpoofRejected { .. }
            | PeerProtocolMismatch { .. }
            | AvatarFetchFailed { .. }
            | WardrobeResolved { .. }
            | AttachmentFetchFailed { .. }
            | AvatarStateDecodeFailed { .. }
            | RoomStateRejected { .. }
            | RoomStateDecodeFailed { .. }
            | RoomStateApplied { .. }
            | PeerWorldDigestMismatch { .. }
            | ChatReceived { .. }
            | ChatDroppedMuted { .. }
            | ItemOfferSent { .. }
            | ItemOfferReceived { .. }
            | ItemOfferAutoDeclinedBusy { .. }
            | ItemOfferDecodeFailed { .. }
            | ItemOfferRejected { .. }
            | ItemOfferDialogAutoDeclinedTimeout { .. }
            | ItemOfferUserResponded { .. }
            | ItemOfferResponseReceived { .. }
            | PendingOfferTimedOut { .. }
            | PeerMuteToggled { .. }
            | SocialResonanceCompleted { .. }
            | OutboundMessageOversize { .. } => Subsystem::Network,

            OffloadJobStarted { .. }
            | OffloadJobCompleted { .. }
            | OffloadJobFailed { .. }
            | OffloadTaskTimeout { .. } => Subsystem::Offload,

            RespawnTriggered { .. }
            | PhysicsBodyRebuilt { .. }
            | GiantAllocation { .. }
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
            | LoadingGateTransitionToInGame { .. }
            | LoadingGateWarning { .. }
            | AmbientSettleCompleted { .. } => Category::Lifecycle,

            RecordFetchRetrying { .. }
            | RecordFetchCompleted { .. }
            | RecordWriteCompleted { .. }
            | RecordWriteFailed { .. }
            | RecordSizeMeasured { .. }
            | RoomRecoveryBannerRaised { .. } => Category::Fetch,

            HeightmapGenCompleted { .. }
            | WorldCompileCompleted { .. }
            | AvatarReseeded { .. }
            | RiggedBuildCompleted { .. } => Category::Generation,

            AmbientBakeStarted { .. }
            | AmbientBakeCompleted { .. }
            | AmbientBakeFallback { .. } => Category::Audio,

            SocketPeerListReceived { .. }
            | RelayAuthRejected { .. }
            | PeerJoined { .. }
            | PeerLeft { .. }
            | PeerIdentitySpoofRejected { .. }
            | PeerProtocolMismatch { .. }
            | AvatarFetchFailed { .. }
            | WardrobeResolved { .. }
            | AttachmentFetchFailed { .. }
            | AvatarStateDecodeFailed { .. }
            | PeerMuteToggled { .. } => Category::Peer,

            RoomStateRejected { .. }
            | RoomStateDecodeFailed { .. }
            | RoomStateApplied { .. }
            | PeerWorldDigestMismatch { .. }
            | OutboundMessageOversize { .. } => Category::Transport,

            ChatReceived { .. } | ChatDroppedMuted { .. } => Category::Chat,

            ItemOfferSent { .. }
            | ItemOfferReceived { .. }
            | ItemOfferAutoDeclinedBusy { .. }
            | ItemOfferDecodeFailed { .. }
            | ItemOfferRejected { .. }
            | ItemOfferDialogAutoDeclinedTimeout { .. }
            | ItemOfferUserResponded { .. }
            | ItemOfferResponseReceived { .. }
            | PendingOfferTimedOut { .. } => Category::Offer,

            SocialResonanceCompleted { .. } => Category::Social,

            OffloadJobStarted { .. }
            | OffloadJobCompleted { .. }
            | OffloadJobFailed { .. }
            | OffloadTaskTimeout { .. } => Category::Job,

            RespawnTriggered { .. } | PhysicsBodyRebuilt { .. } => Category::Physics,

            GiantAllocation { .. } => Category::Perf,

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
            HeightmapGenCompleted {
                duration_secs,
                width,
                height,
                ..
            } => {
                format!("heightmap gen done {width}×{height} in {duration_secs:.1}s")
            }
            AmbientBakeStarted { variant } => format!("ambient bake started ({variant})"),
            AmbientBakeCompleted {
                bytes,
                duration_secs,
            } => {
                format!("ambient bake done ({bytes} B) in {duration_secs:.1}s")
            }
            AmbientBakeFallback { reason } => format!("ambient bake fallback ({reason})"),
            WorldCompileCompleted {
                entity_count,
                duration_secs,
                ..
            } => {
                format!("world compile done ({entity_count} entities) in {duration_secs:.1}s")
            }
            AvatarReseeded { seed } => format!("avatar reseeded (seed {seed})"),
            RiggedBuildCompleted {
                atlas,
                duration_secs,
                ok,
            } => format!(
                "rigged build {} at atlas {atlas} in {duration_secs:.2}s",
                if *ok { "landed" } else { "FAILED" },
            ),
            LoadingGateTransitionToInGame { elapsed_secs } => {
                format!("→ InGame ({elapsed_secs:.1}s)")
            }
            LoadingGateWarning { stage, message } => format!("gate warning [{stage}]: {message}"),
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
            PeerIdentitySpoofRejected {
                claimed_did,
                authenticated_did,
                ..
            } => {
                format!("SPOOF rejected: claimed {claimed_did} ≠ {authenticated_did}")
            }
            PeerProtocolMismatch {
                peer,
                ours,
                theirs,
                build,
            } => match theirs {
                Some(t) => format!("peer {peer} speaks protocol {t}, we speak {ours} ({build})"),
                None => format!("peer {peer} announced no protocol (we speak {ours})"),
            },
            AvatarFetchFailed { did, error, .. } => format!("avatar FAILED: {did} ({error})"),
            WardrobeResolved {
                did,
                requested,
                resolved,
                body_ok,
            } => format!(
                "wardrobe resolved: {did} body={} props {resolved}/{requested}",
                if *body_ok { "ok" } else { "MISSING" }
            ),
            AttachmentFetchFailed { did, rkey, reason } => {
                format!("attachment {rkey} FAILED for {did} ({reason})")
            }
            AvatarStateDecodeFailed { peer, reason } => {
                format!("avatar state decode failed [{peer}]: {reason}")
            }
            RoomStateRejected { sender_did, reason } => {
                format!("room-state rejected from {sender_did} ({reason})")
            }
            RoomStateDecodeFailed { sender_did, error } => {
                format!("room-state decode failed from {sender_did}: {error}")
            }
            RoomStateApplied {
                bytes,
                digest_of_record,
            } => format!("room-state applied ({bytes} B, record {digest_of_record:016x})"),
            PeerWorldDigestMismatch {
                peer,
                record_fp,
                ours,
                theirs,
            } => format!(
                "DESYNC: peer {peer} built {theirs:016x}, we built {ours:016x} from record {record_fp:016x}"
            ),
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
            ItemOfferAutoDeclinedBusy { offer_id } => {
                format!("offer #{offer_id} auto-declined (busy)")
            }
            ItemOfferDecodeFailed { reason } => format!("offer decode failed ({reason})"),
            ItemOfferRejected { offer_id, reason } => {
                format!("offer #{offer_id} rejected ({reason})")
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

            OffloadJobStarted { job } => format!("offload '{job}' started"),
            OffloadJobCompleted { job, duration_secs } => {
                format!("offload '{job}' done in {duration_secs:.2}s")
            }
            OffloadJobFailed { job, reason } => format!("offload '{job}' FAILED ({reason})"),
            OffloadTaskTimeout { job, elapsed_secs } => {
                format!("offload '{job}' TIMEOUT ({elapsed_secs:.1}s)")
            }
            GiantAllocation { bytes } => {
                format!(
                    "giant allocation: {:.1} MiB",
                    *bytes as f64 / (1024.0 * 1024.0)
                )
            }

            RespawnTriggered {
                fell_to_y,
                ground_y,
            } => {
                format!("respawn: fell to y={fell_to_y:.1} (ground {ground_y:.1})")
            }
            PhysicsBodyRebuilt {
                respawns_recent,
                non_finite,
            } => format!(
                "physics body rebuilt ({respawns_recent} respawns in window{})",
                if *non_finite {
                    ", non-finite state"
                } else {
                    ""
                }
            ),
            PortalTravelInitiated { target_did } => format!("portal → {target_did}"),
            PortalTravelCompleted { target_did } => format!("portal arrived {target_did}"),
            PortalTravelFailed { target_did, reason } => {
                format!("portal → {target_did} FAILED ({reason})")
            }
        }
    }
}

/// Sentinel magnitude [`finite_or_sentinel`] substitutes for non-finite
/// floats: far beyond any legitimate world coordinate, well inside f32
/// range, and round-trip-stable through JSON.
pub const NON_FINITE_SENTINEL: f32 = 1.0e30;

/// Deserialize an `f32` that a pre-#868 build may have written as
/// `null` (`serde_json` encodes NaN/±Inf that way): map `null` back to
/// NaN so old incident logs parse.
fn f32_null_as_nan<'de, D>(d: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(<Option<f32> as serde::Deserialize>::deserialize(d)?.unwrap_or(f32::NAN))
}

/// Clamp a physics-derived float into something `serde_json` can encode
/// (#868): NaN/±Inf would serialize as `null` and break the NDJSON
/// schema for every downstream reader. NaN maps to the positive
/// sentinel; ±Inf keep their sign. The magnitude is recognisably
/// impossible, so a report line reading `1e30` says "non-finite at the
/// source" rather than pretending precision.
pub fn finite_or_sentinel(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else if v == f32::NEG_INFINITY {
        -NON_FINITE_SENTINEL
    } else {
        NON_FINITE_SENTINEL
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

/// `seq` of the synthetic marker the PANIC hook appends (#1142). The real
/// sequence counter lives on `SessionLog`, which is a Bevy `Resource` and
/// therefore unreachable from a hook — so a marker claims a value no real
/// event can ever hold, and readers key on that rather than on its position
/// in the file.
pub const CRASH_MARKER_SEQ: u64 = u64::MAX;

/// `seq` of the marker the wasm `pagehide` hook appends on a CLEAN tab close
/// (#1145).
///
/// A separate sentinel rather than a reason string the reader has to parse,
/// because the three wasm exits have to be told apart structurally: this
/// marker means the tab closed, [`CRASH_MARKER_SEQ`] means a Rust panic ran a
/// hook, and NO marker at all means the tab died without either — an OOM trap
/// or a browser kill, which is the case worth escalating and was previously
/// indistinguishable from simply closing the tab.
pub const CLOSE_MARKER_SEQ: u64 = u64::MAX - 1;

/// The last session-relative timestamp the log covers.
///
/// The MAXIMUM, not the final element (#1142). The crash marker is appended
/// last but was stamped `t_mono_secs = 0.0` for want of a clock inside the
/// panic hook, so reading "the last event's timestamp" off a panic file
/// returned zero — which made the session look like it ended before it
/// began, and silently switched off every "started but never finished"
/// check on the one artefact where a hung job is the likeliest story. The
/// hook stamps the marker properly now; taking the max also repairs the
/// reading of every panic file already on disk.
pub fn last_ts(events: &[SessionEvent]) -> f64 {
    events
        .iter()
        .map(|e| e.t_mono_secs)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(0.0)
}

/// The session-relative span the log covers: its widest timestamp minus its
/// narrowest. Min/max rather than last-minus-first for the reason in
/// [`last_ts`] — a zero-stamped marker at the end of a panic file otherwise
/// yields a NEGATIVE duration in the report header.
pub fn span_secs(events: &[SessionEvent]) -> f64 {
    if events.is_empty() {
        return 0.0;
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for e in events {
        lo = lo.min(e.t_mono_secs);
        hi = hi.max(e.t_mono_secs);
    }
    (hi - lo).max(0.0)
}

impl SessionEvent {
    /// Whether this is the panic hook's synthetic crash marker rather than
    /// something the running app recorded. See [`CRASH_MARKER_SEQ`].
    pub fn is_crash_marker(&self) -> bool {
        self.seq == CRASH_MARKER_SEQ
    }

    /// Whether a shutdown hook wrote this rather than the running app — a
    /// panic ([`CRASH_MARKER_SEQ`]) or a clean tab close
    /// ([`CLOSE_MARKER_SEQ`]).
    pub fn is_hook_marker(&self) -> bool {
        self.seq >= CLOSE_MARKER_SEQ
    }

    /// Build an event, deriving `subsystem` and `category` from the payload so
    /// call sites only pass the payload + severity (+ the two stamps). Nothing
    /// overrides the derived pair afterwards, so `subsystem` / `category` are
    /// always exactly what the payload dictates.
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

    /// Every `EventPayload` variant declared in this file.
    ///
    /// Read out of the source rather than listed by hand: a hand-kept list
    /// would only prove that somebody remembered to extend the list.
    fn declared_variants(source: &str) -> Vec<String> {
        let body = source
            .split_once("pub enum EventPayload {")
            .and_then(|(_, rest)| rest.split_once("\n}\n"))
            .expect("the payload enum is in this file")
            .0;
        let mut out = Vec::new();
        for line in body.lines() {
            // A declaration sits at four-space indent and is followed by its
            // shape: `Foo {`, `Foo(`, or a bare `Foo,`.
            let Some(rest) = line.strip_prefix("    ") else {
                continue;
            };
            if rest.starts_with(' ') || rest.starts_with("//") || rest.starts_with('#') {
                continue;
            }
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            // `Foo {`, `Foo(` or a bare `Foo,` — the space before a brace is
            // why this trims first.
            if name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && rest[name.len()..].trim_start().starts_with(['{', '(', ','])
            {
                out.push(name);
            }
        }
        out
    }

    /// Strip every `#[cfg(test)]` module from a source file, so a variant
    /// constructed only by a test fixture does not read as emitted.
    ///
    /// #1146: this guard passed `RoomStateApplied` for as long as that variant
    /// existed — declared, promised by the schema, emitted by nothing — because
    /// three test fixtures name it. That is the exact failure the guard was
    /// written to prevent, so the guard was checking the wrong thing: a
    /// variant earns its place by being emitted in PRODUCTION, and a fixture
    /// that constructs one is testing the schema, not using it.
    ///
    /// Brace-counted from the `#[cfg(test)]` attribute to the module's closing
    /// brace. Crude, and adequate: it is reading Rust this crate wrote, where
    /// a `#[cfg(test)] mod` is always a real module with balanced braces, and
    /// the failure mode of over-stripping is a false alarm on a variant that
    /// then has to prove itself — never a silent pass.
    fn strip_test_modules(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let Some(open) = after.find('{') else {
                // A `#[cfg(test)]` with no following block (an item attribute
                // on a `use`, say) — nothing to strip.
                break;
            };
            let mut depth = 0usize;
            let mut end = None;
            for (i, c) in after[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(open + i + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            match end {
                Some(e) => rest = &after[e..],
                // Unbalanced: stop stripping rather than silently discard the
                // rest of the file, which would turn every remaining emit site
                // invisible and fail the test for the wrong reason.
                None => {
                    rest = after;
                    break;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Whether `name` is CONSTRUCTED in production code outside this file — a
    /// match pattern reads a variant, it does not produce one, and neither
    /// does a test fixture.
    fn is_emitted(name: &str, sources: &[(String, String)]) -> bool {
        let needle = format!("EventPayload::{name}");
        sources.iter().any(|(path, text)| {
            !path.ends_with("diagnostics/event.rs")
                && strip_test_modules(text).lines().any(|line| {
                    line.contains(&needle)
                        && !line.contains("=>")
                        && !line.contains("matches!")
                        && !line.trim_start().starts_with('|')
                })
        })
    }

    fn crate_sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && let Ok(text) = std::fs::read_to_string(&path)
                {
                    out.push((path.to_string_lossy().replace('\\', "/"), text));
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut out,
        );
        assert!(!out.is_empty(), "found no sources to scan");
        out
    }

    /// #1144. A third of this schema was fiction: 25 variants had no emit site
    /// anywhere in the crate, several of which the analyzer and the docs
    /// actively promise — `[Timeline]` rendered "portal → did" for an event
    /// nothing ever wrote, so a session with several hops read as a session
    /// with none. An agent reading this file (documented as the authoritative
    /// schema) builds filters and expectations around records that never
    /// occur.
    ///
    /// This is deliberately a source scan rather than a hand-maintained
    /// `EMITTED_KINDS` list, because a list only proves somebody remembered to
    /// extend it. The rule (#672) is: a typed variant earns its place by being
    /// emitted; otherwise delete it and let a generic channel carry the
    /// signal. A variant added ahead of its emit site fails here — add the
    /// emit, or do not add the variant yet.
    #[test]
    fn every_declared_event_variant_has_an_emit_site() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagnostics/event.rs"),
        )
        .expect("this file");
        let declared = declared_variants(&source);
        assert!(
            declared.len() > 40,
            "the variant scan found only {} — it has stopped matching the file's shape",
            declared.len()
        );

        let sources = crate_sources();
        // #1146: `strip_test_modules` is what makes this guard mean what it
        // says. Assert it actually found something to strip, so a future
        // refactor that moves the fixtures (or renames the attribute) turns
        // this back into the permissive check it used to be LOUDLY rather
        // than silently.
        assert!(
            sources
                .iter()
                .any(|(_, text)| strip_test_modules(text).len() < text.len()),
            "no #[cfg(test)] module was stripped — the guard has stopped \
             distinguishing a production emit from a test fixture"
        );
        let dead: Vec<&String> = declared
            .iter()
            .filter(|name| !is_emitted(name, &sources))
            .collect();
        assert!(
            dead.is_empty(),
            "declared but never emitted: {dead:?}\n\
             Either emit it, or delete the variant — a schema that promises \
             records nothing writes sends its readers looking for them."
        );
    }

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
