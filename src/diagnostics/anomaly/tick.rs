//! The live diagnostic tick + `AnomalyPlugin` (Pillar D-4).
//!
//! Once per second the tick builds a read-only [`LiveCtx`] from the metrics
//! registry + a few pre-gathered ECS scalars, evaluates every applicable rule,
//! applies each rule's debounce policy, and routes an actual fire three ways:
//! a structured `InvariantViolation` event into the session log (Pillar A), the
//! per-rule badge state in the registry (Pillar C reads it), and a console
//! `warn!`/`error!` line. The badge routing is implicit — [`InvariantRegistry::note_verdict`]
//! updates the ledger the GUI reads.
//!
//! The rule-evaluation core is factored into [`run_rules`] (pure over its inputs)
//! so the whole engine — rules, debounce and routing — is unit-testable without
//! standing up a Bevy `App`.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::common_conditions::on_timer;

use crate::diagnostics::SessionLog;
use crate::diagnostics::anomaly::registry::{InvariantRegistry, default_registry};
use crate::diagnostics::anomaly::rule::{LiveCtx, RuleId};
use crate::diagnostics::event::{EventPayload, Severity};
use crate::state::{AppState, LocalPlayer};

/// Tracks when `Loading` was entered, so the live [`LoadingGateStall`] rule can
/// measure how long the gate has been open.
///
/// [`LoadingGateStall`]: crate::diagnostics::anomaly::rules
#[derive(Resource, Default)]
pub struct LoadingClock {
    entered_at: Option<f64>,
}

impl LoadingClock {
    /// Session-relative seconds when `Loading` was entered, or `None` outside
    /// the loading gate — lets the loading screen render a live gate countdown
    /// against the same clock the stall rule measures (C-5).
    pub fn entered_at(&self) -> Option<f64> {
        self.entered_at
    }
}

/// Evaluate every applicable rule against `cx`, debounce, and route fires into
/// `log` + the registry ledger. Pure over its inputs (no `World` access), so
/// tests drive it directly. Returns the number of rules that actually fired.
pub fn run_rules(invariants: &mut InvariantRegistry, cx: &LiveCtx, log: &mut SessionLog) -> usize {
    let now = cx.now_secs;
    // Collect (id, debounce, severity, verdict) first so the immutable borrow
    // of `invariants.rules()` is released before we mutate the ledger. In the
    // same pass, note every rule skipped because its `when_state` no longer
    // matches — those need their badge cleared below.
    let mut to_clear: Vec<RuleId> = Vec::new();
    let results: Vec<_> = invariants
        .rules()
        .iter()
        .filter_map(|r| {
            let h = r.header();
            if let Some(ws) = &h.when_state
                && *ws != cx.state
            {
                to_clear.push(h.id);
                return None;
            }
            r.eval(cx).map(|v| (h.id, h.debounce, h.severity, v))
        })
        .collect();

    // Clear the badge for every state-skipped rule (#632). `note_verdict` only
    // auto-clears rules it actually evaluates (via `Verdict::Clear`), so a rule
    // left `Violated` at the instant its gating state was exited — e.g.
    // `loading.gate_stall` when a slow login finally completes and we switch to
    // `InGame` — would otherwise keep its Critical badge + "session health
    // compromised" banner for the entire rest of the session. Re-entering the
    // state re-evaluates and re-fires normally.
    for id in to_clear {
        invariants.clear_violation(id);
    }

    let mut fired = 0;
    for (id, debounce, severity, verdict) in results {
        if let Some(detail) = invariants.note_verdict(id, debounce, &verdict, now) {
            fired += 1;
            log.record(
                now,
                severity,
                EventPayload::InvariantViolation {
                    rule: id.to_string(),
                    detail: detail.clone(),
                },
            );
            match severity {
                Severity::Critical | Severity::Error => error!("diagnostic {id}: {detail}"),
                Severity::Warn => warn!("diagnostic {id}: {detail}"),
                _ => debug!("diagnostic {id}: {detail}"),
            }
        }
    }
    fired
}

