//! Core anomaly-rule vocabulary (Pillar D-0) — the shared representation the
//! live engine (D-4), the GUI badges (D-6) and the offline analyzer (D-5) all
//! build on.
//!
//! A single [`Rule`] trait carries a declarative [`RuleHeader`] plus up to two
//! evaluator bodies with a common [`Verdict`]: a LIVE body ([`Rule::eval`])
//! reading a per-tick [`LiveCtx`], and a REPLAY body ([`Rule::replay`]) folding
//! a captured event log. A rule may implement one or both — the default impls
//! make the other a no-op — so one definition runs live AND replays offline
//! from a single source of truth (the parity guarantee).
//!
//! Severity reuses the suite-wide [`Severity`] so a rule's severity maps
//! directly onto the `InvariantViolation` event it logs and the GUI badge
//! colour it drives.

use crate::diagnostics::MetricsRegistry;
use crate::diagnostics::event::{SessionEvent, Severity, Subsystem};
use crate::state::AppState;

/// Stable identifier for a rule — also its badge/label key.
pub type RuleId = &'static str;

/// The outcome of evaluating a rule once.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// The invariant holds.
    Clear,
    /// The invariant is violated, with a human-readable detail.
    Violated { detail: String },
}

impl Verdict {
    /// Convenience for a violation with a formatted detail.
    pub fn violated(detail: impl Into<String>) -> Verdict {
        Verdict::Violated {
            detail: detail.into(),
        }
    }

    pub fn is_violated(&self) -> bool {
        matches!(self, Verdict::Violated { .. })
    }
}

/// How often a persistently-violated rule re-fires.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebouncePolicy {
    /// Fire once on the rising edge; re-arm only after a `Clear`.
    OncePerCondition,
    /// Fire, then re-fire at most every `n` seconds while still violated.
    Interval(f32),
    /// Fire on every evaluation that is `Violated` (use sparingly — noisy).
    EveryEval,
}

/// The declarative part of a rule: identity, classification and firing policy.
#[derive(Clone, Debug)]
pub struct RuleHeader {
    pub id: RuleId,
    pub subsystem: Subsystem,
    pub severity: Severity,
    pub debounce: DebouncePolicy,
    pub description: &'static str,
    /// Only evaluate the live body while in this state (`None` = always).
    pub when_state: Option<AppState>,
}

/// Read-only per-tick context passed to [`Rule::eval`]. Metric-threshold rules
/// read the shared [`MetricsRegistry`]; ECS-state rules read the scalars the
/// tick system pre-gathers here, so no rule ever touches the `World` directly
/// (keeping rule bodies pure and unit-testable).
pub struct LiveCtx<'a> {
    pub now_secs: f64,
    pub state: AppState,
    pub metrics: &'a MetricsRegistry,
    /// Seconds spent in `Loading` so far — `Some` only while loading.
    pub loading_elapsed_secs: Option<f64>,
    /// Seconds spent in `InGame` so far — `Some` only in-game (#869).
    /// Grace-gates rules whose 1 Hz gauge samples can predate the world
    /// finishing its spawn on the entry frame.
    pub ingame_elapsed_secs: Option<f64>,
    /// Local player world-Y and the terrain height beneath it, when known.
    pub player_y: Option<f32>,
    pub ground_y: Option<f32>,
    /// Dynamic physics bodies with a non-finite position/rotation/velocity.
    pub nan_body_count: usize,
    /// Avatar-visual entities orphaned from any chassis.
    pub orphan_avatar_count: usize,
    /// Respawns observed in the recent window (for thrash detection).
    pub respawns_recent: u32,
}

/// A diagnostic invariant. Implement [`eval`](Rule::eval) for live detection
/// and/or [`replay`](Rule::replay) for offline detection over a captured log;
/// the default impls make the unimplemented side a no-op.
pub trait Rule: Send + Sync {
    fn header(&self) -> &RuleHeader;

    /// Live evaluation over the per-tick context. `None` means "no live body"
    /// (a replay-only rule); `Some(Verdict::Clear)` means evaluated-and-ok.
    fn eval(&self, _cx: &LiveCtx) -> Option<Verdict> {
        None
    }

    /// Offline evaluation over the whole captured event log. Empty means "no
    /// replay body" (a live-only rule).
    fn replay(&self, _events: &[SessionEvent]) -> Vec<Verdict> {
        Vec::new()
    }

    /// Whether this rule carries a [`replay`](Rule::replay) body — i.e. its
    /// violations can be re-derived offline from the event stream. Defaults to
    /// `false` (a live-only rule); override to `true` alongside a real `replay`
    /// impl. The offline analyzer (D-5) uses this to tell a re-derivable rule
    /// from a live-only one, whose fires it can only *surface* from the captured
    /// `InvariantViolation` events rather than re-derive.
    fn is_replayable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::EventPayload;

    /// A trivial rule implementing BOTH bodies, to exercise the trait surface.
    struct EntitySpikeToy;
    const TOY_HEADER: RuleHeader = RuleHeader {
        id: "toy.entity_spike",
        subsystem: Subsystem::Runtime,
        severity: Severity::Warn,
        debounce: DebouncePolicy::OncePerCondition,
        description: "entity count over 10",
        when_state: None,
    };
    impl Rule for EntitySpikeToy {
        fn header(&self) -> &RuleHeader {
            &TOY_HEADER
        }
        fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
            let n = cx.metrics.gauge("runtime.entity.count")?.last();
            Some(if n > 10.0 {
                Verdict::violated(format!("{n} entities"))
            } else {
                Verdict::Clear
            })
        }
        fn replay(&self, events: &[SessionEvent]) -> Vec<Verdict> {
            events
                .iter()
                .filter(|e| matches!(&e.payload, EventPayload::SessionEnd { reason } if reason == "spike"))
                .map(|_| Verdict::violated("logged spike"))
                .collect()
        }
    }

    #[test]
    fn rule_eval_reads_metrics_and_replay_folds_events() {
        let toy = EntitySpikeToy;
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge("runtime.entity.count", 12.0);
        let cx = LiveCtx {
            now_secs: 1.0,
            state: AppState::InGame,
            metrics: &metrics,
            loading_elapsed_secs: None,
            ingame_elapsed_secs: Some(60.0),
            player_y: None,
            ground_y: None,
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
        };
        assert_eq!(toy.eval(&cx), Some(Verdict::violated("12 entities")));

        let events = vec![SessionEvent::new(
            0,
            0.0,
            None,
            Severity::Info,
            EventPayload::SessionEnd {
                reason: "spike".into(),
            },
        )];
        assert_eq!(toy.replay(&events).len(), 1);

        // Header wiring is intact.
        assert_eq!(toy.header().id, "toy.entity_spike");
    }
}
