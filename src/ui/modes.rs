//! The invisible-mode banner (#1240 f170, #1241 f160 / f161).
//!
//! Four of this tranche's findings are one defect: the app puts the player
//! into a mode that changes what the movement keys do — or stops them
//! doing anything at all — and says nothing anywhere. The symptom is
//! identical in every case ("the keys don't work, the app has hung"), and
//! so is the fix: name the mode, where the player is looking, in the
//! sentence that also says how to leave it.
//!
//! * **Held still** (#1240 f170) — clicking a row in Avatar › Visuals, or
//!   a worn prop, locks every axis and zeroes gravity
//!   (`player::freeze_local_avatar_while_editing`). A grep of `src/ui` for
//!   "frozen", "held still" and "movement paused" returned nothing.
//! * **Unrecognised preset** (#1241 f161) — a record written by a newer
//!   build inserts NO preset marker (`player::preset::build_preset_components`),
//!   so not one drive system runs; the Controls sheet meanwhile reported
//!   "Piloting: On foot" and listed the walk keys.
//! * **Swimming / Wading** (#1241 f160) — entering water silently remaps
//!   Space and Shift to opposite meanings, and from below the surface the
//!   water plane is back-face culled, so there is not even a waterline to
//!   see.
//!
//! The decision is [`movement_mode`], pure and ordered, so the priority
//! between two simultaneous modes is stated once rather than falling out
//! of render order. The banner itself is a bare `egui::Area` painted at
//! the top of the panel-free rect: it takes no pointer input, because an
//! interactive area over the 3D scene sets `wants_pointer_input` and
//! would silently break orbiting the camera through it (the lesson
//! `ui::nametag` records).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::player::humanoid::WaterState;

/// A mode the player is in that the movement keys do not advertise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MovementMode {
    /// This build does not model the record's locomotion preset, so no
    /// drive system runs at all (#1241 f161).
    UnrecognisedPreset,
    /// A gizmo is aimed at the avatar or something it wears, so the
    /// chassis is frozen (#1240 f170).
    HeldStill,
    /// Head below the surface: Space rises, Shift/C descends, and the
    /// jump ray never runs (#1241 f160).
    Swimming,
    /// Feet below the surface, head above: same keys, slower (#1241 f160).
    Wading,
}

impl MovementMode {
    /// The banner's sentence. Each names the mode AND the way out of it,
    /// because the whole failure is a user who cannot tell a mode from a
    /// hang — and a mode with no stated exit is barely better.
    pub fn line(self) -> &'static str {
        match self {
            Self::UnrecognisedPreset => {
                "This build can't drive your locomotion preset — nothing will move. \
                 Pick one in Avatar › Locomotion."
            }
            Self::HeldStill => "Held still while a gizmo is aimed · Esc releases",
            Self::Swimming => "Swimming · Space rises · Shift descends",
            Self::Wading => "Wading · slower going",
        }
    }

    /// Whether the banner reads as a problem (warn amber) or as a fact
    /// about the world (the ordinary weak text). Only the unrecognised
    /// preset is a dead end; being frozen or wet is a state the player
    /// chose and can leave.
    pub fn is_a_problem(self) -> bool {
        matches!(self, Self::UnrecognisedPreset)
    }
}

/// Which mode the banner states, ordered most-total-first.
///
/// An unrecognised preset outranks everything: nothing else the banner
/// could say is true while no drive system exists. The freeze outranks the
/// water modes for the same reason — a frozen body is not swimming, it is
/// parked in water.
pub fn movement_mode(
    unrecognised_preset: bool,
    held_still: bool,
    water: WaterState,
) -> Option<MovementMode> {
    if unrecognised_preset {
        return Some(MovementMode::UnrecognisedPreset);
    }
    if held_still {
        return Some(MovementMode::HeldStill);
    }
    match water {
        WaterState::Swimming { .. } => Some(MovementMode::Swimming),
        WaterState::Wading { .. } => Some(MovementMode::Wading),
        WaterState::Dry => None,
    }
}

/// Facts about how the local avatar is currently moving that the movement
/// code knows and no UI could see (#1241 f160, f168).
///
/// `WaterState` was referenced outside `player::humanoid` only by
/// `player::rigged::motion` and never by `src/ui` at all, so the key remap
/// it drives had no surface anywhere; the derived walk speed existed only
/// as a local inside the drive system, so the editor could not tell the
/// owner that their Run slider had gone below it.
///
/// Written by `player::humanoid::publish_movement_facts` (private to the
/// player module), which
/// is deliberately NOT the drive system: the drive systems stand down
/// while an egui text field has focus, and a banner that vanished whenever
/// the player clicked into chat would be worse than none.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct LocalMovement {
    /// Dry / wading / swimming, from `humanoid_water_state`.
    pub water: WaterState,
    /// The unshifted walk this body actually walks at (m/s), derived from
    /// the built rig — `None` until the rigged body lands. Read by the
    /// locomotion editor so the Run slider can say when it has been
    /// dragged below it (#1241 f168).
    pub derived_walk: Option<f32>,
    /// The CAMERA is below a water surface (#1241 f160). Separate from
    /// [`Self::water`], which classifies the avatar: a third-person orbit
    /// camera dips under the surface on its own and, because the water
    /// plane is back-face culled (`world_builder::material`), there is
    /// nothing to see from below — no tint, no fog swap, no surface at
    /// all. The player cannot tell swimming from falling through empty
    /// space, and the flow current then moves them for no visible reason.
    pub camera_submerged: bool,
}

