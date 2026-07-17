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

use crate::avatar::{BskyProfileCache, draw_avatar_icon};
use crate::diagnostics::anomaly::InvariantRegistry;
use crate::player::{AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset};
use crate::state::{ChatHistory, CurrentRoomDid, LocalPlayer, RemotePeer};
use crate::ui::unsaved_guard::{GuardedAction, UnsavedGuard};

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
    /// The Settings window (#857): theme picker + client toggles.
    pub settings: bool,
    /// The controls overlay. Defaults to open — this is the first-run
    /// hint — and is re-openable from the toolbar.
    pub controls: bool,
    /// True once the Controls sheet has been dismissed at least once on
    /// this machine (#834). While false — a true first run — the sheet
    /// is center-anchored so a brand-new visitor cannot miss it; ever
    /// after it is a normal draggable window near the right edge.
    pub controls_seen: bool,
    /// True once the owner-gestures callout has fired (#851): the first
    /// `InGame` arrival in a world the player OWNS re-opens the Controls
    /// sheet so its "You own this world" section (right-click menu,
    /// Shift-copy, Esc) is actually seen. Persisted like the rest of the
    /// struct so it happens once per machine, not once per session.
    pub owner_hint_seen: bool,
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
            settings: false,
            controls: true,
            controls_seen: false,
            owner_hint_seen: false,
        }
    }
}

/// Does the signed-in player own the overland they're standing in?
/// Ownership is DID equality — the room record lives in the owner's PDS.
fn owns_current_room(
    session: Option<&AtprotoSession>,
    current_room: Option<&CurrentRoomDid>,
) -> bool {
    match (session, current_room) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    }
}

/// Reserved width of the toolbar wordmark (#860) — fixed so the brand
/// text can never shift the toggles after it (same contract as the
/// badge widths below).
const WORDMARK_WIDTH: f32 = 148.0;

/// Reserved width of the Chat toggle — wide enough for "Chat (99+)" so
/// the unread badge appearing/growing never shifts the buttons after it.
const CHAT_TOGGLE_WIDTH: f32 = 84.0;
/// Reserved width of the People toggle, sized for "People (99+)".
const PEOPLE_TOGGLE_WIDTH: f32 = 100.0;
/// Reserved slot width of the anomaly dot, occupied even while healthy
/// so the dot appearing/vanishing stops shifting the Controls button.
const ANOMALY_DOT_WIDTH: f32 = 14.0;

/// Counts above this render as "99+" — the badge is a "look here"
/// signal, not a metric, and capping it keeps the reserved width honest.
const BADGE_COUNT_CAP: usize = 99;

/// A panel toggle with a fixed minimum width and a one-line tooltip:
/// the width reservation is what keeps count-badged labels ("Chat (3)")
/// from shifting the rest of the row as the count changes.
fn toggle_with_badge(ui: &mut egui::Ui, flag: &mut bool, label: String, min_width: f32, tip: &str) {
    let size = egui::vec2(min_width, ui.spacing().interact_size.y);
    if ui
        .add_sized(size, egui::Button::selectable(*flag, label))
        .on_hover_text(tip)
        .clicked()
    {
        *flag = !*flag;
    }
}

/// Format a badge count, capped so the label can't outgrow its
/// reserved width.
fn badge_count(n: usize) -> String {
    if n > BADGE_COUNT_CAP {
        format!("{BADGE_COUNT_CAP}+")
    } else {
        n.to_string()
    }
}

