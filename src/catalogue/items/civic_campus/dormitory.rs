//! Dormitory — a Civic/Campus secondary. A three-storey brick residence hall:
//! six piers standing proud of recessed spandrel bands, framing fifteen real
//! openings over fifteen lit rooms; a recessed entrance under a bracketed
//! canopy with steps down to its own apron; and a parapet ring over a roof of
//! bulkhead, vents and tank.
//!
//! Rebuilt as a shell under #972. What shipped was a solid brick box with
//! pictures of a building on it:
//!
//! 1. **Three bands of `modern_city::curtain_wall`** — a lit glass *cuboid*
//!    behind proud fins, which that helper's own note says is wrong below
//!    first-floor level, and which handed a `Window` texture is the one thing
//!    it cannot be (#972 lesson 20). The entrance "door" was another glazed
//!    cuboid.
//! 2. **No openings, no reveals, no rooms.** A hall of residence whose whole
//!    subject is *people living behind those windows* had nothing behind any of
//!    them — and the same three bands ran unbroken past the entrance, so the
//!    way in was a flat panel on a wall.
//! 3. **The canopy floated.** A 2.6 × 1.2 concrete slab at 2.8 m with nothing
//!    holding it up and nothing under it: no threshold, no step, no reveal.
//! 4. **Flat list, upright brick, no bonding** (#972 lessons 2 and 3).
//!
//! Now the elevation is the brick that *frames* the openings, every card sits
//! in a reveal over a room that is lit or dim by turns, and the tree is nested
//! plinth → body → storey trim → parapet → roof.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, glow, id_quat, lit_interior, nest, plane, prim, quat_x,
    solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_RED, CONCRETE_GREY, GLASS_TINT, STEEL_GREY, STONE_PALE, brick, concrete, pane_grid,
    steel, stone,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// Hall plan, and the plinth it stands on.
const W: f32 = 11.0;
const D: f32 = 7.5;
const PLINTH_H: f32 = 0.55;
const PLINTH_OVER: f32 = 0.45;
const FLOOR: f32 = PLINTH_H;

/// Three storeys of rooms, and the wall they are cut out of.
const STOREY: f32 = 2.85;
const STOREYS: usize = 3;
const WALL_T: f32 = 0.35;
const BODY_TOP: f32 = FLOOR + STOREYS as f32 * STOREY;

/// The parapet ring over the cornice, and the roof deck inside it.
const CORNICE_H: f32 = 0.3;
const PARAPET_H: f32 = 0.85;
const PARAPET_T: f32 = 0.3;
const ROOF_Y: f32 = BODY_TOP + CORNICE_H;

/// Wall planes and the frames just inside them.
const FRONT: f32 = -D * 0.5;
const WALL_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane in the reveal, and the lit room panel behind it.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;
const ROOM_Z: f32 = FRONT + 1.5;
/// Centre plane of proud trim — string courses, sills, the entrance surround.
const TRIM_Z: f32 = FRONT - 0.07;
/// How far a card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// How far the piers stand proud of the recessed spandrel bands between them.
/// Flush is a coplanar seam running the height of the building on the most
/// looked-at face, and it is invisible in a still.
const PIER_PROUD: f32 = 0.06;

/// One room's window, and the entrance that replaces the centre one at street
/// level.
const WIN_W: f32 = 1.25;
const WIN_SILL: f32 = 0.95;
const WIN_H: f32 = 1.4;
const ENTRY_W: f32 = 1.9;
const ENTRY_H: f32 = 2.55;

/// Bay centres. Five rooms a floor, and the middle one is the way in.
const BAY_X: [f32; 5] = [-4.3, -2.15, 0.0, 2.15, 4.3];
const ENTRY_BAY: usize = 2;

/// Approach apron in front of the entrance — what the steps and everything set
/// down outside actually stand on (#972 lesson 19).
const APRON_W: f32 = 4.6;
const APRON_D: f32 = 2.6;
const APRON_T: f32 = 0.18;
/// The apron laps **under** the plinth's front edge rather than stopping at
/// it: the top tread has to land on the plinth, so an apron that stops short
/// leaves that tread standing on nothing — which is what the guard found on
/// the first build of this rebuild, 0.1 m off its own paving.
const APRON_Z: f32 = FRONT + 0.1 - APRON_D * 0.5;

/// Brick length in metres — a real brick, laid flat, in one course frame the
/// whole building shares (#972 lesson 2).
const BRICK_LEN: f32 = 0.215;

// --- Palette local to this entry. ------------------------------------------

