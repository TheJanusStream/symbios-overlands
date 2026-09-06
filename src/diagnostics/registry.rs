//! Shared metrics registry (Spine E-1) — the one source of truth the session
//! log (A), the Diagnostics GUI (C) and the invariant engine (D) all read.
//!
//! It holds three value shapes, all allocation-bounded so the wasm heap stays
//! flat: [`Gauge`] (latest value + a fixed ring for sparklines), [`Counter`]
//! (monotonic), and [`Histogram`] (bounded sample set reduced to a [`Distro`]
//! via the shared [`distro`] reducer — the same `min/p50/p90/max/mean` summary
//! that used to live in `urban/diagnostics.rs`, now single-sourced here).
//!
//! Metrics are keyed by stable `&'static str` names (see the `names` module,
//! E-2) so lookups are pointer-cheap and typos are compile errors.
//!
//! # Read API — the stable contract (E-6)
//!
//! The reader methods on [`MetricsRegistry`] (grouped under the "Read API"
//! banner below) plus the metric **names**
//! ([`names::ALL`](crate::diagnostics::names::ALL)) are the published surface
//! every downstream pillar binds to — treat them as API. Renaming a metric or
//! changing a reader's shape breaks both a GUI row *and* any invariant that
//! reads it (see the map below).
//!
//! | Consumer | Reads via |
//! | --- | --- |
//! | C — Overview sparkline (C-3) | [`ring_slice`](MetricsRegistry::ring_slice) (raw samples), [`gauge_distro`](MetricsRegistry::gauge_distro) (p50/p90/max), [`gauge_latest`](MetricsRegistry::gauge_latest) |
//! | C — health cards (C-4) | [`counter_value`](MetricsRegistry::counter_value), [`hist_distro_str`](MetricsRegistry::hist_distro_str), [`gauge_latest`](MetricsRegistry::gauge_latest) |
//! | D — invariant thresholds | [`gauge`](MetricsRegistry::gauge) / [`counter`](MetricsRegistry::counter) (latest value + windowed growth over the sparkline ring) |
//!
//! Every name in `names::ALL` is pre-seeded at startup, so a named-but-never-
//! observed metric reads as `—` ([`gauge_latest`](MetricsRegistry::gauge_latest)
//! → `None`) / `0` ([`counter_value`](MetricsRegistry::counter_value)) rather
//! than looking absent.
//!
//! ## Which invariant reads which metric (D)
//!
//! The D-pillar rules that threshold on a metric — the rest read pre-gathered
//! [`LiveCtx`](crate::diagnostics::anomaly::LiveCtx) scalars, or (the
//! replay-only rules) only the event log:
//!
//! | Invariant (rule id) | Metric(s) read live |
//! | --- | --- |
//! | `runtime.frame_time_spike` | `runtime.frame_time.ms` |
//! | `runtime.terrain_collider_missing` | `runtime.collider.count` |
//! | `runtime.asset_handle_spike` | `runtime.mesh_handle.count` (window growth) |
//! | `runtime.asset_growth_across_rebuilds` | `runtime.rebuild.image_handles` / `.mesh_handles` (per-rebuild marks, #921) |
//! | `runtime.memory_retention_across_rebuilds` | `runtime.rebuild.memory_bytes` vs the handle marks (#921) |
//! | `runtime.shape_mesh_cache_growth` | `runtime.shape_mesh_cache.len` (window growth) |
//! | `net.identity_spoof_burst` | `net.identity.spoofed_count` (also replays `PeerIdentitySpoofRejected`) |
//!
//! `LiveCtx`-scalar rules (no metric read): `runtime.player_fell_through_terrain`
//! (player/ground Y), `runtime.nan_in_physics` (NaN body count),
//! `runtime.respawn_thrashing` (recent respawns — cf. `runtime.respawn.count`),
//! `runtime.orphan_avatar_visual` (orphan count), `loading.gate_stall`
//! (`loading_elapsed_secs` — cf. `loading.gate.total_secs`). Replay-only rules
//! (event log, no live metric): `loading.record_fetch_exhausted`,
//! `offload.ambient_bake_stall`, `offload.task_never_resolves`,
//! `net.peer_churn_spike`, `net.offer_acceptance_anomaly`,
//! `net.silent_decode_failure`.

