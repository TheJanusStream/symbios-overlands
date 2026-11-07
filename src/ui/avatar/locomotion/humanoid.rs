//! The On foot locomotion panel - a person's capsule, a beast's collider and
//! a winged body's collider and flight.
//!
//! A person's capsule is theirs to tune: radius, length, mass and jump are
//! sliders. A four-legged beast's collider lies along its trunk and is sized
//! from its body on every edit of it (#1803, owner decision D3), so for a
//! beast those rows are a read-only "Body collider" readout instead - its
//! length, its depth, how high its back stands and where along it the
//! collider sits - that says it follows the body. The run, the water speeds
//! and the derived walk are a beast's as much as a person's, and stay.
//!
//! A winged body (#1876) is sized from its body as a beast is (#1875): its
//! collider is the same read-only readout - lying along the trunk of a bat
//! or a glider, standing upright round a bird - and its jump is its body's.
//! It flies, so it is shown its flight too: the handling its owner tunes -
//! how many beats its stamina holds and how fast it comes back, the glide
//! ratio, and the bank, roll and pitch it turns by, the fields a refit keeps
//! ([`crate::pds::FlightParams::keep_handling`]) - as sliders, and the
//! numbers its body sets - its flap power, trim and stall speeds, a beat's
//! push, the wingbeat and the take-off leap - read only.
//!
//! Which of these the panel draws is the BODY's kind ([`OnFootBody::of`]),
//! not the preset's shape: a person under a lying preset someone else saved
//! is a person, and is offered a person's capsule to set right. Only when
//! the body is not one this build can read, or is not resolved yet, does
//! the shape decide. That is why this panel is a function of the body as
//! well as the preset, where every other preset's is a
//! [`super::LocomotionPanel`] impl.

use bevy_egui::egui;

use super::fp_slider;
use crate::pds::avatar::EngineAvatarRecord;
use crate::pds::avatar::RiggedKind;
use crate::pds::avatar::locomotion::carries_beast;
use crate::pds::{FlightParams, HumanoidParams};
use crate::player::LocalMovement;

/// The readout's sentence saying who sizes a beast's collider (#1803):
/// painted, not hovered, because it explains why there is nothing to drag.
pub(super) const FOLLOWS_THE_BODY: &str =
    "Follows the body - sculpt it on the Body tab and the collider, mass and jump follow.";

/// The flight section's sentence saying who sets the numbers it reads out
/// (#1876), painted for the same reason.
pub(super) const FLIGHT_FOLLOWS_THE_BODY: &str =
    "Set by the body - sculpt it on the Body tab and these follow. The sliders below are yours.";

/// What a winged record whose flight section an older client dropped is told
/// (#1876): the section comes back with the body's next refit.
pub(super) const NO_FLIGHT_SECTION: &str = "This record carries no flight section - an older \
     client dropped it. The body's own comes back with the next edit of the body.";

/// Which body the On foot panel is drawn for (#1803, #1876): the body's
/// kind, as the camera frames it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OnFootBody {
    /// A person, offered their capsule to tune.
    Person,
    /// A four-legged beast, its collider read out ([`carries_beast`]).
    Beast,
    /// A winged body of any family: its collider read out, as fitted, and
    /// its flight.
    Winged,
}

impl OnFootBody {
    /// The panel for the preset `p` under `body`: a winged body by its
    /// record's kind, and otherwise a beast or a person as
    /// [`carries_beast`] tells them.
    pub(super) fn of(body: Option<&EngineAvatarRecord>, p: &HumanoidParams) -> Self {
        if body.is_some_and(|body| RiggedKind::of_record(body).is_winged()) {
            Self::Winged
        } else if carries_beast(body, p) {
            Self::Beast
        } else {
            Self::Person
        }
    }

    /// Whether the body sizes the collider, the mass and the jump, which are
    /// then read out rather than offered.
    fn sized_by_body(self) -> bool {
        !matches!(self, Self::Person)
    }
}

