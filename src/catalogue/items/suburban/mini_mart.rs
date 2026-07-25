//! Mini-mart — a Suburban secondary. A small convenience store on an asphalt
//! forecourt: a brick base under rendered walls, a glazed shopfront with a
//! door you could actually walk through, a lit fascia, a flat parapet with an
//! AC unit, and a pylon sign at the kerb.
//!
//! The shopfront is the whole point of this entry, and it is now a **shell**.
//! Before the overhaul it was [`curtain_wall`](crate::catalogue::items::modern_city::curtain_wall)
//! — a lit glass box with proud mullion fins — pinned to a solid mass, with no
//! interior and no entrance at all. That abstraction is fine on the tower it
//! was written for, where nobody stands close enough to look in; on a shop at
//! eye level it reads as an illuminated panel, and the `Window` texture it was
//! handed masks its panes *away*, so the one thing it could not do was be a
//! window. Here the brickwork and render *frame* the opening, glazing cards
//! fill it, and behind them is a lit shop: gondola aisles, a chiller run, a
//! counter and a ceiling strip.
//!
//! Everything else on the #972 ledger lands too — the brick lies flat at a
//! real 215 mm in one shared course frame ([`util::bonded_brick`]), the base
//! course stands proud of the render above so their side faces never share a
//! plane, and the tree stands the way the building does.
//!
//! [`util::bonded_brick`]: crate::catalogue::items::util::bonded_brick

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::roadside::{SIGN_AMBER, asphalt, sign_board};
use crate::catalogue::items::util::{
    self, cuboid_tapered, glow, id_quat, lit_interior, nest, plane, prim, quat_x, solid,
    window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_TAN, CAR_SILVER, RENDER_WHITE, brick, concrete, enamel, parked_car, render};

// --- Shell dimensions. Everything below is derived from these. -------------

const W: f32 = 10.0;
const D: f32 = 8.0;
const BASE_H: f32 = 0.4;
/// Height of the brick base course, measured from the forecourt.
const BRICK_H: f32 = 1.0;
/// Wall height from the forecourt to the underside of the parapet.
const BODY_H: f32 = 4.0;
/// Wall thickness, and so the depth of the shopfront reveal.
const WALL_T: f32 = 0.32;

/// Outer face of the front wall — the hero direction, `-Z`.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane, set back inside the reveal so the wall's thickness reads.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;
/// How far behind the glazing the front aisle stands. Close enough that a pane
/// frames a whole object rather than a speck of the back wall (#972 lesson 6).
const AISLE_Z: f32 = FRONT + 1.55;
/// How far a glazing card oversails its opening on every edge (lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Width of the piers closing the shopfront's two ends.
const PIER_W: f32 = 0.9;
/// The shopfront opening spans this in X, between the piers.
const OPEN_X0: f32 = -W * 0.5 + PIER_W;
const OPEN_X1: f32 = W * 0.5 - PIER_W;
/// Where the display run ends and the entrance bay begins.
const DOOR_X0: f32 = 2.0;
/// Head height of the whole shopfront opening.
const HEAD_Y: f32 = BASE_H + 3.05;
/// Top of the brick stall riser under the display glazing. The entrance bay
/// has none, so the doors reach the floor.
const SILL_Y: f32 = BASE_H + BRICK_H;

/// Brick length in metres — a real 215 mm brick.
const BRICK_LEN: f32 = 0.215;

// --- Palette local to this entry. ------------------------------------------

/// Shopfront joinery — dark anodised aluminium, which is what draws the
/// opening against pale render.
const SHOPFRONT: [f32; 3] = [0.22, 0.23, 0.25];
/// Forecourt asphalt, and the paint on it.
const TARMAC: [f32; 3] = [0.26, 0.26, 0.28];
const BAY_PAINT: [f32; 3] = [0.82, 0.80, 0.72];

/// The store's brickwork, laid flat at a real brick's size and bonded into one
/// world course frame for the face it serves.
fn bonded(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(BRICK_TAN), BRICK_LEN, face, center)
}

