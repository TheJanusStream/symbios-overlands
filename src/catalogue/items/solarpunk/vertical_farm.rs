//! Vertical farm — a Solarpunk secondary. A four-storey grow tower: a
//! structural frame of columns and floor bands framing eight glazed grow halls,
//! each with racked crops under a grow-light fixed to the deck above; a green
//! curtain on real trellis wire down both flanks; an entrance and a produce
//! dock at street level; and a roof of solar array, tank and garden.
//!
//! Rebuilt as a shell under #972. What shipped was four shelves and four sheets
//! of glass hung on a concrete post:
//!
//! 1. **The glazing was four `Window`-textured cuboids.** The generator masks
//!    its panes away, so each was a frame with holes onto the terrace behind it
//!    (#972 lesson 20).
//! 2. **Nothing held anything up.** The terrace shelves were 4.6 m wide on a
//!    2.6 m core — a metre of cantilever each side into thin air — and each
//!    grow-light strip floated half a metre under the shelf it was described as
//!    being fixed to. The shelves also oversailed the base slab (#972 lesson 8).
//! 3. **The green curtain was a flat plate.** An 0.18 × 8.3 × 1.9 cuboid of
//!    plain green on each flank: the flat-lightbox gotcha in its planted form,
//!    and it reads as painted concrete from every angle.
//! 4. **No way in and nothing at street level.** A food tower with no door, no
//!    dock and no lit ground floor, over a flat 30-prim list.
//!
//! Now the load path is visible — three columns carry four floor bands, the
//! bands carry the glazing and the racks, and each grow-light hangs off the
//! underside of the deck above it — and the tree is nested the same way, deck
//! on deck, so one gizmo drag moves a storey and everything in it.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest, plane,
    prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, GLASS_CLEAN, GROW_PINK, LEAF_GREEN, PV_BLUE, SOIL_DARK, STEEL_GREY,
    STEEL_WHITE, TIMBER_WARM, WATER_BLUE, concrete, crop_tufts, foliage, pane_grid, pv, steel,
    timber, water,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// The poured base, and the interior floor level it sets.
const PAD_W: f32 = 6.4;
const PAD_D: f32 = 5.6;
const PAD_T: f32 = 0.35;
const FLOOR: f32 = PAD_T;

/// Tower plan. `FRONT` is the `−Z` hero face the render tool and the settlement
/// placer both look down.
const W: f32 = 5.4;
const D: f32 = 4.2;
const FRONT: f32 = -D * 0.5;
const BACK: f32 = D * 0.5;
const WALL_T: f32 = 0.3;

/// Street storey — entrance and produce dock — then four grow decks on it.
const GROUND_H: f32 = 3.0;
const GROUND_TOP: f32 = FLOOR + GROUND_H;
const LEVELS: usize = 4;
const LEVEL_H: f32 = 1.95;
/// Depth of the floor band that shows on the elevation, and thickness of the
/// deck slab behind it.
const BAND_H: f32 = 0.42;
const DECK_T: f32 = 0.16;
const TOWER_TOP: f32 = GROUND_TOP + LEVELS as f32 * LEVEL_H;

/// Column stock and centres. Three columns, two bays a level.
const COL: f32 = 0.4;
const COL_X: [f32; 3] = [-2.5, 0.0, 2.5];
/// Column centre plane; the floor bands sit behind it so the frame reads as
/// frame, and the glazing behind them again.
const COL_Z: f32 = FRONT + COL * 0.5;
const BAND_Z: f32 = FRONT + 0.32;
const BAND_D: f32 = 0.36;
const GLAZE_Z: f32 = FRONT + 0.44;
/// Rear lining of a grow hall — held about a metre and a half in, so the eye
/// lands on the racks rather than on a far wall (#972 lesson 6).
const HALL_Z: f32 = FRONT + 1.5;

/// Length of the hangers a grow-light is suspended on, and the light's own
/// depth.
///
/// The light is a **pendant**, not a strip pressed to the soffit, and that is a
/// sightline decision rather than a styling one. A bar whose top is the deck
/// above sits exactly in the shadow of the opening's own head: from the street
/// the eye enters an opening at a downward angle, so at 0.7 m behind the head
/// everything within ~0.16 m of the soffit is hidden by the reveal — which put
/// the one element that says "this is a grow tower" out of sight in the first
/// render of this rebuild. Dropped on hangers it clears the head, and the
/// hangers are what say it is fixed to anything (#972 lesson 10).
const HANGER: f32 = 0.34;
const LIGHT_H: f32 = 0.14;

/// Service core across the back: stair, lift and risers.
const CORE_D: f32 = 1.5;
const CORE_FRONT: f32 = BACK - CORE_D;

/// How far a glazing card oversails its opening (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Street-level openings: the entrance screen and the produce dock.
const ENTRY_H: f32 = 2.4;
const DOCK_H: f32 = 2.6;

// --- Palette local to this entry. ------------------------------------------

