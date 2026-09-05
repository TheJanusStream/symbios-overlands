//! Locomotion-tab UI: per-preset slider panel plus the central preset
//! picker. Each preset's [`LocomotionPanel`] impl lives in its own
//! submodule so adding a new preset is one new file + one match arm in
//! [`draw_locomotion_tab`].

mod airplane;
mod car;
mod gait;
mod helicopter;
mod hover_boat;
mod humanoid;

use bevy_egui::egui;

use crate::pds::{Fp, GaitParams, LocomotionConfig};
use crate::ui::modes::LocalMovement;

/// Egui detail panel for one locomotion preset. Implemented on each
/// `*Params` struct in this module's siblings — `draw_locomotion_tab`
/// dispatches to whichever variant the live `LocomotionConfig` carries.
pub trait LocomotionPanel {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool, facts: &LocalMovement);
}

/// The name of the preset whose tuning a switch away from `current` would
/// throw away, or `None` when it would throw away nothing (#1256 f102).
///
/// "Nothing" means the live config still equals its own preset defaults —
/// the exact comparison the retired #838 confirm modal used to gate on. An
/// untuned switch stays silent, because the sentence is about loss, not
/// about having clicked.
fn discarded_tuning(current: &LocomotionConfig) -> Option<&'static str> {
    let kind = current.kind_tag();
    LocomotionConfig::pickers()
        .iter()
        .find(|(k, _, _)| *k == kind)
        .filter(|(_, _, ctor)| ctor() != *current)
        .map(|(_, label, _)| *label)
}

/// What the owner is told at the moment their tuning is replaced.
///
/// Undo alone was not enough: the loss was INVISIBLE when it happened — the
/// panel simply redrew with different sliders — so by the time anyone
/// noticed, the 32-entry ring could have rolled past it. `undo_label` only
/// names a future undo ENTRY; the undo toast fires on Ctrl+Z, which is
/// exactly the gesture someone who does not know they lost anything will
/// never make.
fn switch_discard_line(from: &str, to: &str) -> String {
    format!("Switched to {to} — your {from} tuning was replaced with defaults. Ctrl+Z restores it.")
}

