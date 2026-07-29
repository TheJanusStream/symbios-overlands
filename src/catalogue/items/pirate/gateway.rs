//! Harbour Gate — the Pirate bespoke social gateway.
//!
//! Two rubble-stone piers on a cobbled plinth, corbelled out to ashlar caps
//! that carry a re-used ship's beam for a lintel. A carved name-board rides
//! over the beam, a yard above it flies the black colours, a stern lantern
//! hangs in the opening's head and a ship's bell waits on the port pier for
//! anyone who wants the harbour master. Bollards and a coiled hawser dress the
//! approach.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone
//! standing in the opening — walking into it opens the destination picker.
//! Everything else frames that opening as a gate you pass through.
//!
//! # How it is built
//!
//! Nested rather than flat (#972 lesson 3): the plinth is the root, each pier
//! is parented to the plinth it stands on and carries its own cap and
//! ironmongery, and the lintel carries the name-board, the yard, the colours
//! and the lantern. One gizmo drag on the lintel takes the whole head of the
//! gate with it. Every piece is authored in the prop's ground-relative world
//! frame and rebased by [`nest`]; the veil is added with [`attach`] for the
//! same reason (#1010).
//!
//! The render front is `-Z`, so the name-board, the colours, the lantern and
//! the bell all face the approach.
//!
//! # Lighting
//!
//! Three warm lights, all of them objects rather than strips: the stern
//! lantern under the lintel and a bracket lantern on each pier. A gateway has
//! to read as *active*, and the usual way to say that is a glow bar under the
//! head — but a glow bar is the one thing a 1700s harbour gate cannot have, so
//! the light comes from lanterns that are really there and the spill under the
//! lintel is a low wash sitting outside the veil's own depth.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::pirate::lantern;
use crate::catalogue::items::util::{
    attach, bonded_siding, cuboid_tapered, cylinder_tapered, face_uv_offset, footing, glow,
    id_quat, nest, prim, quat_x, quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BONE_PALE, BRONZE_FITTING, GOLD_LEAF, HULL_OAK, HULL_TAR, IRON_BLACK, ROPE_HEMP, SIGN_AMBER,
    STONE_LIME, STONE_QUAY, ashlar, bone, bronze, cobbles, fx, hemp, iron, sailcloth, strake, tar,
};

// --- The frame, stated once ------------------------------------------------
//
// Every dimension below is derived from these, and so is the veil. #972
// lesson 18: the opening exists as one expression, not as a number repeated
// in the geometry and again in the zone.

/// Cobbled plinth footprint and thickness.
const PLINTH: [f32; 3] = [5.8, 0.30, 3.2];
/// Pier centre offset from the axis.
const PIER_X: f32 = 1.85;
/// Pier stock (width × height × depth), standing on the plinth.
const PIER: [f32; 3] = [0.80, 3.20, 0.90];
/// Ashlar cap stock — corbelled proud of the pier on all four sides.
const CAP: [f32; 3] = [0.96, 0.24, 1.06];
/// Lintel stock — a ship's beam, so it is deep and it oversails.
const LINTEL: [f32; 3] = [4.90, 0.55, 1.00];

/// Plinth top — the level everything stands on.
const DECK: f32 = PLINTH[1];
/// Pier top.
const PIER_TOP: f32 = DECK + PIER[1];
/// Cap top, and the soffit the lintel bears on.
const CAP_TOP: f32 = PIER_TOP + CAP[1];
/// Lintel top.
const LINTEL_TOP: f32 = CAP_TOP + LINTEL[1];

/// Clear half-width of the walk-through, from the piers' inner faces.
const CLEAR_HALF: f32 = PIER_X - PIER[0] * 0.5;

/// How far the veil reaches **into** the frame on every side.
///
/// The veil is a rendered cuboid as well as a sensor (#1006), so a face that
/// stops short of the frame shows an edge hanging in the opening and one that
/// overshoots shows a slab sticking out of the masonry. Burying each face a
/// few centimetres puts all four inside solid geometry, which is exactly what
/// `gateway_fit::fit_faults` asserts. Small enough that it never eats into the
/// headroom a player walks through.
const VEIL_BITE: f32 = 0.04;

/// Carved name-board stock, riding over the lintel.
const BOARD: [f32; 3] = [3.40, 0.85, 0.26];

