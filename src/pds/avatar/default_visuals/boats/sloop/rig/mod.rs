//! The sloop's rigs: five variants over one set of heights, every one of
//! them resolved against the air-draft cap by the RIG'S HIGHEST POINT.
//!
//! # Why the cap is resolved against the top of the rig, not the masthead
//!
//! On the gaff rig the masthead IS the highest point, which is why the rig
//! used to be derived from a `TRUCK_PER_LOA` and nothing else. A gunter's yard
//! and a square topsail's topmast both stand ABOVE the masthead, so a rig that
//! resolved its mast against the cap first and its yard second would sail the
//! yard straight through a gateway lintel - and the cap binds on six of seven
//! surveyed seeds, so that is not a corner case (#1366). Each variant here
//! says how tall its whole rig wants to be ([`Rigging::top_per_loa`]) and
//! where its masthead falls inside that ([`Rigging::truck_of_span`]), and
//! [`Rig::new`] caps the TOP; the masthead is derived from it. A yard over
//! the truck is inside the cap by construction rather than by a constant
//! somebody remembered to subtract.
//!
//! # One node per sail where the mesher allows it
//!
//! A gaff mainsail is TWO cuboids - a cuboid's torture can taper and shear its
//! top edge but cannot TILT it, so a four-sided sail with a peaked head does
//! not exist in one piece. A gunter's or a bermudan's mainsail is a triangle,
//! and a triangle is ONE cuboid tapered to a point. That is why both are
//! cheaper than the rig they were measured against.

mod bermuda;
mod cutter;
mod gaff;
mod gunter;
mod square_topsail;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::seeded_defaults::SloopRig;

use super::super::super::common::{cuboid, id_quat, prim, with_shape};
use super::super::profile::HullProfile;
use super::super::{AIR_DRAFT_CAP, AIR_DRAFT_MARGIN, BoatColours, dim, hover};
use super::hull::line;

/// The gaff rig's highest point - its masthead - as a fraction of the overall
/// length, before the air-draft cap has its say. 0.84 is what the agreed
/// prototype carries at the nominal 2.8 m, and at that size the cap lands on
/// the same number.
const TRUCK_PER_LOA: f32 = 0.84;

/// How far below the spar head it flies from the burgee's centre hangs, as a
/// fraction of the overall length - the flag's top edge just under the truck.
const BURGEE_DROP: f32 = 0.0125;

/// Mainsail and jib offsets to the side of the spars they are set on, so a
/// spar is never buried in its own canvas.
const SAIL_OFFSET: f32 = 0.0107;

/// One rig variant: how tall it wants to be, and how it is drawn over the
/// heights [`Rig::new`] resolves for it.
pub(super) trait Rigging: Sync {
    /// The rig's highest point as a fraction of the overall length, before
    /// the air-draft cap.
    fn top_per_loa(&self) -> f32 {
        TRUCK_PER_LOA
    }

    /// Where the masthead falls in the span from the mast heel to the top of
    /// the rig: 1.0 for a rig whose highest point IS its masthead.
    fn truck_of_span(&self) -> f32 {
        1.0
    }

    /// How far the boom's after end reaches past the gaff rig's, as a
    /// fraction of the overall length, and how far it rises over its length.
    /// A bermudan main of the same area wants a longer foot.
    fn boom_reach(&self) -> (f32, f32) {
        (0.0, 0.007)
    }

    /// Draw the rig.
    fn build(&self, rig: &Rig, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours);
}

/// The drawing for a rig variant - the one match over [`SloopRig`].
pub(super) fn rigging(r: SloopRig) -> &'static dyn Rigging {
    match r {
        SloopRig::Gaff => &gaff::Gaff,
        SloopRig::GaffCutter => &cutter::GaffCutter,
        SloopRig::Gunter => &gunter::Gunter,
        SloopRig::Bermuda => &bermuda::Bermuda,
        SloopRig::SquareTopsail => &square_topsail::SquareTopsail,
    }
}

/// A spar head a flag can fly from: its height, its station and its radius
/// there.
#[derive(Clone, Copy, Debug)]
pub(super) struct Head {
    y: f32,
    z: f32,
    r: f32,
}

