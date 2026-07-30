//! Rum Tuns — the cargo a buccaneer harbour is actually about.
//!
//! Three tuns stowed on a stillage: two chocked side by side in the lower
//! course, the third nested in the valley between them and broached — bung
//! out, a tap driven in the head, a pail catching what comes. Lashings pass
//! over the stow to ring bolts in the paving, and the wreck of a fourth cask
//! lies in staves along one edge.
//!
//! # Why one of them is broached
//!
//! Three identical casks are a warehouse inventory. What makes this a *prop*
//! is the ONE that is open: a bung out, a tap in, and a pail under it — a
//! single asymmetry that says somebody has been at it. The same reasoning as
//! the [`super::quay_capstan`]'s two empty sockets and the battery's two
//! covered guns; the kit keeps arriving at it because a count of identical
//! things reads as a diagram.
//!
//! # Every cask rests on something it touches
//!
//! The lower pair sit on chocks whose height is *derived* from the cask's own
//! axis height and radius; the top cask's centre comes out of the two-circle
//! nesting geometry rather than off a guessed offset. That is the careening
//! slip's lesson (#1030) at prop scale: a shore placed at a plausible height
//! rather than a derived one left an eleven-metre hull standing on air, and a
//! cask is the same fault in miniature — with the aggravation that a floating
//! barrel is at eye level.
//!
//! Both derivations are guarded from the built prims, measured from the
//! opposite direction to the placement (#972 lesson 21).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, face_uv_offset, footing, id_quat, nest, prim, quat_x, quat_y,
    quat_z, solid, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_FITTING, DECK_HOLY, HULL_OAK, HULL_TAR, IRON_BLACK, PORT_BAND, ROPE_HEMP, STONE_QUAY,
    WHARF_GREY, board, bronze, cobbles, fx, hemp, iron, tar,
};

/// The paved stand — the sub-root every footprint guard measures against
/// (#972 lesson 19).
const PAD: [f32; 3] = [5.6, 0.24, 5.0];
const GROUND: f32 = PAD[1];

/// A tun's radius and length, laid on its side.
///
/// A tun proper is about 950 litres — a cask no one man shifts, which is the
/// whole reason a stillage and a set of chocks exist to put it on.
const TUN_R: f32 = 0.6;
const TUN_LEN: f32 = 1.44;

/// Where the two skids of the stillage cross under the casks, in `X`.
///
/// Inboard of the cask heads, so a cask is carried near its quarters rather
/// than at its ends — which is how it is actually chocked, and what keeps the
/// skids from projecting past the stow.
const SKID_X: f32 = TUN_LEN * 0.31;
const SKID_H: f32 = 0.2;
const SKID_W: f32 = 0.34;

/// How far a skid runs athwart the stand: the lower pair's spread plus a bearing
/// outboard of each of them, since a stillage that stops under its outermost
/// cask is not carrying it. Also the line the loose gear has to keep clear of —
/// which is why it is a constant and not written out twice.
const SKID_LEN: f32 = (LOWER_Z[1] - LOWER_Z[0]) + TUN_R * 2.4;

/// Chock height above the skid — how far the cradle rises to meet the cask.
const CHOCK_H: f32 = 0.22;

/// The lower course's axis height: skid top, chock, and the cask's radius.
const LOWER_Y: f32 = GROUND + SKID_H + CHOCK_H + TUN_R;

/// The lower pair's `Z` stations. Their separation also fixes how deep the
/// third cask settles into the valley between them — see [`upper_y`].
const LOWER_Z: [f32; 2] = [-0.66, 0.66];

/// Where the lashings are made fast, in `Z`. Outboard of the stow, on the
/// stones, at a ring bolt.
const RING_Z: f32 = 1.78;

/// Hero side — the render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

/// The rope a lashing is laid up in, and the radius its guard selects on.
const LASHING_R: f32 = 0.03;

pub struct RumTuns;

impl CatalogueEntry for RumTuns {
    fn slug(&self) -> &'static str {
        "rum_tuns"
    }
    fn name(&self) -> &'static str {
        "Rum Tuns"
    }
    fn description(&self) -> &'static str {
        "Three tuns stowed on a stillage, the top one broached with a tap driven and a pail beneath."
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
            clearance: 3.2,
            min_spawn_dist: 12.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Axis height of the cask nested in the valley between the lower pair.
