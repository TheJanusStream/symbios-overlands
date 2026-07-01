//! Diagnostics HUD: local identity, current room DID, peer roster with per-
//! peer mute toggles, the "Copy Landmark Link" share button (bundles the
//! current room DID + player position + yaw into a URL that the WASM build
//! opens directly and the native build accepts as `--did=… --pos=… --rot=…`),
//! a native-only wireframe-mode checkbox (skipped on WebGL2 where
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
use crate::diagnostics::event::Severity;
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
    match sev {
        Severity::Trace => egui::Color32::DARK_GRAY,
        Severity::Info => egui::Color32::LIGHT_GRAY,
        Severity::Warn => egui::Color32::from_rgb(210, 170, 90),
        Severity::Error => egui::Color32::from_rgb(210, 120, 90),
        Severity::Critical => egui::Color32::from_rgb(220, 90, 90),
    }
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

/// A per-metric anomaly dot (Pillar C, reading the D-6 badge ledger): draws a
/// severity-coloured `●` beside a metric row when the named invariant rule is
/// currently violated, with its detail on hover. A row with no associated rule
/// passes `""` (never matches → nothing drawn). C-6 (#618) extends this into the
/// full metric→rule table across every subsystem card.
fn anomaly_badge(ui: &mut egui::Ui, invariants: &InvariantRegistry, rule_id: &str) {
    if let Some((_, sev, st)) = invariants.active_badges().find(|(id, _, _)| *id == rule_id) {
        ui.colored_label(severity_color(sev), "●")
            .on_hover_text(st.last_detail.clone());
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
        egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 200, 140)),
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
        anomaly_badge(ui, invariants, "runtime.frame_time_spike");
    });

    ui.separator();

    // Live counts — a compact grid; each metric row shows its anomaly badge.
    egui::Grid::new("diag-overview-counts")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let count_row = |ui: &mut egui::Ui, label: &str, name: &str, rule: &str| {
                ui.label(label);
                let v = metrics
                    .gauge_latest(name)
                    .map(|n| format!("{n:.0}"))
                    .unwrap_or_else(|| "—".to_string());
                ui.monospace(v);
                anomaly_badge(ui, invariants, rule);
                ui.end_row();
            };
            count_row(ui, "Entities", names::RUNTIME_ENTITY_COUNT, "");
            count_row(
                ui,
                "Mesh handles",
                names::RUNTIME_MESH_HANDLE_COUNT,
                "runtime.asset_handle_spike",
            );
            count_row(
                ui,
                "Material handles",
                names::RUNTIME_MATERIAL_HANDLE_COUNT,
                "",
            );
            count_row(
                ui,
                "Colliders",
                names::RUNTIME_COLLIDER_COUNT,
                "runtime.terrain_collider_missing",
            );
            count_row(
                ui,
                "ShapeMeshCache",
                names::RUNTIME_SHAPE_MESH_CACHE_LEN,
                "runtime.shape_mesh_cache_growth",
            );
        });

    ui.separator();
    memory_readout(ui, metrics);
}

