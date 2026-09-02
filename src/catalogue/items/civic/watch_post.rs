//! Watch post — a stilted timber platform with a railing and a pyramidal
//! roof. An escalation-Conflict scatter prop: a hasty lookout reads the
//! same whether it overlooks a medieval road or a cyberpunk checkpoint.
//!
//! Reworked under #972 after an in-world check ("ladder steps and roof
//! rotated wrongly; ladder not on the open side"). The roof was a
//! four-sided [`cone`] at the identity, and a revolved prim's vertex 0 is on
//! `+X` — so the pyramid sat as a DIAMOND over the square fascia, its
//! corners 0.4 m out along each axis and its flat faces barely reaching the
//! fascia's edge. It is turned an eighth of a turn now and sized from the
//! fascia so the faces overhang it. The ladder leaned on the railed `+X`
//! side, its rungs ran along `X` (across the ladder's plane, not between
//! the rails) and their offset drifted the wrong way with height. It now
//! stands on the open `-Z` front: the rails are [`strut`]s from a foot on
//! the ground to a head at the deck's own edge, and every rung is placed on
//! that same line. Still a makeshift lookout — the band is Conflict, and
//! rough is right; what changed is that the parts now touch.
//!
//! #972 lesson 35: **a low-resolution revolved prim is a polygon with a
//! vertex on `+X`**, so a 4-sided cone over a square is a diamond and an
//! 8-sided drum has flats on the diagonals. Square-on-square wants the
//! eighth turn, and the guard reads the built quaternion and rotates the
//! vertex-0 ray to see where a corner lands. And **a ladder is one line**:
//! author the rails from foot to head and every rung on the same
//! interpolation; a rung spans the axis the rails are *separated* along,
//! which the guard derives from the two built rails rather than assumes.

use std::f32::consts::{FRAC_PI_4, SQRT_2};

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_y, solid, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{LANTERN_WARM, WOOD, WOOD_GREY, quat_z, wood};

const LEG_H: f32 = 2.0;
/// Stilt legs stand at `±HALF`.
const HALF: f32 = 0.55;
const DECK_Y: f32 = LEG_H;
const DECK_T: f32 = 0.12;
const DECK_HALF: f32 = 0.725;
const BOARD_T: f32 = 0.04;
/// Top of the deck boards — what a climber steps onto.
const DECK_TOP: f32 = DECK_Y + DECK_T * 0.5 + BOARD_T;
const POST_H: f32 = 1.1;
const EAVE_Y: f32 = DECK_Y + POST_H;
const FASCIA_HALF: f32 = 0.775;
const FASCIA_T: f32 = 0.1;
/// How far the roof's flat faces overhang the fascia.
const EAVE: f32 = 0.12;
/// A four-sided cone's flat faces lie `r / √2` from its axis, so the base
/// radius that puts them `FASCIA_HALF + EAVE` out is that times `√2`.
const ROOF_R: f32 = (FASCIA_HALF + EAVE) * SQRT_2;
const ROOF_H: f32 = 0.75;
/// The roof's base is sunk 5 mm into the fascia's top.
const ROOF_BASE: f32 = EAVE_Y + FASCIA_T * 0.5 - 0.005;
const FINIAL_R: f32 = 0.05;
const FINIAL_H: f32 = 0.2;
/// The finial's base sits this far below the apex, where the pyramid is
/// still wider than it (#972 lesson 33b).
const FINIAL_SINK: f32 = 0.075;
/// Ladder: centreline, rail half-spacing, foot on the ground and head
/// leaning on the deck's front edge — on the open `-Z` side, beside the
/// lantern's line rather than under it.
const LADDER_X: f32 = 0.3;
const LADDER_HALF: f32 = 0.16;
const LADDER_FOOT: [f32; 3] = [LADDER_X, 0.0, -(DECK_HALF + 0.6)];
const LADDER_HEAD: [f32; 3] = [LADDER_X, DECK_TOP + 0.25, -(DECK_HALF + 0.03)];
const RAIL_R: f32 = 0.035;
const RUNG: f32 = 0.05;
const RUNGS: usize = 6;

