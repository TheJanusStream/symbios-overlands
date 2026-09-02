//! Bench — a slatted seat on cast-iron end-frames. An escalation-Calm scatter
//! prop: public seating signals a settled, unthreatened place to linger in
//! any setting.
//!
//! Rebuilt under #972 after an in-world check ("reads too blocky; the top
//! beam floats"). The shipped bench faced `+Z`, so the render front and the
//! settlement placer both saw its back; its backrest was three loose bars
//! at one rake standing in front of a *vertical* back leg that stopped at
//! 0.8 m, so the top two slats hung in the air; and every member was 70 mm
//! square stock, which is what a bench looks like when it is made of
//! blocks. Now the seat faces `-Z`, the stock is slimmer and the legs are
//! turned, and each end frame is one chain — foot, leg, seat rail, then a
//! raked stile ([`strut`]) running from the seat rail to above the top
//! slat — with every backrest slat seated on that stile's own line.
//!
//! #972 lesson 34: **a raked part is carried by a raked member.** Three
//! slats at one lean and a vertical leg behind them agree at exactly one
//! height; above it the slats are in the air, and the record cannot say
//! so. Author the carrier as a line between two points (`strut`), seat
//! every part on that line through one function ([`stile_z`]), and guard
//! it from the other direction — read the built stile's ends through
//! `rotate_by` and check each slat's centre lies on the segment and below
//! its head. Run against the shipped constants, the guard fails with the
//! top slat 0.23 m above the tallest thing behind it.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, nest, prim, quat_x, solid, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD, bronze, wood};

const IRON: [f32; 3] = [0.12, 0.12, 0.13];

/// Slat length — the bench's own length.
const LEN: f32 = 1.4;
/// The two cast end frames stand at `±FRAME_X`.
const FRAME_X: f32 = 0.64;
/// Seat surface height.
const SEAT_Y: f32 = 0.45;
const SLAT_T: f32 = 0.04;
const SLAT_W: f32 = 0.08;
/// Seat slat centrelines across the seat. The front is `-Z`, the render
/// front.
const SEAT_SLATS_Z: [f32; 5] = [-0.2, -0.1, 0.0, 0.1, 0.2];
/// Flat cast stock — seat rail, tie, armrest width.
const BAR: f32 = 0.05;
/// Turned leg / stile radius.
const LEG_R: f32 = 0.03;
/// Front and back leg lines.
const FRONT_Z: f32 = -0.22;
const BACK_Z: f32 = 0.22;
/// Seat rail centre height; its top carries the slats.
const RAIL_Y: f32 = SEAT_Y - SLAT_T - BAR * 0.5;
/// Backrest rake toward `+Z`, radians.
const LEAN: f32 = 0.2;
/// The stile's head, above the top slat.
const STILE_TOP: f32 = 0.94;
/// Backrest slat centres, measured up the stile.
const BACK_SLATS_Y: [f32; 3] = [0.58, 0.72, 0.86];
const BACK_SLAT_H: f32 = 0.09;
const BACK_SLAT_T: f32 = 0.035;
/// Armrest centre height and thickness.
const ARM_Y: f32 = 0.67;
const ARM_T: f32 = 0.04;
/// Ankle stretcher height between the two legs of a frame.
const STRETCHER_Y: f32 = 0.14;

/// Where the stile is, in `z`, at height `y` — the one line the stile, the
/// three slats and the armrest's back end all read from (#972 lesson 18:
/// one expression, bound once).
fn stile_z(y: f32) -> f32 {
    BACK_Z + (y - RAIL_Y) * LEAN.tan()
}

pub struct Bench;

impl CatalogueEntry for Bench {
    fn slug(&self) -> &'static str {
        "bench"
    }
    fn name(&self) -> &'static str {
        "Bench"
    }
    fn description(&self) -> &'static str {
        "Slatted wooden seat on cast-iron end-frames."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Calm)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.1,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One cast end frame at `x`: the seat rail is the sub-root, carrying the