use std::collections::{HashMap, VecDeque};
use std::fmt;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

/// Ring length for gauge sparklines: 120 samples = 2 min at the 1 Hz scrape.
pub const RING_CAP: usize = 120;
/// Max histogram samples retained before oldest-drop — bounded so a long
/// session can't grow it without limit (the ShapeMeshCache lesson applied to
/// the metrics themselves).
pub const HIST_CAP: usize = 512;

/// Which value shape a named metric is, so the GUI/log can enumerate metrics
/// and pick the right reader. Referenced by the name table (E-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricKind {
    Gauge,
    Counter,
    Histogram,
}

/// A latest value plus a fixed ring of recent samples for sparklines. The ring
/// never allocates after construction (`observe` overwrites the oldest slot).
#[derive(Clone, Debug)]
pub struct Gauge {
    last: f64,
    ring: [f64; RING_CAP],
    head: usize,
    len: usize,
}

impl Default for Gauge {
    fn default() -> Self {
        // `[f64; 120]` exceeds the arrays that derive `Default`, so hand-roll it.
        Gauge {
            last: 0.0,
            ring: [0.0; RING_CAP],
            head: 0,
            len: 0,
        }
    }
}

impl Gauge {
    fn observe(&mut self, v: f64) {
        self.last = v;
        self.ring[self.head] = v;
        self.head = (self.head + 1) % RING_CAP;
        if self.len < RING_CAP {
            self.len += 1;
        }
    }

    /// The most recent observed value.
    pub fn last(&self) -> f64 {
        self.last
    }

    /// Retained samples, oldest → newest, for a sparkline.
    pub fn iter(&self) -> impl Iterator<Item = f64> + '_ {
        let start = if self.len == RING_CAP { self.head } else { 0 };
        (0..self.len).map(move |i| self.ring[(start + i) % RING_CAP])
    }

    /// Number of retained samples (≤ [`RING_CAP`]).
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the gauge has been observed yet.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A monotonically increasing count, plus a sampled history so "how much
/// did this rise recently" is answerable.
///
/// The history is a [`Gauge`] rather than a second ring implementation: a
/// counter sampled at the 1 Hz scrape ([`MetricsRegistry::sample_counters`])
/// is exactly a gauge of its own running total, and reusing the ring keeps
/// the two shapes' window semantics identical by construction.
///
/// It exists because a cumulative total cannot express "recent" (#1271
/// f179) — the same sentence [`RecentRespawns`] carries. `net.identity
/// .spoofed_count` reaching 3 once at boot pinned the toolbar's alarm dot
/// for the rest of the session with no way to clear it, because the rule
/// under it was thresholding a number that never comes back down.
///
/// [`RecentRespawns`]: crate::diagnostics::anomaly::RecentRespawns
#[derive(Clone, Debug, Default)]
pub struct Counter {
    value: u64,
    history: Gauge,
}

impl Counter {
    fn incr_by(&mut self, n: u64) {
        self.value = self.value.saturating_add(n);
    }

    /// Push the running total onto the history ring. Driven by the 1 Hz
    /// scrape, NOT by `incr_by`: the ring's window only means "the last
    /// ~2 minutes" if its samples are evenly spaced in time, and a
    /// per-increment push would make a busy counter's window a few
    /// seconds and a quiet one's the whole session.
    fn sample(&mut self) {
        self.history.observe(self.value as f64);
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    /// The sampled history ring (oldest → newest).
    pub fn history(&self) -> &Gauge {
        &self.history
    }
}

/// A bounded sample set reduced on read to a [`Distro`].
#[derive(Clone, Debug, Default)]
pub struct Histogram {
    samples: VecDeque<f64>,
    dropped: u64,
}

impl Histogram {
    fn observe(&mut self, v: f64) {
        self.samples.push_back(v);
        while self.samples.len() > HIST_CAP {
            self.samples.pop_front();
            self.dropped += 1;
        }
    }

    /// `min/p50/p90/max/mean` over the retained samples, `None` when empty.
    pub fn distro(&self) -> Option<Distro> {
        let v: Vec<f64> = self.samples.iter().copied().collect();
        distro(&v)
    }

