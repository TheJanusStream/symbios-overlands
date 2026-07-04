//! Report sections: `[Timeline]`, `[Loading Gate]` (+ per-stage
//! distros), `[Event Tallies]`, `[Metric Trends]`, and the
//! `write_sections` tail assembler.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::diagnostics::event::{
    Category, EventPayload, FetchStatus, SessionEvent, Severity, Subsystem,
};
use crate::diagnostics::registry::{Distro, HistPoint, MetricSnapshot, distro};

use super::parse::*;
use super::{plural, round1};

/// Cap on `[Timeline]` rows so a pathological log (thousands of portal hops)
/// can't grow the report without bound; the overflow is summarised.
pub(super) const TIMELINE_MAX: usize = 60;

/// A short label for the events that define the session's *shape* — the ones the
/// `[Timeline]` renders at their timestamp. `None` for the high-frequency /
/// detail events (metric snapshots, per-peer transforms, chat, …) that would
/// bury the milestones.
pub(super) fn timeline_label(p: &EventPayload) -> Option<String> {
    use EventPayload::*;
    Some(match p {
        StartupSnapshot(s) => format!("startup ({:?})", s.phase),
        LoadingPhaseStarted => "loading gate opened".to_string(),
        RecordFetchCompleted { record, status, .. } => format!("{record:?} fetch {status:?}"),
        RecordWriteCompleted { record, .. } => format!("{record:?} saved to PDS"),
        RecordWriteFailed { record, .. } => format!("{record:?} save FAILED"),
        HeightmapGenCompleted { .. } => "heightmap generated".to_string(),
        AmbientBakeCompleted { .. } => "ambient bake done".to_string(),
        AmbientBakeFallback { .. } => "ambient bake fell back to silence".to_string(),
        WorldCompileCompleted { entity_count, .. } => {
            format!("world compiled ({entity_count} entities)")
        }
        AvatarReseeded { seed } => format!("avatar reseeded (seed {seed})"),
        LoadingGateTransitionToInGame { elapsed_secs } => format!("→ InGame ({elapsed_secs:.1}s)"),
        PortalTravelInitiated { target_did } => format!("portal → {target_did}"),
        PortalTravelCompleted { target_did } => format!("portal arrived {target_did}"),
        PortalTravelFailed { target_did, .. } => format!("portal → {target_did} FAILED"),
        SessionSegmentReset { reason } => format!("segment reset ({reason})"),
        SessionEnd { reason } => format!("session end ({reason})"),
        _ => return None,
    })
}

/// The `[Timeline]` section: the milestone events (see [`timeline_label`]) at
/// their session-relative timestamps, so an agent can see the run's arc at a
/// glance. Capped at [`TIMELINE_MAX`] with an explicit overflow line — never a
/// silent truncation.
pub(super) fn write_timeline(s: &mut String, events: &[SessionEvent]) {
    let rows: Vec<(f64, String)> = events
        .iter()
        .filter_map(|e| timeline_label(&e.payload).map(|l| (e.t_mono_secs, l)))
        .collect();
    let _ = writeln!(s);
    let _ = writeln!(s, "[Timeline]");
    if rows.is_empty() {
        let _ = writeln!(s, "  (no milestone events)");
        return;
    }
    let shown = rows.len().min(TIMELINE_MAX);
    for (t, label) in rows.iter().take(shown) {
        let _ = writeln!(s, "  {t:>8.1}s  {label}");
    }
    if rows.len() > shown {
        let _ = writeln!(s, "  … {} more milestone(s)", rows.len() - shown);
    }
}

/// Extracts one loading stage's `duration_secs` from a payload (`None` if the
/// payload isn't that stage's completion). The [`LOADING_STAGES`] table pairs
/// each with a label so the single-session `[Loading Gate]` section and the
/// `--diff-sessions` `[Loading Gate Delta]` fold the *same* stage set the same
/// way (the sibling-section-consistency rule).
type StagePick = fn(&EventPayload) -> Option<f64>;

pub(super) fn stage_record_fetch(p: &EventPayload) -> Option<f64> {
    match p {
        // Success-only, to match the other three stages: a decode-failure /
        // exhausted / best-effort fetch also emits a `RecordFetchCompleted`, but
        // its (often near-timeout) latency would skew a "how long did fetches
        // take" distro — the failure surfaces in the verdict + invariants instead.
        EventPayload::RecordFetchCompleted {
            duration_secs,
            status: FetchStatus::Ok | FetchStatus::NotFound,
            ..
        } => Some(*duration_secs),
        _ => None,
    }
}