pub struct WatchPost;

impl CatalogueEntry for WatchPost {
    fn slug(&self) -> &'static str {
        "watch_post"
    }
    fn name(&self) -> &'static str {
        "Watch Post"
    }
    fn description(&self) -> &'static str {
        "Stilted timber platform with a railing and a peaked roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Conflict)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A point `t` of the way up the ladder's line, foot to head.
fn along_ladder(t: f32) -> [f32; 3] {
    [
        LADDER_FOOT[0] + (LADDER_HEAD[0] - LADDER_FOOT[0]) * t,
        LADDER_FOOT[1] + (LADDER_HEAD[1] - LADDER_FOOT[1]) * t,
        LADDER_FOOT[2] + (LADDER_HEAD[2] - LADDER_FOOT[2]) * t,
    ]
}

fn build_tree() -> Generator {
    let leg = || solid(cylinder_tapered(0.08, LEG_H, 8, 0.0, wood(WOOD)));

    let mut prims = Vec::new();

    // Four stilt legs.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(leg(), [sx * HALF, LEG_H * 0.5, sz * HALF], id_quat()));
    }

    // X-braces tying the legs on all four faces.
    for (cx, cz, vert) in [
        (0.0_f32, HALF, false), // back face
        (0.0, -HALF, false),    // front face
        (-HALF, 0.0, true),     // left face
        (HALF, 0.0, true),      // right face
    ] {
        for s in [-1.0_f32, 1.0] {
            let rot = if vert {
                quat_x(s * 0.75)
            } else {
                quat_z(s * 0.75)
            };
            prims.push(prim(
                solid(cuboid_tapered([0.05, 1.5, 0.05], 0.0, wood(WOOD_GREY))),
                [cx, LEG_H * 0.5, cz],
                rot,
            ));
        }
    }

    // Platform deck + plank boards.
    prims.push(prim(
        solid(cuboid_tapered(
            [DECK_HALF * 2.0, DECK_T, DECK_HALF * 2.0],
            0.0,
            wood(WOOD_GREY),
        )),
        [0.0, DECK_Y, 0.0],
        id_quat(),
    ));
    for dz in [-0.5_f32, -0.17, 0.17, 0.5] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, BOARD_T, 0.28], 0.0, wood(WOOD))),
            [0.0, DECK_Y + DECK_T * 0.5 + BOARD_T * 0.5, dz],
            id_quat(),
        ));
    }

    // Corner posts rising from the deck to carry the roof eaves.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.06, POST_H, 8, 0.0, wood(WOOD))),
            [sx * 0.66, DECK_Y + POST_H * 0.5, sz * 0.66],
            id_quat(),
        ));
    }

    // Railing — top + mid rails around the back and sides; the front (-Z)
    // is left open as the lookout's vantage, and it is where the ladder
    // lands.
    for rail_y in [DECK_Y + 0.46, DECK_Y + 0.24] {
        // Back rail.
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.07, 0.07], 0.0, wood(WOOD))),
            [0.0, rail_y, 0.66],
            id_quat(),
        ));
        // Side rails.
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.07, 0.07, 1.4], 0.0, wood(WOOD))),
                [sx * 0.66, rail_y, 0.0],
                id_quat(),
            ));
        }
    }

    // Square eave fascia the roof sits on.
    prims.push(prim(
        solid(cuboid_tapered(
            [FASCIA_HALF * 2.0, FASCIA_T, FASCIA_HALF * 2.0],
            0.0,
            wood([0.3, 0.2, 0.12]),
        )),
        [0.0, EAVE_Y, 0.0],
        id_quat(),
    ));
    // Pyramidal roof: a four-sided cone turned an eighth so its FACES lie
    // over the fascia's sides (vertex 0 of a revolved prim is on +X), sized
    // so those faces overhang it by `EAVE`.
    prims.push(prim(
        cone(ROOF_R, ROOF_H, 4, wood([0.32, 0.22, 0.13])),
        [0.0, ROOF_BASE + ROOF_H * 0.5, 0.0],
        quat_y(FRAC_PI_4),
    ));
    // Finial, seated below the apex where the pyramid is still wider than it.
    let apex = ROOF_BASE + ROOF_H;
    prims.push(prim(
        cylinder_tapered(FINIAL_R, FINIAL_H, 6, 0.0, wood(WOOD)),
        [0.0, apex - FINIAL_SINK + FINIAL_H * 0.5, 0.0],
        id_quat(),
    ));

    // A warning lantern hung under the eave at the open front.
    prims.push(prim(
        cuboid_tapered([0.16, 0.22, 0.16], 0.0, glow(LANTERN_WARM, 2.6)),
        [0.0, EAVE_Y - 0.2, -0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.04, 0.18, 0.04], 0.0, wood(WOOD))),
        [0.0, EAVE_Y - 0.02, -0.5],
        id_quat(),
    ));

    // The access ladder, leaning on the open front: two rails on one line
    // from the ground to the deck edge, rungs on the same line.
    for sx in [-1.0_f32, 1.0] {
        let dx = sx * LADDER_HALF;
        let foot = [LADDER_FOOT[0] + dx, LADDER_FOOT[1], LADDER_FOOT[2]];
        let head = [LADDER_HEAD[0] + dx, LADDER_HEAD[1], LADDER_HEAD[2]];
        prims.push(strut(foot, head, RAIL_R, 8, wood(WOOD)));
    }
    for i in 0..RUNGS {
        let t = (i + 1) as f32 / (RUNGS + 1) as f32;
        prims.push(prim(
            solid(cuboid_tapered(
                [LADDER_HALF * 2.0 + RAIL_R * 2.0, RUNG, RUNG],
                0.0,
                wood(WOOD_GREY),
            )),
            along_ladder(t),
            id_quat(),
        ));
    }

    super::assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
    };
    use crate::pds::GeneratorKind;
    use std::f32::consts::FRAC_1_SQRT_2;

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// A member's two ends along its own local `Y`, from the BUILT node
    /// (#972 lessons 21/23), low end first.
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

    /// The two ladder rails: the only tilted members over two metres long.
    fn rails(root: &Generator) -> Vec<([f32; 3], [f32; 3])> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            let long = match &g.kind {
                GeneratorKind::Cuboid { size, .. } => size.0[1] > 2.2,
                GeneratorKind::Cylinder { height, .. } => height.0 > 2.2,
                _ => false,
            };
            if long
                && g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0]
                && let Some(e) = ends(g, at)
            {
                out.push(e);
            }
        });
        out
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WatchPost.build(""), "watch_post");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&WatchPost.build(""), "watch_post");
    }

    /// **The pyramid is square to the fascia and overhangs it.** A revolved
    /// prim's vertex 0 lies on its local `+X`; rotating that ray by the
    /// roof's built quaternion says where a corner actually lands, and a
    /// corner on a diagonal means the faces lie over the fascia's sides.
    /// Against the shipped identity rotation this fails with the corner at
    /// `[1.15, 0, 0]`.
    #[test]
    fn the_roof_is_square_to_the_fascia_and_overhangs_it() {
        let root = WatchPost.build("");
        let mut roof: Option<([f32; 3], f32, f32, [f32; 4])> = None;
        let mut fascia: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cone { radius, height, .. } => {
                roof = Some((at, radius.0, height.0, g.transform.rotation.0))
            }
            GeneratorKind::Cuboid { size, .. } if (size.0[0] - FASCIA_HALF * 2.0).abs() < 1e-4 => {
                fascia = Some((at, size.0))
            }
            _ => {}
        });
        let (at, r, h, q) = roof.expect("a roof cone");
        let (fat, fsize) = fascia.expect("an eave fascia");
        let corner = rotate_by(q, [r, 0.0, 0.0]);
        assert!(
            (corner[0].abs() - corner[2].abs()).abs() < 1e-3,
            "watch_post: a corner of the roof lands at {corner:?} — the pyramid's corners are \
             on the axes, so it sits as a diamond over the square fascia"
        );
        let face = r * FRAC_1_SQRT_2;
        assert!(
            face >= fsize[0] * 0.5 + 0.05,
            "watch_post: the roof's faces reach {face} m from the axis over a fascia \
             {} m wide — no eave",
            fsize[0]
        );
        let base = at[1] - h * 0.5;
        assert!(
            base > fat[1] - fsize[1] * 0.5 && base < fat[1] + fsize[1] * 0.5,
            "watch_post: the roof's base at {base} is not seated in the fascia"
        );
    }

    /// The finial is seated in the roof, not balanced on its point (#972
    /// lesson 33b) — and on a turned pyramid the width that matters is the
    /// FACE inset, not the corner radius.
    #[test]
    fn the_finial_is_seated_in_the_roof_not_balanced_on_it() {
        let root = WatchPost.build("");
        let mut roof: Option<([f32; 3], f32, f32)> = None;
        let mut finial: Option<([f32; 3], f32, f32)> = None;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cone { radius, height, .. } => roof = Some((at, radius.0, height.0)),
            GeneratorKind::Cylinder { radius, height, .. } if at[1] > EAVE_Y + 0.5 => {
                finial = Some((at, radius.0, height.0))
            }
            _ => {}
        });
        let (rat, rr, rh) = roof.expect("a roof cone");
        let (fat, fr, fh) = finial.expect("a finial over the roof");
        let apex = rat[1] + rh * 0.5;
        let drop = apex - (fat[1] - fh * 0.5);
        assert!(drop > 0.0, "the finial's base is above the apex");
        let face_there = rr * drop / rh * FRAC_1_SQRT_2;
        assert!(
            face_there >= fr,
            "watch_post: at the finial's base the pyramid's faces are {face_there} m from the \
             axis and the finial is {fr} — balanced on the point"
        );
        assert!(
            fat[1] + fh * 0.5 > apex,
            "the finial does not clear the apex"
        );
    }

    /// **The ladder stands on the ground and lands on the OPEN side.** Both
    /// rails' feet are on the ground; both heads are above the deck top and
    /// just in front of the deck's `-Z` edge, the side with no rail.
    /// Against the shipped build the heads land at `x ≈ 0.69, z = ±0.16` —
    /// the railed right side.
    #[test]
    fn the_ladder_stands_on_the_ground_and_lands_on_the_open_front() {
        let rails = rails(&WatchPost.build(""));
        assert_eq!(rails.len(), 2, "two ladder rails");
        for (foot, head) in rails {
            assert!(
                foot[1].abs() < 0.02,
                "a rail's foot at {foot:?} is not on the ground"
            );
            assert!(
                head[1] > DECK_TOP,
                "a rail's head at {head:?} is below the deck top"
            );
            assert!(
                head[2] < -DECK_HALF && head[2] > -(DECK_HALF + 0.1) && head[0].abs() < DECK_HALF,
                "watch_post: a rail's head lands at {head:?}, not on the deck's open -Z edge \
                 (half-width {DECK_HALF})"
            );
            assert!(
                foot[2] < head[2] - 0.3,
                "the ladder does not lean: {foot:?} -> {head:?}"
            );
        }
    }

    /// **Every rung bridges the rails on their own line.** The axis the two
    /// rails are separated along is read from the built rails; each rung's
    /// long axis must be that axis, and its centre must sit on the rails'
    /// mean line at its own height. Against the shipped build the rungs run
    /// along `X` while the rails are 0.32 m apart along `Z`.
    #[test]
    fn every_rung_bridges_the_rails_on_their_own_line() {
        let root = WatchPost.build("");
        let rails = rails(&root);
        assert_eq!(rails.len(), 2);
        let (a, b) = (rails[0], rails[1]);
        let sep = [a.1[0] - b.1[0], a.1[2] - b.1[2]];
        let sep_axis = if sep[0].abs() > sep[1].abs() { 0 } else { 2 };
        let mut rungs = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let s = size.0;
            let thin = |v: f32| (v - RUNG).abs() < 1e-4;
            let long_axis = match (thin(s[0]), thin(s[1]), thin(s[2])) {
                (false, true, true) if s[0] > 0.3 => 0,
                (true, true, false) if s[2] > 0.3 => 2,
                _ => return,
            };
            rungs += 1;
            assert_eq!(
                long_axis, sep_axis,
                "watch_post: a rung at {at:?} runs along axis {long_axis} but the rails are \
                 separated along axis {sep_axis} — the steps are rotated"
            );
            // Where the rails' mean line is at this rung's height.
            let mid = |p: [f32; 3], q: [f32; 3]| {
                [
                    (p[0] + q[0]) * 0.5,
                    (p[1] + q[1]) * 0.5,
                    (p[2] + q[2]) * 0.5,
                ]
            };
            let (foot, head) = (mid(a.0, b.0), mid(a.1, b.1));
            let t = (at[1] - foot[1]) / (head[1] - foot[1]);
            assert!(
                (0.0..=1.0).contains(&t),
                "a rung at {at:?} is off the rails' height"
            );
            let x = foot[0] + t * (head[0] - foot[0]);
            let z = foot[2] + t * (head[2] - foot[2]);
            assert!(
                (at[0] - x).abs() < 0.02 && (at[2] - z).abs() < 0.02,
                "watch_post: a rung at {at:?} is off the rails' line, which is at [{x}, _, {z}] there"
            );
        });
        assert_eq!(rungs, RUNGS, "one rung per step");
    }

    /// The front (`-Z`) is open — no rail run across it — while the back and
    /// both sides carry their two rails each.
    #[test]
    fn the_railing_leaves_the_front_open() {
        let mut runs: Vec<[f32; 3]> = Vec::new();
        walk(&WatchPost.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, .. } = &g.kind
                && (size.0[1] - 0.07).abs() < 1e-4
                && size.0[0].max(size.0[2]) > 1.0
                && at[1] > DECK_TOP
                && at[1] < EAVE_Y - 0.3
            {
                runs.push(at);
            }
        });
        assert_eq!(runs.len(), 6, "two rails on each of three sides");
        assert!(
            runs.iter().all(|r| r[2] > -0.3),
            "watch_post: a rail run crosses the open front: {runs:?}"
        );
    }

    /// The lantern hangs under the eave at the open front, on a hanger that
    /// reaches the fascia (#972 lesson 27: "fixed to" is a relation).
    #[test]
    fn the_lantern_hangs_from_the_fascia_at_the_open_front() {
        let root = WatchPost.build("");
        let mut lantern: Option<([f32; 3], f32)> = None;
        let mut hanger: Option<([f32; 3], f32)> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, common, .. } = &g.kind {
                if common.material.emission_strength.0 > 1.0 {
                    lantern = Some((at, size.0[1]));
                } else if (size.0[0] - 0.04).abs() < 1e-4 && (size.0[2] - 0.04).abs() < 1e-4 {
                    hanger = Some((at, size.0[1]));
                }
            }
        });
        let (l, lh) = lantern.expect("a warning lantern");
        let (h, hh) = hanger.expect("a hanger");
        assert!(l[2] < -0.3, "the lantern at {l:?} is not at the open front");
        assert!(
            h[1] - hh * 0.5 <= l[1] + lh * 0.5 + 1e-4,
            "the hanger does not reach the lantern"
        );
        assert!(
            h[1] + hh * 0.5 >= EAVE_Y - FASCIA_T * 0.5,
            "the hanger does not reach the fascia"
        );
    }

    /// The deck is carried by four legs standing on the ground and reaching
    /// its underside.
    #[test]
    fn the_deck_is_carried_by_four_legs() {
        let mut legs = 0;
        walk(&WatchPost.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder { height, .. } = &g.kind
                && (height.0 - LEG_H).abs() < 1e-4
                && g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0]
            {
                legs += 1;
                assert!(
                    (at[1] - height.0 * 0.5).abs() < 1e-4,
                    "a leg at {at:?} is off the ground"
                );
                assert!(
                    at[1] + height.0 * 0.5 >= DECK_Y - DECK_T * 0.5,
                    "a leg stops short of the deck"
                );
            }
        });
        assert_eq!(legs, 4);
    }
}
