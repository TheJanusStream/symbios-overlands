//! Diagnostics HUD — a five-tab panel (see [`DiagTab`]). The Overview /
//! Runtime / Network / Offload tabs draw the frame-time sparkline and
//! per-subsystem metric health cards + anomaly badges over the shared
//! metrics registry. The Identity tab carries the historic panel: local
//! identity, current room DID, peer roster with per-peer mute toggles,
//! the "Copy Landmark Link" share button (bundles the current room DID +
//! player position + yaw into a URL that the WASM build opens directly
//! and the native build accepts as `--did=… --pos=… --rot=…`), a
//! native-only wireframe-mode checkbox (skipped on WebGL2 where
//! `POLYGON_MODE_LINE` is unavailable), a scrolling event log, and the
//! log-out button (routed through [`crate::ui::unsaved_guard`] so
//! unpublished edits are never silently discarded).

#[cfg(not(target_arch = "wasm32"))]
use bevy::pbr::wireframe::WireframeConfig;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::boot_params::{build_landmark_link, write_to_clipboard};
use crate::diagnostics::anomaly::InvariantRegistry;
use crate::diagnostics::event::{Severity, Subsystem};
use crate::diagnostics::{MetricsRegistry, SessionLog, names};
use crate::state::{CurrentRoomDid, LocalPlayer, RemotePeer};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};

/// Which tab of the Diagnostics panel is showing. Identity carries the historic
/// panel (preserved verbatim); Overview (C-3) draws the frame-time sparkline +
/// counts + memory, and the Runtime / Network / Offload tabs (C-4) draw the
/// per-subsystem health cards over the shared metrics registry + anomaly badges.
/// Default is Identity so the panel opens exactly as it did before tabs existed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DiagTab {
    Overview,
    Runtime,
    Network,
    Offload,
    #[default]
    Identity,
}

impl DiagTab {
    const ALL: [DiagTab; 5] = [
        DiagTab::Overview,
        DiagTab::Runtime,
        DiagTab::Network,
        DiagTab::Offload,
        DiagTab::Identity,
    ];

    fn label(self) -> &'static str {
        match self {
            DiagTab::Overview => "Overview",
            DiagTab::Runtime => "Runtime",
            DiagTab::Network => "Network",
            DiagTab::Offload => "Offload",
            DiagTab::Identity => "Identity",
        }
    }
}

/// Map a [`Severity`] to the HUD colour used for both the event-log line tint
/// and the anomaly badges / toolbar dot (D-6), so a warning reads the same amber
/// everywhere. `pub(crate)` so [`crate::ui::toolbar`] can colour its worst-active
/// dot identically.
pub(crate) fn severity_color(sev: Severity) -> egui::Color32 {
    use crate::config::ui::diagnostics as cfg;
    let [r, g, b] = match sev {
        Severity::Trace => cfg::SEVERITY_TRACE_RGB,
        Severity::Info => cfg::SEVERITY_INFO_RGB,
        Severity::Warn => cfg::SEVERITY_WARN_RGB,
        Severity::Error => cfg::SEVERITY_ERROR_RGB,
        Severity::Critical => cfg::SEVERITY_CRITICAL_RGB,
    };
    egui::Color32::from_rgb(r, g, b)
}