///
/// Two cylinders of radius `TUN_R` whose centres are `d` apart carry a third
/// of the same radius at `sqrt((2r)² − (d/2)²)` above their own centre line —
/// the apex of an isosceles triangle with two sides of `2r`. Deriving it is
/// the point: a guessed offset either buries the top cask in the two below it
/// or leaves it hovering, and both are visible from every angle.
fn upper_y() -> f32 {
    let half_gap = (LOWER_Z[1] - LOWER_Z[0]) * 0.5;
    let span = TUN_R * 2.0;
    LOWER_Y + (span * span - half_gap * half_gap).max(0.0).sqrt()
}

/// One tun lying along `X`, with its hoops — a **flat** list.
///
/// Flat, not nested, and that is not a style choice: a cask on its side is a
/// rotated prim, and a rotated parent spins its children's offsets out of the
/// frame the record describes while every guard in this file walks
/// translations only (#972 lesson 22). The first build nested the hoops under
/// the cask and `assert_no_tilted_parents` caught it immediately.
///
/// `broached` drives the whole difference between stock and prop: the bung is
/// out of the top and a tap is driven into the head. The pail that catches it
/// belongs to the stones, so the caller places that.
fn tun(z: f32, y: f32, broached: bool, seed: u32) -> Vec<Generator> {
    // The cask itself. The negative taper is the bilge: a straight drum reads
    // as a bin, and the kit's other casks are bulged the same way.
    let mut out = vec![prim(
        solid(cylinder_tapered(TUN_R, TUN_LEN, 14, -0.12, board(HULL_OAK))),
        [0.0, y, z],
        quat_z(FRAC_PI_2),
    )];
    // Hoops at the chimes and the bilge — four is what makes a barrel read as
    // coopered rather than as a drum. Laid in the cask's own plane, which for
    // a cask lying along `X` is a quarter turn about `Z`: the kit's
    // convention, shared with the tavern's stillage and the magazine's
    // powder casks.
    out.extend([-0.62_f32, -0.24, 0.24, 0.62].into_iter().map(|f| {
        prim(
            torus(0.038, TUN_R * 0.98, iron(IRON_BLACK, seed)),
            [f * TUN_LEN * 0.5, y, z],
            quat_z(FRAC_PI_2),
        )
    }));

    if broached {
        // Bung out of the top of the bilge.
        out.push(prim(
            solid(cylinder_tapered(
                0.07,
                0.13,
                8,
                0.12,
                bronze(BRONZE_FITTING, seed ^ 0x5),
            )),
            [0.0, y + TUN_R * 0.94, z],
            id_quat(),
        ));
        // Tap driven low in the head, where a cask on its side actually
        // drains. Bronze rather than iron: iron taints spirit, which is the
        // same fact that gives the magazine's powder casks copper hoops.
        out.push(prim(
            solid(cylinder_tapered(
                0.042,
                0.3,
                8,
                0.16,
                bronze(BRONZE_FITTING, seed ^ 0x7),
            )),
            [TUN_LEN * 0.5 + 0.13, y - TUN_R * 0.42, z],
            quat_z(-FRAC_PI_2),
        ));
    }
    out
}

/// The chocks that carry one cask of the lower course.
///
/// Height is the distance from the skid top to the cask's own underside, so
/// the cradle reaches whatever the cask's radius or the skid's height become.
fn chocks(z: f32) -> Vec<Generator> {
    let base = GROUND + SKID_H;
    let h = (LOWER_Y - TUN_R - base).max(0.06);
    [-SKID_X, SKID_X]
        .into_iter()
        .map(|x| {
            prim(
                solid(cuboid_tapered(
                    [SKID_W, h, TUN_R * 1.35],
                    0.3,
                    board(WHARF_GREY),
                )),
                [x, base + h * 0.5, z],
                id_quat(),
            )
        })
        .collect()
}

