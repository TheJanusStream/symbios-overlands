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
use crate::diagnostics::SessionLog;
use crate::diagnostics::anomaly::InvariantRegistry;
use crate::diagnostics::event::Severity;
use crate::state::{CurrentRoomDid, LocalPlayer, RemotePeer};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};

/// Which tab of the Diagnostics panel is showing. Only the Identity tab carries
/// content today (the historic panel, preserved verbatim); the health tabs are
/// filled in by later pillar-C steps (C-3/C-4) and read the shared metrics
/// registry + anomaly badges then. Default is Identity so the panel opens
/// exactly as it did before tabs were added.
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
            // the health tabs are placeholders until C-3/C-4 populate them.
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

            // Non-Identity tabs are not built yet — show a placeholder and skip
            // the historic body (early-return from the window closure).
            if *active_tab != DiagTab::Identity {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        format!("{} health — populated in a later step.", active_tab.label()),
                    );
                });
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
}
