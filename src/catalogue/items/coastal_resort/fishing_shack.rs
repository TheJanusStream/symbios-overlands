//! Fishing shack — the Coastal-Resort *poor* landmark. A weathered
//! driftwood hut on short stilts at the tide line, its plank door standing
//! open on a dim lamplit room, a patched gable roof, a drying net slung on
//! one wall and a salt barrel by the steps. The hardscrabble counterpart to
//! the [`grand_hotel`](super::grand_hotel): same coast, opposite end of the
//! prosperity axis (`Poor`), so a destitute coastal room grows the fishing
//! hamlet instead of the resort strip.
//!
//! Rebuilt under #972. Three faults, all of them ordinary:
//!
//! 1. **It had no way in.** The deck stood 1.15 m off the sand on stilts and
//!    the door opened onto air. It now has steps derived from the deck's own
//!    height, and a rail to hold on the way up.
//! 2. **The roof had a plateau.** `taper` was 0.7, which pinches a cuboid to
//!    a 30 % flat top — a truncated wedge, not a ridge. At 0.99 it comes to a
//!    line, and the two gables that leaves are boarded like the walls.
//! 3. **The boards were masonry.** The kit's [`plank`] carried the
//!    generator's hard-coded end-joint grid, so every driftwood surface —
//!    walls, deck, roof, barrel — rendered as a coarse blocky lattice. Fixed
//!    kit-wide; see [`plank`]'s own note.
//!
//! And one thing added rather than fixed: the door stands **open**. A shack
//! with a closed plank door is a box with a darker rectangle on it; open, the
//! lantern inside becomes the point, and a hut that small has nothing else to
//! be about.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, lit_interior, nest,
    plane, prim, quat_x, quat_y, solid, sphere, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{BUOY_RED, DECK_WOOD, DRIFT_GREY, LAMP_WARM, STEEL_GREY, canvas, enamel, plank, steel};

// --- Dimensions. Everything below derives from these. ----------------------

/// Deck plan (X × Z) and how far it stands off the sand.
const DECK_W: f32 = 5.2;
const DECK_D: f32 = 4.2;
const DECK_Y: f32 = 1.05;
const DECK_T: f32 = 0.28;
/// Top of the deck boards — the floor level, and the datum for everything
/// above.
const FLOOR: f32 = DECK_Y + DECK_T * 0.5;

/// Hut plan and wall height above the floor.
const HUT_W: f32 = 4.2;
const HUT_D: f32 = 3.2;
const WALL_H: f32 = 2.3;
/// Wall thickness, and so the depth of every reveal.
const WALL_T: f32 = 0.16;
/// Top of the walls — where the roof lands.
const WALL_TOP: f32 = FLOOR + WALL_H;

/// Outer face of the shore-facing wall — the `-Z` hero direction the render
/// tool and the settlement placer both look down.
const FRONT: f32 = -HUT_D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane and the surface behind it, inside the reveal.
const GLAZE_Z: f32 = FRONT + 0.1;
const ROOM_Z: f32 = FRONT + 0.5;
/// Centre plane of proud trim — battens, casings, corner boards.
const TRIM_Z: f32 = FRONT - 0.04;

/// Ridge rise above the wall top, and how far the roof oversails at the
/// eaves. There is deliberately no rake overhang: the gable triangle *is* the
/// wall carried up, so it has to land in the wall's own plane, and the barge
/// boards supply the overhang read.
const RIDGE_RISE: f32 = 1.15;
const EAVE_OVER: f32 = 0.42;
/// The Z pinch that takes the roof to a ridge line. Not `1.0`: the record
/// sanitiser clamps `taper` at 0.99, and a value it rewrites fails the
/// entry's own round-trip guard rather than rendering differently.
const RIDGE_TAPER: f32 = 0.99;

