//! Harbour Tavern — the buccaneer port's public house.
//!
//! A two-storey timber-framed tavern on a rubble plinth: a boarded false
//! front rising past the eaves, a first-floor gallery slung over the lane on
//! posts, three bays of leaded lights over a lit taproom, a door standing
//! open, and a painted sign swinging from a wrought bracket.
//!
//! # The false front, and why it is the silhouette
//!
//! A false front is a flat parapet carried up past the roof so the building
//! reads taller and squarer from the street than it is. It is the one piece
//! of architecture that says *frontier boom town* in any period, which is
//! also the risk: `wild_west::saloon` is built on the same device. The
//! separation is in everything behind it — this one is **ship-built**, clad
//! in [`strake`] rather than clapboard, on rubble rather than dust, with a
//! gallery and a hanging sign where the saloon has a boardwalk and a
//! porch. If the two ever start to read alike, the answer is more harbour
//! here, not less false front.
//!
//! # The glazing
//!
//! This is the entry the kit's [`glass`](super::glass) card was written for,
//! and it follows
//! the idiom exactly (#972 lessons 1, 6, 7 and 9): every card is a flat
//! `Plane` set back in a real reveal, lapped past the opening so no edge is
//! coplanar with the jamb, over an interior with something in it — and the
//! taproom is laid out **bay by bay**, because a shell with the furniture all
//! at one end has a black rectangle in every other bay.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    BALUSTER_PITCH, attach, bonded_boards, bonded_siding, cuboid_tapered, cuboid_tapered_xz,
    cylinder_tapered, face_uv_offset, footing, glow, id_quat, lit_interior, nest, plane, prim,
    quat_x, quat_y, quat_z, railing, solid, torus, tube, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_FITTING, DECK_HOLY, GLASS_AMBER, GOLD_LEAF, HULL_OAK, HULL_TAR, IRON_BLACK, OAK_JOINERY,
    PORT_BAND, ROPE_HEMP, SHINGLE_GREY, SIGN_AMBER, STONE_QUAY, WHARF_GREY, board, bronze, cobbles,
    fx, hemp, iron, lantern, pane_grid, shingle, strake,
};

// --- The building, stated once ---------------------------------------------

/// Cobbled apron the tavern stands on — the sub-root every footprint guard
/// measures against (#972 lesson 19).
const APRON: [f32; 3] = [15.0, 0.30, 13.0];
/// Apron top.
const GROUND: f32 = APRON[1];

/// Rubble plinth under the frame.
const PLINTH: [f32; 3] = [11.4, 0.42, 9.4];
/// Plinth top — the taproom floor.
const FLOOR: f32 = GROUND + PLINTH[1];

/// The framed body: width, ground-storey height, depth.
const BODY: [f32; 3] = [10.8, 3.30, 8.8];
/// Ground-storey head.
const STOREY_1: f32 = FLOOR + BODY[1];
/// Upper-storey height, and its head.
const UPPER_H: f32 = 2.90;
const EAVES: f32 = STOREY_1 + UPPER_H;

/// The hero plane — the street elevation. `-Z` is where the render tool and
/// the settlement placer both look from.
const FRONT_Z: f32 = -BODY[2] * 0.5;

/// Ground-floor opening: clear width and height, and its sill.
const WIN_W: f32 = 1.85;
const WIN_H: f32 = 1.55;
const SILL: f32 = FLOOR + 0.95;
/// Upper-floor opening — shorter, as an upper storey's windows are.
const UP_W: f32 = 1.45;
const UP_H: f32 = 1.25;
/// Upper sill, held clear of the gallery railing that stands in front of it.
///
/// Derived rather than picked. At a tidy `+0.72` the sill landed at 4.74 and
/// the railing on the gallery below topped out at 4.92, so the balustrade
/// crossed the bottom of every upper window — which is #972 lesson 24's fault
/// exactly: a rail in front of an opening hides the one thing the opening was
/// cut for. The sill now sits a clear margin above whatever the railing
/// reaches, so re-proportioning the gallery cannot reopen it.
const UP_SILL: f32 = GALLERY_Y + 0.07 + RAIL_H + 0.16;

/// Entrance opening.
const DOOR_W: f32 = 1.7;
const DOOR_H: f32 = 2.3;

/// How far an opening is recessed into the wall, and how far the card laps
/// past the hole it fills.
///
/// The reveal is what makes a window read as a hole rather than as a picture:
/// without it the card, the jamb and the wall are all on one plane. The lap
/// (#972 lesson 7) keeps the card's edges off the reveal's own planes — a
/// card sized exactly to its opening puts four edges into four coplanar ties,
/// and the frame is opaque so the overhang is never seen.
const REVEAL: f32 = 0.28;
const CARD_LAP: f32 = 0.06;

/// How far interior surfaces stay behind the wall face they meet.
///
/// A floor run out to `FRONT_Z` exactly puts its leading edge on the same
/// plane as the piers' front faces — coplanar along the whole sill line,
/// which is what showed in-world (#1028).
const FLOOR_INSET: f32 = 0.06;

/// How far the taproom's back lining stands in front of the rear wall.
///
/// #972 lesson 6: goods against the back wall of a 7 m room are unreadable
/// specks. Holding the fit-out forward is what makes the depth read.
const ROOM_BACK: f32 = FRONT_Z + 3.6;

/// Bay centres across the street elevation. Three bays: window, door, window.
const BAYS: [f32; 3] = [-3.3, 0.0, 3.3];

/// Gallery — the first-floor balcony slung over the lane.
const GALLERY_D: f32 = 1.9;
const GALLERY_Y: f32 = STOREY_1 - 0.12;
/// Gallery railing height, measured from its deck.
const RAIL_H: f32 = 0.95;

pub struct HarbourTavern;