/// The currently-violated rules as `(id, severity, last detail, fire count)`,
/// worst-severity first (ties broken by id for stable output). The pure data
/// behind the badge strip, unit-tested independently of egui.
fn collect_badges(invariants: &InvariantRegistry) -> Vec<(&'static str, Severity, String, u64)> {
    let mut badges: Vec<(&'static str, Severity, String, u64)> = invariants
        .active_badges()
        .map(|(id, sev, st)| (id, sev, st.last_detail.clone(), st.fire_count))
        .collect();
    badges.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    badges
}

/// Render the anomaly badge strip (Pillar D-6): a persistent red banner while
/// any `Critical` invariant is active, then one severity-coloured badge per
/// currently-violated rule with its last detail, worst-severity first. Reads the
/// same [`InvariantRegistry::active_badges`] / [`InvariantRegistry::worst_active`]
/// ledger the live engine writes, so the panel mirrors the anomaly engine's
/// current state. Shown on every tab so the health signal is never hidden.
fn render_anomaly_section(ui: &mut egui::Ui, invariants: &InvariantRegistry) {
    let badges = collect_badges(invariants);

    // Persistent banner while any Critical invariant is active — the same
    // Frame idiom the room-recovery banner uses.
    let crit = badges.iter().filter(|b| b.1 == Severity::Critical).count();
    if crit > 0 {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(90, 20, 20))
            .inner_margin(6.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 210, 210),
                    format!(
                        "⚠ {crit} CRITICAL invariant{} active — session health is compromised.",
                        if crit == 1 { "" } else { "s" }
                    ),
                );
            });
        ui.add_space(4.0);
    }

    if badges.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(130, 190, 130),
            "✓ No active anomalies",
        );
    } else {
        ui.label(format!("Active Anomalies ({})", badges.len()));
        for (id, sev, detail, fires) in &badges {
            let color = severity_color(*sev);
            ui.horizontal(|ui| {
                ui.colored_label(color, "●");
                ui.monospace(egui::RichText::new(*id).small().color(color));
                if *fires > 1 {
                    ui.label(
                        egui::RichText::new(format!("×{fires}"))
                            .small()
                            .color(egui::Color32::DARK_GRAY),
                    );
                }
                if !detail.is_empty() {
                    ui.monospace(
                        egui::RichText::new(format!("— {detail}"))
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        }
    }
    ui.separator();
}

/// The single metric→invariant-rule mapping the GUI badges read (Pillar C-6):
/// which rule's live state badges each metric row. A metric absent here shows no
/// badge; a mapped rule only lights up while it is *live*-violated (the
/// replay-only rules surface in the analyzer/log, not the live panel).
const METRIC_RULE_TABLE: &[(&str, &str)] = &[
    (names::RUNTIME_FRAME_TIME_MS, "runtime.frame_time_spike"),
    (
        names::RUNTIME_MESH_HANDLE_COUNT,
        "runtime.asset_handle_spike",
    ),
    (
        names::RUNTIME_COLLIDER_COUNT,
        "runtime.terrain_collider_missing",
    ),
    (
        names::RUNTIME_SHAPE_MESH_CACHE_LEN,
        "runtime.shape_mesh_cache_growth",
    ),
    (names::RUNTIME_RESPAWN_COUNT, "runtime.respawn_thrashing"),
    (names::NET_PEER_DISCONNECTED_COUNT, "net.peer_churn_spike"),
    (
        names::NET_IDENTITY_SPOOFED_COUNT,
        "net.identity_spoof_burst",
    ),
    (
        names::NET_OFFER_ACCEPTED_COUNT,
        "net.offer_acceptance_anomaly",
    ),
    (
        names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS,
        "offload.ambient_bake_stall",
    ),
    (
        names::OFFLOAD_JOB_ERROR_COUNT,
        "offload.task_never_resolves",
    ),
    (
        names::LOADING_RECORD_FETCH_LATENCY_MS,
        "loading.record_fetch_exhausted",
    ),
    (names::LOADING_GATE_TOTAL_SECS, "loading.gate_stall"),
];

/// The invariant rule that badges `metric`, if any (see [`METRIC_RULE_TABLE`]).
fn rule_for_metric(metric: &str) -> Option<&'static str> {
    METRIC_RULE_TABLE
        .iter()
        .find(|(m, _)| *m == metric)
        .map(|(_, rule)| *rule)
}

/// A per-metric anomaly pill (Pillar C-6): when the rule that badges `metric_id`
/// (via [`METRIC_RULE_TABLE`]) is currently live-violated, draw a severity-
/// coloured `●` beside the row, hovering the rule name + when it last fired + its
/// detail. No mapping, or nothing active, draws nothing — so a clean session (or
/// a build without the anomaly engine populated) shows a plain panel.
fn anomaly_badge(ui: &mut egui::Ui, invariants: &InvariantRegistry, metric_id: &str) {
    let Some(rule_id) = rule_for_metric(metric_id) else {
        return;
    };
    if let Some((_, sev, st)) = invariants.active_badges().find(|(id, _, _)| *id == rule_id) {
        ui.colored_label(severity_color(sev), "●")
            .on_hover_text(format!(
                "{rule_id} — last fired {:.0}s: {}",
                st.last_fired_secs, st.last_detail
            ));
    }
}

/// The count of live-violated invariants attributable to a tab, for its label
/// badge (C-6). Overview aggregates everything; the subsystem tabs count their
/// own subsystem (Offload also owns the loading-gate rules).
fn tab_anomaly_count(tab: DiagTab, invariants: &InvariantRegistry) -> usize {
    match tab {
        DiagTab::Overview => invariants.active_badges().count(),
        DiagTab::Runtime => invariants.active_count_for(Subsystem::Runtime),
        DiagTab::Network => invariants.active_count_for(Subsystem::Network),
        DiagTab::Offload => {
            invariants.active_count_for(Subsystem::Offload)
                + invariants.active_count_for(Subsystem::Loading)
        }
        DiagTab::Identity => 0,
    }
}

/// Hand-rolled polyline sparkline over a metric's recent history, drawn straight
/// onto the panel via `ui.painter()` (no `egui_plot` dependency). Auto-scales to
/// the sample min/max; fewer than two samples just shows the backing strip.
fn sparkline(ui: &mut egui::Ui, samples: &[f64], height: f32) {
    let width = ui.available_width().max(32.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
    if samples.len() < 2 {
        return;
    }
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in samples {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let range = (hi - lo).max(1e-6);
    let n = samples.len();
    let pts: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let x = rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
            let t = ((v - lo) / range) as f32;
            egui::pos2(x, rect.bottom() - rect.height() * t)
        })
        .collect();
    painter.add(egui::Shape::line(
        pts,
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(120, 200, 140)),
    ));
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
fn memory_readout(ui: &mut egui::Ui, metrics: &MetricsRegistry) {
    #[cfg(not(target_arch = "wasm32"))]
    let (label, name) = ("Process RSS", names::RUNTIME_MEMORY_PROCESS_RSS_BYTES);
    #[cfg(target_arch = "wasm32")]
    let (label, name) = ("WASM heap", names::RUNTIME_MEMORY_WASM_BYTES);
    match metrics.gauge_latest(name) {
        Some(bytes) => {
            ui.monospace(format!("{label}: {}", fmt_bytes(bytes)));
        }
        None => {
            ui.colored_label(egui::Color32::GRAY, format!("{label}: unavailable"));
        }
    }
}

/// The Overview / Perf tab (Pillar C-3): a frame-time sparkline + an FPS/distro
/// line, a compact grid of the live entity/asset/collider counts, and the
/// memory readout. Each row surfaces its anomaly badge from the shared ledger.
fn render_overview_tab(
    ui: &mut egui::Ui,
    metrics: &MetricsRegistry,
    invariants: &InvariantRegistry,
) {
    ui.label("Frame time (ms, last ~2 min)");
    sparkline(ui, &metrics.ring_slice(names::RUNTIME_FRAME_TIME_MS), 40.0);
    ui.horizontal(|ui| {
        let fps = metrics
            .gauge_latest(names::RUNTIME_FPS)
            .map(|f| format!("{f:.0}"))
            .unwrap_or_else(|| "—".to_string());
        ui.monospace(format!("FPS {fps}"));
        let frame = metrics
            .gauge_distro(names::RUNTIME_FRAME_TIME_MS)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".to_string());
        ui.monospace(egui::RichText::new(format!("· frame {frame}")).small());
        anomaly_badge(ui, invariants, names::RUNTIME_FRAME_TIME_MS);
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
            count_row(ui, "Entities", names::RUNTIME_ENTITY_COUNT);
            count_row(ui, "Mesh handles", names::RUNTIME_MESH_HANDLE_COUNT);
            count_row(ui, "Material handles", names::RUNTIME_MATERIAL_HANDLE_COUNT);
            count_row(ui, "Image handles", names::RUNTIME_IMAGE_HANDLE_COUNT);
            count_row(ui, "Colliders", names::RUNTIME_COLLIDER_COUNT);
            count_row(ui, "ShapeMeshCache", names::RUNTIME_SHAPE_MESH_CACHE_LEN);
        });

    ui.separator();
    memory_readout(ui, metrics);
}

/// One subsystem health card: a titled `egui::Frame` wrapping a 3-column grid of
/// `(label, value, anomaly badge)` rows. `rows` is `(label, value, metric_id)` —
/// the metric id keys the badge via [`METRIC_RULE_TABLE`] (C-6); a metric with no
/// mapped rule simply draws no dot.
fn health_card(
    ui: &mut egui::Ui,
    invariants: &InvariantRegistry,
    title: &str,
    rows: &[(&str, String, &str)],
) {
    egui::Frame::new()
        .fill(egui::Color32::from_gray(28))
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
                        ui.monospace(value.as_str());
                        anomaly_badge(ui, invariants, metric);
                        ui.end_row();
                    }
                });
        });
    ui.add_space(6.0);
}

