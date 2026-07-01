//! Invariant registry + debounce ledger (Pillar D-1).
//!
//! [`InvariantRegistry`] owns the built-in [`Rule`] set and a per-rule
//! [`RuleRuntimeState`] that serves double duty: the debounce ledger (so a
//! persistently-violated rule fires once, not every tick) *and* the GUI badge
//! source (which rules are currently violated, at what severity). One
//! [`default_registry`] constructor is shared by the live plugin (D-4) and the
//! offline analyzer (D-5), so the rule set is byte-identical in both.

use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::diagnostics::anomaly::rule::{DebouncePolicy, Rule, RuleId, Verdict};
use crate::diagnostics::event::{Severity, Subsystem};

/// Per-rule runtime state — the debounce ledger and the badge source.
#[derive(Clone, Debug, Default)]
pub struct RuleRuntimeState {
    /// Session-relative time of the most recent fire.
    pub last_fired_secs: f64,
    /// Whether the rule's last evaluation was a violation (drives the badge).
    pub currently_violated: bool,
    /// Total fires this session.
    pub fire_count: u64,
    /// Detail string of the most recent violation (badge hover / banner).
    pub last_detail: String,
}

/// The rule set + ledger, resident as a Bevy resource for the live engine and
/// constructed standalone by the offline analyzer.
#[derive(Resource, Default)]
pub struct InvariantRegistry {
    rules: Vec<Box<dyn Rule>>,
    state: HashMap<RuleId, RuleRuntimeState>,
}

impl InvariantRegistry {
    /// Register a rule and seed its ledger entry.
    pub fn register(&mut self, rule: impl Rule + 'static) {
        self.state.entry(rule.header().id).or_default();
        self.rules.push(Box::new(rule));
    }

    /// The registered rules (for the tick to evaluate / the analyzer to replay).
    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    /// Apply a rule's [`DebouncePolicy`] to a fresh verdict, updating the ledger,
    /// and return `Some(detail)` when the caller should actually FIRE (log +
    /// badge) — or `None` when the fire is debounced or the verdict is `Clear`.
    /// `now` is session-relative seconds.
    pub fn note_verdict(
        &mut self,
        id: RuleId,
        debounce: DebouncePolicy,
        verdict: &Verdict,
        now: f64,
    ) -> Option<String> {
        let st = self.state.entry(id).or_default();
        match verdict {
            Verdict::Clear => {
                st.currently_violated = false;
                None
            }
            Verdict::Violated { detail } => {
                let was = st.currently_violated;
                st.currently_violated = true;
                st.last_detail = detail.clone();
                let should_fire = match debounce {
                    DebouncePolicy::OncePerCondition => !was,
                    DebouncePolicy::EveryEval => true,
                    DebouncePolicy::Interval(n) => !was || (now - st.last_fired_secs) >= n as f64,
                };
                if should_fire {
                    st.fire_count += 1;
                    st.last_fired_secs = now;
                    Some(detail.clone())
                } else {
                    None
                }
            }
        }
    }

    /// Clear a rule's violated flag when it stops being evaluated (e.g. its
    /// `when_state` no longer matches the current state), so a stale badge or
    /// banner does not stick after the condition is no longer being checked.
    pub fn clear_violation(&mut self, id: RuleId) {
        if let Some(st) = self.state.get_mut(id) {
            st.currently_violated = false;
        }
    }

    /// The ledger entry for a rule, if it has one.
    pub fn state(&self, id: RuleId) -> Option<&RuleRuntimeState> {
        self.state.get(id)
    }

    /// Currently-violated rules with their severity + ledger, for the GUI
    /// badges (D-6). Cross-references the rule headers for severity.
    pub fn active_badges(&self) -> impl Iterator<Item = (RuleId, Severity, &RuleRuntimeState)> {
        self.rules.iter().filter_map(move |r| {
            let h = r.header();
            let st = self.state.get(h.id)?;
            st.currently_violated.then_some((h.id, h.severity, st))
        })
    }

    /// The worst severity currently active — for the toolbar warning dot.
    pub fn worst_active(&self) -> Option<Severity> {
        self.active_badges().map(|(_, sev, _)| sev).max()
    }

    /// Count currently-violated rules whose subsystem is `subsystem` — the
    /// GUI's per-tab anomaly counter (C-6). Zero when nothing in that subsystem
    /// is active, so the tab label stays clean.
    pub fn active_count_for(&self, subsystem: Subsystem) -> usize {
        self.rules
            .iter()
            .filter(|r| {
                let h = r.header();
                h.subsystem == subsystem
                    && self.state.get(h.id).is_some_and(|s| s.currently_violated)
            })
            .count()
    }
}

