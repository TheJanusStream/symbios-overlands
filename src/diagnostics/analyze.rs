//! Offline session-log analyzer (Pillar B) — the agent-facing post-mortem the
//! native `render` bin prints for `--analyze-session <log.jsonl>`.
//!
//! It reads a captured NDJSON session log (the durable
//! `diagnostics/session-latest.jsonl` a live run writes, or the wasm
//! "Download log" dump — byte-identical formats), deserializes it back into
//! [`SessionEvent`]s tolerating unknown/torn lines, and folds it into a
//! human-readable report: the header + verdict (B-1), the per-subsystem
//! `[Event Tallies]` + `[Metric Trends]` (B-3), the `[Timeline]` +
//! `[Loading Gate]` per-stage timings (B-2), and the replayed
//! `[Invariant Violations]` section (D-5). [`diff_report`] adds the
//! `--diff-sessions <a> <b>` before/after comparison (B-4), and [`Filters`] /
//! [`report_with`] add the `--analyze-session` subsystem / category / severity /
//! time-window filters (B-5).
//!
//! The report builders are pure over `&[SessionEvent]`, so they unit-test
//! without any file IO or a Bevy `App`; the native `render_tool` supplies the
//! file read. The report format follows the `urban/diagnostics.rs` road-dump
//! idiom (`=== header ===` + labelled sections).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::diagnostics::event::{
    Category, EventPayload, FetchStatus, SessionEvent, Severity, StartupInfo, Subsystem,
};
use crate::diagnostics::registry::{Distro, HistPoint, MetricSnapshot, distro};

/// A session log parsed from NDJSON, plus the count of lines that failed to
/// deserialize (an unknown/renamed variant from a newer build, or a torn final
/// line from a crash) — surfaced in the report so a truncated log is never
/// silently analyzed as if it were complete.
pub struct ParsedLog {
    pub events: Vec<SessionEvent>,
    pub unparseable: usize,
}

/// Parse an NDJSON session log line-by-line. Each line is deserialized
/// independently, so one bad line (an unknown `kind` from a newer schema, or a
/// half-written tail line after a crash) is skipped and counted rather than
/// aborting the whole analysis. Blank lines are ignored (not counted).
pub fn parse_ndjson(text: &str) -> ParsedLog {
    let mut events = Vec::new();
    let mut unparseable = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<SessionEvent>(line) {
            Ok(ev) => events.push(ev),
            Err(_) => unparseable += 1,
        }
    }
    ParsedLog {
        events,
        unparseable,
    }
}

/// The most informative startup snapshot: prefer a `Session`-phase record (its
/// `session_did` / relay are filled in), else fall back to the first `Boot`
/// snapshot (build info is identical, only the DID differs).
fn startup(events: &[SessionEvent]) -> Option<&StartupInfo> {
    let mut boot = None;
    for e in events {
        if let EventPayload::StartupSnapshot(info) = &e.payload {
            if info.session_did.is_some() {
                return Some(info);
            }
            boot.get_or_insert(info.as_ref());
        }
    }
    boot
}

/// The reason of the last `SessionEnd` record, if the session ended cleanly.
fn session_end(events: &[SessionEvent]) -> Option<&str> {
    events.iter().rev().find_map(|e| match &e.payload {
        EventPayload::SessionEnd { reason } => Some(reason.as_str()),
        _ => None,
    })
}

/// Count events at `Warn` / `Error` / `Critical` — the top-line health signal.
fn severity_tally(events: &[SessionEvent]) -> [usize; 3] {
    let mut t = [0usize; 3];
    for e in events {
        match e.severity {
            Severity::Warn => t[0] += 1,
            Severity::Error => t[1] += 1,
            Severity::Critical => t[2] += 1,
            _ => {}
        }
    }
    t
}