pub(super) fn stage_heightmap(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::HeightmapGenCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

pub(super) fn stage_ambient_bake(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::AmbientBakeCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

pub(super) fn stage_world_compile(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::WorldCompileCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

/// The four heavy loading stages the session log times, in report order.
pub(super) const LOADING_STAGES: &[(&str, StagePick)] = &[
    ("record fetch", stage_record_fetch),
    ("heightmap", stage_heightmap),
    ("ambient bake", stage_ambient_bake),
    ("world compile", stage_world_compile),
];

/// The gate elapsed stamped on the Loading → InGame transition, if it happened.
pub(super) fn gate_total_secs(events: &[SessionEvent]) -> Option<f64> {
    events.iter().find_map(|e| match &e.payload {
        EventPayload::LoadingGateTransitionToInGame { elapsed_secs } => Some(*elapsed_secs),
        _ => None,
    })
}

/// The `min/p50/p90/max/mean` distro of every `duration_secs` `pick` extracts,
/// via the shared [`distro`] reducer (`None` when the stage never ran).
///
/// [`distro`]: crate::diagnostics::registry::distro
pub(super) fn stage_distro(events: &[SessionEvent], pick: StagePick) -> Option<Distro> {
    let v: Vec<f64> = events.iter().filter_map(|e| pick(&e.payload)).collect();
    distro(&v)
}

/// One stage-timing line, or `—` when the stage never ran in this log.
pub(super) fn write_stage_distro(
    s: &mut String,
    label: &str,
    events: &[SessionEvent],
    pick: StagePick,
) {
    match stage_distro(events, pick) {
        Some(d) => {
            let _ = writeln!(s, "  {label:<14} {d}  (n={})", d.n);
        }
        None => {
            let _ = writeln!(s, "  {label:<14} —");
        }
    }
}

/// The `[Loading Gate]` section: the Login → Loading → InGame gate time plus a
/// per-stage duration distro for the four heavy loading stages the session log
/// times (record fetch / heightmap / ambient bake / world compile).
pub(super) fn write_loading_gate(s: &mut String, events: &[SessionEvent]) {
    let _ = writeln!(s);
    let _ = writeln!(s, "[Loading Gate]");
    let _ = writeln!(s, "  {}", gate_line(events));
    for (label, pick) in LOADING_STAGES {
        write_stage_distro(s, label, events, *pick);
    }
}

/// The `Loading → InGame:` line for the gate section — the elapsed if it
/// happened, else why it didn't (stalled/truncated vs no gate at all). Shared by
/// the single-session section and the diff so the two phrase it identically.
pub(super) fn gate_line(events: &[SessionEvent]) -> String {
    match gate_total_secs(events) {
        // round1 (not a bare {:.1}) so the single-session gate line and the
        // diff's opt_secs render the same elapsed identically at half-tenths.
        Some(secs) => format!("Loading → InGame:  {:.1}s", round1(secs)),
        None => {
            let started = events
                .iter()
                .any(|e| matches!(e.payload, EventPayload::LoadingPhaseStarted));
            let why = if started {
                "did not reach InGame (stalled or truncated log)"
            } else {
                "no loading gate in this log"
            };
            format!("Loading → InGame:  — ({why})")
        }
    }
}

// --- per-subsystem event tallies (B-3) --------------------------------------

/// Subsystems in report row-order. The paired [`subsystem_index`] is an
/// *exhaustive* match, so adding a [`Subsystem`] variant is a compile error
/// there — forcing a new index arm. If this array is not extended to match, the
/// tally still can't panic or silently drop: [`write_event_tallies`] folds any
/// out-of-range event into a surfaced `unclassified` count. The
/// `order_arrays_match_their_index` test pins each array to its index fn so a
/// reorder/insert can't misalign a column.
pub(super) const SUBSYSTEM_ORDER: [Subsystem; 5] = [
    Subsystem::Loading,
    Subsystem::Network,
    Subsystem::Offload,
    Subsystem::Runtime,
    Subsystem::Session,
];

pub(super) fn subsystem_index(s: Subsystem) -> usize {
    match s {
        Subsystem::Loading => 0,
        Subsystem::Network => 1,
        Subsystem::Offload => 2,
        Subsystem::Runtime => 3,
        Subsystem::Session => 4,
    }
}

/// Severities in report column-order (low → high). See [`severity_index`].
pub(super) const SEVERITY_ORDER: [Severity; 5] = [
    Severity::Trace,
    Severity::Info,
    Severity::Warn,
    Severity::Error,
    Severity::Critical,
];

pub(super) fn severity_index(s: Severity) -> usize {
    match s {
        Severity::Trace => 0,
        Severity::Info => 1,
        Severity::Warn => 2,
        Severity::Error => 3,
        Severity::Critical => 4,
    }
}

/// The short column label for a severity in the tally matrix header.
pub(super) fn severity_short(s: Severity) -> &'static str {
    match s {
        Severity::Trace => "trace",
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
        Severity::Critical => "crit",
    }
}

/// Every [`Category`] in report order (index via [`category_index`]).
pub(super) const CATEGORY_ORDER: [Category; 16] = [
    Category::Lifecycle,
    Category::Fetch,
    Category::Generation,
    Category::Audio,
    Category::Peer,
    Category::Transport,
    Category::Offer,
    Category::Chat,
    Category::Social,
    Category::Job,
    Category::Physics,
    Category::Asset,
    Category::Perf,
    Category::Portal,
    Category::Anomaly,
    Category::Snapshot,
];

pub(super) fn category_index(c: Category) -> usize {
    match c {
        Category::Lifecycle => 0,
        Category::Fetch => 1,
        Category::Generation => 2,
        Category::Audio => 3,
        Category::Peer => 4,
        Category::Transport => 5,
        Category::Offer => 6,
        Category::Chat => 7,
        Category::Social => 8,
        Category::Job => 9,
        Category::Physics => 10,
        Category::Asset => 11,
        Category::Perf => 12,
        Category::Portal => 13,
        Category::Anomaly => 14,
        Category::Snapshot => 15,
    }
}

/// The `[Event Tallies]` section: a subsystem × severity matrix plus a
/// by-category count line, so an agent can see *where* a session's noise came
/// from (peer churn under Network, offload failures, runtime respawns, …)
/// without reading every line. The 1 Hz `MetricsSnapshot` records — session
/// bookkeeping, kept out of the timeline too — are excluded from the matrix (and
/// the count noted) so they don't bury the notable-event counts; their data
/// drives the `[Metric Trends]` section instead.
pub(super) fn write_event_tallies(s: &mut String, events: &[SessionEvent]) {
    let snapshots = events
        .iter()
        .filter(|e| matches!(e.payload, EventPayload::MetricsSnapshot(_)))
        .count();

    // [subsystem][severity] counts + a flat by-category tally, both over the
    // non-snapshot events. The array sizes are driven by the ORDER arrays so a
    // new severity/subsystem/category widens the matrix rather than overflowing
    // it; the accesses are bounds-checked so that even if a future variant is
    // added to an enum + its index match but *not* to the ORDER array, its
    // events fall into a surfaced `unclassified` bucket rather than panicking
    // report() (the index fns are exhaustive matches, so the omission is caught
    // at compile time first — this is belt-and-suspenders, never silent).
    let mut matrix = [[0usize; SEVERITY_ORDER.len()]; SUBSYSTEM_ORDER.len()];
    let mut by_cat = [0usize; CATEGORY_ORDER.len()];
    let mut unclassified = 0usize;
    for e in events {
        if matches!(e.payload, EventPayload::MetricsSnapshot(_)) {
            continue;
        }
        let cell = matrix
            .get_mut(subsystem_index(e.subsystem))
            .and_then(|row| row.get_mut(severity_index(e.severity)));
        match (cell, by_cat.get_mut(category_index(e.category))) {
            (Some(cell), Some(cat)) => {
                *cell += 1;
                *cat += 1;
            }
            // Only reachable if an ORDER array drifted behind its enum; counted
            // and surfaced below, so the totals stay self-consistent.
            _ => unclassified += 1,
        }
    }

    let _ = writeln!(s);
    let excluded = if snapshots > 0 {
        format!(
            "  (excluding {snapshots} metric snapshot{})",
            plural(snapshots)
        )
    } else {
        String::new()
    };
    let _ = writeln!(s, "[Event Tallies]{excluded}");

    let grand: usize = matrix.iter().flatten().sum();
    if grand == 0 {
        if unclassified > 0 {
            let _ = writeln!(s, "  {unclassified} unclassified event(s)");
        } else {
            let _ = writeln!(s, "  (no non-snapshot events)");
        }
        return;
    }

    // subsystem × severity matrix — only rows with events, plus a totals row.
    let mut header = format!("  {:<10}", "subsystem");
    for sev in SEVERITY_ORDER {
        let _ = write!(header, "{:>7}", severity_short(sev));
    }
    let _ = writeln!(header, "{:>8}", "total");
    let _ = write!(s, "{header}");

    let mut col_totals = [0usize; SEVERITY_ORDER.len()];
    for (si, sub) in SUBSYSTEM_ORDER.iter().enumerate() {
        let row = matrix[si];
        let row_total: usize = row.iter().sum();
        if row_total == 0 {
            continue;
        }
        let mut line = format!("  {:<10}", format!("{sub:?}"));
        for (c, n) in row.iter().enumerate() {
            col_totals[c] += n;
            let _ = write!(line, "{n:>7}");
        }
        let _ = writeln!(line, "{row_total:>8}");
        let _ = write!(s, "{line}");
    }
    let mut totals = format!("  {:<10}", "total");
    for n in col_totals {
        let _ = write!(totals, "{n:>7}");
    }
    let _ = writeln!(totals, "{grand:>8}");
    let _ = write!(s, "{totals}");

    // by-category — the non-zero categories, busiest first, as a compact line.
    let mut cats: Vec<(Category, usize)> = CATEGORY_ORDER
        .iter()
        .enumerate()
        .filter(|(i, _)| by_cat[*i] > 0)
        .map(|(i, c)| (*c, by_cat[i]))
        .collect();
    cats.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| category_index(a.0).cmp(&category_index(b.0)))
    });
    let line: Vec<String> = cats.iter().map(|(c, n)| format!("{c:?} {n}")).collect();
    let _ = writeln!(s, "  by category:  {}", line.join("   "));

    if unclassified > 0 {
        let _ = writeln!(
            s,
            "  ({unclassified} unclassified — a taxonomy variant is missing from an analyzer order table)"
        );
    }
}