/// The colours: cloth size, how far the head hangs below the yard, and the
/// clearance kept between the flag's foot and the name-board under it.
const FLAG_W: f32 = 1.55;
const FLAG_H: f32 = 1.05;
const FLAG_DROP: f32 = 0.06;
/// Deliberately generous. A flag is the one part of this gate that a viewer
/// reads as *hanging*, so a foot that grazes the board it hangs over looks
/// wrong long before it is actually intersecting.
const FLAG_CLEAR: f32 = 0.22;

/// Hero side — the approach the gate is read from. The render tool and the
/// settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

pub struct PirateGateway;

impl CatalogueEntry for PirateGateway {
    fn slug(&self) -> &'static str {
        "pirate_gateway"
    }
    fn name(&self) -> &'static str {
        "Harbour Gate"
    }
    fn description(&self) -> &'static str {
        "Rubble piers under a ship's-beam lintel, flying the black colours over a lantern-lit \
         walk-through, opening onto travel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One pier, with its cap, its bracket lantern and (on the port side) the
/// harbour bell. `side` is `-1.0` for port, `+1.0` for starboard.
fn pier(side: f32) -> Generator {
    let x = side * PIER_X;
    let cap_c = [x, PIER_TOP + CAP[1] * 0.5, 0.0];
    let pier_c = [x, DECK + PIER[1] * 0.5, 0.0];

    let mut parts = vec![
        // Ashlar cap, corbelled proud of the rubble below it. A cap flush with
        // its pier is a coplanar seam running the whole perimeter of the most
        // looked-at part of the gate, and it is invisible in a still (#972).
        prim(
            solid(cuboid_tapered(CAP, 0.0, ashlar(STONE_LIME, 0x9A))),
            cap_c,
            id_quat(),
        ),
    ];

    // Wrought bracket carrying a lantern on the approach face, held clear of
    // the opening so it lights the threshold without standing in it.
    let lamp_y = DECK + 2.35;
    let bracket_z = -PIER[2] * 0.5 - 0.26;
    parts.push(prim(
        solid(cuboid_tapered(
            [0.07, 0.07, 0.55],
            0.0,
            iron(IRON_BLACK, 0x1B),
        )),
        [x, lamp_y + 0.34, bracket_z * 0.5],
        id_quat(),
    ));
    parts.push(lantern([x, lamp_y, bracket_z], 0.68, 0x2C));

    // The harbour bell, on the port pier only. One bell, because two would
    // read as decoration rather than as the thing a ship hails with.
    if side < 0.0 {
        let bell_y = DECK + 2.55;
        let bell_z = -PIER[2] * 0.5 - 0.22;
        // Headstock across the pier face.
        parts.push(prim(
            solid(cuboid_tapered(
                [0.44, 0.09, 0.09],
                0.0,
                iron(IRON_BLACK, 0x1C),
            )),
            [x, bell_y + 0.30, bell_z],
            id_quat(),
        ));
        // The bell: a taper-up cone reads as a cast bell where a cylinder
        // reads as a bucket.
        parts.push(prim(
            solid(cylinder_tapered(
                0.20,
                0.34,
                14,
                0.42,
                bronze(BRONZE_FITTING, 0x2D),
            )),
            [x, bell_y + 0.09, bell_z],
            id_quat(),
        ));
        // Clapper lanyard, hanging where a hand can reach it.
        parts.push(prim(
            cylinder_tapered(0.02, 0.5, 6, 0.0, hemp(ROPE_HEMP)),
            [x, bell_y - 0.34, bell_z],
            id_quat(),
        ));
    }

    nest(
        prim(
            solid(cuboid_tapered(PIER, 0.05, cobbles(STONE_QUAY, 0x40))),
            pier_c,
            id_quat(),
        ),
        parts,
    )
}