/// A lit room at dusk. Held under 0.909 on the red channel because
/// `lit_interior` warms its emission by 1.1 there, and above that the sanitiser
/// clamps it and the entry fails its own round-trip guard.
const ROOM_LIT: [f32; 3] = [0.86, 0.74, 0.52];
/// A room whose light is off — still a room, just a dark one, so the elevation
/// reads as a building somebody lives in rather than as a lightbox.
const ROOM_DIM: [f32; 3] = [0.30, 0.30, 0.32];
/// Lobby lining and the noticeboard's felt.
const LOBBY_WARM: [f32; 3] = [0.72, 0.66, 0.54];
const NOTICE_FELT: [f32; 3] = [0.20, 0.34, 0.26];

// --- Derived levels. -------------------------------------------------------

/// Floor level of storey `s`.
fn storey_y(s: usize) -> f32 {
    FLOOR + s as f32 * STOREY
}
/// The clear opening of bay `b` on storey `s`, as `(centre_y, [w, h])`. The
/// entrance takes the middle bay at street level and is taller, which is what
/// makes the way in read as a way in.
fn opening(s: usize, b: usize) -> (f32, [f32; 2]) {
    let y = storey_y(s);
    if s == 0 && b == ENTRY_BAY {
        (y + ENTRY_H * 0.5, [ENTRY_W, ENTRY_H])
    } else {
        (y + WIN_SILL + WIN_H * 0.5, [WIN_W, WIN_H])
    }
}
/// Whether that room's light is on. Deterministic, and mixed on purpose: a
/// hall where every window burns is a lightbox, and one where none do is a
/// slab (#972 — the tenement's lesson).
fn is_lit(s: usize, b: usize) -> bool {
    !(s * 5 + b * 3).is_multiple_of(4)
}

// --- Shared construction. --------------------------------------------------

/// Brick laid flat at a real brick's size, in the shared world course frame.
fn wall_mat(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(BRICK_RED), BRICK_LEN, face, center)
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

/// Cast stone in the world frame — plinth, string courses, sills, copings.
fn cast(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = if color == STONE_PALE {
        stone(color)
    } else {
        concrete(color)
    };
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// A cast band, sill or coping.
fn band(size: [f32; 3], center: [f32; 3], color: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, cast(color, center, face))),
        center,
        id_quat(),
    )
}

/// Glazing filling one opening: a card on a flat quad in the reveal, lapped
/// into the brick either side.
fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(GLASS_TINT, 0.45, panes),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit surface inside — what a card's masked-away panes actually show.
fn lit(size: [f32; 3], center: [f32; 3], color: [f32; 3], strength: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(color, strength)),
        center,
        id_quat(),
    )
}

pub struct Dormitory;

impl CatalogueEntry for Dormitory {
    fn slug(&self) -> &'static str {
        "dormitory"
    }
    fn name(&self) -> &'static str {
        "Dormitory"
    }
    fn description(&self) -> &'static str {
        "Three-storey brick residence hall of lit rooms behind a bracketed entrance."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.5,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The hall as a tree that stands the way it does: the plinth at the bottom,
/// the body on it, the cornice and parapet on the body, the roof inside the
/// parapet — with the approach apron its own sub-assembly, so the steps are
/// checked against the paving they land on rather than against the building
/// (#972 lesson 19).
fn build_tree() -> Generator {
    let center = [0.0, PLINTH_H * 0.5, 0.0];
    let plinth = band(
        [W + PLINTH_OVER, PLINTH_H, D + PLINTH_OVER],
        center,
        CONCRETE_GREY,
        FaceKey::SideNz,
    );
    nest(plinth, vec![apron(), body()])
}

/// The approach apron and the flight up onto the plinth.
///
/// Both ends of the flight are derived — the top tread meets the plinth's own
/// top and the bottom one meets the apron — so no riser can float and none can
/// be a climb.
fn apron() -> Generator {
    let center = [0.0, APRON_T * 0.5, APRON_Z];
    let slab = band(
        [APRON_W, APRON_T, APRON_D],
        center,
        CONCRETE_GREY,
        FaceKey::Top,
    );

    let risers = 3;
    let rise = (FLOOR - APRON_T) / risers as f32;
    let going = 0.34;
    let mut parts = Vec::new();
    for i in 0..risers {
        let top = APRON_T + (i + 1) as f32 * rise;
        let c = [
            0.0,
            (APRON_T + top) * 0.5,
            FRONT - going * (risers - 1 - i) as f32 - going * 0.5,
        ];
        parts.push(band(
            [ENTRY_W + 1.0, top - APRON_T, going],
            c,
            STONE_PALE,
            FaceKey::Top,
        ));
    }
    // A pair of bollard lamps flanking the flight, stood on the apron's own
    // extent rather than at a round number off the building.
    for sx in [-1.0_f32, 1.0] {
        let bx = sx * (APRON_W * 0.5 - 0.4);
        parts.push(prim(
            solid(cylinder_tapered(0.11, 0.95, 10, 0.08, steel(STEEL_GREY))),
            [bx, APRON_T + 0.475, APRON_Z - 0.35],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered([0.17, 0.16, 0.17], 0.25, glow([1.0, 0.88, 0.6], 2.2)),
            [bx, APRON_T + 1.02, APRON_Z - 0.35],
            id_quat(),
        ));
    }
    nest(slab, parts)
}

