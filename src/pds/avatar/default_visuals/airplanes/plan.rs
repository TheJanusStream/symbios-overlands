//! One fuselage plan per airplane, and every line and mount read off it.
//!
//! The airplane's counterpart of the helicopter's
//! [`PodPlan`](super::super::helicopters::PodPlan), for the same reason
//! every family has one: a single profile that feeds the body, every skin
//! laid on it and every mount hung off it, so nothing she carries can float
//! (#1359). Here it is the FUSELAGE, as `(z, centreline height, radius)`
//! stations spinner to tail post, its section drawn [`FuseProfile::section`]
//! times taller than it is wide.
//!
//! # Two frames, and the one map between them
//!
//! A type is AUTHORED in its prototype's frame: metres as the agreed
//! prototype drew them (#1616), nose at `+Z`, `y = 0` on the main wheels'
//! bottoms with the fuselage level - her attitude in flight. That frame is
//! kept on purpose, as the helicopter's is: the prototype was signed off as
//! a Python writer (`target/dump/aircraft-p5/plane.py`), and a port that
//! reads its numbers as it does can be checked against it node for node.
//!
//! [`FusePlan::at`] takes a prototype point to the DRAWN frame: true metres,
//! nose at `+Z`, and the origin at the middle of her box - her collider - so
//! the assembler hangs her with no drop at all. Between the two stand the
//! blueprint's three numbers:
//!
//! * the GIRTH fattens every section UPWARD off its belly - the centreline
//!   rises by the half-height's whole growth (the prototype's `set_fat`) -
//!   so the gear legs, the struts and the box's floor, which are read off
//!   the belly, meet it at any girth;
//! * the CABIN stretches the run between [`FuseProfile::firewall`] and
//!   [`FuseProfile::cabin_rear`] - the windows ride it - and carries
//!   everything aft of it rigidly aft, the tail with it; the wing keeps its
//!   chord ([`FusePlan::at_off`]);
//! * the SCALE is whatever makes the drawn spinner-to-tail-post exactly the
//!   blueprint's length.
//!
//! # Her box
//!
//! Her fuselage, owner decision 3 of #1611 as #1616 recommendation 4 drew
//! it: spinner to rudder, as wide as her widest section - or as her cowl
//! ring, where a radial's stands proud of it ([`FuseProfile::cowl_ring`],
//! #1623) - and from [`FuseProfile::box_floor_rise`] over her belly to her
//! cabin roof. The floor stands that far up so that, sitting tail-down on
//! her three wheels (#1616 recommendation 3), the box's tail corner clears
//! the ground: floored at her belly it stood 3 cm into it, ahead of her tail
//! wheel. Her wings, tail surfaces, gear and airscrew stand outside it - a
//! biplane's upper wing over it as the high-wing's wing stands on her roof.

use crate::seeded_defaults::AirplaneBlueprint;

/// A type's fuselage, in its prototype's frame - the one table an airplane
/// type is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FuseProfile {
    /// `(z, centreline height, radius)` spinner to tail post (m, prototype
    /// frame, at girth 1).
    pub(crate) stations: &'static [(f32, f32, f32)],
    /// How much taller the section is than it is wide - the node scale every
    /// sweep over the fuselage shares, so their sections meet flush.
    pub(crate) section: f32,
    /// The firewall's station and the cabin's rear (m, prototype frame): the
    /// run the cabin stretch lengthens.
    pub(crate) firewall: f32,
    pub(crate) cabin_rear: f32,
    /// The spinner's tip and the rudder's trailing edge (m, prototype
    /// frame): the airframe the blueprint's length names, and her box's two
    /// ends.
    pub(crate) nose: f32,
    pub(crate) tail: f32,
    /// The station whose belly and roof her box spans, the cabin's deepest,
    /// and how far over that belly the box's floor stands (m, prototype
    /// frame).
    pub(crate) box_station: f32,
    pub(crate) box_floor_rise: f32,
    /// The radius of a cowl ring standing proud of the fuselage at girth 1
    /// (m, prototype frame) - a radial's, open at its middle so the
    /// cylinders show - which her box's width holds; `0.0` where the cowl is
    /// the fuselage's own nose.
    pub(crate) cowl_ring: f32,
}

/// One airplane's fuselage at its seeded proportions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FusePlan {
    profile: FuseProfile,
    /// True metres per prototype metre.
    scale: f32,
    /// The fuselage's girth over the profile's.
    girth: f32,
    /// The cabin's run over the profile's.
    cabin: f32,
    /// The box's middle (m, prototype frame, stretched): her drawn origin.
    y_mid: f32,
    z_mid: f32,
}

