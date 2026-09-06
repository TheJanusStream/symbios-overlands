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
    /// The badge's FACE text, and the only sentence most people will
    /// ever read about this rule (#1271 f409).
    ///
    /// It is UI copy: `ui::diagnostics` renders it verbatim in the Active
    /// Anomalies strip and beside every per-metric pill, and
    /// `ui::fonts::glyph_coverage_tests::rule_prose_is_ui_copy` holds it to
    /// the same product vocabulary as every other label in the app. Write
    /// what has gone wrong for the person reading it and, where there is
    /// one, what to try. The mechanism goes in [`technical`](Self::technical).
    pub description: &'static str,
    /// The precise statement of the condition, for the hover.
    ///
    /// `None` when [`description`](Self::description) already says it
    /// exactly — a rule like "you keep being put back at the start" has no
    /// second layer to peel. Where the two differ, this is the half that
    /// may name a threshold, a metric or a subsystem; it is still read by
    /// a person, so it stays clear of the product's own vocabulary rules
    /// (no "PDS", the place is a "world"), and the raw numbers belong in
    /// the verdict detail rather than here.
    pub technical: Option<&'static str>,
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
    /// Whether a physics collider has been observed at any point since the
    /// **current** `InGame` entry (#922). The collider gauge's ring cannot
    /// answer this — it spans state transitions, so at session start it
    /// still holds the boot/attract world's colliders, and "seen then
    /// lost" read from the ring mistakes the boot→room handover for an
    /// in-game vanish. Maintained by the tick system, reset each time the
    /// loading gate opens.
    pub colliders_seen_ingame: bool,
    /// The longest-waiting in-flight offload job and its age in seconds
    /// (#1143), or `None` when nothing is dispatched. Fed from
    /// [`OffloadWatch`](crate::diagnostics::offload_watch::OffloadWatch) so
    /// the rule stays pure over its inputs rather than reaching into the
    /// process-global census.
    pub oldest_pending_job: Option<(&'a str, f64)>,
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

    /// Whether this rule carries an [`eval`](Rule::eval) body — i.e. it can
    /// ever be violated LIVE, and so can ever badge a row in the HUD.
    /// Defaults to `false`; override to `true` alongside a real `eval` impl.
    ///
    /// The mirror of [`is_replayable`](Rule::is_replayable), and it exists
    /// for the same reason turned inside out (#1272 f173). `eval`'s own
    /// contract already says `None` means "no live body", but that answer
    /// only arrives when there is a `LiveCtx` to pass — and the thing that
    /// needed to know was `ui::diagnostics`' `METRIC_RULE_TABLE`, which is a
    /// `const` mapping metric rows to rules. Five of its fourteen rows were
    /// mapped to rules that could never light, so those rows read as
    /// "checked and healthy" while nothing checked them.
    ///
    /// `rule::live_bodies_are_declared` pins this against the real `eval`
    /// bodies in both directions.
    fn has_live_body(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::EventPayload;
    use crate::diagnostics::names;
    use crate::diagnostics::registry::MetricKind;

    /// A `LiveCtx` over `metrics` with every optional reading PRESENT, so
    /// a rule that returns `None` under it is returning `None` because it
    /// has no live body — not because an input was missing.
    ///
    /// This is what makes [`live_bodies_are_declared`] a real check rather
    /// than a tautology: handed an empty context, half the live-bodied
    /// rules answer `None` too, and the guard would pass while proving
    /// nothing.
    pub(crate) fn settled_ctx(metrics: &MetricsRegistry) -> LiveCtx<'_> {
        LiveCtx {
            now_secs: 600.0,
            state: AppState::InGame,
            metrics,
            loading_elapsed_secs: Some(5.0),
            ingame_elapsed_secs: Some(600.0),
            player_y: Some(10.0),
            ground_y: Some(9.5),
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
            colliders_seen_ingame: true,
            oldest_pending_job: Some(("heightmap", 0.5)),
        }
    }

    /// Every named metric observed enough times that a rule reading a
    /// WINDOW (mesh-handle growth, the rebuild-delta rules, the glare
    /// flag's sustained samples) has one to read.
    pub(crate) fn warmed_metrics() -> MetricsRegistry {
        let mut m = MetricsRegistry::default();
        m.preseed(names::ALL);
        for i in 0..16 {
            for (name, kind) in names::ALL {
                match kind {
                    MetricKind::Gauge => m.observe_gauge(name, 1.0),
                    MetricKind::Histogram => m.observe_hist(name, 1.0),
                    MetricKind::Counter => m.incr(name),
                }
            }
            m.sample_counters();
            let _ = i;
        }
        m
    }

    /// `has_live_body()` must agree with whether `eval` actually answers,
    /// in BOTH directions (#1272 f173) — the same two-way pinning
    /// `replay::replayable_rule_set_is_pinned` gives the replay side.
    ///
    /// A rule that gains an `eval` and forgets the override goes on
    /// reading as "analyzer only" in the panel; one that declares a live
    /// body it does not have puts an unreachable dot back on a row.
    #[test]
    fn live_bodies_are_declared() {
        let metrics = warmed_metrics();
        let cx = settled_ctx(&metrics);
        let registry = crate::diagnostics::anomaly::default_registry();
        let mut wrong = Vec::new();
        for rule in registry.rules() {
            let declared = rule.has_live_body();
            let answers = rule.eval(&cx).is_some();
            if declared != answers {
                wrong.push(format!(
                    "{}: has_live_body() = {declared} but eval() {} under a settled context",
                    rule.header().id,
                    if answers { "answered" } else { "returned None" }
                ));
            }
        }
        assert!(
            registry.rules().len() > 20,
            "the registry handed back {} rules",
            registry.rules().len()
        );
        assert!(wrong.is_empty(), "{}", wrong.join("\n  "));

        // Controls, both ways round. A guard that cannot see either
        // mistake passes forever.
        struct DeclaresButHasNone;
        impl Rule for DeclaresButHasNone {
            fn header(&self) -> &RuleHeader {
                &TOY_HEADER
            }
            fn has_live_body(&self) -> bool {
                true
            }
        }
        assert!(DeclaresButHasNone.has_live_body());
        assert!(
            DeclaresButHasNone.eval(&cx).is_none(),
            "an unreachable dot back on a row is what this direction catches"
        );
        // `EntitySpikeToy` below has a real `eval` and does NOT override,
        // which is the forgot-the-override direction.
        assert!(!EntitySpikeToy.has_live_body());
        assert!(EntitySpikeToy.eval(&cx).is_some());
    }

    /// A trivial rule implementing BOTH bodies, to exercise the trait surface.
    struct EntitySpikeToy;
    const TOY_HEADER: RuleHeader = RuleHeader {
        id: "toy.entity_spike",
        subsystem: Subsystem::Runtime,
        severity: Severity::Warn,
        debounce: DebouncePolicy::OncePerCondition,
        description: "entity count over 10",
        technical: None,
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
            colliders_seen_ingame: false,
            oldest_pending_job: None,
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