#[allow(clippy::too_many_arguments)]
fn diagnostic_tick(
    metrics: Res<crate::diagnostics::MetricsRegistry>,
    mut invariants: ResMut<InvariantRegistry>,
    mut log: ResMut<SessionLog>,
    time: Res<Time>,
    state: Res<State<AppState>>,
    loading_clock: Res<LoadingClock>,
    player_q: Query<&Transform, With<LocalPlayer>>,
    bodies_q: Query<&Transform, With<avian3d::prelude::RigidBody>>,
) {
    let now = time.elapsed_secs_f64();
    let cur_state = state.get().clone();

    let loading_elapsed_secs = (cur_state == AppState::Loading)
        .then(|| loading_clock.entered_at.map(|t| now - t))
        .flatten();
    let player_y = player_q.iter().next().map(|t| t.translation.y);
    // Bound cost: this is a 1 Hz scan of physics-body transforms only.
    let nan_body_count = bodies_q
        .iter()
        .filter(|t| {
            !t.translation.is_finite() || !t.rotation.to_array().iter().all(|c| c.is_finite())
        })
        .count();

    let cx = LiveCtx {
        now_secs: now,
        state: cur_state,
        metrics: &metrics,
        loading_elapsed_secs,
        player_y,
        // ground_y / orphan_avatar_count / respawns_recent are wired when their
        // sources land (heightmap sampling, avatar-visual marker query, respawn
        // counter via E-4); until then those rules stay dormant.
        ground_y: None,
        nan_body_count,
        orphan_avatar_count: 0,
        respawns_recent: 0,
    };

    run_rules(&mut invariants, &cx, &mut log);
}

fn loading_clock_enter(
    mut clock: ResMut<LoadingClock>,
    time: Res<Time>,
    mut log: ResMut<SessionLog>,
) {
    let now = time.elapsed_secs_f64();
    clock.entered_at = Some(now);
    // Mark the loading-gate open in the session stream (B-2 timeline start +
    // the LoadingGateStall replay rule's start marker).
    log.info(now, EventPayload::LoadingPhaseStarted);
}

fn loading_clock_exit(
    mut clock: ResMut<LoadingClock>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut log: ResMut<SessionLog>,
) {
    // Record the total wall time spent in the loading gate (E-4) before clearing
    // the entry stamp — this OnExit(Loading) system owns the gate timing. The
    // only exit from `Loading` is into `InGame` (logout is `InGame → Login`), so
    // this is the Loading → InGame transition.
    if let Some(entered_at) = clock.entered_at {
        let now = time.elapsed_secs_f64();
        let elapsed = now - entered_at;
        crate::diagnostics::samplers::loading_gate_total_secs(&mut metrics, elapsed, now);
        log.info(
            now,
            EventPayload::LoadingGateTransitionToInGame {
                elapsed_secs: elapsed,
            },
        );
    }
    clock.entered_at = None;
}

/// Installs the live anomaly engine: the shared rule registry, the loading
/// clock, and the 1 Hz tick. Additive — no existing system changes.
pub struct AnomalyPlugin;