impl FusePlan {
    /// The plan for a seeded machine of this type.
    pub(crate) fn new(bp: &AirplaneBlueprint, profile: FuseProfile) -> Self {
        let mut plan = Self {
            profile,
            scale: 1.0,
            girth: bp.girth,
            cabin: bp.cabin,
            y_mid: 0.0,
            z_mid: 0.0,
        };
        let (nose, tail) = (plan.stretch(profile.nose), plan.stretch(profile.tail));
        plan.scale = bp.length / (nose - tail);
        plan.z_mid = (nose + tail) * 0.5;
        plan.y_mid = (plan.box_floor() + plan.box_roof()) * 0.5;
        plan
    }

    /// The fuselage's girth over the profile's - what the spinner and the
    /// cylinder heads grow by.
    pub(crate) fn girth(&self) -> f32 {
        self.girth
    }

    /// The fuselage's section scale, for a sweep laid over it.
    pub(crate) fn section(&self) -> f32 {
        self.profile.section
    }

    /// A prototype station `z` after the cabin stretch: the cowl as drawn,
    /// the cabin's run lengthened, and everything aft of it carried aft by
    /// what the run gained.
    pub(crate) fn stretch(&self, z: f32) -> f32 {
        let (front, rear) = (self.profile.firewall, self.profile.cabin_rear);
        if z >= front {
            z
        } else if z >= rear {
            front - (front - z) * self.cabin
        } else {
            z - (self.cabin - 1.0) * (front - rear)
        }
    }

    /// The prototype station `z` whose stretch is `drawn` - [`Self::stretch`]
    /// undone: what a part fixed RIGIDLY off a station reads the fuselage
    /// at, where it is drawn rather than where its own station is.
    pub(crate) fn unstretch(&self, drawn: f32) -> f32 {
        let (front, rear) = (self.profile.firewall, self.profile.cabin_rear);
        let rear_drawn = front - (front - rear) * self.cabin;
        if drawn >= front {
            drawn
        } else if drawn >= rear_drawn {
            front - (front - drawn) / self.cabin
        } else {
            drawn + (self.cabin - 1.0) * (front - rear)
        }
    }

    /// A prototype point in the drawn frame (m): scaled, the cabin
    /// stretched, the box's middle at the origin.
    pub(crate) fn at(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        [
            x * self.scale,
            (y - self.y_mid) * self.scale,
            (self.stretch(z) - self.z_mid) * self.scale,
        ]
    }

    /// A prototype point `dz` fore (`+`) or aft (`-`) of station `z`, the
    /// offset RIGID: a part fixed to something on a stretched station - a
    /// wing on the cabin roof - keeps its own length, rather than stretching
    /// with the run.
    pub(crate) fn at_off(&self, x: f32, y: f32, z: f32, dz: f32) -> [f32; 3] {
        let [x, y, z] = self.at(x, y, z);
        [x, y, z + dz * self.scale]
    }

    /// A prototype length in true metres - a radius, a side, a gap.
    pub(crate) fn len(&self, v: f32) -> f32 {
        v * self.scale
    }