// --- The body. -------------------------------------------------------------

/// Back and flank walls, the road elevation, the string courses that ring all
/// four sides, and the cornice that carries the parapet.
fn body() -> Generator {
    // The back wall is the sub-root: a real slab at the bottom of the body,
    // and the only one of the four that is not cut about.
    let back_c = [
        0.0,
        FLOOR + (BODY_TOP - FLOOR) * 0.5,
        D * 0.5 - WALL_T * 0.5,
    ];
    let root = wall([W, BODY_TOP - FLOOR, WALL_T], back_c, FaceKey::SidePz);

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, BODY_TOP - FLOOR, D - WALL_T * 2.0],
            [sx * (W * 0.5 - WALL_T * 0.5), (FLOOR + BODY_TOP) * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    elevation(&mut parts);
    flank_windows(&mut parts);

    // String course at every upper sill line, ringing all four elevations so
    // the building has a horizontal as well as a vertical grid.
    //
    // A ring is centred on the **building**, oversized in plan — not on the
    // trim plane. Centred at `TRIM_Z` it becomes a 7.7 m slab starting 3.8 m in
    // front of the wall: two cantilevered shelves the width of the site, which
    // is what the first render of this rebuild grew.
    for s in 1..STOREYS {
        let y = storey_y(s) + WIN_SILL - 0.11;
        parts.push(band(
            [W + 0.16, 0.16, D + 0.16],
            [0.0, y, 0.0],
            STONE_PALE,
            FaceKey::SideNz,
        ));
    }
    // Cornice under the parapet.
    let corn_c = [0.0, BODY_TOP + CORNICE_H * 0.5, 0.0];
    parts.push(band(
        [W + 0.3, CORNICE_H, D + 0.3],
        corn_c,
        STONE_PALE,
        FaceKey::SideNz,
    ));
    parts.push(parapet());
    nest(root, parts)
}

/// The road elevation: six brick piers standing proud of the recessed spandrel
/// bands between them, framing fifteen openings, each with its own room.
fn elevation(parts: &mut Vec<Generator>) {
    // Pier edges, derived from the openings rather than authored beside them.
    let mut edges = vec![-W * 0.5];
    for (b, &x) in BAY_X.iter().enumerate() {
        let half = if b == ENTRY_BAY { ENTRY_W } else { WIN_W } * 0.5;
        edges.push(x - half);
        edges.push(x + half);
    }
    edges.push(W * 0.5);

    // Full-height piers, proud.
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        let c = [
            (a + b) * 0.5,
            (FLOOR + BODY_TOP) * 0.5,
            WALL_MID - PIER_PROUD * 0.5,
        ];
        parts.push(wall(
            [b - a, BODY_TOP - FLOOR, WALL_T + PIER_PROUD],
            c,
            FaceKey::SideNz,
        ));
    }

    for s in 0..STOREYS {
        let sy = storey_y(s);
        for (b, &bx) in BAY_X.iter().enumerate() {
            let (cy, size) = opening(s, b);
            let head = cy + size[1] * 0.5;
            let sill = cy - size[1] * 0.5;
            // Recessed spandrel over the opening, and a sill wall under it
            // where the opening does not start at the floor.
            parts.push(wall(
                [size[0], sy + STOREY - head, WALL_T],
                [bx, (head + sy + STOREY) * 0.5, WALL_MID],
                FaceKey::SideNz,
            ));
            if sill > sy + 1e-3 {
                parts.push(wall(
                    [size[0], sill - sy, WALL_T],
                    [bx, (sy + sill) * 0.5, WALL_MID],
                    FaceKey::SideNz,
                ));
                // Cast sill, proud of the brick under every window.
                parts.push(band(
                    [size[0] + 0.24, 0.12, 0.24],
                    [bx, sill - 0.06, TRIM_Z + 0.03],
                    STONE_PALE,
                    FaceKey::Top,
                ));
            }

            if s == 0 && b == ENTRY_BAY {
                entrance(bx, parts);
            } else {
                room(s, b, bx, cy, size, parts);
            }
        }
    }
}

