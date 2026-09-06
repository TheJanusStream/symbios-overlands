//! Built-in invariant rules.
//!
//! This slice (D-2) holds the **log-expressible** invariants: conditions fully
//! determined by the event stream, so each implements a REPLAY body (folded by
//! the offline analyzer) and, where an equivalent live signal exists, a LIVE
//! body too. The ECS-state rules (NaN transforms, terrain collider, asset
//! growth, …) are added by the D-3 slice, which extends [`register_builtins`].

use crate::diagnostics::anomaly::registry::InvariantRegistry;
use crate::diagnostics::anomaly::rule::{DebouncePolicy, LiveCtx, Rule, RuleHeader, Verdict};
use crate::diagnostics::event::{EventPayload, FetchStatus, SessionEvent, Severity, Subsystem};
use crate::diagnostics::names;
use crate::state::AppState;

/// Register every built-in rule. Shared by the live plugin (D-4) and the
/// offline analyzer (D-5) via [`super::registry::default_registry`].
pub fn register_builtins(reg: &mut InvariantRegistry) {
    reg.register(LoadingGateStall);
    reg.register(RecordFetchExhausted);
    reg.register(AmbientBakeStall);
    reg.register(TaskNeverResolves);
    reg.register(PeerChurnSpike);
    reg.register(OfferAcceptanceAnomaly);
    reg.register(IdentitySpoofBurst);
    reg.register(PeerProtocolMismatched);
    reg.register(WorldDigestMismatch);
    reg.register(SilentDecodeFailure);
    reg.register(GlareSuspected);
    reg.register(RelayConnectionRejected);
    reg.register(RelayTokenRefreshFailing);
    reg.register(WardrobeUnresolved);
    reg.register(AssetFetchFailing);
    // D-3 ECS-state (live-only) rules.
    super::rules_ecs::register_ecs_rules(reg);
}

/// The last timestamp the log covers, for "never resolved" checks.
///
/// The maximum, not the final element — see
/// [`crate::diagnostics::event::last_ts`] for why the distinction is what
/// keeps these rules alive on a panic file (#1142).
fn last_ts(events: &[SessionEvent]) -> f64 {
    crate::diagnostics::event::last_ts(events)
}

/// Durations of unresolved / over-budget start→end spans: for each event
/// matching `is_start`, find the first later event matching `is_end`; emit the
/// gap when it exceeds `budget`, or the elapsed-so-far when it never resolved
/// but the log continued past `budget`.
fn stall_durations(
    events: &[SessionEvent],
    is_start: impl Fn(&EventPayload) -> bool,
    is_end: impl Fn(&EventPayload) -> bool,
    budget: f64,
) -> Vec<f64> {
    let last = last_ts(events);
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if !is_start(&e.payload) {
            continue;
        }
        let start = e.t_mono_secs;
        let end = events[i + 1..]
            .iter()
            .find(|f| is_end(&f.payload))
            .map(|f| f.t_mono_secs);
        match end {
            Some(t) if t - start > budget => out.push(t - start),
            None if last - start > budget => out.push(last - start),
            _ => {}
        }
    }
    out
}

// --- WardrobeUnresolved ------------------------------------------------------
struct WardrobeUnresolved;
const WARDROBE_UNRESOLVED: RuleHeader = RuleHeader {
    id: "net.wardrobe_unresolved",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "someone here is showing as a plain figure — their look could not be loaded",
    technical: Some(
        "a peer's wardrobe record resolved with no body, so they render as a bare chassis",
    ),
    when_state: None,
};
impl Rule for WardrobeUnresolved {
    fn header(&self) -> &RuleHeader {
        &WARDROBE_UNRESOLVED
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Replay-only on purpose: the live signal is per-peer and arrives as a
    /// discrete event, not as a gauge a 1 Hz tick could sample. The captured
    /// log is also where the question is actually asked — "why was Bob a bare
    /// chassis for me but not for Alice" is a post-mortem question (#1144).
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::WardrobeResolved {
                    did,
                    body_ok: false,
                    ..
                } => Some(Verdict::violated(format!(
                    "{did} resolved no wardrobe record — rendered as a bare chassis"
                ))),
                _ => None,
            })
            .collect()
    }
}

// --- AssetFetchFailing ------------------------------------------------------
/// How many asset failures in one session are worth a verdict.
///
/// Not one: a single dead source in a room somebody else authored is
/// ordinary, and a rule that fires on it would light the toolbar dot in
/// every second room. Three is the point at which the pattern is the room
/// rather than the asset — a host that is down, a whole batch of references
/// pointing at a moved bucket, or a client whose network is gone.
const ASSET_FAILURE_VERDICT_THRESHOLD: usize = 3;

struct AssetFetchFailing;
const ASSET_FETCH_FAILING: RuleHeader = RuleHeader {
    id: "asset.fetch_failing",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::OncePerCondition,
    description: "pictures, sounds or ground textures in this world are failing to load",
    technical: Some(
        "three or more asset fetches failed for one class this session — usually a host that \
         is down or a batch of references pointing at a moved bucket",
    ),
    when_state: None,
};
impl Rule for AssetFetchFailing {
    fn header(&self) -> &RuleHeader {
        &ASSET_FETCH_FAILING
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Replay-only, like [`WardrobeUnresolved`] and for the same reason: the
    /// live signal is a discrete per-source event, and the question ("why did
    /// this room have no pictures") is asked afterwards.
    ///
    /// Before #1246 there was no signal at all — a grep for `metrics` or
    /// `diagnostics` across the five asset fetch paths returned nothing, so
    /// the app's designated "something is wrong" channel was blind to the one
    /// surface that fails silently by construction and depends entirely on
    /// third-party hosts.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let mut by_class: std::collections::BTreeMap<&str, usize> = Default::default();
        for event in events {
            if let EventPayload::AssetFetchFailed { asset, .. } = &event.payload {
                *by_class.entry(asset.as_str()).or_default() += 1;
            }
        }
        by_class
            .into_iter()
            .filter(|(_, count)| *count >= ASSET_FAILURE_VERDICT_THRESHOLD)
            .map(|(class, count)| {
                Verdict::violated(format!("{count} {class} assets could not be fetched"))
            })
            .collect()
    }
}

