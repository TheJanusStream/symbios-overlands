//! Picket fence — a Suburban prop. A short run of white pointed pickets on
//! two rails between capped posts: the classic front-yard boundary.
//!
//! Reworked under #972 after an in-world check ("tip positions and
//! rotations are off and the texture's uv looks weird"). Every picket's
//! point was a four-sided [`cone`](crate::catalogue::items::util::cone), and a revolved prim's vertex 0 is on
//! `+X` (#972 lesson 35) — so each tip was a DIAMOND wider than its own
//! picket, its corners 90 mm out on both sides of a 50 mm board. The point
//! is now a box pinched to a line in `X` only ([`cuboid_tapered_xz`]), the
//! same thickness as the picket, sunk into its top. And the pickets wore the
//! kit's `Plank` at the identity, whose courses run up `V` — horizontal
//! bands across a vertical board, which reads as brick (lesson 15). Pickets
//! and posts now carry [`upright_boards`], the quarter turn that stands the
//! grain up; the rails, which are horizontal boards, keep the identity.
//!
//! Lesson 35's second half, made concrete: **a pointed picket is a
//! ridge, not a cone.** The point of a board is a triangle in the board's
//! own plane, pinched along the board's width and leaving its thickness
//! alone. Any revolved point on a flat board is wider than the board in the
//! thin direction, however its corners are turned.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, solid, upright_boards,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_WHITE, wood};

/// Post-to-post span.
const SPAN: f32 = 4.0;
const POST_W: f32 = 0.14;
const POST_H: f32 = 1.25;
/// Rails: two flat boards on edge between the posts, on `z = 0`.
const RAIL: [f32; 3] = [SPAN, 0.09, 0.04];
const RAIL_Y: [f32; 2] = [0.35, 0.85];
/// Pickets nailed to the rails' `-Z` face, clearing the grass.
const PICKET_W: f32 = 0.09;
const PICKET_T: f32 = 0.02;
const PICKET_H: f32 = 0.95;
const PICKET_LIFT: f32 = 0.15;
const PICKET_Z: f32 = -(RAIL[2] + PICKET_T) * 0.5;
/// The point: a ridge as tall as the board is wide, sunk into the top. The
/// pinch is the sanitiser's ceiling (`MAX_TORTURE_TAPER`), which leaves a
/// sub-millimetre flat rather than a true edge; anything higher is
/// rewritten on the way to the PDS.
const TIP_H: f32 = 0.09;
const TIP_PINCH: f32 = crate::pds::sanitize::limits::MAX_TORTURE_TAPER;
const TIP_SINK: f32 = 0.005;
const PICKETS: usize = 15;
/// Clear run the pickets are pitched over (inside the posts).
const RUN: f32 = SPAN - POST_W - 0.16;

pub struct PicketFence;

impl CatalogueEntry for PicketFence {
    fn slug(&self) -> &'static str {
        "picket_fence"
    }
    fn name(&self) -> &'static str {
        "Picket Fence"
    }
    fn description(&self) -> &'static str {
        "Run of white pointed pickets on two rails."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The `x` of picket `k`, pitched evenly over the clear run.
fn picket_x(k: usize) -> f32 {
    -RUN * 0.5 + RUN * k as f32 / (PICKETS - 1) as f32
}

