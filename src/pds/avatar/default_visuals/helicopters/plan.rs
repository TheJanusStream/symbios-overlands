//! One pod plan per rotorcraft, and every line and mount read off it.
//!
//! The helicopter's counterpart of the skiff's
//! [`BodyPlan`](super::super::skiffs::BodyPlan) and the boat's
//! `HullProfile`, for the reason every family has one: a single profile that
//! feeds the body, every skin laid on it and every mount hung off it, so
//! nothing she carries can float (#1359). Here it is the POD - the cabin a
//! light helicopter is built round - as `(z, centreline height, radius)`
//! stations nose to rear, its section drawn [`PodProfile::section`] times
//! taller than it is wide.
//!
//! # Two frames, and the one map between them
//!
//! A type is AUTHORED in its prototype's frame: metres as the agreed
//! prototype drew them (#1613), nose at `+Z`, `y = 0` on the skid bottoms.
//! That frame is kept on purpose. The prototype was signed off as a Python
//! writer, and a port that reads `crown(0.05) + 0.20` where the writer reads
//! `crown(0.05) + 0.20` can be checked against it node for node; a port that
//! re-derived every number in true metres could not.
//!
//! [`PodPlan::at`] takes a prototype point to the DRAWN frame: true metres,
//! nose at `+Z`, `y = 0` on the body DATUM, and the craft centred fore and
//! aft. Between the two stand the blueprint's three numbers:
//!
//! * the GIRTH fattens every pod section, mostly UPWARD - the centreline
//!   rises by half the half-height's growth, so the crown rises three times
//!   as far as the belly drops (the prototype's `set_fat`, kept as drawn:
//!   the agreed proportion B was drawn with it). The skids' cross tubes
//!   read the keel off the same sections, so they meet it at any girth;
//! * the BOOM stretches everything aft of [`PodProfile::boom_root`] and
//!   leaves the cabin alone. On the utility the boom, the fin, the
//!   stabiliser and the tail rotor ride its stations, and so, by a
//!   centimetre at most, do the pod's tail cone, the skins, the fairing and
//!   the skid tails that reach past it; on the coaxial it is her whole
//!   tapering tail, nearly two metres of fuselage, and her skids are drawn
//!   rigid off the cabin's rear instead (#1628);
//! * the SCALE is whatever makes the drawn nose-to-boom-end exactly the
//!   blueprint's length.
//!
//! # Why the datum and the centring
//!
//! The visual root is a hidden hub, and the assembler overwrites its
//! translation ([`apply_travel_pose`](super::super::assemble::apply_travel_pose)),
//! so it stands at the drawn frame's origin: on the datum it is buried in
//! the cabin, where a hub on the skid line showed between the skids (#1613).
//! And avian centres a collider on its body, so a box can only hold what is
//! drawn round the body's origin: the craft is centred fore and aft on it,
//! nose to fin, as a skiff is between her bumpers.
//!
//! # A blunt nose is measured
//!
//! The nose is the first station, and for the utility's pointed pod that is
//! where she is drawn. A BLUNT nose is not: the girth raises each station by
//! half its OWN growth, so the trainer's centreline climbs steeply off her
//! small first station and the rings it tilts bulge forward of it - up to
//! 7.5 cm on her fullest bubble (#1621). A type with such a nose reads how
//! far its pod is drawn past its first station off the mesher's own
//! vertices and hands it to [`PodPlan::with_nose_reach`], which carries it
//! into the box's nose and the centring, so her collider holds her glass and
//! her shape is the one the owner agreed.
//!
//! # So is a craft drawn standing
//!
//! A type may stand pitched in her level box - the autogyro sits tail-down
//! on her tail wheel (#1624), every part turned about her main axle. Turned,
//! neither end is where the profile's stations put it, so she measures both
//! off the machine she draws, standing, and hands them to
//! [`PodPlan::with_nose_reach`] and [`PodPlan::with_tail_reach`]: her box
//! and her centring are the standing machine's. The turn is rigid, so the
//! two reaches it measures do not move with the centring they set.
//!
//! A level type whose tail is a part hung past her boom's end measures the
//! same way: the coaxial's tail is her shroud's aft lip, and her nose her
//! fuselage's tip, leaning a hair past its station (#1628). So does one
//! drawn SHORT of it, whose tail reach is negative: the tandem's length runs
//! to the prototype's ramp's end, and her tail light stands 5 to 13 cm short
//! of it (#1630).

use crate::seeded_defaults::HelicopterBlueprint;