/// One brick slab of the base course. `wraps` names the other faces of the
/// same slab that meet brick at a corner someone can see; each costs a draw
/// call, so it lists the corners that read, not every face that exists.
fn brick_slab(size: [f32; 3], center: [f32; 3], face: FaceKey, wraps: &[FaceKey]) -> Generator {
    let mut kind = solid(cuboid_tapered(size, 0.0, bonded(center, face)));
    for &w in wraps {
        kind = util::with_face(kind, w, bonded(center, w));
    }
    prim(kind, center, id_quat())
}

/// Rendered wall slab.
fn wall(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, render(RENDER_WHITE))),
        center,
        id_quat(),
    )
}

pub struct MiniMart;

impl CatalogueEntry for MiniMart {
    fn slug(&self) -> &'static str {
        "mini_mart"
    }
    fn name(&self) -> &'static str {
        "Mini-Mart"
    }
    fn description(&self) -> &'static str {
        "Small convenience store with a glazed storefront and a lit pole sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Forecourt at the bottom, and on it the shop, the pylon and a customer's
/// car. Written outermost-last, because [`nest`] rebases a subtree that
/// already carries its own world translation.
fn build_tree() -> Generator {
    let pad = prim(
        solid(cuboid_tapered(
            [W + 6.0, BASE_H, D + 4.0],
            0.0,
            asphalt(TARMAC),
        )),
        [0.0, BASE_H * 0.5, -3.0],
        id_quat(),
    );

    let mut parts = vec![shop(), pylon()];
    // Painted parking bays, so the forecourt is a car park rather than a grey
    // rectangle. Held a hair above the tarmac: paint flush with the slab would
    // be two coplanar faces fighting for the same plane.
    //
    // Bays and car are both sized off the pad's own front edge rather than
    // guessed from the building. Guessed, they overhung it: the car stood a
    // metre past the tarmac on nothing, which is the same bug the house's
    // driveway had, and the reason both now derive their depth from the slab
    // they stand on.
    let pad_front = -3.0 - (D + 4.0) * 0.5;
    let bay_len = 4.0;
    let bay_z = pad_front + bay_len * 0.5;
    for i in 0..3 {
        parts.push(prim(
            cuboid_tapered([0.12, 0.02, bay_len], 0.0, enamel(BAY_PAINT)),
            [-1.4 + i as f32 * 2.6, BASE_H + 0.01, bay_z],
            id_quat(),
        ));
    }
    parts.extend(parked_car([2.5, BASE_H, bay_z], CAR_SILVER));

    nest(pad, parts)
}

// --- The shop. -------------------------------------------------------------

/// Sales-floor deck, and under it the whole shop: the brick base course, the
/// rendered walls, the shopfront that frames the glazing, the fit-out behind
/// it, and — on the walls — the parapet with its AC unit.
///
/// The deck is the sub-root because it is the lowest piece of the building and
/// everything else stands on or above it.
fn shop() -> Generator {
    let inner = [W - WALL_T * 2.0, D - WALL_T * 2.0];
    let mut parts = Vec::new();

    // Base course, standing 40 mm proud of the render above it.
    //
    // Flush, the two masses' side faces are coplanar all the way round the
    // building and z-fight along every elevation — the whole perimeter, on the
    // most-looked-at part of the wall. A base course is a plinth in real
    // construction anyway, so the fix is also the truth.
    for (size, center, face, wraps) in [
        (
            [W + 0.08, BRICK_H, WALL_T],
            [0.0, BASE_H + BRICK_H * 0.5, D * 0.5 - WALL_T * 0.5],
            FaceKey::SidePz,
            &[][..],
        ),
        (
            [WALL_T, BRICK_H, D - WALL_T * 2.0],
            [W * 0.5 + 0.04 - WALL_T * 0.5, BASE_H + BRICK_H * 0.5, 0.0],
            FaceKey::SidePx,
            &[FaceKey::Top][..],
        ),
        (
            [WALL_T, BRICK_H, D - WALL_T * 2.0],
            [
                -(W * 0.5 + 0.04) + WALL_T * 0.5,
                BASE_H + BRICK_H * 0.5,
                0.0,
            ],
            FaceKey::SideNx,
            &[FaceKey::Top][..],
        ),
    ] {
        parts.push(brick_slab(size, center, face, wraps));
    }

    // Rendered walls above the base, on the three closed sides.
    let upper_h = BODY_H - BRICK_H;
    let upper_y = BASE_H + BRICK_H + upper_h * 0.5;
    parts.push(wall(
        [W, upper_h, WALL_T],
        [0.0, upper_y, D * 0.5 - WALL_T * 0.5],
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, upper_h, D - WALL_T * 2.0],
            [sx * (W * 0.5 - WALL_T * 0.5), upper_y, 0.0],
        ));
    }

    shopfront(&mut parts);
    sales_floor(&mut parts, inner);
    parts.push(parapet());

    let deck = prim(
        cuboid_tapered(
            [inner[0], 0.08, inner[1]],
            0.0,
            lit_interior([0.30, 0.29, 0.28], 0.14),
        ),
        [0.0, BASE_H + 0.05, 0.0],
        id_quat(),
    );
    nest(deck, parts)
}

