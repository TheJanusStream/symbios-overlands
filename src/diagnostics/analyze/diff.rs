//! Before/after session diff (B-4): the `--diff-sessions <a> <b>` delta
//! report builders — verdict, loading-gate, metric-peak and invariant
//! deltas over two [`ParsedLog`]s.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};

use super::parse::*;
use super::sections::*;
use super::{plural, round1};

/// A compact one-line session summary for the diff header (build · DID ·
/// duration · exit), tolerant of a torn/short log.
pub(super) fn session_line(log: &ParsedLog) -> String {
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
pub(super) fn gauge_peaks(events: &[SessionEvent]) -> BTreeMap<String, f64> {
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
pub(super) fn counter_totals(events: &[SessionEvent]) -> BTreeMap<String, u64> {
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
pub(super) fn has_metric_snapshot(events: &[SessionEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e.payload, EventPayload::MetricsSnapshot(_)))
}

/// A signed integer delta (`+2` / `-1` / `0`).
pub(super) fn signed_i(d: i64) -> String {
    if d > 0 {
        format!("+{d}")
    } else {
        format!("{d}")
    }
}

/// A signed one-decimal delta (`+0.3` / `-1.2`).
pub(super) fn signed_f(d: f64) -> String {
    if d >= 0.0 {
        format!("+{d:.1}")
    } else {
        format!("{d:.1}")
    }
}

/// Format a metric value for the diff, humanizing byte gauges (see [`fmt_bytes`])
/// and otherwise rendering one decimal — consistent with `[Metric Trends]`.
pub(super) fn fmt_metric(name: &str, v: f64) -> String {
    if name.ends_with("bytes") {
        fmt_bytes(v)
    } else {
        format!("{v:.1}")
    }
}

/// A signed metric delta (byte-aware): `+80.0 MB`, `-2.3`, `+0.0`.
pub(super) fn fmt_metric_delta(name: &str, d: f64) -> String {
    let body = fmt_metric(name, d.abs());
    if d >= 0.0 {
        format!("+{body}")
    } else {
        format!("-{body}")
    }
}

/// An `Option<f64>` as `X.Xs` or `—` (rounded to match [`round1`]-based deltas).
pub(super) fn opt_secs(v: Option<f64>) -> String {
    v.map(|x| format!("{:.1}s", round1(x)))
        .unwrap_or_else(|| "—".to_string())
}

/// The `[Verdict Delta]` section: warn/error/critical counts A → B, plus an
/// overall better/worse/same read (severity-weighted: critical, then error,
/// then warn).
pub(super) fn write_verdict_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
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
pub(super) fn write_gate_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
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
pub(super) fn write_metric_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
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
pub(super) fn invariant_tag(a: usize, b: usize) -> (&'static str, u8) {
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
pub(super) fn write_invariant_delta(s: &mut String, a: &ParsedLog, b: &ParsedLog) {
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
