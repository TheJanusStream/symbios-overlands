//! Motel — a Roadside secondary. A five-bay strip of rooms behind a covered
//! walkway: brick piers framing a real door and a real window per room, lit
//! rooms behind the glass, one door standing open, a glazed office at the end,
//! and a neon MOTEL pylon on its own footing out on the lot.
//!
//! Rebuilt as a shell under #972. What shipped was a solid block with pictures
//! of a motel painted on it:
//!
//! 1. **Four `Window`-textured slabs and a borrowed `curtain_wall`** on a solid
//!    brick mass (#972 lesson 20). The generator masks its panes away, so each
//!    was a frame with holes onto the brick behind it — and the office's
//!    glazing came from another kit's helper, which is a lit glass *box* behind
//!    fins and cannot be a window anywhere.
//! 2. **The doors were flat enamel panels.** No opening, no reveal, no room —
//!    so the one thing a motel elevation is *made* of had no depth at all.
//! 3. **Nothing stood on anything.** The walkway posts were 0.6 m off the back
//!    of the slab, the walkway roof oversailed it by 0.9, and the pylon's mast
//!    stood 0.5 m past its edge over bare ground (#972 lessons 8 and 19). All
//!    three are invisible unless a contact-sheet tile looks along that edge.
//! 4. **No lot, no walkway, no roof.** A flat concrete band did duty as the
//!    roof, and there was no way for anyone to park, walk or arrive.
//!
//! Now: an asphalt lot with painted bays and wheel stops, a raised walkway the
//! posts actually stand on, a brick shell whose piers frame ten real openings
//! over a lit room apiece, a canopy roof with the strip's fascia and sign, and
//! the pylon on a poured footing derived from the lot's own extent.

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, glow, id_quat, lit_interior, nest, plane, prim, quat_x,
    quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use std::f32::consts::FRAC_PI_2;

use super::{
    ASPHALT_DARK, BRICK_TAN, CONCRETE_GREY, CORRUGATED_GREY, ENAMEL_BLUE, ENAMEL_CREAM, GLASS_TINT,
    NEON_CYAN, NEON_RED, SIGN_AMBER, STEEL_GREY, asphalt, brick, concrete, corrugated, enamel, fx,
    glass, sign_board, steel,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// The lot. Its own extent is what everything outdoors is placed from, rather
/// than a round number measured off the building (#972 lesson 8).
const LOT_W: f32 = 19.0;
const LOT_FRONT: f32 = -7.6;
const LOT_BACK: f32 = 6.6;
const LOT_T: f32 = 0.14;
const LOT_TOP: f32 = LOT_T;
const LOT_D: f32 = LOT_BACK - LOT_FRONT;
const LOT_CZ: f32 = (LOT_BACK + LOT_FRONT) * 0.5;

/// Room block: `FRONT` is the outer face of the elevation the road sees, which
/// is the `−Z` direction the render tool and the settlement placer both look
/// down.
const BLOCK_W: f32 = 15.0;
const BLOCK_D: f32 = 5.4;
const FRONT: f32 = 0.0;
const BACK: f32 = BLOCK_D;
const WALL_T: f32 = 0.3;
const WALL_MID: f32 = FRONT + WALL_T * 0.5;

/// Covered walkway in front of the rooms, and the floor level it sets.
const WALK_D: f32 = 2.6;
const WALK_FRONT: f32 = -WALK_D;
const WALK_H: f32 = 0.16;
const FLOOR: f32 = LOT_TOP + WALK_H;

/// Wall height and the deck that sits on it.
const WALL_H: f32 = 3.4;
const WALL_TOP: f32 = FLOOR + WALL_H;
const ROOF_T: f32 = 0.24;
/// Roof oversail past the block, and past the walkway's front edge.
const ROOF_OVER: f32 = 0.3;
const ROOF_FRONT: f32 = FRONT - ROOF_OVER;
const ROOF_BACK: f32 = BACK + ROOF_OVER;

/// The walkway canopy is a **separate, lower** structure than the block's own
/// roof, which is the shape a strip motel actually has and the thing the first
/// rebuild got wrong: one deck at wall-top height projecting three metres over
/// the walkway roofs the entire elevation, so from any camera above eye level
/// the prop is a grey rectangle with a lot underneath it. Held down here, the
/// block's brick and its roofline read over the top of it.
const CANOPY_T: f32 = 0.18;
const CANOPY_UNDER: f32 = FLOOR + 2.3;
const CANOPY_TOP: f32 = CANOPY_UNDER + CANOPY_T;
const CANOPY_FRONT: f32 = WALK_FRONT - ROOF_OVER;
/// The canopy laps onto the wall it is fixed to rather than meeting its face.
const CANOPY_BACK: f32 = FRONT + 0.35;

/// Canopy fascia — the biggest coloured surface on the prop, and what carries
/// the strip's identity.
const FASCIA_H: f32 = 0.42;
const FASCIA_T: f32 = 0.14;

/// Glazing plane inside the reveal, the lit-room panel behind it, and the
/// plane a door leaf hangs in.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.7;
const ROOM_Z: f32 = FRONT + 2.3;
const LEAF_Z: f32 = FRONT + WALL_T * 0.45;
/// Centre plane of proud trim — door casings, room-number plaques, curtains.
const TRIM_Z: f32 = FRONT - 0.05;
/// How far a glazing card oversails its opening (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Openings, in metres. Door heads and window heads line up, which is what
/// makes a strip of rooms read as one building rather than ten holes.
const DOOR_W: f32 = 0.9;
const DOOR_H: f32 = 2.05;
const WIN_W: f32 = 1.2;
const WIN_SILL: f32 = 0.95;
const HEAD: f32 = DOOR_H;

/// Room bay centres. The office takes the `+X` end, nearest the road sign.
const BAY_X: [f32; 5] = [-6.0, -3.0, 0.0, 3.0, 6.0];
const OFFICE_BAY: usize = 4;
/// Which room stands open on a lit interior. A strip of shut doors is a wall
/// with rectangles on it; one open door is where the depth comes from.
const OPEN_ROOM: usize = 1;
/// How far that leaf stands open, in radians.
const DOOR_SWING: f32 = 1.05;

/// Walkway posts, held in from the walkway's own front edge so they stand on
/// the paving rather than off it (#972 lesson 19).
const POST_S: f32 = 0.17;
const POST_Z: f32 = WALK_FRONT + 0.24;

/// Brick length, in metres — a real brick, laid flat, in one course frame
/// shared by every masonry surface on the prop (#972 lesson 2).
const BRICK_LEN: f32 = 0.215;

// --- Palette local to this entry. ------------------------------------------

/// Warm lamplight in an occupied room.
const ROOM_WARM: [f32; 3] = [0.72, 0.56, 0.34];
/// Drawn curtain behind the glass — the one pale note in a lit room.
const CURTAIN_PALE: [f32; 3] = [0.78, 0.74, 0.64];
/// Paint on the parking bays and wheel stops.
const LINE_PAINT: [f32; 3] = [0.86, 0.84, 0.74];

// --- Shared construction. --------------------------------------------------

/// Brick laid flat at a real brick's size, in the shared world course frame.
fn wall_mat(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(BRICK_TAN), BRICK_LEN, face, center)
}