/// two turned legs on their foot pads, an ankle stretcher, the raked stile,
/// the arm post over the front leg and the armrest that ties post to stile.
fn end_frame(x: f32) -> Generator {
    let iron = || bronze(IRON);
    let rail = prim(
        solid(cuboid_tapered(
            [BAR, BAR, BACK_Z - FRONT_Z + BAR],
            0.0,
            iron(),
        )),
        [x, RAIL_Y, 0.0],
        id_quat(),
    );
    // Ground to the rail's underside.
    let leg_h = RAIL_Y - BAR * 0.5;
    let mut parts = Vec::new();
    for z in [FRONT_Z, BACK_Z] {
        parts.push(prim(
            solid(cylinder_tapered(LEG_R, leg_h, 8, 0.0, iron())),
            [x, leg_h * 0.5, z],
            id_quat(),
        ));
        parts.push(prim(
            solid(cuboid_tapered([0.1, 0.02, 0.12], 0.0, iron())),
            [x, 0.01, z],
            id_quat(),
        ));
    }
    // Ankle stretcher between the legs.
    parts.push(prim(
        solid(cuboid_tapered(
            [BAR * 0.8, BAR * 0.8, BACK_Z - FRONT_Z],
            0.0,
            iron(),
        )),
        [x, STRETCHER_Y, 0.0],
        id_quat(),
    ));
    // Raked stile: from the seat rail to above the top slat, on the line
    // the slats are seated on.
    parts.push(strut(
        [x, RAIL_Y, BACK_Z],
        [x, STILE_TOP, stile_z(STILE_TOP)],
        LEG_R * 0.95,
        8,
        iron(),
    ));
    // Arm post over the front leg, seat rail to armrest.
    let post_h = ARM_Y - RAIL_Y;
    parts.push(prim(
        solid(cylinder_tapered(LEG_R * 0.85, post_h, 8, 0.0, iron())),
        [x, RAIL_Y + post_h * 0.5, FRONT_Z],
        id_quat(),
    ));
    // Armrest: a little proud of the post at the front, through the stile
    // at the back.
    let arm_front = FRONT_Z - 0.03;
    let arm_back = stile_z(ARM_Y) + LEG_R;
    parts.push(prim(
        solid(cuboid_tapered(
            [BAR, ARM_T, arm_back - arm_front],
            0.0,
            iron(),
        )),
        [x, ARM_Y, (arm_front + arm_back) * 0.5],
        id_quat(),
    ));
    nest(rail, parts)
}

