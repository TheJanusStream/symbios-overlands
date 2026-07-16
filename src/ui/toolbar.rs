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

use crate::diagnostics::anomaly::InvariantRegistry;
use crate::player::{AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset};
use crate::state::{CurrentRoomDid, LocalPlayer};

/// Open/closed state for every toolbar-managed window. Initialised at
/// app startup, overwritten by the persisted prefs ([`crate::prefs`],
/// #820) when the machine has saved a layout, and carried across logout
/// so the next session reopens the same panels. Serde: `#[serde(default)]`
/// fills bools missing from an older prefs file with these defaults, so
/// the struct can grow without breaking saved state.
#[derive(Resource, Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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
    invariants: Res<InvariantRegistry>,
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
                // Worst-active anomaly dot (D-6): a severity-coloured ● appears
                // beside the Diagnostics toggle whenever an invariant is
                // violated, so a broken session is visible even with the panel
                // closed. Nothing renders while healthy.
                if let Some(worst) = invariants.worst_active() {
                    let n = invariants.active_badges().count();
                    ui.colored_label(crate::ui::diagnostics::severity_color(worst), "●")
                        .on_hover_text(format!(
                            "{n} active anomal{} — open Diagnostics",
                            if n == 1 { "y" } else { "ies" }
                        ));
                }
                ui.toggle_value(&mut panels.controls, "Controls");
            });
        });
    });
}

/// The chassis the local player is currently piloting, resolved from the
/// preset marker on the [`LocalPlayer`] entity. Drives which movement key rows
/// the Controls cheat-sheet shows (#803).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PilotedChassis {
    OnFoot,
    Boat,
    Skiff,
    Airship,
    Airplane,
}

/// One key-binding row in the Controls cheat-sheet: the key glyphs and what
/// they do on the current chassis.
struct ControlRow {
    keys: &'static str,
    action: &'static str,
}

// Per-chassis movement rows. These mirror the live key handlers in
// `player/{humanoid,hover_boat,car,helicopter,airplane}.rs`, so the sheet can
// never drift from the actual controls again (#803) — change both together.
const ON_FOOT_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W A S D  or  Arrows",
        action: "walk",
    },
    ControlRow {
        keys: "Space",
        action: "jump · climb · swim up",
    },
    ControlRow {
        keys: "Shift / Ctrl",
        action: "swim down",
    },
];
const BOAT_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ↑ / ↓",
        action: "drive forward / reverse",
    },
    ControlRow {
        keys: "A / D  or  ← / →",
        action: "steer",
    },
    ControlRow {
        keys: "Space",
        action: "hop up",
    },
];
const SKIFF_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ↑ / ↓",
        action: "throttle / reverse",
    },
    ControlRow {
        keys: "A / D  or  ← / →",
        action: "steer (on the ground)",
    },
    ControlRow {
        keys: "Space",
        action: "handbrake",
    },
];
const AIRSHIP_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ↑ / ↓",
        action: "fly forward / back",
    },
    ControlRow {
        keys: "A / D  or  ← / →",
        action: "yaw (turn)",
    },
    ControlRow {
        keys: "Q / E",
        action: "strafe left / right",
    },
    ControlRow {
        keys: "Space / Shift",
        action: "climb / descend",
    },
];
const AIRPLANE_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "W / S  or  ↑ / ↓",
        action: "pitch down / up",
    },
    ControlRow {
        keys: "A / D  or  ← / →",
        action: "roll",
    },
    ControlRow {
        keys: "Q / E",
        action: "yaw (rudder)",
    },
    ControlRow {
        keys: "Space / Shift",
        action: "throttle up / down",
    },
];

/// Movement key rows for the piloted chassis — the pure preset→rows mapping
/// (#803, unit-tested below). The camera rows and portal hint are shared and
/// rendered separately by [`controls_hint_ui`].
fn movement_rows(chassis: PilotedChassis) -> &'static [ControlRow] {
    match chassis {
        PilotedChassis::OnFoot => ON_FOOT_ROWS,
        PilotedChassis::Boat => BOAT_ROWS,
        PilotedChassis::Skiff => SKIFF_ROWS,
        PilotedChassis::Airship => AIRSHIP_ROWS,
        PilotedChassis::Airplane => AIRPLANE_ROWS,
    }
}