/// Lit lining of a grow hall — pale, so the racks read against it.
const HALL_PALE: f32 = 0.34;
const HALL_LINING: [f32; 3] = [0.72, 0.74, 0.68];
/// Lobby and dock lighting, warmer than the halls above.
const STREET_WARM: [f32; 3] = [0.68, 0.62, 0.48];
/// Produce crates on the dock.
const CRATE_TAN: [f32; 3] = [0.62, 0.52, 0.34];
/// Stair-light glazing on the core. Deliberately much darker than the concrete
/// it is set into: a pale panel on a pale wall is a change of tone nobody sees,
/// and at midday a real stair window reads as a dark rectangle.
const STAIR_GLASS: [f32; 3] = [0.26, 0.32, 0.31];

// --- Derived levels. -------------------------------------------------------

/// Floor level of grow deck `k` — the height its band and its slab sit at, and
/// the underside its grow-light hangs from is the *next* one up.
fn level_y(k: usize) -> f32 {
    GROUND_TOP + k as f32 * LEVEL_H
}
/// The clear glazed height of one grow hall: the level's own band eats the
/// bottom of it, and the next level's slab closes the top.
fn hall_span(k: usize) -> (f32, f32) {
    (level_y(k) + BAND_H, level_y(k + 1))
}
/// The two clear bay widths between the three columns, as `(centre, width)`.
fn bays() -> [(f32, f32); 2] {
    let a = COL_X[0] + COL * 0.5;
    let b = COL_X[1] - COL * 0.5;
    let c = COL_X[1] + COL * 0.5;
    let d = COL_X[2] - COL * 0.5;
    [((a + b) * 0.5, b - a), ((c + d) * 0.5, d - c)]
}

// --- Shared construction. --------------------------------------------------

/// Pale eco-concrete in the world frame, so the board marks run through a
/// corner instead of restarting at every slab's own centre.
fn cast(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = concrete(CONCRETE_PALE);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One cast slab of the frame. The centre is bound once and handed to the
/// material *and* the transform — passing a bonding helper a different reading
/// of "the middle of the slab" is the one way to defeat the frame guard
/// silently (#972 lesson 18).
fn slab(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, cast(center, face))),
        center,
        id_quat(),
    )
}

/// Panes across and down for an opening, so a 2 m grow hall and a 1 m sidelight
/// get lights of roughly the same size rather than the same count.
fn panes(size: [f32; 2]) -> (u32, u32) {
    let n = |m: f32| ((m / 0.75).round() as u32).clamp(1, 8);
    (n(size[0]), n(size[1]))
}

/// Glazing filling one opening: a card on a flat quad in the reveal, lapped
/// into the frame either side.
fn glazing(size: [f32; 2], center: [f32; 3], lit: f32) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(GLASS_CLEAN, lit, panes(size)),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit surface inside the tower — what a card's masked-away panes show.
fn lit(size: [f32; 3], center: [f32; 3], color: [f32; 3], strength: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(color, strength)),
        center,
        id_quat(),
    )
}

pub struct VerticalFarm;

impl CatalogueEntry for VerticalFarm {
    fn slug(&self) -> &'static str {
        "vertical_farm"
    }
    fn name(&self) -> &'static str {
        "Vertical Farm"
    }
    fn description(&self) -> &'static str {
        "Four-storey grow tower of racked crops under grow-lights, behind a planted trellis."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The tower as a tree that stands the way it does: the pad at the bottom, the
