//! Handcart — a Medieval prop. A two-wheel oak cart with iron-shod spoked
//! wheels, parked with its shafts propped level under a cross-handle,
//! loaded with grain sacks, a small ale cask and a wicker basket: the
//! workaday transport of a market town.
//!
//! Reworked under #972 after an in-world check ("the push bar is rotated
//! wrongly; the cargo barrels intersect the cart walls"). There was no push
//! bar: the two shafts ended in mid-air over a vertical prop stick that
//! touched neither of them and read as a handle standing on end. The cask
//! lay across the bed 0.7 m long in a 0.92 m clear width, but centred
//! 0.28 m off the axis, so it ran out through one side board by 0.17 m —
//! and the basket did the same by 60 mm, and the cask ran into a sack. The
//! shafts now end in a [`strut`] handle from one shaft end to the other,
//! the prop stands under the handle, and the load is laid out on the bed
//! the way ground furniture is laid out on a slab (#972 lessons 8 and 19):
//! every piece inside the boards' clear, no two pieces sharing plan.
//!
//! #972 lesson 36: **cargo is ground furniture and the bed is its slab.**
//! A load authored by eye at round offsets is lesson 8's overhang with the
//! side board as the edge, and the symptom is a barrel through a wall,
//! which no four-angle sheet shows unless a tile looks along that board.
//! Guard it as two containments — every piece's plan box inside the clear,
//! and no two pieces' plan boxes overlapping — computed from each piece's
//! BUILT rotation, so a cask lying along `Z` is measured by its length in
//! `Z`. The cask's hoops were children of the turned cask at an offset
//! along its axis, which is lesson 22's shape exactly; they are siblings
//! now, turned the same way.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLOTH_CREAM, IRON_DARK, WOOD_DARK, WOOD_OAK, cloth, iron, timber};

const BED_Y: f32 = 0.72;
const BED: [f32; 3] = [2.0, 0.22, 1.1];
/// The bed's top — what the load stands on.
const BED_TOP: f32 = BED_Y + BED[1] * 0.5;
const BOARD_T: f32 = 0.08;
const BOARD_H: f32 = 0.4;
/// Side boards on `±SIDE_Z`, end boards on `±END_X`; the clear inside them
/// is what the load has to fit.
const SIDE_Z: f32 = 0.5;
const END_X: f32 = 1.0;
const CLEAR_Z: f32 = SIDE_Z - BOARD_T * 0.5;
const CLEAR_X: f32 = END_X - BOARD_T * 0.5;
const AXLE_Y: f32 = 0.5;
/// Hubs stand clear of the side boards (the shipped 0.62 put 20 mm of hub
/// inside the board).
const WHEEL_Z: f32 = 0.66;
/// Shafts reach forward (`+X`) to `SHAFT_END`, at `±SHAFT_Z`.
const SHAFT_Y: f32 = BED_Y - 0.05;
const SHAFT_Z: f32 = 0.4;
const SHAFT_LEN: f32 = 1.5;
const SHAFT_END: f32 = 0.95 + SHAFT_LEN;
/// The cross-handle sits just inside the shaft ends and stands proud of
/// them on both sides.
const HANDLE_X: f32 = SHAFT_END - 0.05;
const HANDLE_Z: f32 = SHAFT_Z + 0.07;
const HANDLE_R: f32 = 0.035;
/// Cask: two frustums belly to belly, hoops at the ends and the belly.
const CASK_X: f32 = -0.5;
const CASK_BELLY_R: f32 = 0.32;
const CASK_END_R: f32 = 0.28;
const CASK_HALF_LEN: f32 = 0.35;
const HOOP_T: f32 = 0.02;
/// Sacks either side of the axis against the side boards, and the basket
/// at the front against the end board — each placed from the board's own
/// clear with a stated gap (#972 lesson 8), not at a round number.
const LOAD_GAP: f32 = 0.02;
const SACK: [f32; 3] = [0.5, 0.5, 0.42];
const SACK_X: f32 = 0.15;
const SACK_Z: f32 = CLEAR_Z - SACK[2] * 0.5 - LOAD_GAP;
const BASKET_R: f32 = 0.22;
const BASKET_H: f32 = 0.34;
/// The basket flares by `BASKET_FLARE` at its rim (a negative taper).
const BASKET_FLARE: f32 = 0.08;
const BASKET_X: f32 = CLEAR_X - BASKET_R * (1.0 + BASKET_FLARE) - LOAD_GAP;

pub struct Handcart;