/// One brick slab of the shell. The centre is bound once and handed to the
/// material *and* the transform — passing a bonding helper a different reading
/// of "the middle of the wall" is the one way to defeat the frame guard
/// silently (#972 lesson 18).
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, wall_mat(center, face))),
        center,
        id_quat(),
    )
}

/// Board-formed concrete in the world frame — the lot's kerbs, the walkway,
/// the pylon footing.
fn slab_mat(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = concrete(CONCRETE_GREY);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// The kit's own storefront glass, re-cut to one opening's pane grid. Pane
/// counts are the one thing a shared card material cannot know, and they are
/// what tell a viewer how big the opening is.
fn pane_grid(panes: (u32, u32), lit: f32) -> SovereignMaterialSettings {
    let mut m = glass(GLASS_TINT, lit);
    if let crate::pds::SovereignTextureConfig::Window(cfg) = &mut m.texture {
        cfg.panes_x = panes.0;
        cfg.panes_y = panes.1;
    }
    m
}

/// Glazing filling one opening: a card on a flat quad in the reveal, lapped
/// into the brick either side.
fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(panes, 0.5),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit surface inside a room — what a card's masked-away panes actually
/// show. Kept below the sunlit brick around the opening, or the depth
/// flattens.
fn lit(size: [f32; 3], center: [f32; 3], color: [f32; 3], strength: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(color, strength)),
        center,
        id_quat(),
    )
}

/// A painted enamel part — door leaf, casing, fascia, wheel stop.
fn painted(size: [f32; 3], center: [f32; 3], color: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, enamel(color))),
        center,
        id_quat(),
    )
}

/// The elevation's openings, left to right: a door and a window per room, and
/// the office's own pair. Every pier, sill and spandrel is derived from this
/// list, so moving an opening cannot leave its brickwork behind.
fn openings() -> Vec<(f32, f32, bool)> {
    let mut v = Vec::new();
    for (b, &bx) in BAY_X.iter().enumerate() {
        if b == OFFICE_BAY {
            v.push((bx - 0.9, 0.95, true));
            v.push((bx + 0.55, 1.5, false));
        } else {
            v.push((bx - 0.8, DOOR_W, true));
            v.push((bx + 0.6, WIN_W, false));
        }
    }
    v
}

pub struct Motel;

impl CatalogueEntry for Motel {
    fn slug(&self) -> &'static str {
        "motel"
    }
    fn name(&self) -> &'static str {
        "Motel"
    }
    fn description(&self) -> &'static str {
        "Single-storey room strip behind a covered walkway, with a neon MOTEL pylon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 10.0,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The strip as a tree that stands the way it does: the lot at the bottom,
/// and on it the three things that stand on it — the walkway with its posts and
/// machines, the room block on its own base course, and the pylon on its
/// footing. Each of those is a **sub-root that is the surface**, which is the
/// guard shape that found all three of the shipped build's overhangs (#972
/// lesson 19).
fn build_tree() -> Generator {
    let center = [0.0, LOT_TOP - LOT_T * 0.5, LOT_CZ];
    let lot = prim(
        solid(cuboid_tapered([LOT_W, LOT_T, LOT_D], 0.0, {
            let mut m = asphalt(ASPHALT_DARK);
            m.uv_offset = util::face_uv_offset(FaceKey::Top, center);
            m
        })),
        center,
        id_quat(),
    );

    let mut parts = vec![walkway(), block(), pylon()];
    parts.extend(parking());
    nest(lot, parts)
}