/// Render the picker row (one selectable label per preset, switching
/// preset replaces `*locomotion` with the new variant's default-tuned
/// instance) followed by the per-preset detail panel.
///
/// #838 originally routed a lossy switch through the shared confirm modal;
/// #866 retired that in favour of undo. The undo contract is the one in
/// force: switching preset replaces the whole config with the new variant's
/// defaults, the switch is one entry in the ring, and `undo_label` names it.
///
/// #1256 f102: undo alone was not enough, because the loss was INVISIBLE at
/// the moment it happened — the panel simply redrew with different sliders,
/// so by the time an owner noticed their tuning was gone the 32-entry ring
/// could have rolled past it. The switch now says what it replaced, at the
/// moment it replaces it, and `toasts` is threaded here for that. A switch
/// that discards nothing (the config still IS its own defaults) stays
/// silent — the sentence is about loss, not about clicking.
#[allow(clippy::too_many_arguments)]
pub fn draw_locomotion_tab(
    ui: &mut egui::Ui,
    locomotion: &mut LocomotionConfig,
    gait: &mut Option<GaitParams>,
    fallback_seed: u64,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
    // What the live body is actually doing (#1241 f168): a panel that
    // tunes movement needs to be able to say when a value it publishes
    // has stopped having an effect on THIS body.
    facts: &LocalMovement,
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
) {
    let current_kind = locomotion.kind_tag();

    ui.horizontal_wrapped(|ui| {
        ui.label("Preset:");
        for (kind, label, ctor) in LocomotionConfig::pickers() {
            // Fires on the click itself (#866): pre-undo this asked for
            // confirmation when tuning would be discarded, but a switch
            // is now one Ctrl+Z away and the toast names it.
            if ui.selectable_label(current_kind == *kind, *label).clicked() && current_kind != *kind
            {
                // Measured BEFORE the replacement.
                let lost = discarded_tuning(locomotion);
                *locomotion = ctor();
                undo_label.set(format!("preset switch to {label}"));
                *dirty = true;
                if let Some(from) = lost {
                    toasts.warn(switch_discard_line(from, label), now);
                }
            }
        }
    });
    ui.separator();

    match locomotion {
        LocomotionConfig::HoverBoat(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Humanoid(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Airplane(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Helicopter(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Car(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Unknown => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                "This avatar's locomotion preset was authored against a newer schema — \
                 pick a preset above to replace it.",
            );
        }
    }

    ui.separator();
    gait::draw_gait_section(ui, locomotion, gait, fallback_seed, dirty, undo_label);
}

// ---------------------------------------------------------------------------
// Shared widgets — narrower than `ui::room::widgets::fp_slider` (this one
// takes a step size and emits no inline label, leaving the caller to draw
// labels next to a stack of related sliders).
// ---------------------------------------------------------------------------

pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    dirty: &mut bool,
) {
    if ui
        .add(crate::ui::num::slider(&mut value.0, range).step_by(step))
        .changed()
    {
        *dirty = true;
    }
}

/// Three-component drag editor for `Fp3` half-extents (or any other
/// vec3-shaped numeric triple). Edits land in the underlying `[f32; 3]`
/// directly so the caller's `Fp3` wrapper picks up the change without an
/// intermediate copy.
pub(super) fn fp3_extents(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        for axis in value.iter_mut() {
            if ui
                .add(crate::ui::num::drag(axis).speed(0.05).range(0.05..=20.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Move one authored value on whichever preset this is — the smallest
    /// possible "the owner tuned something".
    fn tune(cfg: &mut LocomotionConfig) {
        match cfg {
            LocomotionConfig::Humanoid(p) => {
                p.capsule_radius = crate::pds::types::Fp(p.capsule_radius.0 + 0.1);
            }
            LocomotionConfig::HoverBoat(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Car(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Helicopter(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Airplane(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Unknown => panic!("every pickable preset is a known one"),
        }
    }

    /// THE SEQUENCE (#1256 f102): tune the Car preset for ten minutes,
    /// click Helicopter to compare, click Car again — and every value is
    /// back to default, with no warning at any point.
    ///
    /// #838 guarded this with a confirm modal; #866 retired that in favour
    /// of undo, on the grounds that "a switch is now one Ctrl+Z away and the
    /// toast names it". No toast fired here: `undo_label.set` only names a
    /// future undo ENTRY, and the undo toast appears when the user presses
    /// Ctrl+Z — which is exactly the gesture someone who does not know they
    /// lost anything will never make. Undo is only a remedy for someone who
    /// notices within 32 edits.
    #[test]
    fn switching_away_from_tuned_settings_says_what_it_replaced() {
        // An untuned preset is still its own defaults: nothing is lost, so
        // nothing is said. Clicking around must not manufacture warnings.
        for (_, label, ctor) in LocomotionConfig::pickers() {
            let untouched = ctor();
            assert_eq!(
                discarded_tuning(&untouched),
                None,
                "an untouched {label} has no tuning to lose"
            );
        }

        // Tune each one, and the switch owes the owner a sentence naming it.
        for (_, label, ctor) in LocomotionConfig::pickers() {
            let mut tuned = ctor();
            tune(&mut tuned);
            assert_eq!(
                discarded_tuning(&tuned),
                Some(*label),
                "a tuned {label} must name itself as what the switch throws away"
            );
        }

        // And the sentence says what happened and how to undo it — the
        // whole point being that it arrives at the moment of the loss.
        let line = switch_discard_line("Car", "Helicopter");
        assert!(line.contains("Car") && line.contains("Helicopter"));
        assert!(
            line.contains("Ctrl+Z"),
            "the remedy has to be in the sentence: {line}"
        );
    }

    /// A record authored against a newer schema has no preset defaults to
    /// compare against, so a switch away from it claims no loss — the panel
    /// above already tells the owner to replace it.
    #[test]
    fn an_unrecognised_preset_claims_no_lost_tuning() {
        assert_eq!(discarded_tuning(&LocomotionConfig::Unknown), None);
    }
}