/// One study bedroom behind its window: the glazing, a lit (or dark) rear
/// lining, a bed against it and a desk lamp — enough that the eye lands on
/// something 1.5 m in rather than on a far wall (#972 lesson 6).
fn room(s: usize, b: usize, bx: f32, cy: f32, size: [f32; 2], parts: &mut Vec<Generator>) {
    parts.push(glazing(size, [bx, cy, GLAZE_Z], (2, 2)));
    let on = is_lit(s, b);
    let tone = if on { ROOM_LIT } else { ROOM_DIM };
    parts.push(lit(
        [size[0] + 0.9, size[1] + 0.9, 0.08],
        [bx, cy, ROOM_Z],
        tone,
        if on { 0.4 } else { 0.1 },
    ));
    parts.push(lit(
        [1.35, 0.42, 0.85],
        [bx + 0.1, cy - size[1] * 0.5 + 0.12, ROOM_Z - 0.7],
        [0.5, 0.48, 0.46],
        0.12,
    ));
    if on {
        parts.push(prim(
            cuboid_tapered([0.17, 0.2, 0.17], 0.3, glow([1.0, 0.86, 0.58], 2.0)),
            [bx - 0.42, cy + 0.1, ROOM_Z - 0.45],
            id_quat(),
        ));
    }
    // Casement mullion proud of the glass, so a two-pane card still reads as
    // joinery at street distance.
    parts.push(band(
        [0.07, size[1], 0.07],
        [bx, cy, TRIM_Z + 0.06],
        STONE_PALE,
        FaceKey::SideNz,
    ));
}

/// The way in: a stone surround round a recessed reveal, glazed double doors
/// under a transom, a lit lobby laid out for the bay it stands in (#972
/// lesson 9), and a canopy on brackets that actually reach it.
fn entrance(bx: f32, parts: &mut Vec<Generator>) {
    let head = FLOOR + ENTRY_H;
    // Stone surround, proud of the brick, wider than the opening it frames.
    parts.push(band(
        [ENTRY_W + 0.7, 0.24, 0.3],
        [bx, head + 0.12, TRIM_Z],
        STONE_PALE,
        FaceKey::SideNz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(band(
            [0.35, ENTRY_H + 0.24, 0.3],
            [
                bx + sx * (ENTRY_W * 0.5 + 0.175),
                FLOOR + (ENTRY_H + 0.24) * 0.5,
                TRIM_Z,
            ],
            STONE_PALE,
            FaceKey::SideNz,
        ));
    }

    // Lobby: a lit lining, a porter's desk on the door's centreline, a
    // noticeboard beside it and a ceiling wash across the whole bay.
    parts.push(lit(
        [ENTRY_W + 1.4, ENTRY_H + 0.6, 0.08],
        [bx, FLOOR + ENTRY_H * 0.5, ROOM_Z + 0.5],
        LOBBY_WARM,
        0.38,
    ));
    parts.push(lit(
        [1.5, 1.0, 0.6],
        [bx - 0.15, FLOOR + 0.5, ROOM_Z - 0.3],
        [0.48, 0.38, 0.28],
        0.16,
    ));
    parts.push(lit(
        [0.9, 0.7, 0.05],
        [bx + 1.0, FLOOR + 1.6, ROOM_Z + 0.42],
        NOTICE_FELT,
        0.22,
    ));
    parts.push(prim(
        cuboid_tapered([ENTRY_W, 0.08, 0.9], 0.0, glow([1.0, 0.95, 0.84], 1.6)),
        [bx, head - 0.3, ROOM_Z - 0.5],
        id_quat(),
    ));

    // Glazed double doors in the reveal, with a transom light over them.
    let leaf_h = ENTRY_H - 0.55;
    parts.push(glazing(
        [ENTRY_W - 0.16, leaf_h],
        [bx, FLOOR + leaf_h * 0.5, GLAZE_Z],
        (2, 3),
    ));
    parts.push(glazing(
        [ENTRY_W - 0.16, ENTRY_H - leaf_h - 0.14],
        [bx, FLOOR + (leaf_h + 0.14 + ENTRY_H) * 0.5, GLAZE_Z],
        (3, 1),
    ));
    parts.push(band(
        [ENTRY_W - 0.16, 0.1, 0.14],
        [bx, FLOOR + leaf_h + 0.07, GLAZE_Z - 0.08],
        STONE_PALE,
        FaceKey::SideNz,
    ));

    // Canopy, and the brackets that hold it. Its underside is derived from the
    // door head it has to clear, and each bracket reaches from the wall to the
    // canopy's own soffit — the shipped slab hung at a round 2.8 m on nothing.
    let soffit = head + 0.35;
    let depth = 1.5;
    parts.push(band(
        [ENTRY_W + 1.5, 0.2, depth],
        [bx, soffit + 0.1, FRONT - depth * 0.5 + 0.1],
        CONCRETE_GREY,
        FaceKey::Top,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.12, 0.5, 1.1], 0.0, steel(STEEL_GREY))),
            [bx + sx * (ENTRY_W * 0.5 + 0.5), soffit - 0.25, FRONT - 0.55],
            id_quat(),
        ));
    }
    // Lamp under the canopy — below anything that spans the head, so it lights
    // something the approach can see (#972 lesson 10).
    parts.push(prim(
        cuboid_tapered([0.3, 0.12, 0.3], 0.2, glow([1.0, 0.9, 0.68], 2.4)),
        [bx, soffit - 0.08, FRONT - 0.6],
        id_quat(),
    ));
}

