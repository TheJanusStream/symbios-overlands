//! Barn — the Rural/Farmland landmark. The classic red barn: vertical
//! board-and-batten on a fieldstone foundation under a true gambrel roof,
//! presenting its gable end and its big sliding doors to the approach, one
//! leaf rolled open on a lit hay floor, with a hayloft door and hoist beam in
//! the gable and a louvred cupola on the ridge. Chaff drifts off the loft on
//! the golden-hour air.
//!
//! Three things about it are deliberate, and all three were wrong before
//! #972's pass:
//!
//! 1. **It is a gambrel, and it has gable ends.** The roof used to be two
//!    uniformly-tapered blocks, which is a *hip*: tapering a cuboid pinches
//!    it on both axes at once, so the barn's celebrated end profile was
//!    quietly rounded away on all four sides and the hayloft door opened onto
//!    a slope. It is now four real roof planes — steep skirt, shallow upper
//!    pitch — over trapezoidal gable panels clad in the same boarding as the
//!    walls, which is what makes the silhouette read as a barn from a
//!    distance at which nothing else about it does.
//! 2. **The boards stand up.** The `Plank` generator lays courses up V, so
//!    the kit's barn board came out as horizontal lap siding — a farmhouse
//!    material on a barn. [`util::bonded_boards`] turns it through the one
//!    quarter turn the pattern survives (see that function for why), and the
//!    barn finally wears board-and-batten.
//! 3. **The doors open on something.** A closed barn is a red box with a big
//!    flat panel on it, and the two lit windows beside it were `Window` cards
//!    stuck to a solid wall — frames with holes onto the boarding behind.
//!    Rolling one leaf back onto its pier buys four metres of real depth and
//!    makes the hay floor, the mow wall and the hanging lantern the point of
//!    the prop.
//!
//! [`util::bonded_boards`]: crate::catalogue::items::util::bonded_boards

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, attach, cone, cuboid_tapered, cuboid_tapered_xz, footing, glow, id_quat, lit_interior,
    nest, plane, prim, quat_mul, quat_x, quat_y, quat_z, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BARN_RED, HAY_GOLD, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, barn_board, enamel, fx,
    metal_roof, stone,
};

// --- The barn's dimensions. Everything below derives from these. -----------

/// Body width across the gable (X) and length along the ridge (Z).
const W: f32 = 12.5;
const D: f32 = 17.0;
/// Fieldstone foundation height, and the eaves height above it.
const FOOT_H: f32 = 0.55;
const WALL_H: f32 = 5.4;
/// Wall thickness, and so the depth of every reveal.
const WALL_T: f32 = 0.3;
/// Top of the boarded wall — the eaves line, and the datum the gambrel
/// profile below is measured from.
const WALL_TOP: f32 = FOOT_H + WALL_H;

/// Outer face of the gable the barn presents — the `-Z` hero direction the
/// render tool and the settlement placer both look down.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane, set back inside the reveal so the wall's thickness reads.
const GLAZE_Z: f32 = FRONT + 0.2;
/// Where a room panel stands behind an opening.
const ROOM_Z: f32 = FRONT + 0.62;
/// Centre plane of proud trim boards. Deep enough that their back faces end
/// up inside the wall rather than coplanar with its outer face.
const TRIM_Z: f32 = FRONT - 0.05;

// --- The gambrel profile, in (x, y-above-[`WALL_TOP`]) pairs. --------------

/// Half-width at the eaves — the wall's own corner, so the skirt lands on it.
const EAVE_X: f32 = W * 0.5;
/// Where the steep lower skirt breaks into the shallow upper pitch.
const KNUCKLE_X: f32 = 4.0;
const KNUCKLE_Y: f32 = 2.8;
/// Ridge height above the eaves line.
const RIDGE_Y: f32 = 5.2;
/// The pinch that takes the upper gable panel to a point. Not `1.0`: the
/// record sanitiser clamps `taper` at 0.99, and a value it rewrites fails the
/// entry's own round-trip guard rather than rendering differently.
const APEX_TAPER: f32 = 0.99;
/// How far the roof planes oversail the eaves and the gables.
const EAVE_OVER: f32 = 0.55;
const RAKE_OVER: f32 = 0.5;
/// Roof slab thickness — real enough to show a shadow under the eave.
const ROOF_T: f32 = 0.26;

/// Where the lower skirt's outer edge actually lands, as `(x, y above
/// [`WALL_TOP`])`.
///
/// The skirt runs on past the wall corner at its own pitch by [`EAVE_OVER`],
/// so the eave's position is a *consequence* of the gambrel profile. Every
/// part that hangs off it — the fascia, the barge board's outer end — derives
/// from here rather than from a hand-picked pair, which is the same discipline
/// #972 lesson 11 arrived at from the other direction: a mounted part's
/// standoff comes from its host, never from the eye.
fn eave_edge() -> (f32, f32) {
    let (dx, dy) = (EAVE_X - KNUCKLE_X, -KNUCKLE_Y);
    let len = dx.hypot(dy);
    (EAVE_X + dx / len * EAVE_OVER, dy / len * EAVE_OVER)
}

