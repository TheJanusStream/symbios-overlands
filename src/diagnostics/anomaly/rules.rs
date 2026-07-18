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
    reg.register(SilentDecodeFailure);
    reg.register(GlareSuspected);
    reg.register(RelayConnectionRejected);
    // D-3 ECS-state (live-only) rules.
    super::rules_ecs::register_ecs_rules(reg);
}

/// The last event's timestamp (session end), for "never resolved" checks.
fn last_ts(events: &[SessionEvent]) -> f64 {
    events.last().map(|e| e.t_mono_secs).unwrap_or(0.0)
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
    description: "loading gate exceeded its time budget",
    when_state: Some(AppState::Loading),
};
impl Rule for LoadingGateStall {
    fn header(&self) -> &RuleHeader {
        &LOADING_GATE_STALL
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
    description: "a PDS record fetch exhausted its retry budget",
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
    description: "ambient audio bake did not finish within its budget",
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
const TASK_TIMEOUT_SECS: f64 = 60.0;

struct TaskNeverResolves;
const TASK_NEVER_RESOLVES: RuleHeader = RuleHeader {
    id: "offload.task_never_resolves",
    subsystem: Subsystem::Offload,
    severity: Severity::Critical,
    debounce: DebouncePolicy::OncePerCondition,
    description: "an offloaded job never reported completion or failure",
    when_state: None,
};
impl Rule for TaskNeverResolves {
    fn header(&self) -> &RuleHeader {
        &TASK_NEVER_RESOLVES
    }
    fn is_replayable(&self) -> bool {
        true
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
    description: "an unusual burst of peers leaving in a short window",
    when_state: None,
};
impl Rule for PeerChurnSpike {
    fn header(&self) -> &RuleHeader {
        &PEER_CHURN_SPIKE
    }
    fn is_replayable(&self) -> bool {
        true
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
    description: "offer accept/decline ratio is extreme (possible automation)",
    when_state: None,
};
impl Rule for OfferAcceptanceAnomaly {
    fn header(&self) -> &RuleHeader {
        &OFFER_ACCEPTANCE_ANOMALY
    }
    fn is_replayable(&self) -> bool {
        true
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
    description: "repeated spoofed identity claims from peers",
    when_state: None,
};
impl Rule for IdentitySpoofBurst {
    fn header(&self) -> &RuleHeader {
        &IDENTITY_SPOOF_BURST
    }
    fn is_replayable(&self) -> bool {
        true
    }
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx
            .metrics
            .counter(names::NET_IDENTITY_SPOOFED_COUNT)?
            .value();
        Some(if n >= SPOOF_LIMIT {
            Verdict::violated(format!("{n} spoofed identity claims"))
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

// --- SilentDecodeFailure ----------------------------------------------------
struct SilentDecodeFailure;
const SILENT_DECODE_FAILURE: RuleHeader = RuleHeader {
    id: "net.silent_decode_failure",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "a peer payload failed to decode and was silently dropped",
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
    description: "relay reported peers in the room but no WebRTC data channel opened \
                  (offer glare or ICE/NAT failure)",
    when_state: Some(AppState::InGame),
};
impl Rule for GlareSuspected {
    fn header(&self) -> &RuleHeader {
        &GLARE_SUSPECTED
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
struct RelayConnectionRejected;
const RELAY_CONNECTION_REJECTED: RuleHeader = RuleHeader {
    id: "net.relay_connection_rejected",
    subsystem: Subsystem::Network,
    severity: Severity::Error,
    debounce: DebouncePolicy::OncePerCondition,
    description: "the relay refused our WebSocket handshake (auth 401 / HTTP 4xx) — \
                  most often a stale/expired service-auth token",
    when_state: None,
};
impl Rule for RelayConnectionRejected {
    fn header(&self) -> &RuleHeader {
        &RELAY_CONNECTION_REJECTED
    }
    fn is_replayable(&self) -> bool {
        true
    }
    /// Live: the cumulative `net.signal.auth_rejections` gauge is non-zero — the
    /// relay refused at least one (re)connect this session. Unlike a stalled
    /// handshake this leaves no peer_list, so `GlareSuspected` cannot see it.
    fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
        let n = cx.metrics.gauge(names::NET_SIGNAL_AUTH_REJECTIONS)?.last();
        Some(if n >= 1.0 {
            Verdict::violated(format!("{n:.0} relay handshake rejection(s) this session"))
        } else {
            Verdict::Clear
        })
    }
    /// Replay: one verdict per logged rejection.
    fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
        events
            .iter()
            .filter_map(|e| match &e.payload {
                EventPayload::RelayAuthRejected { status, .. } => {
                    Some(Verdict::violated(if *status == 0 {
                        "relay refused handshake (auth)".to_string()
                    } else {
                        format!("relay refused handshake (HTTP {status})")
                    }))
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
            ev(200.0, EventPayload::RoomStateApplied),
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
            ev(100.0, EventPayload::RoomStateApplied),
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
            ev(20.0, EventPayload::RoomStateApplied),
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
            ev(20.0, EventPayload::RoomStateApplied),
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
}