/// street storey on it, then **deck on deck** all the way up, with the roof on
/// the top deck. One gizmo drag moves a storey and everything it holds — which
/// is the whole editability contract, and what a flat thirty-prim list cannot
/// give (#972 lesson 3).
fn build_tree() -> Generator {
    let center = [0.0, PAD_T * 0.5, 0.0];
    let pad = prim(
        solid(cuboid_tapered(
            [PAD_W, PAD_T, PAD_D],
            0.0,
            cast(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );
    // Buried plinth under the pad, so a slope-snapped tower shows stone
    // instead of daylight under its downhill edge.
    nest(pad, vec![street(), footing(PAD_W, PAD_D, [0.0, 0.0], 7.0)])
}

// --- The street storey. ----------------------------------------------------

/// Ground floor slab, and on it the frame, the flanks, the core, the entrance,
/// the dock, the green curtain — and the first grow deck.
fn street() -> Generator {
    let center = [0.0, FLOOR + DECK_T * 0.5, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [W, DECK_T, D],
            0.0,
            cast(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );

    let mut parts = Vec::new();
    // Columns, full height, standing on the pad and carrying every deck above.
    for &x in &COL_X {
        parts.push(slab(
            [COL, TOWER_TOP - FLOOR, COL],
            [x, (FLOOR + TOWER_TOP) * 0.5, COL_Z],
            FaceKey::SideNz,
        ));
    }
    // Flanks and the service core across the back, full height.
    for sx in [-1.0_f32, 1.0] {
        parts.push(slab(
            [WALL_T, TOWER_TOP - FLOOR, D - WALL_T * 2.0],
            [
                sx * (W * 0.5 - WALL_T * 0.5),
                (FLOOR + TOWER_TOP) * 0.5,
                0.0,
            ],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
        parts.push(green_curtain(sx));
    }
    parts.push(slab(
        [W, TOWER_TOP - FLOOR, CORE_D],
        [0.0, (FLOOR + TOWER_TOP) * 0.5, BACK - CORE_D * 0.5],
        FaceKey::SidePz,
    ));
    parts.push(core_elevation());

    entrance(&mut parts);
    dock(&mut parts);
    // Head band closing the street storey under the first grow deck.
    parts.push(slab(
        [W, GROUND_TOP - (FLOOR + DOCK_H), BAND_D],
        [0.0, (FLOOR + DOCK_H + GROUND_TOP) * 0.5, BAND_Z],
        FaceKey::SideNz,
    ));

    // The stack: written innermost first, so each deck carries the one above.
    let mut stack = roof();
    for k in (0..LEVELS).rev() {
        stack = grow_deck(k, stack);
    }
    parts.push(stack);

    nest(deck, parts)
}

/// The service side of the core: a riser pilaster, the stair's own lit lights
/// and a caged ladder.
///
/// The first draft of this put five lit panels at `BACK − 0.04` — a standoff
/// picked by eye, which put every one of them **inside** the 1.5 m core they
/// were mounted on and left the back a blank slab. Each standoff here comes off
/// the wall's own face and the part's own half depth (#972 lesson 11), and the
/// surround is embedded 20 mm rather than sitting flush, so no face shares a
/// plane with the wall.
fn core_elevation() -> Generator {
    let riser_d = 0.35;
    let riser_c = [-1.9, (FLOOR + TOWER_TOP) * 0.5, BACK + riser_d * 0.5 - 0.02];
    let riser = slab([0.6, TOWER_TOP - FLOOR, riser_d], riser_c, FaceKey::SidePz);

    let mut parts = Vec::new();
    for k in 0..5 {
        let y = FLOOR + 1.6 + k as f32 * 2.1;
        // The light itself, embedded 5 mm so it does not share a plane with
        // the wall and standing 45 mm proud of it.
        let glass_t = 0.05;
        let gz = BACK - 0.005 + glass_t * 0.5;
        parts.push(prim(
            cuboid_tapered([0.56, 0.78, glass_t], 0.0, lit_interior(STAIR_GLASS, 0.3)),
            [1.3, y, gz],
            id_quat(),
        ));
        // Its surround is a **frame**, not a slab. The first version was a
        // 0.12 m cast box centred 20 mm proud of the wall with the light placed
        // at a hand-picked `BACK + 0.06`, which is inside it. That is #972
        // lesson 11 twice in one file: derive the mounted part's standoff from
        // its host, and then check the host is not *in front of* it.
        let bar = 0.08;
        let (fw, fh) = (0.72_f32, 0.95_f32);
        for sy in [-1.0_f32, 1.0] {
            parts.push(slab(
                [fw, bar, 0.1],
                [1.3, y + sy * (fh - bar) * 0.5, BACK + 0.03],
                FaceKey::SidePz,
            ));
        }
        for sx in [-1.0_f32, 1.0] {
            parts.push(slab(
                [bar, fh - bar * 2.0, 0.1],
                [1.3 + sx * (fw - bar) * 0.5, y, BACK + 0.03],
                FaceKey::SidePz,
            ));
        }
    }
    // Caged ladder up the far side of the riser.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.03,
                TOWER_TOP - FLOOR - 1.0,
                6,
                0.0,
                steel(STEEL_GREY),
            )),
            [
                -1.9 + sx * 0.22,
                (FLOOR + 1.0 + TOWER_TOP) * 0.5,
                BACK + riser_d + 0.05,
            ],
            id_quat(),
        ));
    }
    for k in 0..14 {
        parts.push(prim(
            solid(cuboid_tapered([0.48, 0.04, 0.04], 0.0, steel(STEEL_GREY))),
            [-1.9, FLOOR + 1.2 + k as f32 * 0.68, BACK + riser_d + 0.05],
            id_quat(),
        ));
    }
    nest(riser, parts)
}

/// The entrance: a glazed screen over a lit lobby, with a canopy over the door.
fn entrance(parts: &mut Vec<Generator>) {
    let (bx, bw) = bays()[0];
    let head = FLOOR + ENTRY_H;
    let size = [bw, ENTRY_H];
    parts.push(glazing(size, [bx, FLOOR + ENTRY_H * 0.5, GLAZE_Z], 0.4));
    parts.push(slab(
        [bw, FLOOR + DOCK_H - head, BAND_D],
        [bx, (head + FLOOR + DOCK_H) * 0.5, BAND_Z],
        FaceKey::SideNz,
    ));
    // Lobby: a counter of trays on the entrance centreline, a lit lining behind
    // it, and a ceiling wash — the bay needs its own thing to look at, not the
    // dock's fit-out shifted sideways (#972 lesson 9).
    parts.push(lit(
        [bw + 0.6, ENTRY_H, 0.08],
        [bx, FLOOR + ENTRY_H * 0.5, HALL_Z + 0.4],
        STREET_WARM,
        0.32,
    ));
    parts.push(prim(
        solid(cuboid_tapered([1.7, 0.95, 0.6], 0.0, timber(TIMBER_WARM))),
        [bx, FLOOR + 0.475, FRONT + 1.1],
        id_quat(),
    ));
    parts.extend(crop_tufts(
        [bx, FLOOR + 0.95, FRONT + 1.1],
        [1.3, 0.35],
        4,
        2,
        0.28,
        foliage(CROP_GREEN),
    ));
    parts.push(prim(
        cuboid_tapered([bw - 0.3, 0.08, 1.3], 0.0, glow([1.0, 0.96, 0.86], 1.5)),
        [bx, head - 0.22, FRONT + 1.2],
        id_quat(),
    ));
    // Canopy over the door, standing off the column line rather than flush with
    // it, so it never shares a plane with the frame.
    parts.push(prim(
        solid(cuboid_tapered(
            [bw + 0.5, 0.14, 1.0],
            0.0,
            steel(STEEL_WHITE),
        )),
        [bx, head + 0.2, FRONT - 0.42],
        id_quat(),
    ));
}

/// The produce dock: the shutter rolled **up** on a lit packing floor.
///
/// The right answer to "a card on a solid" is sometimes no glazing at all — a
/// dispatch bay is a genuine hole, the crates are the point, and the alpha-card
/// idiom never enters into it (#972, the boardwalk's lesson).
fn dock(parts: &mut Vec<Generator>) {
    let (bx, bw) = bays()[1];
    let head = FLOOR + DOCK_H;

    // Rolled drum and its jamb tracks — the shutter is up, and it is the drum
    // that says so.
    parts.push(prim(
        solid(cylinder_tapered(0.26, bw - 0.1, 12, 0.0, steel(STEEL_GREY))),
        [bx, head - 0.3, FRONT + 0.5],
        util::quat_z(FRAC_PI_2),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.1, DOCK_H - 0.6, 0.14],
                0.0,
                steel(STEEL_GREY),
            )),
            [
                bx + sx * (bw * 0.5 - 0.05),
                FLOOR + (DOCK_H - 0.6) * 0.5,
                FRONT + 0.5,
            ],
            id_quat(),
        ));
    }
    // Packing floor: a lit lining, a run of crates, a bench, and a strip light
    // *below* the drum, which crosses the sightline from the street
    // (#972 lesson 10).
    parts.push(lit(
        [bw + 0.8, DOCK_H, 0.08],
        [bx, FLOOR + DOCK_H * 0.5, HALL_Z + 0.4],
        STREET_WARM,
        0.3,
    ));
    for (i, cx) in [-0.7_f32, 0.05, 0.7].iter().enumerate() {
        let h = 0.42 + (i % 2) as f32 * 0.2;
        parts.push(prim(
            solid(cuboid_tapered([0.6, h, 0.55], 0.0, timber(CRATE_TAN))),
            [
                bx + cx,
                FLOOR + h * 0.5,
                FRONT + 1.15 + (i % 2) as f32 * 0.35,
            ],
            id_quat(),
        ));
        parts.extend(crop_tufts(
            [bx + cx, FLOOR + h, FRONT + 1.15 + (i % 2) as f32 * 0.35],
            [0.4, 0.3],
            2,
            2,
            0.22,
            foliage(CROP_GREEN),
        ));
    }
    parts.push(prim(
        cuboid_tapered([bw - 0.5, 0.1, 0.5], 0.0, glow([1.0, 0.96, 0.86], 1.6)),
        [bx, head - 0.85, FRONT + 1.5],
        id_quat(),
    ));
}