// --- Openings. -------------------------------------------------------------

/// The big doorway, and the two window openings that flank it.
const DOOR_W: f32 = 4.0;
const DOOR_H: f32 = 4.6;
const WIN_X: f32 = 5.05;
const WIN_W: f32 = 1.2;
const WIN_H: f32 = 1.3;
const WIN_SILL: f32 = 2.5;
/// The single tall opening in each long wall, and where along the barn it
/// sits: forward, over the lit bay, because a window into an unlit two-thirds
/// of a barn is a black rectangle (#972 lesson 6).
const SIDE_WIN_Z: f32 = FRONT + 4.5;
const SIDE_WIN_L: f32 = 1.6;
const SIDE_WIN_H: f32 = 1.5;
const SIDE_WIN_SILL: f32 = 2.6;

/// Hayloft door, in the lower gable panel, and its sill above the eaves line.
const LOFT_W: f32 = 1.9;
const LOFT_H: f32 = 2.0;
const LOFT_SILL: f32 = 0.25;

// --- Palette local to this entry. ------------------------------------------

/// The doors are painted a shade deeper than the walls, as they are on a
/// real barn — same paint, more coats, more weather.
const DOOR_RED: [f32; 3] = [0.42, 0.10, 0.08];
/// The unlit interior lining: dark, warm, and darker than the sunlit boarding
/// around the opening, or the depth the open door exists to show flattens.
const MOW_DARK: [f32; 3] = [0.20, 0.15, 0.11];

// --- Shared construction. --------------------------------------------------

/// The barn's board-and-batten, stood upright and laid in the shared world
/// frame — see [`util::bonded_boards`].
fn boards(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_boards(barn_board(color), face, center)
}

/// One boarded slab of the shell. The position drives both the placement and
/// the UV frame, so the two cannot drift apart.
fn board_wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, boards(BARN_RED, center, face))),
        center,
        id_quat(),
    )
}

/// A proud painted trim board — corner boards, casings, battens, barge
/// boards. Always oversized against what it laps and always standing off the
/// surface it laps, so it never shares a plane with its host.
fn trim(size: [f32; 3], center: [f32; 3]) -> Generator {
    trim_facing(size, center, FaceKey::SideNz, id_quat())
}

/// [`trim`], for a board on an elevation other than the hero gable or one
/// that carries a tilt. `face` is the elevation whose board frame it joins;
/// a tilted board takes the frame of what it trims rather than of its own
/// rotated centre, which is why the caller passes the centre it wants.
fn trim_facing(
    size: [f32; 3],
    center: [f32; 3],
    face: FaceKey,
    rotation: crate::pds::Fp4,
) -> Generator {
    prim(
        cuboid_tapered(
            size,
            0.0,
            util::bonded_boards(barn_board(TRIM_WHITE), face, center),
        ),
        center,
        rotation,
    )
}

/// How far a glazing card oversails its opening on every edge — the coplanar
/// rule applied to a card (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Clear glazing filling one opening on the gable, on a flat quad at
/// [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3]) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            window_card(TRIM_WHITE, 2, 2, 0.34, 0.1),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// The same card turned onto a long wall. `sx` is the side it faces.
///
/// Two rotations compose: [`quat_x`]`(-FRAC_PI_2)` stands the quad up facing
/// `-Z` (mapping its local Z extent onto world Y), then a yaw swings that
/// face onto `±X`. So `size` still reads as `[width, height]`, with the width
/// running along the barn.
fn side_glazing(size: [f32; 2], center: [f32; 3], sx: f32) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            window_card(TRIM_WHITE, 2, 2, 0.34, 0.1),
        ),
        center,
        quat_mul(quat_y(-sx * FRAC_PI_2), quat_x(-FRAC_PI_2)),
    )
}

/// A room panel behind an opening — the surface a card's masked-away panes
/// actually show. Nothing lights the inside of an enclosed prop, so these
/// carry a low self-lit term of their own.
fn room(size: [f32; 3], center: [f32; 3], lit: bool) -> Generator {
    let mat = if lit {
        lit_interior([0.70, 0.55, 0.30], 0.5)
    } else {
        lit_interior([0.26, 0.20, 0.16], 0.12)
    };
    prim(cuboid_tapered(size, 0.0, mat), center, id_quat())
}

pub struct Barn;

impl CatalogueEntry for Barn {
    fn slug(&self) -> &'static str {
        "barn"
    }
    fn name(&self) -> &'static str {
        "Barn"
    }
    fn description(&self) -> &'static str {
        "Red gambrel barn with board-and-batten walls, a rolled-back door and a cupola."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 14.0,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The barn as a tree that stands the way it does: fieldstone footing at the