/// A rig's heights, all measured from the design waterline and all resolved
/// against the air-draft cap before anything is drawn.
pub(super) struct Rig {
    loa: f32,
    /// Deck height at the mast step, and the mast station.
    heel: f32,
    mast_z: f32,
    /// The rig's highest point - what the cap is checked against.
    top: f32,
    /// Masthead.
    truck: f32,
    /// Where a gaff meets the mast, where its peak ends up, and where the
    /// forestay and shrouds land.
    throat: f32,
    peak_y: f32,
    peak_z: f32,
    hounds: f32,
    /// Boom: its forward end above the waterline, how far it rises, and its
    /// after end.
    gooseneck: f32,
    boom_rise: f32,
    boom_aft: f32,
    /// Bowsprit end, and the height of the jib's tack on it.
    sprit_z: f32,
    tack_y: f32,
}

impl Rig {
    pub(super) fn new(hull: &HullProfile, rigging: &dyn Rigging) -> Self {
        let loa = hull.loa;
        let [_, heel, mast_z] = hull.mast_step();
        // The top of the rig is the proportional height OR what the gateway
        // leaves, whichever is less. The cap is an absolute height above the
        // GROUND and the boat floats a quarter of a draft over it, so a bigger
        // hull gets a relatively shorter rig - which is the honest consequence
        // of sailing a model boat through a 2.86 m lintel.
        let top = (loa * rigging.top_per_loa())
            .min(AIR_DRAFT_CAP - hover(hull.draft) - AIR_DRAFT_MARGIN)
            .max(heel + loa * 0.25);
        let truck = heel + (top - heel) * rigging.truck_of_span();
        let span = truck - heel;
        let (reach, boom_rise) = rigging.boom_reach();
        let [_, stem_y, _] = hull.bow_fitting();
        Self {
            loa,
            heel,
            mast_z,
            top,
            truck,
            throat: heel + span * 0.585,
            peak_y: heel + span * 0.936,
            peak_z: -0.143 * loa,
            hounds: heel + span * 0.895,
            gooseneck: heel + loa * 0.065,
            boom_rise: loa * boom_rise,
            boom_aft: -(0.386 + reach) * loa,
            sprit_z: hull.stem_z() + loa * 0.143,
            tack_y: stem_y + loa * 0.011,
        }
    }

    /// The rig's highest point above the design waterline (m).
    #[cfg(test)]
    pub(super) fn top(&self) -> f32 {
        self.top
    }

    /// The mast's three stations, heel to truck, tapering as it goes.
    ///
    /// A method rather than three literals inside a builder because the spars
    /// that hang off it need to know how thick it is where they meet it, and
    /// a second copy of these numbers is exactly how a gaff ends up clear of
    /// the mast it is supposed to hang from (#1366).
    fn mast(&self) -> [([f32; 3], f32); 3] {
        let loa = self.loa;
        [
            (
                [0.0, self.heel - loa * 0.007, self.mast_z],
                dim(loa * 0.0107),
            ),
            ([0.0, self.throat, self.mast_z], dim(loa * 0.0086)),
            ([0.0, self.truck, self.mast_z], dim(loa * 0.0057)),
        ]
    }

    /// The mast's radius at height `y`, straight off [`Self::mast`]'s own
    /// stations. Linear between them, which is what an embed depth wants: the
    /// question is how deep to bury a jaw, not where a surface is.
    fn mast_radius_at(&self, y: f32) -> f32 {
        let m = self.mast();
        for w in m.windows(2) {
            let (([_, y0, _], r0), ([_, y1, _], r1)) = (w[0], w[1]);
            if y <= y1 {
                let t = ((y - y0) / (y1 - y0).max(1e-6)).clamp(0.0, 1.0);
                return r0 + (r1 - r0) * t;
            }
        }
        m[2].1
    }

    /// Where a spar that meets the mast at height `y` seats its inboard end:
    /// half a mast radius abaft the mast's axis, at the mast's thickness
    /// THERE. A gaff's jaws embrace the mast and a gooseneck is a fitting on
    /// it; written as a constant offset the gaff cleared the mast at the
    /// nominal size and the boat came apart at the throat (#1366).
    fn jaw(&self, y: f32, rise: f32) -> [f32; 3] {
        [0.0, y + rise, self.mast_z - self.mast_radius_at(y) * 0.5]
    }

