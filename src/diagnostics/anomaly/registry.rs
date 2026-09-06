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

/// The severity floor for anything that reads as an ALARM: the toolbar's
/// worst-active dot and the click-through it offers (#1271 f184).
///
/// Two shipped rules are [`Severity::Info`] — `runtime.orphan_avatar_visual`
/// and `runtime.memory_retention_across_rebuilds` — and before this the dot
/// fired for either of them. A user who has learned that the dot means "open
/// this, something is wrong" clicks it and finds a note. An alarm that fires
/// for information is an alarm that gets ignored, and it is ignored for the
/// Critical that arrives twenty minutes later.
///
/// Info rules are not silenced, only demoted: they still carry a per-metric
/// pill, a tab-label count and a row in the Active Anomalies strip, which is
/// where a note belongs.
pub const ALARM_FLOOR: Severity = Severity::Warn;

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

    /// A rule's human-readable one-liner, for the GUI badge text (#837 —
    /// the ids alone read as internal identifiers, not player language).
    pub fn rule_description(&self, id: RuleId) -> Option<&'static str> {
        self.header_of(id).map(|h| h.description)
    }

    /// The precise statement behind a rule's badge, for the hover (#1271
    /// f409). `None` when the description already says it exactly.
    pub fn rule_technical(&self, id: RuleId) -> Option<&'static str> {
        self.header_of(id).and_then(|h| h.technical)
    }

    fn header_of(&self, id: RuleId) -> Option<&crate::diagnostics::anomaly::RuleHeader> {
        self.rules
            .iter()
            .find(|r| r.header().id == id)
            .map(|r| r.header())
    }

    /// The worst severity currently active at or above `min` — for the
    /// toolbar warning dot.
    ///
    /// The floor is a parameter and not a default because there is no
    /// safe default: the toolbar wants [`ALARM_FLOOR`] and a test
    /// asserting "did anything at all fire" wants [`Severity::Trace`],
    /// and a caller that picks the wrong one silently either lights an
    /// alarm for trivia or hides a Critical. Passing it is the reminder.
    pub fn worst_active(&self, min: Severity) -> Option<Severity> {
        self.active_badges()
            .map(|(_, sev, _)| sev)
            .filter(|sev| *sev >= min)
            .max()
    }

    /// How many currently-violated rules sit at or above `min` — the
    /// count the toolbar dot prints beside itself. It has to share the
    /// dot's floor: a dot that appears for one Warn while saying "3"
    /// because two Info rules are also live is counting something the
    /// user cannot see.
    pub fn active_count_at_least(&self, min: Severity) -> usize {
        self.active_badges()
            .filter(|(_, sev, _)| *sev >= min)
            .count()
    }

    /// The header of the worst currently-active badge at or above `min`.
    /// Ties resolve to the first rule in registration order, which is
    /// stable within a build.
    fn worst_header(&self, min: Severity) -> Option<&crate::diagnostics::anomaly::RuleHeader> {
        self.rules
            .iter()
            .map(|r| r.header())
            .filter(|h| {
                h.severity >= min && self.state.get(h.id).is_some_and(|st| st.currently_violated)
            })
            .max_by_key(|h| h.severity)
    }

    /// The subsystem owning the worst currently-active badge at or above
    /// `min` — the toolbar dot's click target routes to the matching
    /// Diagnostics tab (#835).
    pub fn worst_active_subsystem(&self, min: Severity) -> Option<Subsystem> {
        self.worst_header(min).map(|h| h.subsystem)
    }

    /// What the worst currently-active badge at or above `min` SAYS — the
    /// toolbar dot's hover (#1271 f409).
    ///
    /// The dot used to say only "{n} active anomalies — click to open
    /// Diagnostics", naming neither the subsystem nor the problem, so a
    /// user who noticed it could not tell whether it was about their
    /// connection without opening a panel and picking the right tab. The
    /// rule already carries a sentence for exactly this.
    pub fn worst_active_description(&self, min: Severity) -> Option<&'static str> {
        self.worst_header(min).map(|h| h.description)
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
            technical: None,
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
        assert!(reg.worst_active(Severity::Trace).is_none());
        reg.note_verdict("a", DebouncePolicy::OncePerCondition, &violated(), 0.0);
        assert_eq!(reg.worst_active(Severity::Trace), Some(Severity::Warn));
        assert_eq!(reg.active_badges().count(), 1);
        reg.clear_violation("a");
        assert!(reg.worst_active(Severity::Trace).is_none());
    }

    /// #1271 f184. `runtime.orphan_avatar_visual` is `Severity::Info` and
    /// `runtime.frame_time_spike` is `Severity::Warn`; the toolbar dot must
    /// see the second and not the first, while the badge strip sees both.
    #[test]
    fn the_alarm_floor_hides_info_rules_from_the_toolbar_dot() {
        let mut reg = default_registry();
        let d = DebouncePolicy::OncePerCondition;
        reg.note_verdict("runtime.orphan_avatar_visual", d, &violated(), 0.0);

        // The control: this is exactly the state that used to light the dot.
        assert_eq!(reg.worst_active(Severity::Trace), Some(Severity::Info));
        assert_eq!(reg.active_badges().count(), 1, "the strip still lists it");
        assert_eq!(reg.worst_active(ALARM_FLOOR), None, "no dot for a note");
        assert_eq!(reg.active_count_at_least(ALARM_FLOOR), 0);
        assert_eq!(reg.worst_active_subsystem(ALARM_FLOOR), None);

        // A real alarm lights it, and the count it prints is the alarm
        // count — not the two badges the strip is showing.
        reg.note_verdict("runtime.frame_time_spike", d, &violated(), 1.0);
        assert_eq!(reg.worst_active(ALARM_FLOOR), Some(Severity::Warn));
        assert_eq!(reg.active_badges().count(), 2);
        assert_eq!(reg.active_count_at_least(ALARM_FLOOR), 1);
        assert_eq!(
            reg.worst_active_subsystem(ALARM_FLOOR),
            Some(Subsystem::Runtime)
        );
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