/// bottom, the boarded shell on it, the roof on the shell's eaves line, and
/// the cupola on the ridge.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let base = prim(
        solid(cuboid_tapered(
            [W + 0.7, FOOT_H, D + 0.7],
            0.0,
            stone(STONE_GREY),
        )),
        [0.0, FOOT_H * 0.5, 0.0],
        id_quat(),
    );
    // Buried footing under the fieldstone base, so a barn snapped to the high
    // point of a field keeps its ground under the downhill corner.
    let mut root = nest(
        base,
        vec![shell(), footing(W + 0.7, D + 0.7, [0.0, 0.0], 14.0)],
    );
    // Signature life: chaff drifting out of the open bay.
    attach(
        &mut root,
        fx::chaff_drift([0.0, WALL_TOP - 1.4, FRONT - 1.6], 0xC4AF_DA11),
    );
    root
}

// --- The shell. ------------------------------------------------------------

/// Threshing floor, and on it everything the barn is: the walls that frame
/// the openings, the glazing, the fit-out behind it, the doors, and — on the
/// eaves line — the roof.
fn shell() -> Generator {
    let mut parts = Vec::new();
    let mid_y = FOOT_H + WALL_H * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Back gable wall — solid; only the approach face is cut.
    parts.push(board_wall(
        [W, WALL_H, WALL_T],
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    side_walls(&mut parts);
    gable_elevation(&mut parts);
    fit_out(&mut parts);
    sliding_doors(&mut parts);
    parts.push(roof());

    let floor = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.12, inner_d],
            0.0,
            lit_interior([0.34, 0.26, 0.19], 0.16),
        ),
        [0.0, FOOT_H + 0.06, 0.0],
        id_quat(),
    );
    nest(floor, parts)
}

/// The two long walls, each framing one tall opening over the lit bay.
///
/// Four slabs a side rather than one: the opening has to be a real hole for
/// the card in it to be anything but a frame over boarding, and the cheapest
/// honest framing of a single opening in a long wall is fore piece, sill
/// band, head band, aft piece.
fn side_walls(parts: &mut Vec<Generator>) {
    let z0 = FRONT + WALL_T;
    let z1 = -FRONT - WALL_T;
    let (wa, wb) = (SIDE_WIN_Z - SIDE_WIN_L * 0.5, SIDE_WIN_Z + SIDE_WIN_L * 0.5);
    let head = SIDE_WIN_SILL + SIDE_WIN_H;
    debug_assert!(z0 < wa && wb < z1, "the side opening left its own wall");

    for sx in [-1.0_f32, 1.0] {
        let cx = sx * (W * 0.5 - WALL_T * 0.5);
        let face = if sx > 0.0 {
            FaceKey::SidePx
        } else {
            FaceKey::SideNx
        };
        for (a, b) in [(z0, wa), (wb, z1)] {
            parts.push(board_wall(
                [WALL_T, WALL_H, b - a],
                [cx, FOOT_H + WALL_H * 0.5, (a + b) * 0.5],
                face,
            ));
        }
        parts.push(board_wall(
            [WALL_T, SIDE_WIN_SILL, SIDE_WIN_L],
            [cx, FOOT_H + SIDE_WIN_SILL * 0.5, SIDE_WIN_Z],
            face,
        ));
        parts.push(board_wall(
            [WALL_T, WALL_H - head, SIDE_WIN_L],
            [cx, FOOT_H + (head + WALL_H) * 0.5, SIDE_WIN_Z],
            face,
        ));
        // Glazing in the reveal, with a lit bay behind it.
        let cy = FOOT_H + SIDE_WIN_SILL + SIDE_WIN_H * 0.5;
        parts.push(side_glazing(
            [SIDE_WIN_L, SIDE_WIN_H],
            [sx * (W * 0.5 - 0.2), cy, SIDE_WIN_Z],
            sx,
        ));
        parts.push(room(
            [0.1, SIDE_WIN_H + 0.5, SIDE_WIN_L + 0.5],
            [sx * (W * 0.5 - 0.75), cy, SIDE_WIN_Z],
            true,
        ));
        // Corner boards, turning both corners of this elevation.
        for sz in [-1.0_f32, 1.0] {
            parts.push(trim(
                [0.3, WALL_H, 0.3],
                [
                    sx * (W * 0.5 - 0.06),
                    FOOT_H + WALL_H * 0.5,
                    sz * (D * 0.5 - 0.06),
                ],
            ));
        }
    }
}

