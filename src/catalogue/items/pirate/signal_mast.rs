//! Signal Mast — how a harbour talks to the roads.
//!
//! A pole mast stepped in a stone block, staked down by four shrouds, with a
//! yard across it, a hoist of signal flags run up the halyard, a gaff at the
//! head, and a fife rail at the foot where the falls are belayed.
//!
//! # This entry is almost entirely rigging
//!
//! Which is why it is worth being careful. Every prior fault in this kit's
//! rope and spar work — the careening slip's falls yawing off their posts,
//! its shores leaning away from the hull, its capstan bars snapping to the
//! nearest quarter turn, its mast built as its own mirror image — was a
//! hand-rolled conversion from "this runs from A to B" into a rotation. So
//! nothing here rolls one: every shroud, stay, yard and halyard is a
//! [`strut`], whose rotation is derived from the two points it spans and
//! therefore cannot disagree with them.
//!
//! # What makes it read at settlement distance
//!
//! A bare pole is a stick. Three things carry it: the SPREAD of the shrouds
//! (a triangle of stays says tension, a vertical line says nothing), the
//! YARD across it (which gives the silhouette a horizontal to read against),
//! and the flag HOIST — four small coloured cloths climbing the halyard,
//! which is the only part that says the mast is in use rather than derelict.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, face_uv_offset, footing, glow, id_quat, nest, prim, quat_x,
    solid, sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_FITTING, CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, ENSIGN_RED, GOLD_LEAF, HULL_OAK,
    IRON_BLACK, PORT_BAND, ROPE_HEMP, STONE_LIME, STONE_QUAY, ashlar, board, bronze, cobbles, fx,
    hemp, iron, jolly_roger, sailcloth,
};

/// The paved pad — the sub-root every footprint guard measures against.
const PAD: [f32; 3] = [7.6, 0.26, 7.6];
const GROUND: f32 = PAD[1];

/// The stone step the mast is stepped in, and its top.
const STEP: [f32; 3] = [1.6, 0.5, 1.6];
const DECK: f32 = GROUND + STEP[1];

/// Mast height above its step, and its stock at the heel.
const MAST_H: f32 = 9.0;
const MAST_R: f32 = 0.19;
/// Masthead — where the gaff, the truck and the halyard blocks live.
const HEAD: f32 = DECK + MAST_H;

/// Where the shrouds are made fast: how far out, and how high up the mast.
///
/// The SPREAD is the whole read. Shrouds led to the mast's own foot would be
/// four vertical sticks saying nothing; taken well out to their own blocks
/// they make four visible triangles, which is what tension looks like.
const SHROUD_OUT: f32 = 2.9;
const SHROUD_UP: f32 = 0.62;

/// The yard: how far up it crosses, and how far it reaches each side.
const YARD_UP: f32 = 0.58;
const YARD_HALF: f32 = 2.4;

/// Fife rail at the foot — where the falls are belayed.
const RAIL_H: f32 = 1.05;
const RAIL_R: f32 = 1.1;

/// Hero side. The render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

pub struct SignalMast;

impl CatalogueEntry for SignalMast {
    fn slug(&self) -> &'static str {
        "signal_mast"
    }
    fn name(&self) -> &'static str {
        "Signal Mast"
    }
    fn description(&self) -> &'static str {
        "A staked signal mast with a yard, a hoist of flags on the halyard and the black colours \
         at the gaff."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.4,
            min_spawn_dist: 14.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The four shrouds, and the deadeyes they are set up to.
///
/// Struts from a point on the mast to a block on the pad. Both ends exist as
/// real geometry, so the rope is the run rather than an approximation of it.
fn shrouds() -> Vec<Generator> {
    let hitch = DECK + MAST_H * SHROUD_UP;
    let mut out = Vec::new();
    for i in 0..4 {
        let a = std::f32::consts::FRAC_PI_4 + i as f32 * FRAC_PI_2;
        let (c, s) = (a.cos(), a.sin());
        let foot = [SHROUD_OUT * c, GROUND + 0.36, SHROUD_OUT * s];
        // Deadeye block on the stones, which is what the shroud is set up to.
        out.push(prim(
            solid(cuboid_tapered(
                [0.34, 0.36, 0.34],
                0.18,
                ashlar(STONE_LIME, 0xC1 + i as u32),
            )),
            [foot[0], GROUND + 0.18, foot[2]],
            id_quat(),
        ));
        out.push(prim(
            torus(0.035, 0.12, iron(IRON_BLACK, 0xC5 + i as u32)),
            [foot[0], GROUND + 0.36, foot[2]],
            quat_x(FRAC_PI_2),
        ));
        // The shroud itself, from the block to its hitch on the mast.
        out.push(strut(
            [foot[0], foot[1], foot[2]],
            [MAST_R * c, hitch, MAST_R * s],
            0.04,
            6,
            hemp(ROPE_HEMP),
        ));
    }
    out
}