/// The hero face, built as the pieces that *frame* the opening — two rendered
/// piers, a brick stall riser under the display bay, and the fascia beam over
/// the lot — plus the glazing and the entrance filling it.
fn shopfront(parts: &mut Vec<Generator>) {
    // Piers closing the two front corners, brick below and render above so
    // they continue both bands round the corner.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * (W * 0.5 - PIER_W * 0.5);
        parts.push(brick_slab(
            [PIER_W, BRICK_H, WALL_T],
            [x, BASE_H + BRICK_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
            &[if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            }],
        ));
        parts.push(wall(
            [PIER_W, BODY_H - BRICK_H, WALL_T],
            [x, BASE_H + BRICK_H + (BODY_H - BRICK_H) * 0.5, FRONT_MID],
        ));
    }
    // Stall riser under the display bay only — the same brick as the base
    // course, carrying on below the sill rather than reading as a separate
    // material bolted under the window. Its top is the sill a shopper leans
    // over, so that face wraps into the world frame too.
    let riser_w = DOOR_X0 - OPEN_X0;
    parts.push(brick_slab(
        [riser_w, SILL_Y - BASE_H, WALL_T],
        [OPEN_X0 + riser_w * 0.5, (BASE_H + SILL_Y) * 0.5, FRONT_MID],
        FaceKey::SideNz,
        &[FaceKey::Top],
    ));
    // Beam over the whole opening, carrying the wall up to the parapet.
    let beam_h = BASE_H + BODY_H - HEAD_Y;
    parts.push(wall(
        [OPEN_X1 - OPEN_X0, beam_h, WALL_T],
        [(OPEN_X0 + OPEN_X1) * 0.5, HEAD_Y + beam_h * 0.5, FRONT_MID],
    ));

    // Display glazing: 4.9 × 2.05, five panes across by two up comes out
    // near-square at that aspect. Opacity below the 0.5 mask cutoff, so the
    // panes are genuinely open and the fit-out shows through them.
    let disp_w = DOOR_X0 - OPEN_X0;
    let disp_h = HEAD_Y - SILL_Y;
    parts.push(prim(
        plane(
            [disp_w + GLAZE_LAP, disp_h + GLAZE_LAP],
            window_card(SHOPFRONT, 5, 2, 0.34, 0.03),
        ),
        [OPEN_X0 + disp_w * 0.5, SILL_Y + disp_h * 0.5, GLAZE_Z],
        quat_x(-FRAC_PI_2),
    ));
    // Entrance: a pair of glazed leaves reaching the floor, and a mullion
    // between them. This is the thing the old storefront had no version of —
    // there was simply no way in.
    let door_w = OPEN_X1 - DOOR_X0;
    let door_h = HEAD_Y - BASE_H;
    parts.push(prim(
        plane(
            [door_w + GLAZE_LAP, door_h + GLAZE_LAP],
            window_card(SHOPFRONT, 2, 3, 0.34, 0.06),
        ),
        [DOOR_X0 + door_w * 0.5, BASE_H + door_h * 0.5, GLAZE_Z],
        quat_x(-FRAC_PI_2),
    ));
    parts.push(prim(
        cuboid_tapered([0.09, door_h, 0.1], 0.0, enamel(SHOPFRONT)),
        [
            DOOR_X0 + door_w * 0.5,
            BASE_H + door_h * 0.5,
            GLAZE_Z - 0.07,
        ],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.05, 1.0, 0.05], 0.0, enamel([0.62, 0.63, 0.65])),
            [
                DOOR_X0 + door_w * 0.5 + sx * 0.26,
                BASE_H + 1.05,
                GLAZE_Z - 0.11,
            ],
            id_quat(),
        ));
    }

    // Lit fascia over the opening — segmented cells in a housing, not a washed
    // slab: a broad flat panel at strength blooms to white.
    parts.extend(sign_board(
        [0.0, BASE_H + BODY_H - 0.42, FRONT - 0.12],
        [W - 1.6, 0.78],
        (5, 1),
        SIGN_AMBER,
        2.0,
        -1.0,
    ));
    // Bollards guarding the glazing, which is what a forecourt shop has.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.16, 0.85, 0.16],
                0.06,
                enamel([0.78, 0.62, 0.18]),
            )),
            [sx * 3.1, BASE_H + 0.42, FRONT - 0.75],
            id_quat(),
        ));
    }
    // An ice chest parked by the door, the way they always are.
    parts.push(prim(
        solid(cuboid_tapered(
            [1.0, 0.95, 0.7],
            0.04,
            enamel([0.86, 0.86, 0.84]),
        )),
        [OPEN_X0 - 0.1, BASE_H + 0.48, FRONT - 0.55],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.8, 0.3, 0.06], 0.0, glow([0.30, 0.55, 0.85], 0.9)),
        [OPEN_X0 - 0.1, BASE_H + 0.62, FRONT - 0.92],
        id_quat(),
    ));
}