impl CatalogueEntry for HarbourTavern {
    fn slug(&self) -> &'static str {
        "harbour_tavern"
    }
    fn name(&self) -> &'static str {
        "Harbour Tavern"
    }
    fn description(&self) -> &'static str {
        "A ship-built tavern under a boarded false front, its gallery over the lane and its \
         taproom lit behind leaded lights."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Ship-built boarding in the shared world course frame, standing UP.
///
/// The tavern is clad by shipwrights out of the same stock as a hull, so its
/// walls are vertical boarding rather than lap siding — [`bonded_boards`]
/// applies the quarter turn and pre-rotates the world offset to match, which
/// is the half of #972 lesson 15 that is easy to skip and invisible when you
/// do (every slab still gets vertical boards; they just each start at their
/// own centre).
fn clad(center: [f32; 3], color: [f32; 3]) -> crate::pds::SovereignMaterialSettings {
    bonded_boards(strake(color), FaceKey::SideNz, center)
}

/// One glazed opening: the card on its flat quad, set back in the reveal.
///
/// Returns the card alone — the wall around it is the caller's, because the
/// piers and sill walls that frame an opening are structure and belong to the
/// elevation, not to the window.
fn light(x: f32, sill: f32, w: f32, h: f32, panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [w + CARD_LAP * 2.0, h + CARD_LAP * 2.0],
            pane_grid(GLASS_AMBER, 0.0, panes),
        ),
        [x, sill + h * 0.5, FRONT_Z + REVEAL],
        quat_x(-FRAC_PI_2),
    )
}

/// The taproom, laid out bay by bay (#972 lesson 9).
///
/// Every opening on the street gets its own thing to look at: the left window
/// a bar counter with casks behind it, the door a lit floor running back to
/// the hearth, the right window a table with a lantern on it. A fit-out
/// authored for "the taproom" puts the counter where a counter goes and
/// leaves two of the three bays a black rectangle.
fn taproom() -> Vec<Generator> {
    let ceil = FLOOR + BODY[1] - 0.5;
    let mut out = vec![
        // Floor and back lining. The lining is WARMER than the floor: three
        // interior surfaces at one tone make a flat grey box however well
        // they are lit.
        // Held FLOOR_INSET behind the wall face. Run to FRONT_Z exactly, the
        // floor's leading edge and the piers' front faces share one plane and
        // z-fight along the whole sill line — the battery's casemate fault
        // arriving indoors (#1028).
        prim(
            solid(cuboid_tapered(
                [BODY[0] - 1.2, 0.1, ROOM_BACK - FRONT_Z - FLOOR_INSET],
                0.0,
                lit_interior([0.30, 0.24, 0.18], 0.16),
            )),
            [0.0, FLOOR + 0.05, (FRONT_Z + FLOOR_INSET + ROOM_BACK) * 0.5],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [BODY[0] - 1.2, BODY[1] - 0.6, 0.12],
                0.0,
                lit_interior([0.46, 0.32, 0.19], 0.34),
            )),
            [0.0, FLOOR + (BODY[1] - 0.6) * 0.5, ROOM_BACK],
            id_quat(),
        ),
        // Ceiling wash, held under the window heads so it lights what the
        // street can see rather than the ceiling itself.
        prim(
            solid(cuboid_tapered(
                [BODY[0] - 2.0, 0.08, 2.4],
                0.0,
                lit_interior([0.42, 0.30, 0.18], 0.3),
            )),
            [0.0, ceil, FRONT_Z + 1.8],
            id_quat(),
        ),
    ];

    // Left bay: the bar counter, with a cask stillage behind it.
    let bar_x = BAYS[0];
    out.push(prim(
        solid(cuboid_tapered([2.7, 1.02, 0.6], 0.0, board(HULL_OAK))),
        [bar_x, FLOOR + 0.51, FRONT_Z + 1.5],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered([2.9, 0.08, 0.76], 0.0, board(DECK_HOLY))),
        [bar_x, FLOOR + 1.06, FRONT_Z + 1.5],
        id_quat(),
    ));
    for (i, dz) in [0.0_f32, 0.62].into_iter().enumerate() {
        for dx in [-0.75_f32, 0.0, 0.75] {
            out.push(prim(
                solid(cylinder_tapered(0.34, 0.78, 12, -0.1, board(HULL_OAK))),
                [
                    bar_x + dx,
                    FLOOR + 0.39 + i as f32 * 0.8,
                    ROOM_BACK - 0.55 - dz,
                ],
                quat_z(FRAC_PI_2),
            ));
            out.push(prim(
                torus(0.035, 0.35, iron(IRON_BLACK, 0x71)),
                [
                    bar_x + dx,
                    FLOOR + 0.39 + i as f32 * 0.8,
                    ROOM_BACK - 0.55 - dz,
                ],
                quat_z(FRAC_PI_2),
            ));
        }
    }
    out.push(lantern(
        [bar_x + 1.1, FLOOR + 1.75, FRONT_Z + 1.4],
        0.5,
        0x72,
    ));

    // Centre bay: the hearth, straight down the sightline from the open door.
    out.push(prim(
        solid(cuboid_tapered(
            [2.1, 1.3, 0.5],
            0.0,
            cobbles(STONE_QUAY, 0x73),
        )),
        [BAYS[1], FLOOR + 0.65, ROOM_BACK - 0.32],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [1.35, 0.85, 0.3],
            0.0,
            glow(SIGN_AMBER, 2.0),
        )),
        [BAYS[1], FLOOR + 0.44, ROOM_BACK - 0.5],
        id_quat(),
    ));

    // Right bay: a table with a lantern and two stools.
    let tab_x = BAYS[2];
    out.push(prim(
        solid(cuboid_tapered([1.5, 0.07, 0.9], 0.0, board(DECK_HOLY))),
        [tab_x, FLOOR + 0.78, FRONT_Z + 1.7],
        id_quat(),
    ));
    for dx in [-0.55_f32, 0.55] {
        out.push(prim(
            solid(cuboid_tapered([0.09, 0.78, 0.09], 0.0, board(HULL_OAK))),
            [tab_x + dx, FLOOR + 0.39, FRONT_Z + 1.7],
            id_quat(),
        ));
        out.push(prim(
            solid(cylinder_tapered(0.19, 0.46, 10, 0.1, board(HULL_OAK))),
            [tab_x + dx * 1.9, FLOOR + 0.23, FRONT_Z + 1.7],
            id_quat(),
        ));
    }
    out.push(lantern([tab_x, FLOOR + 1.25, FRONT_Z + 1.7], 0.44, 0x74));
    out
}

