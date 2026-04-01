use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::state::{DiagnosticsLog, RemotePeer};

pub fn diagnostics_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    peers: Query<&RemotePeer>,
    diagnostics: Res<DiagnosticsLog>,
) {
    egui::SidePanel::left("diagnostics")
        .resizable(true)
        .default_width(crate::config::ui::diagnostics::PANEL_DEFAULT_WIDTH)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            ui.heading("Diagnostics");
            ui.separator();

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
            for peer in peers.iter() {
                let handle = peer.handle.as_deref().unwrap_or("identifying…");
                let did = peer.did.as_deref().unwrap_or("unknown");
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::GREEN, "●");
                    ui.vertical(|ui| {
                        ui.monospace(format!("@{}", handle));
                        ui.monospace(egui::RichText::new(did).small().color(egui::Color32::GRAY));
                    });
                });
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
