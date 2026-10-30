//! The On foot locomotion panel - a person's capsule and a beast's collider.
//!
//! A person's capsule is theirs to tune: radius, length, mass and jump are
//! sliders. A four-legged beast's collider lies along its trunk and is sized
//! from its body on every edit of it (#1803, owner decision D3), so for a
//! beast those rows are a read-only "Body collider" readout instead - its
//! length, its depth, how high its back stands and where along it the
//! collider sits - that says it follows the body. The run, the water speeds
//! and the derived walk are a beast's as much as a person's, and stay.
//!
//! Which of the two the panel draws is the BODY's kind ([`carries_beast`]),
//! not the preset's shape: a person under a lying preset someone else saved
//! is a person, and is offered a person's capsule to set right. Only when
//! the body is not one this build can read, or is not resolved yet, does
//! the shape decide. That is why this panel is a function of the body as
//! well as the preset, where every other preset's is a
//! [`super::LocomotionPanel`] impl.

use bevy_egui::egui;

use super::fp_slider;
use crate::pds::HumanoidParams;
use crate::player::LocalMovement;

// Whether the On foot panel draws a beast's read-only collider for a preset
// under a body (#1803): the body's kind, as the camera frames it (#1804).
pub(super) use crate::pds::avatar::locomotion::carries_beast;

/// The readout's sentence saying who sizes a beast's collider (#1803):
/// painted, not hovered, because it explains why there is nothing to drag.
pub(super) const FOLLOWS_THE_BODY: &str =
    "Follows the body - sculpt it on the Body tab and the collider, mass and jump follow.";

impl HumanoidParams {
    /// The On foot panel: a beast's "Body collider" readout when `beast`
    /// ([`carries_beast`]), a person's capsule sliders otherwise.
    pub(super) fn draw_on_foot(
        &mut self,
        ui: &mut egui::Ui,
        dirty: &mut bool,
        facts: &LocalMovement,
        beast: bool,
    ) {
        if beast {
            body_collider(self, ui, dirty);
        } else {
            egui::CollapsingHeader::new("Capsule")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label("Radius (m)");
                    fp_slider(ui, &mut self.capsule_radius, 0.05..=1.0, 0.01, dirty);
                    ui.label("Cylinder length (m)");
                    fp_slider(ui, &mut self.capsule_length, 0.1..=4.0, 0.05, dirty);
                    ui.label(
                        egui::RichText::new(format!(
                            "Total height ≈ {:.2} m (length + 2·radius)",
                            self.total_height()
                        ))
                        .small()
                        .weak(),
                    );
                    ui.label("Mass (kg)");
                    fp_slider(ui, &mut self.mass, 30.0..=150.0, 1.0, dirty);
                    ui.label("Linear damping");
                    fp_slider(ui, &mut self.linear_damping, 0.0..=3.0, 0.05, dirty);
                });
        }

        egui::CollapsingHeader::new("Locomotion")
            .default_open(true)
            .show(ui, |ui| {
                // The record field is still `walk_speed` on the wire, but
                // since the run key (#1193) it is the speed SHIFT travels at;
                // unshifted walking is derived from the body. The label says
                // what the slider does now.
                ui.label("Run speed (m/s)");
                fp_slider(ui, &mut self.walk_speed, 1.0..=10.0, 0.1, dirty);
                // The dead band at the bottom of that slider (#1241 f168).
                // `apply_humanoid_walk` takes `walking.min(travel)`, so a
                // run tuned at or below the body's own walk collapses both
                // branches to one number and Shift stops doing anything -
                // with no message, no disabled control, just a key that
                // stopped working. The derived walk is ~1.85 m/s on the
                // default body against a slider that starts at 1.0, so
                // roughly the bottom tenth of its travel is the trap.
                if let Some(walk) = facts.derived_walk {
                    ui.label(
                        egui::RichText::new(format!("Walk is ~{walk:.1} m/s on this body"))
                            .small()
                            .weak(),
                    );
                    if crate::player::humanoid::run_key_is_dead(self.walk_speed.0, walk) {
                        ui.colored_label(
                            crate::ui::theme::current(ui.ctx()).status.warn,
                            "Run speed is at or below this body's walk - Shift will \
                             have no effect.",
                        );
                    }
                }
                ui.label("Acceleration (1/s)");
                fp_slider(ui, &mut self.acceleration, 2.0..=30.0, 0.5, dirty);
                // A beast's jump is its body's (D4), in the readout above.
                if !beast {
                    ui.label("Jump impulse (N·s)");
                    fp_slider(ui, &mut self.jump_impulse, 100.0..=1500.0, 10.0, dirty);
                }
                ui.label("Stop snappiness (1/s)");
                fp_slider(ui, &mut self.stop_damping, 1.0..=60.0, 0.5, dirty);
                ui.label("Facing turn rate (1/s)");
                fp_slider(ui, &mut self.turn_rate, 1.0..=30.0, 0.5, dirty);
            });

        egui::CollapsingHeader::new("Water")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Wading speed factor");
                fp_slider(ui, &mut self.wading_speed_factor, 0.0..=1.0, 0.05, dirty);
                ui.label("Swim speed (m/s)");
                fp_slider(ui, &mut self.swim_speed, 0.5..=8.0, 0.1, dirty);
                ui.label("Swim vertical speed (m/s)");
                fp_slider(ui, &mut self.swim_vertical_speed, 0.2..=5.0, 0.1, dirty);
            });
    }
}

/// A beast's "Body collider" section (#1803): what its body sized, read
/// only, and the one row of it the body does not size - the damping.
///
/// The height a beast stands is its back's, the top of its trunk over its
/// feet ([`HumanoidParams::stand_height`]), never the lying capsule's own
/// diameter, which is only the trunk's depth.
fn body_collider(p: &mut HumanoidParams, ui: &mut egui::Ui, dirty: &mut bool) {
    egui::CollapsingHeader::new("Body collider")
        .default_open(true)
        .show(ui, |ui| {
            for line in collider_lines(p) {
                ui.label(line);
            }
            ui.label(egui::RichText::new(FOLLOWS_THE_BODY).small().weak());
            ui.label("Linear damping");
            fp_slider(ui, &mut p.linear_damping, 0.0..=3.0, 0.05, dirty);
        });
}

/// The readout's lines, in the order they are painted.
pub(super) fn collider_lines(p: &HumanoidParams) -> [String; 5] {
    let (radius, length) = (p.capsule_radius.0, p.capsule_length.0);
    let ahead = p.capsule_ahead();
    [
        format!("Length {:.2} m along the trunk", length + 2.0 * radius),
        format!("Depth {:.2} m, back to belly", 2.0 * radius),
        format!("Back height {:.2} m over the feet", p.stand_height()),
        format!(
            "Centred {:.2} m {} the body's origin",
            ahead.abs(),
            if ahead < 0.0 { "behind" } else { "ahead of" }
        ),
        format!(
            "Mass {:.1} kg, jump impulse {:.0} N·s",
            p.mass.0, p.jump_impulse.0
        ),
    ]
}