/// The street elevation: piers, sill walls, spandrels and the head band that
/// frame the three bays, plus the cards that fill them.
///
/// Built as a shell (#972 lesson 1). The alternative — a solid wall with
/// glazing laid on it — is the fault the ledger has caught more than any
/// other, and it is worse here than anywhere: the whole subject of a tavern
/// is the light coming out of it.
fn street_elevation() -> Vec<Generator> {
    let mut out = Vec::new();
    let z = FRONT_Z + BODY[2] * 0.25;
    let wall_d = BODY[2] * 0.5;

    // Pier edges either side of every opening, plus the building's own ends.
    let mut edges = vec![-BODY[0] * 0.5, BODY[0] * 0.5];
    for (i, x) in BAYS.into_iter().enumerate() {
        let w = if i == 1 { DOOR_W } else { WIN_W };
        edges.push(x - w * 0.5);
        edges.push(x + w * 0.5);
    }
    edges.sort_by(|a, b| a.partial_cmp(b).expect("finite"));

    // Full-height piers between the openings.
    for pair in edges.chunks(2) {
        let (a, b) = (pair[0], pair[1]);
        let w = b - a;
        if w < 0.05 {
            continue;
        }
        let cx = (a + b) * 0.5;
        if BAYS.iter().any(|x| (x - cx).abs() < 0.05) {
            continue;
        }
        let c = [cx, FLOOR + BODY[1] * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered([w, BODY[1], wall_d], 0.0, clad(c, HULL_OAK))),
            c,
            id_quat(),
        ));
    }

    // Sill walls under the two windows, and spandrels over all three bays.
    for (i, x) in BAYS.into_iter().enumerate() {
        let w = if i == 1 { DOOR_W } else { WIN_W };
        let head = if i == 1 { FLOOR + DOOR_H } else { SILL + WIN_H };
        if i != 1 {
            let c = [x, (FLOOR + SILL) * 0.5, z];
            out.push(prim(
                solid(cuboid_tapered(
                    [w, SILL - FLOOR, wall_d],
                    0.0,
                    clad(c, HULL_OAK),
                )),
                c,
                id_quat(),
            ));
            // Cast sill, standing proud so the opening has a shadow line.
            out.push(prim(
                solid(cuboid_tapered(
                    [w + 0.3, 0.12, 0.34],
                    0.0,
                    cobbles(STONE_QUAY, 0x75),
                )),
                [x, SILL + 0.06, FRONT_Z + 0.1],
                id_quat(),
            ));
        }
        let c = [x, (head + STOREY_1) * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered(
                [w, STOREY_1 - head, wall_d],
                0.0,
                clad(c, HULL_OAK),
            )),
            c,
            id_quat(),
        ));
        if i != 1 {
            out.push(light(x, SILL, WIN_W, WIN_H, (3, 3)));
        }
    }

    // The door, standing OPEN against its jamb on the lit taproom. A closed
    // door makes the centre bay a darker rectangle on the wall and throws
    // away the one sightline that reaches the hearth.
    //
    // This is the pivot-about-an-edge shape (#972 lesson 21's corollary),
    // and the first build failed it in the purest way available: the leaf's
    // CENTRE was placed on the swung arc and its ROTATION was left at the
    // identity — a wall-parallel slab hanging diagonally beside its own
    // doorway (#1028). Centre and turn now come from ONE direction vector:
    // `quat_y(θ)` carries local +X to (cos θ, 0, −sin θ), so the same
    // (cos, −sin) pair that aims the leaf also places its midpoint along it
    // from the hinge. Getting one wrong now gets both wrong, which is the
    // property that makes the guard on the hinge edge decisive.
    let leaf_w = DOOR_W * 0.92;
    let swing = 1.15_f32;
    let hinge = [BAYS[1] - DOOR_W * 0.5, FLOOR + DOOR_H * 0.5, FRONT_Z + 0.1];
    let arm = [swing.cos(), -swing.sin()];
    out.push(prim(
        solid(cuboid_tapered(
            [leaf_w, DOOR_H - 0.1, 0.09],
            0.0,
            board(WHARF_GREY),
        )),
        [
            hinge[0] + leaf_w * 0.5 * arm[0],
            hinge[1],
            hinge[2] + leaf_w * 0.5 * arm[1],
        ],
        quat_y(swing),
    ));
    // Threshold stone under it.
    out.push(prim(
        solid(cuboid_tapered(
            [DOOR_W + 0.5, 0.12, 0.7],
            0.0,
            cobbles(STONE_QUAY, 0x76),
        )),
        [BAYS[1], FLOOR + 0.06, FRONT_Z - 0.1],
        id_quat(),
    ));
    out
}

