//! One body plan per land craft, and every line and mount read off it.
//!
//! The skiff's counterpart of the boat's
//! [`HullProfile`](super::super::boats::HullProfile), and of the airship's
//! `EnvProfile` before it, for the same reason: the airship works because a
//! single profile function feeds the envelope, every trim line and every
//! mount, so nothing it carries can float. The legacy skiff did not. Its
//! canopy mounted at a fixed `y = 0.33` that only one of its four chassis ever
//! reached, so on the other three it hovered over the machine in open air, and
//! its fenders, wheels and anchors agreed only because three files repeated
//! the same magic numbers (#1359 diagnosis, #1364 item 1).
//!
//! # The shape of it
//!
//! A plan is up to [`MAX_SWEEP_POINTS`] stations, and each carries exactly one
//! number: the body's **half-width** there. The section's depth is not a
//! second free number - one swept tube has one radius per station, so
//!
//! ```text
//! crown(z) =  half_width(z) x section
//! sill(z)  = -half_width(z) x section
//! ```
//!
//! and the body's profile falls out of its plan form. That coupling is the
//! point rather than a limitation: it is what makes a bonnet that narrows
//! toward the radiator also drop toward it, the way a real one does, and it is
//! what lets a lamp, a louvre or a rubbing strip be seated on the skin by
//! asking [`BodyPlan::side_at`] instead of by guessing a fraction of the
//! width.
//!
//! # The authoring frame
//!
//! Every dimension is in TRUE METRES, nose at `+Z`, and `y = 0` is the body
//! **datum**: the plane the body sweeps are cut on, which is the cockpit
//! coaming line and the one horizontal every panel is measured from. The
//! ground is [`BodyPlan::datum_height`] *below* it.
//!
//! The datum rather than the ground line, although every band in the brief is
//! quoted above the ground - because the assembled visual root IS a hidden hub
//! sitting exactly on the visual origin (the assembler overwrites its
//! translation in
//! [`apply_travel_pose`](super::super::assemble::apply_travel_pose)), and a
//! hub on the ground line would stand in open air under the car. On the datum
//! it is buried in the bodywork whatever the seed.

use crate::pds::sanitize::limits::MAX_SWEEP_POINTS;
use crate::seeded_defaults::SkiffBlueprint;

/// One axle of a body plan.
///
/// The wheel anchor SET is a property of the plan, never a slug-string check
/// on the chassis part the way the legacy trike's single front wheel was: a
/// type declares its axles and the wheels, the guards and the beams all read
/// the same list. Four wheels today; three, two and six arrive with
/// #1374-#1378 as two more rows in a table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Axle {
    /// Station in units of half the wheelbase from its centre: `+1.0` is the
    /// front axle and `-1.0` the rear, so a six-wheeler adds one between.
    pub(crate) at: f32,
    /// A pair of wheels, or a single one on the centreline (a cyclecar's
    /// front).
    pub(crate) paired: bool,
}

/// Where a craft type puts the stations every land craft has.
///
/// Separate from the plan-form table because the two answer different
/// questions - the table is the body's *shape*, this is what is arranged along
/// it - and because a type that shares a body with another (a coupe over the
/// roadster's tub, #1367) wants to move the second without touching the first.
#[derive(Debug)]
pub(crate) struct Layout {
    /// Where the bonnet gives way to the scuttle, as a z fraction of the
    /// overall length.
    pub(crate) cowl: f32,
    /// The cockpit opening's after and forward ends, likewise. Everything
    /// between them is open: the tub's bored shell is the footwell and the
    /// scuttle and tail deck stop at these two stations.
    pub(crate) cockpit: (f32, f32),
    /// The seat back's station.
    pub(crate) seat: f32,
    /// The radiator shell's face.
    pub(crate) radiator: f32,
    /// The bumper bar, which is what actually sets the overall length.
    pub(crate) bumper: f32,
    /// The axles, front first.
    pub(crate) axles: &'static [Axle],
}