/// What the shopper sees through the glazing: a front aisle of stock right
/// behind the glass, a lit chiller run down the far wall, a counter by the
/// door, and a ceiling strip.
///
/// Depth discipline is the whole game here. Goods against the back wall of an
/// 8 m shop sit seven metres behind the glass and shrink to unreadable specks;
/// the front aisle is held at [`AISLE_Z`], close enough that a pane frames a
/// recognisable object.
fn sales_floor(parts: &mut Vec<Generator>, inner: [f32; 2]) {
    // Dim rear lining, the envelope everything else reads against.
    parts.push(prim(
        cuboid_tapered(
            [inner[0], BODY_H - 0.5, 0.08],
            0.0,
            lit_interior([0.24, 0.23, 0.22], 0.11),
        ),
        [0.0, BASE_H + BODY_H * 0.5, D * 0.5 - WALL_T - 0.06],
        id_quat(),
    ));
    // Ceiling strips, well forward so they light what the glazing shows.
    for z in [-1.4_f32, 0.9] {
        parts.push(prim(
            cuboid_tapered(
                [inner[0] * 0.62, 0.09, 0.34],
                0.0,
                glow([1.0, 0.97, 0.88], 1.7),
            ),
            [0.0, BASE_H + BODY_H - 0.55, z],
            id_quat(),
        ));
    }

    // Front gondola: a plinth and two shelves of stock, only as wide as the
    // display bay.
    let disp_w = DOOR_X0 - OPEN_X0;
    let disp_cx = OPEN_X0 + disp_w * 0.5;
    parts.push(prim(
        cuboid_tapered(
            [disp_w - 0.4, 0.95, 0.6],
            0.0,
            lit_interior([0.32, 0.30, 0.28], 0.15),
        ),
        [disp_cx, BASE_H + 0.52, AISLE_Z],
        id_quat(),
    ));
    for y in [1.15_f32, 1.75] {
        parts.push(prim(
            cuboid_tapered(
                [disp_w - 0.4, 0.08, 0.55],
                0.0,
                lit_interior([0.34, 0.32, 0.30], 0.15),
            ),
            [disp_cx, BASE_H + y, AISLE_Z],
            id_quat(),
        ));
    }
    // Stock. The only saturated colour inside, sized so one box roughly fills
    // a pane — smaller reads as noise through the mullions.
    let stock = [
        (-3.4_f32, 1.32_f32, [0.74, 0.24, 0.18_f32]),
        (-2.5, 1.32, [0.86, 0.70, 0.24]),
        (-1.55, 1.32, [0.24, 0.46, 0.66]),
        (-0.6, 1.32, [0.78, 0.44, 0.16]),
        (0.35, 1.32, [0.32, 0.56, 0.30]),
        (-3.1, 1.92, [0.84, 0.64, 0.26]),
        (-1.9, 1.92, [0.56, 0.26, 0.52]),
        (-0.7, 1.92, [0.26, 0.52, 0.60]),
        (0.4, 1.92, [0.76, 0.34, 0.22]),
    ];
    for (x, y, c) in stock {
        parts.push(prim(
            cuboid_tapered([0.58, 0.44, 0.42], 0.0, lit_interior(c, 0.5)),
            [x, BASE_H + y, AISLE_Z],
            id_quat(),
        ));
    }
    // Chiller run down the far wall — the cold blue glow that says "shop" from
    // outside at any hour.
    parts.push(prim(
        cuboid_tapered([0.7, 2.1, 3.4], 0.0, lit_interior([0.28, 0.30, 0.32], 0.14)),
        [-W * 0.5 + WALL_T + 0.4, BASE_H + 1.1, 0.9],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.1, 1.7, 3.1], 0.0, glow([0.44, 0.68, 0.86], 1.1)),
        [-W * 0.5 + WALL_T + 0.78, BASE_H + 1.15, 0.9],
        id_quat(),
    ));
    // The service counter, squarely behind the **entrance** bay.
    //
    // Parked off to one side of it — where it was — the doors framed nothing
    // but unlit floor five metres back, and a glazed entrance with nothing lit
    // behind it is a black rectangle: the display bay read beautifully while
    // the way in read as a hole in the wall. Both bays need their own thing to
    // look at, and the counter is the obvious one for this bay, because it is
    // what a shopper walks toward.
    let door_cx = (DOOR_X0 + OPEN_X1) * 0.5;
    parts.push(prim(
        cuboid_tapered([2.0, 1.05, 0.7], 0.0, lit_interior([0.34, 0.30, 0.26], 0.2)),
        [door_cx, BASE_H + 0.57, FRONT + 2.2],
        id_quat(),
    ));
    // Back-bar behind the counter, so the sightline through the doors ends on
    // a lit surface rather than running out into the dark.
    parts.push(prim(
        cuboid_tapered(
            [2.4, 2.3, 0.09],
            0.0,
            lit_interior([0.36, 0.33, 0.28], 0.22),
        ),
        [door_cx, BASE_H + 1.25, FRONT + 3.4],
        id_quat(),
    ));
    for y in [1.0_f32, 1.7] {
        parts.push(prim(
            cuboid_tapered(
                [2.2, 0.07, 0.3],
                0.0,
                lit_interior([0.40, 0.36, 0.30], 0.24),
            ),
            [door_cx, BASE_H + y, FRONT + 3.2],
            id_quat(),
        ));
    }
}

