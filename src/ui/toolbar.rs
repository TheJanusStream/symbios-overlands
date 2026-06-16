//! In-game UI shell: the top toolbar and the first-run controls hint.
//!
//! Before this existed every panel was a floating egui window that
//! spawned collapsed somewhere over the viewport — discoverable only by
//! noticing its title bar. The toolbar enumerates every panel as a
//! toggle button (so features like the Catalogue or drag-to-gift in
//! People are visible at a glance), and [`UiPanels`] is the single
//! source of truth for which windows are open: each window system reads
//! its flag via `egui::Window::open`, which also gives every window a
//! native close button that writes the flag back.
//!
//! The controls hint covers the other half of the discoverability gap:
//! a first-time visitor landing from a shared link is never told the
//! movement keys. It pops once per session on `InGame` entry and can be
//! re-opened any time from the toolbar's "Controls" button.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::state::CurrentRoomDid;

/// Open/closed state for every toolbar-managed window. Initialised at
/// app startup and reset by `logout::cleanup_on_logout` so the next
/// session starts from the defaults (including a fresh controls hint).
#[derive(Resource)]
pub struct UiPanels {
    pub chat: bool,
    pub people: bool,
    pub avatar: bool,
    pub world_editor: bool,
    pub inventory: bool,
    pub catalogue: bool,
    pub diagnostics: bool,
    /// The controls overlay. Defaults to open — this is the first-run
    /// hint — and is re-openable from the toolbar.
    pub controls: bool,
}

impl Default for UiPanels {
    fn default() -> Self {
        Self {
            chat: false,
            people: false,
            avatar: false,
            world_editor: false,
            inventory: false,
            catalogue: false,
            diagnostics: false,
            controls: true,
        }
    }
}

/// Slim top bar enumerating every panel as a toggle button. The World
/// Editor button only renders for the room's owner — the panel itself
/// is owner-gated too, so showing the button to a visitor would be a
/// dead control.
pub fn toolbar_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut audio_muted: ResMut<crate::audio_mute::AudioMuted>,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let owns_room = match (session.as_deref(), current_room.as_deref()) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    };

    egui::TopBottomPanel::top("overlands-toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.toggle_value(&mut panels.chat, "Chat");
            ui.toggle_value(&mut panels.people, "People");
            ui.toggle_value(&mut panels.avatar, "Avatar");
            ui.toggle_value(&mut panels.inventory, "Inventory");
            ui.toggle_value(&mut panels.catalogue, "Catalogue");
            if owns_room {
                ui.toggle_value(&mut panels.world_editor, "World Editor");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Master mute. First in the right-to-left layout, so it
                // sits in the far-right corner. The icon shows the current
                // state; the hover text names the action a click performs.
                let (icon, action) = if audio_muted.0 {
                    ("🔇", "Unmute all audio")
                } else {
                    ("🔊", "Mute all audio")
                };
                if ui.button(icon).on_hover_text(action).clicked() {
                    audio_muted.0 = !audio_muted.0;
                }
                ui.toggle_value(&mut panels.diagnostics, "Diagnostics");
                ui.toggle_value(&mut panels.controls, "Controls");
            });
        });
    });
}

/// Movement / camera cheat-sheet. Open on first `InGame` entry (the
/// [`UiPanels`] default) and from the toolbar afterwards. The key set
/// is the union across the five locomotion presets, annotated where a
/// key only means something for some chassis.
pub fn controls_hint_ui(mut contexts: EguiContexts, mut panels: ResMut<UiPanels>) {
    if !panels.controls {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut open = true;
    egui::Window::new("Controls")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("controls-grid")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    ui.monospace("W / S  or  ↑ / ↓");
                    ui.label("move forward / back (pitch for aircraft)");
                    ui.end_row();
                    ui.monospace("A / D  or  ← / →");
                    ui.label("turn / strafe (roll for aircraft)");
                    ui.end_row();
                    ui.monospace("Q / E");
                    ui.label("yaw (airplane & helicopter)");
                    ui.end_row();
                    ui.monospace("Space");
                    ui.label("jump · climb · swim up");
                    ui.end_row();
                    ui.monospace("Shift / Ctrl");
                    ui.label("descend · swim down");
                    ui.end_row();
                    ui.monospace("Right-drag");
                    ui.label("orbit camera");
                    ui.end_row();
                    ui.monospace("Middle-drag");
                    ui.label("pan camera");
                    ui.end_row();
                    ui.monospace("Scroll");
                    ui.label("zoom");
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label("Walk through a portal doorway to travel into another overland.");
            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                if ui.button("Got it").clicked() {
                    panels.controls = false;
                }
            });
        });
    if !open {
        panels.controls = false;
    }
}
