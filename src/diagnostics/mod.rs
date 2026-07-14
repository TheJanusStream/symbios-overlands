//! Diagnostic suite (epic #588) — the developer- and agent-facing tooling for
//! catching issues early across a live session.
//!
//! The suite is built around three shared spines every part reads/writes so
//! nothing drifts:
//!
//! - a single append-only **session event stream** ([`event`]) — the same
//!   records feed the in-game event log (a bounded tail view), a native NDJSON
//!   file a coding agent reads for a post-mortem, and the offline
//!   `--analyze-session` analyzer;
//! - a shared **metrics registry** (gauges / counters / histograms);
//! - an **invariant registry** of rules that run both live and, replayed,
//!   offline.
//!
//! # Sub-module map
//!
//! - [`event`] — the [`event::SessionEvent`] model + [`event::EventPayload`]
//!   taxonomy (Pillar A-1). Free of gameplay types so it round-trips through
//!   serde on native and wasm.
//! - [`log`] — the [`SessionLog`] funnel + in-memory ring buffer backing the
//!   in-game event log and the wasm "Download log" dump.
//! - [`sink`] — the native NDJSON file writer (per-session files +
//!   `session-latest.jsonl`); [`panic`](mod@panic) mirrors the tail into a panic-hook
//!   ring so a crash still leaves a `session-panic-*.jsonl`.
//! - [`snapshot`] — the startup record (build/environment context) emitted as
//!   each session's first event.
//! - [`registry`] / [`names`] — the shared metrics spine (gauges / counters /
//!   histograms) and its single metric-name vocabulary; [`samplers`] are the
//!   per-subsystem helpers that feed it, and [`bevy_bridge`] scrapes Bevy's
//!   built-in diagnostics into it once per second.
//! - [`anomaly`] — the invariant engine: one rule set run live (1 Hz) and
//!   replayed offline.
//! - [`analyze`] — the offline `--analyze-session` / `--diff-sessions`
//!   post-mortem reports printed by the `render` bin.
//! - [`plugin`] — the Bevy wiring (`DiagnosticsPlugin`): constructs the log,
//!   arms the panic hook, records the boot snapshot, registers flush systems.

pub mod analyze;
pub mod anomaly;
pub mod bevy_bridge;
pub mod crash_log;
pub mod event;
pub mod log;
pub mod names;
pub mod panic;
pub mod plugin;
pub mod registry;
pub mod samplers;
pub mod sink;
pub mod snapshot;

pub use bevy_bridge::MetricsPlugin;
pub use log::SessionLog;
pub use plugin::DiagnosticsPlugin;
pub use registry::{Distro, MetricKind, MetricsRegistry, distro, distro_str};
pub use sink::Sink;