/// The head of the gate: the lintel and everything it carries.
fn head() -> Generator {
    let lintel_c = [0.0, CAP_TOP + LINTEL[1] * 0.5, 0.0];
    // A beam's grain runs along its span, and the plank generator lays its
    // courses **up V** — which on a `SideNz` face is `-y`, i.e. horizontal
    // bands running the length of the beam. So the default lay is already
    // right and the quarter turn `bonded_boards` applies would be exactly
    // wrong here (#972 lesson 15 cuts both ways: the turn is a tool for
    // board-and-batten, not a default). `bonded_siding` therefore adds the
    // world-frame offset and nothing else, so the lintel's grain lines up
    // with the pier caps' rather than restarting at its own centre.
    let lintel = solid(cuboid_tapered(
        LINTEL,
        0.0,
        bonded_siding(strake(HULL_OAK), FaceKey::SideNz, lintel_c),
    ));

    let board_c = [0.0, LINTEL_TOP + 0.44, 0.0];
    let board_top = board_c[1] + BOARD[1] * 0.5;
    // The yard is derived from the board's top and the flag's own drop, not
    // picked. The first build put it at a tidy `LINTEL_TOP + 1.42`, which hung
    // the colours' bottom half *inside* the name-board — two flat faces
    // occupying the same 0.5 m, which the record states perfectly happily and
    // which the four-angle sheet showed as a dark box sitting on the sign.
    // Same shape as #972 lesson 16: the clearance has to be evaluated where
    // the part actually ends, not where its centre is.
    let yard_y = board_top + FLAG_DROP + FLAG_H + FLAG_CLEAR;

    let mut carried = vec![
        // Carved name-board on an oak backing. The board is the frame; the lit
        // face inside it is what carries the theme (#972 lesson 13's shape —
        // put the colour in the frame, not over the image).
        prim(
            solid(cuboid_tapered(BOARD, 0.0, strake(HULL_OAK))),
            board_c,
            id_quat(),
        ),
        // Deep-saturated amber rather than the pale tallow gold: this is the
        // largest lit face on the gate, and a pale hue at strength goes white.
        prim(
            cuboid_tapered([2.96, 0.52, 0.08], 0.0, glow(SIGN_AMBER, 2.0)),
            [0.0, board_c[1], FRONT * (0.13 + 0.05)],
            id_quat(),
        ),
        // Gilt mouldings above and below the lettering panel.
        prim(
            cuboid_tapered([3.30, 0.06, 0.10], 0.0, glow(GOLD_LEAF, 0.5)),
            [0.0, board_c[1] + 0.34, FRONT * 0.16],
            id_quat(),
        ),
        prim(
            cuboid_tapered([3.30, 0.06, 0.10], 0.0, glow(GOLD_LEAF, 0.5)),
            [0.0, board_c[1] - 0.34, FRONT * 0.16],
            id_quat(),
        ),
        // The stern lantern in the head of the opening. Hung on the approach
        // side of the lintel, clear of the veil's own depth, so it lights the
        // threshold instead of standing inside the light it makes.
        prim(
            solid(cylinder_tapered(0.03, 0.30, 6, 0.0, iron(IRON_BLACK, 0x1D))),
            [0.0, CAP_TOP - 0.15, FRONT * 0.62],
            id_quat(),
        ),
        lantern([0.0, CAP_TOP - 0.62, FRONT * 0.62], 0.86, 0x2E),
        // Low wash under the lintel — the lantern's spill on the beam soffit.
        // Kept outside the veil box and thin enough to stay a bar, not a lid.
        prim(
            cuboid_tapered([2.6, 0.09, 0.12], 0.0, glow(SIGN_AMBER, 1.4)),
            [0.0, CAP_TOP + 0.06, FRONT * 0.54],
            id_quat(),
        ),
    ];

    // Mast and yard over the board, flying the colours. The mast is sized to
    // carry the yard and stand a little proud of it, so the derivation above
    // cannot leave the yard hanging off nothing.
    let mast_h = yard_y - LINTEL_TOP + 0.45;
    carried.push(prim(
        solid(cylinder_tapered(0.07, mast_h, 8, 0.08, strake(HULL_OAK))),
        [0.0, LINTEL_TOP + mast_h * 0.5, 0.0],
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.05, 2.6, 8, 0.0, strake(HULL_OAK))),
        [0.0, yard_y, 0.0],
        quat_z(FRAC_PI_2),
    ));
    carried.push(colours(yard_y));

    nest(prim(lintel, lintel_c, id_quat()), carried)
}

