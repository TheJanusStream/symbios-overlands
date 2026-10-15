//! The hardtop: a body-coloured cabin over the cockpit, glazed with a lit
//! window band - never a glass box (#1359 rule 4, #1367) - and drawn as part
//! of the body it closes rather than as a box set on it (#1652).
//!
//! [`RoadsterPlan::canopy_seat`] was published in #1364 with nothing reading it,
//! so that a canopy could never again choose its own height and hover over a
//! body it does not fit. This was its first reader: the cabin is bedded on that
//! plane, over the cockpit it closes, and as wide as the body is.
//!
//! # Why a Superellipsoid
//!
//! The phase-1 prototype tried the family's own idiom first - an upper
//! half-pipe swept over the cockpit's stations, sprung out of the scuttle and
//! dying into the tail deck - and it failed twice by render, for one reason: a
//! swept dome has no upright side. From the chase camera's 22.9 degree pitch a
//! window band low on it read as a glowing ring round a pod, and one high on it
//! as a hatch rim. A boxy Superellipsoid - the "pressed panel" the redesign's
//! rules allow - has a flat roof on near-upright sides, and a band on those
//! sides reads as the glazing of a closed car.
//!
//! # Why laid along the body (#1652)
//!
//! Stood upright it was a box ON the body: a near-upright screen and back, a
//! flat roof, and one width from end to end - so over a boat-tail, which
//! narrows from 0.99 to 0.76 of its beam along the cockpit, it stood out past
//! the tail's flanks. Laid along the body instead, its north-south axis
//! running aft ([`Cabin`]), the torture that tapers toward that axis's end
//! narrows it with the body's plan and lowers its roof onto the tail deck -
//! the FASTBACK the owner chose over a flatter coupe - a shear holds its floor
//! level on the coaming, and the axis is tilted nose-up so the front end, the
//! screen, rakes back out of the scuttle. Its cross-section is still the boxy
//! one, a flat roof on upright sides, so the band still has sides to lie on.
//!
//! The connectedness guard resolves a Superellipsoid as its box; the drawn
//! cabin is inside that box everywhere, so there the guard can over-count
//! contact but never under-count it.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::math::Vec3;

use crate::pds::generator::Generator;

use super::super::super::common::{prim, quat_x, quat_xyzw, superellipsoid, with_shape};
use super::super::SkiffColours;
use super::RoadsterPlan;
use super::{line, sweep};

/// The cabin's profile along its length - the superellipsoid's north-south
/// exponent, laid aft: blunt ends, rounded off.
const CABIN_NS: f32 = 0.45;
/// Its cross-section: a flat roof on near-upright sides, rounded at the eaves.
const CABIN_EW: f32 = 0.55;
/// How far its axis falls aft of level (degrees) - which is how far its nose,
/// the screen, rakes back.
const CABIN_TILT_DEG: f32 = 18.0;
/// How far it is bedded into the coaming, as a fraction of the length.
const CABIN_BED: f32 = 0.010;
/// Its half-width over the body's at its nose - a hand inside the flank, so
/// the body's shoulder shows under it.
const CABIN_WIDTH: f32 = 0.94;
/// How far its axis reaches ahead of the cockpit's lip, as a fraction of the
/// length, so the nose is buried in the scuttle. Aft it ends at the cockpit's
/// after end, where the tail deck takes over.
const CABIN_REACH: f32 = 0.01;
/// Latitudes (along the cabin) and longitudes (round it) it is drawn with.
const CABIN_LATITUDES: u32 = 24;
const CABIN_LONGITUDES: u32 = 24;
/// The window band's centre and half-height, as fractions of the cabin's
/// height. At 0.56, where the upright cabin carried it, the band sat on the
/// laid cabin's nose POLE and stood out ahead of it; at 0.66 it crosses the
/// raked screen.
const WINDOW_AT: f32 = 0.66;
const WINDOW_HALF: f32 = 0.11;
/// Stations round the band's closed loop, the first repeated as the last.
const WINDOW_STATIONS: usize = 15;
/// The door pillars' station along the cabin - its taper parameter, 0 at the
/// nose and 1 at the tail - and their thickness fore and aft, as a node scale
/// on a round tube: a flattened pillar is no rotated node, so the guard's
/// similarity rule holds.
const PILLAR_AT: f32 = 0.36;
const PILLAR_FLATTEN: f32 = 0.35;
/// How far a pillar's top stays under the roof over it, as a fraction of the
/// length: sized for the upright cabin's flat roof, a pillar stood through a
/// fastback's.
const PILLAR_HEADROOM: f32 = 0.002;