/// A type's pod, in its prototype's frame - the one table a rotorcraft
/// type is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PodProfile {
    /// `(z, centreline height, radius)` nose to rear (m, prototype frame,
    /// at girth 1).
    pub(crate) stations: &'static [(f32, f32, f32)],
    /// How much taller the pod's section is than it is wide - the node
    /// scale every sweep over the pod shares, so their sections meet flush.
    pub(crate) section: f32,
    /// The body datum's height over the skid bottoms (m, prototype frame):
    /// where the root's hidden hub sits, inside the cabin.
    pub(crate) datum: f32,
    /// The station aft of which the boom stretch applies (m, prototype
    /// frame): where the boom leaves the pod.
    pub(crate) boom_root: f32,
    /// The boom's end, where the fin stands (m, prototype frame, before the
    /// stretch): the aft end of the length the blueprint names.
    pub(crate) boom_end: f32,
    /// How far aft of the boom's end the craft is drawn (m, prototype
    /// frame): the fin's raked top. Measured, not remembered - the family's
    /// `the_box_holds_the_drawn_fuselage` reads it off the drawn tree. A
    /// type that measures it as she draws ([`PodPlan::with_tail_reach`])
    /// writes zero here.
    pub(crate) tail_reach: f32,
}

/// One rotorcraft's pod at its seeded proportions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PodPlan {
    profile: PodProfile,
    /// True metres per prototype metre.
    scale: f32,
    /// The cabin's girth over the profile's.
    girth: f32,
    /// The boom's reach over the profile's.
    boom: f32,
    /// The craft's fore-and-aft centre (m, prototype frame, stretched).
    z_mid: f32,
    /// How far ahead of its first station the pod is drawn (m, prototype
    /// frame): nothing for a pointed nose, measured for a blunt one
    /// ([`Self::with_nose_reach`]).
    nose_reach: f32,
    /// How far aft of the boom's end the craft is drawn (m, prototype
    /// frame): the profile's, or measured off the craft she draws
    /// ([`Self::with_tail_reach`]) - negative when she is drawn short of it.
    tail_reach: f32,
}

impl PodPlan {
    /// The plan for a seeded machine of this type.
    pub(crate) fn new(bp: &HelicopterBlueprint, profile: PodProfile) -> Self {
        let nose = profile.stations[0].0;
        let mut plan = Self {
            profile,
            scale: 1.0,
            girth: bp.girth,
            boom: bp.boom,
            z_mid: 0.0,
            nose_reach: 0.0,
            tail_reach: profile.tail_reach,
        };
        let boom_end = plan.stretch(profile.boom_end);
        plan.scale = bp.length / (nose - boom_end);
        plan.centre();
        plan
    }

    /// Centre the craft fore and aft: midway between her drawn nose and her
    /// drawn tail.
    fn centre(&mut self) {
        let nose = self.profile.stations[0].0 + self.nose_reach;
        let boom_end = self.stretch(self.profile.boom_end);
        self.z_mid = (nose + boom_end - self.tail_reach) * 0.5;
    }

    /// This plan with its pod drawn `reach` (m, prototype frame) ahead of its
    /// first station - a blunt nose's bulge, measured off the drawn pod by
    /// the type - in the box's nose and the craft's centring. Her length,
    /// her scale and every station stay as they were.
    pub(crate) fn with_nose_reach(mut self, reach: f32) -> Self {
        self.nose_reach = reach.max(0.0);
        self.centre();
        self
    }

    /// This plan with its tail drawn `reach` (m, prototype frame) aft of its
    /// boom's end - measured off the machine the type draws, as she stands -
    /// in the box's tail and the craft's centring, in place of the
    /// profile's. Her length, her scale and every station stay as they were.
    ///
    /// A negative reach is a tail drawn short of the boom's end, and is kept:
    /// the tandem's (#1630). The nose's is floored at zero, because a nose
    /// is drawn at least to its first station.
    pub(crate) fn with_tail_reach(mut self, reach: f32) -> Self {
        self.tail_reach = reach;
        self.centre();
        self
    }

    /// The cabin's girth over the profile's - what the skid track grows by.
    pub(crate) fn girth(&self) -> f32 {
        self.girth
    }

    /// The pod's section scale, for a sweep laid over it.
    pub(crate) fn section(&self) -> f32 {
        self.profile.section
    }

    /// A prototype station `z` after the boom stretch.
    pub(crate) fn stretch(&self, z: f32) -> f32 {
        let root = self.profile.boom_root;
        if z < root {
            root + (z - root) * self.boom
        } else {
            z
        }
    }