/// The hero gable: the boarding that *frames* the doorway and its two
/// flanking windows.
///
/// Left to right the wall is an outer pier, a window bay (sill band under,
/// head band over), an inner pier, the doorway, and the mirror of all of it —
/// plus the head band that carries the wall over the doors.
fn gable_elevation(parts: &mut Vec<Generator>) {
    let door_x = DOOR_W * 0.5;
    let (wa, wb) = (WIN_X - WIN_W * 0.5, WIN_X + WIN_W * 0.5);
    let head = WIN_SILL + WIN_H;

    for sx in [-1.0_f32, 1.0] {
        // Outer pier and inner pier, both full height.
        for (a, b) in [(wb, W * 0.5), (door_x, wa)] {
            parts.push(board_wall(
                [b - a, WALL_H, WALL_T],
                [sx * (a + b) * 0.5, FOOT_H + WALL_H * 0.5, FRONT_MID],
                FaceKey::SideNz,
            ));
        }
        // Under and over the window.
        parts.push(board_wall(
            [WIN_W, WIN_SILL, WALL_T],
            [sx * WIN_X, FOOT_H + WIN_SILL * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
        parts.push(board_wall(
            [WIN_W, WALL_H - head, WALL_T],
            [sx * WIN_X, FOOT_H + (head + WALL_H) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
        // The glazing, its room, and the white casing round it.
        let cy = FOOT_H + WIN_SILL + WIN_H * 0.5;
        parts.push(glazing([WIN_W, WIN_H], [sx * WIN_X, cy, GLAZE_Z]));
        parts.push(room(
            [WIN_W + 0.5, WIN_H + 0.5, 0.1],
            [sx * WIN_X, cy, ROOM_Z],
            true,
        ));
        parts.push(trim(
            [WIN_W + 0.44, 0.16, 0.24],
            [sx * WIN_X, cy + WIN_H * 0.5 + 0.14, TRIM_Z],
        ));
        parts.push(trim(
            [WIN_W + 0.44, 0.14, 0.3],
            [sx * WIN_X, cy - WIN_H * 0.5 - 0.12, TRIM_Z],
        ));
    }
    // The wall over the doorway.
    parts.push(board_wall(
        [DOOR_W, WALL_H - DOOR_H, WALL_T],
        [0.0, FOOT_H + (DOOR_H + WALL_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    // Painted casing round the doorway: two jambs and a head.
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim(
            [0.3, DOOR_H + 0.3, 0.12],
            [sx * (door_x + 0.15), FOOT_H + DOOR_H * 0.5, TRIM_Z - 0.16],
        ));
    }
    parts.push(trim(
        [DOOR_W + 0.6, 0.3, 0.12],
        [0.0, FOOT_H + DOOR_H + 0.15, TRIM_Z - 0.16],
    ));
}

/// What the open door shows: the threshing floor, a mow wall of stacked hay
/// held close behind the opening, a loft deck over it, and a lantern hung
/// *below* the door head.
///
/// Depth discipline, twice over (#972 lessons 6 and 10). The mow wall is four
/// metres in, not seventeen, because goods at the back of a long shed are
/// unreadable specks; and the lantern hangs under the loft deck rather than
/// on it, because a light behind anything that spans the opening's head is a
/// light nobody sees.
fn fit_out(parts: &mut Vec<Generator>) {
    let mow_z = FRONT + 4.4;
    // Mow wall — the surface the doorway actually frames.
    parts.push(prim(
        cuboid_tapered(
            [W - 1.4, WALL_H - 0.4, 0.2],
            0.0,
            lit_interior(MOW_DARK, 0.14),
        ),
        [0.0, FOOT_H + (WALL_H - 0.4) * 0.5, mow_z],
        id_quat(),
    ));
    // Stacked bales against it, in two courses, so the mow reads as stored
    // crop rather than as a painted wall.
    for (i, (x, y, w)) in [
        (-2.6_f32, 0.55_f32, 2.0_f32),
        (0.1, 0.55, 2.2),
        (2.6, 0.55, 1.9),
        (-1.5, 1.62, 1.8),
        (1.4, 1.62, 2.0),
    ]
    .into_iter()
    .enumerate()
    {
        let tone = 0.94 + (i % 3) as f32 * 0.05;
        parts.push(prim(
            cuboid_tapered(
                [w, 1.0, 1.1],
                0.0,
                lit_interior(
                    [HAY_GOLD[0] * tone, HAY_GOLD[1] * tone, HAY_GOLD[2] * tone],
                    0.22,
                ),
            ),
            [x, FOOT_H + y, mow_z - 0.7],
            id_quat(),
        ));
    }
    // Loft deck over the bay, just clear of the door head.
    parts.push(prim(
        cuboid_tapered(
            [W - 1.4, 0.18, 5.2],
            0.0,
            lit_interior([0.30, 0.23, 0.17], 0.14),
        ),
        [0.0, FOOT_H + DOOR_H + 0.22, FRONT + 2.9],
        id_quat(),
    ));
    // The lantern: a small housing with a smaller lens, hung under the deck.
    // A broad panel at strength blooms white; a small one reads as a colour.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.24, 0.3, 0.24],
            0.0,
            enamel([0.2, 0.19, 0.18]),
        )),
        [-0.9, FOOT_H + 3.3, FRONT + 2.2],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.16, 0.18, 0.16], 0.0, glow(LAMP_WARM, 2.6)),
        [-0.9, FOOT_H + 3.24, FRONT + 2.2],
        id_quat(),
    ));
}

/// The two sliding leaves and the track they hang from: one rolled back onto
/// its pier, one still closed over half the doorway.
///
/// The open leaf is what turns the prop from a box with a panel on it into a
/// building with an inside, and its travel is derived from the doorway's own
/// half-width so it can never be parked over the window beside it.
fn sliding_doors(parts: &mut Vec<Generator>) {
    let leaf_w = DOOR_W * 0.5 + 0.06;
    let leaf_h = DOOR_H - 0.06;
    let leaf_z = TRIM_Z - 0.24;
    let open_cx = -(DOOR_W * 0.5 + leaf_w * 0.5 + 0.03);
    // Track rail, spanning exactly the doorway plus the run the open leaf
    // parks on — derived from both, so it can neither fall short of the
    // parked leaf nor run on across the window beside it.
    let (ra, rb) = (open_cx - leaf_w * 0.5 - 0.25, DOOR_W * 0.5 + 0.25);
    parts.push(trim(
        [rb - ra, 0.2, 0.14],
        [(ra + rb) * 0.5, FOOT_H + DOOR_H + 0.18, leaf_z + 0.12],
    ));
    // Closed leaf on the right half; open leaf rolled clear of the doorway
    // to the left. Its park position is derived from the doorway's own half
    // width plus the leaf's, so it can neither creep back over the opening
    // nor drift onto the window beside it.
    for (side, cx) in [(1.0_f32, DOOR_W * 0.25), (-1.0, open_cx)] {
        let hangers = [-0.6_f32, 0.6_f32];
        parts.push(prim(
            solid(cuboid_tapered(
                [leaf_w, leaf_h, 0.14],
                0.0,
                util::bonded_boards(
                    barn_board(DOOR_RED),
                    FaceKey::SideNz,
                    [cx, FOOT_H + leaf_h * 0.5, leaf_z],
                ),
            )),
            [cx, FOOT_H + leaf_h * 0.5, leaf_z],
            id_quat(),
        ));
        // Top / mid / bottom battens and the mirrored diagonal that makes the
        // classic Z-brace.
        for ty in [0.35_f32, leaf_h * 0.5, leaf_h - 0.35] {
            parts.push(trim([leaf_w, 0.15, 0.06], [cx, FOOT_H + ty, leaf_z - 0.1]));
        }
        // The diagonal that makes the classic barn-door Z-brace. Its tilt is
        // derived from the leaf's own proportions, so it stays a diagonal of
        // the leaf however the doorway is resized.
        let rise = leaf_h - 0.9;
        parts.push(trim_facing(
            [leaf_w.hypot(rise), 0.18, 0.05],
            [cx, FOOT_H + leaf_h * 0.5, leaf_z - 0.13],
            FaceKey::SideNz,
            quat_z(side * rise.atan2(leaf_w)),
        ));
        for hx in hangers {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.1, 0.26, 0.1],
                    0.0,
                    enamel([0.28, 0.26, 0.24]),
                )),
                [cx + hx, FOOT_H + leaf_h + 0.1, leaf_z - 0.02],
                id_quat(),
            ));
        }
    }
}