/// Painted bays and wheel stops, both ends of every line derived from the lot's
/// own extent — the shipped build put its furniture at round numbers measured
/// off the building and hung it over the edge.
fn parking() -> Vec<Generator> {
    let z0 = WALK_FRONT - 0.3;
    let z1 = LOT_FRONT + 0.3;
    let len = z0 - z1;
    let cz = (z0 + z1) * 0.5;
    let lines = 7;
    let pitch = (LOT_W - 4.0) / (lines - 1) as f32;

    let mut out = Vec::new();
    for i in 0..lines {
        let x = (i as f32 - (lines - 1) as f32 * 0.5) * pitch;
        out.push(prim(
            cuboid_tapered([0.12, 0.03, len], 0.0, enamel(LINE_PAINT)),
            [x, LOT_TOP + 0.015, cz],
            id_quat(),
        ));
        if i + 1 < lines {
            out.push(painted(
                [pitch * 0.55, 0.14, 0.18],
                [x + pitch * 0.5, LOT_TOP + 0.07, z1 + 1.1],
                LINE_PAINT,
            ));
        }
    }
    out
}

/// The raised walkway, and everything standing on it: the canopy posts, an ice
/// machine and a vending machine outside the office.
fn walkway() -> Generator {
    let center = [0.0, LOT_TOP + WALK_H * 0.5, WALK_FRONT + WALK_D * 0.5];
    let slab = prim(
        solid(cuboid_tapered(
            [BLOCK_W + 0.6, WALK_H, WALK_D],
            0.0,
            slab_mat(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );

    let mut parts = Vec::new();
    for x in [-7.0_f32, -3.5, 0.0, 3.5, 7.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [POST_S, CANOPY_UNDER - FLOOR, POST_S],
                0.0,
                steel(STEEL_GREY),
            )),
            [x, (FLOOR + CANOPY_UNDER) * 0.5, POST_Z],
            id_quat(),
        ));
    }
    // Ice and vending, each standing against a **pier** rather than against
    // the office. The first rebuild put both outside the office door, where
    // they hid the one bay whose glazing is the point of the bay — the same
    // shape as #972 lesson 9, arrived at from outside the glass.
    let pier = |a: f32, b: f32| (a + b) * 0.5;
    parts.push(painted(
        [0.72, 1.55, 0.62],
        [pier(-4.8, -4.25), FLOOR + 0.775, FRONT - 0.42],
        ENAMEL_CREAM,
    ));
    parts.push(prim(
        cuboid_tapered([0.5, 0.42, 0.06], 0.0, glow(SIGN_AMBER, 1.9)),
        [pier(-4.8, -4.25), FLOOR + 1.18, FRONT - 0.74],
        id_quat(),
    ));
    parts.push(painted(
        [0.62, 1.75, 0.58],
        [pier(1.2, 1.75), FLOOR + 0.875, FRONT - 0.4],
        ENAMEL_BLUE,
    ));
    parts.push(prim(
        cuboid_tapered([0.44, 0.8, 0.06], 0.0, glow(NEON_RED, 1.6)),
        [pier(1.2, 1.75), FLOOR + 1.15, FRONT - 0.7],
        id_quat(),
    ));

    nest(slab, parts)
}

// --- The room block. -------------------------------------------------------

/// The brick base course — the block's sub-root, standing 60 mm **proud** of
/// the wall above it. Flush is a coplanar seam running the whole perimeter on
/// the most looked-at part of the building, and it is invisible in a still.
fn block() -> Generator {
    let center = [0.0, (LOT_TOP + FLOOR) * 0.5, (FRONT + BACK) * 0.5];
    let plinth = wall(
        [BLOCK_W + 0.12, FLOOR - LOT_TOP, BLOCK_D + 0.12],
        center,
        FaceKey::SideNz,
    );

    let mut parts = Vec::new();
    // Back and flank walls — only the road elevation is cut.
    parts.push(wall(
        [BLOCK_W, WALL_H, WALL_T],
        [0.0, FLOOR + WALL_H * 0.5, BACK - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, WALL_H, BLOCK_D - WALL_T * 2.0],
            [
                sx * (BLOCK_W * 0.5 - WALL_T * 0.5),
                FLOOR + WALL_H * 0.5,
                (FRONT + BACK) * 0.5,
            ],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    elevation(&mut parts);
    back_elevation(&mut parts);
    parts.push(canopy());
    parts.push(roof());
    nest(plinth, parts)
}

/// The service side: a staff door with its own step, bathroom vents over each
/// room, and a downpipe at each end.
///
/// A motel's back genuinely has no windows, which is the trap — the answer to
/// "nothing happens here" is not a fifteen-metre blank slab, it is the plant
/// that really is there. Every standoff is derived from the wall's **own** face
/// rather than picked by eye (#972 lesson 11), and the door's step stands on
/// the lot rather than on the building it serves (#972 lesson 19).
fn back_elevation(parts: &mut Vec<Generator>) {
    let door_t = 0.08;
    let dx = BLOCK_W * 0.5 - 2.2;
    parts.push(painted(
        [0.95, 2.05, door_t],
        [dx, FLOOR + 1.025, BACK + door_t * 0.5],
        ENAMEL_CREAM,
    ));
    parts.push(painted(
        [1.2, 0.12, 0.16],
        [dx, FLOOR + 2.13, BACK + 0.08],
        ENAMEL_CREAM,
    ));
    // The step the door actually opens onto, sized off the lot behind it.
    let step_c = [dx, LOT_TOP + WALK_H * 0.5, BACK + 0.45];
    parts.push(prim(
        solid(cuboid_tapered(
            [1.35, WALK_H, 0.8],
            0.0,
            slab_mat(step_c, FaceKey::Top),
        )),
        step_c,
        id_quat(),
    ));

    // A bathroom vent hood over each room.
    for (b, &bx) in BAY_X.iter().enumerate() {
        if b == OFFICE_BAY {
            continue;
        }
        parts.push(painted(
            [0.46, 0.34, 0.18],
            [bx, FLOOR + 2.55, BACK + 0.09],
            STEEL_GREY,
        ));
    }
    // Downpipes off the roof's own edge, run down to the lot.
    let head = WALL_TOP + ROOF_T;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.06,
                head - LOT_TOP,
                8,
                0.0,
                steel(STEEL_GREY),
            )),
            [
                sx * (BLOCK_W * 0.5 - 0.45),
                (LOT_TOP + head) * 0.5,
                BACK + 0.1,
            ],
            id_quat(),
        ));
    }
}

