//! Owner's Transom — the Pirate identity monument (#975).
//!
//! A captured ship's stern board, nailed up on the harbour wall. Two sided
//! oak frames stand on a cobbled kerb and carry the transom; the room owner's
//! portrait fills its gilt-carved panel, a rope moulding runs round it, a
//! stern lantern hangs off each frame and a bronze prize plate is fixed
//! below. A coiled hawser and two round shot dress the base.
//!
//! The conceit is period-true and is why this subject was chosen over a
//! plaque or a statue: a ship's transom is *literally* where its name and
//! arms were carried, and nailing a taken vessel's stern board up ashore is
//! exactly what a prize crew did with it. So the monument is a frame that
//! already existed to hold an identity.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares, and
//! [`util::pfp_panel`](crate::catalogue::items::util::pfp_panel) for the ones
//! the panel itself enforces.
//!
//! # The two rules this file has to get right on its own
//!
//! * **The theme goes in the FRAME, never on the image** (#972 lesson 13).
//!   `base_color` multiplies the fetched portrait, so a themed tint over the
//!   panel stains the owner's face — and only once a picture loads, which is
//!   a state no render taken here can show. The gilt, the oak and the rope
//!   are what make this monument a pirate's; the panel stays pure white.
//! * **The standoff is derived, not picked** (#972 lessons 11 and 28). Ten of
//!   the twenty-four shipped monuments had the portrait buried inside the
//!   slab it was mounted on, every one of them from choosing the panel's `z`
//!   by eye. Here the transom's own half-depth sets it, so a deeper board
//!   cannot swallow the panel.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    bonded_siding, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim,
    quat_x, quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRONZE_GUN, GOLD_LEAF, HULL_OAK, IRON_BLACK, ROPE_HEMP, STONE_QUAY, bronze, cobbles, fx, hemp,
    iron, lantern, strake,
};

/// Side of the owner's portrait. Square, always — a face cannot survive being
/// stretched, which is why [`pfp_panel`] takes one scalar and not a pair.
const PANEL: f32 = 1.9;
/// Height of the portrait's centre. Well above head height, so nobody can
/// stand in front of the owner's face.
const PANEL_Y: f32 = 3.05;
/// The plane the portrait sits on. Hero side is `-Z`.
const PANEL_Z: f32 = -0.10;

/// Thickness of the transom board behind the portrait.
const TRANSOM_D: f32 = 0.18;
/// How far the portrait stands **proud** of the transom it is fixed to.
///
/// The whole standoff derivation hangs off this one number, and it is a
/// *reveal*, not a gap: a carved panel is recessed into its board and its
/// face still stands clear of the surrounding timber. Thirty millimetres is
/// comfortably more than the four the shared guard demands, which leaves room
/// for the board to be re-proportioned without the portrait sinking into it.
const PANEL_PROUD: f32 = 0.03;
/// Centre of the transom board — **derived** from the panel's plane, its own
/// depth and the reveal, so no hand-picked `z` can bury the portrait.
const TRANSOM_Z: f32 = PANEL_Z + TRANSOM_D * 0.5 + PANEL_PROUD;

/// Cobbled kerb footprint.
const KERB: [f32; 3] = [3.5, 0.32, 1.5];
/// Sided oak frame stock, and how far out from the axis the pair stands.
const FRAME: [f32; 3] = [0.26, 4.6, 0.32];
const FRAME_X: f32 = 1.34;

/// Kerb top — what everything stands on.
const DECK: f32 = KERB[1];
/// Frame top.
const FRAME_TOP: f32 = DECK + FRAME[1];

/// Gilt frame stock round the portrait.
const GILT: f32 = 0.16;

/// Radius of one round shot.
const SHOT_R: f32 = 0.115;

/// The frame post's inner face — what the ground dressing has to stay clear
/// of. Derived rather than eyeballed, because the first build put the hawser
/// coil straight through the post (#1025).
const POST_INNER: f32 = FRAME_X - FRAME[0] * 0.5;

/// Centre of the coiled hawser, and of the shot stack, on the two sides of
/// the monument. Both are pulled in from [`POST_INNER`] by their own outer
/// radius plus a margin, so neither can touch the timber.
const COIL_X: f32 = -(POST_INNER - 0.34 - 0.06 - 0.08);
const SHOT_X: f32 = POST_INNER - SHOT_R * 3.0 - 0.08;

pub struct PirateMonument;

