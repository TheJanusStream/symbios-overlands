//! Idle-motion (gait) section of the Locomotion tab — the editor surface
//! for the record's optional [`GaitParams`] (#875).
//!
//! A record without a `gait` section renders the DID/seed-derived idle
//! motion (see [`crate::pds::avatar::gait`]), so the sliders start from
//! that derivation and the record only materialises an explicit section
//! once the owner actually moves one — untouched avatars keep publishing
//! byte-identical records.

use bevy_egui::egui;

use super::fp_slider;
use crate::pds::{GaitParams, LocomotionConfig};
use crate::player::gait::GaitMode;

/// Draw the "Idle motion" collapsing section. `fallback_seed` feeds both
/// the slider baseline for a record without an explicit `gait` section
/// and the ⟲ re-derive button — it is the master seed from the editor's
/// seed row (falling back to the DID seed), so re-deriving idle motion
/// agrees with what a whole-avatar re-roll of the same seed would set.
pub(super) fn draw_gait_section(
    ui: &mut egui::Ui,
    locomotion: &LocomotionConfig,
    gait: &mut Option<GaitParams>,
    fallback_seed: u64,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
) {
    egui::CollapsingHeader::new("Idle motion")
        .default_open(false)
        .show(ui, |ui| {
            let Some(mode) = GaitMode::for_locomotion(locomotion) else {
                ui.label(
                    egui::RichText::new(
                        "This preset has no idle profile — the Airplane rolls its \
                         chassis directly, so a visual-root sway would double up. \
                         Idle-motion tuning applies when a different preset is \
                         active.",
                    )
                    .small()
                    .weak(),
                );
                return;
            };

            // Baseline: the explicit record section, else the seed
            // derivation every peer falls back to. Edits materialise the
            // section; an untouched panel writes nothing.
            let mut p = gait
                .clone()
                .unwrap_or_else(|| GaitParams::for_seed(fallback_seed));
            let mut changed = false;

            if gait.is_none() {
                ui.label(
                    egui::RichText::new(
                        "Derived from the avatar seed — move a slider to customise.",
                    )
                    .small()
                    .weak(),
                );
            }

            // The humanoid profile is the only walker; the vehicle
            // profiles reuse sway amplitude/frequency and head-turn as
            // heave / list / shiver / nose-wander scale.
            if mode == GaitMode::Humanoid {
                ui.label("Step cadence (steps/s at full walk speed)");
                fp_slider(ui, &mut p.step_cadence, 0.2..=6.0, 0.05, &mut changed);
                ui.label("Step bounce (m)");
                fp_slider(
                    ui,
                    &mut p.step_bounce_amplitude,
                    0.0..=0.3,
                    0.005,
                    &mut changed,
                );
            }
            ui.label("Sway frequency (Hz)");
            fp_slider(
                ui,
                &mut p.idle_sway_frequency,
                0.0..=3.0,
                0.05,
                &mut changed,
            );
            ui.label("Sway amplitude (m)");
            fp_slider(
                ui,
                &mut p.idle_sway_amplitude,
                0.0..=0.2,
                0.005,
                &mut changed,
            );
            ui.label("Head turn / nose wander (±°)");
            fp_slider(
                ui,
                &mut p.head_turn_variance_degrees,
                0.0..=60.0,
                1.0,
                &mut changed,
            );
            ui.label("Overall intensity");
            fp_slider(ui, &mut p.idle_intensity, 0.0..=3.0, 0.05, &mut changed);

            if changed {
                *gait = Some(p);
                *dirty = true;
            }

            if ui
                .button("⟲ Re-derive from seed")
                .on_hover_text(
                    "Replace the idle-motion tuning with the values the current \
                     master seed derives — what an untouched avatar of this seed \
                     would show.",
                )
                .clicked()
            {
                *gait = Some(GaitParams::for_seed(fallback_seed));
                undo_label.set("idle-motion reseed".to_string());
                *dirty = true;
            }
        });
}
