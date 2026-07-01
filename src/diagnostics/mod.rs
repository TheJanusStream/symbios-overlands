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
//!
//! Later pillars add: `log` (the `SessionLog` funnel + ring buffer), `sink`
//! (native NDJSON writer), `snapshot` (the startup record), `registry`/`names`
//! (the metrics spine), and `anomaly` (the invariant engine).

pub mod anomaly;
pub mod bevy_bridge;
pub mod event;
pub mod log;
pub mod names;
pub mod panic;
pub mod plugin;
pub mod registry;
pub mod sink;
pub mod snapshot;

pub use bevy_bridge::MetricsPlugin;
pub use log::SessionLog;
pub use plugin::DiagnosticsPlugin;
pub use registry::{Distro, MetricKind, MetricsRegistry, distro, distro_str};
pub use sink::Sink;