/// The doorway, and the small window beside it.
const DOOR_W: f32 = 0.95;
const DOOR_H: f32 = 1.9;
const DOOR_X: f32 = -0.85;
/// How far the leaf swings out of the opening. Enough that the doorway is a
/// hole rather than a leaf-shaped rectangle, not so far that the leaf sails
/// past the wall it hangs on.
const DOOR_SWING: f32 = 0.85;
const WIN_X: f32 = 1.05;
const WIN_W: f32 = 0.8;
const WIN_H: f32 = 0.7;
const WIN_SILL: f32 = 1.15;

// --- Palette local to this entry. ------------------------------------------

/// Tarred boards of the door leaf and the patch — a different weathering from
/// the walls, which is what makes them read as separate timber.
const TAR_BROWN: [f32; 3] = [0.34, 0.28, 0.22];

// --- Shared construction. --------------------------------------------------

/// The hut's boarding: driftwood plank stood **upright**, laid in the shared
/// world frame.
///
/// A shack is built from whatever washed up, nailed on end — and the
/// generator only lays courses up V, so vertical boarding needs the quarter
/// turn [`util::bonded_boards`] applies (safe here precisely because the
/// stagger is off; see that function).
///
/// [`util::bonded_boards`]: crate::catalogue::items::util::bonded_boards
fn boards(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_boards(plank(color), face, center)
}

/// One boarded slab of the hut. The position drives both the placement and
/// the UV frame, so the two cannot drift apart.
fn board_wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, boards(DRIFT_GREY, center, face))),
        center,
        id_quat(),
    )
}

/// A proud batten or casing — always oversized against what it laps and
/// always standing off the surface it laps, so it never shares a plane with
/// its host.
fn batten(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        cuboid_tapered(
            size,
            0.0,
            util::bonded_boards(plank(DECK_WOOD), FaceKey::SideNz, center),
        ),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.05;

pub struct FishingShack;

impl CatalogueEntry for FishingShack {
    fn slug(&self) -> &'static str {
        "fishing_shack"
    }
    fn name(&self) -> &'static str {
        "Fishing Shack"
    }
    fn description(&self) -> &'static str {
        "Weathered driftwood hut on stilts, its door open on a lamplit room."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The shack as a tree that stands the way it does: stilts at the bottom, the
/// deck on them, the hut on the deck, the roof on the hut — with the steps
/// their own sub-assembly off the deck.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    // The stilts are the root: they are the lowest thing, and the deck they
    // carry has to move with them.
    let mut legs = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let at = [
                sx * (DECK_W * 0.5 - 0.5),
                DECK_Y * 0.5,
                sz * (DECK_D * 0.5 - 0.5),
            ];
            if sx > 0.0 || sz > 0.0 {
                legs.push(prim(
                    solid(cylinder_tapered(0.17, DECK_Y, 8, 0.06, plank(DECK_WOOD))),
                    at,
                    id_quat(),
                ));
            }
        }
    }
    // The shoreward-left stilt is the root; the rest hang off it, along with
    // the deck and everything the deck carries.
    let root = prim(
        solid(cylinder_tapered(0.17, DECK_Y, 8, 0.06, plank(DECK_WOOD))),
        [-(DECK_W * 0.5 - 0.5), DECK_Y * 0.5, -(DECK_D * 0.5 - 0.5)],
        id_quat(),
    );
    legs.push(deck());
    nest(root, legs)
}