/// The cabin as drawn: a Barr superellipsoid laid along the body - its
/// north-south axis aft and tilted [`CABIN_TILT_DEG`] nose-up - and tortured
/// toward its tail, so in its own frame (x across, y aft along the axis, z
/// up off it) a section `t` of the way from the nose (0) to the tail (1) is
/// narrowed by `taper[0] t`, lowered by `taper[1] t` and slid `shear t` up
/// its own height axis.
///
/// Everything is solved off the plan ([`Cabin::of`]), and everything a part
/// is seated on - the band's slice, a pillar's station, the roof over it - is
/// read back off the same surface, from the mesher's own parametrisation.
#[derive(Clone, Copy, Debug)]
struct Cabin {
    /// Half-width before the taper narrows it, half-length along the axis,
    /// half-height (m).
    half: [f32; 3],
    /// The axis's middle (m): height over the datum and station.
    y: f32,
    z: f32,
    /// The tail's taper `[across, height]`.
    taper: [f32; 2],
    /// The tail's slide up the section's height axis (m) - what holds the
    /// floor level under the tilt and the taper.
    shear: f32,
    /// The floor's height (m over the datum): the bed, under the canopy
    /// seat's plane.
    floor: f32,
}

impl Cabin {
    /// The cabin over this plan's cockpit.
    ///
    /// Its axis runs from [`CABIN_REACH`] ahead of the cockpit's lip to its
    /// after end. Its width is [`CABIN_WIDTH`] of the body's at the nose and
    /// tapers to the same share of it at the tail - inside the plan between,
    /// which is fuller than the line between its two ends. Its roof falls to
    /// the tail deck's crown at the tail, its floor stays on the bed, and it
    /// stands as tall as it must for its DRAWN peak to meet the screen top
    /// the blueprint sets: a rounded nose and a falling roof both keep the
    /// roof under its half-height, so the half-height is solved for.
    fn of(plan: &RoadsterPlan) -> Self {
        let l = plan.length;
        let (aft, fwd) = plan.cockpit_z();
        let nose = fwd + CABIN_REACH * l;
        let half_len = (nose - aft) * 0.5;
        let floor = plan.canopy_seat()[1] - CABIN_BED * l;
        let (ax, across) = (
            CABIN_WIDTH * plan.half_width_at(nose),
            (1.0 - plan.half_width_at(aft) / plan.half_width_at(nose)).max(0.0),
        );
        let deck = plan.crown_at(aft);
        let build = |az: f32| {
            let (sa, ca) = tilt();
            let lower = (1.0 - (deck - floor) / (2.0 * az * ca)).max(0.0);
            Self {
                half: [ax, half_len, az],
                y: floor + az * ca - half_len * sa,
                z: (nose + aft) * 0.5,
                taper: [across, lower],
                // The axis falls aft by the tilt, so the section's bottom
                // climbs it at the same rate - and the taper lifts it too.
                shear: 2.0 * half_len * sa / ca - az * lower,
                floor,
            }
        };
        // The peak rises with the half-height at every station, so bisect.
        let top = plan.screen_top();
        let (mut lo, mut hi) = ((top - floor) * 0.5, top - floor);
        for _ in 0..48 {
            let mid = (lo + hi) * 0.5;
            if build(mid).peak() < top {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        build((lo + hi) * 0.5)
    }

    /// The share of the full section a station `t` keeps - the
    /// superellipsoid's rounding toward its poles.
    fn keeps(t: f32) -> f32 {
        let s = (2.0 * t - 1.0).abs();
        (1.0 - s.powf(2.0 / CABIN_NS)).max(0.0).powf(CABIN_NS * 0.5)
    }

    /// Where the cabin's own frame puts `(y, z)` (along, up), in the plan's.
    fn placed(&self, y: f32, z: f32) -> (f32, f32) {
        let (sa, ca) = tilt();
        (self.y - y * sa + z * ca, self.z - y * ca - z * sa)
    }

    /// The highest point of section `t`: its roof (m over the datum).
    fn roof(&self, t: f32) -> f32 {
        let [_, ay, az] = self.half;
        let z = az * Self::keeps(t) * (1.0 - self.taper[1] * t) + self.shear * t;
        self.placed(ay * (2.0 * t - 1.0), z).0
    }

    /// The roof's highest point, anywhere along the cabin.
    fn peak(&self) -> f32 {
        (0..=256)
            .map(|i| self.roof(i as f32 / 256.0))
            .fold(f32::MIN, f32::max)
    }

    /// Where section `t` stands at height `h`: its half-width there and its
    /// station, or `None` where the section does not reach that high.
    fn at(&self, h: f32, t: f32) -> Option<(f32, f32)> {
        let [ax, ay, az] = self.half;
        let (sa, ca) = tilt();
        let y = ay * (2.0 * t - 1.0);
        // The height axis of the section's own (tilted) plane at `h`.
        let up = (h - self.y + y * sa) / ca;
        let z = (up - self.shear * t) / (1.0 - self.taper[1] * t).max(1e-6);
        let b = az * Self::keeps(t);
        if b <= 0.0 || z.abs() >= b {
            return None;
        }
        let x =
            ax * Self::keeps(t) * (1.0 - (z / b).abs().powf(2.0 / CABIN_EW)).powf(CABIN_EW * 0.5);
        Some((x * (1.0 - self.taper[0] * t), self.placed(y, up).1))
    }

    /// Whether the drawn cabin holds `p` (the plan's frame): the torture
    /// undone at the point's own station, then Barr's inside-outside test.
    fn holds(&self, p: Vec3) -> bool {
        let [ax, ay, az] = self.half;
        let (sa, ca) = tilt();
        let (dy, dz) = (p.y - self.y, p.z - self.z);
        let (y, up) = (-sa * dy - ca * dz, ca * dy - sa * dz);
        let t = ((y + ay) / (2.0 * ay)).clamp(0.0, 1.0);
        let x = p.x / (1.0 - self.taper[0] * t).max(1e-6);
        let z = (up - self.shear * t) / (1.0 - self.taper[1] * t).max(1e-6);
        let section = (x / ax).abs().powf(2.0 / CABIN_EW) + (z / az).abs().powf(2.0 / CABIN_EW);
        section.powf(CABIN_EW / CABIN_NS) + (y / ay).abs().powf(2.0 / CABIN_NS) <= 1.0
    }

    /// The point where a ray from `from` along `dir` leaves the cabin - by
    /// bisection, so it is on the drawn surface to a hundredth of a
    /// millimetre. `from` must be inside.
    fn exit(&self, from: Vec3, dir: Vec3) -> Vec3 {
        let (mut lo, mut hi) = (0.0f32, 2.0 * (self.half[0] + self.half[1]));
        for _ in 0..40 {
            let mid = (lo + hi) * 0.5;
            if self.holds(from + dir * mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        from + dir * lo
    }

    /// The closed slice of the cabin at height `h`, at [`WINDOW_STATIONS`]
    /// equal angles about its middle - so more of them at its ends, where it
    /// turns across the screen and over the falling roof, than along its
    /// straight sides - the first repeated as the last.
    fn slice(&self, h: f32) -> Vec<[f32; 3]> {
        let centre = Vec3::new(0.0, h, self.z);
        debug_assert!(
            self.holds(centre),
            "the band's height {h} is over the cabin's middle"
        );
        let fore = self.exit(centre, Vec3::Z);
        let aft = self.exit(centre, Vec3::NEG_Z);
        let middle = (fore + aft) * 0.5;
        let mut out: Vec<[f32; 3]> = (0..WINDOW_STATIONS - 1)
            .map(|k| {
                let (s, c) = (TAU * k as f32 / (WINDOW_STATIONS - 1) as f32).sin_cos();
                self.exit(middle, Vec3::new(s, 0.0, c)).to_array()
            })
            .collect();
        out.push(out[0]);
        out
    }

    /// The node: laid along the body and tilted, tapered and slid.
    fn node(&self, c: &SkiffColours) -> Generator {
        prim(
            with_shape(
                superellipsoid(
                    self.half,
                    CABIN_NS,
                    CABIN_EW,
                    CABIN_LATITUDES,
                    CABIN_LONGITUDES,
                    c.paint.clone(),
                ),
                self.taper,
                [0.0; 3],
                [0.0, self.shear],
            ),
            [0.0, self.y, self.z],
            quat_xyzw(quat_x(-FRAC_PI_2 - CABIN_TILT_DEG.to_radians())),
        )
    }
}

/// The cabin's tilt as `(sin, cos)`.
fn tilt() -> (f32, f32) {
    CABIN_TILT_DEG.to_radians().sin_cos()
}

/// The top of the cabin's roof (m above the datum) - where a decorative aura
/// hovers over a closed car rather than inside it.
pub(super) fn roof_y(plan: &RoadsterPlan) -> f32 {
    Cabin::of(plan).peak()
}

/// The cabin, its lit window band and a door pillar a side.
pub(super) fn build(kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
    let cabin = Cabin::of(plan);
    kids.push(cabin.node(c));
    // The glazing: ONE closed lit loop round the cabin at window height, in
    // the lamp slot - which is already the lit window material - lying ON the
    // drawn surface. Where it crosses the front it is the windscreen, and
    // where it crosses the falling roof behind, the rear light.
    let height = plan.screen_top() - cabin.floor;
    let h = cabin.floor + height * WINDOW_AT;
    let band = height * WINDOW_HALF;
    let glazing: Vec<([f32; 3], f32)> = cabin.slice(h).into_iter().map(|p| (p, band)).collect();
    kids.push(line(&glazing, 8, c.lamp.clone()));
    // A body-coloured door pillar a side, standing on the cabin's flank across
    // the band - fatter than the band, so it breaks the band into panes, and
    // held under the roof over it. A cabin too low to carry the band at the
    // pillar's station would draw none; the guards sweep every blueprint
    // corner for both.
    let Some(middle) = cabin.at(h, PILLAR_AT) else {
        return;
    };
    let r = band * 1.25;
    let reach =
        (cabin.roof(PILLAR_AT) - h - r - PILLAR_HEADROOM * plan.length).clamp(0.0, 1.9 * band);
    for side in [-1.0f32, 1.0] {
        let column: Vec<([f32; 3], f32)> = [-1.0f32, 0.0, 1.0]
            .iter()
            .map(|&k| {
                let y = h + k * reach;
                let (x, z) = cabin.at(y, PILLAR_AT).unwrap_or(middle);
                ([side * x, y, z], r)
            })
            .collect();
        kids.push(sweep(
            &column,
            8,
            [1.0, 1.0, PILLAR_FLATTEN],
            [0.0, 1.0],
            0.0,
            c.paint.clone(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{Affine3A, Vec2, Vec3};

    use super::super::guard::{placed, plans, triangles, vertices};
    use super::*;
    use crate::pds::avatar::livery::skiff_colours;
    use crate::pds::avatar::parts::PartCtx;

    /// Every closed car's hardtop as drawn, each part's vertices in the
    /// plan's frame: the cabin first, then the band and the two pillars.
    fn hardtops() -> Vec<(RoadsterPlan, String, Vec<Vec<Vec3>>)> {
        let c = skiff_colours(&PartCtx::for_seed(0));
        plans()
            .into_iter()
            .map(|(plan, what)| {
                let mut kids = Vec::new();
                build(&mut kids, &plan, &c);
                let drawn = kids
                    .iter()
                    .map(|g| vertices(g, placed(g, Affine3A::IDENTITY)))
                    .collect();
                (plan, what, drawn)
            })
            .collect()
    }

    /// The cabin keeps the body's shoulder all along it (#1652): no vertex of
    /// it over the coaming stands further out than [`CABIN_WIDTH`] of the
    /// body's half-width at that vertex's own station. Stood upright at one
    /// width from end to end, it reached 0.99 of a boat-tail's half-width over
    /// the narrowing tail - a box standing on the flanks, not a cabin inside
    /// the shoulders - where laid along the body it stays under 0.92 on every
    /// body and blueprint.
    #[test]
    fn the_cabin_keeps_the_bodys_shoulder_all_along_it() {
        for (plan, what, drawn) in hardtops() {
            for v in drawn[0].iter().filter(|v| v.y >= 0.0) {
                let share = v.x.abs() / plan.half_width_at(v.z);
                assert!(
                    share <= CABIN_WIDTH,
                    "{what}: the cabin draws {v} at {share:.3} of the body's half-width \
                     there, past {CABIN_WIDTH}"
                );
            }
        }
    }

    /// The fastback's solve, read off what it draws (#1652): the roof's
    /// peak meets the screen top the blueprint sets and never passes it, and
    /// the floor stands level on the bed - as low ahead of the cabin's middle
    /// as behind it, and nowhere under the bed.
    #[test]
    fn the_cabins_roof_meets_the_screen_top_and_its_floor_stays_on_the_bed() {
        for (plan, what, drawn) in hardtops() {
            let (top, floor) = (
                plan.screen_top(),
                plan.canopy_seat()[1] - CABIN_BED * plan.length,
            );
            let cabin = &drawn[0];
            let highest = cabin.iter().map(|v| v.y).fold(f32::MIN, f32::max);
            assert!(
                highest <= top + 1e-4 && top - highest < 0.003 * plan.length,
                "{what}: the roof peaks at {highest}, the screen top is {top}"
            );
            let middle = cabin.iter().map(|v| v.z).sum::<f32>() / cabin.len() as f32;
            for (half, ahead) in [("fore", true), ("aft", false)] {
                let lowest = cabin
                    .iter()
                    .filter(|v| (v.z > middle) == ahead)
                    .map(|v| v.y)
                    .fold(f32::MAX, f32::min);
                assert!(
                    lowest >= floor - 1e-4 && lowest - floor < 0.0015 * plan.length,
                    "{what}: the floor's {half} half stands at {lowest}, the bed at {floor}"
                );
            }
        }
    }

    /// The cabin's own inside-outside test - what the band's stations are
    /// found by - agrees with the surface the mesher draws (#1652): every
    /// vertex of the drawn cabin, drawn a fiftieth of the way in toward the
    /// cabin's middle, is held, and the same distance out is not. It is the
    /// mesher's torture run backwards by hand, so this is what ties the
    /// glazing to the skin it lies on.
    #[test]
    fn the_band_is_laid_on_the_cabin_the_mesher_draws() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let cabin = Cabin::of(&plan);
            let node = cabin.node(&c);
            let middle = Vec3::new(0.0, cabin.y, cabin.z);
            for v in vertices(&node, placed(&node, Affine3A::IDENTITY)) {
                let out = v - middle;
                assert!(
                    cabin.holds(middle + out * 0.98) && !cabin.holds(middle + out * 1.02),
                    "{what}: the drawn vertex {v} is not where the cabin's own test puts its skin"
                );
            }
        }
    }

    /// The cabin's drawn roof over the centreline at station `z`: the
    /// highest of its triangles straight over `(0, z)`. Read off the
    /// triangles, not the vertices: laid along the body, the cabin's
    /// latitude rings stand a tenth of a metre apart amidships, and the roof
    /// between two of them is the facet that joins them.
    fn roof_over(triangles: &[[Vec3; 3]], z: f32) -> f32 {
        triangles
            .iter()
            .filter_map(|[a, b, c]| {
                let (e1, e2) = (
                    Vec2::new(b.x - a.x, b.z - a.z),
                    Vec2::new(c.x - a.x, c.z - a.z),
                );
                let to = Vec2::new(-a.x, z - a.z);
                let det = e1.perp_dot(e2);
                if det.abs() < 1e-12 {
                    return None;
                }
                let (u, v) = (to.perp_dot(e2) / det, e1.perp_dot(to) / det);
                (u >= 0.0 && v >= 0.0 && u + v <= 1.0)
                    .then_some(a.y + u * (b.y - a.y) + v * (c.y - a.y))
            })
            .fold(f32::MIN, f32::max)
    }

    /// Both pillars are drawn and stand under the roof over them (#1652): no
    /// pillar vertex is higher than the cabin's roof at that vertex's
    /// station. Sized for the upright cabin's flat roof, a pillar stood
    /// through the fastback's falling one.
    #[test]
    fn both_pillars_stand_under_the_roof() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let mut kids = Vec::new();
            build(&mut kids, &plan, &c);
            assert_eq!(kids.len(), 4, "{what}: a cabin, its band and two pillars");
            let roof = triangles(&kids[0], placed(&kids[0], Affine3A::IDENTITY));
            for pillar in &kids[2..] {
                for p in vertices(pillar, placed(pillar, Affine3A::IDENTITY)) {
                    let over = roof_over(&roof, p.z);
                    assert!(
                        p.y < over,
                        "{what}: a pillar draws {p}, {} m over the roof at its station",
                        p.y - over
                    );
                }
            }
        }
    }
}