/// The upper storey, its three lights, and the gallery slung under them.
fn upper_storey() -> Vec<Generator> {
    let z = FRONT_Z + BODY[2] * 0.25;
    let wall_d = BODY[2] * 0.5;
    let mut out = Vec::new();

    let mut edges = vec![-BODY[0] * 0.5, BODY[0] * 0.5];
    for x in BAYS {
        edges.push(x - UP_W * 0.5);
        edges.push(x + UP_W * 0.5);
    }
    edges.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    for pair in edges.chunks(2) {
        let (a, b) = (pair[0], pair[1]);
        let w = b - a;
        if w < 0.05 {
            continue;
        }
        let cx = (a + b) * 0.5;
        if BAYS.iter().any(|x| (x - cx).abs() < 0.05) {
            continue;
        }
        let c = [cx, STOREY_1 + UPPER_H * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered([w, UPPER_H, wall_d], 0.0, clad(c, HULL_OAK))),
            c,
            id_quat(),
        ));
    }
    for x in BAYS {
        let c = [x, (STOREY_1 + UP_SILL) * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered(
                [UP_W, UP_SILL - STOREY_1, wall_d],
                0.0,
                clad(c, HULL_OAK),
            )),
            c,
            id_quat(),
        ));
        let head = UP_SILL + UP_H;
        let c2 = [x, (head + EAVES) * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered(
                [UP_W, EAVES - head, wall_d],
                0.0,
                clad(c2, HULL_OAK),
            )),
            c2,
            id_quat(),
        ));
        out.push(light(x, UP_SILL, UP_W, UP_H, (2, 3)));
        // A lit chamber behind each — #972 lesson 6's vertical half. From the
        // lane the eye goes UP through an upper window, so what it frames is
        // the room's far corner; unlit, all three read as black rectangles,
        // which is the one thing a card must never do.
        // Held CLOSE to the opening and lit harder than the taproom's lining.
        // At 1.5 m back and 0.28 the three upper lights rendered as dark
        // rectangles beside a glowing ground floor: an upper room has no
        // hearth to borrow from, so its own panel has to do all the work, and
        // the further back it sits the less of it the reveal admits.
        out.push(prim(
            solid(cuboid_tapered(
                [UP_W + 0.5, UP_H + 0.4, 0.1],
                0.0,
                lit_interior([0.52, 0.36, 0.20], 0.5),
            )),
            [x, UP_SILL + UP_H * 0.5, FRONT_Z + 0.95],
            id_quat(),
        ));
    }
    out
}

/// The gallery: a boarded balcony on posts, railed, with a shingle pentice
/// over it.
fn gallery() -> Generator {
    let front = FRONT_Z - GALLERY_D;
    let deck_c = [0.0, GALLERY_Y, FRONT_Z - GALLERY_D * 0.5];
    let mut carried = Vec::new();

    // Posts down to the apron, derived from the gallery's own front edge so
    // they cannot end up standing off it (#972 lesson 19).
    for sx in [-1.0_f32, 1.0] {
        for f in [0.32_f32, 0.88] {
            let x = sx * BODY[0] * 0.5 * f;
            carried.push(prim(
                solid(cuboid_tapered(
                    [0.16, GALLERY_Y - GROUND, 0.16],
                    0.02,
                    board(WHARF_GREY),
                )),
                [x, (GROUND + GALLERY_Y) * 0.5, front + 0.16],
                id_quat(),
            ));
        }
    }
    // Railing round the three open sides.
    let rail_y = GALLERY_Y + 0.07;
    // The long run takes a COARSER pitch than the shared default.
    //
    // `BALUSTER_PITCH` (0.42 m) is calibrated for a prop seen at prop
    // distance — a boardwalk you stand next to. On an eleven-metre gallery it
    // is twenty-four balusters, and the railing alone came to a third of the
    // whole building's record for detail that is two pixels wide from the
    // street. Widened until it still reads as *balusters* rather than as a
    // ladder, which is the property that matters, not the count.
    carried.extend(railing(
        [-BODY[0] * 0.5, rail_y, front],
        [BODY[0] * 0.5, rail_y, front],
        RAIL_H,
        BALUSTER_PITCH * 1.6,
        board(WHARF_GREY),
    ));
    for sx in [-1.0_f32, 1.0] {
        carried.extend(railing(
            [sx * BODY[0] * 0.5, rail_y, front],
            [sx * BODY[0] * 0.5, rail_y, FRONT_Z],
            RAIL_H,
            BALUSTER_PITCH,
            board(WHARF_GREY),
        ));
    }
    // No pentice over the gallery, deliberately.
    //
    // The first build put one there and it had nowhere to go: a shelter has to
    // clear the gallery's own railing (4.92 m) and stop under the upper sills,
    // and on a two-storey building those two levels are the same level. The
    // choice is between a canopy that fouls the balustrade and one that roofs
    // the whole elevation — the motel's fault, where a walkway canopy at
    // wall-top height turned the building into a grey rectangle with a lane
    // underneath. A ship's gallery is open to the sky, so this one is too.

    nest(
        prim(
            solid(cuboid_tapered(
                [BODY[0], 0.14, GALLERY_D],
                0.0,
                bonded_siding(board(WHARF_GREY), FaceKey::Top, deck_c),
            )),
            deck_c,
            id_quat(),
        ),
        carried,
    )
}