// --- LoadingGateStall -------------------------------------------------------
/// The loading gate is considered stalled (Critical) past this many seconds —
/// the shared threshold the live rule, the replay rule, and the loading-screen
/// countdown (C-5) all colour against.
pub const GATE_STALL_SECS: f64 = 120.0;

struct LoadingGateStall;
const LOADING_GATE_STALL: RuleHeader = RuleHeader {
    id: "loading.gate_stall",
    subsystem: Subsystem::Loading,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "this world is taking much longer than usual to open",
    technical: Some("the sign-in to in-world gate ran past its 120 s budget"),
    when_state: Some(AppState::Loading),
};
impl Rule for LoadingGateStall {
    fn header(&self) -> &RuleHeader {
        &LOADING_GATE_STALL
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let elapsed = cx.loading_elapsed_secs?;
        Some(if elapsed > GATE_STALL_SECS {
            Verdict::violated(format!(
                "in loading gate {elapsed:.0}s (> {GATE_STALL_SECS:.0}s)"
            ))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        stall_durations(
            events,
            |p| matches!(p, EventPayload::LoadingPhaseStarted),
            |p| matches!(p, EventPayload::LoadingGateTransitionToInGame { .. }),
            GATE_STALL_SECS,
        )
        .into_iter()
        .map(|d| Verdict::violated(format!("loading gate took {d:.0}s")))
        .collect()
    }
}

// --- RecordFetchExhausted ---------------------------------------------------
struct RecordFetchExhausted;
const RECORD_FETCH_EXHAUSTED: RuleHeader = RuleHeader {
    id: "loading.record_fetch_exhausted",
    subsystem: Subsystem::Loading,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "something stored on your account could not be fetched after several tries",
    technical: Some("a record fetch against the account's data server exhausted its retries"),
    when_state: None,
};
impl Rule for RecordFetchExhausted {
    fn header(&self) -> &RuleHeader {
        &RECORD_FETCH_EXHAUSTED
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::RecordFetchCompleted {
                    record,
                    did,
                    status: FetchStatus::Exhausted,
                    ..
                } => Some(Verdict::violated(format!(
                    "{record:?} fetch for {did} exhausted retries"
                ))),
                _ => None,
            })
            .collect()
    }
}

// --- AmbientBakeStall -------------------------------------------------------
const AMBIENT_STALL_SECS: f64 = 30.0;

struct AmbientBakeStall;
const AMBIENT_BAKE_STALL: RuleHeader = RuleHeader {
    id: "offload.ambient_bake_stall",
    subsystem: Subsystem::Offload,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "background sound for this world is taking too long to prepare — you may hear \
                  silence",
    technical: Some("the ambient audio bake did not finish within its 30 s budget"),
    when_state: None,
};
impl Rule for AmbientBakeStall {
    fn header(&self) -> &RuleHeader {
        &AMBIENT_BAKE_STALL
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        stall_durations(
            events,
            |p| matches!(p, EventPayload::AmbientBakeStarted { .. }),
            |p| {
                matches!(
                    p,
                    EventPayload::AmbientBakeCompleted { .. }
                        | EventPayload::AmbientBakeFallback { .. }
                )
            },
            AMBIENT_STALL_SECS,
        )
        .into_iter()
        .map(|d| Verdict::violated(format!("ambient bake took {d:.0}s")))
        .collect()
    }
}

// --- TaskNeverResolves (offload jobs, keyed by job name) --------------------
use crate::diagnostics::offload_watch::TASK_TIMEOUT_SECS;

