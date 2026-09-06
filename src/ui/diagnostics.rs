//! Diagnostics HUD — a five-tab panel (see [`DiagTab`]). The Overview /
//! Runtime / Network / Offload tabs draw the frame-time sparkline and
//! per-subsystem metric health cards + anomaly badges over the shared
//! metrics registry.
//!
//! Two things about the cards are load-bearing rather than incidental
//! (#1272). The rows are built as DATA by [`health_cards`] and painted by
//! [`render_health_tab`], so `metric_rows_and_rules_line_up` can walk every
//! row of every tab and check it against [`METRIC_RULE_TABLE`] — a mapped
//! metric can no longer end up with no row anywhere (eight signalling
//! gauges were scraped every second and rendered on no screen at all), and
//! a row can no longer lose the rule that badges it. And each mapping
//! carries a [`Watch`], because an empty badge was being read as a check
//! that passed while five of fourteen mapped rows were pointed at rules
//! that could never light on this panel.
//!
//! The Session tab (#837 — the honest remainder after
//! the toolbar's account chip took identity / logout / Copy Landmark
//! Link) holds the session-debug tools: a native-only wireframe-mode
//! checkbox (skipped on WebGL2 where `POLYGON_MODE_LINE` is
//! unavailable), the session-log export controls, a demoted debug peer
//! roster (DIDs + copy buttons — People owns presence and mutes), and
//! the scrolling event log. This module also hosts
//! [`landmark_link_button`], the share-your-spot button the account
//! chip renders (bundles the current room DID + player position + yaw
//! into a URL the WASM build opens directly and the native build
//! accepts as `--did=… --pos=… --rot=…`).

#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::wireframe::WireframeConfig;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::boot_params::{ClipboardQueue, build_landmark_link};
use crate::diagnostics::anomaly::{InvariantRegistry, RuleId};
use crate::diagnostics::event::{Severity, Subsystem};
use crate::diagnostics::{Distro, MetricsRegistry, SessionLog, names};
use crate::state::RemotePeer;

/// Which tab of the Diagnostics panel is showing. Overview (C-3) draws the
/// frame-time sparkline + counts + memory; the Runtime / Network / Offload
/// tabs (C-4) draw the per-subsystem health cards over the shared metrics
/// registry + anomaly badges; Session (#837 — né "Identity", before the
/// toolbar's account chip absorbed identity/logout/share) keeps the
/// session-debug remainder: wireframe toggle, log export, a demoted peer
/// roster, and the event log. Default is Overview — the health summary,
/// not a legacy-parity drawer.
///
/// A `Resource` (not a window-local) since #835, so the toolbar's anomaly
/// dot can open the panel directly onto the worst-offending tab.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DiagTab {
    #[default]
    Overview,
    Runtime,
    Network,
    Offload,
    Session,
}

impl DiagTab {
    const ALL: [DiagTab; 5] = [
        DiagTab::Overview,
        DiagTab::Runtime,
        DiagTab::Network,
        DiagTab::Offload,
        DiagTab::Session,
    ];

    fn label(self) -> &'static str {
        match self {
            DiagTab::Overview => "Overview",
            DiagTab::Runtime => "Runtime",
            DiagTab::Network => "Network",
            DiagTab::Offload => "Offload",
            DiagTab::Session => "Session",
        }
    }
}

/// Map a [`Severity`] to the HUD colour used for both the event-log line tint
/// and the anomaly badges / toolbar dot (D-6), so a warning reads the same amber
/// everywhere. `pub(crate)` so [`crate::ui::toolbar`] can colour its worst-active
/// dot identically. Delegates to the active theme's severity ramp (#856) —
/// the `ui` handle is how the palette is reached from render code.
pub(crate) fn severity_color(ui: &egui::Ui, sev: Severity) -> egui::Color32 {
    crate::ui::theme::current(ui.ctx()).status.severity(sev)
}

/// The currently-violated rules as `(id, severity, last detail, fire count,
/// description, last fired)`, worst-severity first (ties broken by id for
/// stable output). The pure data behind the badge strip, unit-tested
/// independently of egui. The description is the badge's face text (#837 —
/// human words, the raw id demotes to the hover); ids without a description
/// (impossible for registered rules) fall back to the id itself.
type Badge = (&'static str, Severity, String, u64, &'static str, f64);
fn collect_badges(invariants: &InvariantRegistry) -> Vec<Badge> {
    let mut badges: Vec<Badge> = invariants
        .active_badges()
        .map(|(id, sev, st)| {
            (
                id,
                sev,
                st.last_detail.clone(),
                st.fire_count,
                invariants.rule_description(id).unwrap_or(id),
                st.last_fired_secs,
            )
        })
        .collect();
    badges.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    badges
}

/// What the session-log export controls need, bundled so the anomaly banner
/// can offer them without `render_anomaly_section` growing four parameters
/// three of its two callers do not use.
/// How tall the Active Anomalies list is allowed to get before it scrolls
/// (#1273 f181). Roughly four rows — enough that a healthy-ish session shows
/// its whole strip and a broken one does not swallow the tab under it.
const ANOMALY_STRIP_MAX_HEIGHT: f32 = 120.0;

pub(crate) struct LogExportDeps<'a> {
    pub session_log: &'a SessionLog,
    pub clipboard: &'a ClipboardQueue,
    pub toasts: &'a mut crate::ui::toast::Toasts,
    pub now: f64,
}

/// The hover behind a badge: the rule's precise statement of the condition,
/// then its id and `trailer` (when it last fired, and its detail).
///
/// The face text is the plain-language sentence now (#1271 f409) — the panel
/// used to render developer prose straight at the user ("relay reported peers
/// in the room but no WebRTC data channel opened (offer glare or ICE/NAT
/// failure)"), which is the only sentence this whole review area ever says
/// about a connectivity failure. The precision is not thrown away, it moves
/// one layer down, which is the pattern the Offload tab already proved.
fn rule_hover(invariants: &InvariantRegistry, id: RuleId, trailer: &str) -> String {
    match invariants.rule_technical(id) {
        Some(technical) => format!("{technical}\n\n{id} — {trailer}"),
        None => format!("{id} — {trailer}"),
    }
}

/// Render the anomaly badge strip (Pillar D-6): a persistent red banner while
/// any `Critical` invariant is active, then one severity-coloured badge per
/// currently-violated rule with its last detail, worst-severity first. Reads the
/// same [`InvariantRegistry::active_badges`] / [`InvariantRegistry::worst_active`]
/// ledger the live engine writes, so the panel mirrors the anomaly engine's
/// current state. Shown on every tab so the health signal is never hidden.
fn render_anomaly_section(
    ui: &mut egui::Ui,
    invariants: &InvariantRegistry,
    export: Option<&mut LogExportDeps<'_>>,
) {
    let th = crate::ui::theme::current(ui.ctx());
    let badges = collect_badges(invariants);

    // Persistent banner while any Critical invariant is active — the same
    // Frame idiom the room-recovery banner uses.
    let crit = badges.iter().filter(|b| b.1 == Severity::Critical).count();
    if crit > 0 {
        egui::Frame::new()
            .fill(th.danger_surface)
            .inner_margin(6.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.colored_label(
                    th.danger_surface_text,
                    format!(
                        "⚠ {crit} CRITICAL invariant{} active — session health is compromised.",
                        if crit == 1 { "" } else { "s" }
                    ),
                );
            });
        // The remedy under the banner, on whatever tab the dot routed to
        // (#1272 f175). The Critical memory rules tell the user to download
        // the session log, and that button lived on a fourth tab they had to
        // find for themselves — while the failure it warns about is the one
        // that takes the tab and the log down together. Skipped on the
        // Session tab, which draws these controls in their own right.
        if let Some(export) = export {
            render_log_export_controls(
                ui,
                export.session_log,
                export.clipboard,
                export.toasts,
                export.now,
            );
        }
        ui.add_space(4.0);
    }

    if badges.is_empty() {
        crate::ui::affordances::ok_label(ui, "No active anomalies");
    } else {
        ui.label(format!("Active Anomalies ({})", badges.len()));
        // Capped, with the Critical banner above it left un-scrolled so the
        // loudest signal is always pinned (#1273 f181). The strip sits
        // OUTSIDE every per-tab scroll area — it is drawn on every tab on
        // purpose — and with 29 rules registered an unbounded list eats the
        // height the tab body was going to get, worst exactly when the most
        // is wrong. `auto_shrink` on the vertical axis so a one-badge strip
        // still takes one badge of room.
        egui::ScrollArea::vertical()
            .id_salt("diag_anomaly_strip")
            .max_height(ANOMALY_STRIP_MAX_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (id, sev, detail, fires, description, last_fired) in &badges {
                    let color = th.status.severity(*sev);
                    ui.horizontal(|ui| {
                        crate::ui::affordances::status_dot(ui, color);
                        // Human description up front (#837); the raw rule id and
                        // when it last fired live in the hover for debugging.
                        // Body, not `.small()` (#1259 f243): this line is what a
                        // user is pointed at when something has gone wrong, and
                        // it is tinted with the severity ramp on top — the Trace
                        // tier put it at 9 pt AND low contrast at once.
                        ui.label(egui::RichText::new(*description).color(color))
                            .on_hover_text(rule_hover(
                                invariants,
                                id,
                                &format!("last fired {}", crate::format_elapsed_ts(*last_fired)),
                            ));
                        if *fires > 1 {
                            ui.label(
                                egui::RichText::new(format!("×{fires}"))
                                    .small()
                                    .color(th.text_faint),
                            );
                        }
                        if !detail.is_empty() {
                            ui.monospace(
                                egui::RichText::new(format!("— {detail}"))
                                    .small()
                                    .color(th.text_weak),
                            );
                        }
                    });
                }
            });
    }
    ui.separator();
}

/// How the rule mapped to a metric row actually watches it (#1272 f173).
///
/// The panel used to promise coverage it did not have: five of fourteen rows
/// were mapped to rules that could never light on screen, and an absent badge
/// reads as a check that passed. A row now says which of the three it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Watch {
    /// Evaluated live in a state this panel runs in, so no dot really does
    /// mean "checked, and fine".
    Live,
    /// Replay-only: re-derived from the session log by the offline analyzer
    /// and never live-violated, so nothing badges this row while you play.
    Analyzer,
    /// Evaluated live, but only during loading — a state the Diagnostics
    /// panel never runs in. The loading screen owns the live signal.
    WhileLoading,
}

impl Watch {
    /// The quiet marker a non-`Live` row carries, and what it means.
    fn marker(self) -> Option<(&'static str, &'static str)> {
        match self {
            Watch::Live => None,
            Watch::Analyzer => Some((
                "log only",
                "Nothing checks this while you are playing — it is worked out \
                 afterwards from the session log. An empty space here is not \
                 a pass.",
            )),
            Watch::WhileLoading => Some((
                "while loading",
                "This is checked while a world is opening, not once you are in \
                 it — the loading screen shows it live. An empty space here is \
                 not a pass.",
            )),
        }
    }
}