    /// Samples dropped past [`HIST_CAP`] over the session.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// The `min/p50/p90/max/mean` summary of a sample, plus the sample count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distro {
    pub min: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
    pub mean: f64,
    pub n: usize,
}

impl fmt::Display for Distro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "min {:.1}  p50 {:.1}  p90 {:.1}  max {:.1}  mean {:.1}",
            self.min, self.p50, self.p90, self.max, self.mean
        )
    }
}

/// Reduce a sample to `min/p50/p90/max/mean`, or `None` when empty. The single
/// implementation shared by the road diagnostics, the metrics histograms, and
/// the offline analyzer, so every distribution summary reads identically.
pub fn distro(v: &[f64]) -> Option<Distro> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |q: f64| s[(((s.len() - 1) as f64) * q).round() as usize];
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    Some(Distro {
        min: s[0],
        p50: pick(0.5),
        p90: pick(0.9),
        max: s[s.len() - 1],
        mean,
        n: s.len(),
    })
}

/// The [`distro`] summary as a display string, or `—` when empty — the exact
/// form the road report prints (`urban/diagnostics.rs` calls this).
pub fn distro_str(v: &[f64]) -> String {
    distro(v)
        .map(|d| d.to_string())
        .unwrap_or_else(|| "—".to_string())
}

/// One gauge's latest value in a [`MetricSnapshot`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GaugePoint {
    pub name: String,
    pub last: f64,
}

/// One counter's value in a [`MetricSnapshot`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CounterPoint {
    pub name: String,
    pub value: u64,
}

/// One histogram's reduced distribution in a [`MetricSnapshot`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HistPoint {
    pub name: String,
    pub min: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
    pub mean: f64,
    pub n: usize,
}

/// A flat, serde-friendly snapshot of the registry at one instant — the payload
/// the session log records periodically (E-5) so a post-mortem can chart metric
/// trends over the session (memory growth, frame-time p95, entity/asset drift).
/// Only scalars are captured (gauge `last`, counter `value`, histogram distro);
/// the gauge sparkline rings are GUI-only and never serialized, so each JSONL
/// line stays compact.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MetricSnapshot {
    pub at_secs: f64,
    pub gauges: Vec<GaugePoint>,
    pub counters: Vec<CounterPoint>,
    pub histograms: Vec<HistPoint>,
}

/// The metrics store, resident as a Bevy resource. Fed by the Bevy-diagnostics
/// bridge + per-subsystem samplers (E-3/E-4); read by GUI, log and engine.
#[derive(Resource, Default)]
pub struct MetricsRegistry {
    gauges: HashMap<&'static str, Gauge>,
    counters: HashMap<&'static str, Counter>,
    histograms: HashMap<&'static str, Histogram>,
}