struct TaskNeverResolves;
const TASK_NEVER_RESOLVES: RuleHeader = RuleHeader {
    id: "offload.task_never_resolves",
    subsystem: Subsystem::Offload,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a background task stopped responding — part of this world may never finish \
                  loading",
    technical: Some("an offloaded job never reported completion or failure"),
    when_state: None,
};
impl Rule for TaskNeverResolves {
    fn header(&self) -> &RuleHeader {
        &TASK_NEVER_RESOLVES
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live body added in #1143. Before it this rule was replay-only, and its
    /// replay had nothing to fold either — `OffloadJobStarted` had no emit
    /// site in the crate, so the one rule written for "a worker job never
    /// answered" could not fire on any log, live or captured. The offload
    /// census now feeds both.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let (job, elapsed) = cx.oldest_pending_job?;
        Some(if elapsed > TASK_TIMEOUT_SECS {
            Verdict::violated(format!(
                "job '{job}' in flight {elapsed:.0}s (> {TASK_TIMEOUT_SECS:.0}s)"
            ))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let last = last_ts(events);
        let mut out = Vec::new();
        for (i, e) in events.iter().enumerate() {
            let EventPayload::OffloadJobStarted { job } = &e.payload else {
                continue;
            };
            let start = e.t_mono_secs;
            let end = events[i + 1..].iter().find_map(|f| match &f.payload {
                EventPayload::OffloadJobCompleted { job: j, .. }
                | EventPayload::OffloadJobFailed { job: j, .. }
                    if j == job =>
                {
                    Some(f.t_mono_secs)
                }
                _ => None,
            });
            match end {
                Some(t) if t - start > TASK_TIMEOUT_SECS => out.push(Verdict::violated(format!(
                    "job '{job}' took {:.0}s",
                    t - start
                ))),
                None if last - start > TASK_TIMEOUT_SECS => {
                    out.push(Verdict::violated(format!("job '{job}' never resolved")))
                }
                _ => {}
            }
        }
        out
    }
}

// --- PeerChurnSpike (windowed) ----------------------------------------------
const CHURN_WINDOW_SECS: f64 = 300.0;
const CHURN_LIMIT: usize = 10;

struct PeerChurnSpike;
const PEER_CHURN_SPIKE: RuleHeader = RuleHeader {
    id: "net.peer_churn_spike",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a lot of people left this world at once — the connection may be dropping them",
    technical: Some("more than 10 peer departures inside a 300 s window"),
    when_state: None,
};
impl Rule for PeerChurnSpike {
    fn header(&self) -> &RuleHeader {
        &PEER_CHURN_SPIKE
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn has_live_body(&self) -> bool {
        true
    }
    /// Live: departures inside the counter's retained window (#1272 f173).
    ///
    /// It was replay-only, and `ui::diagnostics` mapped the "Leaves
    /// (session)" row to it anyway — so that row carried a dot that could
    /// never light, which reads as a check that passed. The counter ring
    /// (#1271 f179) is what makes a windowed count expressible at all;
    /// before it there was nothing live to threshold but a session total,
    /// and a session total cannot say "spike".
    ///
    /// **The live window is the ring's, which is SHORTER than the replay
    /// window, and the limit is deliberately the same number.** That makes
    /// the live body a strict subset: it fires only on a burst tight enough
    /// to fit inside the retained window, so it can never claim a spike the
    /// analyzer would not also find. Matching the replay window exactly
    /// would need a ring five times longer for a `Warn` nobody acts on
    /// mid-session.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let leaves = cx
            .metrics
            .counter_window_rise(names::NET_PEER_DISCONNECTED_COUNT)?;
        Some(if leaves as usize > CHURN_LIMIT {
            Verdict::violated(format!("{leaves} people left in the last few minutes"))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let leaves: Vec<f64> = events
            .iter()
            .filter(|e| matches!(e.payload, EventPayload::PeerLeft { .. }))
            .map(|e| e.t_mono_secs)
            .collect();
        // Any window [t, t+W] containing more than the limit is a spike.
        for (i, &t0) in leaves.iter().enumerate() {
            let count = leaves[i..]
                .iter()
                .take_while(|&&t| t - t0 <= CHURN_WINDOW_SECS)
                .count();
            if count > CHURN_LIMIT {
                return vec![Verdict::violated(format!(
                    "{count} peers left within {CHURN_WINDOW_SECS:.0}s"
                ))];
            }
        }
        Vec::new()
    }
}

// --- OfferAcceptanceAnomaly -------------------------------------------------
const MIN_OFFERS_FOR_RATIO: usize = 10;

struct OfferAcceptanceAnomaly;
const OFFER_ACCEPTANCE_ANOMALY: RuleHeader = RuleHeader {
    id: "net.offer_acceptance_anomaly",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::OncePerCondition,
    description: "item offers here are being answered in a pattern that does not look like a \
                  person",
    technical: Some("item-offer accept ratio outside 10–90% over at least 10 offers"),
    when_state: None,
};
impl Rule for OfferAcceptanceAnomaly {
    fn header(&self) -> &RuleHeader {
        &OFFER_ACCEPTANCE_ANOMALY
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn has_live_body(&self) -> bool {
        true
    }
    /// Live: the same ratio over the two counters the replay's
    /// `ItemOfferUserResponded` events increment (#1272 f173) — accepted
    /// and declined are the two arms of a user's answer, and the busy
    /// auto-decline is counted separately and excluded from both, exactly
    /// as the replay excludes it.
    ///
    /// **Cumulative on purpose**, unlike the two rules #1271 f179 had to
    /// window. Those thresholded a total that only ever rises, so they
    /// could never clear. A RATIO is not a total: answering the next few
    /// offers the other way moves it back inside the band and the rule
    /// clears itself. It is genuinely level-triggered.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let acc = cx.metrics.counter(names::NET_OFFER_ACCEPTED_COUNT)?.value();
        let dec = cx.metrics.counter(names::NET_OFFER_DECLINED_COUNT)?.value();
        let total = acc + dec;
        if (total as usize) < MIN_OFFERS_FOR_RATIO {
            return Some(Verdict::Clear);
        }
        let ratio = acc as f64 / total as f64;
        Some(if !(0.1..=0.9).contains(&ratio) {
            Verdict::violated(format!(
                "accept ratio {:.0}% over {total} offers",
                ratio * 100.0
            ))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let (mut acc, mut total) = (0usize, 0usize);
        for e in events {
            if let EventPayload::ItemOfferUserResponded { accepted, .. } = &e.payload {
                total += 1;
                acc += usize::from(*accepted);
            }
        }
        if total < MIN_OFFERS_FOR_RATIO {
            return Vec::new();
        }
        let ratio = acc as f64 / total as f64;
        if !(0.1..=0.9).contains(&ratio) {
            // Percent, not the raw fraction: the rule only fires below 0.1 or
            // above 0.9, so `{ratio:.0}` collapsed every violation to "0" or
            // "1" (#635f). "5%"/"95%" is what the reader needs.
            vec![Verdict::violated(format!(
                "accept ratio {:.0}% over {total} offers",
                ratio * 100.0
            ))]
        } else {
            Vec::new()
        }
    }
}

// --- IdentitySpoofBurst -----------------------------------------------------
const SPOOF_LIMIT: u64 = 3;

struct IdentitySpoofBurst;
const IDENTITY_SPOOF_BURST: RuleHeader = RuleHeader {
    id: "net.identity_spoof_burst",
    subsystem: Subsystem::Network,
    severity: Severity::Warn,
    debounce: DebouncePolicy::Interval(30.0),
    description: "someone here keeps claiming to be a different account",
    technical: Some("repeated spoofed identity claims from peers, rejected on arrival"),
    when_state: None,
};
impl Rule for IdentitySpoofBurst {
    fn header(&self) -> &RuleHeader {
        &IDENTITY_SPOOF_BURST
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live: spoof claims **within the recent window**, not the session
    /// total (#1271 f179).
    ///
    /// It used to threshold the cumulative counter, which never comes back
    /// down — so three rejected claims in the first minute of a session lit
    /// the badge for the rest of it, re-firing every 30 seconds into the log,
    /// and the row looked identical whether it was happening now or an hour
    /// ago. A `burst` is a rate; the counter's sampled history is what can
    /// express one, and the latch clears itself once the window empties.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx
            .metrics
            .counter_window_rise(names::NET_IDENTITY_SPOOFED_COUNT)?;
        Some(if n >= SPOOF_LIMIT {
            Verdict::violated(format!("{n} spoofed identity claims just now"))
        } else {
            Verdict::Clear
        })
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        let n = events
            .iter()
            .filter(|e| matches!(e.payload, EventPayload::PeerIdentitySpoofRejected { .. }))
            .count();
        if n as u64 >= SPOOF_LIMIT {
            vec![Verdict::violated(format!("{n} spoofed identity claims"))]
        } else {
            Vec::new()
        }
    }
}

// --- PeerProtocolMismatched -------------------------------------------------
struct PeerProtocolMismatched;
const PEER_PROTOCOL_MISMATCHED: RuleHeader = RuleHeader {
    id: "net.peer_protocol_mismatch",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "someone here is running a different version of Overlands — you may not see \
                  each other properly",
    technical: Some(
        "a peer announced a different wire-protocol version, so messages between the two \
         builds may not decode",
    ),
    when_state: None,
};
impl Rule for PeerProtocolMismatched {
    fn header(&self) -> &RuleHeader {
        &PEER_PROTOCOL_MISMATCHED
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Replay-only, and deliberately so: there is no live counter to read
    /// because the condition is not a rate. One incompatible peer in the room
    /// is the whole finding — a second one adds nothing to the diagnosis, and
    /// the emit sites already collapse a re-announcement to a single event.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::PeerProtocolMismatch {
                    peer, ours, theirs, ..
                } => Some(Verdict::violated(match theirs {
                    Some(t) => format!("peer {peer} speaks protocol {t}, we speak {ours}"),
                    None => format!("peer {peer} announced no protocol (we speak {ours})"),
                })),
                _ => None,
            })
            .collect()
    }
}

// --- WorldDigestMismatch ----------------------------------------------------
struct WorldDigestMismatch;
const WORLD_DIGEST_MISMATCH: RuleHeader = RuleHeader {
    id: "net.world_digest_mismatch",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "you and someone else here are seeing different versions of this world",
    technical: Some("two peers derived different world digests from the same stored record"),
    when_state: None,
};
impl Rule for WorldDigestMismatch {
    fn header(&self) -> &RuleHeader {
        &WORLD_DIGEST_MISMATCH
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// The rule the project has wanted twice (#51, #882) and could not have,
    /// because nothing computed a digest to disagree about. It fires on the
    /// event alone: the emit site has already established that both peers
    /// claim the same record and that our own world had settled, which is the
    /// entire condition — there is no rate or threshold to add on top, and one
    /// occurrence is the whole finding.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::PeerWorldDigestMismatch {
                    peer, ours, theirs, ..
                } => Some(Verdict::violated(format!(
                    "peer {peer} derived {theirs:016x} where we derived {ours:016x}"
                ))),
                _ => None,
            })
            .collect()
    }
}