/// The false front, the roof behind it, and the sign that hangs off it.
fn head() -> Vec<Generator> {
    let mut out = Vec::new();
    let parapet_h = 1.5;
    let fc = [0.0, EAVES + parapet_h * 0.5, FRONT_Z + 0.18];
    // The false front itself: a flat boarded parapet, standing PROUD of the
    // wall below so its base throws a shadow line rather than continuing the
    // elevation as one surface.
    out.push(prim(
        solid(cuboid_tapered(
            [BODY[0] + 0.5, parapet_h, 0.36],
            0.0,
            clad(fc, HULL_TAR),
        )),
        fc,
        id_quat(),
    ));
    // Coping along its head.
    out.push(prim(
        solid(cuboid_tapered(
            [BODY[0] + 0.7, 0.16, 0.5],
            0.0,
            board(WHARF_GREY),
        )),
        [0.0, EAVES + parapet_h + 0.08, FRONT_Z + 0.18],
        id_quat(),
    ));
    // Roof behind it: a ridge along the building, so the false front hides a
    // real roof rather than a flat lid.
    let roof_h = 1.9;
    out.push(prim(
        solid(cuboid_tapered_xz(
            [BODY[0] + 0.4, roof_h, BODY[2] + 0.4],
            [0.0, 0.94],
            shingle(SHINGLE_GREY),
        )),
        [0.0, EAVES + roof_h * 0.5, 0.35],
        id_quat(),
    ));
    // Eaves fascia on the flanks, hung off the roof's own edge.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [0.14, 0.28, BODY[2] + 0.4],
                0.0,
                board(WHARF_GREY),
            )),
            [sx * (BODY[0] * 0.5 + 0.27), EAVES + 0.14, 0.35],
            id_quat(),
        ));
    }
    // Stack, clearing the ridge — a flue that stops inside its own roof is
    // the fault the farmhouse and the suburban house both shipped.
    let stack_top = EAVES + roof_h + 0.9;
    out.push(prim(
        solid(cuboid_tapered(
            [1.0, stack_top - STOREY_1, 1.0],
            0.05,
            cobbles(STONE_QUAY, 0x77),
        )),
        [BAYS[1] + 2.4, (STOREY_1 + stack_top) * 0.5, 2.2],
        id_quat(),
    ));
    // The flue is OPEN — a hollow pot, not a capped slab. A chimney with a
    // lid is a column with a hat on it, and the smoke rising from it starts
    // out of a solid (#1028). `tube` gives a real bore with an inner wall
    // and an annular rim, so from any angle above the eaves you look into a
    // hole; the smoke emitter sits in its mouth.
    out.push(prim(
        solid(tube(0.42, 0.30, 0.55, 12, cobbles(STONE_QUAY, 0x7D))),
        [BAYS[1] + 2.4, stack_top + 0.27, 2.2],
        id_quat(),
    ));

    // The sign: a wrought bracket off the false front with a painted board
    // swinging under it. A tavern's name goes on a hanging sign, not on the
    // fascia — that is what tells a stranger it is a public house.
    let arm = 1.5_f32;
    let bx = -BODY[0] * 0.5 + 0.9;
    let by = EAVES + 0.35;
    out.push(prim(
        solid(cuboid_tapered(
            [0.07, 0.07, arm],
            0.0,
            iron(IRON_BLACK, 0x78),
        )),
        [bx, by, FRONT_Z - arm * 0.5],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [0.06, 0.5, 0.06],
            0.0,
            iron(IRON_BLACK, 0x79),
        )),
        [bx, by - 0.25, FRONT_Z - arm + 0.1],
        id_quat(),
    ));
    let sign_c = [bx, by - 1.0, FRONT_Z - arm + 0.1];
    out.push(prim(
        solid(cuboid_tapered(
            [1.3, 0.95, 0.09],
            0.0,
            bonded_siding(board(OAK_JOINERY), FaceKey::SideNx, sign_c),
        )),
        sign_c,
        id_quat(),
    ));
    // Painted face, deep-saturated at low strength: a broad pale lit panel
    // blooms to a white blank (the standing gotcha).
    // Painted face — small and deep-saturated, so it holds its hue where a
    // broad pale lit panel would bloom to a white blank.
    //
    // Thin in Z, matching the board it lies on. The first build carried a
    // leftover quarter-turn in its DIMENSIONS — thin in X, tall in Z — so
    // the lit face stood edge-on to the street and stuck through the board
    // sideways, which read in-world as the sign's device rotated 90° off
    // (#1028). The board is thin in Z; anything mounted on it must be too.
    out.push(prim(
        cuboid_tapered([1.05, 0.72, 0.05], 0.0, glow(SIGN_AMBER, 1.6)),
        [bx, by - 1.0, FRONT_Z - arm + 0.1 - 0.075],
        id_quat(),
    ));
    // Gilt beading round it, and a lamp so the sign reads after dark.
    // Gilt beading, its ring IN the board's plane: a torus lies in XZ with
    // its axis on +Y, so facing the street (−Z) is a quarter-turn about X —
    // the `quat_z` the first build used stood the ring edge-on beside the
    // board, the same 90° family of error as the face above.
    out.push(prim(
        torus(0.03, 0.52, glow(GOLD_LEAF, 0.45)),
        [bx, by - 1.0, FRONT_Z - arm + 0.1 - 0.06],
        quat_x(FRAC_PI_2),
    ));
    out.push(lantern([bx, by - 0.2, FRONT_Z - arm + 0.1], 0.46, 0x7A));
    out
}