/// The single metric→invariant-rule mapping the GUI badges read (Pillar C-6):
/// which rule's live state badges each metric row, and whether that rule can
/// light it at all. A metric absent here shows no badge and no marker.
///
/// A metric may appear more than once — the wasm-heap row is watched by a
/// `Warn` rule and a `Critical` one, and [`anomaly_badge`] draws the worst of
/// them that is active.
///
/// `metric_rows_and_rules_line_up` pins every row of this table against the
/// rule set AND against the cards that render, so neither half can drift.
const METRIC_RULE_TABLE: &[(&str, &str, Watch)] = &[
    (
        names::RUNTIME_FRAME_TIME_MS,
        "runtime.frame_time_spike",
        Watch::Live,
    ),
    (
        names::RUNTIME_FRAME_HITCH_MS,
        "runtime.frame_hitch",
        Watch::Live,
    ),
    (
        names::RUNTIME_MESH_HANDLE_COUNT,
        "runtime.asset_handle_spike",
        Watch::Live,
    ),
    (
        names::RUNTIME_COLLIDER_COUNT,
        "runtime.terrain_collider_missing",
        Watch::Live,
    ),
    (
        names::RUNTIME_SHAPE_MESH_CACHE_LEN,
        "runtime.shape_mesh_cache_growth",
        Watch::Live,
    ),
    (
        names::RUNTIME_RESPAWN_COUNT,
        "runtime.respawn_thrashing",
        Watch::Live,
    ),
    // The browser OOM is the app's single most destructive live failure, and
    // the click-through routed it to a tab with no memory row on it at all
    // (#1272 f175). Two rules, one row: the worst active one badges it.
    //
    // wasm-only, and the `cfg` is load-bearing: the memory ROW is cfg-split
    // (native shows process RSS, which no rule watches), so mapping these
    // unconditionally would point two rules at a metric that has no row on a
    // native build — which is the shape of defect this whole table now
    // guards against.
    #[cfg(target_arch = "wasm32")]
    (
        names::RUNTIME_MEMORY_WASM_BYTES,
        "runtime.wasm_memory_high",
        Watch::Live,
    ),
    #[cfg(target_arch = "wasm32")]
    (
        names::RUNTIME_MEMORY_WASM_BYTES,
        "runtime.wasm_memory_critical",
        Watch::Live,
    ),
    (
        names::NET_PEER_DISCONNECTED_COUNT,
        "net.peer_churn_spike",
        Watch::Live,
    ),
    (
        names::NET_IDENTITY_SPOOFED_COUNT,
        "net.identity_spoof_burst",
        Watch::Live,
    ),
    (
        names::NET_OFFER_ACCEPTED_COUNT,
        "net.offer_acceptance_anomaly",
        Watch::Live,
    ),
    // The signalling gauges the two live link rules evaluate on (#1272 f401).
    // They were scraped every second and rendered nowhere, so the tab the
    // anomaly dot routes a link failure to said nothing about the link.
    (
        names::NET_SIGNAL_AWAITING_PEERS,
        "net.signal_glare_suspected",
        Watch::Live,
    ),
    (
        names::NET_SIGNAL_AUTH_REJECTIONS,
        "net.relay_connection_rejected",
        Watch::Live,
    ),
    (
        names::NET_RELAY_TOKEN_REFRESH_FAILURES,
        "net.relay_token_refresh_failing",
        Watch::Live,
    ),
    (
        names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS,
        "offload.ambient_bake_stall",
        Watch::Analyzer,
    ),
    (
        names::OFFLOAD_JOB_ERROR_COUNT,
        "offload.task_never_resolves",
        Watch::Live,
    ),
    (
        names::LOADING_RECORD_FETCH_LATENCY_MS,
        "loading.record_fetch_exhausted",
        Watch::Analyzer,
    ),
    (
        names::LOADING_GATE_TOTAL_SECS,
        "loading.gate_stall",
        Watch::WhileLoading,
    ),
    // #802/#837 close-the-loop: audio overload badges the looping-voice
    // row, so the toolbar dot finally routes to the Audio card.
    (
        names::AUDIO_SPATIAL_ACTIVE_SINKS,
        "audio.looping_voices_overload",
        Watch::Live,
    ),
];

/// The invariant rules that badge `metric` (see [`METRIC_RULE_TABLE`]).
fn rules_for_metric(metric: &str) -> impl Iterator<Item = (&'static str, Watch)> + '_ {
    METRIC_RULE_TABLE
        .iter()
        .filter(move |(m, _, _)| *m == metric)
        .map(|(_, rule, watch)| (*rule, *watch))
}

/// A per-metric anomaly pill (Pillar C-6): when a rule that badges `metric_id`
/// (via [`METRIC_RULE_TABLE`]) is currently live-violated, draw a severity-
/// coloured `●` beside the row, hovering the rule's precise statement + when it
/// last fired + its detail. Where several rules watch one row, the worst active
/// one wins.
///
/// A row whose rules cannot light live carries a quiet marker instead of
/// nothing at all (#1272 f173): silence on such a row was being read as a check
/// that passed, and the module doc sold the mapping as exactly that.
fn anomaly_badge(ui: &mut egui::Ui, invariants: &InvariantRegistry, metric_id: &str) {
    let mut watches = Vec::new();
    let mut worst: Option<(
        &'static str,
        Severity,
        &crate::diagnostics::anomaly::RuleRuntimeState,
    )> = None;
    for (rule_id, watch) in rules_for_metric(metric_id) {
        watches.push(watch);
        if let Some((_, sev, st)) = invariants.active_badges().find(|(id, _, _)| *id == rule_id)
            && worst.is_none_or(|(_, worst_sev, _)| sev > worst_sev)
        {
            worst = Some((rule_id, sev, st));
        }
    }
    if let Some((rule_id, sev, st)) = worst {
        let colour = severity_color(ui, sev);
        crate::ui::affordances::status_dot(ui, colour);
        // The rule's human description BESIDE the dot (#1260 f240). The
        // pill was a painted circle whose whole meaning lived in a
        // pointer tooltip: no text in any input mode, and egui opens a
        // tooltip for a pointer and never for keyboard focus. The
        // Active Anomalies list has said this in words since #837; the
        // per-metric pill was the one that never did.
        let description = invariants.rule_description(rule_id).unwrap_or(rule_id);
        // `last_fired_secs` is a session timestamp, not an age — format it
        // like the event log's stamps instead of reading as "Ns ago" (#837).
        crate::ui::affordances::hint(
            ui.label(egui::RichText::new(description).small().color(colour)),
            &rule_hover(
                invariants,
                rule_id,
                &format!(
                    "last fired {}: {}",
                    crate::format_elapsed_ts(st.last_fired_secs),
                    st.last_detail
                ),
            ),
        );
        return;
    }
    // Nothing active. If every rule on this row is one that cannot light
    // here, say so rather than leaving a blank that reads as a pass.
    if !watches.is_empty()
        && watches.iter().all(|w| *w != Watch::Live)
        && let Some((label, why)) = watches[0].marker()
    {
        let th = crate::ui::theme::current(ui.ctx());
        crate::ui::affordances::hint(
            ui.label(egui::RichText::new(label).small().color(th.text_faint)),
            why,
        );
    }
}

/// The count of live-violated invariants attributable to a tab, for its label
/// badge (C-6). Overview aggregates everything; the subsystem tabs count their
/// own subsystem (Offload also owns the loading-gate rules).
/// The Diagnostics tab that presents anomalies from `subsystem` — the
/// inverse of [`tab_anomaly_count`]'s attribution, used by the toolbar
/// dot's click-through (#835). `Session` has no card tab of its own;
/// its badges render on every tab, so Overview is the honest landing.
pub(crate) fn tab_for_subsystem(subsystem: Option<Subsystem>) -> DiagTab {
    match subsystem {
        Some(Subsystem::Runtime) => DiagTab::Runtime,
        Some(Subsystem::Network) => DiagTab::Network,
        Some(Subsystem::Offload) | Some(Subsystem::Loading) => DiagTab::Offload,
        Some(Subsystem::Session) | None => DiagTab::Overview,
    }
}

fn tab_anomaly_count(tab: DiagTab, invariants: &InvariantRegistry) -> usize {
    match tab {
        DiagTab::Overview => invariants.active_badges().count(),
        DiagTab::Runtime => invariants.active_count_for(Subsystem::Runtime),
        DiagTab::Network => invariants.active_count_for(Subsystem::Network),
        DiagTab::Offload => {
            invariants.active_count_for(Subsystem::Offload)
                + invariants.active_count_for(Subsystem::Loading)
        }
        DiagTab::Session => 0,
    }
}

/// One 60 Hz frame's worth of milliseconds — the line a frame-time trace is
/// read against, and the first gridline on the sparkline.
const FRAME_BUDGET_MS: f64 = 1000.0 / 60.0;

/// The band the frame-time sparkline is drawn against: zero to two budgets,
/// with a gridline at each (#1273 f182).
///
/// A FIXED band, which is the whole point. The plot used to scale itself to
/// its own min and max, so a flawless session holding 16.0–16.3 ms drew the
/// same full-height sawtooth as one stuttering between 16 and 300 — the shape
/// carried no information about milliseconds at all, and it read alarmingly by
/// default because noise is always drawn as drama. Against a fixed band a
/// steady trace is a flat line near the bottom, which is what steady looks
/// like.
const FRAME_BAND_MS: (f64, f64) = (0.0, FRAME_BUDGET_MS * 2.0);
const FRAME_GRIDLINES_MS: &[f64] = &[FRAME_BUDGET_MS, FRAME_BUDGET_MS * 2.0];

/// Where a sample sits in a band: 0 at the bottom, 1 at the top, clamped.
///
/// Clamped rather than scaled, so one 400 ms stall does not flatten the rest
/// of the trace into the floor — an outlier is drawn at the ceiling and marked
/// (see [`sparkline`]) instead of rewriting the axis for every other sample.
fn spark_t(v: f64, (lo, hi): (f64, f64)) -> f32 {
    let span = (hi - lo).max(1e-9);
    (((v - lo) / span) as f32).clamp(0.0, 1.0)
}

/// Hand-rolled polyline sparkline over a metric's recent history, drawn
/// straight onto the panel via `ui.painter()` (no `egui_plot` dependency),
/// against the FIXED `band` with a gridline at each of `gridlines`.
///
/// The band's ends are printed at the rect's corners and the sample under the
/// pointer is on the hover, so the vertical scale is readable rather than
/// implied (#1273 f182). Samples above the band are clamped to the top edge
/// and dotted in `status.error`, so an off-scale stall is visible AS
/// off-scale.
fn sparkline(
    ui: &mut egui::Ui,
    samples: &[f64],
    height: f32,
    band: (f64, f64),
    gridlines: &[f64],
    unit: &str,
) {
    let width = ui.available_width().max(32.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let th = crate::ui::theme::current(ui.ctx());
    painter.rect_filled(rect, 2.0, th.chart_fill_deep);

    let y_of = |v: f64| rect.bottom() - rect.height() * spark_t(v, band);

    // Gridlines and the band's ends first, so they read as the backdrop —
    // and so an empty chart still states its scale rather than being a
    // blank strip whose height means nothing.
    for line in gridlines {
        let y = y_of(*line);
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, th.text_faint),
        );
    }
    let corner = egui::TextStyle::Small.resolve(ui.style());
    painter.text(
        rect.left_top() + egui::vec2(3.0, 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0}{unit}", band.1),
        corner.clone(),
        th.text_weak,
    );
    painter.text(
        rect.left_bottom() + egui::vec2(3.0, -1.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.0}{unit}", band.0),
        corner,
        th.text_weak,
    );

    if samples.len() < 2 {
        return;
    }
    let n = samples.len();
    let x_of = |i: usize| rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
    let pts: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &v)| egui::pos2(x_of(i), y_of(v)))
        .collect();
    painter.add(egui::Shape::line(
        pts,
        egui::Stroke::new(1.5_f32, th.accent),
    ));
    // Off-scale samples, marked where they were clamped.
    for (i, &v) in samples.iter().enumerate() {
        if v > band.1 {
            painter.circle_filled(egui::pos2(x_of(i), rect.top() + 2.0), 2.0, th.status.error);
        }
    }

    // The sample under the pointer. The rect was already allocated with
    // `Sense::hover()` and nothing had ever been hung on the response.
    if let Some(pos) = response.hover_pos() {
        let frac = ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        let i = ((frac * (n - 1) as f32).round() as usize).min(n - 1);
        response.on_hover_text(format!("{:.1}{unit}", samples[i]));
    }
}