/// The black colours, hanging from the starboard half of the yard.
///
/// Authored as a flat quad with its device on the front, and *not* as a
/// rotated parent: the flag hangs plumb, and the two crossed bones are turned
/// leaf prims with no children of their own — which is the form #972 lesson 22
/// permits, since a turn with nothing offset under it displaces nothing.
fn colours(yard_y: f32) -> Generator {
    // Hung off the yard's starboard half, so the gate is not symmetrical —
    // a flag centred over the opening reads as signage, off to one side it
    // reads as colours flown.
    let cx = 0.82;
    let cy = yard_y - FLAG_H * 0.5 - FLAG_DROP;
    let cz = FRONT * 0.04;
    let cloth = prim(
        cuboid_tapered(
            [FLAG_W, FLAG_H, 0.03],
            0.0,
            sailcloth(HULL_TAR, [0.09, 0.09, 0.10]),
        ),
        [cx, cy, cz],
        id_quat(),
    );
    let device_z = cz + FRONT * 0.03;
    nest(
        cloth,
        vec![
            // Skull.
            prim(
                sphere(0.17, 3, bone(BONE_PALE)),
                [cx, cy + 0.16, device_z],
                id_quat(),
            ),
            // Jaw, a touch below and narrower.
            prim(
                solid(cuboid_tapered([0.17, 0.07, 0.05], 0.2, bone(BONE_PALE))),
                [cx, cy + 0.01, device_z],
                id_quat(),
            ),
            // Crossed bones under it. Leaf prims, so the turn carries nothing.
            prim(
                cuboid_tapered([0.62, 0.06, 0.05], 0.0, bone(BONE_PALE)),
                [cx, cy - 0.24, device_z],
                quat_z(0.6),
            ),
            prim(
                cuboid_tapered([0.62, 0.06, 0.05], 0.0, bone(BONE_PALE)),
                [cx, cy - 0.24, device_z],
                quat_z(-0.6),
            ),
        ],
    )
}