fn build_tree() -> Generator {
    let horizontal = || wood(WOOD_WHITE);
    let upright = || upright_boards(wood(WOOD_WHITE));

    // Lower rail — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered(RAIL, 0.0, horizontal())),
        [0.0, RAIL_Y[0], 0.0],
        id_quat(),
    )];
    prims.push(prim(
        solid(cuboid_tapered(RAIL, 0.0, horizontal())),
        [0.0, RAIL_Y[1], 0.0],
        id_quat(),
    ));

    // End posts with square pyramid caps, aligned with the post.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * SPAN * 0.5;
        prims.push(prim(
            solid(cuboid_tapered([POST_W, POST_H, POST_W], 0.0, upright())),
            [x, POST_H * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [POST_W + 0.04, 0.08, POST_W + 0.04],
                0.9,
                horizontal(),
            )),
            [x, POST_H + 0.04 - TIP_SINK, 0.0],
            id_quat(),
        ));
    }

    // Pickets on the -Z face of the rails, each with its ridge point.
    for k in 0..PICKETS {
        let x = picket_x(k);
        let top = PICKET_LIFT + PICKET_H;
        prims.push(prim(
            solid(cuboid_tapered(
                [PICKET_W, PICKET_H, PICKET_T],
                0.0,
                upright(),
            )),
            [x, PICKET_LIFT + PICKET_H * 0.5, PICKET_Z],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [PICKET_W, TIP_H, PICKET_T],
                [TIP_PINCH, 0.0],
                upright(),
            )),
            [x, top - TIP_SINK + TIP_H * 0.5, PICKET_Z],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_sanitize_stable,
    };
    use crate::pds::{GeneratorKind, PrimCommon};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// The pickets: upright BOARDS — over 0.5 m tall, under 0.2 m wide and
    /// under 0.1 m thick, which admits the shipped 120 × 50 mm pickets and
    /// excludes the square posts (#972 lesson 24: a selector tuned to the
    /// new constants finds nothing in the old build and "bites" on a count).
    fn pickets(root: &Generator) -> Vec<([f32; 3], [f32; 3], f32)> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
                && size.0[1] > 0.5
                && size.0[0] < 0.2
                && size.0[2] < 0.1
            {
                out.push((at, size.0, material.uv_rotation.0));
            }
        });
        out
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PicketFence.build(""), "picket_fence");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&PicketFence.build(""), "picket_fence");
    }

    #[test]
    fn no_two_faces_tie_for_depth() {
        assert_no_coplanar_faces(&PicketFence.build(""), "picket_fence");
    }

    /// **Every point sits on its own picket and is no wider than it.** A
    /// point is whatever stands on a picket's top; it must be pinched in
    /// `X` only, match the board's thickness, and have its base inside the
    /// board. Against the shipped build this fails on the first cone: 180 mm
    /// across a 50 mm board.
    #[test]
    fn every_point_is_a_ridge_seated_in_its_picket() {
        let root = PicketFence.build("");
        let pickets = pickets(&root);
        assert_eq!(pickets.len(), PICKETS);
        let mut points = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let Some((pat, psize, _)) = pickets
                .iter()
                .find(|(p, s, _)| (p[0] - at[0]).abs() < 1e-4 && at[1] > p[1] + s[1] * 0.4)
            else {
                return;
            };
            // Anything standing on a picket's top is its point.
            let (w, h, t, pinch) = match &g.kind {
                GeneratorKind::Cuboid {
                    size,
                    common: PrimCommon { torture, .. },
                    ..
                } => (size.0[0], size.0[1], size.0[2], torture.taper.0),
                GeneratorKind::Cone { radius, height, .. } => {
                    (radius.0 * 2.0, height.0, radius.0 * 2.0, [1.0, 1.0])
                }
                _ => return,
            };
            points += 1;
            assert!(
                (t - psize[2]).abs() < 1e-4 && w <= psize[0] + 1e-4,
                "picket_fence: the point at {at:?} is {w} by {t} on a picket {} by {} — wider \
                 than the board it tops",
                psize[0],
                psize[2]
            );
            assert!(
                pinch[0] >= TIP_PINCH - 1e-4 && pinch[1].abs() < 1e-4,
                "picket_fence: the point at {at:?} pinches {pinch:?}; a board's point is a \
                 ridge, pinched in X alone"
            );
            let base = at[1] - h * 0.5;
            let top = pat[1] + psize[1] * 0.5;
            assert!(
                base < top && base > top - 0.02,
                "picket_fence: the point's base at {base} is not seated in the picket top at {top}"
            );
            assert!(
                (at[2] - pat[2]).abs() < 1e-4,
                "the point is off its picket's plane"
            );
        });
        assert_eq!(points, PICKETS, "one point per picket");
    }

    /// **Upright boards wear upright grain.** Every picket and post carries
    /// the quarter turn; the rails, which run horizontally, do not. Against
    /// the shipped build every picket is at the identity.
    #[test]
    fn vertical_boards_carry_the_quarter_turn_and_rails_do_not() {
        let root = PicketFence.build("");
        for (at, _, rot) in pickets(&root) {
            assert!(
                (rot - 90.0).abs() < 1e-4,
                "picket_fence: the picket at {at:?} has its plank courses running across it \
                 (uv_rotation {rot})"
            );
        }
        let mut rails = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
                && size.0[0] > 3.0
            {
                rails += 1;
                assert_eq!(material.uv_rotation.0, 0.0, "a rail at {at:?} is turned");
            }
        });
        assert_eq!(rails, 2);
    }

    /// The pickets clear the grass, stand proud on the `-Z` face of the
    /// rails, and are pitched evenly between the posts.
    #[test]
    fn pickets_clear_the_ground_and_sit_evenly_on_the_rails_front() {
        let root = PicketFence.build("");
        let mut ps = pickets(&root);
        ps.sort_by(|a, b| a.0[0].partial_cmp(&b.0[0]).unwrap());
        let pitch = ps[1].0[0] - ps[0].0[0];
        for w in ps.windows(2) {
            assert!(
                (w[1].0[0] - w[0].0[0] - pitch).abs() < 1e-4,
                "uneven picket pitch"
            );
        }
        for (at, size, _) in &ps {
            assert!(
                at[1] - size[1] * 0.5 > 0.05,
                "a picket at {at:?} is in the grass"
            );
            assert!(
                at[2] + size[2] * 0.5 <= -RAIL[2] * 0.5 + 1e-4,
                "a picket at {at:?} is not on the rails' -Z face"
            );
            assert!(
                at[0].abs() + size[0] * 0.5 < SPAN * 0.5 - POST_W * 0.5,
                "a picket is inside a post"
            );
        }
    }
}