/// Slim top bar enumerating every panel as a toggle button. The World
/// Editor button is enabled only for the room's owner — the panel
/// itself is owner-gated too. Visitors see it disabled with an
/// ownership explanation instead of not at all (#851).
///
/// #835 additions: one-line tooltips on every toggle, an unread-count
/// badge on Chat, a live headcount on People, a clickable anomaly dot
/// that opens Diagnostics on the worst tab, and an account chip at the
/// far right (identity, current room, Copy Landmark Link, Log out — the
/// two-click home for actions that used to hide in Diagnostics→Identity).
#[allow(clippy::too_many_arguments)]
pub fn toolbar_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut audio_muted: ResMut<crate::audio_mute::AudioMuted>,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
    invariants: Res<InvariantRegistry>,
    mut chat: ResMut<ChatHistory>,
    peers: Query<&RemotePeer>,
    mut diag_tab: ResMut<crate::ui::diagnostics::DiagTab>,
    mut commands: Commands,
    profile_cache: Res<BskyProfileCache>,
    local_player_q: Query<&Transform, With<LocalPlayer>>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let owns_room = owns_current_room(session.as_deref(), current_room.as_deref());

    // An open Chat window means every message is on screen — the badge
    // only counts what arrives while it's closed. Guarded write so the
    // resource isn't marked changed every frame the window sits open.
    if panels.chat && chat.unread != 0 {
        chat.unread = 0;
    }
    let chat_label = if chat.unread > 0 {
        format!("Chat ({})", badge_count(chat.unread))
    } else {
        "Chat".to_owned()
    };
    // Everyone in the room, self included — matching the People window's
    // own "In room (N)" header.
    let people_total = peers.iter().count() + session.is_some() as usize;

    egui::TopBottomPanel::top("overlands-toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Wordmark (#860): the product's name at the bar's left edge.
            // Non-interactive, accent-coloured, fixed width — the brand
            // anchor for every session, and the same product identity the
            // wasm splash and OAuth return pages carry.
            let accent = crate::ui::theme::current(ui.ctx()).accent;
            ui.add_sized(
                egui::vec2(WORDMARK_WIDTH, ui.spacing().interact_size.y),
                egui::Label::new(
                    egui::RichText::new("SYMBIOS OVERLANDS")
                        .strong()
                        .color(accent),
                )
                .selectable(false),
            );
            ui.separator();
            toggle_with_badge(
                ui,
                &mut panels.chat,
                chat_label,
                CHAT_TOGGLE_WIDTH,
                "Chat — talk with everyone in this overland (Enter)",
            );
            toggle_with_badge(
                ui,
                &mut panels.people,
                format!("People ({})", badge_count(people_total)),
                PEOPLE_TOGGLE_WIDTH,
                "People — who's here; drag an item onto a row to gift it",
            );
            ui.toggle_value(&mut panels.avatar, "Avatar")
                .on_hover_text("Avatar — edit your look and vehicle");
            ui.toggle_value(&mut panels.inventory, "Inventory")
                .on_hover_text("Inventory — your saved item blueprints");
            ui.toggle_value(&mut panels.catalogue, "Catalogue")
                .on_hover_text("Catalogue — browse placeable items");
            if owns_room {
                ui.toggle_value(&mut panels.world_editor, "World Editor")
                    .on_hover_text("World Editor — reshape this overland (you own it)");
            } else {
                // Rendered disabled instead of hidden (#851): the silent
                // pop-in taught nobody why the button exists — now a
                // visitor hovering it learns the ownership rule.
                ui.add_enabled(false, egui::Button::selectable(false, "World Editor"))
                    .on_disabled_hover_text(
                        "Only this overland's owner can edit it. Your own overland \
                         is editable when you're home.",
                    );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Account chip — first in the right-to-left layout, so it
                // owns the far-right corner. The only 2-click route to
                // logout and location sharing (#835); Diagnostics keeps
                // its duplicates.
                if let Some(sess) = session.as_deref() {
                    ui.menu_button(format!("@{}", sess.handle), |ui| {
                        ui.horizontal(|ui| {
                            draw_avatar_icon(
                                ui,
                                Some(sess.did.as_str()),
                                &profile_cache,
                                crate::ui::chat::AVATAR_ICON_PX,
                            );
                            ui.monospace(format!("@{}", sess.handle));
                        });
                        ui.monospace(
                            egui::RichText::new(&sess.did)
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                        if let Some(room) = current_room.as_deref() {
                            ui.separator();
                            ui.label(if owns_room {
                                "Current overland: yours"
                            } else {
                                "Current overland:"
                            });
                            if !owns_room {
                                ui.monospace(
                                    egui::RichText::new(&room.0)
                                        .small()
                                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                                );
                            }
                            let player_tf = local_player_q.single().ok().copied();
                            if crate::ui::diagnostics::landmark_link_button(
                                ui,
                                &room.0,
                                player_tf,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                            ) {
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button("Log out").clicked() {
                            // Route through the unsaved-edits guard instead
                            // of flipping the state directly: it transitions
                            // immediately when nothing is dirty, and offers
                            // Publish / Discard / Cancel otherwise.
                            commands.insert_resource(UnsavedGuard::new(GuardedAction::Logout));
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Account — identity, share your spot, log out");
                }
                // Master mute. The icon shows the current state; the hover
                // text names the action a click performs.
                let (icon, action) = if audio_muted.0 {
                    ("🔇", "Unmute all audio")
                } else {
                    ("🔊", "Mute all audio")
                };
                if ui.button(icon).on_hover_text(action).clicked() {
                    audio_muted.0 = !audio_muted.0;
                }
                ui.toggle_value(&mut panels.diagnostics, "Diagnostics")
                    .on_hover_text("Diagnostics — session health, metrics, and logs");
                ui.toggle_value(&mut panels.settings, "Settings")
                    .on_hover_text("Settings — theme & client preferences");
                // Worst-active anomaly dot (D-6): a severity-coloured ●
                // beside the Diagnostics toggle whenever an invariant is
                // violated, so a broken session is visible even with the
                // panel closed. The slot is reserved even while healthy so
                // the dot's appearance doesn't shift the Controls button;
                // clicking it opens Diagnostics on the worst tab (#835).
                let slot = egui::vec2(ANOMALY_DOT_WIDTH, ui.spacing().interact_size.y);
                let (dot_rect, dot_resp) = ui.allocate_exact_size(slot, egui::Sense::click());
                if let Some(worst) = invariants.worst_active() {
                    // Painted circle, not a "●" glyph — U+25CF is
                    // tofu in the proportional family (#861).
                    ui.painter().circle_filled(
                        dot_rect.center(),
                        4.5,
                        crate::ui::diagnostics::severity_color(ui, worst),
                    );
                    let n = invariants.active_badges().count();
                    let dot_resp = dot_resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!(
                            "{n} active anomal{} — click to open Diagnostics",
                            if n == 1 { "y" } else { "ies" }
                        ));
                    if dot_resp.clicked() {
                        panels.diagnostics = true;
                        *diag_tab = crate::ui::diagnostics::tab_for_subsystem(
                            invariants.worst_active_subsystem(),
                        );
                    }
                }
                ui.toggle_value(&mut panels.controls, "Controls")
                    .on_hover_text("Controls — movement & camera cheat-sheet");
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

impl PilotedChassis {
    /// Player-facing name for the sheet's "Piloting:" heading (#834) —
    /// the rows already swap live with the chassis (#803), but without
    /// this the window never said WHICH chassis they describe.
    fn label(self) -> &'static str {
        match self {
            Self::OnFoot => "On foot",
            Self::Boat => "Boat",
            Self::Skiff => "Skiff",
            Self::Airship => "Airship",
            Self::Airplane => "Airplane",
        }
    }
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
        // Ctrl is not bound on wasm (#839): W+Ctrl is the browser's
        // close-tab chord. Mirrors `player::humanoid`'s swim keys.
        keys: if cfg!(target_arch = "wasm32") {
            "Shift / C"
        } else {
            "Shift / Ctrl / C"
        },
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

// World-editing gesture rows (#851), shown to the room's owner. These
// mirror the live handlers — same #803 contract as the movement rows,
// change both together:
// * right-click menu → `editor_gizmo::context_menu::detect_scene_right_click`
//   (click-vs-drag discrimination: a right-DRAG still orbits the camera)
// * left-click pick  → the editor's scene picker (active while an editor
//   window is open)
// * Shift-copy-drag  → `editor_gizmo::drag` (Shift at drag-start clones)
// * Esc              → drag abort + selection clear (`ui::shortcuts`)
const EDITOR_ROWS: &[ControlRow] = &[
    ControlRow {
        keys: "Right-click",
        action: "create / select menu (a right-DRAG still orbits)",
    },
    ControlRow {
        keys: "Left-click",
        action: "pick the object under the cursor (editor open)",
    },
    ControlRow {
        keys: "Shift-drag",
        action: "drag a copy instead of moving",
    },
    ControlRow {
        keys: "Esc",
        action: "abort a drag · clear the selection",
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

/// First `InGame` arrival in a world the player OWNS re-opens the
/// Controls sheet so the owner-gestures section is actually seen once
/// (#851). Latched via the persisted [`UiPanels::owner_hint_seen`], so
/// it fires once per machine — visiting other worlds doesn't count, and
/// re-logins don't re-flash it. Registered on `OnEnter(InGame)`.
pub fn flash_owner_controls_once(
    mut panels: ResMut<UiPanels>,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
) {
    if owns_current_room(session.as_deref(), current_room.as_deref()) && !panels.owner_hint_seen {
        panels.owner_hint_seen = true;
        panels.controls = true;
    }
}

/// Movement / camera cheat-sheet. Open on first `InGame` entry (the
/// [`UiPanels`] default) and from the toolbar afterwards. The movement rows are
/// context-sensitive to the chassis the player is currently piloting (#803);
/// the camera rows and portal hint are shared, and the room's owner
/// additionally gets the world-editing gesture rows (#851).
#[allow(clippy::type_complexity)]
pub fn controls_hint_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    local: Query<
        (
            Has<HoverBoatPreset>,
            Has<CarPreset>,
            Has<HelicopterPreset>,
            Has<AirplanePreset>,
        ),
        With<LocalPlayer>,
    >,
    session: Option<Res<AtprotoSession>>,
    current_room: Option<Res<CurrentRoomDid>>,
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
    let mut window = egui::Window::new("Controls")
        .open(&mut open)
        .collapsible(false)
        .resizable(false);
    // Center-anchored ONLY on a true first run, where missing it would
    // strand a brand-new visitor (#834). `.anchor()` re-pins every
    // frame — permanently immovable — so once the sheet has been seen
    // it becomes a normal draggable window near the right edge, and can
    // no longer superimpose with the (also centered) offer modal.
    if panels.controls_seen {
        let (pos, _size) = chrome.place(crate::ui::layout::UiWindow::Controls, ctx);
        window = window.default_pos(pos).constrain_to(ctx.available_rect());
    } else {
        window = window.anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    }
    let response = window.show(ctx, |ui| {
        ui.strong(format!("Piloting: {}", chassis.label()));
        ui.add_space(4.0);
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
                // Global shortcuts (#836) — same on every chassis.
                ui.monospace("Enter");
                ui.label("open chat");
                ui.end_row();
                ui.monospace("Esc");
                ui.label("back out: drag · selection · windows");
                ui.end_row();
                ui.monospace("Ctrl+S");
                ui.label("save the editor you're in");
                ui.end_row();
            });
        ui.add_space(6.0);
        ui.small("Change your vehicle in Avatar › Locomotion.");
        ui.add_space(6.0);
        ui.label("Walk through a portal doorway to travel into another overland.");
        // Owner-only: the world-editing gestures (#851). Every one of
        // these was previously undiscoverable — and right-click doubling
        // as camera orbit actively taught people to avoid the menu.
        if owns_current_room(session.as_deref(), current_room.as_deref()) {
            ui.add_space(8.0);
            ui.separator();
            ui.strong("You own this world — World Editor");
            ui.add_space(4.0);
            egui::Grid::new("controls-editor-grid")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    for row in EDITOR_ROWS {
                        ui.monospace(row.keys);
                        ui.label(row.action);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.small(
                "The World/Local toggle beside the editor's transform fields \
                 switches the drag gizmo's orientation.",
            );
        }
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            if ui.button("Got it").clicked() {
                panels.controls = false;
            }
        });
    });
    // Only track geometry once de-anchored — remembering the anchored
    // rect would persist "screen center" as the window's home.
    if panels.controls_seen
        && let Some(response) = response.as_ref()
    {
        chrome.remember(
            crate::ui::layout::UiWindow::Controls,
            response.response.rect,
        );
    }
    if !open {
        panels.controls = false;
    }
    // Any dismissal — [x] or "Got it" — ends the first-run treatment on
    // this machine (persisted via #820, like the rest of UiPanels).
    if !panels.controls && !panels.controls_seen {
        panels.controls_seen = true;
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
    fn editor_rows_cover_the_core_gestures() {
        // #851's acceptance: a new owner learns the three core editing
        // gestures (plus the escape) from the sheet alone. Guard the rows
        // so a future trim can't silently drop one.
        let keys: Vec<&str> = EDITOR_ROWS.iter().map(|r| r.keys).collect();
        for expected in ["Right-click", "Left-click", "Shift-drag", "Esc"] {
            assert!(keys.contains(&expected), "editor rows lost {expected}");
        }
        for row in EDITOR_ROWS {
            assert!(!row.action.is_empty(), "{} row has empty action", row.keys);
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