// --- metric-series trends (B-3) ---------------------------------------------

/// Human-readable byte size (`256.0 MB`) for the memory-gauge trend rows. Kept
/// local — `analyze.rs` is the always-compiled, dependency-light module, so it
/// can't reach the GUI's `fmt_bytes` (which lives in the native-only `ui`).
pub(super) fn fmt_bytes(v: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut val = v.max(0.0);
    let mut u = 0;
    while val >= 1024.0 && u < UNITS.len() - 1 {
        val /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", val.round() as u64, UNITS[u])
    } else {
        format!("{val:.1} {}", UNITS[u])
    }
}

/// The `[Metric Trends]` section: charts the periodic [`MetricSnapshot`] records
/// the session log writes (E-5) so a post-mortem can see slow drift the verdict
/// can't — the memory-growth curve, frame-time percentiles over the run, and
/// entity/asset-count drift (the leak signal). Gauges show first → last plus the
/// [`distro`] over the whole run; counters show first → last with growth;
/// histograms show their final accumulated distribution. Every metric that
/// appears in any snapshot is charted, name-sorted (so a subsystem's metrics
/// group together); the set is bounded by `names::ALL`.
pub(super) fn write_metric_trends(s: &mut String, events: &[SessionEvent]) {
    let _ = writeln!(s);
    let _ = writeln!(s, "[Metric Trends]");

    let snaps: Vec<&MetricSnapshot> = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::MetricsSnapshot(snap) => Some(snap.as_ref()),
            _ => None,
        })
        .collect();
    if snaps.is_empty() {
        let _ = writeln!(s, "  (no metric snapshots in this log)");
        return;
    }
    let lo = snaps.first().map(|s| s.at_secs).unwrap_or(0.0);
    let hi = snaps.last().map(|s| s.at_secs).unwrap_or(0.0);
    let _ = writeln!(
        s,
        "  over {} snapshot{} ({lo:.1}s–{hi:.1}s)",
        snaps.len(),
        plural(snaps.len())
    );

    // Gauges: the chronological series of `last` values per name.
    let mut gauges: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for snap in &snaps {
        for g in &snap.gauges {
            gauges.entry(g.name.as_str()).or_default().push(g.last);
        }
    }
    if !gauges.is_empty() {
        let _ = writeln!(s, "  gauges (first → last, distribution over the run):");
        for (name, series) in &gauges {
            let first = series.first().copied().unwrap_or(0.0);
            let last = series.last().copied().unwrap_or(0.0);
            let d = distro(series);
            if name.ends_with("bytes") {
                // Byte gauges (memory) are humanized so the growth curve — the
                // headline leak signal — is readable rather than a wall of digits.
                let dist = d
                    .map(|d| {
                        format!(
                            "min {}  p50 {}  p90 {}  max {}  mean {}",
                            fmt_bytes(d.min),
                            fmt_bytes(d.p50),
                            fmt_bytes(d.p90),
                            fmt_bytes(d.max),
                            fmt_bytes(d.mean)
                        )
                    })
                    .unwrap_or_else(|| "—".to_string());
                let _ = writeln!(
                    s,
                    "    {name:<34} {:>12} → {:<12} {dist}  (n={})",
                    fmt_bytes(first),
                    fmt_bytes(last),
                    series.len()
                );
            } else {
                let dist = d.map(|d| d.to_string()).unwrap_or_else(|| "—".to_string());
                let _ = writeln!(
                    s,
                    "    {name:<34} {first:>12.1} → {last:<12.1} {dist}  (n={})",
                    series.len()
                );
            }
        }
    }

    // Counters: first non-zero snapshot value → last, with the captured growth.
    let mut counters: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for snap in &snaps {
        for c in &snap.counters {
            counters
                .entry(c.name.as_str())
                .and_modify(|e| e.1 = c.value)
                .or_insert((c.value, c.value));
        }
    }
    if !counters.is_empty() {
        let _ = writeln!(s, "  counters (first → last):");
        for (name, (first, last)) in &counters {
            let growth = last.saturating_sub(*first);
            let _ = writeln!(s, "    {name:<34} {first:>12} → {last:<12} (+{growth})");
        }
    }

    // Histograms: the final accumulated distribution per name. These are the
    // sampler-fed metric histograms in ms — distinct from the [Loading Gate]
    // stage timings, which are timed from the lifecycle events in secs.
    let mut hists: BTreeMap<&str, &HistPoint> = BTreeMap::new();
    for snap in &snaps {
        for h in &snap.histograms {
            hists.insert(h.name.as_str(), h);
        }
    }
    if !hists.is_empty() {
        let _ = writeln!(s, "  histograms (final distribution):");
        for (name, h) in &hists {
            let d = Distro {
                min: h.min,
                p50: h.p50,
                p90: h.p90,
                max: h.max,
                mean: h.mean,
                n: h.n,
            };
            let _ = writeln!(s, "    {name:<34} {d}  (n={})", h.n);
        }
    }
}