/// The yard, its lifts, and the flag hoist on the halyard.
fn yard_and_hoist() -> Vec<Generator> {
    let yard_y = DECK + MAST_H * YARD_UP;
    let mut out = vec![
        // The yard, laid athwartships as a strut so it is level by
        // construction rather than by a quarter turn that has to be right.
        strut(
            [-YARD_HALF, yard_y, 0.0],
            [YARD_HALF, yard_y, 0.0],
            0.09,
            8,
            board(HULL_OAK),
        ),
        // Truss holding it to the mast.
        prim(
            solid(cuboid_tapered(
                [0.5, 0.16, 0.3],
                0.0,
                iron(IRON_BLACK, 0xC9),
            )),
            [0.0, yard_y, 0.0],
            id_quat(),
        ),
    ];
    // Lifts from the yard arms to the masthead — the pair of diagonals that
    // stop the yard reading as a stick balanced on a pole.
    for sx in [-1.0_f32, 1.0] {
        out.push(strut(
            [sx * YARD_HALF * 0.94, yard_y, 0.0],
            [0.0, HEAD - 0.35, 0.0],
            0.03,
            6,
            hemp(ROPE_HEMP),
        ));
        // Brace pendant down to the fife rail, so the yard is worked from the
        // deck rather than floating in the rig.
        out.push(strut(
            [sx * YARD_HALF * 0.9, yard_y - 0.06, 0.0],
            [sx * RAIL_R * 0.8, DECK + RAIL_H, FRONT * 0.2],
            0.025,
            6,
            hemp(ROPE_HEMP),
        ));
    }

    // The halyard, and the hoist of four signal flags climbing it. This is
    // the only part that says the mast is IN USE, so it is the part that has
    // to read: four small cloths in alternating colours, on the hero side.
    let hal_lo = DECK + RAIL_H;
    let hal_hi = HEAD - 0.5;
    let hal_z = FRONT * 0.42;
    out.push(strut(
        [0.0, hal_lo, hal_z],
        [0.0, hal_hi, hal_z],
        0.022,
        6,
        hemp(ROPE_HEMP),
    ));
    // Spread between the YARD and the masthead, derived from both — not from
    // a fraction of the halyard's own length. At `0.42 + i·0.13` of the
    // halyard the lowest two flags came out a metre BELOW the yard, reading
    // against the spar instead of the sky, which is what the guard caught.
    // The constraint is "clear of the yard, under the truck", so those are
    // the two numbers it should be built from.
    let flags = 4;
    let flag_lo = yard_y + 0.7;
    let flag_hi = hal_hi - 0.5;
    for i in 0..flags {
        let t = (i as f32 + 0.5) / flags as f32;
        let y = flag_lo + (flag_hi - flag_lo) * t;
        let (warp, weft) = if i % 2 == 0 {
            (ENSIGN_RED, CANVAS_BONE)
        } else {
            (CANVAS_BONE, CANVAS_SHADE)
        };
        out.push(prim(
            solid(cuboid_tapered(
                [0.62, 0.44, 0.04],
                0.06,
                sailcloth(warp, weft),
            )),
            [0.36, y, hal_z],
            id_quat(),
        ));
    }
    out
}