/// The road elevation: the brick that *frames* ten openings, the glazing and
/// leaves in them, and a lit room behind every one.
fn elevation(parts: &mut Vec<Generator>) {
    let ops = openings();

    // Piers, from the block's own edge through every gap between openings.
    let mut edges = vec![-BLOCK_W * 0.5];
    for (x, w, _) in &ops {
        edges.push(x - w * 0.5);
        edges.push(x + w * 0.5);
    }
    edges.push(BLOCK_W * 0.5);
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        parts.push(wall(
            [b - a, WALL_H, WALL_T],
            [(a + b) * 0.5, FLOOR + WALL_H * 0.5, WALL_MID],
            FaceKey::SideNz,
        ));
    }

    // Spandrel over every opening, and a sill wall under every window.
    for (x, w, is_door) in &ops {
        parts.push(wall(
            [*w, WALL_H - HEAD, WALL_T],
            [*x, FLOOR + (HEAD + WALL_H) * 0.5, WALL_MID],
            FaceKey::SideNz,
        ));
        if !is_door {
            parts.push(wall(
                [*w, WIN_SILL, WALL_T],
                [*x, FLOOR + WIN_SILL * 0.5, WALL_MID],
                FaceKey::SideNz,
            ));
        }
    }

    for (b, &bx) in BAY_X.iter().enumerate() {
        if b == OFFICE_BAY {
            office(bx, parts);
        } else {
            room(b, bx, parts);
        }
    }
}

/// One guest room: the window and its lit interior, the door in its reveal,
/// the casing and the lit number plaque beside it.
fn room(b: usize, bx: f32, parts: &mut Vec<Generator>) {
    let (dx, wx) = (bx - 0.8, bx + 0.6);
    let cy = FLOOR + WIN_SILL + (HEAD - WIN_SILL) * 0.5;
    let win_h = HEAD - WIN_SILL;

    parts.push(glazing([WIN_W, win_h], [wx, cy, GLAZE_Z], (2, 2)));
    // What the panes show, laid out so the eye lands on something 1.2–2.3 m
    // in rather than on a far wall (#972 lesson 6): a warm rear lining, a bed
    // against it, a curtain drawn across a third of the light, and a lamp.
    parts.push(lit(
        [WIN_W + 1.1, 2.1, 0.08],
        [wx, FLOOR + 1.15, ROOM_Z],
        ROOM_WARM,
        if b == OPEN_ROOM { 0.42 } else { 0.3 },
    ));
    parts.push(lit(
        [1.6, 0.46, 1.15],
        [wx + 0.1, FLOOR + 0.23, ROOM_Z - 0.85],
        [0.55, 0.5, 0.46],
        0.16,
    ));
    parts.push(lit(
        [1.62, 0.1, 1.18],
        [wx + 0.1, FLOOR + 0.5, ROOM_Z - 0.85],
        CURTAIN_PALE,
        0.2,
    ));
    parts.push(prim(
        cuboid_tapered([0.2, 0.22, 0.2], 0.35, glow(SIGN_AMBER, 2.2)),
        [wx - 0.85, FLOOR + 0.82, ROOM_Z - 0.5],
        id_quat(),
    ));
    // Curtain drawn across the left third of the light, just inside the glass.
    parts.push(lit(
        [WIN_W * 0.34, win_h, 0.05],
        [wx - WIN_W * 0.31, cy, GLAZE_Z + 0.16],
        CURTAIN_PALE,
        0.22,
    ));

    // Doorway: a lit hall behind it, the casing round it, and the number.
    parts.push(lit(
        [DOOR_W + 0.7, DOOR_H + 0.3, 0.08],
        [dx, FLOOR + DOOR_H * 0.5, ROOM_Z - 0.6],
        ROOM_WARM,
        0.34,
    ));
    parts.push(painted(
        [DOOR_W + 0.24, 0.13, 0.13],
        [dx, FLOOR + DOOR_H + 0.06, TRIM_Z - 0.02],
        ENAMEL_CREAM,
    ));
    parts.push(prim(
        cuboid_tapered([0.3, 0.2, 0.05], 0.0, glow(SIGN_AMBER, 1.8)),
        [
            dx - DOOR_W * 0.5 - 0.26,
            FLOOR + DOOR_H - 0.24,
            TRIM_Z - 0.03,
        ],
        id_quat(),
    ));

    if b == OPEN_ROOM {
        parts.push(open_leaf(dx));
    } else {
        parts.push(painted(
            [DOOR_W - 0.06, DOOR_H - 0.05, 0.07],
            [dx, FLOOR + (DOOR_H - 0.05) * 0.5, LEAF_Z],
            ENAMEL_BLUE,
        ));
    }
}

/// The one leaf standing open.
///
/// It pivots about its **hinge edge**, which is the single point its centre and
/// its rotation both have to agree about, so both come off one direction vector
/// (#972 lesson 21). `arm` runs hinge → free edge; `quat_y(φ)` sends the leaf's
/// local `+X` to `(cos φ, 0, −sin φ)`, which is `arm` exactly at
/// `φ = DOOR_SWING`.
fn open_leaf(dx: f32) -> Generator {
    let hinge = [dx - DOOR_W * 0.5 + 0.03, LEAF_Z];
    let arm = [DOOR_SWING.cos(), -DOOR_SWING.sin()];
    let leaf_w = DOOR_W - 0.06;
    painted_turned(
        [leaf_w, DOOR_H - 0.05, 0.07],
        [
            hinge[0] + arm[0] * leaf_w * 0.5,
            FLOOR + (DOOR_H - 0.05) * 0.5,
            hinge[1] + arm[1] * leaf_w * 0.5,
        ],
        quat_y(DOOR_SWING),
        ENAMEL_BLUE,
    )
}