/// The parapet band round the roof, and the AC unit standing on the deck
/// behind it.
///
/// The band is proud of the walls on every side, so it joins nothing in-plane
/// and needs no corner wrap; the roof deck is held a hair inside the walls for
/// the same reason.
fn parapet() -> Generator {
    let top = BASE_H + BODY_H;
    let band = prim(
        solid(cuboid_tapered(
            [W + 0.34, 0.55, D + 0.34],
            0.0,
            render([0.72, 0.71, 0.69]),
        )),
        [0.0, top + 0.2, 0.0],
        id_quat(),
    );
    nest(
        band,
        vec![
            prim(
                solid(cuboid_tapered(
                    [W - 0.06, 0.22, D - 0.06],
                    0.0,
                    concrete([0.40, 0.40, 0.41]),
                )),
                [0.0, top - 0.05, 0.0],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered(
                    [2.0, 0.9, 1.8],
                    0.04,
                    enamel([0.70, 0.70, 0.72]),
                )),
                [-2.5, top + 0.5, 1.2],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered(
                    [1.5, 0.1, 1.4],
                    0.0,
                    enamel([0.52, 0.53, 0.55]),
                )),
                [-2.5, top + 0.98, 1.2],
                id_quat(),
            ),
        ],
    )
}

/// The pylon at the kerb: a footing, a mast, and a lit board on top.
///
/// Rooted at the footing so a drag on it takes mast and board together — the
/// sign is the one part of this entry a settlement is most likely to want
/// moved, and under a flat list it left its own pole behind.
fn pylon() -> Generator {
    // Set in from the pad's own edge by the footing's half-width. Placed at a
    // round `W / 2 + 3`, it landed exactly on the edge and the footing
    // overhung it by 450 mm — a sign standing half on air.
    const FOOT: f32 = 0.9;
    let x = (W + 6.0) * 0.5 - FOOT * 0.5 - 0.15;
    let z = FRONT - 1.2;
    let foot = prim(
        solid(cuboid_tapered(
            [FOOT, 0.34, FOOT],
            0.08,
            concrete([0.56, 0.55, 0.54]),
        )),
        [x, BASE_H + 0.1, z],
        id_quat(),
    );
    let mast = prim(
        solid(cuboid_tapered(
            [0.32, 5.0, 0.32],
            0.0,
            enamel([0.58, 0.59, 0.61]),
        )),
        [x, BASE_H + 2.7, z],
        id_quat(),
    );
    let mut board = sign_board(
        [x, BASE_H + 5.0, z],
        [1.9, 1.7],
        (1, 2),
        SIGN_AMBER,
        2.2,
        -1.0,
    );
    let head = board.remove(0);
    nest(foot, vec![nest(mast, vec![nest(head, board)])])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&MiniMart.build(""), "mini_mart");
    }

    fn walk(g: &Generator, at: [f32; 3], f: &mut impl FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// LESSON 1: the glazing is two cards on flat quads at `uv_scale` 1.0, each
    /// lapping its opening, and no solid carries one.
    ///
    /// The old storefront was a lit glass *cuboid* behind a grid of fins, so
    /// the `Window` texture wrapped its six faces and masked its panes away
    /// onto the solid wall behind — a shopfront that could not be looked
    /// through, on a shop with no way in.
    #[test]
    fn the_glazing_is_unscaled_lapping_quads() {
        let mut planes = 0;
        walk(&MiniMart.build(""), [0.0; 3], &mut |g, _| match &g.kind {
            GeneratorKind::Plane { size, material, .. }
                if matches!(material.texture, SovereignTextureConfig::Window(_)) =>
            {
                assert_eq!(
                    material.uv_scale.0, 1.0,
                    "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                );
                assert!(
                    size.0[0] > GLAZE_LAP && size.0[1] > GLAZE_LAP,
                    "card {:?} does not lap its opening",
                    size.0
                );
                planes += 1;
            }
            GeneratorKind::Cuboid { material, .. } => assert!(
                !matches!(material.texture, SovereignTextureConfig::Window(_)),
                "a Window card on a solid is a frame over nothing"
            ),
            _ => {}
        });
        assert_eq!(
            planes, 2,
            "expected the display bay and the entrance glazed"
        );
    }

    /// LESSON 1's other half, and lesson 6: there is a lit fit-out, and it
    /// stands close enough to the glass that a pane frames it.
    #[test]
    fn the_fitout_stands_close_behind_the_glass() {
        let mut near = 0;
        walk(&MiniMart.build(""), [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { material, .. } = &g.kind
                && material.emission_strength.0 > 0.12
                && material.emission_strength.0 < 1.0
                && pos[2] < AISLE_Z + 0.3
            {
                near += 1;
            }
        });
        assert!(
            near >= 10,
            "only {near} lit pieces sit within reach of the glazing — the \
             storefront will read as a black hole"
        );
    }

    /// LESSON 2: every brick surface sits in the one world course frame, for
    /// the face it serves, with its courses flat and its bond tiling.
    #[test]
    fn the_brickwork_is_flat_and_bonded() {
        let root = MiniMart.build("");
        let mut slabs = Vec::new();
        let mut overrides = Vec::new();
        walk(&root, [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid {
                material, faces, ..
            } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Brick(_))
            {
                slabs.push((pos, material.clone()));
                for ov in faces {
                    overrides.push((pos, ov.face, ov.material.clone()));
                }
            }
        });
        assert_eq!(
            slabs.len(),
            6,
            "expected 3 base-course slabs + 2 front piers + the stall riser"
        );
        const FACES: [FaceKey; 6] = [
            FaceKey::SideNz,
            FaceKey::SidePz,
            FaceKey::SidePx,
            FaceKey::SideNx,
            FaceKey::Top,
            FaceKey::Bottom,
        ];
        for (pos, m) in &slabs {
            let SovereignTextureConfig::Brick(cfg) = &m.texture else {
                unreachable!("filtered to Brick above");
            };
            let cols = (cfg.scale.0 * cfg.aspect_ratio.0).round();
            assert!(
                cols < cfg.scale.0,
                "slab at {pos:?}: {cols} columns to {} rows stands its bricks upright",
                cfg.scale.0
            );
            assert!(
                cols >= 4.0,
                "slab at {pos:?}: {cols} bricks per tile splits at the seam"
            );
            let stagger = cfg.scale.0 * cfg.row_offset.0;
            assert!(
                (stagger - stagger.round()).abs() < 1e-6,
                "slab at {pos:?}: scale × row_offset = {stagger} does not tile"
            );
            assert!(
                cfg.cell_variance.0 <= 0.15,
                "slab at {pos:?}: jitter too high"
            );
            assert!(
                FACES.iter().any(|&f| {
                    let e = util::face_uv_offset(f, *pos).0;
                    (e[0] - m.uv_offset.0[0]).abs() < 1e-3 && (e[1] - m.uv_offset.0[1]).abs() < 1e-3
                }),
                "slab at {pos:?} carries offset {:?}, which is no face's frame",
                m.uv_offset.0
            );
        }
        // Overrides name their face, so they are checked exactly.
        assert_eq!(
            overrides.len(),
            5,
            "expected both base-course returns, the riser's sill and both pier \
             returns to wrap — and nothing else, since a vertical corner \
             carries its courses on the base offset alone"
        );
        for (pos, face, m) in &overrides {
            let e = util::face_uv_offset(*face, *pos).0;
            assert!(
                (e[0] - m.uv_offset.0[0]).abs() < 1e-3 && (e[1] - m.uv_offset.0[1]).abs() < 1e-3,
                "{face:?} of the slab at {pos:?} carries offset {:?}, not its frame's {e:?}",
                m.uv_offset.0
            );
        }
    }

    /// The base course stands **proud** of the render above it.
    ///
    /// Flush, the two masses' vertical side faces are coplanar around the whole
    /// perimeter and z-fight along every elevation. It is the standing coplanar
    /// trap in its most expensive form — a full-height seam on the part of the
    /// building people look at — and it is invisible in a still render, which
    /// is why it is pinned here rather than left to the eye.
    #[test]
    fn the_base_course_oversails_the_render() {
        let root = MiniMart.build("");
        let mut brick_x = 0.0_f32;
        let mut render_x = 0.0_f32;
        walk(&root, [0.0; 3], &mut |g, pos| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            // The side walls: slender in X, deep in Z, centred on x = 0's
            // mirror pair, and away from the shopfront plane.
            if size.0[0] > 0.5 || pos[2].abs() > 0.01 {
                return;
            }
            let edge = pos[0].abs() + size.0[0] * 0.5;
            match &material.texture {
                SovereignTextureConfig::Brick(_) => brick_x = brick_x.max(edge),
                SovereignTextureConfig::Stucco(_) => render_x = render_x.max(edge),
                _ => {}
            }
        });
        assert!(
            brick_x > 0.0 && render_x > 0.0,
            "did not find both side walls"
        );
        assert!(
            brick_x > render_x + 0.02,
            "the base course ends at {brick_x} against render at {render_x} — \
             coplanar, and it will z-fight the full height of every elevation"
        );
    }

    /// Everything on the forecourt is *on* the forecourt.
    ///
    /// The car and the bay lines used to be positioned off the building, and
    /// overhung the tarmac by a metre — a car standing on nothing, which only
    /// shows in the one contact-sheet tile that happens to look along the front
    /// edge. Both are now derived from the pad's own extent; this pins it.
    #[test]
    fn nothing_on_the_forecourt_overhangs_it() {
        let root = MiniMart.build("");
        let GeneratorKind::Cuboid { size, .. } = &root.kind else {
            panic!("the forecourt pad is the root")
        };
        let pad = size.0;
        let pad_z = root.transform.translation.0[2];
        let (z0, z1) = (pad_z - pad[2] * 0.5, pad_z + pad[2] * 0.5);
        let (x0, x1) = (-pad[0] * 0.5, pad[0] * 0.5);
        let mut checked = 0;
        walk(&root, [0.0; 3], &mut |g, pos| {
            // Only the things standing on the tarmac in front of the shop.
            let half = match &g.kind {
                GeneratorKind::Cuboid { size, .. } => [size.0[0] * 0.5, size.0[2] * 0.5],
                GeneratorKind::Cylinder { radius, .. } => [radius.0, radius.0],
                _ => return,
            };
            if pos[2] > FRONT - 0.5 || pos[1] > BASE_H + 2.0 {
                return;
            }
            assert!(
                pos[2] - half[1] > z0 - 1e-3
                    && pos[2] + half[1] < z1 + 1e-3
                    && pos[0] - half[0] > x0 - 1e-3
                    && pos[0] + half[0] < x1 + 1e-3,
                "a forecourt part at {pos:?} (half-extent {half:?}) hangs off the \
                 pad, which spans x {x0}..{x1}, z {z0}..{z1}"
            );
            checked += 1;
        });
        assert!(
            checked >= 10,
            "only {checked} forecourt parts found — the bays or the car went missing"
        );
    }

    /// LESSON 3, the editability contract: the sub-assemblies a gizmo drag has
    /// to move as one.
    #[test]
    fn the_tree_stands_the_way_the_shop_does() {
        fn size(g: &Generator) -> usize {
            1 + g.children.iter().map(size).sum::<usize>()
        }
        let root = MiniMart.build("");
        // Forecourt's own children: the shop, the pylon, three bay lines and a
        // seven-prim car.
        assert_eq!(root.children.len(), 12, "forecourt children");
        let shop = &root.children[0];
        let pylon = &root.children[1];
        assert!(
            size(shop) > 45,
            "the shop subtree lost its walls or its fit-out"
        );
        // The pylon is one chain: footing → mast → sign head → lit cells.
        assert_eq!(pylon.children.len(), 1, "footing → mast");
        assert_eq!(pylon.children[0].children.len(), 1, "mast → sign head");
        assert_eq!(size(pylon), 5, "footing, mast, housing and two lit cells");
        // The parapet carries the roof deck and the AC unit.
        let parapet = shop
            .children
            .iter()
            .find(|c| size(c) == 4)
            .expect("parapet → deck + AC body + AC fan");
        assert_eq!(parapet.children.len(), 3);
    }
}