/// A stair light per storey on each flank, so the ends are a building rather
/// than two blank brick slabs.
fn flank_windows(parts: &mut Vec<Generator>) {
    for sx in [-1.0_f32, 1.0] {
        for s in 0..STOREYS {
            let cy = storey_y(s) + WIN_SILL + WIN_H * 0.5;
            // Reveal box proud of the flank, with the light inside it. The
            // panel stands proud of the wall, not inside it (#972 lesson 11).
            parts.push(band(
                [0.14, WIN_H + 0.3, WIN_W + 0.3],
                [sx * (W * 0.5 - 0.02), cy, 0.6],
                STONE_PALE,
                if sx > 0.0 {
                    FaceKey::SidePx
                } else {
                    FaceKey::SideNx
                },
            ));
            parts.push(lit(
                [0.06, WIN_H, WIN_W],
                [sx * (W * 0.5 + 0.06), cy, 0.6],
                if is_lit(s, 4) { ROOM_LIT } else { ROOM_DIM },
                0.3,
            ));
        }
    }
}

// --- The parapet and the roof. ---------------------------------------------

/// A ring of four parapet walls with their own copings, and inside it the roof
/// deck with its bulkhead, vents and tank.
fn parapet() -> Generator {
    let front_c = [
        0.0,
        ROOF_Y + PARAPET_H * 0.5,
        -(D * 0.5 + 0.15 - PARAPET_T * 0.5),
    ];
    let root = wall([W + 0.3, PARAPET_H, PARAPET_T], front_c, FaceKey::SideNz);

    let mut parts = vec![wall(
        [W + 0.3, PARAPET_H, PARAPET_T],
        [
            0.0,
            ROOF_Y + PARAPET_H * 0.5,
            D * 0.5 + 0.15 - PARAPET_T * 0.5,
        ],
        FaceKey::SidePz,
    )];
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [PARAPET_T, PARAPET_H, D + 0.3 - PARAPET_T * 2.0],
            [
                sx * (W * 0.5 + 0.15 - PARAPET_T * 0.5),
                ROOF_Y + PARAPET_H * 0.5,
                0.0,
            ],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    // Copings, one per run, oversailing their own wall.
    let cop_y = ROOF_Y + PARAPET_H + 0.05;
    for sz in [-1.0_f32, 1.0] {
        parts.push(band(
            [W + 0.44, 0.1, PARAPET_T + 0.14],
            [0.0, cop_y, sz * (D * 0.5 + 0.15 - PARAPET_T * 0.5)],
            STONE_PALE,
            FaceKey::Top,
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(band(
            [PARAPET_T + 0.14, 0.1, D + 0.44],
            [sx * (W * 0.5 + 0.15 - PARAPET_T * 0.5), cop_y, 0.0],
            STONE_PALE,
            FaceKey::Top,
        ));
    }

    parts.push(roof());
    nest(root, parts)
}

/// The roof deck and what stands on it.
fn roof() -> Generator {
    let center = [0.0, ROOF_Y + 0.08, 0.0];
    let deck = band(
        [W - 0.2, 0.16, D - 0.2],
        center,
        CONCRETE_GREY,
        FaceKey::Top,
    );
    let top = ROOF_Y + 0.16;

    let mut parts = vec![
        // Stair bulkhead with its own door.
        wall([2.4, 2.2, 2.0], [-2.6, top + 1.1, 1.4], FaceKey::SideNz),
        band(
            [2.6, 0.14, 2.2],
            [-2.6, top + 2.27, 1.4],
            CONCRETE_GREY,
            FaceKey::Top,
        ),
    ];
    // Vents.
    for (i, x) in [0.9_f32, 2.4].iter().enumerate() {
        parts.push(prim(
            solid(cylinder_tapered(0.24, 0.9, 10, 0.06, steel(STEEL_GREY))),
            [*x, top + 0.45, -1.2 + i as f32 * 0.5],
            id_quat(),
        ));
        parts.push(prim(
            solid(cylinder_tapered(0.3, 0.12, 10, 0.4, steel(STEEL_GREY))),
            [*x, top + 0.96, -1.2 + i as f32 * 0.5],
            id_quat(),
        ));
    }
    // Water tank on a braced stand.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cuboid_tapered([0.11, 0.9, 0.11], 0.0, steel(STEEL_GREY))),
            [3.1 + sx * 0.6, top + 0.45, 1.6 + sz * 0.6],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cylinder_tapered(0.85, 1.1, 14, 0.05, steel(STEEL_GREY))),
        [3.1, top + 1.45, 1.6],
        id_quat(),
    ));
    nest(deck, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive,
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

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Dormitory.build(""), "dormitory");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&Dormitory.build(""), "dormitory");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Dormitory.build(""), "dormitory");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Dormitory.build(""), "dormitory");
    }

    #[test]
    fn keeps_its_lit_rooms() {
        assert!(
            has_emissive(&Dormitory.build("")),
            "the hall lost its room lamps and its entrance lantern"
        );
    }

    /// #972 lesson 1: every pane is a card on a flat quad at `uv_scale` 1.0 —
    /// fourteen room windows plus the entrance's doors and transom.
    #[test]
    fn every_opening_is_a_card_on_a_quad() {
        let mut cards = 0;
        walk(&Dormitory.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "a Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 16, "fourteen rooms, the doors and the transom");
    }

    /// #972 lesson 7: every room card oversails the opening the brick leaves
    /// it, checked against [`opening`] — which is where the piers, spandrels
    /// and sills all come from too.
    #[test]
    fn every_card_laps_its_opening() {
        let mut sizes: Vec<[f32; 2]> = Vec::new();
        walk(&Dormitory.build(""), [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                sizes.push(size.0);
            }
        });
        for s in 0..STOREYS {
            for b in 0..BAY_X.len() {
                if s == 0 && b == ENTRY_BAY {
                    continue;
                }
                let (_, o) = opening(s, b);
                assert!(
                    sizes.iter().any(|c| (c[0] - o[0] - GLAZE_LAP).abs() < 1e-4
                        && (c[1] - o[1] - GLAZE_LAP).abs() < 1e-4),
                    "no card laps the {o:?} opening on storey {s} bay {b}"
                );
            }
        }
        assert!(
            !sizes.iter().any(|c| (c[0] - WIN_W).abs() < 1e-4),
            "a card sized exactly to a {WIN_W} m opening ties with its own reveal"
        );
    }

    /// Every room card sits between its own storey's sill wall and the spandrel
    /// above it. Both are derived from [`opening`], so this is a check on the
    /// *relationship* rather than on the numbers: raise a sill or lengthen a
    /// storey and the guard follows.
    #[test]
    fn every_card_sits_inside_its_own_storey() {
        let mut cards: Vec<([f32; 3], [f32; 2])> = Vec::new();
        walk(&Dormitory.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                cards.push((at, size.0));
            }
        });
        for (at, size) in &cards {
            let bot = at[1] - size[1] * 0.5;
            let top = at[1] + size[1] * 0.5;
            let s = ((at[1] - FLOOR) / STOREY).floor().max(0.0) as usize;
            let sy = storey_y(s.min(STOREYS - 1));
            assert!(
                bot >= sy - GLAZE_LAP && top <= sy + STOREY + GLAZE_LAP,
                "a card at {at:?} straddles the floor at {}",
                sy + STOREY
            );
        }
        assert_eq!(cards.len(), 16);
    }

    /// #972 lesson 18: every masonry and cast slab's `uv_offset` is some face's
    /// projection of the position the **built tree** puts it at, read from the
    /// composed translation rather than from the constants the placement used
    /// (#972 lesson 21) — and the bond itself is flat-coursed.
    #[test]
    fn every_clad_surface_shares_one_world_frame() {
        use FaceKey::*;
        let mut checked = 0;
        walk(&Dormitory.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return;
            }
            // Select by what defines a clad surface: a run of real wall, with a
            // second dimension that is a surface rather than a stick (#972
            // lesson 24 — a first draft of this on the greenhouse asked for two
            // dimensions over 0.9 m and quietly skipped the dwarf wall).
            let mut dims = size.0;
            dims.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if dims[2] < 1.5
                || dims[1] < 0.5
                || !matches!(
                    material.texture,
                    SovereignTextureConfig::Brick(_)
                        | SovereignTextureConfig::Concrete(_)
                        | SovereignTextureConfig::Ashlar(_)
                )
            {
                return;
            }
            checked += 1;
            if let SovereignTextureConfig::Brick(cfg) = &material.texture {
                assert!(
                    cfg.scale.0 * cfg.aspect_ratio.0 < cfg.scale.0,
                    "dormitory: {} columns to {} rows stands every brick on end",
                    cfg.scale.0 * cfg.aspect_ratio.0,
                    cfg.scale.0
                );
                assert!(
                    (cfg.scale.0 * cfg.row_offset.0).fract().abs() < 1e-9,
                    "dormitory: the bond breaks every {}th course",
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
                "dormitory: a clad slab at {at:?} carries uv_offset {:?}, which is no \
                 face's projection of where the built tree puts it",
                material.uv_offset.0
            );
        });
        assert!(
            checked >= 20,
            "only {checked} clad surfaces found — suspect the selector before the content"
        );
    }

    /// #972 lesson 8: everything standing on the plinth has its footprint
    /// inside the plinth's.
    #[test]
    fn everything_standing_on_the_plinth_is_on_it() {
        let mut checked = 0;
        walk(&Dormitory.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, hy, hz)) = footprint(g) else {
                return;
            };
            if (at[1] - hy - FLOOR).abs() > 0.03 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= (W + PLINTH_OVER) * 0.5 + 1e-3
                    && at[2].abs() + hz <= (D + PLINTH_OVER) * 0.5 + 1e-3,
                "dormitory: a part at {at:?} (half {hx} × {hz}) stands on the plinth and \
                 hangs off it"
            );
        });
        assert!(
            checked >= 8,
            "only {checked} parts found standing on the plinth"
        );
    }

    /// #972 lesson 19: the sub-root **is** the surface, so every descendant of
    /// the apron is checked against the apron rather than against the building
    /// beside it.
    #[test]
    fn everything_on_the_apron_is_on_the_apron() {
        let root = Dormitory.build("");
        let base = root.transform.translation.0;
        let apron = root
            .children
            .iter()
            .find(|c| c.transform.translation.0[2] < -1.0)
            .expect("the plinth carries the apron");
        let at0 = [
            base[0] + apron.transform.translation.0[0],
            base[1] + apron.transform.translation.0[1],
            base[2] + apron.transform.translation.0[2],
        ];
        let mut n = 0;
        for c in &apron.children {
            walk(c, at0, &mut |g, at| {
                let Some((hx, _, hz)) = footprint(g) else {
                    return;
                };
                n += 1;
                assert!(
                    at[0].abs() + hx <= APRON_W * 0.5 + 1e-3
                        && (at[2] - APRON_Z).abs() + hz <= APRON_D * 0.5 + 1e-3,
                    "dormitory: a part at {at:?} hangs off the apron it stands on"
                );
            });
        }
        assert!(n >= 5, "only {n} parts on the apron");
    }

    /// The flight climbs from the apron to the plinth in equal risers, and its
    /// top tread meets the plinth's own top. The shipped entry had no way in at
    /// all, which is the version of this fault a render cannot even pose.
    #[test]
    fn the_steps_reach_the_plinth_in_even_risers() {
        let root = Dormitory.build("");
        let mut treads: Vec<f32> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            // Treads are the only slabs as wide as the entrance-plus-cheeks and
            // shallower than half a metre, standing off the front of the plinth.
            if (size.0[0] - (ENTRY_W + 1.0)).abs() < 1e-4 && size.0[2] < 0.5 && at[2] < FRONT {
                treads.push(at[1] + size.0[1] * 0.5);
            }
        });
        assert_eq!(treads.len(), 3, "a three-riser flight");
        treads.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (treads[2] - FLOOR).abs() < 1e-3,
            "the top tread at {} does not meet the plinth at {FLOOR}",
            treads[2]
        );
        let rise = treads[0] - APRON_T;
        assert!(rise < 0.2, "a {rise} m riser is a climb, not a step");
        for pair in treads.windows(2) {
            assert!(
                (pair[1] - pair[0] - rise).abs() < 1e-3,
                "uneven riser between {} and {}",
                pair[0],
                pair[1]
            );
        }
    }

    /// The entrance canopy clears the door head it shelters, and each bracket
    /// reaches from the wall to the canopy's own soffit. The shipped slab hung
    /// at a round 2.8 m on nothing at all — a relationship stated as a number,
    /// which is exactly the shape that goes wrong when either end moves.
    #[test]
    fn the_canopy_clears_the_door_and_its_brackets_reach_it() {
        let root = Dormitory.build("");
        let mut canopy: Option<([f32; 3], [f32; 3])> = None;
        let mut brackets: Vec<([f32; 3], f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if (size.0[0] - (ENTRY_W + 1.5)).abs() < 1e-4 {
                canopy = Some((at, size.0));
            }
            if matches!(material.texture, SovereignTextureConfig::Metal(_))
                && (size.0[1] - 0.5).abs() < 1e-4
                && at[2] < FRONT
            {
                brackets.push((at, size.0[1]));
            }
        });
        let (at, size) = canopy.expect("no entrance canopy");
        let soffit = at[1] - size[1] * 0.5;
        assert!(
            soffit > FLOOR + ENTRY_H + 0.1,
            "the canopy's soffit at {soffit} crosses the door head at {}",
            FLOOR + ENTRY_H
        );
        assert_eq!(brackets.len(), 2, "two brackets carry the canopy");
        for (b, h) in brackets {
            assert!(
                (b[1] + h * 0.5 - soffit).abs() < 1e-3,
                "a bracket at {b:?} tops out at {} and does not reach the soffit at \
                 {soffit}",
                b[1] + h * 0.5
            );
        }
    }

    /// The parapet is a ring of four walls with their own copings, enclosing
    /// the roof deck — not a solid cap. A cap is what shipped, and it is why
    /// the hall had no roof to put anything on.
    #[test]
    fn the_parapet_rings_the_roof_it_encloses() {
        let root = Dormitory.build("");
        let mut runs = 0;
        let mut deck: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if (size.0[1] - PARAPET_H).abs() < 1e-4 && at[1] > BODY_TOP {
                runs += 1;
            }
            if (size.0[0] - (W - 0.2)).abs() < 1e-4 && at[1] > BODY_TOP {
                deck = Some((at, size.0));
            }
        });
        assert_eq!(runs, 4, "a parapet is four runs, not a slab");
        let (at, size) = deck.expect("no roof deck inside the parapet");
        assert!(
            at[0].abs() + size[0] * 0.5 <= W * 0.5 + 0.15 + 1e-3,
            "the roof deck oversails the parapet that encloses it"
        );
        // ...and something stands on it.
        let mut on_roof = 0;
        walk(&root, [0.0; 3], &mut |g, a| {
            if let Some((_, hy, _)) = footprint(g)
                && (a[1] - hy - (at[1] + size[1] * 0.5)).abs() < 0.03
            {
                on_roof += 1;
            }
        });
        assert!(on_roof >= 5, "only {on_roof} things stand on the roof deck");
    }

    /// The elevation reads as a building somebody lives in: some rooms lit,
    /// some dark. All-on is a lightbox and all-off is a slab.
    #[test]
    fn the_rooms_are_not_all_lit_the_same() {
        let on = (0..STOREYS)
            .flat_map(|s| (0..BAY_X.len()).map(move |b| (s, b)))
            .filter(|&(s, b)| !(s == 0 && b == ENTRY_BAY))
            .filter(|&(s, b)| is_lit(s, b))
            .count();
        assert!(
            (4..=11).contains(&on),
            "{on} of 14 rooms lit reads as a lightbox or a slab"
        );
    }

    /// The editability contract (#972 lesson 3): the plinth carries the apron
    /// and the body, the body carries the parapet, the parapet carries the
    /// roof. Sub-roots selected by the property that defines them, not by child
    /// count, which changes the moment a part is added.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Dormitory.build("");
        assert_eq!(root.children.len(), 2, "the plinth carries apron + body");
        let sized = |g: &Generator, want: [f32; 3]| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => size
                .0
                .iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() < 1e-3),
            _ => false,
        };
        let body = root
            .children
            .iter()
            .find(|c| sized(c, [W, BODY_TOP - FLOOR, WALL_T]))
            .expect("the plinth carries the body's back wall");
        let parapet = body
            .children
            .iter()
            .find(|c| sized(c, [W + 0.3, PARAPET_H, PARAPET_T]))
            .expect("the body carries the parapet");
        assert!(
            parapet
                .children
                .iter()
                .any(|c| sized(c, [W - 0.2, 0.16, D - 0.2]) && c.children.len() >= 8),
            "the parapet carries a roof deck that carries its own plant"
        );
        assert!(count(&root) > 150, "the hall lost most of its parts");
    }
}