    /// The prototype station whose stretch is `drawn` - [`Self::stretch`]
    /// undone: what a part fixed RIGIDLY off a stretched station reads the
    /// pod at, where it is drawn rather than where its own station is.
    pub(crate) fn unstretch(&self, drawn: f32) -> f32 {
        let root = self.profile.boom_root;
        if drawn < root {
            root + (drawn - root) / self.boom
        } else {
            drawn
        }
    }

    /// A prototype point in the drawn frame (m): scaled, the boom stretched,
    /// the datum at `y = 0` and the craft centred fore and aft.
    pub(crate) fn at(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        [
            x * self.scale,
            (y - self.profile.datum) * self.scale,
            (self.stretch(z) - self.z_mid) * self.scale,
        ]
    }

    /// A prototype point `dz` fore (`+`) or aft (`-`) of station `z`, the
    /// offset RIGID: the stretch carries a part fixed to something on a
    /// stretched station along with its mount, rather than stretching the
    /// gap between them - a beacon on a fin, a nav light on an end plate.
    pub(crate) fn at_off(&self, x: f32, y: f32, z: f32, dz: f32) -> [f32; 3] {
        let [x, y, z] = self.at(x, y, z);
        [x, y, z + dz * self.scale]
    }

    /// A prototype length in true metres - a radius, a side, a gap.
    pub(crate) fn len(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// The pod's stations at this girth, nose first: `(z, centreline y,
    /// radius)` in the prototype frame, `z` not yet stretched. Each
    /// section's centreline rises by half its half-height's growth, so the
    /// cabin swells three parts up to one part down (the prototype's
    /// `set_fat`).
    pub(crate) fn stations(&self) -> impl Iterator<Item = (f32, f32, f32)> + '_ {
        let (g, section) = (self.girth, self.profile.section);
        self.profile
            .stations
            .iter()
            .map(move |&(z, y, r)| (z, y + (g - 1.0) * r * section * 0.5, r * g))
    }