/// The pail under the tap, and the dark spirit standing in it.
fn pail(at: [f32; 3], seed: u32) -> Vec<Generator> {
    vec![
        prim(
            solid(cylinder_tapered(0.19, 0.3, 12, -0.14, board(DECK_HOLY))),
            [at[0], GROUND + 0.15, at[2]],
            id_quat(),
        ),
        prim(
            torus(0.025, 0.185, iron(IRON_BLACK, seed)),
            [at[0], GROUND + 0.27, at[2]],
            id_quat(),
        ),
        // A hand's depth of spirit — dark, so the pail is not an empty
        // socket, and matte, because rum is not a mirror.
        prim(
            solid(cylinder_tapered(0.16, 0.05, 12, 0.0, tar(HULL_TAR))),
            [at[0], GROUND + 0.27, at[2]],
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0xF0);
    paving.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![footing(PAD[0] * 0.78, PAD[2] * 0.78, [0.0, 0.0], 3.2)];

    // The stillage: two skids running athwart, under the casks' quarters.
    for x in [-SKID_X, SKID_X] {
        carried.push(prim(
            solid(cuboid_tapered(
                [SKID_W, SKID_H, SKID_LEN],
                0.0,
                board(WHARF_GREY),
            )),
            [x, GROUND + SKID_H * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Lower course: two tuns, chocked and headed up.
    for (i, z) in LOWER_Z.into_iter().enumerate() {
        carried.extend(chocks(z));
        carried.extend(tun(z, LOWER_Y, false, 0xF1 + i as u32));
    }

    // Upper course: the broached one, nested between the two below it.
    let up_y = upper_y();
    carried.extend(tun(0.0, up_y, true, 0xF3));
    let tap_x = TUN_LEN * 0.5 + 0.13;
    carried.extend(pail([tap_x + 0.3, 0.0, 0.0], 0xF4));

    // Lashings over the stow, made fast to ring bolts in the paving. Struts,
    // so each leg runs between two points that both exist rather than
    // approximately toward one — the fault class this kit retired (#1028,
    // #1030).
    for x in [-SKID_X, SKID_X] {
        let over = [x, up_y + TUN_R * 0.92, 0.0];
        for sz in [-1.0_f32, 1.0] {
            let ring = [x, GROUND + 0.04, sz * RING_Z];
            carried.push(strut(over, ring, LASHING_R, 6, hemp(ROPE_HEMP)));
            carried.push(prim(
                torus(0.03, 0.1, iron(IRON_BLACK, 0xF5)),
                ring,
                quat_x(FRAC_PI_2),
            ));
        }
    }

    // The staves and one hoop of a cask that came apart on the quay, along
    // the far edge. Placed from the PAD's own half-extent and the stave's own
    // reach, so a retuned pad cannot leave them hanging off it
    // (#972 lesson 8).
    // Clear of the skid's own end rather than a comfortable-looking distance
    // from the pad's edge: laid off the edge instead, the outer two ran into
    // the stillage by 70 mm and the clearance guard caught it.
    let stave_z = SKID_LEN * 0.5 + 0.27;
    for (i, dx) in [-1.5_f32, -1.15, -0.92].into_iter().enumerate() {
        carried.push(prim(
            solid(cuboid_tapered([1.2, 0.05, 0.15], 0.12, board(HULL_OAK))),
            // Pitched clear of one another rather than heaped: three staves
            // sharing a footprint is one 500 mm plank as far as any silhouette
            // is concerned, and the clearance guard is the thing that says so.
            [dx, GROUND + 0.025, stave_z + i as f32 * 0.19],
            id_quat(),
        ));
    }
    // A second course laid across the first, because three 50 mm boards flat on
    // the paving have no silhouette at all — in the first render they were a
    // faint change of colour on the stones and nothing else. Crossing them puts
    // 100 mm of height and an edge under the light.
    for dx in [-1.5_f32, -1.05] {
        carried.push(prim(
            solid(cuboid_tapered([1.2, 0.05, 0.15], 0.12, board(HULL_OAK))),
            [dx, GROUND + 0.075, stave_z + 0.19],
            quat_y(FRAC_PI_2),
        ));
    }
    // The hoop off that cask, standing on its edge against the pile — a ring
    // lying flat is the same invisibility as a flat stave, and a hoop on edge
    // is what a cooper's yard actually looks like. Its centre height is its own
    // radius, so it stands ON the stones rather than near them.
    carried.push(prim(
        torus(0.035, 0.42, iron(IRON_BLACK, 0xF6)),
        [-1.85, GROUND + 0.42 + 0.035, stave_z + 0.1],
        quat_x(FRAC_PI_2),
    ));

    // A cooper's hammer where somebody set it down, and a measure by the tap.
    //
    // A hammer rather than the funnel this started as: the funnel, the measure
    // and the pail were three flared vessels of much the same size in a row,
    // which reads as a set of buckets and says nothing. One tool among them is
    // what makes the vessels read as vessels (#972 lesson 26 — a prop needs
    // things of different KINDS, not more of the same thing).
    carried.push(strut(
        [tap_x - 0.15, GROUND + 0.05, FRONT * 1.95],
        [tap_x + 0.55, GROUND + 0.05, FRONT * 1.8],
        0.035,
        6,
        board(DECK_HOLY),
    ));
    carried.push(prim(
        solid(cuboid_tapered(
            [0.17, 0.14, 0.14],
            0.0,
            iron(IRON_BLACK, 0xF7),
        )),
        [tap_x - 0.24, GROUND + 0.07, FRONT * 1.97],
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.13, 0.22, 10, -0.08, board(DECK_HOLY))),
        [tap_x + 0.75, GROUND + 0.11, FRONT * 1.55],
        id_quat(),
    ));
    // A coil of the lashings' slack.
    carried.push(prim(
        torus(0.05, 0.28, hemp(ROPE_HEMP)),
        [-tap_x - 0.2, GROUND + 0.05, -RING_Z + 0.1],
        id_quat(),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, paving)), pad_c, id_quat()),
        carried,
    );
    root.audio = fx::harbour_swell();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
        window_cards,
    };
    use crate::pds::GeneratorKind as K;

    fn built() -> Generator {
        RumTuns.build("")
    }

    /// Every cask, as world-space bounds — selected by the diameter that
    /// *defines* a tun rather than by height or position (#972 lesson 24),
    /// since the stow deliberately has casks at two different levels.
    fn casks() -> Vec<measure::SolidPiece> {
        measure::solids(&built())
            .into_iter()
            .filter(|p| p.kind_tag == "Cylinder" && (p.bounds.size().y - TUN_R * 2.0).abs() < 0.18)
            .collect()
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "rum_tuns");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "rum_tuns");
    }

    #[test]
    fn the_stow_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "rum_tuns");
        assert!(
            window_cards(&g).is_empty(),
            "a stack of casks has grown a window"
        );
    }

    /// The lower pair sit on chocks that actually reach them.
    ///
    /// Cask and cradle solve the same equation ([`LOWER_Y`] and [`chocks`]),
    /// so this is a guard on the derivation surviving: the moment one of them
    /// stops deriving, the cask stands on air. Measured off the built prims,
    /// from the chock's top up to the cask's underside.
    #[test]
    fn the_lower_casks_rest_on_their_chocks() {
        let solids = measure::solids(&built());
        let chocks: Vec<_> = solids
            .iter()
            .filter(|p| p.kind_tag == "Cuboid" && (p.bounds.size().x - SKID_W).abs() < 0.03)
            // The two skids share the chocks' width; they are the long ones.
            .filter(|p| p.bounds.size().z < TUN_R * 2.0)
            .collect();
        assert_eq!(
            chocks.len(),
            LOWER_Z.len() * 2,
            "expected two chocks per lower cask, found {}",
            chocks.len()
        );
        let lower: Vec<_> = casks()
            .into_iter()
            .filter(|c| c.bounds.center().y < upper_y() - TUN_R)
            .collect();
        assert_eq!(
            lower.len(),
            LOWER_Z.len(),
            "expected {} casks in the lower course, found {}",
            LOWER_Z.len(),
            lower.len()
        );
        for cask in &lower {
            let under = cask.bounds.min.y;
            let carrying: Vec<_> = chocks
                .iter()
                .filter(|c| (c.bounds.center().z - cask.bounds.center().z).abs() < TUN_R)
                .collect();
            assert_eq!(
                carrying.len(),
                2,
                "the cask at z = {} is carried by {} chocks",
                cask.bounds.center().z,
                carrying.len()
            );
            for c in carrying {
                assert!(
                    (c.bounds.max.y - under).abs() < 0.1,
                    "a chock tops out at {} where the cask it carries begins at \
                     {under} — the cask is resting on air",
                    c.bounds.max.y
                );
            }
        }
    }

    /// The top cask is seated in the valley between the lower two.
    ///
    /// "Seated" is two facts, and both matter: its underside is *below* the
    /// tops of the pair it rests on (so it has settled in rather than
    /// balanced on their crowns), and its axis is no further from either
    /// neighbour's than two radii (so it is touching them, not bridging a
    /// gap). Read from the built bounds rather than from [`upper_y`], which is
    /// the expression under test.
    #[test]
    fn the_top_cask_nests_between_the_pair() {
        let all = casks();
        let top = all
            .iter()
            .max_by(|a, b| {
                a.bounds
                    .center()
                    .y
                    .partial_cmp(&b.bounds.center().y)
                    .expect("finite")
            })
            .expect("a cask stands above the others");
        let lower: Vec<_> = all
            .iter()
            .filter(|c| c.bounds.center().y < top.bounds.center().y - 0.2)
            .collect();
        assert_eq!(
            lower.len(),
            2,
            "the top cask rests on {} casks",
            lower.len()
        );
        for c in &lower {
            assert!(
                top.bounds.min.y < c.bounds.max.y,
                "the top cask's underside at {} is above the crown of the cask \
                 at z = {} ({}) — it is balanced on air, not nested",
                top.bounds.min.y,
                c.bounds.center().z,
                c.bounds.max.y
            );
            let dy = top.bounds.center().y - c.bounds.center().y;
            let dz = top.bounds.center().z - c.bounds.center().z;
            let axis_gap = (dy * dy + dz * dz).sqrt();
            assert!(
                axis_gap <= TUN_R * 2.0 + 0.02,
                "the top cask's axis is {axis_gap} from its neighbour's — more \
                 than the two radii that would have them touching"
            );
        }
    }

    /// Exactly one cask is broached, and the pail stands under its tap.
    ///
    /// A count, because the single asymmetry is the whole read: two broached
    /// casks are a spill and none is stock. The bronze fittings up on the stow
    /// are the bung and the tap; the funnel is the same alloy but down on the
    /// stones, so height separates them.
    #[test]
    fn one_cask_is_broached_over_its_pail() {
        fn fittings(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { material, .. } = &g.kind
                && material.base_color.0 == BRONZE_FITTING
            {
                out.push(here);
            }
            for c in &g.children {
                fittings(c, here, out);
            }
        }
        let g = built();
        let mut found = Vec::new();
        fittings(&g, [0.0; 3], &mut found);
        let on_stow: Vec<_> = found
            .iter()
            .filter(|c| c[1] > GROUND + TUN_R)
            .copied()
            .collect();
        assert_eq!(
            on_stow.len(),
            2,
            "expected one broached cask — a bung and a tap — but found {} \
             fittings up on the stow",
            on_stow.len()
        );
        // The tap is the one clear of the cask's own heads.
        let tap = on_stow
            .iter()
            .find(|c| c[0].abs() > TUN_LEN * 0.5)
            .expect("the tap projects past a cask head");
        // The pail is selected by the hoop that *makes* it a pail — an iron
        // band round a wooden vessel down on the stones. Sizing it off the
        // bounding box instead found the bronze funnel, which is the same
        // diameter and stands beside it: #972 lesson 24, and the eighth time
        // this kit has paid for a selector keyed on something other than what
        // defines the thing.
        fn pails(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Torus {
                minor_radius,
                major_radius,
                ..
            } = &g.kind
                && here[1] < GROUND + 0.4
                && (major_radius.0 - 0.185).abs() < 0.01
                && minor_radius.0 < 0.04
            {
                out.push(here);
            }
            for c in &g.children {
                pails(c, here, out);
            }
        }
        let mut found_pails = Vec::new();
        pails(&g, [0.0; 3], &mut found_pails);
        assert_eq!(
            found_pails.len(),
            1,
            "expected one pail on the stones, found {}",
            found_pails.len()
        );
        let pail = found_pails[0];
        assert!(
            (pail[0] - tap[0]).abs() < 0.45 && (pail[2] - tap[2]).abs() < 0.35,
            "the pail at {pail:?} is not under the tap at {tap:?}"
        );
    }

    /// Every lashing runs from over the stow to a ring bolt in the paving.
    ///
    /// Read from the built struts via [`rotate_by`], because a rope of the
    /// right length pointing *near* its ring looks correct from three of four
    /// angles — the fault this kit paid for twice before `strut` existed.
    #[test]
    fn the_lashings_reach_their_ring_bolts() {
        fn ropes(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - LASHING_R).abs() < 0.003
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                ropes(c, here, out);
            }
        }
        let mut found = Vec::new();
        ropes(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            4,
            "expected two lashings of two legs each, found {}",
            found.len()
        );
        let crown = upper_y() + TUN_R;
        for (a, b) in &found {
            let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
            assert!(
                hi[1] > upper_y() && hi[1] <= crown + 0.02,
                "a lashing's upper end at {} is not on the top cask's crown \
                 (axis {}, crown {crown})",
                hi[1],
                upper_y()
            );
            assert!(
                hi[2].abs() < 0.05,
                "a lashing passes over z = {} — not over the cask it holds down",
                hi[2]
            );
            assert!(
                lo[1] < GROUND + 0.12,
                "a lashing's lower end at {} never reaches the stones",
                lo[1]
            );
            assert!(
                (lo[2].abs() - RING_Z).abs() < 0.02,
                "a lashing is made fast at z = {} rather than at its ring bolt",
                lo[2]
            );
        }
    }

    /// Nothing overhangs the stand it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_stands_on_the_pad() {
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&built()) {
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the stand in X ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the stand in Z ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
        assert!(checked > 15, "only {checked} parts examined");
    }

    /// The loose gear on the stones stands clear of the stillage.
    ///
    /// The intersection class this kit has now paid for four times — the
    /// tavern's coil through its own wall, the warehouse's bales through its
    /// foundation, the magazine's stowed bar and crate. Gear is checked against
    /// the *structure* it could bury itself in rather than pairwise against
    /// everything: a pail's hoop touches its own staves and its own spirit by
    /// design, and a guard that cannot tell that apart from a stave through a
    /// skid is a guard nobody will keep.
    ///
    /// The test is genuine **penetration**, not contact: a chock resting on a
    /// skid shares a plane with it, and that is what resting on something is.
    #[test]
    fn the_loose_gear_stands_clear_of_the_stow() {
        /// How far two boxes must interpenetrate on every axis before it is a
        /// fault rather than two things touching.
        const BITE: f32 = 0.02;
        let solids = measure::solids(&built());
        // Structure: the skids and the chocks, which share their width.
        let structure: Vec<_> = solids
            .iter()
            .filter(|p| p.kind_tag == "Cuboid" && (p.bounds.size().x - SKID_W).abs() < 0.03)
            .collect();
        assert_eq!(
            structure.len(),
            2 + LOWER_Z.len() * 2,
            "expected two skids and four chocks, found {}",
            structure.len()
        );
        // Gear: whatever else is resting on the paving and stays there. The
        // upper bound matters — a lashing comes down to a ring bolt, so its
        // *box* covers the whole stow even though the rope itself passes
        // nowhere near the skids, and an AABB test on a diagonal cylinder
        // cannot tell the difference.
        let gear: Vec<_> = solids
            .iter()
            .filter(|p| {
                p.bounds.min.y < GROUND + 0.06
                    && p.bounds.max.y < GROUND + TUN_R
                    && p.bounds.size().x < 3.0
            })
            .filter(|p| !structure.iter().any(|s| s.path == p.path))
            .collect();
        assert!(
            gear.len() >= 5,
            "only {} loose pieces found on the stones",
            gear.len()
        );
        for g in &gear {
            for s in &structure {
                let bite = |ax: usize| {
                    let (ga, gb) = (g.bounds.min.to_array(), g.bounds.max.to_array());
                    let (sa, sb) = (s.bounds.min.to_array(), s.bounds.max.to_array());
                    gb[ax].min(sb[ax]) - ga[ax].max(sa[ax])
                };
                assert!(
                    !(0..3).all(|ax| bite(ax) > BITE),
                    "{} at {:?} is driven into the stillage at {:?}",
                    g.kind_tag,
                    g.bounds.center(),
                    s.bounds.center()
                );
            }
        }
    }
}
