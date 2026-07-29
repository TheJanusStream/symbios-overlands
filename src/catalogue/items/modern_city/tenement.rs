//! Tenement — the Modern-City *poor* landmark. A weathered brick walk-up
//! with grimy windows, a steel fire escape zig-zagging up the street face, a
//! raised stoop and a rooftop water tank. The inner-city counterpart to the
//! [`glass_skyscraper`](super::glass_skyscraper): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows this instead of
//! the corporate tower.
//!
//! Built as a **shell** under the standing lessons of #972, the way
//! [`corner_store`](super::corner_store) and the suburban house are:
//!
//! 1. **The glazing fills real holes.** The elevation is the brickwork that
//!    *frames* twenty-four openings — six full-height piers standing 50 mm
//!    proud of seven recessed spandrel bands — and each opening is filled by
//!    a [`window_card`] on a flat quad set back in its reveal, with a room
//!    panel behind it. It used to be a solid block with `Window`-textured
//!    slabs pinned to the outside, and the generator masks its panes *away*,
//!    so every one of them was a frame with holes onto the brick it was stuck
//!    to.
//! 2. **The brick lies flat.** The kit's [`brick`] lays ten columns to five
//!    rows, which under the metre-UV convention stands every brick on end;
//!    [`util::bonded_brick`] re-lays it at a real 215 mm in one shared course
//!    frame, so the bond runs through pier, band and cornice as if the
//!    building had been laid in one pass.
//! 3. **It stands the way a walk-up stands.** Pavement → shell → cornice →
//!    parapet → roof deck → tank frame → tank, with the stoop and the fire
//!    escape as their own subtrees, so one gizmo drag moves a whole
//!    sub-assembly.
//!
//! The piers-proud-of-bands elevation is also what keeps the prim count sane:
//! a five-bay, five-storey grid framed the suburban house's way — a slab per
//! sill and per spandrel — is thirty-five wall slabs, where recessing the
//! bands behind continuous piers is thirteen and gives the façade a shadow
//! line it did not have.
//!
//! [`util::bonded_brick`]: crate::catalogue::items::util::bonded_brick

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest, plane,
    prim, quat_z, solid, torus, window_card, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, LAMP_WARM, brick, concrete, steel, timber};

// --- Shell dimensions. Everything below is derived from these. -------------

/// Body width (X) and depth (Z).
const W: f32 = 12.0;
const D: f32 = 9.0;
/// Pavement plinth. Its top is the datum every height below is measured
/// from, and it is the lobby floor.
const BASE_H: f32 = 0.45;
/// Wall thickness, and so the depth of every window reveal.
const WALL_T: f32 = 0.35;

/// Outer face of the street wall — the `-Z` hero direction the render tool
/// and the settlement placer both look down.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// How far the spandrel bands sit back from the piers' plane. Small enough
/// to read as a shadow line rather than as a second building, big enough
/// that no two brick faces are ever coplanar across the elevation.
const RECESS: f32 = 0.05;
/// Glazing plane: set back inside the reveal so the wall's thickness reads as
/// thickness rather than as a sticker.
const GLAZE_Z: f32 = FRONT + 0.26;
/// Where the room panels stand — far enough behind the glass that the reveal
/// has depth, near enough that a card always frames *something*.
const ROOM_Z: f32 = FRONT + 0.66;
/// Centre plane of the proud trim (sills, string courses). Deep enough that
/// their back faces end up inside the wall rather than coplanar with it.
const TRIM_Z: f32 = FRONT - 0.06;

/// Clear height of the ground storey above the plinth.
const GROUND_H: f32 = 3.3;
/// Upper-storey height.
const STOREY: f32 = 2.85;
/// Upper floors above the ground storey.
const FLOORS: usize = 4;
/// Top of the brickwork above the plinth — four storeys, plus a frieze the
/// cornice can land on.
const PLATE: f32 = GROUND_H + STOREY * FLOORS as f32 + 0.45;

/// Bay centres in X. Five bays at a 2.2 m pitch leave every pier exactly one
/// metre wide, including the two that turn the corner.
const BAY_X: [f32; 5] = [-4.4, -2.2, 0.0, 2.2, 4.4];
/// The centre bay is the entrance on the ground storey.
const DOOR_BAY: usize = 2;
/// Every opening on the hero face is this wide.
const OPEN_W: f32 = 1.2;
/// Upper-storey opening height, and its sill above its own floor line.
const OPEN_H: f32 = 1.7;
const U_SILL_OFF: f32 = 0.85;
/// Ground-storey window sill and opening height above the plinth.
const G_SILL: f32 = 0.9;
const G_OPEN_H: f32 = 1.7;
/// Head of the entrance opening — taller than the windows beside it, as a
/// doorway is, and the level the head band starts from.
const DOOR_HEAD: f32 = 3.3;

