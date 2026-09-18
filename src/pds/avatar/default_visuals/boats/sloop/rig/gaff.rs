//! The gaff rig - a gaff main and a single jib on a bowsprit. The rig the
//! sloop was agreed on in #1363, and the floor every theme can reach.

use crate::pds::generator::Generator;

use super::super::super::BoatColours;
use super::super::super::profile::HullProfile;
use super::{Rig, Rigging};

pub(super) struct Gaff;

impl Rigging for Gaff {
    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        rig.mast_spar(kids, c);
        rig.boom(kids, c);
        rig.bowsprit(kids, hull, c);
        rig.gaff_main(kids, c);
        rig.jib(kids, c);
        rig.standing(kids, hull, c);
        rig.burgee(kids, rig.masthead(), c);
    }
}