    /// The masthead as a spar head.
    fn masthead(&self) -> Head {
        Head {
            y: self.truck,
            z: self.mast_z,
            r: self.mast_radius_at(self.truck),
        }
    }

    /// The boom's two ends.
    fn boom_line(&self) -> [([f32; 3], f32); 2] {
        let loa = self.loa;
        [
            (self.jaw(self.gooseneck, -loa * 0.011), dim(loa * 0.0068)),
            (
                [0.0, self.gooseneck + self.boom_rise, self.boom_aft],
                dim(loa * 0.0082),
            ),
        ]
    }

    /// The boom's centreline height at station `z` (m) - what a boom tent is
    /// hung from.
    pub(super) fn boom_y_at(&self, z: f32) -> f32 {
        let [([_, y0, z0], _), ([_, y1, z1], _)] = self.boom_line();
        let t = ((z - z0) / (z1 - z0)).clamp(0.0, 1.0);
        y0 + (y1 - y0) * t
    }

    // --- the parts every rig shares ----------------------------------------

    fn mast_spar(&self, kids: &mut Vec<Generator>, c: &BoatColours) {
        kids.push(line(&self.mast(), 10, c.timber.clone()));
    }

    fn boom(&self, kids: &mut Vec<Generator>, c: &BoatColours) {
        kids.push(line(&self.boom_line(), 8, c.timber.clone()));
    }

    /// Bowsprit: slim, and it has a job - it carries the forestay and the
    /// jib's tack. The retired boat's was 5 cm thick on a 1.3 m hull and read
    /// as a tank gun.
    fn bowsprit(&self, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        let loa = self.loa;
        let [_, stem_y, _] = hull.bow_fitting();
        kids.push(line(
            &[
                ([0.0, stem_y - loa * 0.025, 0.42 * loa], dim(loa * 0.0071)),
                ([0.0, self.tack_y, self.sprit_z], dim(loa * 0.0046)),
            ],
            8,
            c.timber.clone(),
        ));
    }

    /// Forestay to the bowsprit end and a shroud a side. NO backstay: a gaff
    /// peak swings through where one would be, which is why a gaff boat
    /// carries runners instead, and the shorter rigs keep the same plan.
    fn standing(&self, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        let masthead = [0.0, self.hounds, self.mast_z];
        let chain = |side: f32| hull.deck_edge(0.09, side * 0.93);
        for foot in [[0.0, self.tack_y, self.sprit_z], chain(1.0), chain(-1.0)] {
            self.stay(kids, masthead, foot, c);
        }
    }

    fn stay(&self, kids: &mut Vec<Generator>, from: [f32; 3], to: [f32; 3], c: &BoatColours) {
        let r = dim(self.loa * 0.004);
        kids.push(line(&[(from, r), (to, r)], 5, c.rigging.clone()));
    }

    /// One sail: a thin cuboid tapered, sheared and cambered into shape.
    #[allow(clippy::too_many_arguments)]
    fn sail(
        kids: &mut Vec<Generator>,
        cloth: &SovereignMaterialSettings,
        size: [f32; 3],
        at: [f32; 3],
        taper: [f32; 2],
        bend: [f32; 3],
        shear: [f32; 2],
    ) {
        kids.push(prim(
            with_shape(cuboid(size.map(dim), cloth.clone()), taper, bend, shear),
            at,
            id_quat(),
        ));
    }

    /// A fore-and-aft sail whose foot runs from the mast to the boom's end:
    /// `height` tall, its top `taper`ed and its head's forward edge put at
    /// `head_z`.
    fn main_triangle(
        &self,
        kids: &mut Vec<Generator>,
        c: &BoatColours,
        head_y: f32,
        head_z: f32,
        taper: f32,
    ) {
        let loa = self.loa;
        let foot = self.mast_z - self.boom_aft;
        let body_z = (self.mast_z + self.boom_aft) * 0.5;
        // The head's forward edge is its centre plus half what the taper
        // leaves of the foot, so the shear that puts it at `head_z` is that
        // difference. With `head_z` on the mast the luff stands vertical.
        let shear = head_z - foot * (1.0 - taper) * 0.5 - body_z;
        Self::sail(
            kids,
            &c.canvas,
            [loa * 0.0043, head_y - self.gooseneck, foot],
            [loa * SAIL_OFFSET, (head_y + self.gooseneck) * 0.5, body_z],
            [0.0, taper],
            [0.08, 0.0, 0.0],
            [0.0, shear],
        );
    }