/// A land craft's body, as the one function every part of her is read off.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BodyPlan {
    /// Overall length, bumper to tail (m).
    pub(crate) length: f32,
    /// Maximum bodywork width (m) - not the track.
    pub(crate) body_w: f32,
    /// Wheel-centre to wheel-centre, fore and aft (m).
    pub(crate) wheelbase: f32,
    /// Wheel-centre to wheel-centre, across (m).
    pub(crate) track: f32,
    /// Wheel outer radius (m).
    pub(crate) wheel_r: f32,
    /// The bodywork's crown above the ground (m).
    pub(crate) beltline: f32,
    /// Overall height above the ground (m).
    pub(crate) height: f32,
    /// Section depth per unit half-width - the one knob that turns a plan form
    /// into a body, and the one node scale every body sweep shares so their
    /// sections meet flush. See the module docs.
    pub(crate) section: f32,
    /// `(z fraction of the length, half-width fraction)` tail to nose - the
    /// type's own plan form, and the only thing about a body that is a table.
    stations: &'static [(f32, f32)],
    /// What is arranged along it.
    layout: &'static Layout,
}

impl BodyPlan {
    /// The plan for a seeded machine of this type: the blueprint's true
    /// dimensions under the type's own plan form, section depth and layout.
    pub(crate) fn new(
        bp: &SkiffBlueprint,
        section: f32,
        stations: &'static [(f32, f32)],
        layout: &'static Layout,
    ) -> Self {
        debug_assert!(
            stations.len() >= 2 && stations.len() <= MAX_SWEEP_POINTS,
            "a plan form is 2..={MAX_SWEEP_POINTS} stations, got {}",
            stations.len()
        );
        Self {
            length: bp.length,
            body_w: bp.body_w,
            wheelbase: bp.wheelbase,
            track: bp.track,
            wheel_r: bp.wheel_r,
            beltline: bp.beltline,
            height: bp.height,
            section,
            stations,
            layout,
        }
    }

    // --- the body's own surface --------------------------------------------

    /// Half the maximum bodywork width (m).
    pub(crate) fn half_w(&self) -> f32 {
        self.body_w * 0.5
    }

    /// Half the body's depth at its widest station (m) - the distance from the
    /// datum to the crown, and to the sill.
    pub(crate) fn depth(&self) -> f32 {
        self.half_w() * self.section
    }