/// Brick length in metres — a real 215 mm brick. The kit's shared sizing
/// lays a 172 mm one, small enough at street distance to mip toward flat.
const BRICK_LEN: f32 = 0.215;

// --- Palette local to this entry. ------------------------------------------

/// Sooty brick for the recessed bands — a tenement's spandrels never
/// weather the way its piers do.
const BRICK_SOOT: [f32; 3] = [0.36, 0.20, 0.16];
/// Cast-stone sills, cornice corbels and stoop copings.
const STONE_PALE: [f32; 3] = [0.58, 0.56, 0.52];
/// Window joinery — the tired painted frames the cards carry.
const JOINERY: [f32; 3] = [0.42, 0.40, 0.36];
/// Fire-escape steel: dark, and rustier than the kit's structural grey.
const FE_STEEL: [f32; 3] = [0.30, 0.25, 0.22];
/// Front-door paint — the one saturated note on the elevation.
const DOOR_PAINT: [f32; 3] = [0.22, 0.26, 0.30];
/// Weathered cedar of the rooftop tank.
const TANK_WOOD: [f32; 3] = [0.44, 0.34, 0.24];

// --- Shared construction. --------------------------------------------------

/// This building's brickwork, bonded into the shared world course frame.
fn bonded(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(color), BRICK_LEN, face, center)
}

/// One brick slab of the shell. The position drives both the placement and
/// the UV frame, so the two cannot drift apart.
///
/// `wraps` names the *other* faces of this slab that meet brick at a corner
/// someone can see. That list is short by construction: the four side faces
/// all read `V = -y`, so courses already turn a vertical corner on the base
/// offset alone and only the column phase differs — which matters solely
/// where two slabs are **coplanar**. On this elevation that is exactly the
/// two outer piers, whose outward returns share a plane with the flank walls
/// behind them.
fn brick_slab(
    size: [f32; 3],
    color: [f32; 3],
    center: [f32; 3],
    facing: FaceKey,
    wraps: &[FaceKey],
) -> Generator {
    let mut kind = solid(cuboid_tapered(size, 0.0, bonded(color, center, facing)));
    for &face in wraps {
        kind = with_face(kind, face, bonded(color, center, face));
    }
    prim(kind, center, id_quat())
}

/// A proud cast-stone band — sill, string course, coping. Trim is always
/// oversized against what it laps and always stands off the surface it laps,
/// so it never shares a plane with its host.
fn stone(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, concrete(STONE_PALE))),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge — the coplanar
/// rule applied to a card. Sized to the opening exactly, each edge lands on
/// the reveal's own plane, and a flush edge is a tie the rasteriser has to
/// break. The overhang is never seen, because the frame is opaque and the
/// pier's outer face is nearer the camera than the recessed card.
const GLAZE_LAP: f32 = 0.06;

/// Clear glazing filling one opening, on a flat quad at [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3]) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            window_card(JOINERY, 2, 3, 0.32, 0.09),
        ),
        center,
        util::quat_x(-FRAC_PI_2),
    )
}

/// Every glazed opening on the hero face: bay centre, sill and head above the
/// plinth, and whether a light is on behind it.
///
/// One list rather than three loops, because the elevation, the glazing, the
/// room panels and the guards all have to agree about where the holes are —
/// and the way that agreement breaks is one of them being edited and the
/// others not.
fn openings() -> Vec<(f32, f32, f32, bool)> {
    let mut out = Vec::new();
    for (b, &x) in BAY_X.iter().enumerate() {
        if b != DOOR_BAY {
            out.push((x, G_SILL, G_SILL + G_OPEN_H, b == 3));
        }
        for f in 0..FLOORS {
            let sill = GROUND_H + f as f32 * STOREY + U_SILL_OFF;
            out.push((x, sill, sill + OPEN_H, (f + b) % 4 == 1));
        }
    }
    out
}

pub struct Tenement;

impl CatalogueEntry for Tenement {
    fn slug(&self) -> &'static str {
        "tenement"
    }
    fn name(&self) -> &'static str {
        "Tenement"
    }
    fn description(&self) -> &'static str {
        "Weathered brick walk-up with a raised stoop, fire escape and rooftop water tank."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The walk-up as a tree that stands the way it does: the pavement plinth at