/// The plank deck, and on it the hut, the net, the barrel and the steps.
fn deck() -> Generator {
    let center = [0.0, DECK_Y, 0.0];
    let boards_mat = util::bonded_siding(plank(DRIFT_GREY), FaceKey::Top, center);
    let deck = prim(
        solid(cuboid_tapered([DECK_W, DECK_T, DECK_D], 0.0, boards_mat)),
        center,
        id_quat(),
    );

    let mut parts = vec![hut(), steps()];

    // Drying net, hung from a pole on two pegs off the +X wall rather than
    // pinned flat to it: a net stuck to a wall is a green rectangle, and the
    // pole plus the sag is the whole difference between the two reads.
    let net = canvas([0.46, 0.5, 0.44], [0.36, 0.4, 0.34]);
    let nx = HUT_W * 0.5 + 0.24;
    parts.push(prim(
        solid(cuboid_tapered([0.07, 0.07, 1.9], 0.0, plank(DECK_WOOD))),
        [nx, FLOOR + 1.75, 0.1],
        id_quat(),
    ));
    for sz in [-0.75_f32, 0.75] {
        parts.push(prim(
            solid(cuboid_tapered([0.36, 0.06, 0.06], 0.0, plank(DECK_WOOD))),
            [HUT_W * 0.5 + 0.09, FLOOR + 1.75, 0.1 + sz],
            id_quat(),
        ));
    }
    parts.push(prim(
        cuboid_tapered([0.04, 1.1, 1.75], 0.06, net),
        [nx, FLOOR + 1.18, 0.1],
        id_quat(),
    ));
    for (sz, col) in [(-0.5_f32, BUOY_RED), (0.6, DECK_WOOD)] {
        parts.push(prim(
            solid(sphere(0.14, 3, enamel(col))),
            [nx + 0.06, FLOOR + 0.72, sz],
            id_quat(),
        ));
    }
    // Salt barrel on the deck by the steps, and a lobster pot beside it.
    parts.push(prim(
        solid(cylinder_tapered(0.36, 0.82, 10, 0.09, plank(DECK_WOOD))),
        [1.75, FLOOR + 0.41, -1.5],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered([0.62, 0.34, 0.5], 0.0, plank(DRIFT_GREY))),
        [1.75, FLOOR + 0.17, -0.75],
        id_quat(),
    ));

    nest(deck, parts)
}