    /// The body's half-width at `z` (m), linearly between stations.
    pub(crate) fn half_width_at(&self, z: f32) -> f32 {
        let hw = self.half_w();
        let first = self.stations[0];
        if z <= first.0 * self.length {
            return first.1 * hw;
        }
        for pair in self.stations.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (za, zb) = (a.0 * self.length, b.0 * self.length);
            if z <= zb {
                let t = (z - za) / (zb - za);
                return (a.1 + (b.1 - a.1) * t) * hw;
            }
        }
        self.stations[self.stations.len() - 1].1 * hw
    }

    /// The top of the bodywork at `z` (m above the datum).
    pub(crate) fn crown_at(&self, z: f32) -> f32 {
        self.half_width_at(z) * self.section
    }

    /// The bottom of the bodywork at `z` (m, negative under the datum).
    pub(crate) fn sill_at(&self, z: f32) -> f32 {
        -self.crown_at(z)
    }

    /// Where the body's crowned FLANK stands at station `z` and height `y`.
    ///
    /// The elliptical section's own x, so a lamp, a louvre, a rubbing strip or
    /// an exhaust is seated ON the skin rather than at a guessed fraction of
    /// the width. It is the read the sloop's boot stripe makes of her hull,
    /// and the reason a trim line on either family cannot drift.
    pub(crate) fn side_at(&self, z: f32, y: f32) -> f32 {
        let hw = self.half_width_at(z);
        let t = (y.abs() / (hw * self.section).max(1e-4)).min(0.999);
        hw * (1.0 - t * t).max(0.04).sqrt()
    }

    /// The plan's own stations between two `z`, with the ends interpolated in,
    /// as a sweep path on the datum.
    ///
    /// Every run of the body - bonnet, tub, scuttle, tail deck - is one of
    /// these, which is what makes them meet flush: they are literally the same
    /// profile, sampled over different stretches.
    pub(crate) fn run(&self, from: f32, to: f32) -> Vec<([f32; 3], f32)> {
        let mut zs = vec![from];
        zs.extend(
            self.stations
                .iter()
                .map(|&(zf, _)| zf * self.length)
                .filter(|&z| from < z && z < to),
        );
        zs.push(to);
        zs.iter()
            .map(|&z| ([0.0, 0.0, z], self.half_width_at(z)))
            .collect()
    }

    // --- where the machine sits ---------------------------------------------

    /// How far the ground is below the datum (m).
    ///
    /// Read from the beltline rather than authored: the brief quotes the
    /// beltline above the ground and the datum is one section-depth under the
    /// crown, so this is the one arithmetic that ties the drawn body to the
    /// plane it stands on. [`super::travel_drop`] is the only caller that
    /// matters, and it is what puts the tyres on the suspension's ground line.
    pub(crate) fn datum_height(&self) -> f32 {
        self.beltline - self.depth()
    }

    /// The wheel centres' line (m, under the datum).
    ///
    /// Not a blueprint field: an axle is exactly one wheel radius above the
    /// ground, so deriving it here means the axle line and the wheels standing
    /// on it *cannot* disagree. The legacy blueprint carried a separate
    /// `ride_y`, and keeping the two in step was the reader's problem.
    pub(crate) fn axle_y(&self) -> f32 {
        self.wheel_r - self.datum_height()
    }

    /// The top of the tallest thing the machine carries (m above the datum).
    pub(crate) fn screen_top(&self) -> f32 {
        self.height - self.datum_height()
    }

    /// The widest the machine is drawn, guards included (m) - what a gateway
    /// mouth is measured against.
    pub(crate) fn drawn_width(&self, guard: f32) -> f32 {
        self.track + 2.0 * guard
    }

    // --- named mount stations -----------------------------------------------
    //
    // A part mounts on one of these rather than on a fraction it wrote down.
    // Dressing stations - a mascot here, a lamp there - belong with the slice
    // that dresses a machine (#1379) and are deliberately absent until
    // something reads them.

    /// The nose and the tail of the bodywork (m from the body's centre).
    pub(crate) fn nose_z(&self) -> f32 {
        self.stations[self.stations.len() - 1].0 * self.length
    }
    pub(crate) fn tail_z(&self) -> f32 {
        self.stations[0].0 * self.length
    }

    /// The scuttle break - the bonnet's after end, and the screen's foot.
    pub(crate) fn cowl_z(&self) -> f32 {
        self.layout.cowl * self.length
    }

    /// The cockpit opening's after and forward ends (m).
    pub(crate) fn cockpit_z(&self) -> (f32, f32) {
        (
            self.layout.cockpit.0 * self.length,
            self.layout.cockpit.1 * self.length,
        )
    }

    /// The seat back's station (m).
    pub(crate) fn seat_z(&self) -> f32 {
        self.layout.seat * self.length
    }

    /// The radiator shell's face, and the bumper bar (m).
    pub(crate) fn radiator_z(&self) -> f32 {
        self.layout.radiator * self.length
    }
    pub(crate) fn bumper_z(&self) -> f32 {
        self.layout.bumper * self.length
    }

    /// Where a hood, a tonneau or a hardtop seats: the coaming plane, centred
    /// on the cockpit.
    ///
    /// **Published by the body** (#1364 item 1), which is the whole of that
    /// item. The legacy canopy chose its own height and hovered over three of
    /// the four chassis it could land on; a canopy that asks cannot. Nothing
    /// reads it until the roadster grows a hardtop in #1367, and that is
    /// deliberate: the seat has to be the body's property *before* anything
    /// sits on it, or the next type repeats the mistake.
    pub(crate) fn canopy_seat(&self) -> [f32; 3] {
        let (aft, fwd) = self.cockpit_z();
        [0.0, 0.0, (aft + fwd) * 0.5]
    }

    /// The four (or three, or six) wheel anchors, front axle first.
    pub(crate) fn wheel_anchors(&self) -> Vec<[f32; 3]> {
        let (y, half_wb, half_track) = (self.axle_y(), self.wheelbase * 0.5, self.track * 0.5);
        let mut out = Vec::with_capacity(self.layout.axles.len() * 2);
        for axle in self.layout.axles {
            let z = axle.at * half_wb;
            if axle.paired {
                out.push([-half_track, y, z]);
                out.push([half_track, y, z]);
            } else {
                out.push([0.0, y, z]);
            }
        }
        out
    }

    /// Each axle's station along the machine (m), front first - what a beam is
    /// drawn on, so a beam cannot miss the wheels it carries.
    pub(crate) fn axle_stations(&self) -> Vec<f32> {
        self.layout
            .axles
            .iter()
            .map(|a| a.at * self.wheelbase * 0.5)
            .collect()
    }
}
