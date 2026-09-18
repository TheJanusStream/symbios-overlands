//! The gaff cutter: the gaff rig's spars with two headsails.
//!
//! The outer jib stays on the bowsprit end and a staysail is set inboard on
//! its own stay to the stemhead. That is a real mass in the foretriangle
//! rather than a fitting - the gap between the mast and the jib is the
//! emptiest part of the gaff rig's silhouette.

use crate::pds::generator::Generator;

use super::super::super::profile::HullProfile;
use super::super::super::{BoatColours, dim};
use super::{Rig, Rigging, SAIL_OFFSET, gaff::Gaff};

pub(super) struct GaffCutter;

impl Rigging for GaffCutter {
    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        Gaff.build(rig, kids, hull, c);
        let loa = rig.loa;
        // The inner forestay, hounds to stemhead - what the staysail hangs on.
        let [_, stem_y, stem_z] = hull.bow_fitting();
        let tack = [0.0, stem_y + loa * 0.010, stem_z - loa * 0.010];
        rig.stay(kids, [0.0, rig.hounds, rig.mast_z], tack, c);
        // The staysail, set on the OTHER side of the mast from the jib, as a
        // real staysail sheeted to the opposite quarter would be: two
        // triangles on one side merge into one at 109 px/m. The cloth is the
        // mainsail's - only the jib carries the seeded accent.
        let clew_z = 0.115 * loa;
        let foot = tack[2] - clew_z;
        let mid_z = (tack[2] + clew_z) * 0.5;
        Rig::sail(
            kids,
            &c.canvas,
            [dim(loa * 0.0039), rig.hounds - tack[1], foot],
            [-loa * SAIL_OFFSET, (rig.hounds + tack[1]) * 0.5, mid_z],
            [0.0, 0.97],
            [0.10, 0.0, 0.0],
            [0.0, rig.mast_z + loa * 0.012 - mid_z],
        );
    }
}