impl CatalogueEntry for PirateMonument {
    fn slug(&self) -> &'static str {
        "pirate_monument"
    }
    fn name(&self) -> &'static str {
        "Owner's Transom"
    }
    fn description(&self) -> &'static str {
        "A captured ship's stern board on the harbour wall, gilt-framed and lantern-lit, \
         carrying the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let kerb_c = [0.0, DECK * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0x44);
    paving.uv_offset = crate::catalogue::items::util::face_uv_offset(FaceKey::Top, kerb_c);

    let mut carried = vec![
        footing(KERB[0], KERB[2], [0.0, 0.0], 2.6),
        oak_frame(-1.0),
        oak_frame(1.0),
        transom(did),
        // Coiled hawser at the foot — the dressing that says this board came
        // off a ship rather than out of a mason's yard.
        //
        // Held clear of the frame post on BOTH axes. At x = -1.06 the coil's
        // 0.40 m outer radius reached to -1.46 and the post's face is at
        // -1.21, so the rope ran straight through the timber it was supposed
        // to be lying beside. Derived from the post's own inner face now,
        // rather than from a number that happened to look right in plan.
        prim(
            torus(0.06, 0.34, hemp(ROPE_HEMP)),
            [COIL_X, DECK + 0.06, -0.28],
            id_quat(),
        ),
        prim(
            torus(0.055, 0.27, hemp(ROPE_HEMP)),
            [COIL_X, DECK + 0.16, -0.28],
            id_quat(),
        ),
    ];
    carried.extend(shot_stack([SHOT_X, DECK, -0.34]));

    let mut root = nest(
        prim(solid(cuboid_tapered(KERB, 0.0, paving)), kerb_c, id_quat()),
        carried,
    );
    // Signature life: the swell against the wall this board is nailed to.
    root.audio = fx::harbour_swell();
    root
}

/// A stack of round shot: **three on the ground carrying a fourth on top**.
///
/// The first build had two below and one above, which is a pile rather than a
/// stack — it has no plane of support, so the top ball reads as balanced
/// between two rather than seated in a hollow. Three in a triangle with a
/// fourth in the dimple is the arrangement a gunner's garland actually takes,
/// and it is the smallest one that looks like it would stay put.
///
/// The apex height is derived, not chosen: four equal spheres in contact form
/// a regular tetrahedron of edge `2r`, so the top centre sits `2r·sqrt(2/3)`
/// above the base centres. Pick it by eye and the ball either floats or sinks.
fn shot_stack(base: [f32; 3]) -> Vec<Generator> {
    // Base triangle: three touching spheres, so their centres are 2r apart
    // and the circumradius of that triangle is 2r/sqrt(3).
    let circum = SHOT_R * 2.0 / 3.0_f32.sqrt();
    let y = base[1] + SHOT_R;
    let mut out = Vec::new();
    for (i, k) in [0.0_f32, 1.0, 2.0].into_iter().enumerate() {
        let a = std::f32::consts::FRAC_PI_2 + k * std::f32::consts::TAU / 3.0;
        out.push(prim(
            sphere(SHOT_R, 3, iron(IRON_BLACK, 0x51 + i as u32)),
            [base[0] + circum * a.cos(), y, base[2] + circum * a.sin()],
            id_quat(),
        ));
    }
    out.push(prim(
        sphere(SHOT_R, 3, iron(IRON_BLACK, 0x54)),
        [base[0], y + SHOT_R * 2.0 * (2.0_f32 / 3.0).sqrt(), base[2]],
        id_quat(),
    ));
    out
}

/// One sided oak frame, with its lantern and its cap.
///
/// Sided (square-sectioned) rather than round, deliberately: a ship's frame
/// timber is sawn on two faces, and the round pile that suits a resort pier
/// would read as somebody else's kit here.
fn oak_frame(side: f32) -> Generator {
    let x = side * FRAME_X;
    let post_c = [x, DECK + FRAME[1] * 0.5, 0.0];
    // The two posts share one course frame, so their grain lines up across
    // the gap instead of each restarting at its own centre (#972 lesson 2e).
    let post = solid(cuboid_tapered(
        FRAME,
        0.03,
        bonded_siding(strake(HULL_OAK), FaceKey::SideNz, post_c),
    ));

    let lamp_y = DECK + 3.05;
    nest(
        prim(post, post_c, id_quat()),
        vec![
            // Iron cap band, so the end grain is not left open to the weather.
            prim(
                solid(cuboid_tapered(
                    [FRAME[0] + 0.06, 0.1, FRAME[2] + 0.06],
                    0.0,
                    iron(IRON_BLACK, 0x54),
                )),
                [x, FRAME_TOP + 0.05, 0.0],
                id_quat(),
            ),
            // Carved finial above the band.
            prim(
                sphere(0.11, 3, glow(GOLD_LEAF, 0.4)),
                [x, FRAME_TOP + 0.19, 0.0],
                id_quat(),
            ),
            // Bracket and lantern, held outboard so neither stands in front
            // of the portrait (#972 lesson 28: derive the standoff, then
            // check nothing is in front of it — including obliquely).
            prim(
                solid(cuboid_tapered(
                    [0.06, 0.06, 0.44],
                    0.0,
                    iron(IRON_BLACK, 0x55),
                )),
                [x, lamp_y + 0.36, -0.26],
                id_quat(),
            ),
            lantern([x, lamp_y, -0.42], 0.62, 0x56),
        ],
    )
}