/// Human-readable byte size (B / KiB / MiB / GiB).
fn fmt_bytes(bytes: f64) -> String {
    const KIB: f64 = 1024.0;
    let (mib, gib) = (KIB * KIB, KIB * KIB * KIB);
    if bytes >= gib {
        format!("{:.2} GiB", bytes / gib)
    } else if bytes >= mib {
        format!("{:.1} MiB", bytes / mib)
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

/// Process-memory readout — native RSS or the wasm linear-memory size, cfg-split
/// (scraped from different sources), with a GRAY "unavailable" fallback when the
/// gauge has no sample yet (mirroring the native-only wireframe gate's absence
/// handling).
fn memory_readout(ui: &mut egui::Ui, metrics: &MetricsRegistry, invariants: &InvariantRegistry) {
    let (label, name) = (MEMORY_ROW_LABEL, MEMORY_ROW_METRIC);
    ui.horizontal(|ui| {
        match metrics.gauge_latest(name) {
            Some(bytes) => {
                ui.monospace(format!("{label}: {}", fmt_bytes(bytes)));
            }
            None => {
                let th = crate::ui::theme::current(ui.ctx());
                ui.colored_label(th.text_weak, format!("{label}: unavailable"));
            }
        }
        // The two wasm-heap rules badge this line (#1272 f175). It carried no
        // badge at all, on the only screen it appeared on, for the app's most
        // destructive live failure.
        anomaly_badge(ui, invariants, name);
    });
}

/// The Overview tab's live-count grid, as data so the drift guard can walk it
/// alongside the health cards (#1272 f188).
///
/// The three cache lengths at the end pin the handle counts above them (#919)
/// — a mesh or image count that will not fall is usually one of these holding
/// it, so they are read together.
const OVERVIEW_COUNT_ROWS: &[(&str, &str)] = &[
    ("Entities", names::RUNTIME_ENTITY_COUNT),
    ("Mesh handles", names::RUNTIME_MESH_HANDLE_COUNT),
    ("Material handles", names::RUNTIME_MATERIAL_HANDLE_COUNT),
    ("Image handles", names::RUNTIME_IMAGE_HANDLE_COUNT),
    ("Colliders", names::RUNTIME_COLLIDER_COUNT),
    ("ShapeMeshCache", names::RUNTIME_SHAPE_MESH_CACHE_LEN),
    ("PrimMeshCache", names::RUNTIME_PRIM_MESH_CACHE_LEN),
    ("PrimMaterialCache", names::RUNTIME_PRIM_MATERIAL_CACHE_LEN),
    ("TextureCache", names::RUNTIME_TEXTURE_CACHE_LEN),
];

/// The metric the Overview sparkline plots, and the one beside it.
const OVERVIEW_SPARKLINE_METRIC: &str = names::RUNTIME_FRAME_TIME_MS;
const OVERVIEW_FPS_METRIC: &str = names::RUNTIME_FPS;

/// The metrics the Overview tab renders outside its count grid: the sparkline
/// and its FPS line, and the memory readout. Built from the same consts the
/// renderer draws from, so the drift guard is checking the code rather than a
/// list somebody kept up to date by hand.
#[cfg(test)]
const OVERVIEW_OTHER_METRICS: &[&str] = &[
    OVERVIEW_SPARKLINE_METRIC,
    OVERVIEW_FPS_METRIC,
    MEMORY_ROW_METRIC,
];

/// The Overview / Perf tab (Pillar C-3): a frame-time sparkline + an FPS/distro
/// line, a compact grid of the live entity/asset/collider counts, and the
/// memory readout. Each row surfaces its anomaly badge from the shared ledger.
fn render_overview_tab(
    ui: &mut egui::Ui,
    metrics: &MetricsRegistry,
    invariants: &InvariantRegistry,
) {
    ui.label("Frame time (ms, last ~2 min)");
    sparkline(
        ui,
        &metrics.ring_slice(OVERVIEW_SPARKLINE_METRIC),
        40.0,
        FRAME_BAND_MS,
        FRAME_GRIDLINES_MS,
        "",
    );
    ui.horizontal(|ui| {
        let fps = metrics
            .gauge_latest(OVERVIEW_FPS_METRIC)
            .map(|f| format!("{f:.0}"))
            .unwrap_or_else(|| "—".to_string());
        ui.monospace(format!("FPS {fps}"));
        let distro = metrics.gauge_distro(OVERVIEW_SPARKLINE_METRIC);
        let frame = distro
            .as_ref()
            .map(distro_inline)
            .unwrap_or_else(|| "—".to_string());
        let cell = ui.monospace(egui::RichText::new(format!("· frame {frame}")).small());
        if let Some(d) = &distro {
            cell.on_hover_text(distro_hover(d));
        }
        anomaly_badge(ui, invariants, OVERVIEW_SPARKLINE_METRIC);
    });

    ui.separator();

    // Live counts — a compact grid; the badge is keyed on the row's own metric
    // via the shared METRIC_RULE_TABLE (C-6).
    egui::Grid::new("diag-overview-counts")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let count_row = |ui: &mut egui::Ui, label: &str, name: &str| {
                ui.label(label);
                let v = metrics
                    .gauge_latest(name)
                    .map(|n| format!("{n:.0}"))
                    .unwrap_or_else(|| "—".to_string());
                ui.monospace(v);
                anomaly_badge(ui, invariants, name);
                ui.end_row();
            };
            for (label, name) in OVERVIEW_COUNT_ROWS {
                count_row(ui, label, name);
            }
        });

    ui.separator();
    memory_readout(ui, metrics, invariants);
}

/// A distribution as the panel presents one: `p50`/`p90` in the cell.
///
/// #837 moved the full five-stat line out of the health cards because it was
/// too wide for the panel's 280 pt slot, and left the Overview's copy of it
/// alone — so the landing screen printed the line the cards had just given up,
/// and the two surfaces disagreed about how a distribution is shown. One
/// function now, called by both (#1273 f183).
fn distro_inline(d: &Distro) -> String {
    format!("p50 {:.1}  p90 {:.1}", d.p50, d.p90)
}

/// The rest of the same distribution, for the hover.
fn distro_hover(d: &Distro) -> String {
    format!(
        "min {:.1}  max {:.1}  mean {:.1}  n {}",
        d.min, d.max, d.mean, d.n
    )
}

/// One subsystem health card: a titled `egui::Frame` wrapping a 3-column grid of
/// `(label, value, anomaly badge)` rows. `rows` is `(label, value, metric_id)` —
/// the metric id keys the badge via [`METRIC_RULE_TABLE`] (C-6); a metric with no
/// mapped rule simply draws no dot.
/// What the native wireframe toggle does, and that it is a debug view
/// (#1274 f191). It is a persistent GLOBAL render mode reachable only from
/// one tab of one panel, and nothing outside that checkbox reflected it — a
/// user who ticked it to see what it did was left with a world drawn in wire
/// and no indication of why.
#[cfg(not(target_arch = "wasm32"))]
const WIREFRAME_HINT: &str = "Draw every surface as wire — a render-debug view. Untick to go back to \
     normal. Nothing else changes, and it stays on until you untick it.";

/// The memory row's label and metric, cfg-split at the source they are
/// scraped from — the native process RSS and the wasm linear-memory size are
/// different numbers with different meanings, so they are different gauges.
#[cfg(not(target_arch = "wasm32"))]
const MEMORY_ROW_LABEL: &str = "Process memory";
#[cfg(not(target_arch = "wasm32"))]
const MEMORY_ROW_METRIC: &str = names::RUNTIME_MEMORY_PROCESS_RSS_BYTES;
#[cfg(target_arch = "wasm32")]
const MEMORY_ROW_LABEL: &str = "Browser memory";
#[cfg(target_arch = "wasm32")]
const MEMORY_ROW_METRIC: &str = names::RUNTIME_MEMORY_WASM_BYTES;

/// The texture-slot row's label. The platform qualifier that used to be
/// appended to the VALUE — and was the whole information content of it —
/// moves to [`row_note`] (#1273 f189).
const SLOT_ROW_LABEL: &str = "Texture slots";

/// A per-row explanatory hover, keyed by metric id.
///
/// A side table rather than a fourth element on every row tuple: only a
/// handful of rows carry a number whose meaning is not in its label, and
/// threading an `Option` through forty rows to serve four of them is how a
/// row ends up with no explanation because adding one was tedious.
/// `row_notes_describe_rendered_rows` pins each entry to a row that exists.
fn row_note(metric: &str) -> Option<&'static str> {
    #[cfg(target_arch = "wasm32")]
    const SLOTS: &str = "Of the 16 texture slots WebGL2 gives a shader. Past that \
                         the ground stops drawing its layers.";
    #[cfg(not(target_arch = "wasm32"))]
    const SLOTS: &str = "How many texture slots the ground material uses. WebGPU \
                         has no fixed ceiling, so this is a size, not a budget — \
                         it is the browser build that has 16.";
    #[cfg(target_arch = "wasm32")]
    const MEMORY: &str = "How much memory this browser tab holds. It never goes \
                          back down, so a climb only ends when you reload.";
    #[cfg(not(target_arch = "wasm32"))]
    const MEMORY: &str = "How much memory this app holds.";
    match metric {
        names::RUNTIME_TEXTURE_BIND_SLOTS => Some(SLOTS),
        MEMORY_ROW_METRIC => Some(MEMORY),
        names::NET_SIGNAL_AWAITING_PEERS => Some(
            "People the relay says are here that you have not managed to \
             connect to yet. A number that stays up is a blocked connection.",
        ),
        names::NET_SIGNAL_PEER_LIST_LEN => {
            Some("How many people the relay last said were in this world.")
        }
        names::RECORD_SIZE_ROOM_BYTES
        | names::RECORD_SIZE_AVATAR_BYTES
        | names::RECORD_SIZE_INVENTORY_BYTES => Some(
            "How big this was the last time it was saved. Past the comfortable \
             size below, saving starts to be refused.",
        ),
        names::RUNTIME_FRAME_TIME_MAX_MS => Some(
            "The longest single frame in the last second. The smoothed figure \
             above is an average and cannot see a stutter.",
        ),
        _ => None,
    }
}

fn health_card(
    ui: &mut egui::Ui,
    invariants: &InvariantRegistry,
    metrics: &MetricsRegistry,
    title: &str,
    rows: &[(&str, String, &str)],
) {
    egui::Frame::new()
        .fill(crate::ui::theme::current(ui.ctx()).chart_fill)
        .inner_margin(6.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong());
            // Titles are unique within a tab (only one tab renders per frame), so
            // the title doubles as the grid id.
            egui::Grid::new(title)
                .num_columns(3)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    for (label, value, metric) in rows {
                        ui.label(*label);
                        // Histogram rows render p50/p90 inline (#837 — the
                        // full five-stat line wrapped in the 280px window);
                        // min/max/mean/n move to the hover.
                        let cell = ui.monospace(value.as_str());
                        if let Some(d) = metrics.hist_distro(metric) {
                            cell.on_hover_text(distro_hover(&d));
                        } else if let Some(note) = row_note(metric) {
                            cell.on_hover_text(note);
                        }
                        anomaly_badge(ui, invariants, metric);
                        ui.end_row();
                    }
                });
        });
    ui.add_space(6.0);
}