    /// `(centreline y, half-width, half-height)` of the pod at prototype
    /// station `z`, linear between stations and held past the ends.
    pub(crate) fn pod_at(&self, z: f32) -> (f32, f32, f32) {
        let pts: Vec<(f32, f32, f32)> = self.stations().collect();
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        let (y, r) = if z >= first.0 {
            (first.1, first.2)
        } else if z <= last.0 {
            (last.1, last.2)
        } else {
            let (a, b) = pts
                .windows(2)
                .map(|w| (w[0], w[1]))
                .find(|(a, b)| a.0 >= z && z >= b.0)
                .expect("a station inside the pod lies between two of them");
            let t = (a.0 - z) / (a.0 - b.0);
            (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
        };
        (y, r, r * self.profile.section)
    }

    /// The pod's top at prototype station `z` (prototype frame).
    pub(crate) fn crown(&self, z: f32) -> f32 {
        let (y, _, h) = self.pod_at(z);
        y + h
    }

    /// The pod's keel at prototype station `z` (prototype frame).
    pub(crate) fn sill(&self, z: f32) -> f32 {
        let (y, _, h) = self.pod_at(z);
        y - h
    }

    /// Half the pod's width at prototype station `z`, at height `y`
    /// (prototype frame): the flank a part standing at that height meets,
    /// held off the very top and bottom of the section, where it closes.
    pub(crate) fn flank(&self, z: f32, y: f32) -> f32 {
        let (yc, r, h) = self.pod_at(z);
        let t = ((y - yc).abs() / h.max(1e-4)).min(0.999);
        r * (1.0 - t * t).max(0.04).sqrt()
    }

    /// The arc-length fraction along the pod's STORED centreline, nose
    /// first, at prototype station `z` - what a sweep's `profile_cut` reads,
    /// so a skin trimmed to `[along(z1), along(z0)]` covers `z0..z1` of the
    /// pod it is laid on. Read off the stations as drawn (stretched, the
    /// section divided out), the prototype's `along`.
    pub(crate) fn along(&self, z: f32) -> f32 {
        let section = self.profile.section;
        let pts: Vec<(f32, f32)> = self
            .stations()
            .map(|(zz, y, _)| (self.stretch(zz), y / section))
            .collect();
        let z = self.stretch(z);
        let seg: Vec<f32> = pts
            .windows(2)
            .map(|w| ((w[0].0 - w[1].0).powi(2) + (w[0].1 - w[1].1).powi(2)).sqrt())
            .collect();
        let total: f32 = seg.iter().sum();
        let mut run = 0.0;
        for (w, d) in pts.windows(2).zip(&seg) {
            if w[0].0 >= z && z >= w[1].0 {
                return (run + d * (w[0].0 - z) / (w[0].0 - w[1].0)) / total;
            }
            run += d;
        }
        if z > pts[0].0 { 0.0 } else { 1.0 }
    }

    /// The pod's own stations as sweep points in the drawn frame, each
    /// radius `proud` metres (prototype) over the pod's: the pod itself at
    /// `0`, a skin laid on it a few millimetres out. The `y` is divided by
    /// the section, which the sweep's node scale multiplies back.
    pub(crate) fn pod_sweep(&self, proud: f32) -> Vec<([f32; 3], f32)> {
        let section = self.profile.section;
        self.stations()
            .map(|(z, y, r)| {
                let [x, y, z] = self.at(0.0, y, z);
                ([x, y / section, z], self.len(r + proud))
            })
            .collect()
    }

    /// The pod's first station (prototype frame): her nose, unless it is
    /// drawn past it ([`Self::with_nose_reach`]).
    pub(crate) fn nose(&self) -> f32 {
        self.profile.stations[0].0
    }

    /// The fuselage's length, nose to boom end (m): the blueprint's own
    /// length, by construction of the scale.
    pub(crate) fn fuselage_len(&self) -> f32 {
        let nose = self.profile.stations[0].0;
        (nose - self.stretch(self.profile.boom_end)) * self.scale
    }

    /// The drawn nose tip and the drawn tail (m, along `z`) - what the box
    /// spans fore and aft.
    pub(crate) fn nose_and_tail(&self) -> (f32, f32) {
        let nose = self.profile.stations[0].0;
        let tail = self.profile.boom_end;
        (
            self.at(0.0, 0.0, nose)[2] + self.len(self.nose_reach),
            self.at(0.0, 0.0, tail)[2] - self.len(self.tail_reach),
        )
    }

    /// How far the datum stands over the skid bottoms (m) - where the
    /// ground is under the drawn frame's origin.
    pub(crate) fn datum_height(&self) -> f32 {
        self.len(self.profile.datum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: PodProfile = PodProfile {
        stations: &[(1.0, 0.5, 0.05), (0.0, 0.5, 0.25), (-0.6, 0.6, 0.1)],
        section: 1.2,
        datum: 0.5,
        boom_root: -0.5,
        boom_end: -1.5,
        tail_reach: 0.1,
    };

    fn plan(length: f32, girth: f32, boom: f32) -> PodPlan {
        PodPlan::new(
            &HelicopterBlueprint {
                length,
                girth,
                boom,
                ..HelicopterBlueprint::NOMINAL
            },
            PROFILE,
        )
    }

    /// The drawn nose to boom end IS the blueprint's length, whatever the
    /// boom does to the proportion - and the craft is centred on the origin
    /// from her nose to her drawn tail.
    #[test]
    fn the_drawn_length_is_the_blueprints_and_the_craft_is_centred() {
        for (length, boom) in [(2.9, 0.92), (3.2, 1.0), (3.5, 1.08)] {
            let p = plan(length, 1.2, boom);
            let nose = p.at(0.0, 0.0, 1.0)[2];
            let end = p.at(0.0, 0.0, -1.5)[2];
            assert!((nose - end - length).abs() < 1e-5, "{length} / {boom}");
            assert!(
                (p.fuselage_len() - length).abs() < 1e-5,
                "{length} / {boom}"
            );
            let (nose, tail) = p.nose_and_tail();
            assert!((nose + tail).abs() < 1e-5, "off centre by {}", nose + tail);
        }
    }

    /// The girth swells every section three parts up to one part down:
    /// the half-height grows by `(g - 1) r section`, the belly drops by half
    /// that and the crown rises by one and a half times it.
    #[test]
    fn the_girth_swells_the_cabin_mostly_upward() {
        let (thin, fat) = (plan(3.2, 1.0, 1.0), plan(3.2, 1.3, 1.0));
        for z in [0.9, 0.3, 0.0, -0.4] {
            let (_, r, _) = thin.pod_at(z);
            let growth = 0.3 * r * PROFILE.section;
            let drop = thin.sill(z) - fat.sill(z);
            let rise = fat.crown(z) - thin.crown(z);
            assert!((drop - 0.5 * growth).abs() < 1e-5, "z {z}: drop {drop}");
            assert!((rise - 1.5 * growth).abs() < 1e-5, "z {z}: rise {rise}");
        }
    }

    /// A measured nose reach moves the box's nose by exactly that much and
    /// keeps the craft centred, nose to tail; with none the plan is the one
    /// `new` makes - the utility's, unchanged.
    #[test]
    fn a_nose_reach_moves_the_nose_and_keeps_her_centred() {
        let p = plan(3.2, 1.2, 1.0);
        let reached = p.with_nose_reach(0.05);
        let (nose, tail) = reached.nose_and_tail();
        assert!((nose + tail).abs() < 1e-5, "off centre by {}", nose + tail);
        let ((n0, t0), (n1, t1)) = (p.nose_and_tail(), reached.nose_and_tail());
        assert!(((n1 - t1) - (n0 - t0) - p.len(0.05)).abs() < 1e-5);
        assert!((reached.fuselage_len() - p.fuselage_len()).abs() < 1e-6);
        let unreached = p.with_nose_reach(0.0);
        assert_eq!(unreached.nose_and_tail(), p.nose_and_tail());
        assert_eq!(unreached.at(0.1, 0.6, -0.4), p.at(0.1, 0.6, -0.4));
    }

    /// A measured tail reach moves the box's tail by exactly that much,
    /// in place of the profile's, and keeps the craft centred.
    #[test]
    fn a_tail_reach_moves_the_tail_and_keeps_her_centred() {
        let p = plan(3.2, 1.2, 1.0);
        let reached = p.with_nose_reach(0.03).with_tail_reach(0.25);
        let (nose, tail) = reached.nose_and_tail();
        assert!((nose + tail).abs() < 1e-5, "off centre by {}", nose + tail);
        let ((n0, t0), (n1, t1)) = (p.nose_and_tail(), reached.nose_and_tail());
        let grown = (n1 - t1) - (n0 - t0);
        assert!((grown - p.len(0.03 + 0.25 - PROFILE.tail_reach)).abs() < 1e-5);
        assert!((reached.fuselage_len() - p.fuselage_len()).abs() < 1e-6);
        let kept = p.with_tail_reach(PROFILE.tail_reach);
        assert_eq!(kept.nose_and_tail(), p.nose_and_tail());
    }

    /// A tail drawn short of the boom's end keeps its negative reach: the
    /// box's tail stands that far AHEAD of the boom's end, and the craft is
    /// still centred (#1630).
    #[test]
    fn a_short_tail_keeps_its_negative_reach() {
        let p = plan(3.2, 1.2, 1.0).with_tail_reach(-0.09);
        let (nose, tail) = p.nose_and_tail();
        assert!((nose + tail).abs() < 1e-5, "off centre by {}", nose + tail);
        let end = p.at(0.0, 0.0, PROFILE.boom_end)[2];
        assert!(
            (tail - end - p.len(0.09)).abs() < 1e-5,
            "{tail} against {end}"
        );
    }

    /// A rigid offset rides its station's stretch and is not stretched.
    #[test]
    fn a_rigid_offset_rides_the_stretch() {
        for boom in [0.92, 1.0, 1.08] {
            let p = plan(3.2, 1.2, boom);
            let anchor = p.at(0.0, 0.9, -1.5)[2];
            let off = p.at_off(0.0, 0.9, -1.5, -0.06)[2];
            assert!((anchor - off - p.len(0.06)).abs() < 1e-6, "boom {boom}");
        }
    }

    /// The boom stretch moves only what is aft of its root, and joins
    /// without a step - and `unstretch` undoes it.
    #[test]
    fn the_boom_stretches_only_the_tail() {
        let p = plan(3.2, 1.2, 1.08);
        assert_eq!(p.stretch(0.3), 0.3);
        assert_eq!(p.stretch(-0.5), -0.5);
        assert!((p.stretch(-1.5) - (-0.5 - 1.08)).abs() < 1e-6);
        for z in [0.3, -0.5, -0.9, -1.5] {
            assert!((p.unstretch(p.stretch(z)) - z).abs() < 1e-6, "{z}");
        }
    }

    /// `along` runs nose 0 to rear 1, and halfway along the stations it is
    /// strictly between.
    #[test]
    fn along_runs_nose_to_rear() {
        let p = plan(3.2, 1.2, 1.0);
        assert_eq!(p.along(1.0), 0.0);
        assert!((p.along(-0.6) - 1.0).abs() < 1e-6);
        let mid = p.along(0.0);
        assert!(mid > 0.3 && mid < 0.8, "{mid}");
    }
}