fn build_tree() -> Generator {
    // Tie rail under the seat joining the two end frames: the root.
    let tie = prim(
        solid(cuboid_tapered([FRAME_X * 2.0, BAR, BAR], 0.0, bronze(IRON))),
        [0.0, RAIL_Y - BAR * 0.5, 0.0],
        id_quat(),
    );
    let mut parts = Vec::new();
    // Seat slats, resting on the two seat rails.
    for z in SEAT_SLATS_Z {
        parts.push(prim(
            solid(cuboid_tapered([LEN, SLAT_T, SLAT_W], 0.0, wood(WOOD))),
            [0.0, SEAT_Y - SLAT_T * 0.5, z],
            id_quat(),
        ));
    }
    // Backrest slats, each centred on the stile line and raked with it.
    for y in BACK_SLATS_Y {
        parts.push(prim(
            solid(cuboid_tapered(
                [LEN, BACK_SLAT_H, BACK_SLAT_T],
                0.0,
                wood(WOOD),
            )),
            [0.0, y, stile_z(y)],
            quat_x(LEAN),
        ));
    }
    parts.push(end_frame(-FRAME_X));
    parts.push(end_frame(FRAME_X));
    nest(tie, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
    };
    use crate::pds::GeneratorKind;

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// A member's two ends along its own local `Y`, read from the BUILT
    /// node — its actual quaternion and its actual half-extent (#972
    /// lessons 21 and 23). Returned low end first.
    fn ends(g: &Generator, at: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
        let half = match &g.kind {
            GeneratorKind::Cuboid { size, .. } => size.0[1] * 0.5,
            GeneratorKind::Cylinder { height, .. } => height.0 * 0.5,
            _ => return None,
        };
        let tip = rotate_by(g.transform.rotation.0, [0.0, half, 0.0]);
        let a = [at[0] - tip[0], at[1] - tip[1], at[2] - tip[2]];
        let b = [at[0] + tip[0], at[1] + tip[1], at[2] + tip[2]];
        Some(if a[1] <= b[1] { (a, b) } else { (b, a) })
    }

    /// The backrest slats: the long thin boards above the seat. Selected by
    /// class (a board over a metre long, thin in `Z`, above the seat) rather
    /// than by the exact `LEN`, so the guard reads the shipped build's
    /// 1.34 m slats too (#972 lesson 24: the selector is where a guard goes
    /// quiet).
    fn backrest_slats(root: &Generator) -> Vec<(Generator, [f32; 3])> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, .. } = &g.kind
                && size.0[0] > 1.0
                && size.0[2] < 0.06
                && size.0[1] > 0.05
                && at[1] > SEAT_Y
            {
                out.push((g.clone(), at));
            }
        });
        out
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Bench.build(""), "bench");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Bench.build(""), "bench");
    }

    /// **The top beam does not float.** Every backrest slat tops out below
    /// the head of the tallest member behind the seat on its own side, and
    /// its centre lies on that member's line — read from the built stile's
    /// ends, not from the constants that placed the slats.
    ///
    /// "Behind" is decided by where the slats themselves are, so the guard
    /// bites whichever way the bench faces. Against the shipped build it
    /// fails with the top slat 0.23 m above a vertical back leg.
    #[test]
    fn the_backrest_is_carried_by_a_stile_that_reaches_its_top_slat() {
        let root = Bench.build("");
        let slats = backrest_slats(&root);
        assert_eq!(slats.len(), 3, "three backrest slats");
        let back_sign = slats.iter().map(|(_, at)| at[2]).sum::<f32>().signum();
        // Every upright-ish member at a frame's x, on the backrest's side.
        let mut carriers: Vec<(f32, [f32; 3], [f32; 3])> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if at[0].abs() < 0.5 {
                return;
            }
            let Some((lo, hi)) = ends(g, at) else { return };
            let len = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2))
                .sqrt();
            if hi[1] - lo[1] < 0.7 * len || lo[2] * back_sign < 0.1 {
                return;
            }
            carriers.push((at[0].signum(), lo, hi));
        });
        assert!(
            carriers.len() >= 2,
            "only {} members behind the seat — suspect the selector",
            carriers.len()
        );
        for side in [-1.0_f32, 1.0] {
            let (_, foot, head) = carriers
                .iter()
                .filter(|c| c.0 == side)
                .max_by(|a, b| a.2[1].partial_cmp(&b.2[1]).unwrap())
                .expect("a carrier on each side");
            for (g, at) in &slats {
                let q = g.transform.rotation.0;
                let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                    unreachable!()
                };
                let top = at[1] + rotate_by(q, [0.0, size.0[1] * 0.5, 0.0])[1];
                assert!(
                    top <= head[1] + 1e-4,
                    "bench: a slat tops out at {top} and the tallest member behind it on side \
                     {side} reaches {} — the top beam floats",
                    head[1]
                );
                // Distance from the slat centre to the carrier's segment, in the
                // side plane (y, z).
                let (dy, dz) = (head[1] - foot[1], head[2] - foot[2]);
                let t = ((at[1] - foot[1]) * dy + (at[2] - foot[2]) * dz) / (dy * dy + dz * dz);
                let (py, pz) = (foot[1] + t * dy, foot[2] + t * dz);
                let off = ((at[1] - py).powi(2) + (at[2] - pz).powi(2)).sqrt();
                assert!(
                    off < 0.012,
                    "bench: a slat at {at:?} sits {off} m off the stile's line"
                );
            }
        }
    }

    /// The seat faces the render front: the backrest is on `+Z`, behind the
    /// seat as the camera sees it, and the front legs are on `-Z`.
    #[test]
    fn the_seat_faces_the_render_front() {
        let root = Bench.build("");
        for (_, at) in backrest_slats(&root) {
            assert!(
                at[2] > 0.1,
                "bench: a backrest slat at {at:?} is in front of the seat"
            );
        }
        let mut legs = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder { height, .. } = &g.kind
                && at[1] - height.0 * 0.5 < 1e-3
                && g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0]
            {
                legs += 1;
                assert!(
                    at[2].abs() > 0.1,
                    "a leg at {at:?} is under the middle of the seat"
                );
            }
        });
        assert_eq!(legs, 4, "four turned legs on the ground");
    }

    /// The seat slats rest on the two seat rails — bottom on rail top, and
    /// inside the rail's own span.
    #[test]
    fn the_seat_slats_rest_on_the_rails() {
        let root = Bench.build("");
        let mut rails: Vec<([f32; 3], f32)> = Vec::new();
        let mut slats: Vec<([f32; 3], f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let s = size.0;
            if (s[0] - BAR).abs() < 1e-4
                && (s[1] - BAR).abs() < 1e-4
                && s[2] > 0.4
                && at[0].abs() > 0.5
            {
                rails.push((at, s[2]));
            }
            if (s[0] - LEN).abs() < 1e-4 && (s[1] - SLAT_T).abs() < 1e-4 {
                slats.push((at, s[2]));
            }
        });
        assert_eq!(rails.len(), 2, "two seat rails");
        assert_eq!(slats.len(), SEAT_SLATS_Z.len(), "one slat per centreline");
        for (rail, span) in &rails {
            let top = rail[1] + BAR * 0.5;
            for (slat, w) in &slats {
                assert!(
                    (slat[1] - SLAT_T * 0.5 - top).abs() < 1e-3,
                    "bench: a slat at {slat:?} does not sit on the rail top at {top}"
                );
                assert!(
                    slat[2].abs() + w * 0.5 <= rail[2].abs() + span * 0.5 + 1e-4,
                    "bench: a slat at {slat:?} overhangs its rail"
                );
            }
        }
    }

    /// Each end frame is one unbroken chain up its front line — foot pad,
    /// leg, seat rail, arm post, armrest — from the ground to the armrest's
    /// top (#972 lesson 33: state a stack as a chain, not as pairs).
    #[test]
    fn each_frame_chains_from_the_ground_to_the_armrest() {
        let root = Bench.build("");
        for side in [-1.0_f32, 1.0] {
            let mut spans: Vec<(f32, f32)> = Vec::new();
            let mut arm_top = 0.0_f32;
            walk(&root, [0.0; 3], &mut |g, at| {
                if (at[0] - side * FRAME_X).abs() > 1e-3
                    || g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0]
                {
                    return;
                }
                let (hy, hz) = match &g.kind {
                    GeneratorKind::Cuboid { size, .. } => (size.0[1] * 0.5, size.0[2] * 0.5),
                    GeneratorKind::Cylinder { radius, height, .. } => (height.0 * 0.5, radius.0),
                    _ => return,
                };
                if (at[2] - FRONT_Z).abs() > hz {
                    return;
                }
                spans.push((at[1] - hy, at[1] + hy));
                if let GeneratorKind::Cuboid { size, .. } = &g.kind
                    && (size.0[1] - ARM_T).abs() < 1e-4
                {
                    arm_top = at[1] + hy;
                }
            });
            assert!(
                spans.len() >= 5,
                "side {side}: only {} members on the front line",
                spans.len()
            );
            spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let mut reach = 0.0_f32;
            for (lo, hi) in &spans {
                if *lo > reach + 1e-4 {
                    break;
                }
                reach = reach.max(*hi);
            }
            assert!(
                arm_top > 0.6 && reach >= arm_top - 1e-4,
                "bench side {side}: the frame chains to {reach} and the armrest tops out at \
                 {arm_top} — something on the front line floats"
            );
        }
    }

    /// The armrest's back end reaches the stile at the armrest's own height
    /// — read from the built stile, not from `stile_z`.
    #[test]
    fn the_armrest_reaches_the_stile() {
        let root = Bench.build("");
        for side in [-1.0_f32, 1.0] {
            let mut stile: Option<([f32; 3], [f32; 3])> = None;
            let mut arm: Option<([f32; 3], f32)> = None;
            walk(&root, [0.0; 3], &mut |g, at| {
                if (at[0] - side * FRAME_X).abs() > 1e-3 {
                    return;
                }
                match &g.kind {
                    GeneratorKind::Cylinder { .. }
                        if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] =>
                    {
                        stile = ends(g, at);
                    }
                    GeneratorKind::Cuboid { size, .. } if (size.0[1] - ARM_T).abs() < 1e-4 => {
                        arm = Some((at, size.0[2]));
                    }
                    _ => {}
                }
            });
            let (foot, head) = stile.expect("a raked stile on each side");
            let (arm, len) = arm.expect("an armrest on each side");
            let t = (arm[1] - foot[1]) / (head[1] - foot[1]);
            assert!(
                (0.0..=1.0).contains(&t),
                "the armrest is not within the stile's height"
            );
            let stile_at_arm = foot[2] + t * (head[2] - foot[2]);
            let back_end = arm[2] + len * 0.5;
            assert!(
                back_end >= stile_at_arm && back_end <= stile_at_arm + 2.0 * LEG_R + 1e-3,
                "bench side {side}: the armrest ends at z {back_end} and the stile is at \
                 {stile_at_arm} there"
            );
        }
    }

    /// The editability contract (#972 lesson 3): the tie carries the slats
    /// and two end frames; each frame's seat rail carries everything cast
    /// with it.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        let root = Bench.build("");
        let frames: Vec<&Generator> = root
            .children
            .iter()
            .filter(|c| {
                matches!(&c.kind, GeneratorKind::Cuboid { size, .. }
                    if (size.0[0] - BAR).abs() < 1e-4 && size.0[2] > 0.4)
            })
            .collect();
        assert_eq!(frames.len(), 2, "the tie carries two end frames");
        for f in frames {
            assert_eq!(
                f.children.len(),
                8,
                "a frame carries two legs, two pads, a stretcher, the stile, the post and the armrest"
            );
        }
        assert_eq!(
            root.children.len(),
            SEAT_SLATS_Z.len() + BACK_SLATS_Y.len() + 2
        );
    }
}