// --- The roof. -------------------------------------------------------------

/// The gambrel: four roof planes (a steep skirt and a shallow upper pitch
/// each side), the gable panels that close the ends, barge boards on the
/// hero gable, the eaves fascias, and the cupola on the ridge.
///
/// The sub-root is the ridge cap, so dragging the ridge takes the whole roof
/// and the cupola with it.
fn roof() -> Generator {
    // The ridge cap is the sub-root, built first so everything else can be
    // nested into its frame.
    let ridge = prim(
        solid(cuboid_tapered(
            [0.62, 0.22, D + RAKE_OVER * 2.0 + 0.1],
            0.0,
            metal_roof([ROOF_GREY[0] * 0.9, ROOF_GREY[1] * 0.9, ROOF_GREY[2] * 0.92]),
        )),
        [0.0, WALL_TOP + RIDGE_Y + 0.08, 0.0],
        id_quat(),
    );
    let mut parts = Vec::new();

    // --- The four planes. `seg` is (inner, outer) profile points in
    // (x, y-above-WALL_TOP), plus how far to run past each end.
    let planes: [([f32; 2], [f32; 2], f32, f32); 2] = [
        // Upper pitch: ridge → knuckle, oversailing the ridge a little so the
        // cap has something to sit on.
        ([0.0, RIDGE_Y], [KNUCKLE_X, KNUCKLE_Y], 0.14, 0.0),
        // Lower skirt: knuckle → eave, running on past the wall as the eave.
        ([KNUCKLE_X, KNUCKLE_Y], [EAVE_X, 0.0], 0.0, EAVE_OVER),
    ];
    for (inner, outer, over_in, over_out) in planes {
        let (dx, dy) = (outer[0] - inner[0], outer[1] - inner[1]);
        let len = dx.hypot(dy);
        let (ux, uy) = (dx / len, dy / len);
        let total = len + over_in + over_out;
        let mid = [
            inner[0] + ux * (len * 0.5 + (over_out - over_in) * 0.5),
            inner[1] + uy * (len * 0.5 + (over_out - over_in) * 0.5),
        ];
        let angle = dy.atan2(dx);
        for sx in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [total, ROOF_T, D + RAKE_OVER * 2.0],
                    0.0,
                    // Turned a quarter so the ribs and the rust streaks both
                    // run down the pitch, not across it.
                    util::quarter_turn(metal_roof(ROOF_GREY)),
                )),
                [sx * mid[0], WALL_TOP + mid[1], 0.0],
                quat_z(sx * angle),
            ));
        }
    }

    // --- Gable panels, both ends, clad in the same boarding as the walls.
    // A trapezoid and a triangle, which is what a gambrel end *is* and what
    // a uniformly-tapered block could never be.
    for sz in [-1.0_f32, 1.0] {
        let cz = sz * (D * 0.5 - WALL_T * 0.5);
        let face = if sz > 0.0 {
            FaceKey::SidePz
        } else {
            FaceKey::SideNz
        };
        let lower_c = [0.0, WALL_TOP + KNUCKLE_Y * 0.5, cz];
        parts.push(prim(
            solid(cuboid_tapered_xz(
                [W, KNUCKLE_Y, WALL_T],
                [1.0 - KNUCKLE_X * 2.0 / W, 0.0],
                boards(BARN_RED, lower_c, face),
            )),
            lower_c,
            id_quat(),
        ));
        let upper_c = [0.0, WALL_TOP + (KNUCKLE_Y + RIDGE_Y) * 0.5, cz];
        parts.push(prim(
            solid(cuboid_tapered_xz(
                [KNUCKLE_X * 2.0, RIDGE_Y - KNUCKLE_Y, WALL_T],
                [APEX_TAPER, 0.0],
                boards(BARN_RED, upper_c, face),
            )),
            upper_c,
            id_quat(),
        ));
    }

    hayloft(&mut parts);

    // --- Barge boards on the hero gable, tilted by the roof's *own* pitch
    // rather than by a hand-picked angle, so they cannot stop matching it.
    let barge_z = FRONT - RAKE_OVER + 0.09;
    let (ex, ey) = eave_edge();
    for (inner, outer) in [
        ([0.0_f32, RIDGE_Y], [KNUCKLE_X, KNUCKLE_Y]),
        ([KNUCKLE_X, KNUCKLE_Y], [ex, ey]),
    ] {
        let (dx, dy) = (outer[0] - inner[0], outer[1] - inner[1]);
        let len = dx.hypot(dy);
        let angle = dy.atan2(dx);
        for sx in [-1.0_f32, 1.0] {
            parts.push(trim_facing(
                [len, 0.26, 0.1],
                [
                    sx * (inner[0] + dx * 0.5),
                    WALL_TOP + inner[1] + dy * 0.5 - 0.16,
                    barge_z,
                ],
                FaceKey::SideNz,
                quat_z(sx * angle),
            ));
        }
    }
    // Eaves fascia along both long sides, hung off the roof's *own* edge:
    // the skirt runs on past the wall corner at its own pitch, so where that
    // edge lands is a function of the profile, not a number to guess at.
    let (ex, ey) = eave_edge();
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim_facing(
            [0.12, 0.32, D + RAKE_OVER * 2.0],
            [sx * (ex - 0.06), WALL_TOP + ey - 0.18, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
            id_quat(),
        ));
    }

    parts.push(cupola());
    nest(ridge, parts)
}

