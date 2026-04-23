//! Room roster window. Lists the signed-in user plus every remote peer
//! currently in the room, with a per-peer Mute toggle. The mute flag writes
//! straight into `RemotePeer.muted`; audio-mix / visibility code keys off the
//! same component. Diagnostics still renders its own copy of the roster
//! (with DIDs) — this window is the user-facing social view, Diagnostics is
//! the debug view.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::state::RemotePeer;

pub fn people_ui(
    mut contexts: EguiContexts,
    session: Option<Res<AtprotoSession>>,
    mut peers: Query<&mut RemotePeer>,
) {
    use crate::config::ui::people as cfg;

    let ctx = contexts.ctx_mut().unwrap();
    egui::Window::new("People")
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_size([cfg::WINDOW_DEFAULT_WIDTH, cfg::WINDOW_DEFAULT_HEIGHT])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            let peer_count = peers.iter().count();
            let total = peer_count + session.is_some() as usize;
            ui.label(format!("In room ({})", total));
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([true, false])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    // Self entry at the top. Same blue dot the chat uses
                    // for the local author tag, so the visual "you" cue
                    // carries across both windows. No Mute button on self.
                    if let Some(s) = session.as_deref() {
                        let [r, g, b] = crate::config::ui::chat::AUTHOR_COLOR;
                        let self_color = egui::Color32::from_rgb(r, g, b);
                        ui.horizontal(|ui| {
                            ui.colored_label(self_color, "●");
                            ui.monospace(format!("@{} (you)", s.handle));
                        });
                    }

                    // Remote peers. Handshake-in-progress peers show as
                    // "identifying…" so their presence is visible before
                    // the handle resolves.
                    for mut peer in peers.iter_mut() {
                        let handle = peer.handle.as_deref().unwrap_or("identifying…").to_owned();
                        let dot_color = if peer.muted {
                            egui::Color32::GRAY
                        } else {
                            egui::Color32::GREEN
                        };
                        let mut muted = peer.muted;
                        ui.horizontal(|ui| {
                            ui.colored_label(dot_color, "●");
                            ui.monospace(format!("@{}", handle));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut muted, "Mute");
                                },
                            );
                        });
                        // Guard the write so Bevy's change-detection flag is
                        // only raised when the mute state actually flips —
                        // an unconditional assignment would mark
                        // `RemotePeer` as `Changed` every frame and
                        // invalidate any `Changed<RemotePeer>` filter
                        // downstream.
                        if peer.muted != muted {
                            peer.muted = muted;
                        }
                    }

                    if peer_count == 0 && session.is_none() {
                        ui.colored_label(egui::Color32::GRAY, "(empty)");
                    } else if peer_count == 0 {
                        ui.colored_label(egui::Color32::GRAY, "(no other peers)");
                    }
                });
        });
}