/// The green curtain: real trellis wire on real brackets with foliage threaded
/// through it, in place of the flat green plate that shipped.
///
/// Nothing here is bigger than a plant. That is the whole point — a 0.18 × 8.3
/// slab of plain green reads as painted concrete from every angle, and the
/// guard below states it as a prohibition rather than as a census.
fn green_curtain(sx: f32) -> Generator {
    let x = sx * (W * 0.5 + 0.06);
    let y0 = FLOOR + 0.6;
    let y1 = TOWER_TOP - 0.6;
    let center = [x, (y0 + y1) * 0.5, 0.0];
    let root = prim(
        solid(cuboid_tapered(
            [0.1, 0.12, D - 0.6],
            0.0,
            steel(STEEL_WHITE),
        )),
        [x, y0, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    // Standoff brackets and horizontal battens up the wall.
    for k in 1..6 {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.1, 0.12, D - 0.6],
                0.0,
                steel(STEEL_WHITE),
            )),
            [x, y0 + k as f32 * (y1 - y0) / 5.0, 0.0],
            id_quat(),
        ));
    }
    // Vertical wires between them.
    for k in 0..7 {
        let z = -(D - 0.9) * 0.5 + k as f32 * (D - 0.9) / 6.0;
        parts.push(prim(
            solid(cylinder_tapered(0.02, y1 - y0, 6, 0.0, steel(STEEL_WHITE))),
            [x, center[1], z],
            id_quat(),
        ));
    }
    // Foliage threaded through, in clumps rather than as a sheet.
    for k in 0..6 {
        parts.extend(crop_tufts(
            [
                x + sx * 0.12,
                y0 + 0.35 + k as f32 * (y1 - y0 - 0.7) / 5.0,
                0.0,
            ],
            [0.0, D - 1.1],
            1,
            4,
            0.62,
            foliage(LEAF_GREEN),
        ));
    }
    nest(root, parts)
}