/// Build the registry with every built-in rule registered. Shared by the app
/// plugin (D-4) and the offline analyzer (D-5) so the two evaluate an identical
/// rule set. The built-in rule modules (D-2/D-3) extend this constructor to
/// register themselves.
pub fn default_registry() -> InvariantRegistry {
    let mut reg = InvariantRegistry::default();
    super::rules::register_builtins(&mut reg);
    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::anomaly::rule::{LiveCtx, RuleHeader};
    use crate::diagnostics::event::Subsystem;

    fn header(id: RuleId, debounce: DebouncePolicy) -> RuleHeader {
        RuleHeader {
            id,
            subsystem: Subsystem::Runtime,
            severity: Severity::Warn,
            debounce,
            description: "test",
            when_state: None,
        }
    }

    fn violated() -> Verdict {
        Verdict::violated("x")
    }

    #[test]
    fn once_per_condition_fires_on_rising_edge_only() {
        let mut reg = InvariantRegistry::default();
        let d = DebouncePolicy::OncePerCondition;
        assert!(reg.note_verdict("r", d, &violated(), 0.0).is_some()); // rising edge
        assert!(reg.note_verdict("r", d, &violated(), 1.0).is_none()); // still violated
        assert!(reg.note_verdict("r", d, &Verdict::Clear, 2.0).is_none()); // re-arm
        assert!(reg.note_verdict("r", d, &violated(), 3.0).is_some()); // fires again
        assert_eq!(reg.state("r").unwrap().fire_count, 2);
    }

    #[test]
    fn interval_re_fires_after_the_window() {
        let mut reg = InvariantRegistry::default();
        let d = DebouncePolicy::Interval(5.0);
        assert!(reg.note_verdict("r", d, &violated(), 0.0).is_some()); // first
        assert!(reg.note_verdict("r", d, &violated(), 3.0).is_none()); // within 5s
        assert!(reg.note_verdict("r", d, &violated(), 6.0).is_some()); // past 5s
    }

    #[test]
    fn every_eval_fires_each_violation() {
        let mut reg = InvariantRegistry::default();
        let d = DebouncePolicy::EveryEval;
        assert!(reg.note_verdict("r", d, &violated(), 0.0).is_some());
        assert!(reg.note_verdict("r", d, &violated(), 0.1).is_some());
        assert!(reg.note_verdict("r", d, &violated(), 0.2).is_some());
        assert_eq!(reg.state("r").unwrap().fire_count, 3);
    }

    /// A rule that always violates, to check the badge/worst-active surface.
    struct AlwaysBad(RuleHeader);
    impl Rule for AlwaysBad {
        fn header(&self) -> &RuleHeader {
            &self.0
        }
        fn eval(&self, _cx: &LiveCtx) -> Option<Verdict> {
            Some(violated())
        }
    }

    #[test]
    fn badges_reflect_currently_violated_rules() {
        let mut reg = InvariantRegistry::default();
        reg.register(AlwaysBad(header("a", DebouncePolicy::OncePerCondition)));
        // Not violated until noted.
        assert!(reg.worst_active().is_none());
        reg.note_verdict("a", DebouncePolicy::OncePerCondition, &violated(), 0.0);
        assert_eq!(reg.worst_active(), Some(Severity::Warn));
        assert_eq!(reg.active_badges().count(), 1);
        reg.clear_violation("a");
        assert!(reg.worst_active().is_none());
    }

    #[test]
    fn active_count_for_counts_currently_violated_by_subsystem() {
        let mut reg = default_registry();
        let d = DebouncePolicy::OncePerCondition;
        // terrain_collider_missing is Runtime; identity_spoof_burst is Network.
        reg.note_verdict("runtime.terrain_collider_missing", d, &violated(), 0.0);
        reg.note_verdict("net.identity_spoof_burst", d, &violated(), 0.0);

        assert_eq!(reg.active_count_for(Subsystem::Runtime), 1);
        assert_eq!(reg.active_count_for(Subsystem::Network), 1);
        assert_eq!(reg.active_count_for(Subsystem::Offload), 0);
        // Clearing one drops its subsystem's count back to zero.
        reg.clear_violation("runtime.terrain_collider_missing");
        assert_eq!(reg.active_count_for(Subsystem::Runtime), 0);
    }
}