/// Hayloft door and hoist beam, in the upper gable over the main doorway.
fn hayloft(parts: &mut Vec<Generator>) {
    // In the *lower* gable, not the upper one. The upper panel pinches to a
    // point, so a 1.9 m door placed there is wider than the wall it hangs on
    // by the time it reaches its own head — a fault the four-angle sheet
    // shows only as a stray white edge against the sky, and the guard below
    // now states as an invariant.
    let cy = WALL_TOP + LOFT_SILL + LOFT_H * 0.5;
    let z = FRONT + WALL_T * 0.5;
    // A dark opening behind the leaf, so the loft is a hole rather than a
    // painted rectangle when the leaf is later looked at from an angle.
    parts.push(room(
        [LOFT_W + 0.3, LOFT_H + 0.3, 0.1],
        [0.0, cy, z + 0.3],
        false,
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [LOFT_W, LOFT_H, 0.14],
            0.0,
            util::bonded_boards(
                barn_board(DOOR_RED),
                FaceKey::SideNz,
                [0.0, cy, z - WALL_T * 0.5 - 0.07],
            ),
        )),
        [0.0, cy, z - WALL_T * 0.5 - 0.07],
        id_quat(),
    ));
    for ty in [-LOFT_H * 0.34, LOFT_H * 0.34] {
        parts.push(trim(
            [LOFT_W, 0.16, 0.06],
            [0.0, cy + ty, z - WALL_T * 0.5 - 0.17],
        ));
    }
    // Hoist beam projecting from the gable over the loft door's head, with a
    // pulley block on its nose.
    let beam_z = FRONT - 1.5;
    parts.push(trim(
        [0.26, 0.26, 2.1],
        [0.0, cy + LOFT_H * 0.5 + 0.55, beam_z + 0.55],
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.1, 0.34, 0.22],
            0.0,
            enamel([0.24, 0.23, 0.22]),
        )),
        [0.0, cy + LOFT_H * 0.5 + 0.27, beam_z],
        id_quat(),
    ));
}