// --- One grow deck. --------------------------------------------------------

/// Grow deck `k`: the slab, its floor band on the elevation, both glazed halls
/// with their racks and lit lining, the grow-lights fixed to the underside of
/// the deck above — and that deck itself, nested, so the storey carries what it
/// holds up.
fn grow_deck(k: usize, above: Generator) -> Generator {
    let y = level_y(k);
    let center = [0.0, y + DECK_T * 0.5, (FRONT + CORE_FRONT) * 0.5];
    let hall_d = CORE_FRONT - FRONT;
    let deck = prim(
        solid(cuboid_tapered(
            [W - WALL_T, DECK_T, hall_d],
            0.0,
            cast(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );

    let (y0, y1) = hall_span(k);
    let mut parts = vec![
        // The floor band the street sees, standing behind the columns.
        slab(
            [W, BAND_H, BAND_D],
            [0.0, y + BAND_H * 0.5, BAND_Z],
            FaceKey::SideNz,
        ),
    ];

    for (bx, bw) in bays() {
        let size = [bw, y1 - y0];
        parts.push(glazing(size, [bx, (y0 + y1) * 0.5, GLAZE_Z], 0.35));
        // Lit lining at the back of the hall.
        parts.push(lit(
            [bw + 0.5, y1 - y0, 0.08],
            [bx, (y0 + y1) * 0.5, HALL_Z],
            HALL_LINING,
            HALL_PALE,
        ));
        // Two racks of trays, held forward of the lining so the crop reads.
        for (r, rz) in [FRONT + 0.95_f32, FRONT + 1.35].iter().enumerate() {
            let ry = y0 + 0.22 + r as f32 * 0.62;
            parts.push(prim(
                solid(cuboid_tapered(
                    [bw - 0.2, 0.07, 0.44],
                    0.0,
                    steel(STEEL_WHITE),
                )),
                [bx, ry, *rz],
                id_quat(),
            ));
            parts.push(lit(
                [bw - 0.3, 0.08, 0.36],
                [bx, ry + 0.075, *rz],
                SOIL_DARK,
                0.08,
            ));
            parts.extend(crop_tufts(
                [bx, ry + 0.11, *rz],
                [bw - 0.55, 0.24],
                4,
                2,
                0.26,
                foliage(CROP_GREEN),
            ));
        }
        // Grow-light pendant, hung from the underside of the deck ABOVE on two
        // hangers. Hanger top = `y1`, that deck's own soffit; hanger bottom =
        // the light's top. The shipped strips sat half a metre below the shelf
        // they were described as fixed to, held up by nothing.
        let light_top = y1 - HANGER;
        let lz = FRONT + 1.05;
        for hx in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cylinder_tapered(0.02, HANGER, 6, 0.0, steel(STEEL_GREY))),
                [bx + hx * (bw * 0.5 - 0.35), light_top + HANGER * 0.5, lz],
                id_quat(),
            ));
        }
        parts.push(prim(
            cuboid_tapered([bw - 0.3, LIGHT_H, 0.6], 0.0, glow(GROW_PINK, 2.0)),
            [bx, light_top - LIGHT_H * 0.5, lz],
            id_quat(),
        ));
    }

    parts.push(above);
    nest(deck, parts)
}

// --- The roof. -------------------------------------------------------------