// --- analyzer filters (B-5) -------------------------------------------------

/// Emit the analysis sections (`[Verdict]` … `[Invariant Violations]`) over
/// `events` — the full log, or a [`super::Filters`]-selected subset. Split out so the
/// filtered and unfiltered reports share one section pipeline.
pub(super) fn write_sections(s: &mut String, events: &[SessionEvent]) {
    // -- verdict --------------------------------------------------------------
    let [warn, error, crit] = severity_tally(events);
    let _ = writeln!(s);
    let _ = writeln!(s, "[Verdict]");
    if warn + error + crit == 0 {
        let _ = writeln!(s, "  HEALTHY — no warnings, errors or critical events");
    } else {
        let mut parts = Vec::new();
        if crit > 0 {
            parts.push(format!("{crit} critical"));
        }
        if error > 0 {
            parts.push(format!("{error} error{}", plural(error)));
        }
        if warn > 0 {
            parts.push(format!("{warn} warning{}", plural(warn)));
        }
        let _ = writeln!(s, "  {}", parts.join(", "));
    }

    // -- per-subsystem event tallies (B-3) ------------------------------------
    write_event_tallies(s, events);

    // -- timeline + loading-gate stage timing (B-2) ---------------------------
    write_timeline(s, events);
    write_loading_gate(s, events);

    // -- metric-series trends (B-3) -------------------------------------------
    write_metric_trends(s, events);

    // -- invariant violations (D-5) -------------------------------------------
    // The offline counterpart to the live anomaly engine: replay the shared rule
    // set over the stream + surface captured live-only fires.
    let _ = writeln!(s);
    let _ = write!(
        s,
        "{}",
        crate::diagnostics::anomaly::replay::replay_invariants(events)
    );
}

// === before/after session diff (B-4) ========================================
//
// `--diff-sessions <a> <b>` prints a delta between a baseline run (A) and a
// candidate run (B) so an agent can confirm a fix improved on the baseline. The
// builders are pure over two `&ParsedLog` (unit-tested without file IO), and
// reuse the same reducers the single-session report does — `severity_tally`,
// the `LOADING_STAGES` table + `stage_distro`, and `replay_findings` — so a
// stage or invariant is folded identically in both reports.
