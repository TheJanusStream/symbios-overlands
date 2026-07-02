//! Invariant / anomaly engine (Pillar D) — a single shared rule set that runs
//! LIVE against the metrics registry + ECS state (flagging anomalies into the
//! session log and GUI badges as they happen) and, replayed, OFFLINE over a
//! captured log (the `--analyze-session` post-mortem). One [`Rule`] definition
//! serves both, so a rule added once is caught everywhere (the parity
//! guarantee).
//!
//! # Sub-modules
//!
//! - [`rule`] — the [`Rule`] trait + [`RuleHeader`] / [`Verdict`] /
//!   [`DebouncePolicy`] / [`LiveCtx`] vocabulary (D-0).
//! - [`registry`] — the [`InvariantRegistry`] rule set + debounce ledger + badge
//!   source (D-1), and [`default_registry`] — the *one* rule set both the live
//!   engine and the offline analyzer build (this is what makes them agree).
//! - [`rules`] / [`rules_ecs`] — the built-in invariants: log-expressible
//!   (live + replay) and ECS-state (live-only) respectively (D-2 / D-3).
//! - [`tick`] — the 1 Hz [`AnomalyPlugin`] evaluation + routing to log/badge (D-4).
//! - [`replay`] — the offline harness that folds a captured log (D-5).
//!
//! # Adding a rule
//!
//! Three steps (only step 1 is ever mandatory):
//!
//! 1. **Define the rule** — a unit struct, a `const` [`RuleHeader`] (identity,
//!    subsystem, severity, firing policy), and an `impl` [`Rule`]. Implement
//!    [`eval`](Rule::eval) for LIVE detection (reads a [`LiveCtx`]) and/or
//!    [`replay`](Rule::replay) for OFFLINE detection (folds the captured event
//!    log). A rule with *both* runs live and re-derives in the analyzer from one
//!    definition; if you add a `replay` body, also override
//!    [`is_replayable`](Rule::is_replayable) to `true` (two drift-guard tests in
//!    [`replay`] pin the replayable set both directions — keep them in sync).
//! 2. **Add a [`LiveCtx`] field only if you need a new reading.** Metric-threshold
//!    rules already have everything via `cx.metrics`; a rule that needs fresh ECS
//!    state gains a field on [`LiveCtx`] (in [`rule`]) that the [`tick`] system
//!    pre-gathers — so rule bodies stay pure and never touch the `World`.
//! 3. **Register it** with one line in [`rules::register_builtins`] (or
//!    [`rules_ecs::register_ecs_rules`] for a live-only ECS rule):
//!    `reg.register(MyRule);`. Both feed [`default_registry`], so the rule goes
//!    live *and* offline with no further wiring.
//!
//! ## Worked example
//!
//! A complete live metric-threshold rule (steps 1 + 3) — fires a `Warn` when the
//! entity count runs past a budget, re-firing at most every 10 s while it holds:
//!
//! ```rust
//! use symbios_overlands::diagnostics::anomaly::{
//!     DebouncePolicy, InvariantRegistry, LiveCtx, Rule, RuleHeader, Verdict,
//! };
//! use symbios_overlands::diagnostics::event::{Severity, Subsystem};
//! use symbios_overlands::diagnostics::names;
//!
//! // 1. A rule is a unit struct + a `const RuleHeader` + an `impl Rule`.
//! struct TooManyEntities;
//!
//! const TOO_MANY_ENTITIES: RuleHeader = RuleHeader {
//!     id: "runtime.too_many_entities",
//!     subsystem: Subsystem::Runtime,
//!     severity: Severity::Warn,
//!     debounce: DebouncePolicy::Interval(10.0),
//!     description: "entity count over budget",
//!     when_state: None, // evaluate in every AppState
//! };
//!
//! impl Rule for TooManyEntities {
//!     fn header(&self) -> &RuleHeader {
//!         &TOO_MANY_ENTITIES
//!     }
//!
//!     // Pure: reads only the `LiveCtx`, never the ECS `World`. `None` means the
//!     // input isn't available yet (the gauge has no sample).
//!     fn eval(&self, cx: &LiveCtx) -> Option<Verdict> {
//!         let n = cx.metrics.gauge_latest(names::RUNTIME_ENTITY_COUNT)?;
//!         Some(if n > 50_000.0 {
//!             Verdict::violated(format!("{n:.0} entities"))
//!         } else {
//!             Verdict::Clear
//!         })
//!     }
//! }
//!
//! // 3. One registration line puts it in the shared set that both the live
//! //    engine and the `--analyze-session` analyzer build.
//! let mut reg = InvariantRegistry::default();
//! reg.register(TooManyEntities);
//! ```

pub mod registry;
pub mod replay;
pub mod rule;
pub mod rules;
pub mod rules_ecs;
pub mod tick;

pub use registry::{InvariantRegistry, RuleRuntimeState, default_registry};
pub use replay::{RuleFinding, replay_findings, replay_invariants};
pub use rule::{DebouncePolicy, LiveCtx, Rule, RuleHeader, RuleId, Verdict};
pub use tick::{AnomalyPlugin, LoadingClock};
