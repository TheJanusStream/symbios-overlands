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

#[allow(clippy::too_many_arguments)]
pub fn diagnostics_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut peers: Query<&mut RemotePeer>,
    session_log: Res<SessionLog>,
    mut landmark_status: Local<Option<(String, f64)>>,
    mut active_tab: Local<DiagTab>,
    time: Res<Time>,
    local_player_q: Query<&Transform, With<LocalPlayer>>,
    // Native-only: the wireframe plugin (and the resource it inserts) is
    // skipped on WASM so this parameter only exists off-web.
    #[cfg(not(target_arch = "wasm32"))] mut wireframe: ResMut<WireframeConfig>,
) {
    use crate::config::ui::diagnostics as cfg;

    // Tint the event-log line by severity so warnings/errors stand out in the
    // scrolling HUD (the same severity that drives the analyzer's verdict).
    fn severity_color(sev: Severity) -> egui::Color32 {
        match sev {
            Severity::Trace => egui::Color32::DARK_GRAY,
            Severity::Info => egui::Color32::LIGHT_GRAY,
            Severity::Warn => egui::Color32::from_rgb(210, 170, 90),
            Severity::Error => egui::Color32::from_rgb(210, 120, 90),
            Severity::Critical => egui::Color32::from_rgb(220, 90, 90),
        }
    }

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