/// The full-viewport tint painted while the camera is under water
/// (#1241 f160). Deep enough to read as "you are submerged" at a glance,
/// light enough to leave the scene legible — the alternative on offer was
/// a `DistanceFog` colour swap, which is a far bigger change to a system
/// the whole world's look depends on.
const UNDERWATER_TINT: egui::Color32 = egui::Color32::from_rgba_premultiplied(6, 26, 48, 70);

/// Paint the banner. Non-interactive, top-centre of the panel-free rect —
/// under the toolbar rather than over it, and clear of the gateway
/// re-open chip and the travel overlay at the bottom.
pub fn movement_mode_ui(
    mut contexts: EguiContexts,
    avatar_editor: Res<crate::ui::avatar::AvatarEditorState>,
    movement: Res<LocalMovement>,
    chassis: Query<(), (With<crate::state::LocalPlayer>, AnyPresetMarker)>,
    players: Query<(), With<crate::state::LocalPlayer>>,
    free: Res<crate::ui::layout::PanelFreeRect>,
) {
    // A player that has not spawned yet is not in an unrecognised preset,
    // it simply has no body — the distinction the Controls sheet also
    // makes (#1241 f161).
    let unrecognised = !players.is_empty() && chassis.is_empty();
    let mode = movement_mode(
        unrecognised,
        avatar_editor.holds_avatar_still(),
        movement.water,
    );
    if mode.is_none() && !movement.camera_submerged {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if movement.camera_submerged {
        // The underwater cue (#1241 f160). Painted on egui's BACKGROUND
        // layer, so it tints the 3D scene while every window still sits on
        // top of it — and on a bare painter, which takes no pointer input
        // (an interactive area over the viewport sets
        // `wants_pointer_input` and would silently break orbiting the
        // camera through it, the lesson `ui::nametag` records).
        ctx.layer_painter(egui::LayerId::background()).rect_filled(
            ctx.content_rect(),
            0.0,
            UNDERWATER_TINT,
        );
    }
    let Some(mode) = mode else {
        return;
    };
    let rect = free.0.unwrap_or_else(|| ctx.content_rect());
    let th = crate::ui::theme::current(ctx);
    let colour = if mode.is_a_problem() {
        th.status.warn
    } else {
        th.status.info
    };
    egui::Area::new(egui::Id::new("movement-mode-banner"))
        .order(egui::Order::Background)
        .interactable(false)
        .fixed_pos(egui::pos2(rect.center().x, rect.top() + 8.0))
        .pivot(egui::Align2::CENTER_TOP)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.colored_label(colour, mode.line());
            });
        });
}

/// Marker query alias: "this chassis has SOME locomotion preset".
///
/// Every drive system is marker-queried, so the absence of all five is
/// exactly the state in which nothing runs (#1241 f161). Spelled as a
/// query filter rather than re-reading `LiveAvatarRecord.locomotion`,
/// because the record can be edited a frame before the hot-swap rebuilds
/// the body and the banner must describe the BODY.
type AnyPresetMarker = bevy::ecs::query::Or<(
    With<crate::player::HumanoidPreset>,
    With<crate::player::HoverBoatPreset>,
    With<crate::player::CarPreset>,
    With<crate::player::HelicopterPreset>,
    With<crate::player::AirplanePreset>,
)>;

#[cfg(test)]
mod tests {
    use super::*;

    /// #1240 f170 + #1241 f160/f161. The four modes and their priority,
    /// as one descending sequence: two of them can be true at once (a
    /// frozen avatar standing in a lake), and the banner has one line.
    #[test]
    fn the_most_total_mode_wins_and_dry_says_nothing() {
        let swimming = WaterState::Swimming { depth: 1.0 };
        assert_eq!(
            movement_mode(true, true, swimming),
            Some(MovementMode::UnrecognisedPreset),
            "no drive system exists; nothing else the banner could say is true"
        );
        assert_eq!(
            movement_mode(false, true, swimming),
            Some(MovementMode::HeldStill),
            "a frozen body is not swimming, it is parked in water"
        );
        assert_eq!(
            movement_mode(false, false, swimming),
            Some(MovementMode::Swimming)
        );
        assert_eq!(
            movement_mode(false, false, WaterState::Wading { depth: 0.4 }),
            Some(MovementMode::Wading)
        );
        assert_eq!(movement_mode(false, false, WaterState::Dry), None);
    }

    /// #1241 f160. The remap is the whole point of the swimming line:
    /// Space and Shift do the OPPOSITE of what the Controls sheet's
    /// on-foot rows say while the head is under water, and a player who
    /// dives and presses Shift to run sinks instead.
    #[test]
    fn the_swimming_line_names_both_remapped_keys() {
        let line = MovementMode::Swimming.line();
        assert!(line.contains("Space"), "{line}");
        assert!(line.contains("Shift"), "{line}");
    }

    /// #1240 f170. The freeze is entered by an innocuous click in a list
    /// and the symptom is identical to a hang, so the line has to carry
    /// the way out with it.
    #[test]
    fn the_held_still_line_names_the_key_that_releases_it() {
        assert!(MovementMode::HeldStill.line().contains("Esc"));
    }
}