// --- SilentDecodeFailure ----------------------------------------------------
struct SilentDecodeFailure;
const SILENT_DECODE_FAILURE: RuleHeader = RuleHeader {
    id: "net.silent_decode_failure",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a message from someone here could not be read, and was dropped",
    technical: Some("a peer payload failed to decode and was silently dropped"),
    when_state: None,
};
impl Rule for SilentDecodeFailure {
    fn header(&self) -> &RuleHeader {
        &SILENT_DECODE_FAILURE
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::AvatarStateDecodeFailed { .. } => {
                    Some(Verdict::violated("avatar-state decode failed"))
                }
                EventPayload::RoomStateDecodeFailed { .. } => {
                    Some(Verdict::violated("room-state decode failed"))
                }
                EventPayload::ItemOfferDecodeFailed { .. } => {
                    Some(Verdict::violated("item-offer decode failed"))
                }
                _ => None,
            })
            .collect()
    }
}

// --- GlareSuspected ---------------------------------------------------------
/// Sustained window (at the 1 Hz metric scrape, so ≈ samples == seconds) that
/// the `awaiting_peers` flag must stay raised before a stalled handshake is
/// flagged. A healthy WebRTC handshake completes in ~1–2 s, so this is well
/// clear of a normal connect while still catching a permanent stall promptly.
const GLARE_STALL_SAMPLES: usize = 10;
/// Replay budget (seconds) between a non-empty `peer_list` and the first
/// `PeerJoined`; kept equal to [`GLARE_STALL_SAMPLES`] so the offline verdict
/// matches the live one.
const GLARE_STALL_SECS: f64 = GLARE_STALL_SAMPLES as f64;

