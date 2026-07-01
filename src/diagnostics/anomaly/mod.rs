//! Invariant / anomaly engine (Pillar D) — a single shared rule set that runs
//! LIVE against the metrics registry + ECS state (flagging anomalies into the
//! session log and GUI badges as they happen) and, replayed, OFFLINE over a
//! captured log (the `--analyze-session` post-mortem). One [`rule::Rule`]
//! definition serves both, so a rule added once is caught everywhere.
//!
//! # Sub-module map
//!
//! - [`rule`] — the [`Rule`] trait + [`RuleHeader`] / [`Verdict`] /
//!   [`DebouncePolicy`] / [`LiveCtx`] vocabulary (D-0).
//!
//! Later slices add: `registry` (the rule registry + debounce ledger + badge
//! source, D-1), `rules` (the built-in invariants, D-2/D-3), `tick` (the
//! per-frame/1 Hz evaluation + routing, D-4) and `replay` (the offline harness,
//! D-5).

pub mod registry;
pub mod replay;
pub mod rule;
pub mod rules;
pub mod rules_ecs;
pub mod tick;

pub use registry::{InvariantRegistry, RuleRuntimeState, default_registry};
pub use replay::{RuleFinding, replay_findings, replay_invariants};
pub use rule::{DebouncePolicy, LiveCtx, Rule, RuleHeader, RuleId, Verdict};
pub use tick::AnomalyPlugin;