/// A painted part carrying a yaw — only the swung leaf, and it is a **leaf
/// node**: a turned parent spins its children's offsets out of the record and
/// out of every translation-only guard at once (#972 lesson 22).
fn painted_turned(
    size: [f32; 3],
    center: [f32; 3],
    rot: crate::pds::Fp4,
    color: [f32; 3],
) -> Generator {
    prim(solid(cuboid_tapered(size, 0.0, enamel(color))), center, rot)
}

/// The office bay: a wider light over a lit reception, its own glazed door, and
/// a key rack on the back wall — the bay needs its own thing to look at, not
/// the room fit-out shifted sideways (#972 lesson 9).
fn office(bx: f32, parts: &mut Vec<Generator>) {
    let (dx, wx) = (bx - 0.9, bx + 0.55);
    let cy = FLOOR + WIN_SILL + (HEAD - WIN_SILL) * 0.5;
    let win_h = HEAD - WIN_SILL;

    parts.push(glazing([1.5, win_h], [wx, cy, GLAZE_Z], (3, 2)));
    parts.push(glazing(
        [0.95 - 0.1, DOOR_H - 0.5],
        [dx, FLOOR + DOOR_H - 0.3 - (DOOR_H - 0.5) * 0.5, GLAZE_Z],
        (2, 3),
    ));
    parts.push(painted(
        [0.95, 0.3, 0.07],
        [dx, FLOOR + 0.15, LEAF_Z],
        ENAMEL_CREAM,
    ));

    // Reception: a counter on the door's centreline, a back bar with a key
    // rack, and a ceiling wash across the whole bay.
    parts.push(lit(
        [3.0, 2.2, 0.08],
        [bx, FLOOR + 1.2, ROOM_Z],
        [0.66, 0.6, 0.5],
        0.34,
    ));
    parts.push(lit(
        [2.1, 1.05, 0.7],
        [bx - 0.1, FLOOR + 0.52, ROOM_Z - 1.1],
        [0.5, 0.42, 0.34],
        0.2,
    ));
    // Key rack on the back wall. `lit_interior` warms its emission by 1.1 on
    // the red channel, so a base above 0.909 there is clamped by the sanitiser
    // and the entry fails its own round trip rather than rendering differently.
    parts.push(lit(
        [1.5, 0.7, 0.06],
        [bx + 0.3, FLOOR + 1.85, ROOM_Z - 0.1],
        [0.88, 0.86, 0.80],
        0.4,
    ));
    parts.push(prim(
        cuboid_tapered([2.6, 0.09, 0.5], 0.0, glow([1.0, 0.96, 0.86], 1.6)),
        [bx, FLOOR + 2.5, ROOM_Z - 0.9],
        id_quat(),
    ));
}

// --- The canopy and the roof. ----------------------------------------------

/// The walkway canopy: a low deck on the posts, the strip's fascia hung off
/// its leading edge, the OFFICE sign on that fascia, and a lit soffit under it.
///
/// The fascia's standoff comes from the deck's **own half depth** rather than
/// by eye, and the sign's from the fascia's, so a deeper deck cannot swallow
/// either (#972 lesson 11).
fn canopy() -> Generator {
    let depth = CANOPY_BACK - CANOPY_FRONT;
    let center = [
        0.0,
        CANOPY_UNDER + CANOPY_T * 0.5,
        (CANOPY_FRONT + CANOPY_BACK) * 0.5,
    ];
    let deck = prim(
        solid(cuboid_tapered([BLOCK_W + 0.6, CANOPY_T, depth], 0.0, {
            let mut m = corrugated(CORRUGATED_GREY);
            m.uv_offset = util::face_uv_offset(FaceKey::Top, center);
            m
        })),
        center,
        id_quat(),
    );

    let deck_face = center[2] - depth * 0.5;
    let fascia_z = deck_face - FASCIA_T * 0.5;
    let fascia_face = fascia_z - FASCIA_T * 0.5;
    let mut parts = vec![
        painted(
            [BLOCK_W + 0.6, FASCIA_H, FASCIA_T],
            [0.0, CANOPY_TOP - FASCIA_H * 0.5, fascia_z],
            ENAMEL_BLUE,
        ),
        // Lit soffit under the walkway, so the covered strip is not a black
        // band at the one hour this prop is lit for (#972 lesson 6).
        lit(
            [BLOCK_W + 0.4, 0.05, depth - 0.3],
            [0.0, CANOPY_UNDER - 0.03, center[2]],
            [0.66, 0.62, 0.54],
            0.26,
        ),
    ];
    for g in sign_board(
        [
            BAY_X[OFFICE_BAY],
            CANOPY_TOP - FASCIA_H * 0.5,
            fascia_face - 0.06,
        ],
        [2.2, 0.34],
        (3, 1),
        SIGN_AMBER,
        2.0,
        -1.0,
    ) {
        parts.push(g);
    }
    nest(deck, parts)
}

