use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::state::{DiagnosticsLog, RemotePeer};

pub fn diagnostics_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    mut peers: Query<&mut RemotePeer>,
    diagnostics: Res<DiagnosticsLog>,
) {
    use crate::config::ui::diagnostics as cfg;

    egui::Window::new("Diagnostics")
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
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

            ui.separator();

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
                        ui.monospace(
                            egui::RichText::new(&did).small().color(egui::Color32::GRAY),
                        );
                    });
                    ui.checkbox(&mut muted, "Mute");
                });
                peer.muted = muted;
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
                    for entry in diagnostics.iter() {
                        ui.monospace(
                            egui::RichText::new(entry)
                                .small()
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                    }
                });
        });
}