/// The per-subsystem health tab (Pillar C-4): one or more `egui::Frame` cards of
/// the live metrics for `tab` (Runtime / Network / Offload), each row reading the
/// C-2 metric readers + surfacing its anomaly badge. The Overview + Identity tabs
/// render elsewhere.
fn render_health_tab(
    ui: &mut egui::Ui,
    tab: DiagTab,
    metrics: &MetricsRegistry,
    invariants: &InvariantRegistry,
) {
    // Row-value shorthands over the C-2 readers: gauge latest / counter / distro.
    let g = |name: &str| {
        metrics
            .gauge_latest(name)
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "—".to_string())
    };
    let c = |name: &str| metrics.counter_value(name).to_string();
    let h = |name: &str| metrics.hist_distro_str(name);

    match tab {
        DiagTab::Runtime => health_card(
            ui,
            invariants,
            "Runtime",
            &[
                (
                    "Frame time (ms)",
                    g(names::RUNTIME_FRAME_TIME_MS),
                    names::RUNTIME_FRAME_TIME_MS,
                ),
                ("FPS", g(names::RUNTIME_FPS), names::RUNTIME_FPS),
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
        DiagTab::Network => {
            health_card(
                ui,
                invariants,
                "Peers",
                &[
                    (
                        "Connected",
                        c(names::NET_PEER_CONNECTED_COUNT),
                        names::NET_PEER_CONNECTED_COUNT,
                    ),
                    (
                        "Disconnected",
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
            );
            health_card(
                ui,
                invariants,
                "Avatar fetch",
                &[
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
                ],
            );
            health_card(
                ui,
                invariants,
                "Jitter & offers",
                &[
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
                ],
            );
        }
        DiagTab::Offload => {
            health_card(
                ui,
                invariants,
                "Async jobs",
                &[
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
            );
            health_card(
                ui,
                invariants,
                "Loading gate",
                &[
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
            );
            // Render / WebGL2 texture-slot budget (C-5): the splat material's
            // bind-slot footprint against the 16-slot GLES ceiling on wasm (on
            // native the stains overlay adds one and there is no fixed ceiling).
            // The worker-spawn / msgpack-codec rows from the C-5 brief are not
            // shown: those failures live inside the off-ECS gloo-worker future
            // and never surface to the registry — see the issue for the blocker.
            #[cfg(target_arch = "wasm32")]
            let slot_note = " / 16 (WebGL2 ceiling)";
            #[cfg(not(target_arch = "wasm32"))]
            let slot_note = " (WebGPU — no fixed ceiling)";
            let slots = metrics
                .gauge_latest(names::RUNTIME_TEXTURE_BIND_SLOTS)
                .map(|v| format!("{}{slot_note}", v as u32))
                .unwrap_or_else(|| "—".to_string());
            health_card(
                ui,
                invariants,
                "Render",
                &[(
                    "Splat texture slots",
                    slots,
                    names::RUNTIME_TEXTURE_BIND_SLOTS,
                )],
            );
        }
        DiagTab::Overview | DiagTab::Identity => {}
    }
}

/// Session-log export controls for the Identity tab (Pillar A-8).
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
/// `status` carries a transient `(message, shown_at_secs)` toast (mirroring the
/// landmark-link copy feedback) rendered for a few seconds after a click.
fn render_log_export_controls(
    ui: &mut egui::Ui,
    session_log: &SessionLog,
    status: &mut Option<(String, f64)>,
    now: f64,
) {
    ui.label("Session log");

    #[cfg(not(target_arch = "wasm32"))]
    match session_log.sink_path() {
        Some(path) => {
            ui.monospace(
                egui::RichText::new(&path)
                    .small()
                    .color(egui::Color32::GRAY),
            );
            if ui.button("Copy path").clicked() {
                *status = Some(match write_to_clipboard(&path) {
                    Ok(()) => ("Path copied".to_string(), now),
                    Err(e) => (format!("Copy failed ({e})"), now),
                });
            }
        }
        None => {
            ui.colored_label(
                egui::Color32::GRAY,
                egui::RichText::new("(session log disabled)").small(),
            );
        }
    }

    #[cfg(target_arch = "wasm32")]
    if ui.button("Download session log").clicked() {
        let ndjson = session_log.drain_ndjson();
        let count = session_log.len();
        *status = Some(
            match crate::boot_params::download_text_file(
                "symbios-session-log.jsonl",
                "application/x-ndjson",
                &ndjson,
            ) {
                Ok(()) => (format!("Downloaded {count} events"), now),
                Err(e) => (format!("Download failed ({e})"), now),
            },
        );
    }

    if let Some((msg, at)) = status.as_ref()
        && now - at < 6.0
    {
        ui.colored_label(
            egui::Color32::from_rgb(160, 200, 160),
            egui::RichText::new(msg).small(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn diagnostics_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut peers: Query<&mut RemotePeer>,
    mut session_log: ResMut<SessionLog>,
    invariants: Res<InvariantRegistry>,
    metrics: Res<MetricsRegistry>,
    mut landmark_status: Local<Option<(String, f64)>>,
    mut log_export_status: Local<Option<(String, f64)>>,
    mut active_tab: Local<DiagTab>,
    time: Res<Time>,
    local_player_q: Query<&Transform, With<LocalPlayer>>,
    // Native-only: the wireframe plugin (and the resource it inserts) is
    // skipped on WASM so this parameter only exists off-web.
    #[cfg(not(target_arch = "wasm32"))] mut wireframe: ResMut<WireframeConfig>,
) {
    use crate::config::ui::diagnostics as cfg;

    egui::Window::new("Diagnostics")
        .open(&mut panels.diagnostics)
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            // Tab selector. The historic panel content lives under Identity;
            // Overview (C-3) + the Runtime/Network/Offload health cards (C-4)
            // render the shared metrics spine.
            ui.horizontal(|ui| {
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
            render_anomaly_section(ui, &invariants);

            // Overview / Perf tab (C-3).
            if *active_tab == DiagTab::Overview {
                render_overview_tab(ui, &metrics, &invariants);
                return;
            }

            // Per-subsystem health cards (C-4): Runtime / Network / Offload.
            if matches!(
                *active_tab,
                DiagTab::Runtime | DiagTab::Network | DiagTab::Offload
            ) {
                render_health_tab(ui, *active_tab, &metrics, &invariants);
                return;
            }

            ui.label("Local Identity");
            match &session {
                Some(sess) => {
                    ui.monospace(format!("@{}", sess.handle));
                    ui.monospace(
                        egui::RichText::new(&sess.did)
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
                None => {
                    ui.colored_label(egui::Color32::GRAY, "(not authenticated)");
                }
            }

            if ui.button("Log out").clicked() {
                // Route through the unsaved-edits guard instead of
                // flipping the state directly: it transitions to
                // `AppState::Login` immediately when nothing is dirty,
                // and otherwise offers Publish / Discard / Cancel first.
                commands.insert_resource(UnsavedGuard::new(GuardedAction::Logout));
            }

            // Render-debug toggles. Wireframe is native-only because the
            // wgpu POLYGON_MODE_LINE feature isn't available on WebGL2;
            // the plugin is registered with the same cfg in lib.rs.
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.separator();
                ui.label("Render");
                ui.checkbox(&mut wireframe.global, "Wireframe mode");
            }

            ui.separator();

            if let Some(room) = &room_did {
                ui.label("Room");
                ui.monospace(
                    egui::RichText::new(&room.0)
                        .small()
                        .color(egui::Color32::GRAY),
                );

                // "Copy Landmark Link" — emits a shareable URL pointing at
                // the WASM build with the local player's current room DID,
                // exact world position, and yaw in degrees. Visible to
                // visitors as well as owners (any player in the room can
                // share where they are).
                let player_tf = local_player_q.single().ok().copied();
                let can_copy = player_tf.is_some();
                if ui
                    .add_enabled(can_copy, egui::Button::new("Copy Landmark Link"))
                    .clicked()
                    && let Some(tf) = player_tf
                {
                    // Every locomotion preset writes its yaw into the
                    // chassis transform itself — the humanoid walk
                    // controller now slerps the chassis rotation toward
                    // the movement direction (the rigid-body solver
                    // keeps the capsule axis-aligned via `LockedAxes`),
                    // and the vehicle presets are torque-driven so their
                    // chassis rotation already matches the visual yaw.
                    let yaw_rad = tf.rotation.to_euler(EulerRot::YXZ).0;
                    let yaw_deg = yaw_rad.to_degrees();
                    let link = build_landmark_link(&room.0, tf.translation, yaw_deg);
                    let now = time.elapsed_secs_f64();
                    *landmark_status = Some(match write_to_clipboard(&link) {
                        Ok(()) => (format!("Copied: {link}"), now),
                        Err(e) => (format!("Copy failed ({e}); {link}"), now),
                    });
                }
                if let Some((msg, at)) = landmark_status.as_ref() {
                    let ago = (time.elapsed_secs_f64() - at).max(0.0);
                    if ago < 6.0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(160, 200, 160),
                            egui::RichText::new(msg).small(),
                        );
                    }
                }
                ui.separator();
            }

            let peer_count = peers.iter().count();
            ui.label(format!("Peers ({})", peer_count));

            for mut peer in peers.iter_mut() {
                let handle = peer.handle.as_deref().unwrap_or("identifying…").to_owned();
                let did = peer.did.as_deref().unwrap_or("unknown").to_owned();
                let dot_color = if peer.muted {
                    egui::Color32::GRAY
                } else {
                    egui::Color32::GREEN
                };
                let mut muted = peer.muted;
                ui.horizontal(|ui| {
                    ui.colored_label(dot_color, "●");
                    ui.vertical(|ui| {
                        ui.monospace(format!("@{}", handle));
                        ui.monospace(egui::RichText::new(&did).small().color(egui::Color32::GRAY));
                    });
                    ui.checkbox(&mut muted, "Mute");
                });
                // Guard the write so Bevy's change-detection flag is only
                // raised when the mute state actually flips.  An unconditional
                // assignment marks `RemotePeer` as `Changed` every frame and
                // invalidates any `Changed<RemotePeer>` filters downstream.
                if peer.muted != muted {
                    peer.muted = muted;
                    crate::ui::people::log_peer_mute_toggled(
                        &mut session_log,
                        time.elapsed_secs_f64(),
                        peer.peer_id.to_string(),
                        muted,
                    );
                }
            }

            if peer_count == 0 {
                ui.colored_label(egui::Color32::GRAY, "(no peers)");
            }

            ui.separator();

            // Session-log export: on-disk path + Copy (native) / Download button
            // (wasm), so the same NDJSON the analyzer reads is one click away.
            render_log_export_controls(
                ui,
                &session_log,
                &mut log_export_status,
                time.elapsed_secs_f64(),
            );
            ui.separator();

            ui.label("Event Log");
            let log_height = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("diag_log")
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .max_height(log_height)
                .show(ui, |ui| {
                    // The event log is now a bounded tail view over the unified
                    // `SessionLog` stream, so the on-disk NDJSON file and this
                    // HUD can never diverge (Pillar A-6).
                    for ev in session_log.tail(crate::config::state::MAX_DIAGNOSTICS_ENTRIES) {
                        // Periodic metric snapshots are file/analyzer-only
                        // telemetry — keep them out of the human event log.
                        if matches!(
                            ev.payload,
                            crate::diagnostics::event::EventPayload::MetricsSnapshot(_)
                        ) {
                            continue;
                        }
                        let ts = crate::format_elapsed_ts(ev.t_mono_secs);
                        ui.horizontal(|ui| {
                            ui.monospace(
                                egui::RichText::new(ts).small().color(egui::Color32::GRAY),
                            );
                            ui.monospace(
                                egui::RichText::new(ev.payload.short_line())
                                    .small()
                                    .color(severity_color(ev.severity)),
                            );
                        });
                    }
                });
        });
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
        fn render_once(reg: &InvariantRegistry) {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_anomaly_section(ui, reg);
                });
            });
        }

        // Healthy path (the "✓ No active anomalies" branch).
        render_once(&default_registry());

        // Critical path (banner + badge row).
        let mut reg = default_registry();
        violate(
            &mut reg,
            "runtime.terrain_collider_missing",
            "0 colliders in-game",
        );
        assert_eq!(reg.worst_active(), Some(Severity::Critical));
        render_once(&reg);
    }

    #[test]
    fn fmt_bytes_scales_units() {
        assert_eq!(fmt_bytes(512.0), "512 B");
        assert_eq!(fmt_bytes(2.0 * 1024.0), "2 KiB");
        assert_eq!(fmt_bytes(3.0 * 1024.0 * 1024.0), "3.0 MiB");
        assert_eq!(fmt_bytes(2.0 * 1024.0 * 1024.0 * 1024.0), "2.00 GiB");
    }

    /// Headless egui frame: the A-8 session-log export controls render without
    /// panicking with the sink disabled (default log → "(session log disabled)"),
    /// with a transient toast active, and (native) with a file sink attached so
    /// the path + "Copy path" branch is exercised.
    #[test]
    fn log_export_controls_render_without_panicking() {
        fn render_once(log: &SessionLog, mut status: Option<(String, f64)>) {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_log_export_controls(ui, log, &mut status, 1.0);
                });
            });
        }

        // Disabled sink → the muted "(session log disabled)" branch, no toast.
        render_once(&SessionLog::default(), None);
        // A recent toast still renders (within the 6 s fade window).
        render_once(&SessionLog::default(), Some(("Path copied".into(), 1.0)));

        // Native sink attached → the path + "Copy path" button branch.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use crate::diagnostics::Sink;
            let dir = std::env::temp_dir().join(format!("symbios-a8-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let mut log = SessionLog::default();
            log.set_sink(Sink::open_in(&dir, None));
            assert!(log.sink_path().is_some(), "sink path present once attached");
            render_once(&log, None);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// Headless egui frame: the Overview tab renders both empty (every reader
    /// returns None/0 → "—", blank sparkline) and populated without panicking.
    #[test]
    fn overview_tab_renders_empty_and_populated_without_panicking() {
        fn render_once(m: &MetricsRegistry, reg: &InvariantRegistry) {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
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
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_health_tab(ui, tab, m, reg);
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
    fn rule_for_metric_maps_known_metrics_only() {
        assert_eq!(
            rule_for_metric(names::RUNTIME_COLLIDER_COUNT),
            Some("runtime.terrain_collider_missing")
        );
        assert_eq!(
            rule_for_metric(names::NET_IDENTITY_SPOOFED_COUNT),
            Some("net.identity_spoof_burst")
        );
        // Unmapped metric / unknown name → no badge.
        assert_eq!(rule_for_metric(names::RUNTIME_FPS), None);
        assert_eq!(rule_for_metric("nope"), None);
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
        assert_eq!(tab_anomaly_count(DiagTab::Identity, &reg), 0);
    }
}