/// the bottom, and on it the shell (carrying the cornice, the parapet, the
/// roof deck and the tank), the stoop and the fire escape.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let plinth = prim(
        solid(cuboid_tapered(
            [W + 0.6, BASE_H, D + 0.6],
            0.0,
            concrete([0.42, 0.42, 0.43]),
        )),
        [0.0, BASE_H * 0.5, 0.0],
        id_quat(),
    );
    nest(
        plinth,
        vec![
            shell(),
            stoop(),
            fire_escape(),
            // Buried footing under the pavement plinth. `nest` rebases it
            // out of the ground frame into the plinth's local one.
            footing(W + 0.6, D + 0.6, [0.0, 0.0], 9.0),
        ],
    )
}

// --- The shell. ------------------------------------------------------------

/// Lobby deck, and above it everything the building is: the brickwork that
/// frames the openings, the glazing, the rooms behind it, and — on the
/// frieze — the cornice and everything the roof carries.
///
/// The deck is the sub-root because it is the lowest piece of the shell and
/// every course above stands on it.
fn shell() -> Generator {
    let mut parts = Vec::new();
    let mid_y = BASE_H + PLATE * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Flanks and back: a solid box open only where the hero face is cut, so
    // the glazing has an inside to look into. The flanks are shortened in Z
    // so their ends never share a plane with the front and back slabs.
    for sx in [-1.0_f32, 1.0] {
        parts.push(brick_slab(
            [WALL_T, PLATE, inner_d],
            BRICK_RED,
            [sx * (W * 0.5 - WALL_T * 0.5), mid_y, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
            &[],
        ));
    }
    parts.push(brick_slab(
        [W, PLATE, WALL_T],
        BRICK_RED,
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
        &[],
    ));

    front_elevation(&mut parts);
    for (x, sill, head, lit) in openings() {
        parts.push(glazing(
            [OPEN_W, head - sill],
            [x, BASE_H + (sill + head) * 0.5, GLAZE_Z],
        ));
        parts.push(room_panel(x, sill, head, lit));
    }
    entrance(&mut parts);

    parts.push(cornice());

    let deck = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.12, inner_d],
            0.0,
            lit_interior([0.20, 0.18, 0.17], 0.08),
        ),
        [0.0, BASE_H + 0.06, 0.0],
        id_quat(),
    );
    nest(deck, parts)
}

/// The hero face: six full-height piers standing [`RECESS`] proud of the
/// spandrel bands behind them.
///
/// Every piece shares one course frame, so the bond runs through pier and
/// band as if the elevation had been laid in one pass. The bands stop 100 mm
/// inside the flank walls' outer faces, so their end grain is buried in the
/// masonry instead of tying for depth with the flank it meets.
fn front_elevation(parts: &mut Vec<Generator>) {
    // Piers fill the gaps between openings, plus the two that turn the
    // corner. Their outward returns are the only coplanar brick on the
    // building, so they are the only slabs that need a per-face wrap.
    let mut edges = vec![-W * 0.5];
    for &x in BAY_X.iter() {
        edges.push(x - OPEN_W * 0.5);
        edges.push(x + OPEN_W * 0.5);
    }
    edges.push(W * 0.5);
    let last = edges.len() - 2;
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        let wraps: &[FaceKey] = match i {
            0 => &[FaceKey::SideNx],
            _ if i == last => &[FaceKey::SidePx],
            _ => &[],
        };
        parts.push(brick_slab(
            [b - a, PLATE, WALL_T],
            BRICK_RED,
            [(a + b) * 0.5, BASE_H + PLATE * 0.5, FRONT_MID],
            FaceKey::SideNz,
            wraps,
        ));
    }

    // Recessed spandrel bands, full width behind the piers. `band` is
    // (low, high) above the plinth.
    let band = |lo: f32, hi: f32| {
        brick_slab(
            [W - 0.2, hi - lo, WALL_T],
            BRICK_SOOT,
            [0.0, BASE_H + (lo + hi) * 0.5, FRONT_MID + RECESS],
            FaceKey::SideNz,
            &[],
        )
    };
    // Base course under the whole ground storey.
    parts.push(band(0.0, G_SILL));
    // Above the ground windows, but not across the taller entrance.
    for sx in [-1.0_f32, 1.0] {
        let inner = OPEN_W * 0.5;
        let outer = W * 0.5 - 0.1;
        parts.push(brick_slab(
            [outer - inner, DOOR_HEAD - (G_SILL + G_OPEN_H), WALL_T],
            BRICK_SOOT,
            [
                sx * (inner + outer) * 0.5,
                BASE_H + (G_SILL + G_OPEN_H + DOOR_HEAD) * 0.5,
                FRONT_MID + RECESS,
            ],
            FaceKey::SideNz,
            &[],
        ));
    }
    // Between the storeys, and the frieze above the top row.
    for f in 0..FLOORS {
        let sill = GROUND_H + f as f32 * STOREY + U_SILL_OFF;
        let below = if f == 0 {
            DOOR_HEAD
        } else {
            GROUND_H + (f - 1) as f32 * STOREY + U_SILL_OFF + OPEN_H
        };
        parts.push(band(below, sill));
    }
    parts.push(band(
        GROUND_H + (FLOORS - 1) as f32 * STOREY + U_SILL_OFF + OPEN_H,
        PLATE,
    ));

    // Cast-stone sills. The upper storeys get a string course running the
    // full elevation — cheaper than a sill per opening, and what a walk-up
    // of this period actually has; the ground floor gets individual sills,
    // because a course there would run straight through the entrance.
    for f in 0..FLOORS {
        let sill = GROUND_H + f as f32 * STOREY + U_SILL_OFF;
        parts.push(stone(
            [W + 0.24, 0.16, WALL_T * 0.9],
            [0.0, BASE_H + sill - 0.08, TRIM_Z + 0.04],
        ));
    }
    for (b, &x) in BAY_X.iter().enumerate() {
        if b == DOOR_BAY {
            continue;
        }
        parts.push(stone(
            [OPEN_W + 0.3, 0.14, 0.34],
            [x, BASE_H + G_SILL - 0.07, TRIM_Z],
        ));
    }
}