    /// The fuselage's stations at this girth, spinner first: `(z,
    /// centreline y, radius)` in the prototype frame, `z` not yet
    /// stretched. Each section's centreline rises by its half-height's
    /// whole growth, so the belly stays where it was (the prototype's
    /// `set_fat`).
    pub(crate) fn stations(&self) -> impl Iterator<Item = (f32, f32, f32)> + '_ {
        let (g, section) = (self.girth, self.profile.section);
        self.profile
            .stations
            .iter()
            .map(move |&(z, y, r)| (z, y + (g - 1.0) * r * section, r * g))
    }

    /// `(centreline y, half-width, half-height)` of the fuselage at
    /// prototype station `z`, linear between stations and held past the
    /// ends - the prototype's `fuse_at`.
    pub(crate) fn fuse_at(&self, z: f32) -> (f32, f32, f32) {
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
                .expect("a station inside the fuselage lies between two of them");
            let t = (a.0 - z) / (a.0 - b.0);
            (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
        };
        (y, r, r * self.profile.section)
    }

    /// The fuselage's top at prototype station `z` (prototype frame).
    pub(crate) fn crown(&self, z: f32) -> f32 {
        let (y, _, h) = self.fuse_at(z);
        y + h
    }

    /// The fuselage's belly at prototype station `z` (prototype frame).
    pub(crate) fn sill(&self, z: f32) -> f32 {
        let (y, _, h) = self.fuse_at(z);
        y - h
    }

    /// Where the fuselage's flank stands at prototype station `z` and height
    /// `y` - its half-width there, off the section's ellipse (the
    /// prototype's `flank`).
    pub(crate) fn flank(&self, z: f32, y: f32) -> f32 {
        let (yc, r, h) = self.fuse_at(z);
        let t = ((y - yc).abs() / h.max(1e-4)).min(0.999);
        r * (1.0 - t * t).max(0.04).sqrt()
    }

    /// The arc-length fraction along the fuselage's STORED centreline,
    /// spinner first, at prototype station `z` - what a sweep's
    /// `profile_cut` reads, so a skin trimmed to `[along(z1), along(z0)]`
    /// covers `z0..z1` of the fuselage it is laid on. Read off the stations
    /// as drawn (stretched, the section divided out), the prototype's
    /// `along`.
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

    /// The fuselage's own stations as sweep points in the drawn frame, each
    /// radius `proud` metres (prototype) over the fuselage's: the fuselage
    /// itself at `0`, a skin laid on it a few millimetres out. The `y` is
    /// divided by the section, which the sweep's node scale multiplies back.
    pub(crate) fn fuse_sweep(&self, proud: f32) -> Vec<([f32; 3], f32)> {
        let section = self.profile.section;
        self.stations()
            .map(|(z, y, r)| {
                let [x, y, z] = self.at(0.0, y, z);
                ([x, y / section, z], self.len(r + proud))
            })
            .collect()
    }

    /// The box's floor and its roof (prototype frame): over the cabin's
    /// deepest belly by the profile's rise, and its roof.
    pub(crate) fn box_floor(&self) -> f32 {
        self.sill(self.profile.box_station) + self.profile.box_floor_rise
    }

    pub(crate) fn box_roof(&self) -> f32 {
        self.crown(self.profile.box_station)
    }

    /// Half-extents (m) of her collider: as wide as her widest section or
    /// her cowl ring, floor to cabin roof, spinner to rudder - centred on
    /// the drawn origin by construction.
    pub(crate) fn half_extents(&self) -> [f32; 3] {
        let widest = self
            .stations()
            .map(|(_, _, r)| r)
            .fold(self.profile.cowl_ring * self.girth, f32::max);
        let (nose, tail) = (
            self.stretch(self.profile.nose),
            self.stretch(self.profile.tail),
        );
        [
            self.len(widest),
            self.len(self.box_roof() - self.box_floor()) * 0.5,
            self.len(nose - tail) * 0.5,
        ]
    }

    /// Spinner tip to rudder (m): the blueprint's own length, by
    /// construction of the scale.
    pub(crate) fn airframe_len(&self) -> f32 {
        let (nose, tail) = (
            self.stretch(self.profile.nose),
            self.stretch(self.profile.tail),
        );
        (nose - tail) * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: FuseProfile = FuseProfile {
        stations: &[
            (1.5, 0.7, 0.1),
            (1.0, 0.7, 0.22),
            (0.4, 0.7, 0.23),
            (0.0, 0.72, 0.2),
            (-1.5, 0.8, 0.05),
        ],
        section: 1.35,
        firewall: 1.0,
        cabin_rear: 0.0,
        nose: 1.6,
        tail: -1.55,
        box_station: 0.4,
        box_floor_rise: 0.08,
        cowl_ring: 0.0,
    };

    fn plan(length: f32, girth: f32, cabin: f32) -> FusePlan {
        FusePlan::new(
            &AirplaneBlueprint {
                length,
                girth,
                cabin,
                ..AirplaneBlueprint::NOMINAL
            },
            PROFILE,
        )
    }

    /// The drawn spinner to rudder IS the blueprint's length, whatever
    /// the cabin does - and her box is centred on the drawn origin, fore and
    /// aft and floor to roof.
    #[test]
    fn the_drawn_length_is_the_blueprints_and_the_box_is_centred() {
        for (length, cabin) in [(2.9, 0.9), (3.19, 1.0), (3.5, 1.1)] {
            let p = plan(length, 1.18, cabin);
            let nose = p.at(0.0, 0.0, PROFILE.nose)[2];
            let tail = p.at(0.0, 0.0, PROFILE.tail)[2];
            assert!((nose - tail - length).abs() < 1e-5, "{length} / {cabin}");
            assert!((p.airframe_len() - length).abs() < 1e-5);
            assert!((nose + tail).abs() < 1e-5, "off centre by {}", nose + tail);
            let floor = p.at(0.0, p.box_floor(), 0.0)[1];
            let roof = p.at(0.0, p.box_roof(), 0.0)[1];
            assert!((floor + roof).abs() < 1e-5, "floor {floor}, roof {roof}");
            let [_, hy, hz] = p.half_extents();
            assert!((roof - hy).abs() < 1e-5 && (hz - length * 0.5).abs() < 1e-5);
        }
    }

    /// The girth swells every section upward off its belly: the belly stays
    /// put and the crown rises by the half-height's growth twice over.
    #[test]
    fn the_girth_swells_the_fuselage_off_its_belly() {
        let (thin, fat) = (plan(3.19, 1.0, 1.0), plan(3.19, 1.3, 1.0));
        for z in [1.2, 0.4, 0.0, -0.8] {
            let (_, r, _) = thin.fuse_at(z);
            let growth = 0.3 * r * PROFILE.section;
            assert!(
                (thin.sill(z) - fat.sill(z)).abs() < 1e-5,
                "z {z}: the belly moved"
            );
            let rise = fat.crown(z) - thin.crown(z);
            assert!((rise - 2.0 * growth).abs() < 1e-5, "z {z}: rise {rise}");
        }
    }

    /// The cabin stretch lengthens only the cabin's run, leaves the cowl as
    /// drawn and carries the tail rigidly aft - so a part fixed to the tail
    /// keeps its own size.
    #[test]
    fn the_cabin_stretches_only_its_run() {
        let p = plan(3.19, 1.18, 1.1);
        assert_eq!(p.stretch(1.3), 1.3);
        assert!((p.stretch(0.0) - (1.0 - 1.1)).abs() < 1e-6);
        let span = |a: f32, b: f32| p.stretch(a) - p.stretch(b);
        assert!((span(-0.5, -1.5) - 1.0).abs() < 1e-6, "the tail stretched");
        assert!((span(1.0, 0.0) - 1.1).abs() < 1e-6, "the cabin did not");
        let rigid = p.at_off(0.0, 1.0, 0.82, -0.35)[2] - p.at(0.0, 1.0, 0.82)[2];
        assert!((rigid + p.len(0.35)).abs() < 1e-6);
    }

    /// `unstretch` undoes `stretch` at every station, fore of the cabin, in
    /// it and aft of it, at every cabin factor.
    #[test]
    fn unstretch_undoes_the_cabin_stretch() {
        for cabin in [0.9, 1.0, 1.1] {
            let p = plan(3.19, 1.18, cabin);
            for z in [1.3, 1.0, 0.6, 0.0, -0.7, -1.5] {
                let back = p.unstretch(p.stretch(z));
                assert!(
                    (back - z).abs() < 1e-5,
                    "cabin {cabin}: {z} came back {back}"
                );
            }
        }
    }

    /// A cowl ring wider than the fuselage widens the box to it, and only
    /// the box: the fuselage's own stations are untouched.
    #[test]
    fn the_box_holds_a_cowl_ring_that_stands_proud() {
        let plain = plan(3.19, 1.18, 1.0);
        let ringed = FusePlan::new(
            &AirplaneBlueprint::NOMINAL,
            FuseProfile {
                cowl_ring: 0.3,
                ..PROFILE
            },
        );
        assert!((ringed.half_extents()[0] - ringed.len(0.3 * 1.18)).abs() < 1e-6);
        assert!(ringed.half_extents()[0] > plain.half_extents()[0]);
        assert_eq!(ringed.half_extents()[1], plain.half_extents()[1]);
        let narrow = FusePlan::new(
            &AirplaneBlueprint::NOMINAL,
            FuseProfile {
                cowl_ring: 0.1,
                ..PROFILE
            },
        );
        assert_eq!(narrow.half_extents(), plain.half_extents());
    }

    /// `along` runs spinner 0 to tail 1, and strictly between in the middle.
    #[test]
    fn along_runs_nose_to_tail() {
        let p = plan(3.19, 1.18, 1.0);
        assert_eq!(p.along(1.5), 0.0);
        assert!((p.along(-1.5) - 1.0).abs() < 1e-6);
        let mid = p.along(0.4);
        assert!(mid > 0.2 && mid < 0.6, "{mid}");
    }
}