/// The hut: the boarding that *frames* the doorway and the window, the
/// glazing, the room behind them, the open leaf, and the roof.
fn hut() -> Generator {
    let mut parts = Vec::new();
    let mid_y = FLOOR + WALL_H * 0.5;
    let inner_d = HUT_D - WALL_T * 2.0;

    // Back and side walls — solid; only the shore face is cut.
    parts.push(board_wall(
        [HUT_W, WALL_H, WALL_T],
        [0.0, mid_y, HUT_D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(board_wall(
            [WALL_T, WALL_H, inner_d],
            [sx * (HUT_W * 0.5 - WALL_T * 0.5), mid_y, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    shore_elevation(&mut parts);
    fit_out(&mut parts);
    parts.push(roof());

    // Corner boards, turning all four corners.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(batten(
                [0.14, WALL_H, 0.14],
                [sx * (HUT_W * 0.5 - 0.03), mid_y, sz * (HUT_D * 0.5 - 0.03)],
            ));
        }
    }

    let floor = prim(
        cuboid_tapered(
            [HUT_W - WALL_T * 2.0, 0.06, inner_d],
            0.0,
            lit_interior([0.30, 0.24, 0.18], 0.16),
        ),
        [0.0, FLOOR + 0.03, 0.0],
        id_quat(),
    );
    nest(floor, parts)
}

/// The shore face: four boarded pieces framing a doorway and a window.
fn shore_elevation(parts: &mut Vec<Generator>) {
    let (da, db) = (DOOR_X - DOOR_W * 0.5, DOOR_X + DOOR_W * 0.5);
    let (wa, wb) = (WIN_X - WIN_W * 0.5, WIN_X + WIN_W * 0.5);
    let win_head = WIN_SILL + WIN_H;

    // Piers: wall end → door, door → window, window → wall end.
    for (a, b) in [(-HUT_W * 0.5, da), (db, wa), (wb, HUT_W * 0.5)] {
        parts.push(board_wall(
            [b - a, WALL_H, WALL_T],
            [(a + b) * 0.5, FLOOR + WALL_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    // Over the doorway, under and over the window.
    parts.push(board_wall(
        [DOOR_W, WALL_H - DOOR_H, WALL_T],
        [DOOR_X, FLOOR + (DOOR_H + WALL_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    parts.push(board_wall(
        [WIN_W, WIN_SILL, WALL_T],
        [WIN_X, FLOOR + WIN_SILL * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    parts.push(board_wall(
        [WIN_W, WALL_H - win_head, WALL_T],
        [WIN_X, FLOOR + (win_head + WALL_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));

    // The window: a grimy card in the reveal, over a lamplit room.
    parts.push(prim(
        plane(
            [WIN_W + GLAZE_LAP, WIN_H + GLAZE_LAP],
            window_card(TAR_BROWN, 2, 2, 0.3, 0.12),
        ),
        [WIN_X, FLOOR + WIN_SILL + WIN_H * 0.5, GLAZE_Z],
        quat_x(-FRAC_PI_2),
    ));
    // A board nailed across one pane — the shack's one repair.
    parts.push(batten(
        [WIN_W + 0.26, 0.11, 0.05],
        [WIN_X, FLOOR + WIN_SILL + WIN_H * 0.62, TRIM_Z],
    ));
    // Casing round both openings.
    parts.push(batten(
        [DOOR_W + 0.24, 0.12, 0.06],
        [DOOR_X, FLOOR + DOOR_H + 0.06, TRIM_Z],
    ));
    parts.push(batten(
        [WIN_W + 0.24, 0.1, 0.06],
        [WIN_X, FLOOR + win_head + 0.05, TRIM_Z],
    ));

    // The leaf, hung on the left jamb and swung out over the pier beside it,
    // so the doorway itself is left clear.
    //
    // A leaf pivots about an *edge*, which is two things to get right at once:
    // where its centre goes, and which way its rotation turns. They are easy to
    // disagree — the centre below is correct for a leaf swinging out and to the
    // left, and pairing it with `quat_y(swing)` turns the leaf the other way
    // and leaves neither of its edges anywhere near the hinge. It hangs in mid
    // air beside its own doorway, which is precisely as odd as it sounds and
    // exactly what shipped.
    //
    // `quat_y` sends the leaf's local `+X` to `(cos φ, 0, −sin φ)`, and the
    // direction wanted here — hinge to free edge — is `(−cos θ, 0, −sin θ)`.
    // That is `φ = π − θ`, not `θ`. Both are derived below rather than written
    // as numbers, and [`the_open_leaf_hangs_on_its_hinge`] checks the built
    // node rather than re-deriving them.
    let hinge = [da, FRONT];
    let swing = DOOR_SWING;
    let arm = [-swing.cos(), -swing.sin()];
    let leaf_c = [
        hinge[0] + arm[0] * DOOR_W * 0.5,
        FLOOR + DOOR_H * 0.5,
        hinge[1] + arm[1] * DOOR_W * 0.5,
    ];
    parts.push(prim(
        solid(cuboid_tapered(
            [DOOR_W, DOOR_H - 0.06, 0.07],
            0.0,
            util::bonded_boards(plank(TAR_BROWN), FaceKey::SideNz, leaf_c),
        )),
        leaf_c,
        quat_y(std::f32::consts::PI - swing),
    ));
}

/// What the open door shows: a lamplit room lining held close behind the
/// opening, a bunk, and the lantern that justifies both.
///
/// Depth discipline (#972 lesson 6): the lining is 0.5 m in, not against the
/// back wall, because a hut this small has no depth to spare and the doorway
/// has to frame something at the distance a person actually stands.
fn fit_out(parts: &mut Vec<Generator>) {
    parts.push(prim(
        cuboid_tapered(
            [HUT_W - 0.5, WALL_H - 0.2, 0.08],
            0.0,
            lit_interior([0.42, 0.32, 0.22], 0.3),
        ),
        [0.0, FLOOR + WALL_H * 0.5, ROOM_Z + 0.5],
        id_quat(),
    ));
    // A bunk against the lining, low enough to read under the door head.
    parts.push(prim(
        cuboid_tapered([1.7, 0.4, 0.7], 0.0, lit_interior([0.36, 0.30, 0.24], 0.22)),
        [-0.9, FLOOR + 0.32, ROOM_Z + 0.25],
        id_quat(),
    ));
    // The lantern: a small housing with a smaller lens, hung inside the
    // doorway rather than outside it, so the light and the room it lights are
    // the same thing.
    parts.push(prim(
        solid(cuboid_tapered([0.16, 0.2, 0.14], 0.0, steel(STEEL_GREY))),
        [DOOR_X + 0.05, FLOOR + 1.55, ROOM_Z - 0.06],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.1, 0.12, 0.05], 0.0, glow(LAMP_WARM, 2.2)),
        [DOOR_X + 0.05, FLOOR + 1.53, ROOM_Z - 0.14],
        id_quat(),
    ));
}

/// The gable roof: a ridged plank shell, its two gable ends boarded like the
/// walls, plus barge boards, an eaves fascia, the mismatched patch board and
/// the crooked stovepipe.
///
/// Pinching **Z alone** is what makes this a ridge rather than a truncated
/// wedge. The `±X` faces the pinch leaves are the gables, and they take a
/// per-face override carrying the wall's own upright boarding at its own
/// offset, so the planking runs from the sill straight into the apex.
fn roof() -> Generator {
    let center = [0.0, WALL_TOP + RIDGE_RISE * 0.5, 0.0];
    let mut kind = solid(cuboid_tapered_xz(
        [HUT_W, RIDGE_RISE, HUT_D + EAVE_OVER * 2.0],
        [0.0, RIDGE_TAPER],
        util::bonded_siding(plank(DRIFT_GREY), FaceKey::Top, center),
    ));
    for face in [FaceKey::SidePx, FaceKey::SideNx] {
        kind = util::with_face(kind, face, boards(DRIFT_GREY, center, face));
    }

    let mut parts = Vec::new();
    // Eaves fascia along both long sides, and barge boards up both gables at
    // the roof's *own* pitch rather than a hand-picked tilt.
    for sz in [-1.0_f32, 1.0] {
        parts.push(batten(
            [HUT_W + 0.08, 0.12, 0.09],
            [0.0, WALL_TOP + 0.02, sz * (HUT_D * 0.5 + EAVE_OVER - 0.045)],
        ));
    }
    let half = HUT_D * 0.5 + EAVE_OVER;
    let slope = RIDGE_RISE.hypot(half);
    let pitch = RIDGE_RISE.atan2(half);
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let at = [
                sx * (HUT_W * 0.5 + 0.05),
                WALL_TOP + RIDGE_RISE * 0.5,
                sz * half * 0.5,
            ];
            parts.push(prim(
                cuboid_tapered(
                    [0.09, 0.16, slope],
                    0.0,
                    util::bonded_boards(
                        plank(DECK_WOOD),
                        if sx > 0.0 {
                            FaceKey::SidePx
                        } else {
                            FaceKey::SideNx
                        },
                        [sx * (HUT_W * 0.5 + 0.05), WALL_TOP, 0.0],
                    ),
                ),
                at,
                quat_x(sz * pitch),
            ));
        }
    }
    // The mismatched patch board, lying on the shore pitch where it shows.
    parts.push(prim(
        cuboid_tapered(
            [1.25, 0.08, 0.85],
            0.0,
            util::bonded_siding(plank(TAR_BROWN), FaceKey::Top, [-0.85, 0.0, -0.75]),
        ),
        [-0.85, WALL_TOP + RIDGE_RISE * 0.52, -0.75],
        quat_x(-pitch),
    ));
    // Crooked stovepipe through the back pitch, with a rain cap.
    parts.push(prim(
        solid(cylinder_tapered(0.11, 1.15, 8, 0.0, steel(STEEL_GREY))),
        [1.15, WALL_TOP + RIDGE_RISE + 0.3, 0.55],
        quat_x(0.12),
    ));
    parts.push(prim(
        solid(cylinder_tapered(0.17, 0.08, 8, 0.0, steel(STEEL_GREY))),
        [1.13, WALL_TOP + RIDGE_RISE + 0.9, 0.48],
        id_quat(),
    ));

    nest(prim(kind, center, id_quat()), parts)
}

/// Steps up to the deck, with a hand rail.
///
/// The shack had none: the deck stood 1.2 m off the sand and the door opened
/// onto air. The flight is derived from the deck's own height and lands on
/// its own front edge (#972 lesson 8), so it can neither float nor hang past
/// what it comes off.
fn steps() -> Generator {
    let risers = 4;
    let deck_top = FLOOR;
    let rise = deck_top / risers as f32;
    let going = 0.3;
    let front_edge = -DECK_D * 0.5;
    let cx = DOOR_X;

    let mut parts = Vec::new();
    for i in 0..risers - 1 {
        let top = (i + 1) as f32 * rise;
        parts.push(prim(
            solid(cuboid_tapered([1.15, top, going], 0.0, plank(DECK_WOOD))),
            [
                cx,
                top * 0.5,
                front_edge - going * (risers - 1 - i) as f32 - going * 0.5,
            ],
            id_quat(),
        ));
    }
    // A single rail on the seaward side, on two posts.
    let run = going * risers as f32;
    for pz in [front_edge - 0.2, front_edge - run + 0.2] {
        parts.push(prim(
            solid(cuboid_tapered([0.09, 1.0, 0.09], 0.0, plank(DECK_WOOD))),
            [cx + 0.66, 0.5, pz],
            id_quat(),
        ));
    }
    parts.push(prim(
        cuboid_tapered([0.08, 0.08, run], 0.0, plank(DECK_WOOD)),
        [cx + 0.66, 0.98, front_edge - run * 0.5 + 0.1],
        id_quat(),
    ));

    // The top step is the sub-root: it is what the flight hangs off, and it
    // laps under the deck's own edge so no tread floats.
    let top_step = prim(
        solid(cuboid_tapered(
            [1.15, deck_top, going + 0.1],
            0.0,
            plank(DECK_WOOD),
        )),
        [cx, deck_top * 0.5, front_edge - going * 0.5 + 0.05],
        id_quat(),
    );
    nest(top_step, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_sanitize_stable,
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

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FishingShack.build(""), "fishing_shack");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&FishingShack.build(""), "fishing_shack");
    }

    /// #972 lesson 1, as a prohibition: no *solid* wears a `Window` texture.
    /// The card counts above check what they find; this checks that nothing
    /// was found in the wrong place at all.
    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&FishingShack.build(""), "fishing_shack");
    }

    /// #972 lesson 1: the one window is a card on a `Plane` at `uv_scale` 1.0,
    /// over a real opening. A shack has exactly one, and a second would mean
    /// somebody had stuck one on a solid wall.
    #[test]
    fn the_window_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&FishingShack.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(
                        is_plane,
                        "Window card must sit on a Plane, found {}",
                        g.kind.kind_tag()
                    );
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 1, "the shack has one window");
    }

    /// #972 lesson 4 and 15: driftwood boarding is `stagger`-free and stood
    /// upright, with the world offset turned to match. Miss the turn on the
    /// offset and every slab still gets vertical boards, but each starts them
    /// at its own centre and the joints step at every wall break.
    #[test]
    fn boarding_is_upright_and_shares_one_frame() {
        let mut checked = 0;
        walk(&FishingShack.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { material, .. } = &g.kind else {
                return;
            };
            let SovereignTextureConfig::Plank(cfg) = &material.texture else {
                return;
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "a staggered plank at {at:?} brings back the butt-joint grid"
            );
            if material.uv_rotation.0 == 0.0 {
                return; // the deck and the roof deliberately lie flat
            }
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return; // tilted boards carry the frame of what they trim
            }
            let want: Vec<_> = [
                FaceKey::SideNz,
                FaceKey::SidePz,
                FaceKey::SideNx,
                FaceKey::SidePx,
            ]
            .into_iter()
            .map(|f| {
                let [u, v] = util::face_uv_offset(f, at).0;
                [-v, u]
            })
            .collect();
            let got = material.uv_offset.0;
            assert!(
                want.iter()
                    .any(|w| (w[0] - got[0]).abs() < 1e-3 && (w[1] - got[1]).abs() < 1e-3),
                "boarding at {at:?} carries uv_offset {got:?}, which is no face's \
                 turned projection of its own position"
            );
            checked += 1;
        });
        assert!(checked > 5, "only {checked} upright boarded slabs found");
    }

    /// The roof comes to a ridge rather than a plateau. `taper` 0.7 — what
    /// this carried — leaves a flat top 30 % of the hut's depth wide, which is
    /// a truncated wedge and reads as a botched gable from any angle above
    /// eye level.
    #[test]
    fn the_roof_comes_to_a_ridge() {
        let mut found = false;
        walk(&FishingShack.build(""), [0.0; 3], &mut |g, _| {
            let GeneratorKind::Cuboid { torture, faces, .. } = &g.kind else {
                return;
            };
            let [tx, tz] = torture.taper.0;
            if tz < 0.5 {
                return;
            }
            found = true;
            assert_eq!(tx, 0.0, "the roof is pinched in X too — that is a hip");
            assert!(tz > 0.9, "the ridge taper {tz} leaves a plateau on top");
            let clad: Vec<_> = faces.iter().map(|o| o.face).collect();
            assert!(
                clad.contains(&FaceKey::SidePx) && clad.contains(&FaceKey::SideNx),
                "the gables are not boarded like the walls: {clad:?}"
            );
        });
        assert!(found, "no ridged roof in the tree");
    }

    /// The open leaf hangs on its hinge, swings clear of its own opening, and
    /// does not sail past the wall corner.
    ///
    /// **Read out of the built tree, not recomputed from the constants.** A
    /// leaf that pivots about an *edge* has two independent things to get
    /// right — where its centre goes and which way its rotation turns — and
    /// getting one right makes the other's error look plausible. The first
    /// version of this guard recomputed the free edge from `DOOR_SWING` with
    /// the same formula the placement used, so it agreed with a leaf whose
    /// rotation turned the wrong way and passed while the door hung in mid-air
    /// beside its own doorway. Taking the node's actual quaternion and its
    /// actual half-extent is what makes the check independent of the
    /// authoring, and it is the only reason it can catch this class at all.
    #[test]
    fn the_open_leaf_hangs_on_its_hinge() {
        /// Rotate `v` by the quaternion `q` (`[x, y, z, w]`).
        fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
            let (qx, qy, qz, qw) = (q[0], q[1], q[2], q[3]);
            let cross = |a: [f32; 3], b: [f32; 3]| {
                [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ]
            };
            let u = [qx, qy, qz];
            let uv = cross(u, v);
            let uuv = cross(u, uv);
            [
                v[0] + 2.0 * (qw * uv[0] + uuv[0]),
                v[1] + 2.0 * (qw * uv[1] + uuv[1]),
                v[2] + 2.0 * (qw * uv[2] + uuv[2]),
            ]
        }

        let root = FishingShack.build("");
        let mut leaf: Option<([f32; 3], [f32; 4], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            // The one part of the hut that carries a yaw.
            if g.transform.rotation.0[1].abs() > 1e-4 {
                leaf = Some((at, g.transform.rotation.0, size.0));
            }
        });
        let (at, q, size) = leaf.expect("the doorway carries a swung leaf");

        // The leaf's two ends, in the world frame the tree actually builds.
        let arm = rotate(q, [size[0] * 0.5, 0.0, 0.0]);
        let ends = [
            [at[0] - arm[0], at[1], at[2] - arm[2]],
            [at[0] + arm[0], at[1], at[2] + arm[2]],
        ];
        // One of them must land on a jamb, at the wall plane.
        let jambs = [DOOR_X - DOOR_W * 0.5, DOOR_X + DOOR_W * 0.5];
        let hung = ends.iter().position(|e| {
            jambs.iter().any(|j| (e[0] - j).abs() < 0.03) && (e[2] - FRONT).abs() < 0.05
        });
        let hung = hung.unwrap_or_else(|| {
            panic!(
                "neither end of the leaf sits on a jamb: ends {ends:?}, jambs \
                 {jambs:?} at z {FRONT} — the door is hung on nothing"
            )
        });

        // ...and the other must be genuinely swung out, clear of the opening
        // and inside the wall it hangs on.
        let free = ends[1 - hung];
        assert!(
            free[2] < FRONT - 0.3,
            "the leaf's free edge at z {} is barely off the wall — the doorway \
             still reads as a closed panel",
            free[2]
        );
        assert!(
            free[0] < DOOR_X - DOOR_W * 0.5 + 0.05,
            "the leaf's free edge at x {} swings back across its own opening",
            free[0]
        );
        assert!(
            free[0] > -HUT_W * 0.5 - 0.05,
            "the leaf's free edge at x {} reaches past the wall corner at {}",
            free[0],
            -HUT_W * 0.5
        );
    }

    /// #972 lesson 8: the steps land on the deck they come off, and climb it
    /// in even risers. The shack shipped with none at all — a 1.2 m deck and a
    /// door opening onto air.
    #[test]
    fn the_steps_reach_the_deck_in_even_risers() {
        let root = FishingShack.build("");
        let mut treads: Vec<[f32; 3]> = Vec::new();
        let mut deck_top = f32::MIN;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if sx > 5.0 {
                deck_top = at[1] + sy * 0.5;
            } else if (sx - 1.15).abs() < 1e-3 && sz < 0.45 {
                treads.push([at[0], at[1] + sy * 0.5, at[2]]);
            }
        });
        assert_eq!(treads.len(), 4, "the flight has four treads");
        treads.sort_by(|a, b| a[1].partial_cmp(&b[1]).unwrap());
        assert!(
            (treads.last().unwrap()[1] - deck_top).abs() < 1e-3,
            "the top tread at {} does not meet the deck at {deck_top}",
            treads.last().unwrap()[1]
        );
        let rise = treads[0][1];
        for pair in treads.windows(2) {
            assert!(
                (pair[1][1] - pair[0][1] - rise).abs() < 1e-3,
                "uneven riser between {} and {}",
                pair[0][1],
                pair[1][1]
            );
        }
        // ...and the flight runs down from the deck's front edge, not into it.
        for t in &treads {
            assert!(
                t[2] < -DECK_D * 0.5 + 0.06,
                "a tread at z {} is under the deck rather than off it",
                t[2]
            );
        }
    }

    /// The shack keeps its lantern — escalation's broken-emissive ruin pass
    /// needs something to snuff, and it is the hamlet's only light.
    #[test]
    fn has_a_lantern() {
        assert!(crate::catalogue::items::util::has_emissive(
            &FishingShack.build("")
        ));
    }

    /// The editability contract: the stilts carry the deck, the deck carries
    /// the hut and the steps, the hut carries the roof.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = FishingShack.build("");
        let deck = root
            .children
            .iter()
            .find(|c| c.children.len() > 4)
            .expect("the stilts carry the deck");
        let hut = deck
            .children
            .iter()
            .find(|c| c.children.len() > 8)
            .expect("the deck carries the hut");
        assert!(
            hut.children.iter().any(|c| c.children.len() >= 8),
            "the hut carries a roof that carries its own trim"
        );
        assert!(count(&root) > 40, "the shack lost most of its parts");
    }
}