struct GlareSuspected;
const GLARE_SUSPECTED: RuleHeader = RuleHeader {
    id: "net.signal_glare_suspected",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "cannot reach the other people here — your network or a firewall may be \
                  blocking the connection",
    technical: Some(
        "the relay reported peers in this world but no WebRTC data channel ever opened — \
         offer glare, or ICE/NAT traversal failing",
    ),
    when_state: Some(AppState::InGame),
};
impl Rule for GlareSuspected {
    fn header(&self) -> &RuleHeader {
        &GLARE_SUSPECTED
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live: the `net.signal.awaiting_peers` gauge (set by the 1 Hz signal
    /// scrape) has stayed raised across the whole recent window — the relay
    /// reported peers yet none reached `Connected`.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let samples: Vec<f64> = cx
            .metrics
            .gauge(names::NET_SIGNAL_AWAITING_PEERS)?
            .iter()
            .collect();
        if samples.len() < GLARE_STALL_SAMPLES {
            return Some(Verdict::Clear);
        }
        let recent = &samples[samples.len() - GLARE_STALL_SAMPLES..];
        Some(if recent.iter().all(|&v| v >= 0.5) {
            Verdict::violated(format!(
                "relay reported peers but no WebRTC data channel opened for ~{GLARE_STALL_SAMPLES}s \
                 (offer glare or ICE/NAT failure)"
            ))
        } else {
            Verdict::Clear
        })
    }
    /// Replay: a non-empty `SocketPeerListReceived` with no `PeerJoined` within
    /// the budget (or none before the log ends) — the offline mirror of the
    /// live flag.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        stall_durations(
            events,
            |p| matches!(p, EventPayload::SocketPeerListReceived { count } if *count >= 1),
            |p| matches!(p, EventPayload::PeerJoined { .. }),
            GLARE_STALL_SECS,
        )
        .into_iter()
        .map(|d| {
            Verdict::violated(format!(
                "relay peer_list had peers but none connected within {d:.0}s"
            ))
        })
        .collect()
    }
}

// --- RelayConnectionRejected ------------------------------------------------
/// What to say about `n` relay refusals whose last status was `status`.
///
/// The client is entitled to two different sentences and only ever said one
/// (#1271 f400). A real 4xx is the relay rejecting the credential we
/// presented, and "sign in again" is the right advice. **`status == 0` is the
/// browser**: its WebSocket API hides the handshake status, so the upstream
/// signaller counts consecutive failures under one token fingerprint and, on
/// exhaustion, records a rejection with status `0` — which is exactly what a
/// dropped Wi-Fi connection produces. Reporting that as an expired sign-in
/// sends the user to fix their account when their laptop lost the network,
/// and biases the project's own reading of its failure rates. Shared by the
/// live and replay arms so the panel and the post-mortem cannot disagree.
fn reject_detail(n: u64, status: u64) -> String {
    let times = crate::ui::toolbar::plural(n as usize, "time", "times");
    if status == 0 {
        format!(
            "could not reach the world server {n} {times} — no reason given, \
             which is what a dropped connection looks like from a browser"
        )
    } else {
        format!("the world server refused our sign-in {n} {times} (status {status})")
    }
}

struct RelayConnectionRejected;
const RELAY_CONNECTION_REJECTED: RuleHeader = RuleHeader {
    id: "net.relay_connection_rejected",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "could not reach the world server — usually your connection, though a stale \
                  sign-in can also do it",
    technical: Some(
        "the relay connection was refused. A real 4xx status means it rejected our \
         service-auth token; in the browser the status is never visible, so an ordinary \
         outage exhausting the blind-retry budget arrives here too",
    ),
    when_state: None,
};
impl Rule for RelayConnectionRejected {
    fn header(&self) -> &RuleHeader {
        &RELAY_CONNECTION_REJECTED
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live: the `net.signal.auth_rejections` gauge ROSE across the recent
    /// window — the relay refused a connection just now. Unlike a stalled
    /// handshake this leaves no peer_list, so `GlareSuspected` cannot see it.
    ///
    /// Windowed, not cumulative (#1271 f179). The gauge is a running total,
    /// so `>= 1` was true forever after the first refusal: one rejected
    /// handshake at boot pinned the toolbar's alarm dot for the whole
    /// session, with no acknowledge control and no way to tell a resolved
    /// problem from a live one. The rise is the live fact and it falls back
    /// to zero on its own.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx
            .metrics
            .cumulative_window_rise(names::NET_SIGNAL_AUTH_REJECTIONS)?;
        Some(if n >= 1.0 {
            Verdict::violated(reject_detail(
                n as u64,
                cx.metrics
                    .gauge_latest(names::NET_SIGNAL_LAST_REJECT_STATUS)
                    .unwrap_or(0.0) as u64,
            ))
        } else {
            Verdict::Clear
        })
    }
    /// Replay: one verdict per logged rejection, splitting on the same
    /// status the live body reads.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::RelayAuthRejected { status, .. } => {
                    Some(Verdict::violated(reject_detail(1, *status)))
                }
                _ => None,
            })
            .collect()
    }
}