/// One health card: a title and its `(label, value, metric id)` rows.
///
/// Data, not drawing — [`health_cards`] builds the whole tab and
/// [`render_health_tab`] paints it. That split is what lets
/// `metric_rows_and_rules_line_up` walk every row of every tab and check it
/// against [`METRIC_RULE_TABLE`], so a mapped metric can no longer end up with
/// no row anywhere and a row can no longer lose the rule that badges it
/// (#1272 f188).
type Card = (&'static str, Vec<(&'static str, String, &'static str)>);

/// The Audio card's title, named because the interpretation line and the mute
/// button are drawn immediately after it and the two must not drift apart.
const AUDIO_CARD_TITLE: &str = "Audio";

/// The cards of a per-subsystem health tab (Pillar C-4), in render order.
///
/// A row's `metric id` may be `""` for a derived value with no metric of its
/// own; those draw no badge and are exempt from the drift guard.
fn health_cards(tab: DiagTab, metrics: &MetricsRegistry) -> Vec<Card> {
    // Row-value shorthands over the C-2 readers: gauge latest / counter / distro.
    let g = |name: &str| {
        metrics
            .gauge_latest(name)
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "—".to_string())
    };
    let c = |name: &str| metrics.counter_value(name).to_string();
    // Histograms: p50/p90 inline (#837) — the five-stat line wrapped in
    // the 280px window; health_card hangs min/max/mean/n on the hover.
    let h = |name: &str| {
        metrics
            .hist_distro(name)
            .map(|d| distro_inline(&d))
            .unwrap_or_else(|| "—".to_string())
    };
    let bytes = |name: &str| {
        metrics
            .gauge_latest(name)
            .map(fmt_bytes)
            .unwrap_or_else(|| "—".to_string())
    };

    match tab {
        DiagTab::Runtime => vec![
            (
                "Runtime",
                vec![
                    (
                        "Frame time (ms)",
                        g(names::RUNTIME_FRAME_TIME_MS),
                        names::RUNTIME_FRAME_TIME_MS,
                    ),
                    // The pair #1144 added precisely to make jank visible, and
                    // which was then collected once a second and displayed
                    // nowhere (#1272 f188). The smoothed gauge above cannot
                    // see a hitch; these two are the ones a stutter shows up
                    // in, so they belong beside it.
                    (
                        "Worst frame (ms)",
                        g(names::RUNTIME_FRAME_TIME_MAX_MS),
                        names::RUNTIME_FRAME_TIME_MAX_MS,
                    ),
                    (
                        "Hitches (ms)",
                        h(names::RUNTIME_FRAME_HITCH_MS),
                        names::RUNTIME_FRAME_HITCH_MS,
                    ),
                    ("FPS", g(names::RUNTIME_FPS), names::RUNTIME_FPS),
                    // The click-through for the browser OOM lands on this tab
                    // (#1272 f175) and there was no memory row on it at all.
                    (
                        MEMORY_ROW_LABEL,
                        bytes(MEMORY_ROW_METRIC),
                        MEMORY_ROW_METRIC,
                    ),
                    (
                        "Entities",
                        g(names::RUNTIME_ENTITY_COUNT),
                        names::RUNTIME_ENTITY_COUNT,
                    ),
                    (
                        "Mesh handles",
                        g(names::RUNTIME_MESH_HANDLE_COUNT),
                        names::RUNTIME_MESH_HANDLE_COUNT,
                    ),
                    (
                        "Material handles",
                        g(names::RUNTIME_MATERIAL_HANDLE_COUNT),
                        names::RUNTIME_MATERIAL_HANDLE_COUNT,
                    ),
                    (
                        "Colliders",
                        g(names::RUNTIME_COLLIDER_COUNT),
                        names::RUNTIME_COLLIDER_COUNT,
                    ),
                    (
                        "ShapeMeshCache",
                        g(names::RUNTIME_SHAPE_MESH_CACHE_LEN),
                        names::RUNTIME_SHAPE_MESH_CACHE_LEN,
                    ),
                    (
                        "Respawns",
                        c(names::RUNTIME_RESPAWN_COUNT),
                        names::RUNTIME_RESPAWN_COUNT,
                    ),
                ],
            ),
            // The 100 KiB soft budget had no readout anywhere (#1272 f188) —
            // the one number that decides whether a Save will be refused.
            (
                "Saved sizes",
                vec![
                    (
                        "This world",
                        bytes(names::RECORD_SIZE_ROOM_BYTES),
                        names::RECORD_SIZE_ROOM_BYTES,
                    ),
                    (
                        "Your look",
                        bytes(names::RECORD_SIZE_AVATAR_BYTES),
                        names::RECORD_SIZE_AVATAR_BYTES,
                    ),
                    (
                        "Largest item",
                        bytes(names::RECORD_SIZE_INVENTORY_BYTES),
                        names::RECORD_SIZE_INVENTORY_BYTES,
                    ),
                    (
                        "Comfortable up to",
                        fmt_bytes(crate::pds::record_size::SOFT_RECORD_BUDGET_BYTES as f64),
                        "",
                    ),
                ],
            ),
        ],
        DiagTab::Network => vec![
            // FIRST card on the tab the anomaly dot routes a link failure to
            // (#1272 f401). Eight signalling gauges were scraped every second
            // and rendered on no screen at all, including the two the live
            // glare and relay-rejection rules evaluate on — so the panel the
            // app points at could not answer the one question it exists for.
            (
                "Link",
                vec![
                    (
                        "People the relay reported",
                        g(names::NET_SIGNAL_PEER_LIST_LEN),
                        names::NET_SIGNAL_PEER_LIST_LEN,
                    ),
                    (
                        "Waiting to connect",
                        g(names::NET_SIGNAL_AWAITING_PEERS),
                        names::NET_SIGNAL_AWAITING_PEERS,
                    ),
                    (
                        "Refused connections",
                        g(names::NET_SIGNAL_AUTH_REJECTIONS),
                        names::NET_SIGNAL_AUTH_REJECTIONS,
                    ),
                    (
                        "Sign-in renewals failing",
                        g(names::NET_RELAY_TOKEN_REFRESH_FAILURES),
                        names::NET_RELAY_TOKEN_REFRESH_FAILURES,
                    ),
                    // One row per gauge, not a pair per row. A combined
                    // "sent / answered" cell reads well and leaves the second
                    // gauge with no metric id of its own — so it carries no
                    // badge, gets no hover, and the drift guard cannot see it
                    // at all. Four numbers that only mean something as a
                    // sequence are four rows.
                    (
                        "Connect attempts started",
                        g(names::NET_SIGNAL_OFFERS_INITIATED),
                        names::NET_SIGNAL_OFFERS_INITIATED,
                    ),
                    (
                        "Connect requests sent",
                        g(names::NET_SIGNAL_OFFERS_SENT),
                        names::NET_SIGNAL_OFFERS_SENT,
                    ),
                    (
                        "Replies received",
                        g(names::NET_SIGNAL_ANSWERS_RECEIVED),
                        names::NET_SIGNAL_ANSWERS_RECEIVED,
                    ),
                    (
                        "Connect requests received",
                        g(names::NET_SIGNAL_OFFERS_RECEIVED),
                        names::NET_SIGNAL_OFFERS_RECEIVED,
                    ),
                    (
                        "Replies sent",
                        g(names::NET_SIGNAL_ANSWERS_SENT),
                        names::NET_SIGNAL_ANSWERS_SENT,
                    ),
                ],
            ),
            (
                "Peers",
                vec![
                    // Cumulative session counters relabeled (#837): the old
                    // "Connected/Disconnected" read as live states. The real
                    // live headcount is their difference — joins minus
                    // leaves, since every peer entity increments one and
                    // eventually the other. No rule badge: the People
                    // window/toolbar own presence UX; churn badges below.
                    (
                        "Peers now",
                        metrics
                            .counter_value(names::NET_PEER_CONNECTED_COUNT)
                            .saturating_sub(
                                metrics.counter_value(names::NET_PEER_DISCONNECTED_COUNT),
                            )
                            .to_string(),
                        "",
                    ),
                    (
                        "Joins (session)",
                        c(names::NET_PEER_CONNECTED_COUNT),
                        names::NET_PEER_CONNECTED_COUNT,
                    ),
                    (
                        "Leaves (session)",
                        c(names::NET_PEER_DISCONNECTED_COUNT),
                        names::NET_PEER_DISCONNECTED_COUNT,
                    ),
                    (
                        "Transform rejects",
                        c(names::NET_TRANSFORM_REJECTED_COUNT),
                        names::NET_TRANSFORM_REJECTED_COUNT,
                    ),
                    (
                        "Spoof rejects",
                        c(names::NET_IDENTITY_SPOOFED_COUNT),
                        names::NET_IDENTITY_SPOOFED_COUNT,
                    ),
                ],
            ),
            (
                "Avatar fetch",
                vec![
                    (
                        "Latency (ms)",
                        h(names::NET_AVATAR_FETCH_LATENCY_MS),
                        names::NET_AVATAR_FETCH_LATENCY_MS,
                    ),
                    (
                        "Succeeded",
                        c(names::NET_AVATAR_FETCH_SUCCESS_COUNT),
                        names::NET_AVATAR_FETCH_SUCCESS_COUNT,
                    ),
                    (
                        "Failed",
                        c(names::NET_AVATAR_FETCH_FAIL_COUNT),
                        names::NET_AVATAR_FETCH_FAIL_COUNT,
                    ),
                    (
                        "Worn props failed",
                        c(names::NET_ATTACHMENT_FETCH_FAIL_COUNT),
                        names::NET_ATTACHMENT_FETCH_FAIL_COUNT,
                    ),
                ],
            ),
            (
                "Jitter & offers",
                vec![
                    (
                        "Playout latency (ms)",
                        h(names::NET_JITTER_PLAYOUT_LATENCY_MS),
                        names::NET_JITTER_PLAYOUT_LATENCY_MS,
                    ),
                    (
                        "Offers accepted",
                        c(names::NET_OFFER_ACCEPTED_COUNT),
                        names::NET_OFFER_ACCEPTED_COUNT,
                    ),
                    (
                        "Offers declined",
                        c(names::NET_OFFER_DECLINED_COUNT),
                        names::NET_OFFER_DECLINED_COUNT,
                    ),
                    (
                        "Auto-declined (busy)",
                        c(names::NET_OFFER_AUTO_DECLINED_BUSY_COUNT),
                        names::NET_OFFER_AUTO_DECLINED_BUSY_COUNT,
                    ),
                    (
                        "Live edits refused (too big)",
                        c(names::NET_BROADCAST_OVERSIZE_DROPPED_COUNT),
                        names::NET_BROADCAST_OVERSIZE_DROPPED_COUNT,
                    ),
                ],
            ),
        ],
        DiagTab::Offload => vec![
            (
                "Async jobs",
                vec![
                    (
                        "Heightmap (ms)",
                        h(names::OFFLOAD_HEIGHTMAP_LATENCY_MS),
                        names::OFFLOAD_HEIGHTMAP_LATENCY_MS,
                    ),
                    (
                        "Ambient bake (ms)",
                        h(names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS),
                        names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS,
                    ),
                    (
                        "Texture bake (ms)",
                        h(names::OFFLOAD_TEXTURE_BAKE_LATENCY_MS),
                        names::OFFLOAD_TEXTURE_BAKE_LATENCY_MS,
                    ),
                    (
                        "Job errors",
                        c(names::OFFLOAD_JOB_ERROR_COUNT),
                        names::OFFLOAD_JOB_ERROR_COUNT,
                    ),
                ],
            ),
            // Spatial-audio load (#802): the sustained-lag suspect is the live
            // looping-voice count (each is a per-frame spatialise-and-mix); the
            // bake latency / size + cache footprint separate a spawn hitch and
            // buffer weight from steady playback cost.
            (
                AUDIO_CARD_TITLE,
                vec![
                    (
                        "Looping voices",
                        g(names::AUDIO_SPATIAL_ACTIVE_SINKS),
                        names::AUDIO_SPATIAL_ACTIVE_SINKS,
                    ),
                    (
                        "Contact cues",
                        g(names::AUDIO_CONTACT_ACTIVE_VOICES),
                        names::AUDIO_CONTACT_ACTIVE_VOICES,
                    ),
                    (
                        "Voice bake (ms)",
                        h(names::AUDIO_VOICE_BAKE_LATENCY_MS),
                        names::AUDIO_VOICE_BAKE_LATENCY_MS,
                    ),
                    (
                        "Bake size (bytes)",
                        h(names::AUDIO_VOICE_BAKE_BYTES),
                        names::AUDIO_VOICE_BAKE_BYTES,
                    ),
                    (
                        "Cache entries",
                        g(names::AUDIO_BAKE_CACHE_ENTRIES),
                        names::AUDIO_BAKE_CACHE_ENTRIES,
                    ),
                    (
                        "Cache bytes",
                        g(names::AUDIO_BAKE_CACHE_BYTES),
                        names::AUDIO_BAKE_CACHE_BYTES,
                    ),
                ],
            ),
            (
                "Loading gate",
                vec![
                    (
                        "Record fetch (ms)",
                        h(names::LOADING_RECORD_FETCH_LATENCY_MS),
                        names::LOADING_RECORD_FETCH_LATENCY_MS,
                    ),
                    (
                        "Fetch retries",
                        c(names::LOADING_RECORD_FETCH_RETRY_COUNT),
                        names::LOADING_RECORD_FETCH_RETRY_COUNT,
                    ),
                    (
                        "Last gate (s)",
                        g(names::LOADING_GATE_TOTAL_SECS),
                        names::LOADING_GATE_TOTAL_SECS,
                    ),
                ],
            ),
            // Render / WebGL2 texture-slot budget (C-5): the splat material's
            // bind-slot footprint against the 16-slot GLES ceiling on wasm (on
            // native the stains overlay adds one and there is no fixed ceiling).
            // The worker-spawn / msgpack-codec rows from the C-5 brief are not
            // shown: those failures live inside the off-ECS gloo-worker future
            // and never surface to the registry — see the issue for the blocker.
            (
                "Render",
                vec![(
                    SLOT_ROW_LABEL,
                    {
                        let n = g(names::RUNTIME_TEXTURE_BIND_SLOTS);
                        // The ceiling is short enough to keep in the cell; the
                        // sentence explaining it is on the hover (#1273 f189).
                        #[cfg(target_arch = "wasm32")]
                        {
                            if n == "—" { n } else { format!("{n} / 16") }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            n
                        }
                    },
                    names::RUNTIME_TEXTURE_BIND_SLOTS,
                )],
            ),
        ],
        DiagTab::Overview | DiagTab::Session => Vec::new(),
    }
}

