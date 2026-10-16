//! The rigid Zeppelin: one long doped hull with a cruciform tail, a control
//! car under the bow, a lit passenger deck low on each flank and four engine
//! cars on struts - the line of LZ 127 Graf Zeppelin and LZ 129, and the
//! family's hero and universal floor (#1657, epic #1656).
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1656 decisions 2 and 3):
//! `target/dump/airships-1656/zeppelin.py` is its Python twin, and this port
//! reads it number for number - a station along the hull is the same share
//! of its length, the hull's radius is the plan's, and a fitting's size is
//! the prototype's metres grown with the length ([`HullPlan::len`]). It is
//! drawn at the proportion the owner chose - B, a 3.6 m hull at fineness
//! 5.0 - which is the nominal blueprint.
//!
//! [`hull`] draws the hull and everything laid on it or stood on its crown;
//! [`car`] the control car; [`engines`] the engine cars; [`tail`] the fins,
//! rudders, elevators and nav lights; and [`dressing`] the ladder of
//! secondary masses by ornateness.
//!
//! # Where the port leaves the prototype, and why
//!
//! 1. The cheat line down each flank of the hull is the seeded ACCENT on
//!    every scheme, where the prototype drew it in a sketch's trim on three
//!    sketches and left it off the silver one - the fleet's identity rule
//!    (#1365): every craft carries its owner's colour in a few square inches
//!    of trim.
//! 2. The lit windows are the fleet's lit window - the tertiary's own
//!    light, standing off the hull - where each sketch had a fixed amber or
//!    cyan.
//! 3. The fins are laid out where they stand rather than one fin turned into
//!    four places, and the nav lights sit half sunk in the horizontal fins'
//!    tip caps, where the prototype stood them over the hinge line clear of
//!    the slanted cap: see [`tail`].
//! 4. The tier ladder is new ([`dressing`]).

mod car;
mod dressing;
mod engines;
mod hull;
mod tail;
#[cfg(test)]
mod tests;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirshipBlueprint, ParticleAura};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim};
use super::{AirshipCraft, AirshipFeel, HullPlan, HullProfile, airship_colours, dim};

/// Her hull: a blunt superellipse nose over 0.30 of her length, the full
/// girth held for 0.16, and a near-parabolic tail - the prototype's
/// `shape`, through its sixteen stations.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.30,
    parallel: 0.16,
    nose_power: 2.2,
    tail_power: 1.8,
    stations: &[
        0.0, 0.008, 0.024, 0.05, 0.088, 0.14, 0.21, 0.30, 0.40, 0.50, 0.60, 0.70, 0.79, 0.87, 0.94,
        1.0,
    ],
};

/// The side of the hidden root cube on her axis (m, prototype frame).
const ROOT_CUBE: f32 = 0.02;

/// Where a seeded aura issues from: under her keel amidships, between the
/// two pairs of engine cars - share of her length, and how far under the
/// skin (m, prototype frame).
const AURA_X: f32 = 0.5;
const AURA_DROP: f32 = 0.05;

pub(super) struct Zeppelin;

impl AirshipCraft for Zeppelin {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = airship_colours(ctx);
        let side = dim(plan.len(ROOT_CUBE));
        let mut root = prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat());
        let kids = &mut root.children;
        hull::hull(kids, plan, &c);
        hull::nose(kids, plan, &c);
        hull::hoods(kids, plan, &c);
        hull::cheat_line(kids, plan, &c);
        hull::deck(kids, plan, &c);
        car::car(kids, plan, &c);
        engines::engines(kids, plan, &c);
        tail::tail(kids, plan, &c);
        hull::tail_cap(kids, plan, &c);
        dressing::dress(kids, plan, &c, ctx.ornateness);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // The old airship's zeppelin class, unchanged (#1656 decision 5):
        // 8 m/s^2 of drive on the preset's 0.8 damping tops out at 10 m/s,
        // and a 7 yaw.
        AirshipFeel {
            mass_factor: 1.0,
            drive_accel: 8.0,
            yaw_accel: 7.0,
            linear_damping: 0.8,
            angular_damping: 4.0,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        // Vents, thruster wash and flourishes all issue from under her keel
        // amidships, as they did from under the old gondola; nothing on her
        // burns where a stack could stand.
        [
            0.0,
            -plan.radius(AURA_X) - plan.len(AURA_DROP),
            plan.z(AURA_X),
        ]
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Engine
    }
}