/// The block's own roof, over the rooms only: a deck, a cream fascia band round
/// its front, and the plant a motel keeps up there.
fn roof() -> Generator {
    let depth = ROOF_BACK - ROOF_FRONT;
    let center = [0.0, WALL_TOP + ROOF_T * 0.5, (ROOF_FRONT + ROOF_BACK) * 0.5];
    let deck = prim(
        solid(cuboid_tapered([BLOCK_W + 0.5, ROOF_T, depth], 0.0, {
            let mut m = corrugated(CORRUGATED_GREY);
            m.uv_offset = util::face_uv_offset(FaceKey::Top, center);
            m
        })),
        center,
        id_quat(),
    );

    let deck_face = center[2] - depth * 0.5;
    let mut parts = vec![painted(
        [BLOCK_W + 0.5, 0.34, FASCIA_T],
        [0.0, WALL_TOP + ROOF_T - 0.17, deck_face - FASCIA_T * 0.5],
        ENAMEL_CREAM,
    )];
    for (i, x) in [-5.0_f32, -1.0, 3.5].iter().enumerate() {
        parts.push(painted(
            [1.0, 0.55, 0.8],
            [*x, WALL_TOP + ROOF_T + 0.275, 2.2 + i as f32 * 0.3],
            STEEL_GREY,
        ));
    }
    parts.push(prim(
        solid(cylinder_tapered(0.22, 0.7, 10, 0.1, steel(STEEL_GREY))),
        [6.4, WALL_TOP + ROOF_T + 0.35, 3.4],
        id_quat(),
    ));
    nest(deck, parts)
}

// --- The pylon. ------------------------------------------------------------