impl MetricsRegistry {
    /// Insert an empty entry for every listed metric so the GUI can enumerate
    /// the full catalogue (showing a named-but-empty metric rather than nothing)
    /// before the first observation. Called once at plugin init with
    /// [`names::ALL`](crate::diagnostics::names::ALL).
    pub fn preseed(&mut self, all: &[(&'static str, MetricKind)]) {
        for (name, kind) in all {
            match kind {
                MetricKind::Gauge => {
                    self.gauges.entry(name).or_default();
                }
                MetricKind::Counter => {
                    self.counters.entry(name).or_default();
                }
                MetricKind::Histogram => {
                    self.histograms.entry(name).or_default();
                }
            }
        }
    }

    /// Record a gauge sample (latest value + sparkline point).
    pub fn observe_gauge(&mut self, name: &'static str, v: f64) {
        self.gauges.entry(name).or_default().observe(v);
    }

    /// Increment a counter by one.
    pub fn incr(&mut self, name: &'static str) {
        self.incr_by(name, 1);
    }

    /// Increment a counter by `n`.
    pub fn incr_by(&mut self, name: &'static str, n: u64) {
        self.counters.entry(name).or_default().incr_by(n);
    }

    /// Record a histogram sample.
    pub fn observe_hist(&mut self, name: &'static str, v: f64) {
        self.histograms.entry(name).or_default().observe(v);
    }

    /// Push every counter's running total onto its history ring — one call
    /// from the 1 Hz scrape, so [`counter_window_rise`](Self::counter_window_rise)
    /// has an evenly-spaced window to measure over. Walks the counter map
    /// (tens of entries) once a second.
    pub fn sample_counters(&mut self) {
        for c in self.counters.values_mut() {
            c.sample();
        }
    }

    // ---- Read API (the E-6 stable contract; see the module docs) -----------
    // The published read surface for pillars C (GUI) and D (invariants). These
    // names + shapes are API: a rename ripples into a GUI row and any rule that
    // thresholds on the metric.

    pub fn gauge(&self, name: &str) -> Option<&Gauge> {
        self.gauges.get(name)
    }

    pub fn counter(&self, name: &str) -> Option<&Counter> {
        self.counters.get(name)
    }

    pub fn histogram(&self, name: &str) -> Option<&Histogram> {
        self.histograms.get(name)
    }

    /// The `Distro` for a named histogram, if it exists and is non-empty.
    pub fn hist_distro(&self, name: &str) -> Option<Distro> {
        self.histograms.get(name).and_then(Histogram::distro)
    }

    /// A histogram's distribution as the shared `min p50 p90 max mean` string,
    /// or `—` when the histogram is unknown / has no samples yet — the display
    /// idiom the GUI health cards (C-4) and the road report share. Ergonomic
    /// "distro_string" reader so a card line is a single call.
    pub fn hist_distro_str(&self, name: &str) -> String {
        self.hist_distro(name)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".to_string())
    }

    /// The latest value of a gauge, or `None` when the gauge is unknown or has
    /// never been observed (so the GUI shows `—` rather than a misleading `0`;
    /// an *observed* zero — e.g. a real zero collider count — returns `Some(0.0)`).
    pub fn gauge_latest(&self, name: &str) -> Option<f64> {
        self.gauges
            .get(name)
            .filter(|g| !g.is_empty())
            .map(Gauge::last)
    }

    /// A counter's value, or `0` when it is unknown / never incremented — the
    /// ergonomic reader for the GUI's counter rows (peer churn, rejects, offers).
    pub fn counter_value(&self, name: &str) -> u64 {
        self.counters.get(name).map(Counter::value).unwrap_or(0)
    }

    /// The `Distro` over a gauge's retained sparkline ring (min/p50/p90/max/mean
    /// of the recent ~2-min history), for the Overview tab's frame-time p50/p95
    /// line (C-3). `None` when the gauge is unknown or has no samples yet.
    pub fn gauge_distro(&self, name: &str) -> Option<Distro> {
        let samples = self.ring_slice(name);
        distro(&samples)
    }

    /// How much a gauge ROSE across its retained ring (newest − oldest),
    /// or `None` with fewer than two samples.
    ///
    /// The windowed counterpart of [`gauge_latest`](Self::gauge_latest), and
    /// the shape every "is this still happening" rule wants: a cumulative
    /// total that stopped moving reads as zero here, so a rule written
    /// against it clears itself with no acknowledge affordance to build
    /// and no ledger state to keep (#1271 f179).
    ///
    /// Negative for a gauge that fell — callers thresholding growth
    /// compare against a positive bound, so a fall is naturally clear.
    pub fn gauge_window_rise(&self, name: &str) -> Option<f64> {
        let g = self.gauges.get(name)?;
        if g.len() < 2 {
            return None;
        }
        let first = g.iter().next()?;
        Some(g.last() - first)
    }

    /// How much a CUMULATIVE gauge rose across the retained window.
    ///
    /// The baseline is the oldest retained sample once the ring has
    /// wrapped, and **0 before that** — the value a session's own running
    /// total started from. That second half is what stops a first-sample
    /// blind spot: a relay rejection that lands before the first 1 Hz
    /// scrape shows up as `1 - 1 = 0` under a plain newest-minus-oldest
    /// and would never fire live at all, while the log recorded it
    /// (#1271 f179).
    ///
    /// So for the first [`RING_CAP`] samples this answers "since the
    /// session began" and afterwards a true rolling window. Both are the
    /// right answer to "is this still happening"; the difference is only
    /// how long a quiet rule takes to clear itself, which is bounded by
    /// the ring.
    ///
    /// Use [`gauge_window_rise`](Self::gauge_window_rise) instead for a
    /// gauge that is a LEVEL rather than a total — a mesh-handle count
    /// baselined at 0 would read its whole value as growth.
    pub fn cumulative_window_rise(&self, name: &str) -> Option<f64> {
        Self::rise_over(self.gauges.get(name)?)
    }

    /// How much a counter rose across its sampled history window, on the
    /// same baseline rule as [`cumulative_window_rise`](Self::cumulative_window_rise)
    /// — a counter is a total by construction. `None` until the 1 Hz
    /// [`sample_counters`](Self::sample_counters) pass has laid down a
    /// sample.
    pub fn counter_window_rise(&self, name: &str) -> Option<u64> {
        Self::rise_over(self.counters.get(name)?.history()).map(|r| r.max(0.0) as u64)
    }

    /// The shared baseline rule behind the two readers above.
    fn rise_over(ring: &Gauge) -> Option<f64> {
        if ring.is_empty() {
            return None;
        }
        let baseline = if ring.len() == RING_CAP {
            ring.iter().next()?
        } else {
            0.0
        };
        Some(ring.last() - baseline)
    }

    /// Sparkline samples for a gauge (oldest → newest); empty if unknown.
    pub fn ring_slice(&self, name: &str) -> Vec<f64> {
        self.gauges
            .get(name)
            .map(|g| g.iter().collect())
            .unwrap_or_default()
    }

    /// Reset every metric — called at logout so one session's numbers never
    /// bleed into the next login (parallels the session-log segment reset).
    pub fn clear(&mut self) {
        self.gauges.clear();
        self.counters.clear();
        self.histograms.clear();
    }

    /// Flatten the currently-active metrics into a serde [`MetricSnapshot`] for
    /// the session log. Never-observed (preseeded-empty) gauges/histograms and
    /// still-zero counters are skipped so snapshot lines carry only live data.
    pub fn snapshot(&self, at_secs: f64) -> MetricSnapshot {
        let gauges = self
            .gauges
            .iter()
            .filter(|(_, g)| !g.is_empty())
            .map(|(n, g)| GaugePoint {
                name: n.to_string(),
                last: g.last(),
            })
            .collect();
        let counters = self
            .counters
            .iter()
            .filter(|(_, c)| c.value() > 0)
            .map(|(n, c)| CounterPoint {
                name: n.to_string(),
                value: c.value(),
            })
            .collect();
        let histograms = self
            .histograms
            .iter()
            .filter_map(|(n, h)| {
                h.distro().map(|d| HistPoint {
                    name: n.to_string(),
                    min: d.min,
                    p50: d.p50,
                    p90: d.p90,
                    max: d.max,
                    mean: d.mean,
                    n: d.n,
                })
            })
            .collect();
        MetricSnapshot {
            at_secs,
            gauges,
            counters,
            histograms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauge_ring_keeps_newest_in_order() {
        let mut g = Gauge::default();
        for i in 0..(RING_CAP + 5) {
            g.observe(i as f64);
        }
        let samples: Vec<f64> = g.iter().collect();
        assert_eq!(samples.len(), RING_CAP);
        // Oldest retained is sample 5, newest is RING_CAP+4.
        assert_eq!(samples[0], 5.0);
        assert_eq!(*samples.last().unwrap(), (RING_CAP + 4) as f64);
        assert_eq!(g.last(), (RING_CAP + 4) as f64);
    }

    #[test]
    fn counter_is_monotonic() {
        let mut r = MetricsRegistry::default();
        r.incr("net.peer.connected_count");
        r.incr("net.peer.connected_count");
        r.incr_by("net.peer.connected_count", 3);
        assert_eq!(r.counter("net.peer.connected_count").unwrap().value(), 5);
    }

    #[test]
    fn histogram_drops_oldest_past_cap() {
        let mut h = Histogram::default();
        for i in 0..(HIST_CAP + 10) {
            h.observe(i as f64);
        }
        assert_eq!(h.len(), HIST_CAP);
        assert_eq!(h.dropped(), 10);
    }

    #[test]
    fn distro_reduces_and_formats() {
        let d = distro(&[4.0, 1.0, 3.0, 2.0]).unwrap();
        assert_eq!(d.min, 1.0);
        assert_eq!(d.max, 4.0);
        assert_eq!(d.n, 4);
        assert_eq!((d.mean * 10.0).round() / 10.0, 2.5);
        assert_eq!(distro_str(&[]), "—");
        assert_eq!(
            d.to_string(),
            "min 1.0  p50 3.0  p90 4.0  max 4.0  mean 2.5"
        );
    }

    #[test]
    fn snapshot_captures_active_metrics_and_round_trips() {
        let mut r = MetricsRegistry::default();
        r.preseed(&[("runtime.frame_time.ms", MetricKind::Gauge)]); // preseeded-empty
        r.observe_gauge("runtime.entity.count", 42.0);
        r.incr_by("net.peer.connected_count", 3);
        r.observe_hist("net.jitter.playout_latency_ms", 12.0);

        let snap = r.snapshot(1.0);
        // Preseeded-but-never-observed gauge is skipped; the observed one is in.
        assert!(
            snap.gauges
                .iter()
                .all(|g| g.name != "runtime.frame_time.ms")
        );
        assert!(
            snap.gauges
                .iter()
                .any(|g| g.name == "runtime.entity.count" && g.last == 42.0)
        );
        assert!(
            snap.counters
                .iter()
                .any(|c| c.name == "net.peer.connected_count" && c.value == 3)
        );
        assert_eq!(snap.histograms.len(), 1);

        // Round-trips as an event payload for the session log.
        let line = serde_json::to_string(&snap).unwrap();
        let back: MetricSnapshot = serde_json::from_str(&line).unwrap();
        assert_eq!(snap, back);
    }

    #[test]
    fn registry_observe_and_clear() {
        let mut r = MetricsRegistry::default();
        r.observe_gauge("runtime.frame_time.ms", 16.6);
        r.observe_hist("net.jitter.playout_latency_ms", 12.0);
        assert_eq!(r.gauge("runtime.frame_time.ms").unwrap().last(), 16.6);
        assert!(r.hist_distro("net.jitter.playout_latency_ms").is_some());
        r.clear();
        assert!(r.gauge("runtime.frame_time.ms").is_none());
        assert!(r.hist_distro("net.jitter.playout_latency_ms").is_none());
    }

    #[test]
    fn gui_read_surface_readers() {
        let mut r = MetricsRegistry::default();
        r.preseed(&[
            ("runtime.frame_time.ms", MetricKind::Gauge),
            ("runtime.collider.count", MetricKind::Gauge),
            ("net.peer.connected_count", MetricKind::Counter),
            ("net.jitter.playout_latency_ms", MetricKind::Histogram),
        ]);
        // Never-observed / never-incremented → the GUI shows "—" / 0, not stale data.
        assert_eq!(r.gauge_latest("runtime.frame_time.ms"), None);
        assert!(r.gauge_distro("runtime.frame_time.ms").is_none());
        assert_eq!(r.counter_value("net.peer.connected_count"), 0);
        assert_eq!(r.hist_distro_str("net.jitter.playout_latency_ms"), "—");

        for v in [16.0, 18.0, 20.0, 22.0].iter() {
            r.observe_gauge("runtime.frame_time.ms", *v);
        }
        r.observe_gauge("runtime.collider.count", 0.0); // an OBSERVED zero
        r.incr_by("net.peer.connected_count", 3);
        r.observe_hist("net.jitter.playout_latency_ms", 100.0);

        assert_eq!(r.gauge_latest("runtime.frame_time.ms"), Some(22.0));
        // An observed zero is Some(0.0), distinct from never-observed None.
        assert_eq!(r.gauge_latest("runtime.collider.count"), Some(0.0));
        assert_eq!(r.counter_value("net.peer.connected_count"), 3);
        let d = r.gauge_distro("runtime.frame_time.ms").unwrap();
        assert_eq!((d.min, d.max, d.n), (16.0, 22.0, 4));
        assert!(
            r.hist_distro_str("net.jitter.playout_latency_ms")
                .contains("100")
        );

        // Unknown names never panic.
        assert_eq!(r.gauge_latest("nope"), None);
        assert_eq!(r.counter_value("nope"), 0);
        assert_eq!(r.hist_distro_str("nope"), "—");
    }
}
