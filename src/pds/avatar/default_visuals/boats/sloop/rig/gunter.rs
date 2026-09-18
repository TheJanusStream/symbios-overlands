//! The standing gunter: a yard hoisted almost vertically up a SHORT mast, so
//! the mainsail is a bermudan triangle that comes down in two pieces.
//!
//! The yard is the top of the rig, so the mast is under two thirds of the air
//! draft and the boat looks lighter for the same sail plan. The yard overlaps
//! the mast from a third of the way up, which is what a gunter's jaws do.

use crate::pds::generator::Generator;

use super::super::super::profile::HullProfile;
use super::super::super::{BoatColours, dim};
use super::{Head, Rig, Rigging};

pub(super) struct Gunter;

impl Rigging for Gunter {
    fn top_per_loa(&self) -> f32 {
        0.90
    }

    fn truck_of_span(&self) -> f32 {
        0.62
    }

    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        let loa = rig.loa;
        rig.mast_spar(kids, c);
        rig.boom(kids, c);
        // The yard: heel seated on the mast a third of the way up, head at the
        // top of the rig, raked a little aft so it does not read as a second
        // mast.
        let heel_y = rig.heel + (rig.truck - rig.heel) * 0.30;
        let head = Head {
            y: rig.top,
            z: rig.mast_z - loa * 0.030,
            r: dim(loa * 0.0050),
        };
        kids.push(super::line(
            &[
                (rig.jaw(heel_y, 0.0), dim(loa * 0.0068)),
                ([0.0, head.y, head.z], head.r),
            ],
            8,
            c.timber.clone(),
        ));
        rig.bowsprit(kids, hull, c);
        // ONE triangle: the luff runs up the mast and on up the yard, so the
        // head is a point - a node cheaper than the gaff's two panels. The
        // head is put on the YARD rather than straight over the mast, so the
        // luff leans with the spar it is laced to instead of standing in air
        // ahead of it above the masthead.
        rig.main_triangle(kids, c, rig.top - loa * 0.02, head.z, 0.985);
        rig.jib(kids, c);
        rig.standing(kids, hull, c);
        rig.burgee(kids, head, c);
    }
}
