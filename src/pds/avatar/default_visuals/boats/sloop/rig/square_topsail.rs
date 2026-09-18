//! A gaff main with a square topsail crossed above it - the historic themes'
//! rig, and the only one whose highest point is a spar other than the mast.
//!
//! A real gaff boat carrying a square topsail carries a TOPMAST fidded above
//! her lower mast, and the yard crosses that. Without it the yard crosses thin
//! air a third of a metre over the masthead - the prototype's first draw did
//! exactly that (#1366 defect 5). The topmast truck is the top of this rig and
//! the lower mast comes down to four fifths of the span to pay for it.

use crate::pds::generator::Generator;

use super::super::super::profile::HullProfile;
use super::super::super::{BoatColours, dim};
use super::{Head, Rig, Rigging};

pub(super) struct SquareTopsail;

/// How far under the topmast truck the yard is crossed, as a fraction of the
/// overall length - a burgee's height, so the flag flies clear above it.
const YARD_UNDER_TRUCK: f32 = 0.032;

impl Rigging for SquareTopsail {
    fn top_per_loa(&self) -> f32 {
        0.92
    }

    fn truck_of_span(&self) -> f32 {
        0.80
    }

    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        let loa = rig.loa;
        // Everything the gaff rig carries, except that the burgee flies from
        // the topmast rather than the lower masthead.
        rig.mast_spar(kids, c);
        rig.boom(kids, c);
        rig.bowsprit(kids, hull, c);
        rig.gaff_main(kids, c);
        rig.jib(kids, c);
        rig.standing(kids, hull, c);
        // The topmast, fidded over the lower masthead and standing to the top
        // of the rig.
        let topmast = Head {
            y: rig.top,
            z: rig.mast_z,
            r: dim(loa * 0.0042),
        };
        kids.push(super::line(
            &[
                (
                    [0.0, rig.truck - loa * 0.030, rig.mast_z],
                    dim(loa * 0.0055),
                ),
                ([0.0, topmast.y, topmast.z], topmast.r),
            ],
            8,
            c.timber.clone(),
        ));
        // The yard, athwartships, a touch shorter than the beam so it stays
        // inside the hull's silhouette.
        let yard_y = rig.top - loa * YARD_UNDER_TRUCK;
        let yard_r = dim(loa * 0.0062);
        let half = hull.half_beam * 0.86;
        kids.push(super::line(
            &[
                ([-half, yard_y - loa * 0.004, rig.mast_z], dim(loa * 0.0050)),
                ([0.0, yard_y, rig.mast_z], yard_r),
                ([half, yard_y - loa * 0.004, rig.mast_z], dim(loa * 0.0050)),
            ],
            8,
            c.timber.clone(),
        ));
        // The square sail, bent to the yard: its head on the yard's centre
        // and its plane half a yard radius abaft it - the jaw rule again - and
        // hanging to just over the gaff's throat. Cambered, not tapered: a
        // taper on this box would act on its thickness.
        let thick = dim(loa * 0.0043);
        let drop = yard_y - rig.throat - loa * 0.04;
        Rig::sail(
            kids,
            &c.canvas,
            [half * 2.0 * 0.96, drop, thick],
            [
                0.0,
                yard_y - drop * 0.5,
                rig.mast_z - yard_r * 0.5 - thick * 0.5,
            ],
            [0.0, 0.0],
            [0.0, 0.0, 0.05],
            [0.0, 0.0],
        );
        rig.burgee(kids, topmast, c);
    }
}