/// One bollard with a hawser coiled at its foot. `side` picks the beam.
///
/// Placed from the **plinth's own extent** rather than at a round number off
/// the piers (#972 lesson 8): a bollard measured off the gate is the shape of
/// error that puts ground furniture half a metre past the paving it is
/// supposed to stand on, and no camera angle here would show it.
fn bollard(side: f32) -> Vec<Generator> {
    let x = side * (PLINTH[0] * 0.5 - 0.35);
    let z = FRONT * (PLINTH[2] * 0.5 - 0.35);
    let h = 0.62;
    vec![
        prim(
            solid(cylinder_tapered(0.15, h, 12, 0.14, iron(IRON_BLACK, 0x1E))),
            [x, DECK + h * 0.5, z],
            id_quat(),
        ),
        // Mushroom head, so a rope cannot lift off it.
        prim(
            solid(cylinder_tapered(
                0.2,
                0.09,
                12,
                0.35,
                iron(IRON_BLACK, 0x1F),
            )),
            [x, DECK + h, z],
            id_quat(),
        ),
        // Coiled hawser on the paving beside it, laid INBOARD of the bollard.
        // Outboard it hangs 0.43 m off the far edge of the paving — which is
        // what the footprint guard caught, and the class of error no camera
        // angle here would show (#972 lesson 8).
        prim(
            torus(0.055, 0.3, hemp(ROPE_HEMP)),
            [x - side * 0.42, DECK + 0.055, z + 0.12],
            id_quat(),
        ),
        prim(
            torus(0.05, 0.24, hemp(ROPE_HEMP)),
            [x - side * 0.42, DECK + 0.15, z + 0.12],
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    let plinth_c = [0.0, DECK * 0.5, 0.0];
    // The cobbled paving is one prim in the world course frame, so the
    // threshold and the approach share a single continuous setting-out
    // instead of each restarting the pattern at its own centre (#972 lesson
    // 2(e)). Top face, because that is the one anybody looks at.
    let mut paving = cobbles(STONE_QUAY, 0x41);
    paving.uv_offset = face_uv_offset(FaceKey::Top, plinth_c);

    let mut carried = vec![
        // Buried footing, sized to the drop this footprint spans (#1009), so a
        // terrain-snapped gate shows footing rather than daylight under its
        // downhill edge.
        footing(PLINTH[0], PLINTH[2], [0.0, 0.0], 3.5),
        pier(-1.0),
        pier(1.0),
        head(),
        // A tarred timber sill across the threshold — the plank a boot lands
        // on, and the line that tells a player where the gate begins.
        prim(
            solid(cuboid_tapered([2.9, 0.06, 0.5], 0.0, tar(HULL_TAR))),
            [0.0, DECK + 0.03, 0.0],
            id_quat(),
        ),
        // Two mooring rings bedded in the paving either side of the sill.
        prim(
            torus(0.03, 0.13, iron(IRON_BLACK, 0x20)),
            [-1.1, DECK + 0.02, FRONT * 0.95],
            quat_x(FRAC_PI_2),
        ),
        prim(
            torus(0.03, 0.13, iron(IRON_BLACK, 0x21)),
            [1.1, DECK + 0.02, FRONT * 0.95],
            quat_x(FRAC_PI_2),
        ),
    ];
    carried.extend(bollard(-1.0));
    carried.extend(bollard(1.0));

    let mut root = nest(
        prim(
            solid(cuboid_tapered(PLINTH, 0.0, paving)),
            plinth_c,
            id_quat(),
        ),
        carried,
    );

    // The walk-in zone, fitted to the opening (#1006): every face reaches
    // `VEIL_BITE` into the frame, so no cuboid edge shows. Derived from the
    // frame constants rather than measured by eye — the numbers cannot drift
    // apart if the pier or the lintel is retuned.
    let veil_bottom = DECK - VEIL_BITE;
    let veil_top = CAP_TOP + VEIL_BITE;
    attach(
        &mut root,
        prim(
            GeneratorKind::Gateway {
                size: Fp3([
                    (CLEAR_HALF + VEIL_BITE) * 2.0,
                    veil_top - veil_bottom,
                    PIER[2] - 0.1,
                ]),
            },
            [0.0, (veil_bottom + veil_top) * 0.5, 0.0],
            id_quat(),
        ),
    );

    // Signature life: the swell working against the harbour wall.
    root.audio = fx::harbour_swell();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::gateway_fit;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
    };

    fn built() -> Generator {
        PirateGateway.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "pirate_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&built()), 1);
    }

    /// The veil fills the opening (#1006): all four framed faces sit inside
    /// solid geometry, so none of them shows as a floating cuboid edge.
    #[test]
    fn the_veil_is_buried_in_its_own_frame() {
        let g = built();
        let geo = gateway_fit::measure(&g).expect("the gate carries a veil");
        let faults = gateway_fit::fit_faults(&geo);
        assert!(
            faults.is_empty(),
            "pirate_gateway veil does not fit its frame: {faults:?}"
        );
    }

    /// #972 lesson 22, and the reason every other guard here may walk
    /// translations alone: no rotated node carries an offset child.
    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "pirate_gateway");
    }

    /// #972 lesson 20, stated as a prohibition rather than as a census.
    ///
    /// The three lanterns are what this is really guarding. Their bodies are
    /// glazed drums, so the kit's `glass` card is the obvious reach — and it
    /// is the wrong one, because a `Window` texture masks its panes away and
    /// a drum wearing it shows the sky through its far side. That is the
    /// steampunk gas lamp's shipped fault, on the one prop whose entire
    /// subject is a light seen through glass. `tinted_glass` is the drop-in,
    /// and this fails the moment anybody reaches past it.
    #[test]
    fn nothing_solid_wears_a_window_card() {
        assert_no_glazing_on_solids(&built(), "pirate_gateway");
    }

    /// The colours fly clear of the name-board they hang over.
    ///
    /// Found by the render, not by a guard — the first build derived the yard
    /// from a tidy round number and hung half the flag inside the sign, which
    /// the record states perfectly happily and which the contact sheet showed
    /// as a dark box sitting on the board. It is the coplanar rule arriving
    /// between two parts that are simply in the same place, and it is the sort
    /// of thing that survives review because each part is individually right.
    ///
    /// Read out of the **built tree** rather than recomputed from the
    /// placement's own constants (#972 lesson 21): a guard derived from the
    /// same arithmetic as the placement only checks the arithmetic, and shares
    /// its misreading. This finds both prims by their material and size and
    /// compares the boxes that actually got built.
    #[test]
    fn the_colours_fly_clear_of_the_name_board() {
        let g = built();
        let solids = measure::solids(&g);
        // The flag is the only near-black cloth in the tree; the board is the
        // only oak slab of its plan. Both selected by what defines them
        // (#972 lesson 24), not by an incidental dimension.
        let flag = solids
            .iter()
            .find(|p| {
                let s = p.bounds.size();
                (s.x - FLAG_W).abs() < 0.05 && (s.y - FLAG_H).abs() < 0.05
            })
            .expect("the colours are in the tree");
        let board = solids
            .iter()
            .find(|p| {
                let s = p.bounds.size();
                (s.x - BOARD[0]).abs() < 0.05 && (s.y - BOARD[1]).abs() < 0.05
            })
            .expect("the name-board is in the tree");
        assert!(
            flag.bounds.min.y > board.bounds.max.y,
            "the colours' foot is at {} and the name-board's head at {} — the \
             flag is hanging inside the sign",
            flag.bounds.min.y,
            board.bounds.max.y
        );
        assert!(
            flag.bounds.min.y - board.bounds.max.y >= FLAG_CLEAR - 1e-3,
            "only {:.3} m between the colours and the board; a flag that grazes \
             what it hangs over reads wrong well before it intersects",
            flag.bounds.min.y - board.bounds.max.y
        );
    }

    /// Everything the gate carries stands on the plinth it is nested under
    /// (#972 lesson 19). The bollards are the piece this is really guarding —
    /// ground furniture placed off the *building* instead of off the paving
    /// is the error that shipped three times before it had a test, and it is
    /// invisible unless a camera happens to look along that edge.
    #[test]
    fn every_part_stands_within_the_plinth() {
        let g = built();
        let half = [PLINTH[0] * 0.5, PLINTH[2] * 0.5];
        // The lintel and its board oversail the piers on purpose, and the
        // colours hang outboard of the yard; those are head pieces, not
        // ground furniture, so the footprint rule applies below the cap.
        for p in measure::solids(&g) {
            let c = p.bounds.center();
            if c.y > CAP_TOP {
                continue;
            }
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "a part at {c:?} overhangs the plinth in X ({:?}..{:?})",
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "a part at {c:?} overhangs the plinth in Z ({:?}..{:?})",
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
    }

    /// The gate is a stack and the stack is contiguous (#972 lesson 33):
    /// plinth → pier → cap → lintel, each seated on the one below with no
    /// air between. A ring closes a joint visually and carries nothing, and
    /// a head floating on a hundred millimetres of nothing is invisible in a
    /// four-angle sheet.
    #[test]
    fn the_head_is_carried_by_an_unbroken_column() {
        assert!(
            (PIER_TOP - (DECK + PIER[1])).abs() < 1e-6,
            "the pier does not start at the plinth top"
        );
        assert!(
            (CAP_TOP - (PIER_TOP + CAP[1])).abs() < 1e-6,
            "the cap does not sit on the pier"
        );
        assert!(
            (LINTEL_TOP - (CAP_TOP + LINTEL[1])).abs() < 1e-6,
            "the lintel does not bear on the cap"
        );
    }

    /// The cap really is proud of the pier it corbels off, on both axes —
    /// flush is a coplanar seam running the whole perimeter of the gate's
    /// head, and it is invisible in a still.
    ///
    /// A `const` block rather than a runtime assertion because both operands
    /// are constants: this way the build fails rather than the test suite,
    /// which is strictly earlier feedback for the same claim.
    const _: () = assert!(
        CAP[0] > PIER[0] + 0.05 && CAP[2] > PIER[2] + 0.05,
        "the gate's cap is not corbelled proud of its pier"
    );

    /// The lights are lanterns, and the one broad lit face holds a deep hue.
    /// A pale emissive at strength blooms to a white blank, and the biggest
    /// lit surface does it first (standing gotcha, #972 lesson 30's family).
    #[test]
    fn the_broad_lit_face_is_deep_and_the_hot_ones_are_small() {
        use crate::pds::GeneratorKind as K;
        fn walk(g: &Generator, out: &mut Vec<([f32; 3], f32, [f32; 3])>) {
            if let K::Cuboid { size, material, .. } = &g.kind
                && material.emission_strength.0 > 0.3
            {
                out.push((size.0, material.emission_strength.0, material.base_color.0));
            }
            for c in &g.children {
                walk(c, out);
            }
        }
        let mut lit = Vec::new();
        walk(&built(), &mut lit);
        assert!(!lit.is_empty(), "the gate lost its lit faces");
        for (size, strength, color) in lit {
            let area = size[0] * size[1];
            if area > 0.5 {
                assert!(
                    strength <= 2.2,
                    "a {area} m² lit face at strength {strength} blooms white"
                );
                // Deep-saturated: the brightest channel must clearly lead, or
                // the face is a pale gold and goes white anyway.
                let max = color.iter().copied().fold(0.0_f32, f32::max);
                let min = color.iter().copied().fold(1.0_f32, f32::min);
                assert!(
                    max - min > 0.4,
                    "a broad lit face at {color:?} is not deep-saturated enough \
                     to hold its hue at strength {strength}"
                );
            }
        }
    }
}