/// The masthead: truck, gaff, and the black colours flying from it.
fn masthead() -> Vec<Generator> {
    let gaff_out = 1.5_f32;
    let gaff_up = 1.05_f32;
    let peak = [FRONT * gaff_out, HEAD - 0.2 + gaff_up, 0.0];
    vec![
        // Truck at the very head, with a gilt finial.
        prim(
            solid(cylinder_tapered(0.26, 0.14, 12, 0.2, board(DECK_HOLY))),
            [0.0, HEAD + 0.07, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.13, 3, glow(GOLD_LEAF, 0.4)),
            [0.0, HEAD + 0.22, 0.0],
            id_quat(),
        ),
        // The gaff, raking up and out on the hero side, as a strut from its
        // jaws at the mast to its peak.
        strut([0.0, HEAD - 0.45, 0.0], peak, 0.075, 8, board(HULL_OAK)),
        // Peak halyard back to the masthead, which is what holds the gaff up.
        strut(
            [peak[0], peak[1], peak[2]],
            [0.0, HEAD, 0.0],
            0.025,
            6,
            hemp(ROPE_HEMP),
        ),
        // The colours, bent to the gaff — the kit's shared assembly, taking
        // the attachment point so the luff laps the spar by construction.
        jolly_roger([peak[0] * 0.55, peak[1] - 0.22, peak[2]], 1.5, 1.0),
    ]
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0xC0);
    paving.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    // Bedded BELOW the pad's top rather than flush with it. At
    // `+0.04` the block's underside landed on GROUND exactly — two coplanar
    // faces across the whole footprint (#1028's family), which the guard
    // caught to the millimetre.
    const STEP_SINK: f32 = 0.09;
    let step_c = [0.0, DECK - (STEP[1] + STEP_SINK * 2.0) * 0.5, 0.0];
    let mut carried = vec![
        footing(PAD[0] * 0.6, PAD[2] * 0.6, [0.0, 0.0], 4.4),
        // Stone step, bedded 40 mm into the paving so its underside and the
        // pad's top are not one plane (#1028's coplanar family).
        prim(
            solid(cuboid_tapered(
                [STEP[0], STEP[1] + STEP_SINK * 2.0, STEP[2]],
                0.1,
                ashlar(STONE_LIME, 0xC0),
            )),
            step_c,
            id_quat(),
        ),
        // The mast: a strut from its heel in the step to its head, so nothing
        // about it is a hand-applied rotation.
        strut(
            [0.0, DECK - 0.1, 0.0],
            [0.0, HEAD, 0.0],
            MAST_R,
            10,
            board(HULL_OAK),
        ),
        // Iron heel band where it enters the step.
        prim(
            solid(cylinder_tapered(
                MAST_R + 0.06,
                0.18,
                12,
                0.0,
                iron(IRON_BLACK, 0xCA),
            )),
            [0.0, DECK + 0.09, 0.0],
            id_quat(),
        ),
    ];

    // Fife rail round the foot: four stanchions and a ring, where every fall
    // in the rig is belayed. It is what makes the ropes above look worked.
    for i in 0..4 {
        let a = std::f32::consts::FRAC_PI_4 + i as f32 * FRAC_PI_2;
        carried.push(prim(
            solid(cylinder_tapered(0.07, RAIL_H, 8, 0.08, board(DECK_HOLY))),
            [RAIL_R * a.cos(), DECK + RAIL_H * 0.5, RAIL_R * a.sin()],
            id_quat(),
        ));
        // A belaying pin through the rail, and a coil hung on it.
        carried.push(prim(
            solid(cylinder_tapered(
                0.03,
                0.26,
                6,
                0.0,
                bronze(BRONZE_FITTING, 0xCB),
            )),
            [RAIL_R * a.cos(), DECK + RAIL_H - 0.06, RAIL_R * a.sin()],
            quat_x(FRAC_PI_2),
        ));
    }
    carried.push(prim(
        torus(0.055, RAIL_R, board(DECK_HOLY)),
        [0.0, DECK + RAIL_H, 0.0],
        id_quat(),
    ));

    carried.extend(shrouds());
    carried.extend(yard_and_hoist());
    carried.extend(masthead());

    // A coil of spare halyard on the stones, clear of the step.
    carried.push(prim(
        torus(0.05, 0.3, hemp(ROPE_HEMP)),
        [-(STEP[0] * 0.5 + 0.75), GROUND + 0.05, STEP[2] * 0.5 + 0.6],
        id_quat(),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, paving)), pad_c, id_quat()),
        carried,
    );
    root.audio = fx::rigging_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
        blob_components, rotate_by, window_cards,
    };
    use crate::pds::GeneratorKind as K;

    fn built() -> Generator {
        SignalMast.build("")
    }

    /// Every cylinder in the tree, as its two world-space ends plus radius.
    fn cylinder_ends(g: &Generator, at: [f32; 3], out: &mut Vec<(f32, [f32; 3], [f32; 3])>) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let K::Cylinder { radius, height, .. } = &g.kind {
            let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
            out.push((
                radius.0,
                [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
            ));
        }
        for c in &g.children {
            cylinder_ends(c, here, out);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "signal_mast");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "signal_mast");
    }

    #[test]
    fn the_mast_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "signal_mast");
        assert!(
            window_cards(&g).is_empty(),
            "a signal mast has grown a window"
        );
    }

    /// Every shroud runs from a deadeye on the stones to a hitch on the mast
    /// — and the four SPREAD, which is what makes them read as tension.
    ///
    /// Read from the built struts via [`rotate_by`] (#972 lesson 21). The
    /// spread is the substance: four ropes led to the mast's own foot would
    /// satisfy any "does it touch at both ends" check and look like nothing
    /// at all.
    #[test]
    fn the_four_shrouds_reach_the_mast_and_spread_to_their_blocks() {
        let mut cyls = Vec::new();
        cylinder_ends(&built(), [0.0; 3], &mut cyls);
        let shrouds: Vec<_> = cyls
            .iter()
            .filter(|(r, _, _)| (r - 0.04).abs() < 0.004)
            .collect();
        assert_eq!(
            shrouds.len(),
            4,
            "expected four shrouds, found {}",
            shrouds.len()
        );
        for (_, a, b) in &shrouds {
            let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
            // Upper end on the mast's own axis at the hitch height.
            let hitch = DECK + MAST_H * SHROUD_UP;
            assert!(
                (hi[0].powi(2) + hi[2].powi(2)).sqrt() < MAST_R + 0.05,
                "a shroud's upper end at {hi:?} is not on the mast"
            );
            assert!(
                (hi[1] - hitch).abs() < 0.05,
                "a shroud hitches at y = {} not {hitch}",
                hi[1]
            );
            // Lower end well out from the mast — the spread.
            let out = (lo[0].powi(2) + lo[2].powi(2)).sqrt();
            assert!(
                out > MAST_H * 0.25,
                "a shroud is set up only {out} m out from a {MAST_H} m mast — \\
                 four near-vertical ropes read as nothing at all"
            );
            assert!(
                lo[1] < GROUND + 0.6,
                "a shroud's lower end at y = {} is not on the stones",
                lo[1]
            );
        }
    }

    /// The yard is level, crosses the mast, and is held by two lifts to the
    /// masthead.
    #[test]
    fn the_yard_is_level_and_carried_by_its_lifts() {
        let mut cyls = Vec::new();
        cylinder_ends(&built(), [0.0; 3], &mut cyls);
        let yard_y = DECK + MAST_H * YARD_UP;
        let (_, ya, yb) = cyls
            .iter()
            .find(|(r, _, _)| (r - 0.09).abs() < 0.005)
            .copied()
            .expect("the yard is in the tree");
        assert!(
            (ya[1] - yb[1]).abs() < 1e-3,
            "the yard runs {ya:?} to {yb:?} — it is not level"
        );
        assert!(
            (ya[0] - yb[0]).abs() > YARD_HALF,
            "the yard spans only {} — it is not athwartships",
            (ya[0] - yb[0]).abs()
        );
        // Two lifts, each from near a yard arm up to the masthead region.
        // Selected by radius AND by the run they make: a lift goes from a
        // yard arm UP to the masthead. Radius alone also matches the four
        // belaying pins, which are the same stock — the selector found six
        // lifts on a two-lift mast (#972 lesson 24, yet again).
        let lifts: Vec<_> = cyls
            .iter()
            .filter(|(r, a, b)| {
                let hi = a[1].max(b[1]);
                let lo_x = if a[1] > b[1] { b[0] } else { a[0] };
                (r - 0.03).abs() < 0.003 && hi > yard_y && lo_x.abs() > YARD_HALF * 0.5
            })
            .collect();
        assert_eq!(lifts.len(), 2, "expected two lifts, found {}", lifts.len());
        for (_, a, b) in &lifts {
            let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
            assert!(
                (hi[0].powi(2) + hi[2].powi(2)).sqrt() < 0.4,
                "a lift's upper end at {hi:?} is not at the masthead"
            );
            assert!(
                lo[0].abs() > YARD_HALF * 0.7,
                "a lift's lower end at {lo:?} is not out at a yard arm"
            );
        }
    }

    /// The flag hoist is on the halyard, above the yard, on the hero side.
    ///
    /// The hoist is the only part of the prop that says the mast is in use
    /// rather than derelict, so it is checked as a population rather than as
    /// "something exists": four cloths, all clear of the yard, all forward of
    /// the mast where they read against the sky.
    #[test]
    fn the_flag_hoist_climbs_the_halyard_clear_of_the_yard() {
        let g = built();
        let yard_y = DECK + MAST_H * YARD_UP;
        let mut flags = Vec::new();
        fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid { size, .. } = &g.kind
                && (size.0[0] - 0.62).abs() < 0.02
                && (size.0[1] - 0.44).abs() < 0.02
            {
                out.push((here, size.0));
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        walk(&g, [0.0; 3], &mut flags);
        assert_eq!(
            flags.len(),
            4,
            "expected four signal flags, found {}",
            flags.len()
        );
        for (c, _) in &flags {
            assert!(
                c[1] > yard_y,
                "a flag at y = {} is below the yard at {yard_y} — it reads \\
                 against the spar instead of the sky",
                c[1]
            );
            assert!(
                c[1] < HEAD,
                "a flag at y = {} is above the masthead at {HEAD}",
                c[1]
            );
            assert!(
                c[2] < 0.0,
                "a flag at z = {} is on the far side of the mast from the \\
                 approach",
                c[2]
            );
        }
    }

    /// The colours at the gaff are the kit's shared assembly, intact.
    ///
    /// Three blob groups (cloth, skull, bones), each one connected mass — the
    /// same contract the gate and the battery hold, checked here because this
    /// is a third caller and a shared helper's guarantees are only real where
    /// they are asserted.
    #[test]
    fn the_colours_at_the_gaff_are_three_connected_groups() {
        let g = built();
        let mut kinds = Vec::new();
        fn walk(g: &Generator, out: &mut Vec<K>) {
            if matches!(g.kind, K::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                walk(c, out);
            }
        }
        walk(&g, &mut kinds);
        assert_eq!(kinds.len(), 3, "the colours are cloth + skull + bones");
        for (i, k) in kinds.iter().enumerate() {
            assert_eq!(
                blob_components(k),
                1,
                "group {i} polygonised into more than one piece"
            );
        }
    }

    /// The mast is a stack: pad → step → heel → head, with the heel bedded in
    /// the step rather than standing on it (#972 lesson 33).
    #[test]
    fn the_mast_is_stepped_in_its_block() {
        let g = built();
        let mut cyls = Vec::new();
        cylinder_ends(&g, [0.0; 3], &mut cyls);
        let (_, a, b) = cyls
            .iter()
            .find(|(r, _, _)| (r - MAST_R).abs() < 0.005)
            .copied()
            .expect("the mast is in the tree");
        let (top, heel) = if a[1] > b[1] { (a, b) } else { (b, a) };
        assert!(
            heel[1] < DECK,
            "the mast's heel is at {} and the step's top at {DECK} — it is \\
             standing on the block, not stepped in it",
            heel[1]
        );
        assert!(
            (top[1] - HEAD).abs() < 0.05,
            "the masthead is at {} not {HEAD}",
            top[1]
        );
        let step = measure::solids(&g)
            .into_iter()
            .find(|p| (p.bounds.size().x - STEP[0]).abs() < 0.05)
            .expect("the step is in the tree");
        assert!(
            step.bounds.min.y < GROUND - 1e-3,
            "the step's underside is at {} and the pad's top at {GROUND} — \\
             those two faces are coplanar across the whole block",
            step.bounds.min.y
        );
    }

    /// Everything on the ground stands on the pad (#972 lessons 8, 19).
    #[test]
    fn every_ground_part_stands_on_the_pad() {
        let g = built();
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            if p.bounds.center().y > DECK + 0.5 {
                continue;
            }
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the pad in X",
                p.kind_tag,
                p.bounds.center()
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the pad in Z",
                p.kind_tag,
                p.bounds.center()
            );
        }
        assert!(checked > 8, "only {checked} ground parts examined");
    }
}