/// The per-subsystem health tab (Pillar C-4): the [`health_cards`] for `tab`,
/// each row reading the C-2 metric readers + surfacing its anomaly badge. The
/// Overview + Session tabs render elsewhere.
fn render_health_tab(
    ui: &mut egui::Ui,
    tab: DiagTab,
    metrics: &MetricsRegistry,
    invariants: &InvariantRegistry,
    audio_muted: &mut crate::audio_mute::AudioMuted,
) {
    for (title, rows) in health_cards(tab, metrics) {
        health_card(ui, invariants, metrics, title, &rows);
        if title != AUDIO_CARD_TITLE {
            continue;
        }
        // #802/#837 close-the-loop: one interpretation line + the remedy
        // inline, so the overload badge lands somewhere actionable
        // instead of a bare number. It reads about the card above it, so
        // it is drawn there rather than at the end of the tab.
        ui.label(
            egui::RichText::new(
                "Looping voices and contact cues are the live mixing load — \
                 the first is ambience and construct hum, the second is \
                 one-shots fired by people touching things. When the \
                 overload badge is lit, muting confirms whether audio is \
                 what's dragging the frame.",
            )
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
        let mute_label = if audio_muted.0 {
            "Unmute all audio"
        } else {
            "Mute all audio"
        };
        if ui.button(mute_label).clicked() {
            audio_muted.0 = !audio_muted.0;
        }
        ui.add_space(6.0);
    }
}

/// Session-log export controls for the Session tab (Pillar A-8).
///
/// The two platforms expose the *same* NDJSON stream two different ways:
/// - **native** — the log is already appended to `session-latest.jsonl` on
///   disk, so this shows that read-only path plus a "Copy path" button (so a
///   coding agent can be pointed straight at the file). When the sink is
///   disabled (`SYMBIOS_DIAG=0` / a bare test app) there is no path, so it
///   renders a muted "(session log disabled)" instead.
/// - **wasm** — there is no filesystem, so the in-memory ring *is* the log; a
///   "Download session log" button hands [`SessionLog::drain_ndjson`] to the
///   browser as a byte-for-byte-identical `.jsonl` file the analyzer can read.
///
/// Click outcomes are reported through the app-wide toast channel
/// ([`crate::ui::toast::Toasts`], #819) — the same feedback surface the
/// landmark-link copy uses.
// Each target uses a different subset of these: native has the log path
// and its Copy button (`clipboard`), wasm has the two downloads and their
// feedback (`toasts`, `now`). Since the Copy path button started
// reporting through the clipboard queue (#1141) neither target uses all
// three, so the unused-variable lint is silenced here rather than
// splitting one function into two near-identical ones.
#[allow(unused_variables)]
fn render_log_export_controls(
    ui: &mut egui::Ui,
    session_log: &SessionLog,
    clipboard: &ClipboardQueue,
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
) {
    ui.label("Session log");

    #[cfg(not(target_arch = "wasm32"))]
    match session_log.sink_path() {
        Some(path) => {
            ui.monospace(
                egui::RichText::new(&path)
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            if ui.button("Copy path").clicked() {
                clipboard.copy(&path, "Path copied");
            }
        }
        None => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).text_weak,
                egui::RichText::new("(session log disabled)").small(),
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    if ui.button("Download session log").clicked() {
        let ndjson = session_log.drain_ndjson();
        let count = session_log.len();
        match crate::boot_params::download_text_file(
            "symbios-session-log.jsonl",
            "application/x-ndjson",
            &ndjson,
        ) {
            // "Saving", not "Downloaded" (#1274 f186). `download_text_file`
            // returns `Ok(())` as soon as it has built a Blob and clicked a
            // hidden anchor; `HtmlAnchorElement::click` reports neither a
            // download-blocker refusal nor the user dismissing the Save
            // dialog, so the past tense asserted an outcome the code has no
            // way to observe — and it asserted it in the one scenario the
            // button exists for, a user asked to send a log after a crash,
            // who then reports having sent it. Same honesty as #1141's
            // ClipboardQueue and t05's FetchStatus: describe the thing that
            // was actually done.
            Ok(()) => toasts.info(
                format!(
                    "Saving {count} {} as symbios-session-log.jsonl — check your downloads",
                    crate::ui::toolbar::plural(count, "event", "events")
                ),
                now,
            ),
            Err(e) => toasts.error(format!("Download failed ({e})"), now),
        }
    }

    // Crash-surviving tail of the *previous* session (#811) — present only
    // when boot recovered one from localStorage; the payload survives even
    // when that session died in an OOM trap before its log could be saved.
    #[cfg(target_arch = "wasm32")]
    if crate::diagnostics::crash_log::previous_session_log_bytes() > 0
        && ui.button("Download previous session log").clicked()
    {
        match crate::diagnostics::crash_log::previous_session_log() {
            Some(ndjson) => {
                match crate::boot_params::download_text_file(
                    "symbios-session-log-previous.jsonl",
                    "application/x-ndjson",
                    &ndjson,
                ) {
                    Ok(()) => toasts.info(
                        "Saving the previous session's tail — check your downloads",
                        now,
                    ),
                    Err(e) => toasts.error(format!("Download failed ({e})"), now),
                }
            }
            None => toasts.warn("Previous session tail missing from storage", now),
        }
    }
}

/// "Copy Landmark Link" — emits a shareable URL pointing at the WASM
/// build with the local player's current room DID, exact world position,
/// and yaw in degrees. Visible to visitors as well as owners (any player
/// in the room can share where they are); returns whether the button was
/// clicked so the caller can close its menu.
///
/// **The toolbar's account chip is the only call site** (#835). This
/// comment used to claim the Diagnostics Session tab drew it too; #1274
/// f192 rewrote that tab around `SessionIdentity` and left the sentence
/// standing, and #1276 f47's own value refuter is what caught it — the
/// finding built "so both surfaces are affected" on top of it. Kept as a
/// function rather than inlined because a shareable-link builder with a
/// disabled state and a clipboard side-effect is not toolbar code.
///
/// The disabled hover is the point of #1276 f47: egui shows `on_hover_text`
/// only for an ENABLED response, so a button greyed out because
/// `LocalPlayer` is momentarily absent — during spawn, and across a
/// locomotion hot-swap — said nothing at all. `on_disabled_hover_text` is
/// the idiom this file's caller already uses correctly for the visitor's
/// World Editor button (`toolbar.rs`).
pub(crate) fn landmark_link_button(
    ui: &mut egui::Ui,
    room_did: &str,
    player_tf: Option<Transform>,
    clipboard: &ClipboardQueue,
) -> bool {
    let clicked = ui
        .add_enabled(player_tf.is_some(), egui::Button::new("Copy Landmark Link"))
        .on_hover_text("Copy a link that drops a visitor exactly where you are standing")
        .on_disabled_hover_text(
            "Available once your avatar has finished appearing — the link \
             records where you are standing.",
        )
        .clicked();
    if clicked && let Some(tf) = player_tf {
        // Every locomotion preset writes its yaw into the chassis
        // transform itself — the humanoid walk controller slerps the
        // chassis rotation toward the movement direction (the rigid-body
        // solver keeps the capsule axis-aligned via `LockedAxes`), and
        // the vehicle presets are torque-driven so their chassis rotation
        // already matches the visual yaw.
        let yaw_deg = tf.rotation.to_euler(EulerRot::YXZ).0.to_degrees();
        let link = build_landmark_link(room_did, tf.translation, yaw_deg);
        clipboard.copy(&link, &format!("Copied: {link}"));
    }
    clicked
}

/// What the Session tab says about THIS session: the build it is running and
/// the two ids a support conversation asks for (#1274 f192).
///
/// Every one of these facts was already in the log — the boot `StartupSnapshot`
/// line formats "startup Boot: v… (…) … — did:…" and the Session tab renders
/// it. But as ONE transient unlabelled monospace row that falls out of the
/// 200-entry tail (and out of the 4096-event wasm ring), on the tab whose own
/// module doc calls it "the honest remainder after the account chip took
/// identity" — the remainder kept other people's identities and dropped the
/// user's own, which is the one a support conversation needs. Not a second
/// identity surface: the account chip owns who you ARE, this says what you are
/// RUNNING.
pub(crate) struct SessionIdentity {
    pub build: crate::diagnostics::snapshot::BuildInfo,
    /// Wall-clock ms of the session's first record — the id the on-disk
    /// filename and the analyzer header both key on.
    pub session_start_wall_ms: Option<u64>,
    pub world_did: Option<String>,
}

impl SessionIdentity {
    /// The block as one pasteable block of text, for "Copy session details".
    ///
    /// Pure, so the thing that reaches somebody else's inbox is testable
    /// without a running app.
    pub fn details(&self) -> String {
        let mut out = format!("Overlands {}", self.build.line());
        match self.session_start_wall_ms {
            Some(ms) => out.push_str(&format!("\nsession {ms}")),
            None => out.push_str("\nsession (not started)"),
        }
        if let Some(did) = &self.world_did {
            out.push_str(&format!("\nworld {did}"));
        }
        out
    }
}

/// The session block at the top of the Session tab.
fn render_session_identity(
    ui: &mut egui::Ui,
    identity: &SessionIdentity,
    clipboard: &ClipboardQueue,
) {
    let th = crate::ui::theme::current(ui.ctx());
    egui::Grid::new("diag-session-identity")
        .num_columns(2)
        .spacing([12.0, 3.0])
        .show(ui, |ui| {
            ui.label("Build");
            ui.monospace(identity.build.line());
            ui.end_row();

            ui.label("Session");
            match identity.session_start_wall_ms {
                Some(ms) => ui.monospace(ms.to_string()),
                None => ui.colored_label(th.text_weak, "(not started)"),
            };
            ui.end_row();

            if let Some(did) = &identity.world_did {
                ui.label("World");
                ui.monospace(egui::RichText::new(did).small());
                ui.end_row();
            }
        });
    if ui
        .button("Copy session details")
        .on_hover_text("Copy the build and session ids, for a bug report")
        .clicked()
    {
        clipboard.copy(&identity.details(), "Session details copied");
    }
    ui.separator();
}

/// A row of the Session tab's debug peer roster, lifted out of the ECS query
/// so [`render_session_tab`] is a plain function over its inputs and can be
/// rendered headlessly.
pub(crate) struct PeerRow {
    pub handle: String,
    pub did: String,
    pub muted: bool,
}

/// The floor the event log keeps when the window is too short to give it the
/// remainder (#1273 f181).
///
/// Without it the log's height is *whatever is left*, so on a short viewport
/// it collapses to nothing and the enclosing `ScrollArea` has nothing to
/// scroll — the content fits by shrinking the one part of it that matters.
/// With a floor the content honestly overflows and the scrollbar appears.
const SESSION_LOG_MIN_HEIGHT: f32 = 80.0;

/// The Session tab (#837) — the honest remainder after the account chip (#835)
/// absorbed identity / logout / Copy Landmark Link: render-debug toggles, log
/// export, a demoted debug roster, and the event log.
///
/// `wireframe` is the caller's LOCAL copy of `WireframeConfig.global`, not the
/// resource: a `&mut` through the `ResMut` marks it changed on every frame the
/// tab is drawn, which is the #879 hazard this file documents seventy lines
/// further up (#1274 f177).
fn render_session_tab(
    ui: &mut egui::Ui,
    peers: &[PeerRow],
    session_log: &SessionLog,
    identity: &SessionIdentity,
    export: &mut LogExportDeps<'_>,
    #[cfg(not(target_arch = "wasm32"))] wireframe: &mut bool,
) {
    render_session_identity(ui, identity, export.clipboard);

    // Render-debug toggles. Wireframe is native-only because the
    // wgpu POLYGON_MODE_LINE feature isn't available on WebGL2;
    // the plugin is registered with the same cfg in lib.rs.
    #[cfg(not(target_arch = "wasm32"))]
    {
        ui.label("Render");
        ui.checkbox(wireframe, "Wireframe mode")
            .on_hover_text(WIREFRAME_HINT);
        ui.separator();
    }

    // Session-log export: on-disk path + Copy (native) / Download button
    // (wasm), so the same NDJSON the analyzer reads is one click away.
    render_log_export_controls(
        ui,
        export.session_log,
        export.clipboard,
        export.toasts,
        export.now,
    );
    ui.separator();

    // Demoted duplicate of the People roster (#837): People owns
    // presence and mutes; this fold is for debugging — DIDs with a
    // copy button, collapsed by default and scroll-capped so it can
    // no longer squeeze the event log off-screen.
    egui::CollapsingHeader::new(format!("Peers (debug) ({})", peers.len())).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("diag_peer_roster")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let th = crate::ui::theme::current(ui.ctx());
                for peer in peers {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // A mute set two windows over is reflected here
                            // (#1274 f192). The roster read only the handle
                            // and the DID, so somebody the user had muted
                            // appeared identical to everyone else — a list
                            // quietly contradicting a decision they made.
                            let handle = if peer.muted {
                                format!("@{} (muted)", peer.handle)
                            } else {
                                format!("@{}", peer.handle)
                            };
                            let name = egui::RichText::new(handle);
                            ui.monospace(if peer.muted {
                                name.color(th.text_faint)
                            } else {
                                name
                            });
                            ui.monospace(
                                egui::RichText::new(&peer.did).small().color(th.text_weak),
                            );
                        });
                        if ui.small_button("Copy DID").clicked() {
                            export
                                .clipboard
                                .copy(&peer.did, &format!("Copied: {}", peer.did));
                        }
                    });
                }
                if peers.is_empty() {
                    ui.colored_label(th.text_weak, "(no peers)");
                }
            });
    });
    ui.separator();

    ui.label("Event Log");
    let log_height = ui.available_height().max(SESSION_LOG_MIN_HEIGHT);
    // The event log is a bounded tail view over the unified `SessionLog`
    // stream, so the on-disk NDJSON file and this HUD can never diverge
    // (Pillar A-6).
    let tail: Vec<&crate::diagnostics::event::SessionEvent> = session_log
        .tail(crate::config::state::MAX_DIAGNOSTICS_ENTRIES)
        .collect();
    // The VISIBLE entries, resolved to indices first (#1274 f178).
    //
    // `show_rows` addresses rows by number, and the loop below used to skip
    // the periodic metric snapshots as it went — so row N was not entry N and
    // the virtualiser could not be wrapped around it directly. Materialising
    // the kept indices costs one `Vec<usize>` over a 200-entry tail and makes
    // rows and entries the same thing.
    let rows: Vec<usize> = tail
        .iter()
        .enumerate()
        .filter(|(_, ev)| {
            // Periodic metric snapshots are file/analyzer-only telemetry —
            // keep them out of the human event log.
            !matches!(
                ev.payload,
                crate::diagnostics::event::EventPayload::MetricsSnapshot(_)
            )
        })
        .map(|(i, _)| i)
        .collect();
    let row_height = ui.text_style_height(&egui::TextStyle::Small) + ui.spacing().item_spacing.y;
    egui::ScrollArea::vertical()
        .id_salt("diag_log")
        .auto_shrink([false; 2])
        .stick_to_bottom(true)
        .max_height(log_height)
        .show_rows(ui, row_height, rows.len(), |ui, range| {
            let th = crate::ui::theme::current(ui.ctx());
            for row in range {
                let ev = &tail[rows[row]];
                note_log_row_drawn();
                ui.horizontal(|ui| {
                    ui.monospace(
                        egui::RichText::new(crate::format_elapsed_ts(ev.t_mono_secs))
                            .small()
                            .color(th.text_weak),
                    );
                    ui.monospace(
                        egui::RichText::new(ev.payload.short_line())
                            .small()
                            .color(severity_color(ui, ev.severity)),
                    );
                });
            }
        });
}