/// A room behind one opening — the surface a card's masked-away panes
/// actually show.
///
/// Nothing lights the inside of an enclosed prop, so these carry a low
/// self-lit term of their own ([`lit_interior`]); without it every opening
/// reads as a black rectangle and the whole shell is wasted. A panel per
/// opening rather than one lining per bay is what gives the elevation its
/// life: some rooms are lit, most are not, and the pattern reads from the
/// street as a building somebody lives in.
fn room_panel(x: f32, sill: f32, head: f32, lit: bool) -> Generator {
    let mat = if lit {
        lit_interior([0.72, 0.58, 0.34], 0.55)
    } else {
        lit_interior([0.20, 0.19, 0.20], 0.10)
    };
    prim(
        cuboid_tapered([OPEN_W + 0.4, head - sill + 0.4, 0.1], 0.0, mat),
        [x, BASE_H + (sill + head) * 0.5, ROOM_Z],
        id_quat(),
    )
}

/// The entrance: a painted door under a lit transom, in a reveal, with a
/// vestibule behind it and a tired lamp over the head.
///
/// Depth discipline (#972 lesson 6) applies to a doorway too — the transom
/// exists so the camera looking up through the head frames something warm
/// instead of the underside of the floor above.
fn entrance(parts: &mut Vec<Generator>) {
    let x = BAY_X[DOOR_BAY];
    let head = BASE_H + DOOR_HEAD;
    let sill = BASE_H + G_SILL;

    // Lit vestibule behind the door — the depth the transom looks into.
    parts.push(prim(
        cuboid_tapered(
            [OPEN_W + 0.4, DOOR_HEAD - G_SILL + 0.4, 0.1],
            0.0,
            lit_interior([0.55, 0.44, 0.30], 0.35),
        ),
        [x, (sill + head) * 0.5, ROOM_Z],
        id_quat(),
    ));
    // Transom light over the door, glazed like the windows.
    parts.push(glazing([OPEN_W, 0.42], [x, head - 0.27, GLAZE_Z]));
    // The leaf itself: solid, and lapped over the reveal so no edge of it is
    // coplanar with the brick it hangs in.
    parts.push(prim(
        solid(cuboid_tapered(
            [OPEN_W + 0.06, DOOR_HEAD - G_SILL - 0.54, 0.09],
            0.0,
            glow(DOOR_PAINT, 0.0),
        )),
        [x, sill + (DOOR_HEAD - G_SILL - 0.54) * 0.5, GLAZE_Z - 0.06],
        id_quat(),
    ));
    // Stone door head, and a housing with a smaller lit lens under it — a
    // broad panel at strength blooms to white, a small one reads as a
    // colour.
    parts.push(stone([OPEN_W + 0.5, 0.2, 0.36], [x, head + 0.1, TRIM_Z]));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.22, 0.26, 0.14],
            0.0,
            steel([0.2, 0.19, 0.18]),
        )),
        [x, head + 0.36, FRONT - 0.09],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.14, 0.15, 0.06], 0.0, glow(LAMP_WARM, 2.2)),
        [x, head + 0.34, FRONT - 0.17],
        id_quat(),
    ));
}