/// The louvred cupola and its weathervane, standing on the ridge.
fn cupola() -> Generator {
    let y = WALL_TOP + RIDGE_Y + 0.19;
    let mut parts = Vec::new();
    // Louvres on the two faces the approach sees, as slats rather than as a
    // painted stripe — a cupola is a vent, and it should read as one.
    for sz in [-1.0_f32, 1.0] {
        for i in 0..4 {
            parts.push(prim(
                cuboid_tapered([1.16, 0.13, 0.07], 0.0, enamel([0.26, 0.25, 0.24])),
                [0.0, y + 0.24 + i as f32 * 0.22, sz * 0.68],
                quat_x(sz * 0.42),
            ));
        }
    }
    parts.push(prim(
        solid(cone(1.05, 0.95, 4, metal_roof(ROOF_GREY))),
        [0.0, y + 1.72, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.06, 1.2, 0.06],
            0.0,
            enamel([0.2, 0.2, 0.22]),
        )),
        [0.0, y + 2.7, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([1.05, 0.18, 0.05], 0.3, enamel([0.2, 0.2, 0.22])),
        [0.16, y + 3.2, 0.0],
        id_quat(),
    ));
    let box_c = [0.0, y + 0.65, 0.0];
    let drum = prim(
        solid(cuboid_tapered(
            [1.3, 1.3, 1.3],
            0.0,
            util::bonded_boards(barn_board(TRIM_WHITE), FaceKey::SideNz, box_c),
        )),
        box_c,
        id_quat(),
    );
    nest(drum, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::PrimCommon;
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
        assert_sanitize_stable(&Barn.build(""), "barn");
    }

    #[test]
    fn has_lamp() {
        assert!(crate::catalogue::items::util::has_emissive(&Barn.build("")));
    }

    /// #972 lesson 1: every `Window` card sits on a `Plane` at `uv_scale`
    /// 1.0, one per opening and no more. The barn used to carry four of them
    /// on solid slabs, where the generator's masked-away panes cut holes onto
    /// the boarding they were stuck to.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&Barn.build(""), [0.0; 3], &mut |g, _| {
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
        assert_eq!(cards, 4, "two gable windows and one in each long wall");
    }

    /// #972 lesson 4 and its sequel: the barn's boarding is `stagger`-free
    /// *and* stood upright, and the offset is turned with it. Miss the
    /// rotation on the offset and every slab still gets vertical boards, but
    /// each one starts them at its own centre and the joints step at every
    /// wall break — which a contact sheet will not show.
    #[test]
    fn boarding_is_upright_and_shares_one_frame() {
        use crate::pds::generator::FaceKey;
        let mut checked = 0;
        walk(&Barn.build(""), [0.0; 3], &mut |g, at| {
            let m = match &g.kind {
                GeneratorKind::Cuboid {
                    common: PrimCommon { material, .. },
                    ..
                } => material,
                _ => return,
            };
            let SovereignTextureConfig::Plank(cfg) = &m.texture else {
                return;
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "a staggered plank at {at:?} brings back the butt-joint grid"
            );
            assert_eq!(
                m.uv_rotation.0, 90.0,
                "the boarding at {at:?} is lying down"
            );
            // Only the axis-aligned slabs are in the world frame; the tilted
            // barge boards deliberately carry the frame of the gable they
            // trim, not of their own rotated centre.
            if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0] {
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
                let got = m.uv_offset.0;
                assert!(
                    want.iter()
                        .any(|w| (w[0] - got[0]).abs() < 1e-3 && (w[1] - got[1]).abs() < 1e-3),
                    "boarding at {at:?} carries uv_offset {got:?}, which is no \
                     face's turned projection of its own position"
                );
            }
            checked += 1;
        });
        assert!(checked > 20, "only {checked} boarded slabs found");
    }

    /// The gambrel is a gambrel: two distinct pitches, the upper shallower
    /// than the lower, both steeper than a shed and neither of them a hip.
    ///
    /// A uniformly-tapered block has no pitch to measure at all — which is
    /// exactly why the old roof passed for a barn in a thumbnail and read as
    /// a hipped shed the moment anything looked at its end.
    #[test]
    fn the_roof_is_a_two_pitch_gambrel() {
        let upper = (RIDGE_Y - KNUCKLE_Y).atan2(KNUCKLE_X).to_degrees();
        let lower = KNUCKLE_Y.atan2(EAVE_X - KNUCKLE_X).to_degrees();
        assert!(
            (25.0..40.0).contains(&upper),
            "upper pitch {upper}° is not a gambrel's shallow deck"
        );
        assert!(
            (45.0..65.0).contains(&lower),
            "lower pitch {lower}° is not a gambrel's steep skirt"
        );
        assert!(
            lower > upper + 12.0,
            "the two pitches ({lower}° / {upper}°) are too close to read as a break"
        );
    }

    /// The open leaf parks clear of the window beside it. Its travel is
    /// derived from the doorway's own half-width, and the window from the
    /// wall's; a hand-picked pair drifts into each other the moment either
    /// moves, and the overlap is invisible head-on because the leaf is in
    /// front of the glass.
    #[test]
    fn the_open_leaf_parks_clear_of_the_window() {
        let leaf_w = DOOR_W * 0.5 + 0.06;
        let open_cx = -(DOOR_W * 0.5 + leaf_w * 0.5 + 0.03);
        let leaf_outer = open_cx - leaf_w * 0.5;
        let win_inner = -(WIN_X - WIN_W * 0.5);
        assert!(
            leaf_outer > win_inner + 0.1,
            "the open leaf reaches {leaf_outer}, past the window edge at {win_inner}"
        );
        assert!(
            open_cx + leaf_w * 0.5 <= -DOOR_W * 0.5 + 1e-4,
            "the open leaf still covers part of the doorway"
        );
    }

    /// #972 lesson 6: what the doorway frames is held close. A mow wall at
    /// the far end of a seventeen-metre barn is an unreadable speck, and the
    /// lantern that says "this is lit" has to hang *below* the loft deck that
    /// spans the opening's head (lesson 10).
    #[test]
    fn the_lit_bay_is_held_close_and_the_lamp_below_the_head() {
        let root = Barn.build("");
        let mut mow_z = f32::MAX;
        let mut lamp: Option<[f32; 3]> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid {
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
            else {
                return;
            };
            if material.emission_strength.0 > 2.0 {
                lamp = Some(at);
            }
            if material.emission_strength.0 > 0.1
                && material.emission_strength.0 < 0.2
                && at[2] > FRONT
                && at[1] > 2.0
            {
                mow_z = mow_z.min(at[2]);
            }
        });
        assert!(
            mow_z - FRONT < 6.0,
            "the mow wall sits {} m back from the doorway",
            mow_z - FRONT
        );
        let lamp = lamp.expect("the bay carries a lantern");
        assert!(
            lamp[1] < FOOT_H + DOOR_H,
            "the lantern at {} hangs above the door head at {}",
            lamp[1],
            FOOT_H + DOOR_H
        );
    }

    /// #972 lesson 11, upward: the hayloft door hangs on a panel wide enough
    /// to hold it at *every* height it reaches. A gable pinches, so a door
    /// sized against the panel's base sails straight out through its rake —
    /// which the original placement did, in the upper panel, where the wall
    /// is 0.42 m wide at the door's own head.
    #[test]
    fn the_hayloft_door_stays_inside_its_gable() {
        let head = LOFT_SILL + LOFT_H;
        assert!(
            head <= KNUCKLE_Y,
            "the loft door reaches {head} above the eaves, past the lower \
             gable panel's own top at {KNUCKLE_Y}"
        );
        // Half-width of the lower trapezoid at the door's head.
        let half = EAVE_X - (EAVE_X - KNUCKLE_X) * head / KNUCKLE_Y;
        assert!(
            LOFT_W * 0.5 + 0.3 < half,
            "a {LOFT_W} m door needs {} m of panel at its head, and the gable \
             offers {half}",
            LOFT_W * 0.5 + 0.3
        );
    }

    /// The editability contract: the barn is a tree that stands the way it
    /// does, so dragging the ridge takes the roof and the cupola with it.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Barn.build("");
        let shell = &root.children[0];
        let ridge = shell
            .children
            .iter()
            .find(|c| c.children.len() > 8)
            .expect("the ridge carries the roof");
        assert!(
            ridge.children.iter().any(|c| c.children.len() >= 8),
            "the cupola carries its louvres and vane"
        );
        assert!(count(&root) > 70, "the barn lost most of its parts");
    }
}