impl CatalogueEntry for Handcart {
    fn slug(&self) -> &'static str {
        "handcart"
    }
    fn name(&self) -> &'static str {
        "Handcart"
    }
    fn description(&self) -> &'static str {
        "Two-wheeled oak handcart with iron-shod spoked wheels, grain sacks and an ale cask."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// An iron-shod spoked wheel at `center`, axle lying along Z. The hub is the
/// subtree root (rotated so its axis runs along Z); the wooden rim, iron
/// tyre and six spokes are children in the wheel's local frame — all at the
/// hub's own origin, which is the one shape a turned parent may carry
/// (#972 lesson 22).
fn wheel(center: [f32; 3]) -> Generator {
    let mut w = prim(
        solid(cylinder_tapered(0.13, 0.2, 8, 0.0, iron(IRON_DARK))),
        center,
        quat_x(FRAC_PI_2),
    );
    // Wooden rim + proud iron tyre (both ring the wheel's local Y = its axle).
    w.children.push(prim(
        torus(0.07, 0.48, timber(WOOD_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w.children.push(prim(
        torus(0.04, 0.52, iron(IRON_DARK)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    // Six spokes = three diameter bars in the wheel plane (local XZ).
    for k in 0..3 {
        w.children.push(prim(
            solid(cuboid_tapered([0.92, 0.06, 0.06], 0.0, timber(WOOD_OAK))),
            [0.0, 0.0, 0.0],
            quat_y(k as f32 * FRAC_PI_3),
        ));
    }
    w
}

/// The ale cask lying across the bed with its axis along `Z`: two frustums
/// meeting at the belly (each one's narrow `+Y` end turned outward), plus
/// three hoops as siblings turned the same way. Every piece is a leaf.
fn cask(prims: &mut Vec<Generator>) {
    let cy = BED_TOP + CASK_BELLY_R;
    let taper = 1.0 - CASK_END_R / CASK_BELLY_R;
    for (sz, rot) in [(1.0_f32, quat_x(FRAC_PI_2)), (-1.0, quat_x(-FRAC_PI_2))] {
        prims.push(prim(
            solid(cylinder_tapered(
                CASK_BELLY_R,
                CASK_HALF_LEN,
                12,
                taper,
                timber(WOOD_DARK),
            )),
            [CASK_X, cy, sz * CASK_HALF_LEN * 0.5],
            rot,
        ));
    }
    // End hoops where the staves are already tapering, and a belly hoop.
    let end_z = CASK_HALF_LEN * 0.85;
    let end_r = CASK_BELLY_R - (CASK_BELLY_R - CASK_END_R) * 0.85;
    for (z, r) in [(-end_z, end_r), (0.0, CASK_BELLY_R), (end_z, end_r)] {
        prims.push(prim(
            torus(HOOP_T, r + HOOP_T * 0.25, iron(IRON_DARK)),
            [CASK_X, cy, z],
            quat_x(FRAC_PI_2),
        ));
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Cart bed — the root.
        prim(
            solid(cuboid_tapered(BED, 0.0, timber(WOOD_OAK))),
            [0.0, BED_Y, 0.0],
            id_quat(),
        ),
    ];
    // Side boards.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [BED[0], BOARD_H, BOARD_T],
                0.0,
                timber(WOOD_OAK),
            )),
            [0.0, BED_TOP + BOARD_H * 0.5 - 0.01, sz * SIDE_Z],
            id_quat(),
        ));
    }
    // Front and back boards.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [BOARD_T, BOARD_H, BED[2]],
                0.0,
                timber(WOOD_OAK),
            )),
            [sx * END_X, BED_TOP + BOARD_H * 0.5 - 0.01, 0.0],
            id_quat(),
        ));
    }

    // Axle and two spoked wheels.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 1.3, 8, 0.0, timber(WOOD_DARK))),
        [0.0, AXLE_Y, 0.0],
        quat_x(FRAC_PI_2),
    ));
    prims.push(wheel([0.0, AXLE_Y, WHEEL_Z]));
    prims.push(wheel([0.0, AXLE_Y, -WHEEL_Z]));

    // Two shafts reaching forward, joined by the cross-handle, with a prop
    // stick standing under the handle to hold them level.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [SHAFT_LEN, 0.09, 0.09],
                0.0,
                timber(WOOD_OAK),
            )),
            [SHAFT_END - SHAFT_LEN * 0.5, SHAFT_Y, sz * SHAFT_Z],
            id_quat(),
        ));
    }
    prims.push(strut(
        [HANDLE_X, SHAFT_Y, -HANDLE_Z],
        [HANDLE_X, SHAFT_Y, HANDLE_Z],
        HANDLE_R,
        8,
        timber(WOOD_DARK),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.09, SHAFT_Y, 0.09],
            0.0,
            timber(WOOD_DARK),
        )),
        [HANDLE_X, SHAFT_Y * 0.5, 0.0],
        id_quat(),
    ));

    // The load: a cask across the back of the bed, two sacks side by side,
    // a wicker basket at the front — each inside the boards' clear.
    cask(&mut prims);
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered(SACK, 0.35, cloth(CLOTH_CREAM, WOOD_OAK)),
            [SACK_X, BED_TOP + SACK[1] * 0.5, sz * SACK_Z],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(
            BASKET_R,
            BASKET_H,
            10,
            -BASKET_FLARE,
            timber(WOOD_OAK),
        )),
        [BASKET_X, BED_TOP + BASKET_H * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
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

    /// A prim's axis-aligned box in the prop's frame, from its BUILT
    /// rotation: each local half-extent is turned by the quaternion and the
    /// three absolute images summed per axis. A cask lying along `Z` is
    /// therefore measured by its length in `Z`, which is the whole point.
    /// Taper widens the reading to the prim's widest end.
    fn plan_box(g: &Generator, at: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
        let half = match &g.kind {
            GeneratorKind::Cuboid { size, .. } => {
                [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]
            }
            GeneratorKind::Cylinder {
                radius,
                height,
                common,
                ..
            } => {
                let t = common.torture.taper.0[0];
                let r = radius.0 * (1.0 - t.min(0.0));
                [r, height.0 * 0.5, r]
            }
            GeneratorKind::Torus {
                major_radius,
                minor_radius,
                ..
            } => {
                let r = major_radius.0 + minor_radius.0;
                [r, minor_radius.0, r]
            }
            _ => return None,
        };
        let q = g.transform.rotation.0;
        let mut ext = [0.0_f32; 3];
        for (axis, h) in half.iter().enumerate() {
            let mut v = [0.0; 3];
            v[axis] = *h;
            for (e, w) in ext.iter_mut().zip(rotate_by(q, v)) {
                *e += w.abs();
            }
        }
        Some((
            [at[0] - ext[0], at[1] - ext[1], at[2] - ext[2]],
            [at[0] + ext[0], at[1] + ext[1], at[2] + ext[2]],
        ))
    }

    /// Everything standing on the bed inside the boards: the load.
    fn load(root: &Generator) -> Vec<(&'static str, [f32; 3], [f32; 3])> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if at[1] < BED_TOP || at[0].abs() > CLEAR_X || at[2].abs() > CLEAR_Z {
                return;
            }
            if let Some((lo, hi)) = plan_box(g, at) {
                out.push((g.kind.kind_tag(), lo, hi));
            }
        });
        out
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Handcart.build(""), "handcart");
    }

    /// #972 lesson 22. The shipped cask carried its hoops as children offset
    /// along its own turned axis.
    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Handcart.build(""), "handcart");
    }

    /// **Nothing in the load pierces a board.** Every piece's plan box, from
    /// its built rotation, lies inside the clear between the boards. Against
    /// the shipped build the cask reaches `z = 0.63` through a board whose
    /// inner face is at `0.46`.
    #[test]
    fn the_load_sits_inside_the_boards() {
        let load = load(&Handcart.build(""));
        assert_eq!(
            load.len(),
            8,
            "two cask halves, three hoops, two sacks, a basket: {load:?}"
        );
        for (tag, lo, hi) in &load {
            assert!(
                lo[0] >= -CLEAR_X - 1e-4
                    && hi[0] <= CLEAR_X + 1e-4
                    && lo[2] >= -CLEAR_Z - 1e-4
                    && hi[2] <= CLEAR_Z + 1e-4,
                "handcart: a {tag} spanning {lo:?}..{hi:?} runs out through a board \
                 (clear is ±{CLEAR_X} by ±{CLEAR_Z})"
            );
        }
    }

    /// **No two pieces of the load share plan.** Hoops ring the cask by
    /// design and are left out; everything else is a solid that must not
    /// run into another. Against the shipped build the cask runs into a
    /// sack by 0.16 m.
    #[test]
    fn no_two_pieces_of_the_load_overlap() {
        let load: Vec<_> = load(&Handcart.build(""))
            .into_iter()
            .filter(|(tag, _, _)| *tag != "Torus")
            .collect();
        assert_eq!(load.len(), 5);
        for (i, (ta, alo, ahi)) in load.iter().enumerate() {
            for (tb, blo, bhi) in &load[i + 1..] {
                let apart = ahi[0] <= blo[0] + 1e-4
                    || bhi[0] <= alo[0] + 1e-4
                    || ahi[2] <= blo[2] + 1e-4
                    || bhi[2] <= alo[2] + 1e-4;
                // The two cask halves meet at the belly on purpose.
                let halves = *ta == "Cylinder"
                    && *tb == "Cylinder"
                    && (ahi[0] - bhi[0]).abs() < 1e-4
                    && alo[0] < 0.0;
                assert!(
                    apart || halves,
                    "handcart: a {ta} {alo:?}..{ahi:?} runs into a {tb} {blo:?}..{bhi:?}"
                );
            }
        }
    }

    /// Every solid piece of the load stands ON the bed — its underside on
    /// the bed's top, not floating in it or above it.
    #[test]
    fn the_load_stands_on_the_bed() {
        let load: Vec<_> = load(&Handcart.build(""))
            .into_iter()
            .filter(|(tag, _, _)| *tag != "Torus")
            .collect();
        assert_eq!(load.len(), 5);
        for (tag, lo, _) in &load {
            assert!(
                (lo[1] - BED_TOP).abs() < 0.005,
                "handcart: a {tag} has its underside at {} over a bed top at {BED_TOP}",
                lo[1]
            );
        }
    }

    /// **The push bar is a level cross-handle spanning both shafts**, read
    /// from the built strut's ends, and the prop stick stands under it.
    #[test]
    fn the_handle_spans_the_shafts_and_the_prop_stands_under_it() {
        let root = Handcart.build("");
        let mut handle: Option<([f32; 3], [f32; 3], f32)> = None;
        let mut shafts: Vec<([f32; 3], [f32; 3])> = Vec::new();
        let mut prop: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cylinder { height, radius, .. }
                if at[1] > AXLE_Y + 0.1
                    && height.0 > 0.8
                    && g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] =>
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                handle = Some((
                    [at[0] - tip[0], at[1] - tip[1], at[2] - tip[2]],
                    [at[0] + tip[0], at[1] + tip[1], at[2] + tip[2]],
                    radius.0,
                ));
            }
            GeneratorKind::Cuboid { size, .. } if size.0[0] > 1.2 && at[0] > 1.0 => {
                let (lo, hi) = plan_box(g, at).unwrap();
                shafts.push((lo, hi));
            }
            GeneratorKind::Cuboid { size, .. } if size.0[1] > 0.5 && at[0] > 1.0 => {
                prop = plan_box(g, at);
            }
            _ => {}
        });
        let (a, b, r) = handle.expect("a cross-handle");
        assert_eq!(shafts.len(), 2, "two shafts");
        assert!(
            (a[1] - b[1]).abs() < 1e-4 && (a[0] - b[0]).abs() < 1e-4,
            "handcart: the handle runs {a:?} -> {b:?}, which is not a level bar across the cart"
        );
        for (lo, hi) in &shafts {
            let (zlo, zhi) = (a[2].min(b[2]), a[2].max(b[2]));
            assert!(
                a[0] >= lo[0] && a[0] <= hi[0] && a[1] >= lo[1] && a[1] <= hi[1],
                "handcart: the handle at x {} y {} misses a shaft spanning {lo:?}..{hi:?}",
                a[0],
                a[1]
            );
            assert!(
                zlo <= lo[2] && zhi >= hi[2],
                "handcart: the handle spans z {zlo}..{zhi} and does not reach past the shaft at \
                 z {}..{}",
                lo[2],
                hi[2]
            );
        }
        let (plo, phi) = prop.expect("a prop stick");
        assert!(
            plo[1].abs() < 1e-3,
            "the prop stick does not stand on the ground"
        );
        assert!(
            phi[1] >= a[1] - r && plo[0] <= a[0] && phi[0] >= a[0],
            "handcart: the prop stick tops out at {} at x {}..{} — it holds up nothing (handle \
             underside {} at x {})",
            phi[1],
            plo[0],
            phi[0],
            a[1] - r,
            a[0]
        );
    }

    /// The wheels turn clear of the side boards: each tyre's inner face is
    /// outboard of its board's outer face.
    #[test]
    fn the_wheels_clear_the_side_boards() {
        let mut wheels = 0;
        walk(&Handcart.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder { height, .. } = &g.kind
                && (at[1] - AXLE_Y).abs() < 1e-4
                && !g.children.is_empty()
            {
                wheels += 1;
                let tyre_t = g
                    .children
                    .iter()
                    .filter_map(|c| match &c.kind {
                        GeneratorKind::Torus { minor_radius, .. } => Some(minor_radius.0),
                        _ => None,
                    })
                    .fold(0.0_f32, f32::max);
                let inner = at[2].abs() - tyre_t.max(height.0 * 0.5);
                assert!(
                    inner > SIDE_Z + BOARD_T * 0.5,
                    "handcart: a wheel's inner face at {inner} rubs the side board"
                );
            }
        });
        assert_eq!(wheels, 2);
    }
}