/// Roof slab, and on it the parapet railing, the solar array on its canted
/// frame, the water tank on its stand and the roof garden.
fn roof() -> Generator {
    let center = [0.0, TOWER_TOP + 0.16, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [W + 0.3, 0.32, D + 0.3],
            0.0,
            cast(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );
    let top = TOWER_TOP + 0.32;

    // Railing round all four sides. A railing is not a plate: what makes it
    // read as one is that you can see through it (#972 lesson 24).
    let (hx, hz) = (W * 0.5 + 0.03, D * 0.5 + 0.03);
    let mut parts = Vec::new();
    for sz in [-1.0_f32, 1.0] {
        parts.extend(util::railing(
            [-hx, top, sz * hz],
            [hx, top, sz * hz],
            0.95,
            util::BALUSTER_PITCH,
            steel(STEEL_WHITE),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.extend(util::railing(
            [sx * hx, top, -hz],
            [sx * hx, top, hz],
            0.95,
            util::BALUSTER_PITCH,
            steel(STEEL_WHITE),
        ));
    }

    // Solar array on a canted frame over the back half. The tilt carries only
    // the panel itself — a turned node with offset children spins them out of
    // the record and out of every translation-only guard at once (#972
    // lesson 22), so the legs are siblings.
    let tilt = 0.42_f32;
    for (i, x) in [-1.55_f32, 0.0, 1.55].iter().enumerate() {
        let _ = i;
        parts.push(prim(
            solid(cuboid_tapered([1.4, 0.07, 1.5], 0.0, pv(PV_BLUE))),
            [*x, top + 0.62, 1.15],
            quat_x(tilt),
        ));
        for sz in [-0.6_f32, 0.6] {
            parts.push(prim(
                solid(cylinder_tapered(
                    0.05,
                    0.62 - sz * 0.28,
                    6,
                    0.0,
                    steel(STEEL_GREY),
                )),
                [*x, top + (0.62 - sz * 0.28) * 0.5, 1.15 + sz],
                id_quat(),
            ));
        }
    }

    // Water tank on a stand, over the core.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cuboid_tapered([0.1, 0.7, 0.1], 0.0, steel(STEEL_GREY))),
            [-1.7 + sx * 0.5, top + 0.35, -1.2 + sz * 0.5],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cylinder_tapered(0.7, 1.0, 14, 0.05, steel(STEEL_WHITE))),
        [-1.7, top + 1.2, -1.2],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.9, 0.05, 0.9], 0.0, water(WATER_BLUE)),
        [-1.7, top + 1.72, -1.2],
        id_quat(),
    ));

    // Roof garden in timber planters, on the free half of the deck.
    for (i, x) in [0.6_f32, 2.0].iter().enumerate() {
        let z = -1.3 + i as f32 * 0.1;
        parts.push(prim(
            solid(cuboid_tapered([1.1, 0.4, 1.2], 0.0, timber(TIMBER_WARM))),
            [*x, top + 0.2, z],
            id_quat(),
        ));
        parts.extend(crop_tufts(
            [*x, top + 0.4, z],
            [0.8, 0.9],
            3,
            3,
            0.36,
            foliage(CROP_GREEN),
        ));
    }

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
        assert_sanitize_stable(&VerticalFarm.build(""), "vertical_farm");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&VerticalFarm.build(""), "vertical_farm");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&VerticalFarm.build(""), "vertical_farm");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&VerticalFarm.build(""), "vertical_farm");
    }

    #[test]
    fn keeps_its_grow_lights() {
        assert!(
            has_emissive(&VerticalFarm.build("")),
            "the tower lost its grow-lights"
        );
    }

    /// #972 lesson 1: every pane is a card on a flat quad at `uv_scale` 1.0 —
    /// two per grow deck, plus the entrance screen. The produce dock has no
    /// glazing at all, because a dispatch bay is a genuine hole.
    #[test]
    fn every_opening_is_a_card_on_a_quad() {
        let mut cards = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "a Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, LEVELS * 2 + 1, "eight grow halls and the entrance");
    }

    /// #972 lesson 7: every card oversails the opening its frame leaves it, so
    /// no edge lands on a reveal plane. Checked against the bays the columns
    /// make, which is where the openings come from.
    #[test]
    fn every_card_laps_its_opening() {
        let mut widths: Vec<f32> = Vec::new();
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                widths.push(size.0[0]);
            }
        });
        for (_, bw) in bays() {
            assert!(
                widths.iter().any(|w| (w - bw - GLAZE_LAP).abs() < 1e-4),
                "no card laps the {bw} m bay"
            );
            assert!(
                !widths.iter().any(|w| (w - bw).abs() < 1e-4),
                "a card sized exactly to a {bw} m bay ties with its own reveal"
            );
        }
    }

    /// #972 lesson 18: every cast slab's `uv_offset` is some face's projection
    /// of the position the **built tree** puts it at, read from the composed
    /// translation rather than from the constants the placement used (#972
    /// lesson 21).
    #[test]
    fn every_cast_surface_shares_one_world_frame() {
        use FaceKey::*;
        let mut checked = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0]
                || !matches!(material.texture, SovereignTextureConfig::Concrete(_))
            {
                return;
            }
            let mut dims = size.0;
            dims.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if dims[2] < 1.5 || dims[1] < 0.3 {
                return;
            }
            checked += 1;
            let agrees = [SideNz, SidePz, SideNx, SidePx, Top, Bottom]
                .into_iter()
                .any(|f| {
                    let o = util::face_uv_offset(f, at).0;
                    (o[0] - material.uv_offset.0[0]).abs() < 2e-3
                        && (o[1] - material.uv_offset.0[1]).abs() < 2e-3
                });
            assert!(
                agrees,
                "vertical_farm: a cast slab at {at:?} carries uv_offset {:?}, which is no \
                 face's projection of where the built tree puts it",
                material.uv_offset.0
            );
        });
        assert!(
            checked >= 10,
            "only {checked} cast surfaces found — suspect the selector before the content"
        );
    }

    /// #972 lesson 8: everything standing on the pad has its footprint inside
    /// the pad's. The shipped terrace shelves were 4.6 m on a 4.5 m base.
    #[test]
    fn everything_standing_on_the_pad_is_on_it() {
        let mut checked = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, hy, hz)) = footprint(g) else {
                return;
            };
            if (at[1] - hy - FLOOR).abs() > 0.03 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= PAD_W * 0.5 + 1e-3 && at[2].abs() + hz <= PAD_D * 0.5 + 1e-3,
                "vertical_farm: a part at {at:?} (half {hx} × {hz}) stands on the pad and \
                 hangs off it"
            );
        });
        assert!(
            checked >= 5,
            "only {checked} parts found standing on the pad"
        );
    }

    /// Nothing on the tower cantilevers into thin air: every rack, band, deck
    /// and light lies inside the plan the columns and flanks enclose. The
    /// shipped shelves overhung a 2.6 m core by a metre each side.
    #[test]
    fn nothing_hangs_outside_the_plan_the_frame_encloses() {
        let mut checked = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, _, hz)) = footprint(g) else {
                return;
            };
            // Everything above the street storey, excluding what is deliberately
            // outside the envelope: the trellis on the flanks and the roof
            // furniture above the parapet.
            if at[1] < GROUND_TOP || at[1] > TOWER_TOP || at[0].abs() > W * 0.5 + 0.02 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= W * 0.5 + 1e-3 && at[2] - hz >= FRONT - 0.5 - 1e-3,
                "vertical_farm: a part at {at:?} (half {hx} × {hz}) cantilevers out of the \
                 frame that would have to hold it up"
            );
        });
        assert!(
            checked >= 12,
            "only {checked} parts checked inside the frame"
        );
    }

    /// Each grow-light is fixed to the **underside of the deck above it**, and
    /// each hall's glazing sits between its own floor band and that same deck.
    /// Both are relationships between two derived levels, which is what the
    /// shipped build got wrong in the one way a render cannot show: the strips
    /// were half a metre below anything.
    #[test]
    fn every_grow_light_hangs_from_the_deck_above_its_hall() {
        let root = VerticalFarm.build("");
        let mut lights: Vec<([f32; 3], f32)> = Vec::new();
        let mut cards: Vec<([f32; 3], f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cuboid { size, material, .. }
                if material.emission_strength.0 > 1.5 && material.base_color.0 == GROW_PINK =>
            {
                lights.push((at, size.0[1] * 0.5));
            }
            GeneratorKind::Plane { size, material, .. }
                if matches!(material.texture, SovereignTextureConfig::Window(_)) =>
            {
                cards.push((at, size.0[1] * 0.5));
            }
            _ => {}
        });
        assert_eq!(lights.len(), LEVELS * 2, "one grow-light per hall");
        let soffits: Vec<f32> = (0..LEVELS).map(|k| hall_span(k).1).collect();
        // Every hanger's top and bottom, read out of the built tree.
        let mut hangers: Vec<([f32; 3], f32, f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder { radius, height, .. } = &g.kind
                && (height.0 - HANGER).abs() < 1e-4
                && radius.0 < 0.05
            {
                hangers.push((at, at[1] - height.0 * 0.5, at[1] + height.0 * 0.5));
            }
        });
        assert_eq!(hangers.len(), LEVELS * 4, "two hangers per grow-light");
        for (at, half) in &lights {
            let top = at[1] + half;
            // Match on X as well as height: both bays' hangers share a level,
            // so a height-only selector reports every light carried by four
            // (#972 lesson 24 — the selector is as much a source of false
            // results as the assertion).
            let carried = hangers
                .iter()
                .filter(|(h, lo, hi)| {
                    (h[0] - at[0]).abs() < 1.2
                        && (lo - top).abs() < 1e-3
                        && soffits.iter().any(|s| (s - hi).abs() < 1e-3)
                })
                .count();
            assert_eq!(
                carried, 2,
                "vertical_farm: a grow-light topping out at {top} is carried by {carried} \
                 hangers reaching a deck soffit — the soffits are at {soffits:?}"
            );
        }
        for k in 0..LEVELS {
            let (y0, y1) = hall_span(k);
            let n = cards
                .iter()
                .filter(|(at, half)| {
                    at[1] - half >= y0 - GLAZE_LAP && at[1] + half <= y1 + GLAZE_LAP
                })
                .count();
            assert_eq!(
                n, 2,
                "level {k}'s glazing does not fit between its own decks"
            );
        }
    }

    /// #972: the green curtain is made of **plants**, not of one green slab.
    /// Stated as a prohibition rather than a census — counting foliage prims
    /// passes happily on a single 8 m plate, which is exactly what shipped.
    #[test]
    fn no_foliage_surface_is_a_plate() {
        let mut clumps = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, at| {
            let green = |m: &SovereignMaterialSettings| {
                m.base_color.0 == LEAF_GREEN || m.base_color.0 == CROP_GREEN
            };
            match &g.kind {
                GeneratorKind::Cuboid { size, material, .. } if green(material) => {
                    let big = size.0.iter().cloned().fold(0.0_f32, f32::max);
                    panic!(
                        "vertical_farm: a {big} m green cuboid at {at:?} is a painted plate, \
                         not planting — build the curtain out of things the size of plants"
                    );
                }
                GeneratorKind::Sphere { material, .. } if green(material) => clumps += 1,
                _ => {}
            }
        });
        assert!(
            clumps >= 24,
            "only {clumps} foliage clumps — the trellis is bare"
        );
    }

    /// #972 lesson 11: nothing solid stands **in front of** a lit stair light.
    ///
    /// A panel mounted on a wall has two ways to fail and only one of them
    /// shows in a render: too far out and it floats, too far in and it is
    /// swallowed by whatever it is mounted on. This build hit the second twice
    /// — first a lit panel authored at `BACK − 0.04`, i.e. inside the 1.5 m
    /// core, and then, after that fix, one placed inside its own cast surround.
    /// Both looked like a blank wall, which is exactly what the shipped entry
    /// looked like anyway, so a render can never tell you which you have.
    #[test]
    fn no_stair_light_is_buried_in_what_it_is_mounted_on() {
        let root = VerticalFarm.build("");
        let mut lights: Vec<([f32; 3], f32)> = Vec::new();
        let mut boxes: Vec<([f32; 3], [f32; 3])> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return;
            }
            if material.base_color.0 == STAIR_GLASS {
                lights.push((at, size.0[2] * 0.5));
            } else {
                boxes.push((at, [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]));
            }
        });
        assert_eq!(lights.len(), 5, "five stair lights up the core");
        for (at, half) in &lights {
            let face = at[2] + half;
            assert!(
                face > BACK,
                "vertical_farm: a stair light's face is at {face}, behind the core's own \
                 back at {BACK} — it is inside the wall it is mounted on"
            );
            for (c, e) in &boxes {
                let covers = (c[0] - at[0]).abs() < e[0] && (c[1] - at[1]).abs() < e[1];
                assert!(
                    !covers || c[2] + e[2] <= face + 1e-4,
                    "vertical_farm: a solid at {c:?} presents its face at {} in front of a \
                     stair light at {face} — the light is buried in it",
                    c[2] + e[2]
                );
            }
        }
    }

    /// The roof is railed on all four sides, with balusters. Balusters are the
    /// rail height less the handrail's own stock, so a selector matching on the
    /// exact height finds only the end posts (#972 lesson 24).
    #[test]
    fn the_roof_is_railed_on_every_side() {
        let mut posts: Vec<[f32; 3]> = Vec::new();
        let mut balusters = 0;
        walk(&VerticalFarm.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if at[1] < TOWER_TOP || !(0.65..=0.96).contains(&sy) {
                return;
            }
            if (sx - 0.11).abs() < 1e-3 && (sz - 0.11).abs() < 1e-3 {
                posts.push(at);
            } else if sx < 0.09 && sz < 0.09 {
                balusters += 1;
            }
        });
        assert!(balusters >= 30, "only {balusters} balusters round the roof");
        for sx in [-1.0_f32, 1.0] {
            assert!(
                posts
                    .iter()
                    .any(|p| (p[0] - sx * (W * 0.5 + 0.03)).abs() < 0.02),
                "no railing down the {} flank",
                if sx > 0.0 { "+X" } else { "−X" }
            );
            assert!(
                posts
                    .iter()
                    .any(|p| (p[2] - sx * (D * 0.5 + 0.03)).abs() < 0.02),
                "no railing along the {} end",
                if sx > 0.0 { "+Z" } else { "−Z" }
            );
        }
    }

    /// The editability contract (#972 lesson 3): the pad carries the street
    /// storey, which carries the first grow deck, which carries the second, and
    /// so on up to the roof — so one drag moves a storey and everything in it.
    #[test]
    fn each_deck_carries_the_one_above_it() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = VerticalFarm.build("");
        assert_eq!(
            root.children.len(),
            2,
            "the pad carries the street storey and its buried footing"
        );
        // Walk the stack: from the street storey, the child that is a deck slab
        // of the next level up. Selected by DECK_T, which is what defines a
        // deck, rather than by child count.
        let mut node = &root.children[0];
        for k in 0..LEVELS {
            node = node
                .children
                .iter()
                .find(|c| match &c.kind {
                    GeneratorKind::Cuboid { size, .. } => {
                        (size.0[1] - DECK_T).abs() < 1e-4 && size.0[0] > 3.0
                    }
                    _ => false,
                })
                .unwrap_or_else(|| panic!("deck {k} does not carry deck {}", k + 1));
        }
        assert!(
            node.children.iter().any(|c| match &c.kind {
                GeneratorKind::Cuboid { size, .. } => (size.0[1] - 0.32).abs() < 1e-4,
                _ => false,
            }),
            "the top deck carries the roof"
        );
        assert!(count(&root) > 180, "the tower lost most of its parts");
    }
}