// Thread-local and not a global: `cargo test --lib` runs the suite in one
// process on many threads (#1147, #1189).
#[cfg(test)]
thread_local! {
    static LOG_ROWS_DRAWN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Count a laid-out event-log row, so the virtualisation can be measured
/// rather than asserted (#1274 f178). Compiled away outside tests.
#[inline]
fn note_log_row_drawn() {
    #[cfg(test)]
    LOG_ROWS_DRAWN.with(|c| c.set(c.get() + 1));
}

/// Zero the row counter at the start of a render pass, so a multi-pass
/// harness reports the LAST pass rather than the sum of all of them.
#[cfg(test)]
fn reset_log_rows_drawn() {
    LOG_ROWS_DRAWN.with(|c| c.set(0));
}

#[allow(clippy::too_many_arguments)]
pub fn diagnostics_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    peers: Query<&RemotePeer>,
    session_log: ResMut<SessionLog>,
    invariants: Res<InvariantRegistry>,
    metrics: Res<MetricsRegistry>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    clipboard: Res<ClipboardQueue>,
    mut active_tab: ResMut<DiagTab>,
    time: Res<Time>,
    mut audio_muted: ResMut<crate::audio_mute::AudioMuted>,
    room_did: Option<Res<crate::state::CurrentRoomDid>>,
    // Native-only: the wireframe plugin (and the resource it inserts) is
    // skipped on WASM so this parameter only exists off-web.
    #[cfg(not(target_arch = "wasm32"))] mut wireframe: ResMut<WireframeConfig>,
) {
    // Only for the tab that draws them: both of these allocate a handful of
    // Strings, and doing it on the other four tabs is per-frame work for a
    // surface nobody is looking at (t11's rule).
    let on_session_tab = *active_tab == DiagTab::Session;
    let peer_rows: Vec<PeerRow> = if on_session_tab {
        peers
            .iter()
            .map(|peer| PeerRow {
                handle: peer.handle.as_deref().unwrap_or("identifying…").to_owned(),
                did: peer.did.as_deref().unwrap_or("unknown").to_owned(),
                muted: peer.muted,
            })
            .collect()
    } else {
        Vec::new()
    };
    // Guarded-dirty (#879) for the wireframe toggle too (#1274 f177): a
    // `&mut` through the `ResMut` marks `WireframeConfig` changed on every
    // frame the tab is open, and Bevy re-runs `wireframe_config_changed`
    // — and re-uploads the global material — on each of them.
    #[cfg(not(target_arch = "wasm32"))]
    let mut wireframe_on = wireframe.global;
    let identity = on_session_tab.then(|| SessionIdentity {
        build: crate::diagnostics::snapshot::build_info(),
        session_start_wall_ms: session_log.session_start_wall_ms(),
        world_did: room_did.map(|d| d.0.clone()),
    });
    let ctx = contexts.ctx_mut().unwrap();
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Diagnostics, ctx);
    // Guarded-dirty (#879): `.open(&mut panels.diagnostics)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce — local copy in, write back only on close.
    let mut open = panels.diagnostics;
    let response = egui::Window::new("Diagnostics")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(chrome.available_rect(ctx))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // Tab selector. Overview (C-3) + the Runtime/Network/Offload
            // health cards (C-4) render the shared metrics spine; Session
            // (#837) keeps the session-debug remainder.
            // WRAPPED, not a plain horizontal (#1273 f246). Five tabs of
            // ~36 characters plus their padding come to roughly 310 pt
            // against the ~266 pt of content this window's 280 pt slot
            // gives, so the row forced the window ~50 pt wider than the
            // layout computed for it on its first open — and the badge
            // suffixes and a raised UI scale each widen it further.
            // Wrapping folds to two rows instead of pushing the window,
            // and survives both.
            ui.horizontal_wrapped(|ui| {
                for tab in DiagTab::ALL {
                    // Per-subsystem fired-count badge on the tab label (C-6):
                    // e.g. "Network (2)" while two network invariants are live.
                    let n = tab_anomaly_count(tab, &invariants);
                    let label = if n > 0 {
                        format!("{} ({n})", tab.label())
                    } else {
                        tab.label().to_string()
                    };
                    ui.selectable_value(&mut *active_tab, tab, label);
                }
            });
            ui.separator();

            // Anomaly badges (D-6) — shown on every tab so the live invariant
            // state (Critical banner + violated-rule list) is never hidden
            // behind an un-built health tab.
            let mut export = LogExportDeps {
                session_log: &session_log,
                clipboard: &clipboard,
                toasts: &mut toasts,
                now: time.elapsed_secs_f64(),
            };
            render_anomaly_section(
                ui,
                &invariants,
                (*active_tab != DiagTab::Session).then_some(&mut export),
            );

            // Overview / Perf tab (C-3). Wrapped in a ScrollArea (#837) so
            // content past the window height scrolls instead of clipping;
            // per-tab id salts keep the scroll positions independent.
            if *active_tab == DiagTab::Overview {
                egui::ScrollArea::vertical()
                    .id_salt("diag_scroll_overview")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_overview_tab(ui, &metrics, &invariants);
                    });
                return;
            }

            // Per-subsystem health cards (C-4): Runtime / Network / Offload.
            // The Offload tab's five cards used to clip at the default
            // 480px window height with no way to reach the Render card.
            if matches!(
                *active_tab,
                DiagTab::Runtime | DiagTab::Network | DiagTab::Offload
            ) {
                egui::ScrollArea::vertical()
                    .id_salt(("diag_scroll_health", active_tab.label()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_health_tab(ui, *active_tab, &metrics, &invariants, &mut audio_muted);
                    });
                return;
            }

            // The Session tab body, wrapped in the ScrollArea the other
            // four tabs have had since #837 (#1273 f181) — it was the one
            // tab with no way to reach content past the window's bottom
            // edge on a viewport too short to grow into.
            egui::ScrollArea::vertical()
                .id_salt("diag_scroll_session")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // `on_session_tab` decided both of these, and this
                    // block only runs on that tab.
                    let Some(identity) = &identity else { return };
                    render_session_tab(
                        ui,
                        &peer_rows,
                        &session_log,
                        identity,
                        &mut export,
                        #[cfg(not(target_arch = "wasm32"))]
                        &mut wireframe_on,
                    );
                });
        });
    if let Some(response) = response {
        chrome.remember(
            crate::ui::layout::UiWindow::Diagnostics,
            response.response.rect,
        );
    }
    if panels.diagnostics && !open {
        panels.diagnostics = false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if wireframe.global != wireframe_on {
        wireframe.global = wireframe_on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::anomaly::{DebouncePolicy, Verdict, default_registry};

    /// Note a violation for a real registered rule id so `active_badges` yields
    /// it (the ledger keys badges by header id).
    fn violate(reg: &mut InvariantRegistry, id: &'static str, detail: &str) {
        reg.note_verdict(
            id,
            DebouncePolicy::OncePerCondition,
            &Verdict::violated(detail.to_string()),
            1.0,
        );
    }

    #[test]
    fn collect_badges_orders_worst_first_and_carries_detail() {
        let mut reg = default_registry();
        violate(&mut reg, "runtime.frame_time_spike", "60ms"); // Warn
        violate(&mut reg, "runtime.terrain_collider_missing", "0 colliders"); // Critical

        let badges = collect_badges(&reg);
        assert!(badges.len() >= 2);
        // Critical sorts ahead of Warn.
        assert_eq!(badges[0].0, "runtime.terrain_collider_missing");
        assert_eq!(badges[0].1, Severity::Critical);
        assert_eq!(badges[0].2, "0 colliders");
        assert!(
            badges
                .iter()
                .any(|b| b.0 == "runtime.frame_time_spike" && b.1 == Severity::Warn)
        );
    }

    #[test]
    fn collect_badges_empty_when_healthy() {
        let reg = default_registry();
        assert!(collect_badges(&reg).is_empty());
    }

    /// Headless egui frame: the badge strip renders (empty, and with a Critical
    /// banner + badge active) without panicking against the real egui call path.
    #[test]
    fn anomaly_section_renders_without_panicking() {
        fn render_once(reg: &InvariantRegistry, with_export: bool) {
            let log = SessionLog::default();
            let clipboard = ClipboardQueue::default();
            let mut toasts = crate::ui::toast::Toasts::default();
            let mut export = LogExportDeps {
                session_log: &log,
                clipboard: &clipboard,
                toasts: &mut toasts,
                now: 1.0,
            };
            let ctx = egui::Context::default();
            let _ = ctx.run_ui(egui::RawInput::default(), |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    render_anomaly_section(ui, reg, with_export.then_some(&mut export));
                });
            });
        }

        // Healthy path (the "✓ No active anomalies" branch). Offered the
        // export deps and still expected not to draw them — the controls
        // hang off the Critical banner, not off the section (#1272 f175).
        render_once(&default_registry(), false);
        render_once(&default_registry(), true);

        // Critical path (banner + export controls + badge row).
        let mut reg = default_registry();
        violate(
            &mut reg,
            "runtime.terrain_collider_missing",
            "0 colliders in-game",
        );
        assert_eq!(reg.worst_active(Severity::Trace), Some(Severity::Critical));
        render_once(&reg, false);
        render_once(&reg, true);
    }

    #[test]
    fn fmt_bytes_scales_units() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(2.0 * 1024.0), "2 KiB");
        assert_eq!(fmt_bytes(3.0 * 1024.0 * 1024.0), "3.0 MiB");
        assert_eq!(fmt_bytes(2.0 * 1024.0 * 1024.0 * 1024.0), "2.00 GiB");
    }

    /// Headless egui frame: the A-8 session-log export controls render without
    /// panicking with the sink disabled (default log → "(session log disabled)")
    /// and (native) with a file sink attached so the path + "Copy path" branch
    /// is exercised. Click feedback goes through the toast channel (#819),
    /// whose queue logic is unit-tested in `crate::ui::toast`.
    #[test]
    fn log_export_controls_render_without_panicking() {
        fn render_once(log: &SessionLog) {
            let mut toasts = crate::ui::toast::Toasts::default();
            let ctx = egui::Context::default();
            let _ = ctx.run_ui(egui::RawInput::default(), |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    render_log_export_controls(
                        ui,
                        log,
                        &ClipboardQueue::default(),
                        &mut toasts,
                        1.0,
                    );
                });
            });
        }

        // Disabled sink → the muted "(session log disabled)" branch.
        render_once(&SessionLog::default());

        // Native sink attached → the path + "Copy path" button branch.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::diagnostics::Sink;
            let dir = std::env::temp_dir().join(format!("symbios-a8-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let mut log = SessionLog::default();
            log.set_sink(Sink::open_in(&dir, None));
            assert!(log.sink_path().is_some(), "sink path present once attached");
            render_once(&log);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Headless egui frame: the Overview tab renders both empty (every reader
    /// returns None/0 → "—", blank sparkline) and populated without panicking.
    #[test]
    fn overview_tab_renders_empty_and_populated_without_panicking() {
        fn render_once(m: &MetricsRegistry, reg: &InvariantRegistry) {
            let ctx = egui::Context::default();
            let _ = ctx.run_ui(egui::RawInput::default(), |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    render_overview_tab(ui, m, reg);
                });
            });
        }

        // Empty registry — the all-"—" path.
        render_once(&MetricsRegistry::default(), &default_registry());

        // Populated — frame-time ring + counts + memory readout.
        let mut m = MetricsRegistry::default();
        for v in [16.0, 20.0, 18.0, 22.0, 17.0].iter() {
            m.observe_gauge(names::RUNTIME_FRAME_TIME_MS, *v);
        }
        m.observe_gauge(names::RUNTIME_FPS, 58.0);
        m.observe_gauge(names::RUNTIME_ENTITY_COUNT, 1234.0);
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 3.0);
        m.observe_gauge(
            names::RUNTIME_MEMORY_PROCESS_RSS_BYTES,
            512.0 * 1024.0 * 1024.0,
        );
        render_once(&m, &default_registry());
    }

    /// Headless egui frame: every health tab (Runtime / Network / Offload)
    /// renders both empty and populated (incl. an active badge) without panic.
    #[test]
    fn health_tabs_render_without_panicking() {
        fn render_once(tab: DiagTab, m: &MetricsRegistry, reg: &InvariantRegistry) {
            let ctx = egui::Context::default();
            let mut muted = crate::audio_mute::AudioMuted::default();
            let _ = ctx.run_ui(egui::RawInput::default(), |root| {
                egui::CentralPanel::default().show(root, |ui| {
                    render_health_tab(ui, tab, m, reg, &mut muted);
                });
            });
        }

        let tabs = [DiagTab::Runtime, DiagTab::Network, DiagTab::Offload];
        // Empty registry — every row shows "—" / 0.
        for tab in tabs {
            render_once(tab, &MetricsRegistry::default(), &default_registry());
        }

        // Populated metrics + a live Critical badge (collider missing) that a
        // Runtime row should surface.
        let mut m = MetricsRegistry::default();
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0);
        m.incr_by(names::NET_PEER_CONNECTED_COUNT, 4);
        m.observe_hist(names::NET_AVATAR_FETCH_LATENCY_MS, 120.0);
        m.observe_hist(names::OFFLOAD_HEIGHTMAP_LATENCY_MS, 800.0);
        let mut reg = default_registry();
        violate(&mut reg, "runtime.terrain_collider_missing", "0 colliders");
        for tab in tabs {
            render_once(tab, &m, &reg);
        }
    }

    #[test]
    fn rules_for_metric_maps_known_metrics_only() {
        assert_eq!(
            rules_for_metric(names::RUNTIME_COLLIDER_COUNT).collect::<Vec<_>>(),
            vec![("runtime.terrain_collider_missing", Watch::Live)]
        );
        // One row, two rules: the wasm heap has a Warn and a Critical. Only
        // on the browser build — the native memory row is process RSS, which
        // no rule watches.
        #[cfg(target_arch = "wasm32")]
        assert_eq!(
            rules_for_metric(names::RUNTIME_MEMORY_WASM_BYTES).count(),
            2
        );
        // Unmapped metric / unknown name → no badge.
        assert_eq!(rules_for_metric(names::RUNTIME_FPS).count(), 0);
        assert_eq!(rules_for_metric("nope").count(), 0);
    }

    /// Every metric the panel renders, on every tab.
    fn rendered_metrics() -> std::collections::BTreeSet<&'static str> {
        let m = MetricsRegistry::default();
        let mut out: std::collections::BTreeSet<&'static str> =
            OVERVIEW_COUNT_ROWS.iter().map(|(_, name)| *name).collect();
        out.extend(OVERVIEW_OTHER_METRICS.iter().copied());
        for tab in DiagTab::ALL {
            for (_, rows) in health_cards(tab, &m) {
                out.extend(rows.iter().map(|(_, _, metric)| *metric));
            }
        }
        out.remove("");
        out
    }

    /// #1272 f173 + f188. The three things that had drifted apart — the rule
    /// set, the metric→rule table, and the rows that actually draw — pinned
    /// to each other in one place.
    ///
    /// Before this, five of fourteen mapped rows carried a dot that could
    /// never light (four replay-only rules and one gated to `Loading`, a
    /// state this panel never runs in), and the metrics the two live network
    /// rules evaluate on had no row on any tab — so the Network tab the
    /// anomaly dot routes a link failure to said nothing about the link.
    #[test]
    fn metric_rows_and_rules_line_up() {
        let registry = default_registry();
        let rendered = rendered_metrics();
        let mut wrong = Vec::new();

        for (metric, rule_id, watch) in METRIC_RULE_TABLE {
            let Some(rule) = registry.rules().iter().find(|r| r.header().id == *rule_id) else {
                wrong.push(format!(
                    "{rule_id}: mapped from {metric} but not registered"
                ));
                continue;
            };
            let h = rule.header();
            // The whole point: Watch says what the row's empty badge means,
            // and it has to be the truth about the rule under it.
            let expected = match (rule.has_live_body(), &h.when_state) {
                (false, _) => Watch::Analyzer,
                (true, Some(crate::state::AppState::Loading)) => Watch::WhileLoading,
                (true, _) => Watch::Live,
            };
            if expected != *watch {
                wrong.push(format!(
                    "{rule_id}: table says {watch:?}, the rule is {expected:?} \
                     (live body {}, when_state {:?})",
                    rule.has_live_body(),
                    h.when_state
                ));
            }
            // A mapped metric with no row anywhere is a rule watching a
            // number nobody can see.
            if !rendered.contains(metric) {
                wrong.push(format!(
                    "{metric}: mapped to {rule_id} but rendered on no tab"
                ));
            }
        }
        assert!(
            METRIC_RULE_TABLE.len() > 15,
            "the table shrank to {}",
            METRIC_RULE_TABLE.len()
        );
        assert!(wrong.is_empty(), "\n  {}", wrong.join("\n  "));
    }

    /// A row note is an explanation of a row, so it has to have one.
    #[test]
    fn row_notes_describe_rendered_rows() {
        let rendered = rendered_metrics();
        for metric in [
            names::RUNTIME_TEXTURE_BIND_SLOTS,
            MEMORY_ROW_METRIC,
            names::NET_SIGNAL_AWAITING_PEERS,
            names::NET_SIGNAL_PEER_LIST_LEN,
            names::RECORD_SIZE_ROOM_BYTES,
            names::RECORD_SIZE_AVATAR_BYTES,
            names::RECORD_SIZE_INVENTORY_BYTES,
            names::RUNTIME_FRAME_TIME_MAX_MS,
        ] {
            assert!(row_note(metric).is_some(), "{metric} lost its note");
            assert!(
                rendered.contains(metric),
                "{metric} has a note but no row to hang it on"
            );
        }
        assert!(row_note(names::RUNTIME_FPS).is_none());
    }

    /// #1272 f401's own claim, asserted rather than trusted: the eight
    /// signalling gauges are on a screen now.
    #[test]
    fn the_signalling_gauges_have_rows() {
        let rendered = rendered_metrics();
        for metric in [
            names::NET_SIGNAL_PEER_LIST_LEN,
            names::NET_SIGNAL_AWAITING_PEERS,
            names::NET_SIGNAL_AUTH_REJECTIONS,
            names::NET_SIGNAL_OFFERS_SENT,
            names::NET_SIGNAL_OFFERS_RECEIVED,
            names::NET_SIGNAL_ANSWERS_SENT,
            names::NET_SIGNAL_ANSWERS_RECEIVED,
            names::NET_RELAY_TOKEN_REFRESH_FAILURES,
            // #1272 f188's other named absentees.
            names::RUNTIME_FRAME_TIME_MAX_MS,
            names::RUNTIME_FRAME_HITCH_MS,
            names::RECORD_SIZE_ROOM_BYTES,
            names::NET_BROADCAST_OVERSIZE_DROPPED_COUNT,
        ] {
            assert!(rendered.contains(metric), "{metric} is still on no screen");
        }
        assert!(rendered.contains(names::NET_SIGNAL_OFFERS_INITIATED));
        // The control: a metric nobody claimed a row for is still absent, so
        // this test is checking membership rather than always passing.
        assert!(!rendered.contains(names::NET_BROADCAST_PAYLOAD_BYTES));
    }

    /// How many passes [`drawn_height`] runs before reading a height.
    ///
    /// egui runs a SIZING pass whose geometry is not the settled one, and a
    /// `Button` reads last frame's response to pick its state (t09's recipe).
    /// One pass reports the sizing pass's answer.
    const SETTLE_PASSES: usize = 5;

    /// How tall `body` came out when handed a `w` x `h` rect.
    ///
    /// Two things here were learned by printing the numbers rather than by
    /// reading the code, and both made an earlier version of this helper
    /// return a constant while every assertion built on it passed:
    ///
    /// * It draws into a CHILD `Ui` with an explicit `max_rect`, not into the
    ///   `CentralPanel`'s own — a panel's `min_rect` is the panel, whatever is
    ///   drawn in it, so the obvious version returned the viewport height for
    ///   every input.
    /// * `body` is `FnMut` and runs on EVERY pass. A `FnOnce` taken with
    ///   `Option::take` is consumed by the sizing pass, so the pass whose
    ///   height is read draws nothing and reports zero.
    ///
    /// `the_height_probe_measures_content` is the control on both.
    fn drawn_height(w: f32, h: f32, mut body: impl FnMut(&mut egui::Ui)) -> f32 {
        let ctx = egui::Context::default();
        let mut out = 0.0;
        for _ in 0..SETTLE_PASSES {
            reset_log_rows_drawn();
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(w.max(400.0), h.max(400.0)),
                    )),
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        let rect = egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(w, h));
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        body(&mut child);
                        out = child.min_rect().height();
                    });
                },
            );
        }
        out
    }

    /// The probe answers about CONTENT, not about the rect it was handed.
    #[test]
    fn the_height_probe_measures_content() {
        let one = drawn_height(280.0, 2000.0, |ui| {
            ui.label("a");
        });
        let many = drawn_height(280.0, 2000.0, |ui| {
            for _ in 0..20 {
                ui.label("a");
            }
        });
        assert!(one < 60.0, "one label reported {one} pt");
        assert!(
            many > one * 10.0,
            "20 labels reported {many} pt against {one}"
        );
        assert!(many < 2000.0, "and it is not just returning the rect");
    }

    /// #1274 f178. The log lays out only the rows on screen.
    ///
    /// Counted, not timed — there is no wall-clock harness in this repo and a
    /// timing assertion on ~400 small allocations would be flaky
    /// (t11's method). The pair is the point: the same tail, drawn the old
    /// way and the new way, into the same viewport.
    #[test]
    fn the_event_log_lays_out_only_the_rows_on_screen() {
        use crate::diagnostics::event::{EventPayload, Severity as Sev};

        let mut log = SessionLog::default();
        for i in 0..crate::config::state::MAX_DIAGNOSTICS_ENTRIES {
            log.info(
                i as f64,
                EventPayload::SessionSegmentReset {
                    reason: format!("row {i}"),
                },
            );
            // Every other entry is a metric snapshot, which the log skips —
            // this is the reason `show_rows` could not wrap the old loop
            // directly, so the fixture has to contain them.
            log.record(
                i as f64,
                Sev::Trace,
                EventPayload::SessionEnd {
                    reason: "filler".into(),
                },
            );
        }
        let kept = log
            .tail(crate::config::state::MAX_DIAGNOSTICS_ENTRIES)
            .filter(|e| {
                !matches!(
                    e.payload,
                    crate::diagnostics::event::EventPayload::MetricsSnapshot(_)
                )
            })
            .count();
        assert!(kept > 100, "the fixture only kept {kept} rows");

        let clipboard = ClipboardQueue::default();
        let mut toasts = crate::ui::toast::Toasts::default();
        let mut export = LogExportDeps {
            session_log: &log,
            clipboard: &clipboard,
            toasts: &mut toasts,
            now: 1.0,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let mut wireframe = false;
        let _ = drawn_height(280.0, 600.0, |ui| {
            render_session_tab(
                ui,
                &[],
                &log,
                &SessionIdentity {
                    build: crate::diagnostics::snapshot::build_info(),
                    session_start_wall_ms: Some(1),
                    world_did: None,
                },
                &mut export,
                #[cfg(not(target_arch = "wasm32"))]
                &mut wireframe,
            )
        });
        let drawn = LOG_ROWS_DRAWN.with(|c| c.get());

        assert!(
            drawn < kept / 2,
            "{drawn} of {kept} rows laid out into a 600 pt window"
        );
        // The lower bound matters as much as the upper: a virtualisation
        // bug that draws NOTHING satisfies "fewer than {kept}" forever.
        assert!(drawn > 0, "the log drew no rows at all");
    }

    /// #1274 f192. What "Copy session details" actually puts on the
    /// clipboard, checkable without a running app.
    #[test]
    fn the_session_details_carry_the_build_and_the_ids() {
        let identity = SessionIdentity {
            build: crate::diagnostics::snapshot::build_info(),
            session_start_wall_ms: Some(1787689231458),
            world_did: Some("did:plc:example".into()),
        };
        let text = identity.details();
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.contains("1787689231458"), "{text}");
        assert!(text.contains("did:plc:example"), "{text}");

        // A session that has not written a record yet says so rather than
        // pasting a confident zero.
        let unstarted = SessionIdentity {
            build: crate::diagnostics::snapshot::build_info(),
            session_start_wall_ms: None,
            world_did: None,
        };
        assert!(unstarted.details().contains("(not started)"));
        assert!(!unstarted.details().contains("world "));
    }

    /// #1273 f182. The band is what makes the trace readable, so the    /// #1273 f182. The band is what makes the trace readable, so the
    /// check is that a flat trace draws flat.
    #[test]
    fn a_steady_frame_trace_does_not_draw_a_mountain_range() {
        // A flawless session: 16.0 to 16.3 ms.
        let steady: Vec<f64> = (0..40).map(|i| 16.0 + (i % 4) as f64 * 0.1).collect();
        let ts: Vec<f32> = steady.iter().map(|v| spark_t(*v, FRAME_BAND_MS)).collect();
        let spread = ts.iter().cloned().fold(f32::MIN, f32::max)
            - ts.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            spread < 0.02,
            "a 0.3 ms spread should be a flat line, drew {spread:.3} of the height"
        );

        // The control: the rule that shipped normalised to the samples'
        // own min and max, so THE SAME trace filled the whole rect.
        let (lo, hi) = (
            steady.iter().cloned().fold(f64::MAX, f64::min),
            steady.iter().cloned().fold(f64::MIN, f64::max),
        );
        let old_spread = steady
            .iter()
            .map(|v| ((v - lo) / (hi - lo)) as f32)
            .fold(f32::MIN, f32::max);
        assert!(
            old_spread > 0.99,
            "the self-normalising rule is what this replaced"
        );

        // A stall is drawn AS a stall: pinned to the ceiling, not folded
        // back into the axis.
        assert_eq!(spark_t(400.0, FRAME_BAND_MS), 1.0);
        assert_eq!(spark_t(-5.0, FRAME_BAND_MS), 0.0);
        // And one budget lands exactly halfway up a two-budget band.
        assert!((spark_t(FRAME_BUDGET_MS, FRAME_BAND_MS) - 0.5).abs() < 1e-6);
    }

    /// #1273 f183. One presentation of a distribution, not two.
    #[test]
    fn the_overview_and_the_cards_show_a_distribution_the_same_way() {
        let d = Distro {
            min: 15.9,
            p50: 16.7,
            p90: 18.2,
            max: 402.0,
            mean: 17.1,
            n: 120,
        };
        assert_eq!(distro_inline(&d), "p50 16.7  p90 18.2");
        assert!(
            distro_hover(&d).contains("max 402.0"),
            "the hitch is reachable"
        );
        // The control: the full five-stat line #837 took out of the cards
        // and left on the Overview is what this replaced, and it is
        // roughly twice as wide.
        assert!(d.to_string().len() > distro_inline(&d).len() * 2);
    }

    /// #1273 f181. The strip is drawn on every tab and outside every
    /// per-tab scroll area, so its height comes straight off the tab body.
    /// With 29 rules registered it has to be bounded.
    #[test]
    fn a_full_anomaly_strip_cannot_swallow_the_tab_under_it() {
        fn section_height(reg: &InvariantRegistry) -> f32 {
            drawn_height(280.0, 2000.0, |ui| render_anomaly_section(ui, reg, None))
        }

        // A Critical in BOTH cases, so the banner is in both heights and
        // the difference is the badge LIST. Comparing a Warn-only strip
        // against a Critical one measures the banner too, and reports a
        // cap failure that is really a 40 pt red box.
        let mut one = default_registry();
        violate(&mut one, "runtime.terrain_collider_missing", "0 colliders");
        let mut all = default_registry();
        let ids: Vec<&'static str> = default_registry()
            .rules()
            .iter()
            .map(|r| r.header().id)
            .collect();
        assert!(ids.len() > 25, "only {} rules registered", ids.len());
        for id in &ids {
            violate(&mut all, id, "x");
        }
        assert_eq!(collect_badges(&all).len(), ids.len());

        let (h1, hn) = (section_height(&one), section_height(&all));
        assert!(
            hn - h1 <= ANOMALY_STRIP_MAX_HEIGHT,
            "{} badges added {:.0} pt over one; the cap is {ANOMALY_STRIP_MAX_HEIGHT}",
            ids.len(),
            hn - h1
        );
        // The control: an uncapped list of that many rows is far taller
        // than the cap, so the bound above is doing work rather than
        // being satisfied by a short list.
        assert!(
            h1 * ids.len() as f32 > ANOMALY_STRIP_MAX_HEIGHT * 3.0,
            "the un-capped strip would not have overflowed anyway"
        );
    }

    /// #1273 f181. The Session tab was the one tab with no scroll area,
    /// and the reason it "fitted" was that the event log's height is
    /// whatever is left — so on a short viewport the log shrank to nothing
    /// and the content reported that it fitted. A floor makes the overflow
    /// honest, which is what gives the enclosing `ScrollArea` something to
    /// scroll.
    #[test]
    fn the_session_tab_reports_its_overflow_instead_of_shrinking_the_log() {
        fn body_height(viewport_h: f32) -> f32 {
            let log = SessionLog::default();
            let clipboard = ClipboardQueue::default();
            let mut toasts = crate::ui::toast::Toasts::default();
            let mut export = LogExportDeps {
                session_log: &log,
                clipboard: &clipboard,
                toasts: &mut toasts,
                now: 1.0,
            };
            #[cfg(not(target_arch = "wasm32"))]
            let mut wireframe = false;
            drawn_height(280.0, viewport_h, |ui| {
                render_session_tab(
                    ui,
                    &[],
                    &log,
                    &SessionIdentity {
                        build: crate::diagnostics::snapshot::build_info(),
                        session_start_wall_ms: None,
                        world_did: None,
                    },
                    &mut export,
                    #[cfg(not(target_arch = "wasm32"))]
                    &mut wireframe,
                )
            })
        }

        // A viewport far too short for the tab's fixed content.
        let short = 120.0;
        assert!(
            body_height(short) > short,
            "the body has to say it does not fit, or the scroll area is inert"
        );
        // And the log keeps its floor rather than collapsing to zero: the
        // difference between a tall and a short viewport is bounded by the
        // log's flexible share, not by the whole body.
        assert!(
            body_height(short) >= SESSION_LOG_MIN_HEIGHT,
            "the log kept nothing at all"
        );
        // The control: with room, the body fits and nothing scrolls.
        let roomy = 900.0;
        assert!(body_height(roomy) <= roomy);
    }

    /// #1273 f246. The tab row is wider than the window's slot; wrapping is
    /// what stops it pushing the window instead.
    #[test]
    fn the_tab_row_folds_rather_than_widening_the_window() {
        /// Lay `wrapped` / not out in a `width`-wide column and report how
        /// wide the row came out.
        fn row_width(width: f32, wrapped: bool) -> f32 {
            let ctx = egui::Context::default();
            let mut w = 0.0;
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(2000.0, 400.0),
                    )),
                    ..Default::default()
                },
                |root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        let rect =
                            egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(width, 400.0));
                        let mut column = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        let body = |ui: &mut egui::Ui| {
                            for tab in DiagTab::ALL {
                                let mut active = DiagTab::Overview;
                                ui.selectable_value(&mut active, tab, tab.label());
                            }
                        };
                        if wrapped {
                            column.horizontal_wrapped(body);
                        } else {
                            column.horizontal(body);
                        }
                        w = column.min_rect().width();
                    });
                },
            );
            w
        }

        // A column too narrow for five tabs on one line. The control is
        // the un-wrapped row, which reports MORE than it was given — and
        // that overflow is what `Resize` adds to the window every frame.
        let narrow = 120.0;
        assert!(
            row_width(narrow, false) > narrow,
            "the un-wrapped row is what pushed the window wider"
        );
        assert!(
            row_width(narrow, true) <= narrow + 1.0,
            "the wrapped row stays inside the width it was handed"
        );
    }

    #[test]
    fn tab_anomaly_count_attributes_by_subsystem() {
        let mut reg = default_registry();
        violate(&mut reg, "runtime.terrain_collider_missing", "x"); // Runtime
        violate(&mut reg, "net.identity_spoof_burst", "y"); // Network
        violate(&mut reg, "loading.gate_stall", "z"); // Loading → Offload tab

        assert_eq!(tab_anomaly_count(DiagTab::Runtime, &reg), 1);
        assert_eq!(tab_anomaly_count(DiagTab::Network, &reg), 1);
        // The Offload tab owns the Loading subsystem's gate rules.
        assert_eq!(tab_anomaly_count(DiagTab::Offload, &reg), 1);
        // Overview aggregates everything.
        assert_eq!(tab_anomaly_count(DiagTab::Overview, &reg), 3);
        assert_eq!(tab_anomaly_count(DiagTab::Session, &reg), 0);
    }
}