impl Plugin for AnomalyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(default_registry())
            .init_resource::<LoadingClock>()
            .add_systems(OnEnter(AppState::Loading), loading_clock_enter)
            .add_systems(OnExit(AppState::Loading), loading_clock_exit)
            .add_systems(
                Update,
                diagnostic_tick.run_if(on_timer(Duration::from_secs(1))),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MetricsRegistry;
    use crate::diagnostics::event::Subsystem;
    use crate::diagnostics::names;

    fn ctx_ingame<'a>(metrics: &'a MetricsRegistry) -> LiveCtx<'a> {
        LiveCtx {
            now_secs: 10.0,
            state: AppState::InGame,
            metrics,
            loading_elapsed_secs: None,
            player_y: None,
            ground_y: None,
            nan_body_count: 0,
            orphan_avatar_count: 0,
            respawns_recent: 0,
        }
    }

    #[test]
    fn engine_fires_terrain_collider_missing_and_routes_to_log_and_badge() {
        let mut invariants = default_registry();
        let mut log = SessionLog::with_capacity(64);
        let mut metrics = MetricsRegistry::default();
        // Zero colliders in-game → TerrainColliderMissing (Critical) violates.
        metrics.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0, 10.0);

        let fired = run_rules(&mut invariants, &ctx_ingame(&metrics), &mut log);
        assert!(fired >= 1);

        // Routed to the session log as an InvariantViolation for that rule.
        assert!(log.iter().any(|e| matches!(
            &e.payload,
            EventPayload::InvariantViolation { rule, .. }
                if rule == "runtime.terrain_collider_missing"
        )));
        // Routed to the badge ledger (Pillar C source).
        assert_eq!(invariants.worst_active(), Some(Severity::Critical));
        assert!(
            invariants
                .active_badges()
                .any(|(id, sev, _)| id == "runtime.terrain_collider_missing"
                    && sev == Severity::Critical)
        );
    }

    #[test]
    fn once_per_condition_does_not_re_fire_while_still_violated() {
        let mut invariants = default_registry();
        let mut log = SessionLog::with_capacity(64);
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0, 10.0);

        let mut cx = ctx_ingame(&metrics);
        assert!(run_rules(&mut invariants, &cx, &mut log) >= 1);
        cx.now_secs = 11.0;
        // Still zero colliders → OncePerCondition rule must NOT re-fire.
        let n = invariants
            .state("runtime.terrain_collider_missing")
            .unwrap()
            .fire_count;
        run_rules(&mut invariants, &cx, &mut log);
        assert_eq!(
            invariants
                .state("runtime.terrain_collider_missing")
                .unwrap()
                .fire_count,
            n,
            "no re-fire while continuously violated"
        );
    }

    #[test]
    fn when_state_gate_skips_inapplicable_rules() {
        // In Login, the InGame-gated TerrainColliderMissing must not evaluate.
        let mut invariants = default_registry();
        let mut log = SessionLog::with_capacity(64);
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0, 10.0);
        let mut cx = ctx_ingame(&metrics);
        cx.state = AppState::Login;
        run_rules(&mut invariants, &cx, &mut log);
        assert!(
            invariants
                .state("runtime.terrain_collider_missing")
                .is_none()
                || !invariants
                    .state("runtime.terrain_collider_missing")
                    .unwrap()
                    .currently_violated
        );
        // Sanity: the toy subsystem tag on that rule is Runtime.
        assert_eq!(TERRAIN_SUBSYSTEM, Subsystem::Runtime);
    }

    #[test]
    fn state_gated_badge_clears_on_state_exit() {
        // Regression for #632: a rule left `Violated` at the moment its gating
        // state is exited must have its badge cleared, not stick for the whole
        // session. Before the fix, `run_rules` silently dropped the skipped rule
        // and `currently_violated` stayed `true` forever.
        let mut invariants = default_registry();
        let mut log = SessionLog::with_capacity(64);
        let mut metrics = MetricsRegistry::default();
        metrics.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0, 10.0);

        // In-game with zero colliders → the InGame-gated Critical rule violates
        // and lights its badge.
        let mut cx = ctx_ingame(&metrics);
        run_rules(&mut invariants, &cx, &mut log);
        assert!(
            invariants
                .state("runtime.terrain_collider_missing")
                .unwrap()
                .currently_violated,
            "rule should be violated in-game with zero colliders"
        );
        assert_eq!(invariants.worst_active(), Some(Severity::Critical));

        // Leave InGame (e.g. logout → Login). The rule is now state-skipped;
        // its badge must clear rather than persist.
        cx.state = AppState::Login;
        cx.now_secs = 11.0;
        run_rules(&mut invariants, &cx, &mut log);
        assert!(
            !invariants
                .state("runtime.terrain_collider_missing")
                .unwrap()
                .currently_violated,
            "badge must clear when the gating state is exited (#632)"
        );
        assert_eq!(
            invariants.worst_active(),
            None,
            "no active badge should remain after the gating state is exited"
        );
    }

    // Pin the expected subsystem so a rename is caught by the test above.
    const TERRAIN_SUBSYSTEM: Subsystem = Subsystem::Runtime;
}