/// The corbelled brick cornice, and everything the roof carries above it.
///
/// It is a sub-root rather than a sibling because the parapet stands on it
/// and the tank stands on the deck inside it: dragging the cornice takes the
/// whole roofscape with it, which is what a top course is.
fn cornice() -> Generator {
    let y = BASE_H + PLATE;
    let corbel = brick_slab(
        [W + 0.5, 0.4, D + 0.5],
        BRICK_RED,
        [0.0, y + 0.2, 0.0],
        FaceKey::SideNz,
        &[],
    );
    nest(corbel, roofscape(y + 0.4))
}

/// Roof deck, parapet ring, bulkhead and water tank, standing on the
/// cornice's top at `y`.
fn roofscape(y: f32) -> Vec<Generator> {
    let mut out = vec![
        // Deck. Held just below the parapet's foot so the two never share a
        // horizontal plane.
        prim(
            solid(cuboid_tapered(
                [W - 0.1, 0.16, D - 0.1],
                0.0,
                concrete([0.36, 0.35, 0.34]),
            )),
            [0.0, y - 0.02, 0.0],
            id_quat(),
        ),
    ];
    // Parapet ring: four walls, each capped by its own coping, rather than
    // one slab across the roof — a cap would hide the deck from every angle
    // the contact sheet takes.
    let p_h = 0.7;
    let p_t = 0.34;
    for sz in [-1.0_f32, 1.0] {
        let cz = sz * (D * 0.5 + 0.08 - p_t * 0.5);
        out.push(brick_slab(
            [W + 0.16, p_h, p_t],
            BRICK_RED,
            [0.0, y + p_h * 0.5, cz],
            if sz > 0.0 {
                FaceKey::SidePz
            } else {
                FaceKey::SideNz
            },
            &[],
        ));
        out.push(stone(
            [W + 0.34, 0.12, p_t + 0.16],
            [0.0, y + p_h + 0.06, cz],
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * (W * 0.5 + 0.08 - p_t * 0.5);
        let len = D + 0.16 - p_t * 2.0;
        out.push(brick_slab(
            [p_t, p_h, len],
            BRICK_RED,
            [cx, y + p_h * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
            &[],
        ));
        out.push(stone([p_t + 0.16, 0.12, len], [cx, y + p_h + 0.06, 0.0]));
    }
    // Stair bulkhead — the head of the stair that reaches the roof, and the
    // thing that stops the deck reading as an empty tray.
    out.push(prim(
        solid(cuboid_tapered(
            [2.2, 2.1, 1.8],
            0.0,
            concrete([0.44, 0.43, 0.42]),
        )),
        [-3.4, y + 1.05, 2.0],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [1.0, 1.9, 0.1],
            0.0,
            steel([0.26, 0.23, 0.2]),
        )),
        [-3.4, y + 0.95, 1.06],
        id_quat(),
    ));
    // Vent stack.
    out.push(prim(
        solid(cylinder_tapered(
            0.16,
            1.5,
            8,
            0.0,
            steel([0.34, 0.3, 0.26]),
        )),
        [-1.2, y + 0.75, 2.6],
        id_quat(),
    ));
    out.push(water_tank(y, [2.9, 0.4]));
    out
}

/// The rooftop water tank: a cedar-stave drum on a braced steel frame, with
/// hoops and a conical cap.
///
/// The frame is the sub-root, so the tank rides its legs. Staves rather than
/// hoop-bands come from turning the plank material through a quarter turn
/// ([`timber`] is authored stagger-free for exactly that), which is the one
/// rotation the pattern survives.
fn water_tank(y: f32, at: [f32; 2]) -> Generator {
    let [tx, tz] = at;
    let leg_h = 2.0;
    let spread = 0.82;
    let mut parts = Vec::new();
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.15, leg_h, 0.15],
                0.0,
                steel([0.3, 0.26, 0.22]),
            )),
            [tx + sx * spread, y + leg_h * 0.5, tz + sz * spread],
            id_quat(),
        ));
    }
    // Cross-bracing on the two faces the street sees.
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered(
                [(spread * 2.0).hypot(leg_h * 0.8), 0.08, 0.06],
                0.0,
                steel([0.3, 0.26, 0.22]),
            ),
            [tx, y + leg_h * 0.5, tz + sz * spread],
            quat_z(sz * (leg_h * 0.8).atan2(spread * 2.0)),
        ));
    }
    // Platform the drum sits on.
    parts.push(prim(
        solid(cuboid_tapered(
            [spread * 2.0 + 0.4, 0.14, spread * 2.0 + 0.4],
            0.0,
            steel([0.32, 0.28, 0.24]),
        )),
        [tx, y + leg_h + 0.07, tz],
        id_quat(),
    ));
    let drum_h = 2.5;
    let drum_y = y + leg_h + 0.14 + drum_h * 0.5;
    for hoop in [0.32_f32, 0.68] {
        parts.push(prim(
            torus(0.07, 1.09, steel([0.28, 0.24, 0.2])),
            [tx, y + leg_h + 0.14 + drum_h * hoop, tz],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cylinder_tapered(
            1.02,
            1.0,
            14,
            0.85,
            steel([0.34, 0.3, 0.26]),
        )),
        [tx, drum_y + drum_h * 0.5 + 0.5, tz],
        id_quat(),
    ));
    let drum = prim(
        solid(cylinder_tapered(
            1.05,
            drum_h,
            14,
            0.0,
            util::upright_boards(timber(TANK_WOOD)),
        )),
        [tx, drum_y, tz],
        id_quat(),
    );
    nest(drum, parts)
}