/// The transom board itself: backing, portrait, gilt frame, rope moulding and
/// the prize plate under it.
fn transom(did: &str) -> Generator {
    let board = [PANEL + 0.86, PANEL + 0.92, TRANSOM_D];
    let board_c = [0.0, PANEL_Y + 0.05, TRANSOM_Z];
    let backing = solid(cuboid_tapered(
        board,
        0.0,
        bonded_siding(strake(HULL_OAK), FaceKey::SideNz, board_c),
    ));

    let gilt_z = PANEL_Z - GILT * 0.35;
    let mut carried = vec![pfp_panel(did, PANEL, [0.0, PANEL_Y, PANEL_Z])];

    // Gilt carved frame — four bars clear of the portrait's own square, so
    // nothing stands in front of the face. Low emission: gold leaf catches
    // the lantern, it does not emit.
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            solid(cuboid_tapered(
                [GILT, PANEL + GILT * 2.0, 0.14],
                0.0,
                glow(GOLD_LEAF, 0.45),
            )),
            [sx * (PANEL + GILT) * 0.5, PANEL_Y, gilt_z],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        carried.push(prim(
            solid(cuboid_tapered(
                [PANEL, GILT, 0.14],
                0.0,
                glow(GOLD_LEAF, 0.45),
            )),
            [0.0, PANEL_Y + sy * (PANEL + GILT) * 0.5, gilt_z],
            id_quat(),
        ));
    }

    // Rope moulding round the gilt, in hemp — the detail that makes the frame
    // maritime rather than merely gilded. Leaf prims, so their turns carry
    // nothing (#972 lesson 22).
    let rope_r = (PANEL + GILT * 2.0) * 0.5;
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            cylinder_tapered(0.045, PANEL + GILT * 2.0, 8, 0.0, hemp(ROPE_HEMP)),
            [sx * rope_r, PANEL_Y, gilt_z - 0.09],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        carried.push(prim(
            cylinder_tapered(0.045, PANEL + GILT * 2.0, 8, 0.0, hemp(ROPE_HEMP)),
            [0.0, PANEL_Y + sy * rope_r, gilt_z - 0.09],
            quat_z(FRAC_PI_2),
        ));
    }

    // Carved cresting over the board — a scroll and a pair of volutes, which
    // is what a stern board of the period actually carried above its arms.
    carried.push(prim(
        solid(cuboid_tapered(
            [1.5, 0.2, 0.12],
            0.35,
            glow(GOLD_LEAF, 0.45),
        )),
        [0.0, board_c[1] + board[1] * 0.5 + 0.1, PANEL_Z + 0.02],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            torus(0.05, 0.17, glow(GOLD_LEAF, 0.45)),
            [
                sx * 0.92,
                board_c[1] + board[1] * 0.5 + 0.06,
                PANEL_Z + 0.02,
            ],
            quat_x(FRAC_PI_2),
        ));
    }

    // Bronze prize plate under the portrait, clear of its square.
    carried.push(prim(
        solid(cuboid_tapered(
            [1.55, 0.32, 0.07],
            0.0,
            bronze(BRONZE_GUN, 0x57),
        )),
        [0.0, PANEL_Y - PANEL * 0.5 - 0.42, PANEL_Z - 0.02],
        id_quat(),
    ));

    nest(prim(backing, board_c, id_quat()), carried)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_owner_panel,
        assert_sanitize_stable,
    };

    const DID: &str = "did:plc:test";

    fn built() -> Generator {
        PirateMonument.build(DID)
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "pirate_monument");
    }

    /// The whole #975 contract in one call: one square panel pointed at this
    /// room's owner, white-tinted, unlit, single-sided, stood up the right way
    /// round, at monument scale, with something behind it and nothing in
    /// front.
    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&PirateMonument, DID);
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "pirate_monument");
    }

    #[test]
    fn nothing_solid_wears_a_window_card() {
        assert_no_glazing_on_solids(&built(), "pirate_monument");
    }

    /// The standoff is a derivation, not a number (#972 lessons 11 and 28).
    ///
    /// Stated against the *constants* as well as the built tree, because this
    /// is the failure that shipped on ten of twenty-four monuments and the
    /// render tool can never show it: it fetches no image, so a buried panel
    /// and a working one look identical here.
    #[test]
    fn the_portrait_stands_proud_of_the_board_it_is_fixed_to() {
        let transom_front = TRANSOM_Z - TRANSOM_D * 0.5;
        assert!(
            (transom_front - (PANEL_Z + PANEL_PROUD)).abs() < 1e-6,
            "the transom's front face is at {transom_front}, which is not the \
             panel's plane {PANEL_Z} plus its {PANEL_PROUD} m reveal — the \
             standoff has stopped being derived"
        );
    }

    /// The reveal clears the shared guard's own tolerance with room to spare.
    /// A `const` block, so re-proportioning the board fails the build rather
    /// than the suite.
    const _: () = assert!(
        PANEL_PROUD > 0.004,
        "the portrait's reveal is inside the shared burial guard's tolerance"
    );

    /// Nothing the monument carries stands in the cone between a viewer and
    /// the owner's face.
    ///
    /// The shared guard checks only *boxes* that cover the panel centre, which
    /// leaves the two lanterns — cylinders, mounted forward of the board —
    /// unexamined. They are the pieces most likely to creep inward as the
    /// frame is retuned, and a lantern hanging across somebody's chin is the
    /// exact class of fault the render cannot show.
    #[test]
    fn nothing_hangs_across_the_owners_face() {
        let g = built();
        let half = PANEL * 0.5;
        for p in measure::solids(&g) {
            let b = &p.bounds;
            // Only things standing in front of the portrait's plane.
            if b.max.z >= PANEL_Z {
                continue;
            }
            let overlaps_x = b.max.x > -half && b.min.x < half;
            let overlaps_y = b.max.y > PANEL_Y - half && b.min.y < PANEL_Y + half;
            assert!(
                !(overlaps_x && overlaps_y),
                "{} at {:?} stands in front of the portrait and overlaps it \
                 ({:?}..{:?} in X, {:?}..{:?} in Y)",
                p.kind_tag,
                b.center(),
                b.min.x,
                b.max.x,
                b.min.y,
                b.max.y
            );
        }
    }

    /// The board is carried by an unbroken pair of frames (#972 lesson 33):
    /// the posts stand on the kerb, the transom is inside their height, and
    /// the portrait is inside the transom.
    #[test]
    fn the_transom_is_carried_by_the_frames_it_hangs_between() {
        let board_h = PANEL + 0.92;
        let board_top = PANEL_Y + 0.05 + board_h * 0.5;
        let board_bottom = PANEL_Y + 0.05 - board_h * 0.5;
        assert!(
            board_top < FRAME_TOP,
            "the transom tops out at {board_top}, above the {FRAME_TOP} frames \
             that are supposed to be holding it"
        );
        assert!(
            board_bottom > DECK,
            "the transom's bottom edge at {board_bottom} is below the kerb top"
        );
        // The frames stand outboard of the board's own width, or they are
        // buried in it rather than carrying it.
        let board_half = (PANEL + 0.86) * 0.5;
        assert!(
            FRAME_X + FRAME[0] * 0.5 > board_half,
            "the frames at ±{FRAME_X} are inside the board's own {board_half} m \
             half-width, so they read as battens rather than as posts"
        );
    }

    /// The ground dressing keeps clear of the frame posts (#1025).
    ///
    /// The hawser coil ran straight through the port post in-world — a torus
    /// whose outer radius reached 0.25 m past the timber's inner face. It is
    /// the coplanar family's other half: not two faces fighting for depth, but
    /// two solids simply occupying the same space, which no still shows
    /// because the rope is the same tone as the post behind it.
    ///
    /// Checked against the built tree rather than the constants, so a coil
    /// re-sized without moving cannot pass (#972 lesson 21).
    #[test]
    fn the_ground_dressing_clears_the_frame_posts() {
        let g = built();
        let posts: Vec<_> = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                let s = p.bounds.size();
                (s.y - FRAME[1]).abs() < 0.2 && s.x < 0.5
            })
            .collect();
        assert_eq!(
            posts.len(),
            2,
            "expected two frame posts, found {}",
            posts.len()
        );
        for part in measure::solids(&g) {
            // Only the dressing on the ground; the transom and its gilt are
            // supposed to run between the posts.
            if part.bounds.center().y > DECK + 0.9 {
                continue;
            }
            for post in &posts {
                let overlaps = |lo_a: f32, hi_a: f32, lo_b: f32, hi_b: f32| {
                    hi_a > lo_b + 1e-3 && hi_b > lo_a + 1e-3
                };
                let hit = overlaps(
                    part.bounds.min.x,
                    part.bounds.max.x,
                    post.bounds.min.x,
                    post.bounds.max.x,
                ) && overlaps(
                    part.bounds.min.z,
                    part.bounds.max.z,
                    post.bounds.min.z,
                    post.bounds.max.z,
                ) && overlaps(
                    part.bounds.min.y,
                    part.bounds.max.y,
                    post.bounds.min.y,
                    post.bounds.max.y,
                );
                assert!(
                    !hit || std::ptr::eq(&part.bounds, &post.bounds),
                    "{} at {:?} runs into a frame post at {:?}",
                    part.kind_tag,
                    part.bounds.center(),
                    post.bounds.center()
                );
            }
        }
    }

    /// The shot stack is three carrying a fourth, and the fourth is *seated*.
    ///
    /// The apex height is the tetrahedral one — `2r·sqrt(2/3)` above the base
    /// centres — so it rests in the dimple the three below it make. A ball
    /// placed by eye either floats above the hollow or sinks into it, and at
    /// this scale both read as wrong without being obviously wrong.
    #[test]
    fn the_shot_is_a_stack_of_three_carrying_a_fourth() {
        // Selected by what DEFINES a round shot — an IRON sphere — not by its
        // size. The first draft matched on diameter alone and picked up the
        // two gilt post finials, which are spheres 5 mm smaller (#972 lesson
        // 24: suspect the selector before the content).
        fn iron_spheres(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let crate::pds::GeneratorKind::Sphere {
                radius, material, ..
            } = &g.kind
                && material.base_color.0 == IRON_BLACK
                && (radius.0 - SHOT_R).abs() < 1e-4
            {
                out.push(here);
            }
            for c in &g.children {
                iron_spheres(c, here, out);
            }
        }
        let g = built();
        let mut shot = Vec::new();
        iron_spheres(&g, [0.0; 3], &mut shot);
        assert_eq!(
            shot.len(),
            4,
            "expected four round shot, found {}",
            shot.len()
        );
        let base_y = shot.iter().map(|c| c[1]).fold(f32::MAX, f32::min);
        let ground: Vec<_> = shot
            .iter()
            .filter(|c| (c[1] - base_y).abs() < 1e-3)
            .collect();
        assert_eq!(ground.len(), 3, "the stack's base is not three balls");
        let apex = shot
            .iter()
            .find(|c| (c[1] - base_y).abs() > 1e-3)
            .expect("one ball sits on top");
        let want = base_y + SHOT_R * 2.0 * (2.0_f32 / 3.0).sqrt();
        assert!(
            (apex[1] - want).abs() < 1e-3,
            "the top shot is at {} where four touching spheres put it at {want}",
            apex[1]
        );
        // And it sits over the centre of the three, not off to one side.
        let cx = ground.iter().map(|c| c[0]).sum::<f32>() / 3.0;
        let cz = ground.iter().map(|c| c[2]).sum::<f32>() / 3.0;
        assert!(
            (apex[0] - cx).abs() < 1e-3 && (apex[2] - cz).abs() < 1e-3,
            "the top shot is not over the centroid of the three below it"
        );
    }

    /// Everything stands within the kerb it is nested under (#972 lesson 19).
    /// The cresting and the lantern brackets oversail on purpose, so the rule
    /// is applied to what is on the ground.
    #[test]
    fn every_ground_part_stands_within_the_kerb() {
        let g = built();
        let half = [KERB[0] * 0.5, KERB[2] * 0.5];
        for p in measure::solids(&g) {
            let c = p.bounds.center();
            if c.y > DECK + 1.0 {
                continue;
            }
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "a part at {c:?} overhangs the kerb in X"
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "a part at {c:?} overhangs the kerb in Z"
            );
        }
    }
}