/// Resolve the piloted chassis from the `LocalPlayer`'s preset markers (only
/// one is ever present — the hot-swap strips the old before inserting the new).
/// Falls back to [`PilotedChassis::OnFoot`] when no vehicle marker is present:
/// the humanoid preset, or the local player not yet spawned.
fn piloted_chassis(boat: bool, skiff: bool, airship: bool, airplane: bool) -> PilotedChassis {
    if boat {
        PilotedChassis::Boat
    } else if skiff {
        PilotedChassis::Skiff
    } else if airship {
        PilotedChassis::Airship
    } else if airplane {
        PilotedChassis::Airplane
    } else {
        PilotedChassis::OnFoot
    }
}

/// Movement / camera cheat-sheet. Open on first `InGame` entry (the
/// [`UiPanels`] default) and from the toolbar afterwards. The movement rows are
/// context-sensitive to the chassis the player is currently piloting (#803);
/// the camera rows and portal hint are shared.
#[allow(clippy::type_complexity)]
pub fn controls_hint_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    local: Query<
        (
            Has<HoverBoatPreset>,
            Has<CarPreset>,
            Has<HelicopterPreset>,
            Has<AirplanePreset>,
        ),
        With<LocalPlayer>,
    >,
) {
    if !panels.controls {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let chassis = local.iter().next().map_or(
        PilotedChassis::OnFoot,
        |(boat, skiff, airship, airplane)| piloted_chassis(boat, skiff, airship, airplane),
    );

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
                    for row in movement_rows(chassis) {
                        ui.monospace(row.keys);
                        ui.label(row.action);
                        ui.end_row();
                    }
                    // Camera controls are the same on every chassis.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_resolve_to_the_matching_chassis() {
        assert_eq!(
            piloted_chassis(true, false, false, false),
            PilotedChassis::Boat
        );
        assert_eq!(
            piloted_chassis(false, true, false, false),
            PilotedChassis::Skiff
        );
        assert_eq!(
            piloted_chassis(false, false, true, false),
            PilotedChassis::Airship
        );
        assert_eq!(
            piloted_chassis(false, false, false, true),
            PilotedChassis::Airplane
        );
    }

    #[test]
    fn no_vehicle_marker_falls_back_to_on_foot() {
        // Humanoid preset (no vehicle marker) or the local player not yet spawned.
        assert_eq!(
            piloted_chassis(false, false, false, false),
            PilotedChassis::OnFoot
        );
    }

    #[test]
    fn every_chassis_has_a_non_empty_movement_sheet() {
        for chassis in [
            PilotedChassis::OnFoot,
            PilotedChassis::Boat,
            PilotedChassis::Skiff,
            PilotedChassis::Airship,
            PilotedChassis::Airplane,
        ] {
            let rows = movement_rows(chassis);
            assert!(!rows.is_empty(), "{chassis:?} has no movement rows");
            for row in rows {
                assert!(!row.keys.is_empty(), "{chassis:?} row has empty keys");
                assert!(!row.action.is_empty(), "{chassis:?} row has empty action");
            }
        }
    }

    #[test]
    fn ground_and_air_chassis_read_distinctly() {
        // The whole point of #803: the sheet is no longer a stale union. A
        // skiff shows a handbrake; an airship shows climb/descend + strafe;
        // they must not share a row set.
        assert_ne!(
            movement_rows(PilotedChassis::Skiff).len(),
            0,
            "skiff sheet is empty"
        );
        let skiff_actions: Vec<&str> = movement_rows(PilotedChassis::Skiff)
            .iter()
            .map(|r| r.action)
            .collect();
        assert!(
            skiff_actions.iter().any(|a| a.contains("handbrake")),
            "skiff sheet lost its handbrake row"
        );
        let airship_actions: Vec<&str> = movement_rows(PilotedChassis::Airship)
            .iter()
            .map(|r| r.action)
            .collect();
        assert!(
            airship_actions.iter().any(|a| a.contains("climb")),
            "airship sheet lost its climb/descend row"
        );
    }
}