/// One subsystem health card: a titled `egui::Frame` wrapping a 3-column grid of
/// `(label, value, anomaly badge)` rows. `rows` is `(label, value, rule_id)`; a
/// row with no associated rule passes `""` (no dot).
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
                    for (label, value, rule) in rows {
                        ui.label(*label);
                        ui.monospace(value.as_str());
                        anomaly_badge(ui, invariants, rule);
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
                    "runtime.frame_time_spike",
                ),
                ("FPS", g(names::RUNTIME_FPS), ""),
                ("Entities", g(names::RUNTIME_ENTITY_COUNT), ""),
                (
                    "Mesh handles",
                    g(names::RUNTIME_MESH_HANDLE_COUNT),
                    "runtime.asset_handle_spike",
                ),
                (
                    "Material handles",
                    g(names::RUNTIME_MATERIAL_HANDLE_COUNT),
                    "",
                ),
                (
                    "Colliders",
                    g(names::RUNTIME_COLLIDER_COUNT),
                    "runtime.terrain_collider_missing",
                ),
                (
                    "ShapeMeshCache",
                    g(names::RUNTIME_SHAPE_MESH_CACHE_LEN),
                    "runtime.shape_mesh_cache_growth",
                ),
                (
                    "Respawns",
                    c(names::RUNTIME_RESPAWN_COUNT),
                    "runtime.respawn_thrashing",
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
                        "net.peer_churn_spike",
                    ),
                    ("Disconnected", c(names::NET_PEER_DISCONNECTED_COUNT), ""),
                    (
                        "Transform rejects",
                        c(names::NET_TRANSFORM_REJECTED_COUNT),
                        "",
                    ),
                    (
                        "Spoof rejects",
                        c(names::NET_IDENTITY_SPOOFED_COUNT),
                        "net.identity_spoof_burst",
                    ),
                ],
            );
            health_card(
                ui,
                invariants,
                "Avatar fetch",
                &[
                    ("Latency (ms)", h(names::NET_AVATAR_FETCH_LATENCY_MS), ""),
                    ("Succeeded", c(names::NET_AVATAR_FETCH_SUCCESS_COUNT), ""),
                    ("Failed", c(names::NET_AVATAR_FETCH_FAIL_COUNT), ""),
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
                        "",
                    ),
                    (
                        "Offers accepted",
                        c(names::NET_OFFER_ACCEPTED_COUNT),
                        "net.offer_acceptance_anomaly",
                    ),
                    ("Offers declined", c(names::NET_OFFER_DECLINED_COUNT), ""),
                    (
                        "Auto-declined (busy)",
                        c(names::NET_OFFER_AUTO_DECLINED_BUSY_COUNT),
                        "",
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
                    ("Heightmap (ms)", h(names::OFFLOAD_HEIGHTMAP_LATENCY_MS), ""),
                    (
                        "Ambient bake (ms)",
                        h(names::OFFLOAD_AMBIENT_BAKE_LATENCY_MS),
                        "offload.ambient_bake_stall",
                    ),
                    (
                        "Texture bake (ms)",
                        h(names::OFFLOAD_TEXTURE_BAKE_LATENCY_MS),
                        "",
                    ),
                    (
                        "Job errors",
                        c(names::OFFLOAD_JOB_ERROR_COUNT),
                        "offload.task_never_resolves",
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
                        "loading.record_fetch_exhausted",
                    ),
                    (
                        "Fetch retries",
                        c(names::LOADING_RECORD_FETCH_RETRY_COUNT),
                        "",
                    ),
                    (
                        "Last gate (s)",
                        g(names::LOADING_GATE_TOTAL_SECS),
                        "loading.gate_stall",
                    ),
                ],
            );
        }
        DiagTab::Overview | DiagTab::Identity => {}
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
    session_log: Res<SessionLog>,
    invariants: Res<InvariantRegistry>,
    metrics: Res<MetricsRegistry>,
    mut landmark_status: Local<Option<(String, f64)>>,
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
                    ui.selectable_value(&mut *active_tab, tab, tab.label());
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
                }
            }

            if peer_count == 0 {
                ui.colored_label(egui::Color32::GRAY, "(no peers)");
            }

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
        for (i, v) in [16.0, 20.0, 18.0, 22.0, 17.0].iter().enumerate() {
            m.observe_gauge(names::RUNTIME_FRAME_TIME_MS, *v, i as f64);
        }
        m.observe_gauge(names::RUNTIME_FPS, 58.0, 5.0);
        m.observe_gauge(names::RUNTIME_ENTITY_COUNT, 1234.0, 5.0);
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 3.0, 5.0);
        m.observe_gauge(
            names::RUNTIME_MEMORY_PROCESS_RSS_BYTES,
            512.0 * 1024.0 * 1024.0,
            5.0,
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
        m.observe_gauge(names::RUNTIME_COLLIDER_COUNT, 0.0, 1.0);
        m.incr_by(names::NET_PEER_CONNECTED_COUNT, 4, 1.0);
        m.observe_hist(names::NET_AVATAR_FETCH_LATENCY_MS, 120.0);
        m.observe_hist(names::OFFLOAD_HEIGHTMAP_LATENCY_MS, 800.0);
        let mut reg = default_registry();
        violate(&mut reg, "runtime.terrain_collider_missing", "0 colliders");
        for tab in tabs {
            render_once(tab, &m, &reg);
        }
    }
}