/// Session wall-clock span: last minus first `t_mono_secs` (session-relative,
/// so the first event is ~0 and this is effectively the session length).
fn duration_secs(events: &[SessionEvent]) -> f64 {
    match (events.first(), events.last()) {
        (Some(a), Some(b)) => b.t_mono_secs - a.t_mono_secs,
        _ => 0.0,
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Cap on `[Timeline]` rows so a pathological log (thousands of portal hops)
/// can't grow the report without bound; the overflow is summarised.
const TIMELINE_MAX: usize = 60;

/// A short label for the events that define the session's *shape* — the ones the
/// `[Timeline]` renders at their timestamp. `None` for the high-frequency /
/// detail events (metric snapshots, per-peer transforms, chat, …) that would
/// bury the milestones.
fn timeline_label(p: &EventPayload) -> Option<String> {
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
fn write_timeline(s: &mut String, events: &[SessionEvent]) {
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

fn stage_record_fetch(p: &EventPayload) -> Option<f64> {
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

fn stage_heightmap(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::HeightmapGenCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

fn stage_ambient_bake(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::AmbientBakeCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

fn stage_world_compile(p: &EventPayload) -> Option<f64> {
    match p {
        EventPayload::WorldCompileCompleted { duration_secs, .. } => Some(*duration_secs),
        _ => None,
    }
}

/// The four heavy loading stages the session log times, in report order.
const LOADING_STAGES: &[(&str, StagePick)] = &[
    ("record fetch", stage_record_fetch),
    ("heightmap", stage_heightmap),
    ("ambient bake", stage_ambient_bake),
    ("world compile", stage_world_compile),
];

/// The gate elapsed stamped on the Loading → InGame transition, if it happened.
fn gate_total_secs(events: &[SessionEvent]) -> Option<f64> {
    events.iter().find_map(|e| match &e.payload {
        EventPayload::LoadingGateTransitionToInGame { elapsed_secs } => Some(*elapsed_secs),
        _ => None,
    })
}

/// The `min/p50/p90/max/mean` distro of every `duration_secs` `pick` extracts,
/// via the shared [`distro`] reducer (`None` when the stage never ran).
///
/// [`distro`]: crate::diagnostics::registry::distro
fn stage_distro(events: &[SessionEvent], pick: StagePick) -> Option<Distro> {
    let v: Vec<f64> = events.iter().filter_map(|e| pick(&e.payload)).collect();
    distro(&v)
}

/// One stage-timing line, or `—` when the stage never ran in this log.
fn write_stage_distro(s: &mut String, label: &str, events: &[SessionEvent], pick: StagePick) {
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
fn write_loading_gate(s: &mut String, events: &[SessionEvent]) {
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
fn gate_line(events: &[SessionEvent]) -> String {
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
const SUBSYSTEM_ORDER: [Subsystem; 5] = [
    Subsystem::Loading,
    Subsystem::Network,
    Subsystem::Offload,
    Subsystem::Runtime,
    Subsystem::Session,
];

fn subsystem_index(s: Subsystem) -> usize {
    match s {
        Subsystem::Loading => 0,
        Subsystem::Network => 1,
        Subsystem::Offload => 2,
        Subsystem::Runtime => 3,
        Subsystem::Session => 4,
    }
}

/// Severities in report column-order (low → high). See [`severity_index`].
const SEVERITY_ORDER: [Severity; 5] = [
    Severity::Trace,
    Severity::Info,
    Severity::Warn,
    Severity::Error,
    Severity::Critical,
];

fn severity_index(s: Severity) -> usize {
    match s {
        Severity::Trace => 0,
        Severity::Info => 1,
        Severity::Warn => 2,
        Severity::Error => 3,
        Severity::Critical => 4,
    }
}

/// The short column label for a severity in the tally matrix header.
fn severity_short(s: Severity) -> &'static str {
    match s {
        Severity::Trace => "trace",
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Error => "error",
        Severity::Critical => "crit",
    }
}

/// Every [`Category`] in report order (index via [`category_index`]).
const CATEGORY_ORDER: [Category; 16] = [
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

fn category_index(c: Category) -> usize {
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
fn write_event_tallies(s: &mut String, events: &[SessionEvent]) {
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
fn fmt_bytes(v: f64) -> String {
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
fn write_metric_trends(s: &mut String, events: &[SessionEvent]) {
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

/// Filters for `--analyze-session`: restrict the report's *analysis* sections to
/// events matching a subsystem / category / minimum severity / time window. Every
/// field is optional; an unset field doesn't filter, and an all-`None` filter is
/// a no-op passthrough. The header (session identity) is never filtered — it
/// identifies the run — see [`report_with`]. Built from CLI strings by
/// [`Filters::parse`]; applied purely, so it unit-tests without file IO.
#[derive(Default, Clone, Debug)]
pub struct Filters {
    pub subsystem: Option<Subsystem>,
    pub category: Option<Category>,
    /// Minimum severity: an event matches if its severity is ≥ this.
    pub min_severity: Option<Severity>,
    /// Inclusive lower bound on `t_mono_secs`.
    pub since: Option<f64>,
    /// Inclusive upper bound on `t_mono_secs`.
    pub until: Option<f64>,
}

fn parse_subsystem(s: &str) -> Option<Subsystem> {
    match s.to_ascii_lowercase().as_str() {
        "loading" => Some(Subsystem::Loading),
        "network" | "net" => Some(Subsystem::Network),
        "offload" => Some(Subsystem::Offload),
        "runtime" => Some(Subsystem::Runtime),
        "session" => Some(Subsystem::Session),
        _ => None,
    }
}

fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "trace" => Some(Severity::Trace),
        "info" => Some(Severity::Info),
        "warn" | "warning" => Some(Severity::Warn),
        "error" => Some(Severity::Error),
        "critical" | "crit" => Some(Severity::Critical),
        _ => None,
    }
}

fn parse_category(s: &str) -> Option<Category> {
    let s = s.to_ascii_lowercase();
    CATEGORY_ORDER
        .iter()
        .copied()
        .find(|c| format!("{c:?}").to_ascii_lowercase() == s)
}

impl Filters {
    /// Whether any filter is set (all-`None` = passthrough).
    pub fn is_active(&self) -> bool {
        self.subsystem.is_some()
            || self.category.is_some()
            || self.min_severity.is_some()
            || self.since.is_some()
            || self.until.is_some()
    }

    /// Whether `e` passes every set filter.
    pub fn matches(&self, e: &SessionEvent) -> bool {
        if self.subsystem.is_some_and(|sub| e.subsystem != sub) {
            return false;
        }
        if self.category.is_some_and(|cat| e.category != cat) {
            return false;
        }
        if self.min_severity.is_some_and(|min| e.severity < min) {
            return false;
        }
        if self.since.is_some_and(|since| e.t_mono_secs < since) {
            return false;
        }
        if self.until.is_some_and(|until| e.t_mono_secs > until) {
            return false;
        }
        true
    }

    /// A human summary of the active filters for the `[Filter]` header line.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(sub) = self.subsystem {
            parts.push(format!("subsystem={sub:?}"));
        }
        if let Some(cat) = self.category {
            parts.push(format!("category={cat:?}"));
        }
        if let Some(min) = self.min_severity {
            parts.push(format!("severity≥{min:?}"));
        }
        match (self.since, self.until) {
            (Some(a), Some(b)) => parts.push(format!("t∈[{a:.1}s, {b:.1}s]")),
            (Some(a), None) => parts.push(format!("t≥{a:.1}s")),
            (None, Some(b)) => parts.push(format!("t≤{b:.1}s")),
            (None, None) => {}
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Parse CLI filter strings into a [`Filters`], returning a clear error for
    /// an unknown subsystem / category / severity name (case-insensitive).
    pub fn parse(
        subsystem: Option<&str>,
        category: Option<&str>,
        severity: Option<&str>,
        since: Option<f64>,
        until: Option<f64>,
    ) -> Result<Filters, String> {
        let subsystem = match subsystem {
            Some(s) => Some(parse_subsystem(s).ok_or_else(|| {
                format!("unknown subsystem {s:?} (loading|network|offload|runtime|session)")
            })?),
            None => None,
        };
        let category = match category {
            Some(s) => Some(
                parse_category(s)
                    .ok_or_else(|| format!("unknown category {s:?} (see docs/diagnostics.md)"))?,
            ),
            None => None,
        };
        let min_severity = match severity {
            Some(s) => Some(parse_severity(s).ok_or_else(|| {
                format!("unknown severity {s:?} (trace|info|warn|error|critical)")
            })?),
            None => None,
        };
        Ok(Filters {
            subsystem,
            category,
            min_severity,
            since,
            until,
        })
    }
}

/// Build the post-mortem report for a parsed session log. `path` is echoed in
/// the header so a multi-log analysis is self-labelling. Pure over its inputs.
pub fn report(path: &str, log: &ParsedLog) -> String {
    report_with(path, log, &Filters::default())
}

/// Like [`report`], but restricts the analysis sections to the events matching
/// `filters` (B-5). The header (session id, build, duration, exit) is always
/// derived from the *full* log — it identifies the run — while `[Verdict]` …
/// `[Invariant Violations]` fold only the matching subset, with a `[Filter]` line
/// documenting the lens and how many events matched. Pure over its inputs.
pub fn report_with(path: &str, log: &ParsedLog, filters: &Filters) -> String {
    let full = &log.events;
    let mut s = String::new();
    let _ = writeln!(s, "=== session analysis — {path} ===");

    if full.is_empty() {
        let _ = writeln!(
            s,
            "empty log: no parseable events ({} unparseable line(s))",
            log.unparseable
        );
        return s;
    }

    // -- header (always the FULL log — it identifies the session) -------------
    let start = startup(full);
    // The session id mirrors `SessionLog::session_start_wall_ms` — the wall
    // stamp of the *first* recorded event, which keys the per-session file. If
    // the first event carried no clock, there is no session id (`—`); we don't
    // borrow a later event's stamp, which would misidentify the run.
    let session_id = full
        .first()
        .and_then(|e| e.wall_ms)
        .map(|ms| ms.to_string())
        .unwrap_or_else(|| "—".to_string());
    let did = start
        .and_then(|s| s.session_did.clone().or_else(|| s.boot_target_did.clone()))
        .unwrap_or_else(|| "—".to_string());
    let _ = writeln!(s, "session-id: {session_id}    did: {did}");

    match start {
        Some(info) => {
            let _ = writeln!(
                s,
                "build:      v{} ({}) {}/{}{}",
                info.version,
                info.git_sha,
                info.target_arch,
                info.profile,
                if info.wasm { " wasm" } else { "" }
            );
        }
        None => {
            let _ = writeln!(s, "build:      — (no StartupSnapshot record)");
        }
    }

    let unparseable = if log.unparseable > 0 {
        format!(", {} unparseable", log.unparseable)
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "duration:   {:.1}s   ({} events{})",
        duration_secs(full),
        full.len(),
        unparseable
    );

    match session_end(full) {
        Some(reason) => {
            let _ = writeln!(s, "exit:       {reason}");
        }
        None => {
            let _ = writeln!(
                s,
                "exit:       — no SessionEnd record (crash or truncated log)"
            );
        }
    }

    // -- filter (B-5): the analysis sections fold only the matching subset -----
    let filtered: Vec<SessionEvent> = if filters.is_active() {
        full.iter()
            .filter(|e| filters.matches(e))
            .cloned()
            .collect()
    } else {
        Vec::new() // unused when inactive; avoids cloning the whole log
    };
    let events: &[SessionEvent] = if filters.is_active() { &filtered } else { full };
    if filters.is_active() {
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "[Filter]  {}  —  {} of {} events match",
            filters.describe(),
            events.len(),
            full.len()
        );
        if events.is_empty() {
            let _ = writeln!(s, "  (no events match — nothing to analyze)");
            return s;
        }
    }

    write_sections(&mut s, events);
    s
}

/// Emit the analysis sections (`[Verdict]` … `[Invariant Violations]`) over
/// `events` — the full log, or a [`Filters`]-selected subset. Split out so the
/// filtered and unfiltered reports share one section pipeline.
fn write_sections(s: &mut String, events: &[SessionEvent]) {
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

/// A compact one-line session summary for the diff header (build · DID ·
/// duration · exit), tolerant of a torn/short log.
fn session_line(log: &ParsedLog) -> String {
    let events = &log.events;
    let info = startup(events);
    let build = info
        .map(|i| {
            format!(
                "v{} ({}) {}/{}{}",
                i.version,
                i.git_sha,
                i.target_arch,
                i.profile,
                if i.wasm { " wasm" } else { "" }
            )
        })
        .unwrap_or_else(|| "— (no StartupSnapshot)".to_string());
    let did = info
        .and_then(|i| i.session_did.clone().or_else(|| i.boot_target_did.clone()))
        .unwrap_or_else(|| "—".to_string());
    let exit = session_end(events).unwrap_or("— (no SessionEnd)");
    let unp = if log.unparseable > 0 {
        format!(", {} unparseable", log.unparseable)
    } else {
        String::new()
    };
    format!(
        "{build}  ·  {did}  ·  {:.1}s  ·  exit {exit}  ({} event{}{unp})",
        duration_secs(events),
        events.len(),
        plural(events.len())
    )
}

/// The peak (max over the run) of each gauge that appears in the log's metric
/// snapshots — the leak/spike signal to compare across runs.
fn gauge_peaks(events: &[SessionEvent]) -> BTreeMap<String, f64> {
    let mut peaks: BTreeMap<String, f64> = BTreeMap::new();
    for e in events {
        if let EventPayload::MetricsSnapshot(snap) = &e.payload {
            for g in &snap.gauges {
                peaks
                    .entry(g.name.clone())
                    .and_modify(|m| *m = m.max(g.last))
                    .or_insert(g.last);
            }
        }
    }
    peaks
}

/// The final (last-seen) value of each counter across the log's snapshots.
fn counter_totals(events: &[SessionEvent]) -> BTreeMap<String, u64> {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    for e in events {
        if let EventPayload::MetricsSnapshot(snap) = &e.payload {
            for c in &snap.counters {
                totals.insert(c.name.clone(), c.value);
            }
        }
    }
    totals
}

/// Whether the log carries any metric snapshot at all — the difference between
/// "this counter stayed 0" (a genuine 0) and "we have no metric data for this
/// session" (unknown), so the diff never reads a data-less run as an improvement.
fn has_metric_snapshot(events: &[SessionEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e.payload, EventPayload::MetricsSnapshot(_)))
}

/// A signed integer delta (`+2` / `-1` / `0`).
fn signed_i(d: i64) -> String {
    if d > 0 {
        format!("+{d}")
    } else {
        format!("{d}")
    }
}

/// A signed one-decimal delta (`+0.3` / `-1.2`).
fn signed_f(d: f64) -> String {
    if d >= 0.0 {
        format!("+{d:.1}")
    } else {
        format!("{d:.1}")
    }
}

/// Round to one decimal, so a delta is taken between the *displayed* (rounded)
/// operands and the shown `A → B  (Δ)` always adds up (no `0.6 → 0.3  (-0.2)`
/// where the rounded operands imply `-0.3`).
fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Format a metric value for the diff, humanizing byte gauges (see [`fmt_bytes`])
/// and otherwise rendering one decimal — consistent with `[Metric Trends]`.
fn fmt_metric(name: &str, v: f64) -> String {
    if name.ends_with("bytes") {
        fmt_bytes(v)
    } else {
        format!("{v:.1}")
    }
}

/// A signed metric delta (byte-aware): `+80.0 MB`, `-2.3`, `+0.0`.
fn fmt_metric_delta(name: &str, d: f64) -> String {
    let body = fmt_metric(name, d.abs());
    if d >= 0.0 {
        format!("+{body}")
    } else {
        format!("-{body}")
    }
}

/// An `Option<f64>` as `X.Xs` or `—` (rounded to match [`round1`]-based deltas).
fn opt_secs(v: Option<f64>) -> String {
    v.map(|x| format!("{:.1}s", round1(x)))
        .unwrap_or_else(|| "—".to_string())
}

/// The `[Verdict Delta]` section: warn/error/critical counts A → B, plus an
/// overall better/worse/same read (severity-weighted: critical, then error,
/// then warn).
fn write_verdict_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
    let ta = severity_tally(&a.events); // [warn, error, crit]
    let tb = severity_tally(&b.events);
    let _ = writeln!(s);
    let _ = writeln!(s, "[Verdict Delta]");
    for (label, av, bv) in [
        ("critical", ta[2], tb[2]),
        ("errors", ta[1], tb[1]),
        ("warnings", ta[0], tb[0]),
    ] {
        let d = bv as i64 - av as i64;
        let _ = writeln!(s, "  {label:<10} {av:>4} → {bv:<4}  ({})", signed_i(d));
    }
    // Compare (crit, error, warn) tuples — worst axis dominates.
    let a3 = (ta[2], ta[1], ta[0]);
    let b3 = (tb[2], tb[1], tb[0]);
    let read = match b3.cmp(&a3) {
        std::cmp::Ordering::Less => "B improved (fewer / less-severe events)",
        std::cmp::Ordering::Greater => "B regressed (more / worse events)",
        std::cmp::Ordering::Equal => "no change in verdict counts",
    };
    let _ = writeln!(s, "  → {read}");
}

/// The `[Loading Gate Delta]` section: the gate time A → B and each loading
/// stage's mean duration A → B (folding the same [`LOADING_STAGES`] set the
/// single-session `[Loading Gate]` uses).
fn write_gate_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
    let _ = writeln!(s);
    let _ = writeln!(s, "[Loading Gate Delta]");
    let (ga, gb) = (gate_total_secs(&a.events), gate_total_secs(&b.events));
    let gate_delta = match (ga, gb) {
        (Some(x), Some(y)) => format!("  ({})", signed_f(round1(y) - round1(x))),
        _ => String::new(),
    };
    let _ = writeln!(
        s,
        "  Loading → InGame:  {} → {}{gate_delta}",
        opt_secs(ga),
        opt_secs(gb)
    );
    let _ = writeln!(s, "  per-stage mean:");
    for (label, pick) in LOADING_STAGES {
        let ma = stage_distro(&a.events, *pick).map(|d| d.mean);
        let mb = stage_distro(&b.events, *pick).map(|d| d.mean);
        let delta = match (ma, mb) {
            (Some(x), Some(y)) => format!("  ({})", signed_f(round1(y) - round1(x))),
            _ => String::new(),
        };
        let _ = writeln!(
            s,
            "    {label:<14} {} → {}{delta}",
            opt_secs(ma),
            opt_secs(mb)
        );
    }
}

/// The `[Metric Delta]` section: each gauge's peak (max over the run) and each
/// counter's total A → B, over the union of metrics seen in either session.
fn write_metric_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
    let _ = writeln!(s);
    let _ = writeln!(s, "[Metric Delta]");
    let (pa, pb) = (gauge_peaks(&a.events), gauge_peaks(&b.events));
    let (ca, cb) = (counter_totals(&a.events), counter_totals(&b.events));
    let (has_a, has_b) = (
        has_metric_snapshot(&a.events),
        has_metric_snapshot(&b.events),
    );
    if pa.is_empty() && pb.is_empty() && ca.is_empty() && cb.is_empty() {
        // Distinguish "no snapshots at all" from "snapshots present but only
        // histograms" (this section diffs gauges + counters, not histograms).
        let msg = if has_a || has_b {
            "  (metric snapshots present, but no gauge/counter series to diff)"
        } else {
            "  (no metric snapshots in either session)"
        };
        let _ = writeln!(s, "{msg}");
        return;
    }

    // Display a value at the same rounding its Δ is taken over, so `A → B  (Δ)`
    // always reconciles: byte gauges keep their raw value (fmt_bytes owns the
    // unit); others go through round1 (matching the round1-based delta below).
    let disp = |name: &str, v: f64| {
        let v = if name.ends_with("bytes") {
            v
        } else {
            round1(v)
        };
        fmt_metric(name, v)
    };

    let gnames: BTreeSet<&str> = pa.keys().chain(pb.keys()).map(String::as_str).collect();
    if !gnames.is_empty() {
        let _ = writeln!(s, "  gauge peaks (max over each run):");
        for name in &gnames {
            let (va, vb) = (pa.get(*name).copied(), pb.get(*name).copied());
            let delta = match (va, vb) {
                (Some(x), Some(y)) if name.ends_with("bytes") => {
                    format!("  ({})", fmt_metric_delta(name, y - x))
                }
                (Some(x), Some(y)) => {
                    format!("  ({})", fmt_metric_delta(name, round1(y) - round1(x)))
                }
                _ => String::new(),
            };
            let fa = va.map(|v| disp(name, v)).unwrap_or_else(|| "—".to_string());
            let fb = vb.map(|v| disp(name, v)).unwrap_or_else(|| "—".to_string());
            let _ = writeln!(s, "    {name:<34} {fa:>12} → {fb:<12}{delta}");
        }
    }

    let cnames: BTreeSet<&str> = ca.keys().chain(cb.keys()).map(String::as_str).collect();
    if !cnames.is_empty() {
        let _ = writeln!(s, "  counter totals:");
        for name in &cnames {
            // A 0-valued counter never serializes into a snapshot, so an absent
            // counter on a side that HAS snapshots is a genuine 0; a side with no
            // snapshots at all is unknown (`—`, no delta) — never a false
            // "resolved", mirroring the gauge branch's honesty.
            let va = ca.get(*name).copied().or(has_a.then_some(0));
            let vb = cb.get(*name).copied().or(has_b.then_some(0));
            let delta = match (va, vb) {
                (Some(x), Some(y)) => format!("  ({})", signed_i(y as i64 - x as i64)),
                _ => String::new(),
            };
            let fa = va.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string());
            let fb = vb.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string());
            let _ = writeln!(s, "    {name:<34} {fa:>12} → {fb:<12}{delta}");
        }
    }
}

/// How rule B's fire count relates to rule A's — drives the `[Invariant Delta]`
/// tag and sort order (regressions surface first).
fn invariant_tag(a: usize, b: usize) -> (&'static str, u8) {
    match (a, b) {
        (0, _) => ("NEW", 0),      // fired only in B — a regression
        (_, 0) => ("resolved", 2), // fired only in A — cleared
        (x, y) if y > x => ("worse", 1),
        (x, y) if y < x => ("better", 3),
        _ => ("same", 4),
    }
}

/// The `[Invariant Delta]` section: per-rule fire counts A → B, tagged
/// NEW/worse/resolved/better/same, regressions first — the direct
/// fix-confirmation signal. Uses the shared
/// [`replay_findings`](crate::diagnostics::anomaly::replay::replay_findings) so
/// the counts match the single-session `[Invariant Violations]` section.
fn write_invariant_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
    let _ = writeln!(s);
    let _ = writeln!(s, "[Invariant Delta]");
    let fold = |log: &ParsedLog| -> BTreeMap<String, (Severity, usize)> {
        crate::diagnostics::anomaly::replay::replay_findings(&log.events)
            .into_iter()
            .map(|f| (f.id, (f.severity, f.count)))
            .collect()
    };
    let (fa, fb) = (fold(a), fold(b));
    let ids: BTreeSet<&str> = fa.keys().chain(fb.keys()).map(String::as_str).collect();
    if ids.is_empty() {
        let _ = writeln!(s, "  none — no invariant fires in either session");
        return;
    }

    // Row = (rank, worst-severity-first, id, a, b, tag).
    let mut rows: Vec<(u8, Severity, String, usize, usize, &'static str)> = Vec::new();
    let mut tally = [0usize; 5]; // NEW, worse, resolved, better, same (by rank)
    for id in &ids {
        let a_c = fa.get(*id).map(|(_, c)| *c).unwrap_or(0);
        let b_c = fb.get(*id).map(|(_, c)| *c).unwrap_or(0);
        let sev = fa
            .get(*id)
            .map(|(s, _)| *s)
            .into_iter()
            .chain(fb.get(*id).map(|(s, _)| *s))
            .max()
            .unwrap_or(Severity::Info);
        let (tag, rank) = invariant_tag(a_c, b_c);
        tally[rank as usize] += 1;
        rows.push((rank, sev, id.to_string(), a_c, b_c, tag));
    }
    // Regressions first (rank asc), then worst severity, then id.
    rows.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then_with(|| y.1.cmp(&x.1))
            .then_with(|| x.2.cmp(&y.2))
    });
    for (_, _, id, a_c, b_c, tag) in &rows {
        let _ = writeln!(s, "  [{tag:<8}] {id:<34} {a_c} → {b_c}");
    }
    let _ = writeln!(
        s,
        "  → {} new, {} worse, {} resolved, {} better, {} unchanged",
        tally[0], tally[1], tally[2], tally[3], tally[4]
    );
}

/// Build the before/after diff report for two parsed logs (A = baseline,
/// B = candidate). Pure over its inputs.
pub fn diff_report(path_a: &str, log_a: &ParsedLog, path_b: &str, log_b: &ParsedLog) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "=== session diff — A vs B ===");
    let _ = writeln!(s, "  A: {path_a}");
    let _ = writeln!(s, "     {}", session_line(log_a));
    let _ = writeln!(s, "  B: {path_b}");
    let _ = writeln!(s, "     {}", session_line(log_b));

    if log_a.events.is_empty() && log_b.events.is_empty() {
        let _ = writeln!(s, "\nboth logs empty — nothing to compare");
        return s;
    }

    write_verdict_delta(&mut s, log_a, log_b);
    write_gate_delta(&mut s, log_a, log_b);
    write_metric_delta(&mut s, log_a, log_b);
    write_invariant_delta(&mut s, log_a, log_b);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::event::{FetchStatus, RecordKind, SnapshotPhase, StartupInfo};

    fn ev(t: f64, sev: Severity, payload: EventPayload) -> SessionEvent {
        SessionEvent::new(0, t, Some(1_700_000_000_000), sev, payload)
    }

    fn startup_info(session_did: Option<&str>) -> EventPayload {
        EventPayload::StartupSnapshot(Box::new(StartupInfo {
            phase: if session_did.is_some() {
                SnapshotPhase::Session
            } else {
                SnapshotPhase::Boot
            },
            version: "0.1.0".into(),
            git_sha: "deadbee".into(),
            target_arch: "x86_64".into(),
            profile: "debug".into(),
            wasm: false,
            boot_target_did: None,
            boot_pos: None,
            boot_yaw_deg: None,
            pds: None,
            relay: None,
            session_did: session_did.map(str::to_string),
        }))
    }

    /// A well-formed NDJSON log serialized from real events round-trips, and
    /// blank lines + one unknown-variant line are tolerated (skipped/counted).
    #[test]
    fn parse_ndjson_tolerates_blank_and_unknown_lines() {
        let events = vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
            ev(1.0, Severity::Info, EventPayload::LoadingPhaseStarted),
            ev(
                2.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ),
        ];
        let mut text = String::new();
        for e in &events {
            text.push_str(&serde_json::to_string(e).unwrap());
            text.push('\n');
        }
        // A blank line (ignored) and a future-schema line (counted).
        text.push('\n');
        text.push_str("{\"seq\":9,\"t_mono_secs\":3.0,\"wall_ms\":null,\"subsystem\":\"Runtime\",\"category\":\"Perf\",\"severity\":\"Info\",\"payload\":{\"kind\":\"SomeFutureVariant\"}}\n");

        let parsed = parse_ndjson(&text);
        assert_eq!(parsed.events.len(), 3, "3 good events, blank line skipped");
        assert_eq!(parsed.unparseable, 1, "unknown variant counted, not fatal");
    }

    #[test]
    fn report_healthy_session_has_header_and_healthy_verdict() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(1.5, Severity::Info, EventPayload::LoadingPhaseStarted),
                ev(
                    120.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("session-latest.jsonl", &parsed);
        assert!(r.contains("=== session analysis — session-latest.jsonl ==="));
        assert!(r.contains("did: did:plc:me"));
        assert!(r.contains("v0.1.0 (deadbee) x86_64/debug"));
        assert!(r.contains("120.0s   (3 events)"));
        assert!(r.contains("exit:       app_exit"));
        assert!(r.contains("HEALTHY"));
    }

    #[test]
    fn report_tallies_severities_and_flags_missing_exit() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Warn,
                    EventPayload::LoadingGateWarning {
                        stage: "heightmap".into(),
                        message: "slow".into(),
                    },
                ),
                ev(
                    2.0,
                    Severity::Critical,
                    EventPayload::InvariantViolation {
                        rule: "loading.gate_stall".into(),
                        detail: "200s".into(),
                    },
                ),
                // No SessionEnd → torn/crashed log.
            ],
            unparseable: 2,
        };
        let r = report("crash.jsonl", &parsed);
        assert!(r.contains("1 critical, 1 warning"), "verdict tally: {r}");
        assert!(r.contains("no SessionEnd record"));
        assert!(r.contains("2 unparseable"));
    }

    #[test]
    fn session_id_is_the_first_events_wall_stamp_not_a_later_one() {
        // The first event has no wall clock; a later one does. The session id
        // must stay `—` (it mirrors `SessionLog::session_start_wall_ms`), not
        // borrow the later stamp and misidentify the run.
        let parsed = ParsedLog {
            events: vec![
                SessionEvent::new(
                    0,
                    0.0,
                    None,
                    Severity::Info,
                    startup_info(Some("did:plc:me")),
                ),
                SessionEvent::new(
                    1,
                    1.0,
                    Some(1_700_000_000_000),
                    Severity::Info,
                    EventPayload::LoadingPhaseStarted,
                ),
            ],
            unparseable: 0,
        };
        let r = report("x.jsonl", &parsed);
        assert!(
            r.contains("session-id: —"),
            "session-id must be — when the first event has no wall_ms: {r}"
        );
    }

    #[test]
    fn report_empty_log_is_graceful() {
        let parsed = ParsedLog {
            events: Vec::new(),
            unparseable: 0,
        };
        let r = report("empty.jsonl", &parsed);
        assert!(r.contains("empty log: no parseable events"));
    }

    #[test]
    fn report_renders_timeline_and_loading_gate_stage_timings() {
        use EventPayload::*;
        let did = "did:plc:me".to_string();
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(0.5, Severity::Info, LoadingPhaseStarted),
                ev(
                    1.0,
                    Severity::Info,
                    RecordFetchCompleted {
                        record: RecordKind::Room,
                        did: did.clone(),
                        status: FetchStatus::Ok,
                        duration_secs: 0.8,
                    },
                ),
                ev(
                    1.2,
                    Severity::Info,
                    RecordFetchCompleted {
                        record: RecordKind::Avatar,
                        did: did.clone(),
                        status: FetchStatus::NotFound,
                        duration_secs: 0.4,
                    },
                ),
                // A FAILED fetch: shows in the timeline but must NOT pollute the
                // success-only record-fetch stage distro (its ~timeout latency
                // would skew the percentiles).
                ev(
                    1.5,
                    Severity::Warn,
                    RecordFetchCompleted {
                        record: RecordKind::Inventory,
                        did: did.clone(),
                        status: FetchStatus::Exhausted,
                        duration_secs: 30.0,
                    },
                ),
                ev(
                    2.0,
                    Severity::Info,
                    HeightmapGenCompleted {
                        duration_secs: 1.5,
                        width: 256,
                        height: 256,
                    },
                ),
                ev(
                    2.5,
                    Severity::Info,
                    AmbientBakeCompleted {
                        bytes: 44_100,
                        duration_secs: 0.3,
                    },
                ),
                ev(
                    3.0,
                    Severity::Info,
                    WorldCompileCompleted {
                        entity_count: 1200,
                        duration_secs: 0.9,
                    },
                ),
                ev(
                    3.2,
                    Severity::Info,
                    LoadingGateTransitionToInGame { elapsed_secs: 2.7 },
                ),
                ev(
                    10.0,
                    Severity::Info,
                    SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);

        // Timeline milestones render at their timestamps.
        assert!(r.contains("[Timeline]"), "{r}");
        assert!(r.contains("loading gate opened"), "{r}");
        assert!(r.contains("Room fetch Ok"), "{r}");
        // The failed fetch is a timeline milestone even though it's distro-excluded.
        assert!(r.contains("Inventory fetch Exhausted"), "{r}");
        assert!(r.contains("→ InGame (2.7s)"), "{r}");
        assert!(r.contains("world compiled (1200 entities)"), "{r}");
        assert!(r.contains("session end (app_exit)"), "{r}");

        // Loading-gate section: the gate total + per-stage distros.
        assert!(r.contains("[Loading Gate]"), "{r}");
        assert!(r.contains("Loading → InGame:  2.7s"), "{r}");
        // record-fetch distro folds only the two SUCCESSFUL completions (0.8,
        // 0.4) — the Exhausted failure (30.0s) is excluded, so n=2 not 3 and the
        // max stays 0.8.
        assert!(r.contains("record fetch"), "{r}");
        assert!(
            r.contains("(n=2)"),
            "record-fetch distro should exclude the failure (n=2): {r}"
        );
        assert!(
            !r.contains("max 30.0"),
            "the 30s failure latency must not appear in any distro: {r}"
        );
        assert!(r.contains("heightmap"), "{r}");
        assert!(r.contains("ambient bake"), "{r}");
        assert!(r.contains("world compile"), "{r}");
    }

    #[test]
    fn loading_gate_section_marks_a_missing_gate() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);
        assert!(
            r.contains("Loading → InGame:  — (no loading gate in this log)"),
            "{r}"
        );
        // Every stage row still renders, but as an empty distro — so no `(n=…)`
        // count appears anywhere when nothing ran.
        assert!(r.contains("[Loading Gate]"), "{r}");
        assert!(r.contains("record fetch"), "{r}");
        assert!(r.contains("world compile"), "{r}");
        assert!(
            !r.contains("(n="),
            "no stage distro should render when nothing ran: {r}"
        );
    }

    fn snapshot(at: f64) -> EventPayload {
        use crate::diagnostics::registry::MetricSnapshot;
        EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
            at_secs: at,
            gauges: Vec::new(),
            counters: Vec::new(),
            histograms: Vec::new(),
        }))
    }

    /// The order arrays and their index functions must agree: the index of a
    /// variant equals its position in the array. The index fns are exhaustive
    /// matches (a new variant is a compile error there); this pins the arrays to
    /// them, so a reorder or mid-array insert can't silently misalign a
    /// matrix/category column. (A pure *append* left out of the array is caught
    /// at compile time by the index-fn match, and — belt-and-suspenders —
    /// surfaced as `unclassified` at runtime rather than panicking.)
    #[test]
    fn order_arrays_match_their_index() {
        for (i, s) in SUBSYSTEM_ORDER.iter().enumerate() {
            assert_eq!(subsystem_index(*s), i, "subsystem order/index drift at {i}");
        }
        for (i, s) in SEVERITY_ORDER.iter().enumerate() {
            assert_eq!(severity_index(*s), i, "severity order/index drift at {i}");
        }
        for (i, c) in CATEGORY_ORDER.iter().enumerate() {
            assert_eq!(category_index(*c), i, "category order/index drift at {i}");
        }
    }

    /// The tally matrix counts non-snapshot events by subsystem × severity and
    /// by category, and the 1 Hz metric snapshots are excluded (noted) so they
    /// don't bury the notable-event counts.
    #[test]
    fn event_tallies_matrix_and_categories_exclude_snapshots() {
        let events = vec![
            ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))), // Session / Snapshot
            ev(0.5, Severity::Info, EventPayload::LoadingPhaseStarted), // Loading / Lifecycle
            ev(
                1.0,
                Severity::Warn,
                EventPayload::PeerIdentitySpoofRejected {
                    peer: "p".into(),
                    claimed_did: "a".into(),
                    authenticated_did: "b".into(),
                },
            ), // Network / Peer
            ev(
                1.2,
                Severity::Error,
                EventPayload::OffloadJobFailed {
                    job: "heightmap".into(),
                    reason: "gone".into(),
                },
            ), // Offload / Job
            ev(
                1.5,
                Severity::Critical,
                EventPayload::RespawnTriggered {
                    fell_to_y: -30.0,
                    ground_y: 4.0,
                },
            ), // Runtime / Physics
            ev(2.0, Severity::Trace, snapshot(2.0)),                   // excluded
            ev(3.0, Severity::Trace, snapshot(3.0)),                   // excluded
            ev(
                4.0,
                Severity::Info,
                EventPayload::SessionEnd {
                    reason: "app_exit".into(),
                },
            ), // Session / Lifecycle
        ];
        let mut out = String::new();
        write_event_tallies(&mut out, &events);

        assert!(
            out.contains("[Event Tallies]  (excluding 2 metric snapshots)"),
            "{out}"
        );
        // Matrix header + one row per subsystem that had events.
        assert!(out.contains("subsystem"), "{out}");
        assert!(out.contains("crit"), "{out}");
        assert!(out.contains("Network"), "{out}");
        assert!(out.contains("Offload"), "{out}");
        assert!(out.contains("Runtime"), "{out}");
        assert!(out.contains("Session"), "{out}");
        assert!(out.contains("total"), "{out}");
        // By category: Lifecycle (loading + session end) = 2 is busiest; the two
        // MetricsSnapshot events are excluded, so Snapshot is 1 (startup only),
        // not 3 — the proof that snapshots stayed out of the tally.
        assert!(out.contains("by category:"), "{out}");
        assert!(out.contains("Lifecycle 2"), "{out}");
        assert!(out.contains("Snapshot 1"), "{out}");
    }

    /// A log of nothing but metric snapshots has no notable events to tally.
    #[test]
    fn event_tallies_only_snapshots_reports_no_events() {
        let events = vec![
            ev(1.0, Severity::Trace, snapshot(1.0)),
            ev(2.0, Severity::Trace, snapshot(2.0)),
            ev(3.0, Severity::Trace, snapshot(3.0)),
        ];
        let mut out = String::new();
        write_event_tallies(&mut out, &events);
        assert!(out.contains("(excluding 3 metric snapshots)"), "{out}");
        assert!(out.contains("(no non-snapshot events)"), "{out}");
    }

    fn metric_snapshot(
        at: f64,
        frame_ms: f64,
        entities: f64,
        mem_bytes: f64,
        peers: u64,
    ) -> EventPayload {
        use crate::diagnostics::registry::{CounterPoint, GaugePoint, HistPoint, MetricSnapshot};
        EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
            at_secs: at,
            gauges: vec![
                GaugePoint {
                    name: "runtime.frame_time.ms".into(),
                    last: frame_ms,
                },
                GaugePoint {
                    name: "runtime.entity.count".into(),
                    last: entities,
                },
                GaugePoint {
                    name: "runtime.memory.process_rss_bytes".into(),
                    last: mem_bytes,
                },
            ],
            counters: vec![CounterPoint {
                name: "net.peer.connected_count".into(),
                value: peers,
            }],
            histograms: vec![HistPoint {
                name: "net.jitter.playout_latency_ms".into(),
                min: 8.0,
                p50: 12.0,
                p90: 24.0,
                max: 40.0,
                mean: 14.0,
                n: 100,
            }],
        }))
    }

    #[test]
    fn fmt_bytes_humanizes_sizes() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(1024.0), "1.0 KB");
        assert_eq!(fmt_bytes(256.0 * 1024.0 * 1024.0), "256.0 MB");
        assert_eq!(fmt_bytes(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB");
    }

    /// Metric trends chart the snapshot series: gauge first → last + a run distro,
    /// counter growth, and the histogram's final distribution.
    #[test]
    fn metric_trends_chart_gauge_counter_and_histogram_series() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Trace,
                    metric_snapshot(1.0, 16.0, 1200.0, 256.0 * 1024.0 * 1024.0, 1),
                ),
                ev(
                    2.0,
                    Severity::Trace,
                    metric_snapshot(2.0, 22.0, 1240.0, 336.0 * 1024.0 * 1024.0, 3),
                ),
                ev(
                    3.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);

        assert!(r.contains("[Metric Trends]"), "{r}");
        assert!(r.contains("over 2 snapshots"), "{r}");
        // Gauge trend: first → last + a distro over both samples.
        assert!(r.contains("runtime.frame_time.ms"), "{r}");
        assert!(r.contains("16.0 →"), "gauge first value: {r}");
        assert!(r.contains("(n=2)"), "gauge run distro n: {r}");
        // Entity-count drift (the leak signal) is charted.
        assert!(r.contains("runtime.entity.count"), "{r}");
        // The memory-growth curve is humanized (MB), not raw bytes.
        assert!(r.contains("runtime.memory.process_rss_bytes"), "{r}");
        assert!(r.contains("256.0 MB → 336.0 MB"), "humanized memory: {r}");
        assert!(
            !r.contains("268435456"),
            "raw byte counts must not appear: {r}"
        );
        // Counter growth 1 → 3.
        assert!(r.contains("net.peer.connected_count"), "{r}");
        assert!(r.contains("(+2)"), "counter growth: {r}");
        // Histogram final distribution.
        assert!(r.contains("histograms (final distribution)"), "{r}");
        assert!(r.contains("net.jitter.playout_latency_ms"), "{r}");
    }

    /// A log with no metric snapshots (e.g. a crash before the first scrape)
    /// renders the section but marks it empty — never a silent omission.
    #[test]
    fn metric_trends_absent_when_no_snapshots() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    1.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);
        assert!(r.contains("[Metric Trends]"), "{r}");
        assert!(r.contains("(no metric snapshots in this log)"), "{r}");
    }

    // --- B-4: --diff-sessions -----------------------------------------------

    fn spoof() -> EventPayload {
        EventPayload::PeerIdentitySpoofRejected {
            peer: "p".into(),
            claimed_did: "did:plc:evil".into(),
            authenticated_did: "did:plc:real".into(),
        }
    }

    /// A full baseline log: a gate stall + a spoof burst + slow record fetches,
    /// with metric snapshots showing high memory + peers.
    fn baseline_log() -> ParsedLog {
        use crate::diagnostics::event::RecordKind;
        let did = "did:plc:me".to_string();
        ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(0.4, Severity::Info, EventPayload::LoadingPhaseStarted),
                ev(
                    1.0,
                    Severity::Info,
                    EventPayload::RecordFetchCompleted {
                        record: RecordKind::Room,
                        did: did.clone(),
                        status: FetchStatus::Ok,
                        duration_secs: 2.0,
                    },
                ),
                // 3 spoofs → net.identity_spoof_burst fires (replayable).
                ev(2.0, Severity::Warn, spoof()),
                ev(2.1, Severity::Warn, spoof()),
                ev(2.2, Severity::Warn, spoof()),
                ev(
                    3.0,
                    Severity::Trace,
                    metric_snapshot(3.0, 30.0, 1300.0, 384.0 * 1024.0 * 1024.0, 3),
                ),
                // No InGame transition, and the log runs long → gate_stall replays.
                ev(200.0, Severity::Info, EventPayload::RoomStateApplied),
            ],
            unparseable: 0,
        }
    }

    /// The candidate log: the fix landed — gate reaches InGame fast, no spoofs,
    /// faster fetch, lower memory.
    fn candidate_log() -> ParsedLog {
        use crate::diagnostics::event::RecordKind;
        ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(0.4, Severity::Info, EventPayload::LoadingPhaseStarted),
                ev(
                    0.9,
                    Severity::Info,
                    EventPayload::RecordFetchCompleted {
                        record: RecordKind::Room,
                        did: "did:plc:me".into(),
                        status: FetchStatus::Ok,
                        duration_secs: 0.5,
                    },
                ),
                ev(
                    2.5,
                    Severity::Trace,
                    metric_snapshot(2.5, 17.0, 1200.0, 260.0 * 1024.0 * 1024.0, 2),
                ),
                ev(
                    3.0,
                    Severity::Info,
                    EventPayload::LoadingGateTransitionToInGame { elapsed_secs: 2.6 },
                ),
                ev(
                    10.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        }
    }

    #[test]
    fn diff_report_surfaces_verdict_gate_metric_and_invariant_deltas() {
        let a = baseline_log();
        let b = candidate_log();
        let r = diff_report("baseline.jsonl", &a, "candidate.jsonl", &b);

        // Header labels both sessions.
        assert!(r.contains("=== session diff — A vs B ==="), "{r}");
        assert!(r.contains("A: baseline.jsonl"), "{r}");
        assert!(r.contains("B: candidate.jsonl"), "{r}");

        // Verdict delta: B has fewer warnings (3 spoofs → 0) → improved.
        assert!(r.contains("[Verdict Delta]"), "{r}");
        assert!(r.contains("warnings      3 → 0"), "{r}");
        assert!(r.contains("B improved"), "{r}");

        // Gate delta: A never reached InGame; B reached it in 2.6s.
        assert!(r.contains("[Loading Gate Delta]"), "{r}");
        assert!(r.contains("Loading → InGame:  — → 2.6s"), "{r}");
        // Record-fetch mean improved 2.0s → 0.5s.
        assert!(
            r.contains("record fetch") && r.contains("2.0s → 0.5s"),
            "{r}"
        );
        assert!(r.contains("(-1.5)"), "fetch delta: {r}");

        // Metric delta: memory peak dropped, humanized + signed.
        assert!(r.contains("[Metric Delta]"), "{r}");
        assert!(r.contains("runtime.memory.process_rss_bytes"), "{r}");
        assert!(r.contains("384.0 MB → 260.0 MB"), "{r}");
        assert!(r.contains("(-124.0 MB)"), "memory delta: {r}");
        // Counter delta present.
        assert!(
            r.contains("net.peer.connected_count") && r.contains("3 → 2"),
            "{r}"
        );

        // Invariant delta: gate_stall + spoof_burst fired in A, resolved in B.
        assert!(r.contains("[Invariant Delta]"), "{r}");
        assert!(
            r.contains("[resolved]") && r.contains("loading.gate_stall"),
            "{r}"
        );
        assert!(r.contains("net.identity_spoof_burst"), "{r}");
        assert!(r.contains("resolved,"), "summary line: {r}");
    }

    #[test]
    fn diff_report_flags_a_regression() {
        // A is clean; B introduces a spoof burst → NEW invariant + more warnings.
        let a = candidate_log();
        let b = baseline_log();
        let r = diff_report("good.jsonl", &a, "bad.jsonl", &b);
        assert!(r.contains("B regressed"), "{r}");
        assert!(
            r.contains("[NEW"),
            "a rule that fired only in B is tagged NEW: {r}"
        );
    }

    #[test]
    fn diff_report_handles_two_empty_logs() {
        let empty = ParsedLog {
            events: Vec::new(),
            unparseable: 0,
        };
        let r = diff_report("a.jsonl", &empty, "b.jsonl", &empty);
        assert!(r.contains("both logs empty"), "{r}");
    }

    #[test]
    fn invariant_tag_classifies_each_direction() {
        assert_eq!(invariant_tag(0, 2).0, "NEW");
        assert_eq!(invariant_tag(2, 0).0, "resolved");
        assert_eq!(invariant_tag(1, 3).0, "worse");
        assert_eq!(invariant_tag(3, 1).0, "better");
        assert_eq!(invariant_tag(2, 2).0, "same");
    }

    /// A gauge peak's displayed operands are rounded the same way its Δ is, so a
    /// half-tenth value can't print a row whose `A → B  (Δ)` fails to add up.
    #[test]
    fn metric_delta_gauge_display_reconciles_with_its_delta() {
        use crate::diagnostics::registry::{GaugePoint, MetricSnapshot};
        let snap = |peak: f64| {
            EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
                at_secs: 1.0,
                gauges: vec![GaugePoint {
                    name: "runtime.frame_time.ms".into(),
                    last: peak,
                }],
                counters: Vec::new(),
                histograms: Vec::new(),
            }))
        };
        // 16.25 / 16.34: {:.1} (half-even) and round1 (half-away) would disagree.
        let a = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("x"))),
                ev(1.0, Severity::Trace, snap(16.25)),
            ],
            unparseable: 0,
        };
        let b = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("x"))),
                ev(1.0, Severity::Trace, snap(16.34)),
            ],
            unparseable: 0,
        };
        let r = diff_report("a.jsonl", &a, "b.jsonl", &b);
        // Both operands round1 → 16.3, so the row reads "16.3 → 16.3 (+0.0)".
        assert!(r.contains("16.3 → 16.3"), "operands must be round1'd: {r}");
        assert!(
            r.contains("(+0.0)"),
            "delta must reconcile with operands: {r}"
        );
    }

    /// A counter that fired in A but whose candidate B has *no metric snapshots
    /// at all* (crashed before the first scrape) must read `N → —` (unknown),
    /// never `N → 0` — a data-less run is not an improvement.
    #[test]
    fn metric_delta_absent_counter_without_snapshots_is_unknown_not_resolved() {
        let a = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("x"))),
                ev(
                    1.0,
                    Severity::Trace,
                    metric_snapshot(1.0, 16.0, 1200.0, 256.0 * 1024.0 * 1024.0, 3),
                ),
            ],
            unparseable: 0,
        };
        let b = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("x"))),
                ev(
                    1.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "crash".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = diff_report("a.jsonl", &a, "b.jsonl", &b);
        assert!(r.contains("net.peer.connected_count"), "{r}");
        assert!(
            !r.contains("3 → 0"),
            "an absent-because-no-snapshots counter must not read as resolved: {r}"
        );
        assert!(r.contains("3 → —"), "it should read as unknown: {r}");
    }

    /// Snapshots that carry only histograms (no gauge/counter series this section
    /// diffs) report as present-but-nothing-to-diff, not as absent.
    #[test]
    fn metric_delta_histograms_only_distinguishes_from_no_snapshots() {
        use crate::diagnostics::registry::{HistPoint, MetricSnapshot};
        let snap = || {
            EventPayload::MetricsSnapshot(Box::new(MetricSnapshot {
                at_secs: 1.0,
                gauges: Vec::new(),
                counters: Vec::new(),
                histograms: vec![HistPoint {
                    name: "net.jitter.playout_latency_ms".into(),
                    min: 8.0,
                    p50: 12.0,
                    p90: 24.0,
                    max: 40.0,
                    mean: 14.0,
                    n: 50,
                }],
            }))
        };
        let mklog = || ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("x"))),
                ev(1.0, Severity::Trace, snap()),
            ],
            unparseable: 0,
        };
        let r = diff_report("a.jsonl", &mklog(), "b.jsonl", &mklog());
        assert!(
            r.contains("metric snapshots present, but no gauge/counter series to diff"),
            "{r}"
        );
    }

    // --- B-5: analyzer filters ----------------------------------------------

    fn mixed_log() -> ParsedLog {
        ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))), // Session
                ev(1.0, Severity::Info, EventPayload::LoadingPhaseStarted), // Loading
                ev(
                    5.0,
                    Severity::Warn,
                    EventPayload::PeerIdentitySpoofRejected {
                        peer: "p".into(),
                        claimed_did: "a".into(),
                        authenticated_did: "b".into(),
                    },
                ), // Network / Warn / t=5
                ev(
                    50.0,
                    Severity::Error,
                    EventPayload::OffloadJobFailed {
                        job: "heightmap".into(),
                        reason: "gone".into(),
                    },
                ), // Offload / Error / t=50
                ev(
                    90.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ), // Session
            ],
            unparseable: 0,
        }
    }

    #[test]
    fn filters_parse_names_case_insensitively_and_reject_unknowns() {
        let f = Filters::parse(
            Some("Network"),
            Some("peer"),
            Some("warn"),
            Some(1.0),
            Some(9.0),
        )
        .expect("valid");
        assert_eq!(f.subsystem, Some(Subsystem::Network));
        assert_eq!(f.category, Some(Category::Peer));
        assert_eq!(f.min_severity, Some(Severity::Warn));
        assert!(f.is_active());
        assert!(Filters::parse(Some("bogus"), None, None, None, None).is_err());
        assert!(Filters::parse(None, Some("bogus"), None, None, None).is_err());
        assert!(Filters::parse(None, None, Some("bogus"), None, None).is_err());
        // An all-None filter is an inactive passthrough.
        assert!(
            !Filters::parse(None, None, None, None, None)
                .unwrap()
                .is_active()
        );
    }

    #[test]
    fn subsystem_filter_scopes_sections_but_not_the_header() {
        let log = mixed_log();
        let filters = Filters::parse(Some("network"), None, None, None, None).unwrap();
        let r = report_with("s.jsonl", &log, &filters);
        // Header still identifies the run (from the FULL log).
        assert!(r.contains("did: did:plc:me"), "header from full log: {r}");
        assert!(r.contains("v0.1.0 (deadbee)"), "{r}");
        // Filter line documents the lens + match count (1 Network event of 5).
        assert!(
            r.contains("[Filter]  subsystem=Network  —  1 of 5 events match"),
            "{r}"
        );
        // Verdict reflects only the Network subset: the spoof Warn, not the
        // Offload Error.
        assert!(r.contains("1 warning") && !r.contains("1 error"), "{r}");
    }

    #[test]
    fn severity_filter_is_a_minimum_threshold() {
        let log = mixed_log();
        let filters = Filters::parse(None, None, Some("error"), None, None).unwrap();
        let r = report_with("s.jsonl", &log, &filters);
        // Only the Error (Offload) matches; the Warn spoof is below threshold.
        assert!(r.contains("severity≥Error  —  1 of 5 events match"), "{r}");
        assert!(r.contains("1 error"), "{r}");
    }

    #[test]
    fn time_window_filter_bounds_both_ends_inclusive() {
        let log = mixed_log();
        // [4.0, 60.0] captures the t=5 spoof and t=50 offload fail, not t=0/1/90.
        let filters = Filters::parse(None, None, None, Some(4.0), Some(60.0)).unwrap();
        let r = report_with("s.jsonl", &log, &filters);
        assert!(r.contains("t∈[4.0s, 60.0s]  —  2 of 5 events match"), "{r}");
    }

    #[test]
    fn filter_matching_nothing_reports_it_and_keeps_the_header() {
        let log = mixed_log();
        let filters = Filters::parse(Some("runtime"), None, None, None, None).unwrap();
        let r = report_with("s.jsonl", &log, &filters);
        assert!(r.contains("did: did:plc:me"), "header still present: {r}");
        assert!(r.contains("0 of 5 events match"), "{r}");
        assert!(r.contains("(no events match — nothing to analyze)"), "{r}");
        // No analysis sections emitted when nothing matches.
        assert!(!r.contains("[Verdict]"), "{r}");
    }

    #[test]
    fn no_filter_report_equals_report_with_default() {
        // The unfiltered path must be byte-identical to report_with(default).
        let log = mixed_log();
        assert_eq!(
            report("s.jsonl", &log),
            report_with("s.jsonl", &log, &Filters::default())
        );
    }

    /// A record write (region save) is a timeline milestone (#624) — an in-game
    /// save must be visible in the post-mortem, not just record reads.
    #[test]
    fn timeline_shows_record_writes() {
        use crate::diagnostics::event::RecordKind;
        let write = |t: f64, record| {
            ev(
                t,
                Severity::Info,
                EventPayload::RecordWriteCompleted {
                    record,
                    did: "did:plc:me".into(),
                    duration_secs: 0.4,
                },
            )
        };
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                write(30.0, RecordKind::Room),
                write(35.0, RecordKind::Avatar),
                write(38.0, RecordKind::Inventory),
                ev(
                    40.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);
        // #624 + #626: room, avatar, and inventory saves all trace.
        assert!(r.contains("Room saved to PDS"), "{r}");
        assert!(r.contains("Avatar saved to PDS"), "{r}");
        assert!(r.contains("Inventory saved to PDS"), "{r}");
        // Writes land in the Loading/Fetch bucket (PDS record I/O) — 3 of them.
        assert!(
            r.contains("Fetch 3"),
            "record writes count under Fetch: {r}"
        );
    }

    /// #627: an avatar re-seed is a timeline milestone (Loading/Generation),
    /// so an in-game avatar re-roll is visible rather than inferable only from
    /// asset-handle churn.
    #[test]
    fn timeline_shows_avatar_reseed() {
        let parsed = ParsedLog {
            events: vec![
                ev(0.0, Severity::Info, startup_info(Some("did:plc:me"))),
                ev(
                    50.0,
                    Severity::Info,
                    EventPayload::AvatarReseeded { seed: 7 },
                ),
                ev(
                    60.0,
                    Severity::Info,
                    EventPayload::SessionEnd {
                        reason: "app_exit".into(),
                    },
                ),
            ],
            unparseable: 0,
        };
        let r = report("s.jsonl", &parsed);
        assert!(r.contains("avatar reseeded (seed 7)"), "{r}");
        // Counts under Loading/Generation, alongside region-regen events.
        assert!(r.contains("Generation 1"), "{r}");
    }
}
