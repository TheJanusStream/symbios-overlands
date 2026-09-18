//! The stumpy bermudan: one triangle on one spar, the whole rig on the mast.
//!
//! The rig for the Sleek stance. Nothing crosses the mast, so the silhouette
//! is a single sweep, and the mast can take the whole air draft because it IS
//! the whole air draft. The price is that a bermudan main of the same area
//! wants a longer foot, so the boom reaches past the gaff rig's, the way a
//! real stumpy rig's does.

use crate::pds::generator::Generator;

use super::super::super::BoatColours;
use super::super::super::profile::HullProfile;
use super::{Rig, Rigging};

pub(super) struct Bermuda;

impl Rigging for Bermuda {
    fn top_per_loa(&self) -> f32 {
        0.93
    }

    fn boom_reach(&self) -> (f32, f32) {
        (0.045, 0.012)
    }

    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        rig.mast_spar(kids, c);
        rig.boom(kids, c);
        rig.bowsprit(kids, hull, c);
        // The head a little under the truck, the luff straight up the mast.
        rig.main_triangle(kids, c, rig.truck - rig.loa * 0.03, rig.mast_z, 0.985);
        rig.jib(kids, c);
        rig.standing(kids, hull, c);
        rig.burgee(kids, rig.masthead(), c);
    }
}