/// Neon MOTEL pylon on a poured footing, both placed from the **lot's** own
/// extent. The shipped mast stood half a metre past the slab's edge over bare
/// ground, which no angle in a four-tile sheet happens to look along.
fn pylon() -> Generator {
    let pad = 2.0_f32;
    let px = -(LOT_W * 0.5 - pad * 0.5 - 0.4);
    let pz = LOT_FRONT + pad * 0.5 + 0.6;
    let foot_c = [px, LOT_TOP + 0.1, pz];
    let footing = prim(
        solid(cuboid_tapered(
            [pad, 0.2, pad],
            0.0,
            slab_mat(foot_c, FaceKey::Top),
        )),
        foot_c,
        id_quat(),
    );

    let base = LOT_TOP + 0.2;
    let mast_h = 6.2;
    let mut parts = Vec::new();
    let blade_y = base + mast_h + 1.5;
    parts.push(painted([1.8, 3.4, 0.3], [px, blade_y, pz], ENAMEL_CREAM));
    let mut motel = sign_board(
        [px, blade_y + 0.2, pz - 0.35],
        [1.35, 2.6],
        (1, 5),
        NEON_CYAN,
        2.4,
        -1.0,
    );
    motel[1].audio = fx::neon_buzz();
    parts.extend(motel);
    for g in sign_board(
        [px, blade_y - 1.85, pz - 0.35],
        [1.5, 0.6],
        (3, 1),
        NEON_RED,
        2.4,
        -1.0,
    ) {
        parts.push(g);
    }

    let mast = prim(
        solid(cuboid_tapered([0.36, mast_h, 0.36], 0.0, steel(STEEL_GREY))),
        [px, base + mast_h * 0.5, pz],
        id_quat(),
    );
    nest(footing, vec![nest(mast, parts)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive, rotate_by,
    };
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// Axis-aligned half extents of an upright box or drum, or `None` for
    /// anything a translation-only walk cannot honestly report.
    fn footprint(g: &Generator) -> Option<(f32, f32, f32)> {
        if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
            return None;
        }
        match &g.kind {
            GeneratorKind::Cuboid { size, .. } => {
                Some((size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5))
            }
            GeneratorKind::Cylinder { radius, height, .. } => {
                Some((radius.0, height.0 * 0.5, radius.0))
            }
            _ => None,
        }
    }

    /// The sub-root of the lot's `n`th sub-assembly, with its world position —
    /// the shape every footprint guard below leans on (#972 lesson 19).
    fn sub_root(root: &Generator, pick: &dyn Fn(&Generator) -> bool) -> (Generator, [f32; 3]) {
        let base = root.transform.translation.0;
        let c = root
            .children
            .iter()
            .find(|c| pick(c))
            .expect("no such sub-assembly under the lot");
        let t = c.transform.translation.0;
        (c.clone(), [base[0] + t[0], base[1] + t[1], base[2] + t[2]])
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Motel.build(""), "motel");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&Motel.build(""), "motel");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Motel.build(""), "motel");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Motel.build(""), "motel");
    }

    #[test]
    fn keeps_its_neon() {
        assert!(
            has_emissive(&Motel.build("")),
            "the motel lost its pylon, its room lamps or its plaques"
        );
    }

    /// #972 lesson 1: every pane is a card on a flat quad at `uv_scale` 1.0 —
    /// four room windows, the office light and the office door's glazed panel.
    #[test]
    fn every_opening_is_a_card_on_a_quad() {
        let mut cards = 0;
        walk(&Motel.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "a Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 6, "four room windows, the office light and its door");
    }

    /// Every card oversails the opening the brick leaves it, so no edge lands
    /// on the reveal plane (#972 lesson 7). Checked against the **opening
    /// list**, which is where the piers come from too.
    #[test]
    fn every_card_laps_its_opening() {
        let mut widths: Vec<f32> = Vec::new();
        walk(&Motel.build(""), [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                widths.push(size.0[0]);
            }
        });
        for (_, w, is_door) in openings() {
            if is_door {
                continue;
            }
            assert!(
                widths.iter().any(|c| (c - w - GLAZE_LAP).abs() < 1e-4),
                "no card laps the {w} m opening"
            );
            assert!(
                !widths.iter().any(|c| (c - w).abs() < 1e-4),
                "a card sized exactly to a {w} m opening ties with its own reveal"
            );
        }
    }

    /// #972 lesson 18: every masonry and paving slab's `uv_offset` must be some
    /// face's projection of the position the **built tree** puts it at, read
    /// from the composed translation rather than from the constants the
    /// placement used (#972 lesson 21). And the bond itself is flat-coursed:
    /// the generator counts `scale` rows up V and `scale × aspect_ratio`
    /// columns across U, so an aspect above 1 stands every brick on end.
    #[test]
    fn every_clad_surface_shares_one_world_frame() {
        use FaceKey::*;
        let mut checked = 0;
        walk(&Motel.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return;
            }
            // Select by what defines a clad surface: a run of real wall or
            // paving, with a second dimension that is a surface rather than a
            // stick (#972 lesson 24).
            let mut dims = size.0;
            dims.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if dims[2] < 1.5
                || dims[1] < 0.5
                || !matches!(
                    material.texture,
                    SovereignTextureConfig::Brick(_)
                        | SovereignTextureConfig::Concrete(_)
                        | SovereignTextureConfig::Asphalt(_)
                        | SovereignTextureConfig::Corrugated(_)
                )
            {
                return;
            }
            checked += 1;
            if let SovereignTextureConfig::Brick(cfg) = &material.texture {
                let cols = cfg.scale.0 * cfg.aspect_ratio.0;
                assert!(
                    cols < cfg.scale.0,
                    "motel: {cols} columns to {} rows stands every brick on end",
                    cfg.scale.0
                );
                assert!(
                    (cfg.scale.0 * cfg.row_offset.0).fract().abs() < 1e-9,
                    "motel: the bond breaks every {}th course",
                    1.0 / cfg.row_offset.0
                );
            }
            let agrees = [SideNz, SidePz, SideNx, SidePx, Top, Bottom]
                .into_iter()
                .any(|f| {
                    let o = util::face_uv_offset(f, at).0;
                    (o[0] - material.uv_offset.0[0]).abs() < 2e-3
                        && (o[1] - material.uv_offset.0[1]).abs() < 2e-3
                });
            assert!(
                agrees,
                "motel: a clad slab at {at:?} carries uv_offset {:?}, which is no face's \
                 projection of where the built tree puts it",
                material.uv_offset.0
            );
        });
        assert!(
            checked >= 12,
            "only {checked} clad surfaces found — suspect the selector before the content"
        );
    }

    /// #972 lesson 19, and the fault that shipped: **every descendant of the
    /// walkway stands on the walkway.** The posts were 0.6 m off the back of
    /// the old slab, which is exactly the kind of overhang a sub-root-is-the-
    /// surface walk finds in one pass and no render angle shows.
    #[test]
    fn everything_on_the_walkway_stands_on_it() {
        let root = Motel.build("");
        let (walk_root, at0) = sub_root(&root, &|c| match &c.kind {
            GeneratorKind::Cuboid { size, .. } => (size.0[1] - WALK_H).abs() < 1e-4,
            _ => false,
        });
        let mut n = 0;
        for c in &walk_root.children {
            walk(c, at0, &mut |g, at| {
                let Some((hx, _, hz)) = footprint(g) else {
                    return;
                };
                n += 1;
                assert!(
                    at[0].abs() + hx <= (BLOCK_W + 0.6) * 0.5 + 1e-3
                        && (at[2] - (WALK_FRONT + WALK_D * 0.5)).abs() + hz <= WALK_D * 0.5 + 1e-3,
                    "motel: a part at {at:?} (half {hx} × {hz}) hangs off the walkway it \
                     stands on"
                );
            });
        }
        assert!(n >= 7, "only {n} parts on the walkway");
    }

    /// The same shape for the pylon: the mast and its blade stand on the
    /// footing, and the footing stands on the lot.
    #[test]
    fn the_pylon_stands_on_its_footing_and_the_footing_on_the_lot() {
        let root = Motel.build("");
        let (foot, at0) = sub_root(&root, &|c| match &c.kind {
            GeneratorKind::Cuboid { size, .. } => {
                (size.0[0] - 2.0).abs() < 1e-4 && (size.0[1] - 0.2).abs() < 1e-4
            }
            _ => false,
        });
        assert!(
            at0[0].abs() + 1.0 <= LOT_W * 0.5 + 1e-3
                && (at0[2] - LOT_CZ).abs() + 1.0 <= LOT_D * 0.5 + 1e-3,
            "motel: the pylon footing at {at0:?} hangs off the lot"
        );
        let mast = &foot.children[0];
        let mt = mast.transform.translation.0;
        let mast_at = [at0[0] + mt[0], at0[1] + mt[1], at0[2] + mt[2]];
        let (hx, _, hz) = footprint(mast).expect("the footing carries a mast");
        assert!(
            (mast_at[0] - at0[0]).abs() + hx <= 1.0 + 1e-3
                && (mast_at[2] - at0[2]).abs() + hz <= 1.0 + 1e-3,
            "motel: the mast at {mast_at:?} stands off its own footing"
        );
    }

    /// #972 lesson 8: everything standing on the lot has its footprint inside
    /// the lot's, expressed against the lot's own extent.
    #[test]
    fn everything_standing_on_the_lot_is_on_it() {
        let mut checked = 0;
        walk(&Motel.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, hy, hz)) = footprint(g) else {
                return;
            };
            if (at[1] - hy - LOT_TOP).abs() > 0.03 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= LOT_W * 0.5 + 1e-3
                    && (at[2] - LOT_CZ).abs() + hz <= LOT_D * 0.5 + 1e-3,
                "motel: a part at {at:?} (half {hx} × {hz}) stands on the lot and hangs \
                 off it"
            );
        });
        assert!(
            checked >= 4,
            "only {checked} parts found standing on the lot"
        );
    }

    /// #972 lessons 21 and 23: the open leaf's centre and its rotation are two
    /// decisions that must agree, and the hinge edge is the only point both
    /// have to be right about. Read the built node's own quaternion and half
    /// extent and turn it with the one shared [`rotate_by`].
    #[test]
    fn the_open_door_is_hung_on_its_jamb() {
        let root = Motel.build("");
        let mut leaf: Option<([f32; 3], [f32; 4], f32)> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0[1].abs() > 1e-3 && size.0[1] > 1.5 {
                leaf = Some((at, g.transform.rotation.0, size.0[0] * 0.5));
            }
        });
        let (at, q, half) = leaf.expect("no open leaf in the tree");
        let arm = rotate_by(q, [half, 0.0, 0.0]);
        let ends = [
            [at[0] - arm[0], at[2] - arm[2]],
            [at[0] + arm[0], at[2] + arm[2]],
        ];
        let hinge = [BAY_X[OPEN_ROOM] - 0.8 - DOOR_W * 0.5 + 0.03, LEAF_Z];
        assert!(
            ends.iter()
                .any(|e| (e[0] - hinge[0]).abs() < 0.02 && (e[1] - hinge[1]).abs() < 0.02),
            "motel: the leaf's ends are at {ends:?}, neither on the hinge at {hinge:?} — \
             the door is hung on nothing"
        );
        let free = ends
            .iter()
            .find(|e| (e[0] - hinge[0]).abs() >= 0.02 || (e[1] - hinge[1]).abs() >= 0.02)
            .unwrap();
        assert!(
            free[1] < FRONT - 0.4,
            "motel: the leaf's free edge at z {} is still on the wall — a shut door is a \
             darker rectangle, not a way in",
            free[1]
        );
    }

    /// The canopy is only a canopy if it covers what it shelters: it reaches
    /// past every post it stands on and past the walkway's front edge, every
    /// post reaches its underside, and that underside clears the door heads it
    /// spans. Stated as the relationships rather than as the numbers, so a
    /// deeper walkway or a taller door cannot leave it short.
    #[test]
    fn the_canopy_covers_the_walkway_and_clears_its_doors() {
        let root = Motel.build("");
        let mut deck: Option<([f32; 3], [f32; 3])> = None;
        let mut posts: Vec<([f32; 3], f32, f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if (size.0[1] - CANOPY_T).abs() < 1e-4 && size.0[0] > BLOCK_W {
                deck = Some((at, size.0));
            }
            if (size.0[0] - POST_S).abs() < 1e-4 && (size.0[2] - POST_S).abs() < 1e-4 {
                posts.push((at, size.0[1], size.0[2]));
            }
        });
        let (at, size) = deck.expect("no canopy deck");
        assert_eq!(posts.len(), 5, "five posts carry the canopy");
        let front = at[2] - size[2] * 0.5;
        let under = at[1] - size[1] * 0.5;
        assert!(
            front <= WALK_FRONT + 1e-3,
            "the canopy's front edge at {front} stops short of the walkway edge at \
             {WALK_FRONT}"
        );
        assert!(
            under > FLOOR + DOOR_H + 0.1,
            "the canopy's underside at {under} crosses the door heads at {}",
            FLOOR + DOOR_H
        );
        assert!(
            at[1] + size[1] * 0.5 < WALL_TOP - 0.5,
            "the canopy tops out at {} and roofs the elevation it is supposed to shade",
            at[1] + size[1] * 0.5
        );
        for (p, h, hz) in posts {
            assert!(
                p[2] - hz * 0.5 >= front - 1e-3,
                "a post at {p:?} stands outside the canopy it holds up"
            );
            assert!(
                (p[1] + h * 0.5 - under).abs() < 1e-3,
                "a post at {p:?} does not reach the canopy's underside"
            );
        }
    }

    /// The editability contract (#972 lesson 3): the lot carries the walkway,
    /// the block and the pylon; the block carries the roof; the pylon's footing
    /// carries its mast, which carries the signs.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Motel.build("");
        let sized = |g: &Generator, want: [f32; 3]| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => size
                .0
                .iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() < 1e-3),
            _ => false,
        };
        let block = root
            .children
            .iter()
            .find(|c| sized(c, [BLOCK_W + 0.12, FLOOR - LOT_TOP, BLOCK_D + 0.12]))
            .expect("the lot carries the block's base course");
        let deck_of = |t: f32| {
            block.children.iter().find(move |c| match &c.kind {
                GeneratorKind::Cuboid { size, .. } => (size.0[1] - t).abs() < 1e-4,
                _ => false,
            })
        };
        assert!(
            deck_of(CANOPY_T).is_some_and(|c| c.children.len() >= 6),
            "the block carries a canopy that carries its fascia, soffit and sign"
        );
        assert!(
            deck_of(ROOF_T).is_some_and(|c| c.children.len() >= 5),
            "the block carries a roof that carries its fascia and plant"
        );
        let footing = root
            .children
            .iter()
            .find(|c| sized(c, [2.0, 0.2, 2.0]))
            .expect("the lot carries the pylon footing");
        assert_eq!(footing.children.len(), 1, "the footing carries one mast");
        assert!(
            footing.children[0].children.len() >= 8,
            "the mast carries the blade and both sign panels"
        );
        assert!(count(&root) > 100, "the motel lost most of its parts");
    }
}