fn build_tree() -> Generator {
    let apron_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0x70);
    paving.uv_offset = face_uv_offset(FaceKey::Top, apron_c);

    let plinth_c = [0.0, GROUND + PLINTH[1] * 0.5, 0.0];
    let mut on_plinth = Vec::new();
    on_plinth.extend(street_elevation());
    on_plinth.extend(upper_storey());
    on_plinth.extend(taproom());
    on_plinth.extend(head());
    on_plinth.push(gallery());

    // Flank and rear walls, as single slabs — they carry no openings, so a
    // punched grid would cost twenty prims to say nothing.
    for sx in [-1.0_f32, 1.0] {
        let c = [
            sx * (BODY[0] * 0.5 - 0.2),
            FLOOR + (BODY[1] + UPPER_H) * 0.5,
            0.4,
        ];
        on_plinth.push(prim(
            solid(cuboid_tapered(
                [0.4, BODY[1] + UPPER_H, BODY[2] - 0.8],
                0.0,
                clad(c, HULL_OAK),
            )),
            c,
            id_quat(),
        ));
    }
    let back_c = [0.0, FLOOR + (BODY[1] + UPPER_H) * 0.5, BODY[2] * 0.5 - 0.2];
    on_plinth.push(prim(
        solid(cuboid_tapered(
            [BODY[0], BODY[1] + UPPER_H, 0.4],
            0.0,
            clad(back_c, HULL_OAK),
        )),
        back_c,
        id_quat(),
    ));
    // Storey band ringing all four elevations — a RING, so it takes the
    // building's own centre and its projection goes into its SIZE (#972
    // lesson 31); centred on the trim plane it becomes a cantilevered shelf.
    let band_c = [0.0, STOREY_1 - 0.16, 0.0];
    on_plinth.push(prim(
        solid(with_face(
            cuboid_tapered(
                [BODY[0] + 0.36, 0.22, BODY[2] + 0.36],
                0.0,
                bonded_siding(board(WHARF_GREY), FaceKey::SideNz, band_c),
            ),
            FaceKey::Top,
            bonded_siding(board(WHARF_GREY), FaceKey::Top, band_c),
        )),
        band_c,
        id_quat(),
    ));

    let mut carried = vec![
        footing(PLINTH[0], PLINTH[2], [0.0, 0.0], 9.0),
        nest(
            prim(
                solid(cuboid_tapered(
                    [PLINTH[0], PLINTH[1], PLINTH[2]],
                    0.0,
                    cobbles(STONE_QUAY, 0x7B),
                )),
                plinth_c,
                id_quat(),
            ),
            on_plinth,
        ),
    ];
    // Street furniture, derived from the apron's own extent.
    for (i, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        // Derived from the APRON's own extent, not measured off the building
        // (#972 lesson 8). At a tidy `FRONT_Z - 2.6` the tuns stood 0.42 m
        // past the paving, which is the class of error no camera angle here
        // would show.
        // Inside the apron AND clear of the plinth — both constraints, stated
        // (#972 lesson 8 has the first half; #1028 supplied the second: at
        // x ±5.7 the tun grazed the plinth's own 5.7 m half-width and the
        // hawser coil sat inside its corner). The x is derived from the
        // PLINTH's edge plus the tun's radius, the z from the apron's.
        let x = sx * (PLINTH[0] * 0.5 + 0.75);
        let z = -(APRON[2] * 0.5 - 1.4);
        carried.push(prim(
            solid(cylinder_tapered(0.42, 0.86, 12, -0.06, board(HULL_OAK))),
            [x, GROUND + 0.43, z],
            id_quat(),
        ));
        carried.push(prim(
            torus(0.045, 0.43, iron(IRON_BLACK, 0x7C + i as u32)),
            [x, GROUND + 0.68, z],
            id_quat(),
        ));
        if i == 0 {
            // Forward of the plinth's front face, not inboard of the tun —
            // inboard put it straight back over the plinth corner the tun
            // had just been moved off (#1028; the clearance guard caught it).
            carried.push(prim(
                torus(0.05, 0.3, hemp(ROPE_HEMP)),
                [x - sx * 0.9, GROUND + 0.05, -(PLINTH[2] * 0.5 + 0.55)],
                id_quat(),
            ));
        }
    }
    // A ship's bell by the door — last orders.
    carried.push(prim(
        solid(cuboid_tapered(
            [0.4, 0.08, 0.08],
            0.0,
            iron(IRON_BLACK, 0x7E),
        )),
        [BAYS[1] + 1.5, FLOOR + 2.55, FRONT_Z - 0.3],
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(
            0.16,
            0.26,
            12,
            0.4,
            bronze(BRONZE_FITTING, 0x7F),
        )),
        [BAYS[1] + 1.5, FLOOR + 2.38, FRONT_Z - 0.3],
        id_quat(),
    ));

    let mut root = nest(
        prim(
            solid(cuboid_tapered(APRON, 0.0, paving)),
            apron_c,
            id_quat(),
        ),
        carried,
    );
    // Hearth smoke off the stack. Attached rather than pushed: a child added
    // to a finished root is read in the root's local frame and never rebased
    // (#1010).
    attach(
        &mut root,
        fx::hearth_smoke([BAYS[1] + 2.4, EAVES + 1.9 + 1.1, 2.2], 0x7A_11),
    );
    root.audio = fx::harbour_swell();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive, window_cards,
    };
    use crate::pds::PrimCommon;

    fn built() -> Generator {
        HarbourTavern.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "harbour_tavern");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "harbour_tavern");
    }

    /// #972 lessons 1, 17 and 20, on the entry the kit's card was written for:
    /// nothing solid wears a `Window` texture, no two cards fight for depth,
    /// and the count is exact so a card moved onto a solid fails loudly rather
    /// than quietly reducing the total.
    #[test]
    fn every_light_is_a_card_over_a_real_opening() {
        let g = built();
        assert_no_glazing_on_solids(&g, "harbour_tavern");
        assert_cards_do_not_overlap(&g, "harbour_tavern");
        let cards = window_cards(&g);
        assert_eq!(
            cards.len(),
            5,
            "two ground lights and three upper ones; found {}",
            cards.len()
        );
        for c in &cards {
            // #972 lesson 7: a card sized exactly to its opening puts each
            // edge on the reveal's own plane, and a flush edge is a tie the
            // rasteriser has to break.
            let (w, h) = (c.size[0], c.size[1]);
            let opening = if h > (WIN_H + UP_H) * 0.5 {
                (WIN_W, WIN_H)
            } else {
                (UP_W, UP_H)
            };
            assert!(
                w > opening.0 + CARD_LAP && h > opening.1 + CARD_LAP,
                "a {w} x {h} card does not lap its {opening:?} opening"
            );
            // And it is set BACK, not flush with the wall face.
            assert!(
                c.center[2] > FRONT_Z + REVEAL * 0.5,
                "a card at z = {} is not recessed in its reveal",
                c.center[2]
            );
        }
    }

    /// Every bay has its own thing to look at (#972 lesson 9).
    ///
    /// The fault this guards is not "the taproom is empty" — it is "the
    /// taproom was authored for the room and everything ended up at one end",
    /// which the mini-mart shipped and which reads as one beautiful bay
    /// beside two black rectangles. So it is checked per bay, against the
    /// bays' own centres.
    #[test]
    fn every_bay_has_something_lit_behind_it() {
        let g = built();
        assert!(has_emissive(&g), "the tavern lost its taproom");
        let solids = measure::solids(&g);
        for x in BAYS {
            let filled = solids.iter().any(|p| {
                let c = p.bounds.center();
                (c.x - x).abs() < 1.7
                    && c.z > FRONT_Z
                    && c.z < ROOM_BACK + 0.2
                    && c.y > FLOOR
                    && c.y < FLOOR + BODY[1]
            });
            assert!(
                filled,
                "bay at x = {x} has nothing standing behind its opening — it \
                 will read as a black rectangle whatever the shell does"
            );
        }
    }

    /// The gallery's railing clears the windows above it (#972 lesson 24).
    ///
    /// A balustrade in front of an opening hides the one thing the opening was
    /// cut for, and on a two-storey front the gallery rail and the upper sill
    /// are within a few centimetres of each other by default — the first build
    /// had the rail crossing the bottom 0.18 m of all three upper lights.
    /// Checked against the BUILT railing rather than the constant, since the
    /// helper adds its own stock to whatever height it is given.
    #[test]
    fn the_gallery_railing_clears_the_upper_lights() {
        let g = built();
        let rail_top = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                let b = &p.bounds;
                // The gallery run: out over the lane, at gallery height.
                b.center().z < FRONT_Z && b.max.y > GALLERY_Y && b.max.y < STOREY_1 + 2.0
            })
            .map(|p| p.bounds.max.y)
            .fold(f32::MIN, f32::max);
        assert!(
            rail_top > GALLERY_Y,
            "no gallery railing found above the deck at {GALLERY_Y}"
        );
        assert!(
            rail_top < UP_SILL,
            "the gallery railing tops out at {rail_top} and the upper sills \
             are at {UP_SILL} — the balustrade is standing in front of the \
             windows it is under"
        );
    }

    /// The stack clears its own ridge, and the flue is OPEN.
    ///
    /// Two claims that were two shipped faults. A flue stopping inside the
    /// roof mass is the farmhouse's and suburban house's fault; a flue with a
    /// capped top is this entry's (#1028) — a column with a hat on it, whose
    /// smoke rose out of a solid. The crown must be a `Tube`, because a tube
    /// is the one prim whose top face is an annulus: from any angle above the
    /// eaves you look into a bore, not onto a lid.
    #[test]
    fn the_stack_clears_the_ridge_and_the_flue_is_open() {
        let g = built();
        let solids = measure::solids(&g);
        let ridge = solids
            .iter()
            .filter(|p| p.bounds.size().x > BODY[0] && p.bounds.center().y > EAVES)
            .map(|p| p.bounds.max.y)
            .fold(f32::MIN, f32::max);
        // The shaft: the tall square section rising through the roof.
        let shaft = solids
            .iter()
            .find(|p| {
                let sz = p.bounds.size();
                (sz.x - 1.0).abs() < 0.05 && sz.y > 3.0
            })
            .expect("the chimney shaft is in the tree");
        // The pot: the tree's one Tube, seated on the shaft's top.
        let pot = solids
            .iter()
            .find(|p| p.kind_tag == "Tube")
            .expect("the flue crown is a Tube — a capped chimney is a lid");
        assert!(
            (pot.bounds.min.y - shaft.bounds.max.y).abs() < 0.05,
            "the pot floats at {} over a shaft topping out at {}",
            pot.bounds.min.y,
            shaft.bounds.max.y
        );
        assert!(
            ((pot.bounds.center().x) - shaft.bounds.center().x).abs() < 0.1,
            "the pot is not on its own shaft"
        );
        assert!(
            pot.bounds.max.y > ridge + 0.4,
            "the flue tops out at {} and the ridge at {ridge} — it has to \
             clear the roof it comes through",
            pot.bounds.max.y
        );
    }

    /// Street furniture stands clear of the plinth as well as on the apron.
    ///
    /// The other half of #972 lesson 8, supplied in-world (#1028): the tuns
    /// were derived from the APRON's edge, which put one at x ±5.7 — exactly
    /// the plinth's own half-width — and the hawser coil inside the plinth's
    /// corner. Two solids sharing space is invisible in a still when they
    /// are the same tone, so it is checked as an AABB overlap against the
    /// plinth's real extent.
    #[test]
    fn street_furniture_stays_clear_of_the_plinth() {
        let g = built();
        let ph = [PLINTH[0] * 0.5, PLINTH[1], PLINTH[2] * 0.5];
        let mut furniture = 0;
        for p in measure::solids(&g) {
            // Ground furniture: casks and coils — revolved prims on the
            // apron, below the plinth top.
            if !matches!(p.kind_tag, "Cylinder" | "Torus") {
                continue;
            }
            let b = &p.bounds;
            // STREET furniture stands on the apron; the taproom's casks stand
            // on the plinth, a storey of masonry higher, and are inside the
            // building on purpose. Selecting on the piece's FEET is what
            // separates the populations — the first draft filtered on centre
            // height and promptly flagged the bar's own stillage (#972
            // lesson 24, again).
            if b.min.y > FLOOR - 0.05 || b.center().y < GROUND {
                continue;
            }
            furniture += 1;
            let hits = b.max.x > -ph[0] && b.min.x < ph[0] && b.max.z > -ph[2] && b.min.z < ph[2];
            assert!(
                !hits,
                "{} at {:?} runs into the plinth footprint (±{} x ±{})",
                p.kind_tag,
                b.center(),
                ph[0],
                ph[2]
            );
        }
        assert!(
            furniture >= 3,
            "only {furniture} pieces of street furniture examined — the \
             selector has stopped finding the tuns and the coil"
        );
    }

    /// The taproom floor sits behind the wall face, and the sign's lit face
    /// lies flat ON its board.
    ///
    /// Both were 90°/coplanar faults from #1028: the floor's leading edge
    /// shared the piers' front plane and z-fought along the sill line, and
    /// the lit face kept a quarter-turn in its DIMENSIONS — thin in X
    /// instead of Z — so it stood edge-on through the board. The dimension
    /// check is the one that catches that class: a face mounted on a board
    /// must be thin on the same axis the board is.
    #[test]
    fn the_floor_is_inset_and_the_sign_face_lies_on_its_board() {
        use crate::pds::GeneratorKind as K;
        fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3], f32)>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
            {
                out.push((here, size.0, material.emission_strength.0));
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        let mut boxes = Vec::new();
        walk(&built(), [0.0; 3], &mut boxes);
        // The floor: the wide, thin, low, dim slab.
        let floor = boxes
            .iter()
            .find(|(c, s, e)| s[0] > 8.0 && s[1] < 0.2 && *e > 0.0 && c[1] < FLOOR + 0.3)
            .expect("the taproom floor is in the tree");
        assert!(
            floor.0[2] - floor.1[2] * 0.5 > FRONT_Z + FLOOR_INSET * 0.5,
            "the floor's leading edge is at {} — on the wall face at {FRONT_Z}",
            floor.0[2] - floor.1[2] * 0.5
        );
        // The sign face: the lit plate hanging out over the lane.
        let face = boxes
            .iter()
            .find(|(c, _, e)| *e > 1.0 && c[2] < FRONT_Z - 0.5 && c[1] > EAVES - 2.0)
            .expect("the sign's lit face is in the tree");
        assert!(
            face.1[2] < face.1[0] && face.1[2] < face.1[1],
            "the sign face {:?} is not thin toward the street — it is standing \
             edge-on through its own board, the 90°-off fault",
            face.1
        );
    }

    /// The open leaf hangs on its own hinge (#972 lesson 21, the pivot-
    /// about-an-edge shape).
    ///
    /// The first build placed the leaf's centre on the swung arc and left its
    /// rotation at the identity — a wall-parallel slab floating beside the
    /// doorway (#1028), which is the same family as the fishing shack's
    /// unhinged door. So this guard does what that ledger entry prescribes:
    /// read the BUILT leaf's actual quaternion and half-extent, rotate
    /// `[w/2, 0, 0]` by it with [`rotate_by`], and demand one of the two
    /// resulting ends lands on the hinge jamb at the wall plane. It comes at
    /// the geometry from the opposite direction to the placement, so an
    /// identity rotation, a flipped handedness, or a centre off the arc all
    /// fail it loudly.
    #[test]
    fn the_open_leaf_hangs_on_its_own_hinge() {
        use crate::catalogue::items::util::rotate_by;
        use crate::pds::GeneratorKind as K;
        fn find_leaf(g: &Generator, at: [f32; 3]) -> Option<([f32; 3], [f32; 4], f32)> {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid { size, .. } = &g.kind {
                let q = g.transform.rotation.0;
                let yawed = q[1].abs() > 0.05 && q[0].abs() < 1e-4 && q[2].abs() < 1e-4;
                if yawed && (size.0[1] - (DOOR_H - 0.1)).abs() < 0.05 {
                    return Some((here, q, size.0[0]));
                }
            }
            g.children.iter().find_map(|c| find_leaf(c, here))
        }
        let (center, q, w) = find_leaf(&built(), [0.0; 3]).expect("the swung leaf is in the tree");
        let tip = rotate_by(q, [w * 0.5, 0.0, 0.0]);
        let ends = [
            [center[0] + tip[0], center[2] + tip[2]],
            [center[0] - tip[0], center[2] - tip[2]],
        ];
        let jamb = [BAYS[1] - DOOR_W * 0.5, FRONT_Z + 0.1];
        let on_hinge = ends
            .iter()
            .any(|e| (e[0] - jamb[0]).abs() < 0.09 && (e[1] - jamb[1]).abs() < 0.09);
        assert!(
            on_hinge,
            "neither end of the leaf ({ends:?}) lands on the hinge jamb at \
             {jamb:?} — the door is hung on nothing"
        );
        // And the free end stands OUT from the wall, or the "open" door is
        // lying flat against the elevation.
        let free = ends
            .iter()
            .min_by(|a, b| {
                (a[0] - jamb[0])
                    .abs()
                    .partial_cmp(&(b[0] - jamb[0]).abs())
                    .expect("finite")
                    .reverse()
            })
            .expect("two ends");
        assert!(
            free[1] < FRONT_Z - 0.4,
            "the free edge sits at z = {} — the leaf is not standing open",
            free[1]
        );
    }

    /// Everything on the ground stands on the apron it is nested under
    /// (#972 lessons 8 and 19).
    #[test]
    fn every_ground_part_stands_on_the_apron() {
        let g = built();
        let half = [APRON[0] * 0.5, APRON[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            if p.bounds.center().y > FLOOR + 1.0 {
                continue;
            }
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the apron in X",
                p.kind_tag,
                p.bounds.center()
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the apron in Z",
                p.kind_tag,
                p.bounds.center()
            );
        }
        assert!(
            checked > 10,
            "only {checked} ground parts examined — the selector has stopped \
             finding the plinth and the street furniture"
        );
    }
}