    /// The gaff mainsail and the gaff it hangs from.
    ///
    /// Two cuboids, and that is a mesher fact rather than a choice: the body
    /// carries a horizontal head at the throat and the peak panel sits on it,
    /// its forward edge running up the gaff.
    fn gaff_main(&self, kids: &mut Vec<Generator>, c: &BoatColours) {
        let loa = self.loa;
        kids.push(line(
            &[
                (self.jaw(self.throat, loa * 0.010), dim(loa * 0.0068)),
                (
                    [0.0, self.peak_y + loa * 0.009, self.peak_z],
                    dim(loa * 0.0054),
                ),
            ],
            8,
            c.timber.clone(),
        ));
        let x = loa * SAIL_OFFSET;
        let foot = self.mast_z - self.boom_aft;
        let head = self.mast_z - self.peak_z - loa * 0.054;
        let taper = 1.0 - head / foot;
        let body_z = (self.mast_z + self.boom_aft) * 0.5;
        Self::sail(
            kids,
            &c.canvas,
            [loa * 0.0043, self.throat - self.gooseneck, foot],
            [x, (self.throat + self.gooseneck) * 0.5, body_z],
            [0.0, taper],
            [0.09, 0.0, 0.0],
            [0.0, foot * 0.5 * taper],
        );
        let head_z = body_z + foot * 0.5 * taper;
        Self::sail(
            kids,
            &c.canvas,
            [loa * 0.0039, self.peak_y - self.throat, head],
            [x, (self.peak_y + self.throat) * 0.5, head_z],
            [0.0, 0.97],
            [0.07, 0.0, 0.0],
            [0.0, self.peak_z - head_z],
        );
    }

    /// The jib, tack at the bowsprit end and head at the hounds. The boat's
    /// third identity slot: the one sail small enough to be trim rather than
    /// mass, dyed in the seeded accent (#1365).
    fn jib(&self, kids: &mut Vec<Generator>, c: &BoatColours) {
        let loa = self.loa;
        let clew_z = 0.29 * loa;
        let foot = self.sprit_z - clew_z;
        let mid_z = (self.sprit_z + clew_z) * 0.5;
        Self::sail(
            kids,
            &c.jib,
            [loa * 0.0039, self.hounds - self.tack_y, foot],
            [loa * SAIL_OFFSET, (self.hounds + self.tack_y) * 0.5, mid_z],
            [0.0, 0.97],
            [0.10, 0.0, 0.0],
            [0.0, self.mast_z + loa * 0.018 - mid_z],
        );
    }

    /// The burgee, flying from the highest spar head this rig has.
    ///
    /// SEATED half a spar radius inside that spar - the rule [`Self::jaw`]
    /// takes for the boom and the gaff. It used to hang at `mast_z - loa *
    /// 0.0357`, a restated constant: that overlapped the gaff rig's mast by
    /// under a millimetre, and on a gunter, whose yard head is thinner and
    /// stands further aft, it left the flag floating clear (#1366 defect 4).
    /// The highest thing on the boat and one of her identity slots, so it is
    /// the seeded accent (#1365).
    fn burgee(&self, kids: &mut Vec<Generator>, head: Head, c: &BoatColours) {
        let loa = self.loa;
        let fly = dim(loa * 0.0607);
        kids.push(prim(
            cuboid(
                [dim(loa * 0.0043), dim(loa * 0.0196), fly],
                c.pennant.clone(),
            ),
            [
                0.0,
                head.y - loa * BURGEE_DROP,
                head.z - fly * 0.5 + head.r * 0.5,
            ],
            id_quat(),
        ));
    }
}