// --- RelayTokenRefreshFailing -----------------------------------------------
struct RelayTokenRefreshFailing;
const RELAY_TOKEN_REFRESH_FAILING: RuleHeader = RuleHeader {
    id: "net.relay_token_refresh_failing",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "your sign-in for the world server cannot be renewed — reconnecting will keep \
                  failing until you sign in again",
    technical: Some(
        "the relay service-auth token could not be re-minted several times running, so every \
         reconnect from here presents a stale credential",
    ),
    when_state: None,
};
impl Rule for RelayTokenRefreshFailing {
    fn header(&self) -> &RuleHeader {
        &RELAY_TOKEN_REFRESH_FAILING
    }
    fn has_live_body(&self) -> bool {
        true
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live: the consecutive-failure gauge has reached the alarm threshold.
    /// The counterpart of [`RelayConnectionRejected`] on the other side of
    /// the handshake — that one fires when the relay refuses the credential
    /// we present, this one when we cannot mint one to present. A client in
    /// this state looks healthy on every other gauge.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx
            .metrics
            .gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES)?
            .last();
        Some(
            if n >= crate::config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM as f64 {
                Verdict::violated(format!(
                    "{n:.0} consecutive relay service-auth token refresh failures — \
                     reconnects will present a stale credential"
                ))
            } else {
                Verdict::Clear
            },
        )
    }
    /// Replay: one verdict per logged failure that reached the threshold, so
    /// a post-mortem sees the streak rather than every retry in it.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::ServiceTokenRefreshFailed {
                    reason,
                    consecutive,
                } if *consecutive
                    >= crate::config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM =>
                {
                    Some(Verdict::violated(format!(
                        "relay service-auth token refresh failed {consecutive}x: {reason}"
                    )))
                }
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: f64, payload: EventPayload) -> SessionEvent {
        SessionEvent::new(0, t, None, Severity::Info, payload)
    }

    /// A settled in-game context over `metrics` — the shape the network
    /// rules' live bodies read.
    fn ctx_with(metrics: &crate::diagnostics::MetricsRegistry) -> LiveCtx<'_> {
        LiveCtx {
            now_secs: 1.0,
            state: AppState::InGame,
            metrics,
            loading_elapsed_secs: None,
            ingame_elapsed_secs: Some(60.0),
            player_y: None,
            ground_y: None,
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
            colliders_seen_ingame: false,
            oldest_pending_job: None,
        }
    }

    /// #1143. The live half of the one rule written for "a worker job never
    /// answered". Before the offload census it had no `eval` body at all, and
    /// its replay body had nothing to fold either — `OffloadJobStarted` was a
    /// schema variant with zero emit sites in the crate. The failure it exists
    /// for is a wasm deploy whose `gen-worker.js` is stale or missing: every
    /// job is dispatched, none ever answers, and the log reads HEALTHY.
    #[test]
    fn task_never_resolves_fires_live_on_a_job_that_stopped_answering() {
        use crate::diagnostics::MetricsRegistry;

        fn ctx<'a>(metrics: &'a MetricsRegistry, job: Option<(&'a str, f64)>) -> LiveCtx<'a> {
            LiveCtx {
                now_secs: 120.0,
                state: AppState::InGame,
                metrics,
                loading_elapsed_secs: None,
                ingame_elapsed_secs: Some(120.0),
                player_y: None,
                ground_y: None,
                nan_body_count: 0,
                orphan_avatar_count: 0,
                respawns_recent: 0,
                colliders_seen_ingame: false,
                oldest_pending_job: job,
            }
        }
        let metrics = MetricsRegistry::default();

        let stuck = TaskNeverResolves
            .eval(&ctx(
                &metrics,
                Some(("avatar_build", TASK_TIMEOUT_SECS + 30.0)),
            ))
            .expect("the rule has a live body now");
        assert!(stuck.is_violated());

        // A job still inside its budget, and a session with nothing
        // dispatched, must both stay quiet — a watchdog that cries at every
        // heightmap bake is one nobody reads.
        assert_eq!(
            TaskNeverResolves.eval(&ctx(&metrics, Some(("heightmap", 1.0)))),
            Some(Verdict::Clear)
        );
        assert_eq!(TaskNeverResolves.eval(&ctx(&metrics, None)), None);
    }

    #[test]
    fn gate_stall_replay_flags_a_long_gate() {
        let over = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(
                200.0,
                EventPayload::LoadingGateTransitionToInGame {
                    elapsed_secs: 200.0,
                },
            ),
        ];
        assert_eq!(LoadingGateStall.replay(&over).len(), 1);
        let ok = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(
                5.0,
                EventPayload::LoadingGateTransitionToInGame { elapsed_secs: 5.0 },
            ),
        ];
        assert!(LoadingGateStall.replay(&ok).is_empty());
        // Never transitioned but log continued past budget → stall.
        let never = vec![
            ev(0.0, EventPayload::LoadingPhaseStarted),
            ev(
                200.0,
                EventPayload::RoomStateApplied {
                    bytes: 4096,
                    digest_of_record: 0xfeed,
                },
            ),
        ];
        assert_eq!(LoadingGateStall.replay(&never).len(), 1);
    }

    #[test]
    fn task_never_resolves_pairs_by_job_name() {
        let events = vec![
            ev(
                0.0,
                EventPayload::OffloadJobStarted {
                    job: "heightmap".into(),
                },
            ),
            ev(
                1.0,
                EventPayload::OffloadJobStarted {
                    job: "ambient".into(),
                },
            ),
            ev(
                2.0,
                EventPayload::OffloadJobCompleted {
                    job: "heightmap".into(),
                    duration_secs: 2.0,
                },
            ),
            ev(
                100.0,
                EventPayload::RoomStateApplied {
                    bytes: 4096,
                    digest_of_record: 0xfeed,
                },
            ),
        ];
        // ambient never resolved and the log ran 100s past its start.
        let v = TaskNeverResolves.replay(&events);
        assert_eq!(v.len(), 1);
        assert!(matches!(&v[0], Verdict::Violated { detail } if detail.contains("ambient")));
    }

    #[test]
    fn offer_ratio_flags_extremes_only_above_min_count() {
        // 12 offers, all accepted → ratio 1.0 → anomalous.
        let all_accept: Vec<_> = (0..12)
            .map(|i| {
                ev(
                    i as f64,
                    EventPayload::ItemOfferUserResponded {
                        offer_id: i,
                        accepted: true,
                    },
                )
            })
            .collect();
        assert_eq!(OfferAcceptanceAnomaly.replay(&all_accept).len(), 1);
        // Too few offers → no verdict even if all accepted.
        let few: Vec<_> = (0..3)
            .map(|i| {
                ev(
                    i as f64,
                    EventPayload::ItemOfferUserResponded {
                        offer_id: i,
                        accepted: true,
                    },
                )
            })
            .collect();
        assert!(OfferAcceptanceAnomaly.replay(&few).is_empty());
    }

    #[test]
    fn spoof_and_decode_rules_fire_from_events() {
        let spoofs: Vec<_> = (0..3)
            .map(|_| {
                ev(
                    0.0,
                    EventPayload::PeerIdentitySpoofRejected {
                        peer: "p".into(),
                        claimed_did: "a".into(),
                        authenticated_did: "b".into(),
                    },
                )
            })
            .collect();
        assert_eq!(IdentitySpoofBurst.replay(&spoofs).len(), 1);

        let decode = vec![ev(
            0.0,
            EventPayload::RoomStateDecodeFailed {
                sender_did: "d".into(),
                error: "bad".into(),
            },
        )];
        assert_eq!(SilentDecodeFailure.replay(&decode).len(), 1);
    }

    #[test]
    fn glare_suspected_live_fires_on_sustained_awaiting() {
        use crate::diagnostics::MetricsRegistry;

        // Rebuild the context per call so we never hold a borrow across a mutate.
        fn ctx(metrics: &MetricsRegistry) -> LiveCtx<'_> {
            LiveCtx {
                now_secs: 20.0,
                state: AppState::InGame,
                metrics,
                loading_elapsed_secs: None,
                ingame_elapsed_secs: Some(60.0),
                player_y: None,
                ground_y: None,
                nan_body_count: 0,
                orphan_avatar_count: 0,
                respawns_recent: 0,
                colliders_seen_ingame: false,
                oldest_pending_job: None,
            }
        }

        let mut metrics = MetricsRegistry::default();
        // The relay reported peers but none connected, sustained for the window.
        for _ in 0..GLARE_STALL_SAMPLES {
            metrics.observe_gauge(names::NET_SIGNAL_AWAITING_PEERS, 1.0);
        }
        assert!(GlareSuspected.eval(&ctx(&metrics)).unwrap().is_violated());

        // A subsequent connect drops the flag → the newest sample clears it.
        metrics.observe_gauge(names::NET_SIGNAL_AWAITING_PEERS, 0.0);
        assert_eq!(GlareSuspected.eval(&ctx(&metrics)), Some(Verdict::Clear));

        // Too little history yet → not enough evidence to fire.
        let mut fresh = MetricsRegistry::default();
        fresh.observe_gauge(names::NET_SIGNAL_AWAITING_PEERS, 1.0);
        assert_eq!(GlareSuspected.eval(&ctx(&fresh)), Some(Verdict::Clear));
    }

    #[test]
    fn glare_suspected_replay_flags_peer_list_with_no_join() {
        // A peer_list named a peer, but no PeerJoined ever arrived and the log
        // ran well past the budget → stall.
        let stalled = vec![
            ev(0.0, EventPayload::SocketPeerListReceived { count: 1 }),
            ev(
                20.0,
                EventPayload::RoomStateApplied {
                    bytes: 4096,
                    digest_of_record: 0xfeed,
                },
            ),
        ];
        assert_eq!(GlareSuspected.replay(&stalled).len(), 1);

        // A join within budget → healthy, no verdict.
        let ok = vec![
            ev(0.0, EventPayload::SocketPeerListReceived { count: 1 }),
            ev(2.0, EventPayload::PeerJoined { peer: "p".into() }),
        ];
        assert!(GlareSuspected.replay(&ok).is_empty());

        // An empty peer_list is never a glare candidate.
        let alone = vec![
            ev(0.0, EventPayload::SocketPeerListReceived { count: 0 }),
            ev(
                20.0,
                EventPayload::RoomStateApplied {
                    bytes: 4096,
                    digest_of_record: 0xfeed,
                },
            ),
        ];
        assert!(GlareSuspected.replay(&alone).is_empty());
    }

    #[test]
    fn relay_connection_rejected_fires_live_and_replay() {
        use crate::diagnostics::MetricsRegistry;
        // Replay: one verdict per logged rejection.
        let events = vec![
            ev(
                1.0,
                EventPayload::RelayAuthRejected {
                    status: 401,
                    total: 1,
                },
            ),
            ev(
                2.0,
                EventPayload::RelayAuthRejected {
                    status: 0,
                    total: 2,
                },
            ),
        ];
        assert_eq!(RelayConnectionRejected.replay(&events).len(), 2);
        assert!(RelayConnectionRejected.replay(&[]).is_empty());

        // Live: fires once the cumulative rejection gauge is non-zero.
        fn ctx(metrics: &MetricsRegistry) -> LiveCtx<'_> {
            LiveCtx {
                now_secs: 1.0,
                state: AppState::InGame,
                metrics,
                loading_elapsed_secs: None,
                ingame_elapsed_secs: Some(60.0),
                player_y: None,
                ground_y: None,
                nan_body_count: 0,
                orphan_avatar_count: 0,
                respawns_recent: 0,
                colliders_seen_ingame: false,
                oldest_pending_job: None,
            }
        }
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge(names::NET_SIGNAL_AUTH_REJECTIONS, 0.0);
        assert_eq!(
            RelayConnectionRejected.eval(&ctx(&metrics)),
            Some(Verdict::Clear)
        );
        metrics.observe_gauge(names::NET_SIGNAL_AUTH_REJECTIONS, 2.0);
        assert!(
            RelayConnectionRejected
                .eval(&ctx(&metrics))
                .unwrap()
                .is_violated()
        );
    }

    /// #1271 f179. One relay refusal at boot, then a healthy session: the
    /// badge has to go out on its own, because there is no acknowledge
    /// control and a permanently-lit alarm is an ignored alarm.
    ///
    /// The control is the old condition, asserted on the same registry —
    /// a rule that cannot see what it replaced passes forever.
    #[test]
    fn a_boot_time_relay_refusal_stops_lighting_the_badge() {
        use crate::diagnostics::MetricsRegistry;
        use crate::diagnostics::registry::RING_CAP;

        let mut metrics = MetricsRegistry::default();
        // The refusal lands before the very first 1 Hz scrape, which is
        // the case a plain newest-minus-oldest window would lose entirely.
        metrics.observe_gauge(names::NET_SIGNAL_AUTH_REJECTIONS, 1.0);
        assert!(
            RelayConnectionRejected
                .eval(&ctx_with(&metrics))
                .unwrap()
                .is_violated(),
            "the rising edge still has to fire"
        );

        // A quiet session: the total never moves again.
        for _ in 0..RING_CAP {
            metrics.observe_gauge(names::NET_SIGNAL_AUTH_REJECTIONS, 1.0);
        }
        assert_eq!(
            RelayConnectionRejected.eval(&ctx_with(&metrics)),
            Some(Verdict::Clear),
            "the latch has to clear itself once the window has passed"
        );
        assert!(
            metrics
                .gauge_latest(names::NET_SIGNAL_AUTH_REJECTIONS)
                .unwrap()
                >= 1.0,
            "the control: the cumulative test this replaced is still true, \
             and would still be lighting the toolbar dot"
        );
    }

    /// The same shape over the spoof COUNTER, which had no history at all
    /// until #1271 gave `Counter` one — and whose `Interval(30.0)` debounce
    /// meant the latch also re-logged itself every 30 s forever.
    #[test]
    fn a_boot_time_spoof_burst_stops_lighting_the_badge() {
        use crate::diagnostics::MetricsRegistry;
        use crate::diagnostics::registry::RING_CAP;

        let mut metrics = MetricsRegistry::default();
        metrics.incr_by(names::NET_IDENTITY_SPOOFED_COUNT, SPOOF_LIMIT);
        metrics.sample_counters();
        assert!(
            IdentitySpoofBurst
                .eval(&ctx_with(&metrics))
                .unwrap()
                .is_violated()
        );

        for _ in 0..RING_CAP {
            metrics.sample_counters();
        }
        assert_eq!(
            IdentitySpoofBurst.eval(&ctx_with(&metrics)),
            Some(Verdict::Clear)
        );
        assert!(
            metrics.counter_value(names::NET_IDENTITY_SPOOFED_COUNT) >= SPOOF_LIMIT,
            "the control: the session total is untouched"
        );

        // And a fresh burst inside the window fires again.
        metrics.incr_by(names::NET_IDENTITY_SPOOFED_COUNT, SPOOF_LIMIT);
        metrics.sample_counters();
        assert!(
            IdentitySpoofBurst
                .eval(&ctx_with(&metrics))
                .unwrap()
                .is_violated()
        );
    }

    /// #1271 f400. `status == 0` is every wasm rejection, and wasm is the
    /// deployed target — so the sentence it produces must not name a cause
    /// the client cannot know.
    #[test]
    fn a_status_free_refusal_is_not_reported_as_a_stale_sign_in() {
        let blind = reject_detail(1, 0);
        assert!(blind.contains("could not reach"), "{blind}");
        assert!(
            !blind.contains("sign-in") && !blind.contains("refused our"),
            "a browser cannot know it was the credential: {blind}"
        );

        let refused = reject_detail(2, 401);
        assert!(refused.contains("refused our sign-in"), "{refused}");
        assert!(refused.contains("401"), "{refused}");
        assert!(refused.contains("2 times"), "{refused}");
    }

    /// THE SEQUENCE: the PDS refuses `getServiceAuth` several ticks running.
    /// `RelayConnectionRejected` cannot see this — it counts what the relay
    /// REFUSES, and a client that never mints a token never presents one, so
    /// it looked healthy on every gauge (#1215 f403). One transient failure
    /// must stay quiet; a streak past the threshold must not.
    #[test]
    fn relay_token_refresh_failing_fires_live_and_replay() {
        use crate::diagnostics::MetricsRegistry;

        fn ctx(metrics: &MetricsRegistry) -> LiveCtx<'_> {
            LiveCtx {
                now_secs: 1.0,
                state: AppState::InGame,
                metrics,
                loading_elapsed_secs: None,
                ingame_elapsed_secs: Some(60.0),
                player_y: None,
                ground_y: None,
                nan_body_count: 0,
                orphan_avatar_count: 0,
                respawns_recent: 0,
                colliders_seen_ingame: false,
                oldest_pending_job: None,
            }
        }
        let alarm = crate::config::network::SERVICE_TOKEN_FAILURES_BEFORE_ALARM;
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES, 1.0);
        assert_eq!(
            RelayTokenRefreshFailing.eval(&ctx(&metrics)),
            Some(Verdict::Clear),
            "one hiccup is not a broken credential"
        );
        metrics.observe_gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES, alarm as f64);
        assert!(
            RelayTokenRefreshFailing
                .eval(&ctx(&metrics))
                .unwrap()
                .is_violated()
        );
        // A recovery clears it, so the session's verdict tracks the client's
        // actual state rather than latching on the worst moment.
        metrics.observe_gauge(names::NET_RELAY_TOKEN_REFRESH_FAILURES, 0.0);
        assert_eq!(
            RelayTokenRefreshFailing.eval(&ctx(&metrics)),
            Some(Verdict::Clear)
        );

        let events: Vec<SessionEvent> = (1..=alarm)
            .map(|n| {
                ev(
                    n as f64,
                    EventPayload::ServiceTokenRefreshFailed {
                        reason: "HTTP 400".into(),
                        consecutive: n,
                    },
                )
            })
            .collect();
        assert_eq!(
            RelayTokenRefreshFailing.replay(&events).len(),
            1,
            "the streak reports once at the threshold, not once per retry"
        );
        assert!(RelayTokenRefreshFailing.replay(&[]).is_empty());
    }
}