impl HumanoidParams {
    /// The On foot panel for `body`: a person's capsule sliders, or the
    /// "Body collider" readout of a body that sizes its own, and a winged
    /// body's flight.
    pub(super) fn draw_on_foot(
        &mut self,
        ui: &mut egui::Ui,
        dirty: &mut bool,
        facts: &LocalMovement,
        body: OnFootBody,
    ) {
        if body.sized_by_body() {
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
                // A beast's jump is its body's (D4), and a winged body's
                // (#1875), in the readout above.
                if !body.sized_by_body() {
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

        if body == OnFootBody::Winged {
            match self.flight.as_mut() {
                Some(flight) => flight_section(flight, ui, dirty),
                None => {
                    ui.label(egui::RichText::new(NO_FLIGHT_SECTION).small().weak());
                }
            }
        }
    }
}

/// The "Body collider" section of a body that sizes its own - a beast's
/// (#1803), a winged body's (#1876): what its body sized, read only, and the
/// one row of it the body does not size - the damping.
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

/// The readout's lines, in the order they are painted: a collider lying
/// along the trunk - a beast's, a bat's or a glider's - by its length, its
/// depth, its back's height and where it is centred; one standing upright -
/// a bird's (#1876) - by its height and its width.
///
/// The height a lying body stands is its back's, the top of its trunk over
/// its feet ([`HumanoidParams::stand_height`]), never the lying capsule's own
/// diameter, which is only the trunk's depth.
pub(super) fn collider_lines(p: &HumanoidParams) -> Vec<String> {
    let (radius, length) = (p.capsule_radius.0, p.capsule_length.0);
    let mass = format!(
        "Mass {:.1} kg, jump impulse {:.0} N·s",
        p.mass.0, p.jump_impulse.0
    );
    if !p.is_lying() {
        return vec![
            format!("Height {:.2} m, standing upright", p.total_height()),
            format!("Width {:.2} m across the trunk", 2.0 * radius),
            mass,
        ];
    }
    let ahead = p.capsule_ahead();
    vec![
        format!("Length {:.2} m along the trunk", length + 2.0 * radius),
        format!("Depth {:.2} m, back to belly", 2.0 * radius),
        format!("Back height {:.2} m over the feet", p.stand_height()),
        format!(
            "Centred {:.2} m {} the body's origin",
            ahead.abs(),
            if ahead < 0.0 { "behind" } else { "ahead of" }
        ),
        mass,
    ]
}

/// A winged body's "Flight" section (#1876): what its body sets, read only,
/// and the handling its owner tunes, which every refit of the body keeps
/// ([`FlightParams::keep_handling`]). Each slider's range is the record's own
/// clamp, so no stored value lies off one end of it.
fn flight_section(flight: &mut FlightParams, ui: &mut egui::Ui, dirty: &mut bool) {
    egui::CollapsingHeader::new("Flight")
        .default_open(true)
        .show(ui, |ui| {
            for line in flight_lines(flight) {
                ui.label(line);
            }
            ui.label(egui::RichText::new(FLIGHT_FOLLOWS_THE_BODY).small().weak());
            ui.label("Stamina (full-power beats)");
            fp_slider(ui, &mut flight.stamina_beats, 1.0..=500.0, 1.0, dirty);
            ui.label("Stamina recovery (s)");
            fp_slider(ui, &mut flight.stamina_recovery, 0.5..=120.0, 0.5, dirty);
            ui.label("Glide ratio");
            fp_slider(ui, &mut flight.glide_ratio, 1.0..=20.0, 0.1, dirty);
            ui.label("Steepest bank (°)");
            fp_slider(ui, &mut flight.max_bank, 5.0..=80.0, 1.0, dirty);
            ui.label("Roll rate (°/s)");
            fp_slider(ui, &mut flight.roll_rate, 10.0..=720.0, 5.0, dirty);
            ui.label("Pitch rate (°/s)");
            fp_slider(ui, &mut flight.pitch_rate, 10.0..=360.0, 5.0, dirty);
        });
}

/// The flight section's read-out lines, in the order they are painted: the
/// numbers a winged body's refit sizes from its body (#1875).
pub(super) fn flight_lines(flight: &FlightParams) -> Vec<String> {
    let glide = format!(
        "Glides at {:.1} m/s, stalls under {:.1} m/s",
        flight.trim_speed.0, flight.stall_speed.0
    );
    let leap = format!("Take-off leap {:.1} m/s", flight.takeoff_speed.0);
    // A body that cannot flap is told no wingbeat it never beats.
    if !flight.can_flap() {
        return vec![
            String::from("Cannot flap - it takes off in a strong leap into a glide"),
            glide,
            leap,
        ];
    }
    vec![
        format!("Flap power {:.2}", flight.flight_power.0),
        glide,
        format!(
            "Wingbeat {:.2} a second, each adding {:.1} m/s",
            flight.wingbeat_rate.0, flight.beat_speed.0
        ),
        leap,
    ]
}