// --- The stoop. ------------------------------------------------------------

/// The raised stoop: an apron, four steps up to the door threshold, and a
/// brick cheek wall each side.
///
/// Every part is derived from the flight itself rather than measured off the
/// building (#972 lesson 8) — the apron is sized *from* the run, so a step
/// can never hang over the edge of the pavement it stands on.
fn stoop() -> Generator {
    let risers = 4;
    let rise = (BASE_H + G_SILL) / risers as f32;
    let going = 0.32;
    let run = going * risers as f32;
    let apron_z = FRONT + 0.2 - (run + 0.5) * 0.5;

    let apron = prim(
        solid(cuboid_tapered(
            [3.9, 0.14, run + 0.5],
            0.0,
            concrete([0.4, 0.4, 0.41]),
        )),
        [0.0, 0.07, apron_z],
        id_quat(),
    );

    let mut parts = Vec::new();
    for i in 0..risers {
        let top = (i + 1) as f32 * rise;
        parts.push(prim(
            solid(cuboid_tapered(
                [2.8, top, going],
                0.0,
                concrete([0.5, 0.49, 0.47]),
            )),
            [
                0.0,
                top * 0.5,
                FRONT + 0.06 - going * (risers - i) as f32 + going * 0.5,
            ],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let cz = FRONT + 0.06 - run * 0.5;
        parts.push(brick_slab(
            [0.42, BASE_H + G_SILL + 0.2, run],
            BRICK_RED,
            [sx * 1.61, (BASE_H + G_SILL + 0.2) * 0.5, cz],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
            &[],
        ));
        parts.push(stone(
            [0.56, 0.12, run + 0.12],
            [sx * 1.61, BASE_H + G_SILL + 0.26, cz],
        ));
    }
    nest(apron, parts)
}

// --- The fire escape. ------------------------------------------------------

/// Steel fire escape over the two left-hand bays: a landing at each upper
/// floor line, an outer railing, a zig-zag of stringers, the two verticals
/// that carry the lot, and the drop ladder.
///
/// It hangs off its own sub-root — the lowest landing — so the whole
/// assembly moves as one.
fn fire_escape() -> Generator {
    let fe_x = (BAY_X[0] + BAY_X[1]) * 0.5;
    let fe_w = (BAY_X[1] - BAY_X[0]) + OPEN_W + 0.6;
    let land_z = FRONT - 0.78;
    let rail_z = FRONT - 1.42;
    // A landing sits at its floor line, three quarters of a metre under the
    // sill it serves, which is what makes climbing out of the window read.
    let floor_y = |f: usize| BASE_H + GROUND_H + f as f32 * STOREY + 0.1;

    let mut parts = Vec::new();
    for f in 0..FLOORS {
        let y = floor_y(f);
        if f > 0 {
            parts.push(prim(
                solid(cuboid_tapered([fe_w, 0.1, 1.4], 0.0, steel(FE_STEEL))),
                [fe_x, y, land_z],
                id_quat(),
            ));
        }
        // Outer railing: an open frame, not a plate — a solid panel at this
        // size reads as a balcony wall and hides the landing behind it.
        parts.push(prim(
            cuboid_tapered([fe_w, 0.07, 0.07], 0.0, steel(FE_STEEL)),
            [fe_x, y + 0.95, rail_z],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered([fe_w, 0.05, 0.05], 0.0, steel(FE_STEEL)),
            [fe_x, y + 0.5, rail_z],
            id_quat(),
        ));
        for i in 0..5 {
            let x = fe_x - fe_w * 0.5 + fe_w * (i as f32 + 0.5) / 5.0;
            parts.push(prim(
                cuboid_tapered([0.05, 0.95, 0.05], 0.0, steel(FE_STEEL)),
                [x, y + 0.48, rail_z],
                id_quat(),
            ));
        }
        // Stringer up to the landing above, alternating side.
        if f + 1 < FLOORS {
            let y_hi = floor_y(f + 1);
            let dir = if f % 2 == 0 { 1.0 } else { -1.0 };
            let run = fe_w * 0.62;
            let rise = y_hi - y;
            parts.push(prim(
                cuboid_tapered([run.hypot(rise), 0.09, 0.62], 0.0, steel(FE_STEEL)),
                [fe_x + dir * fe_w * 0.14, (y + y_hi) * 0.5, land_z - 0.16],
                quat_z(dir * rise.atan2(run)),
            ));
            parts.push(prim(
                cuboid_tapered([run.hypot(rise), 0.06, 0.06], 0.0, steel(FE_STEEL)),
                [
                    fe_x + dir * fe_w * 0.14,
                    (y + y_hi) * 0.5 + 0.6,
                    land_z - 0.44,
                ],
                quat_z(dir * rise.atan2(run)),
            ));
        }
    }
    // The two verticals that carry the assembly, from the lowest landing to
    // the top one.
    let span = floor_y(FLOORS - 1) - floor_y(0);
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.12, span + 1.1, 0.12],
                0.0,
                steel(FE_STEEL),
            )),
            [
                fe_x + sx * fe_w * 0.5,
                floor_y(0) + span * 0.5 + 0.3,
                rail_z,
            ],
            id_quat(),
        ));
    }
    // Drop ladder hanging below the lowest landing, stopping short of the
    // pavement the way a counterweighted one does.
    let drop_top = floor_y(0);
    let drop = 1.7_f32;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.06, drop, 0.06], 0.0, steel(FE_STEEL)),
            [
                fe_x + fe_w * 0.3 + sx * 0.3,
                drop_top - drop * 0.5,
                land_z - 0.1,
            ],
            id_quat(),
        ));
    }
    for i in 0..5 {
        parts.push(prim(
            cuboid_tapered([0.62, 0.05, 0.05], 0.0, steel(FE_STEEL)),
            [
                fe_x + fe_w * 0.3,
                drop_top - drop * (i as f32 + 0.5) / 5.0,
                land_z - 0.1,
            ],
            id_quat(),
        ));
    }

    let base = prim(
        solid(cuboid_tapered([fe_w, 0.1, 1.4], 0.0, steel(FE_STEEL))),
        [fe_x, floor_y(0), land_z],
        id_quat(),
    );
    nest(base, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_sanitize_stable, window_cards,
    };
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    /// Walk the tree accumulating translations, calling `f` with every node's
    /// world position. Every sub-root here is axis-aligned, so a plain sum is
    /// the world frame.
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
        assert_sanitize_stable(&Tenement.build(""), "tenement");
    }

    /// #972 lesson 1: every `Window` card sits on a `Plane` at `uv_scale`
    /// 1.0, and there is exactly one per opening plus the transom over the
    /// door. An exact count is the part that bites: a card moved onto a solid
    /// still renders, it just renders as a frame with holes onto the brick
    /// behind it, which is what this entry used to be.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let root = Tenement.build("");
        let mut cards = 0;
        walk(&root, [0.0; 3], &mut |g, _| {
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
        assert_eq!(
            cards,
            openings().len() + 1,
            "expected one card per opening plus the door's transom"
        );
    }

    /// No two glazed surfaces share a plane and overlap.
    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Tenement.build(""), "tenement");
    }

    /// #972 lesson 7: a card lapped over its opening, never flush with the
    /// reveal, and always in front of the room panel it frames.
    #[test]
    fn cards_lap_their_openings_and_stand_clear_of_the_rooms() {
        let root = Tenement.build("");
        for c in window_cards(&root) {
            assert!(
                c.size[0] > OPEN_W + 1e-4,
                "card {c:?} is flush with its reveal in X",
                c = c.size
            );
            assert!(
                c.center[2] < ROOM_Z - 0.2,
                "card at z {} is not clear of the room panel at {ROOM_Z}",
                c.center[2]
            );
        }
    }

    /// #972 lesson 2: the bond is laid flat, at a real brick, in one shared
    /// world course frame — so every brick slab's `uv_offset` must equal its
    /// own face's projection of its own position. A slab moved without its
    /// offset following restarts the courses at its own centre, which is far
    /// too subtle to catch in a render.
    #[test]
    fn every_brick_slab_sits_in_the_shared_course_frame() {
        use crate::pds::generator::FaceKey;
        let root = Tenement.build("");
        let mut checked = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid {
                material, faces, ..
            } = &g.kind
            else {
                return;
            };
            if !matches!(material.texture, SovereignTextureConfig::Brick(_)) {
                return;
            }
            // The base material serves whichever side face the slab presents;
            // all four side conventions agree on V, so testing the U half
            // against every candidate would be vacuous. Instead assert the
            // offset is *one of* the six face projections, and that each
            // explicit override matches its own face exactly.
            let want: Vec<_> = [
                FaceKey::SideNz,
                FaceKey::SidePz,
                FaceKey::SideNx,
                FaceKey::SidePx,
                FaceKey::Top,
                FaceKey::Bottom,
            ]
            .into_iter()
            .map(|f| util::face_uv_offset(f, at).0)
            .collect();
            let got = material.uv_offset.0;
            assert!(
                want.iter()
                    .any(|w| (w[0] - got[0]).abs() < 1e-3 && (w[1] - got[1]).abs() < 1e-3),
                "brick slab at {at:?} carries uv_offset {got:?}, which is not any \
                 face's projection of its own position — its courses restart at \
                 its own centre"
            );
            for o in faces {
                let w = util::face_uv_offset(o.face, at).0;
                let g = o.material.uv_offset.0;
                assert!(
                    (w[0] - g[0]).abs() < 1e-3 && (w[1] - g[1]).abs() < 1e-3,
                    "the {:?} wrap at {at:?} carries {g:?}, not {w:?}",
                    o.face
                );
            }
            checked += 1;
        });
        assert!(checked >= 20, "only {checked} brick slabs found");
    }

    /// #972 lesson 8: everything the stoop is made of stands inside the apron
    /// it stands on. A flight measured off the building instead of off its
    /// own run puts the bottom step on bare ground, and no contact-sheet
    /// angle looks along that edge.
    #[test]
    fn the_stoop_stands_on_its_own_apron() {
        // The stoop is its own subtree and its sub-root *is* the apron, which
        // is the whole point of building it that way: the guard can name the
        // pad without guessing which slab it is.
        let root = Tenement.build("");
        let stoop = &root.children[1];
        let mut apron: Option<([f32; 3], [f32; 3])> = None;
        let mut parts = Vec::new();
        walk(stoop, root.transform.translation.0, &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let half = [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5];
            if apron.is_none() {
                apron = Some((at, half));
            } else {
                parts.push((at, half));
            }
        });
        let (ac, ah) = apron.expect("the stoop stands on an apron");
        assert!(
            !parts.is_empty(),
            "no stoop parts found in front of the wall"
        );
        for (c, h) in parts {
            for axis in [0usize, 2] {
                assert!(
                    c[axis] - h[axis] > ac[axis] - ah[axis] - 1e-3
                        && c[axis] + h[axis] < ac[axis] + ah[axis] + 1e-3,
                    "a stoop part at {c:?} (half {h:?}) hangs over the apron \
                     at {ac:?} (half {ah:?}) on axis {axis}"
                );
            }
        }
    }

    /// The fire escape's landings line up with the floors they serve, and sit
    /// below the sills people climb out of.
    #[test]
    fn fire_escape_landings_sit_under_their_sills() {
        for f in 0..FLOORS {
            let landing = GROUND_H + f as f32 * STOREY + 0.1;
            let sill = GROUND_H + f as f32 * STOREY + U_SILL_OFF;
            assert!(
                sill - landing > 0.5 && sill - landing < 1.1,
                "landing {landing} is {} below its sill {sill}",
                sill - landing
            );
        }
    }

    /// The building keeps a lit window: escalation's broken-emissive ruin
    /// pass needs something to snuff.
    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Tenement.build("")
        ));
    }

    /// The editability contract (#972 lesson 3): the prop is a tree that
    /// stands the way the building does, so dragging the cornice takes the
    /// whole roofscape with it and dragging the stoop takes its steps.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Tenement.build("");
        assert_eq!(
            root.children.len(),
            4,
            "plinth carries shell, stoop, escape, and its buried footing"
        );
        let shell = &root.children[0];
        let cornice = shell
            .children
            .iter()
            .find(|c| c.children.len() > 6)
            .expect("the cornice carries the roofscape");
        assert!(
            cornice.children.iter().any(|c| c.children.len() >= 6),
            "the tank's drum carries its frame"
        );
        assert!(count(&root) > 100, "the shell lost most of its parts");
        assert!(
            count(&root) < (crate::pds::sanitize::limits::MAX_GENERATOR_NODES as usize),
            "the tree exceeds the record's node budget"
        );
    }
}
