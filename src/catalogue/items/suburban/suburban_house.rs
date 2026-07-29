//! Suburban house — a Suburban secondary, and the building the neighbourhood
//! is made of: a two-storey family home in lap siding under a ridged shingle
//! roof, with an attached gable-end garage, a covered porch, a brick flue and
//! a car on the drive.
//!
//! It is built as a **shell**, not as a block with pictures on it. The three
//! standing lessons of #972 all land on this one entry:
//!
//! 1. **The glazing fills real holes.** The front elevation is assembled from
//!    the siding that *frames* three bays of openings — four piers, two sill
//!    walls, three spandrels and a head band — and each opening is filled by a
//!    [`window_card`] on a flat quad, set back in the reveal, with a dim lit
//!    fit-out behind it. Before the overhaul the windows were solid glass
//!    slabs pinned to the outside of a solid body, so the generator's
//!    alpha-masked panes cut holes onto the siding they were stuck to.
//! 2. **The tiling materials are laid the way the real thing is.** The siding
//!    runs as unbroken courses through pier, spandrel and band as if the wall
//!    were clad in one pass ([`util::bonded_siding`]) — it used to carry the
//!    generator's hard-coded three-butt-joints-per-tile grid, which turned
//!    lap siding into coarse masonry, and every slab restarted its own
//!    courses at its own centre. The chimney's brick lies flat at a real
//!    215 mm ([`util::bonded_brick`]) instead of standing every brick on end.
//! 3. **It stands the way a house stands.** Plinth → floor deck → walls →
//!    upper storey → wall plate → roof → flue, with the garage wing and the
//!    porch as their own subtrees, so one gizmo drag moves a whole
//!    sub-assembly instead of stranding the parts it carried.
//!
//! Deliberately silent: the kit's [`fx::birdsong`](super::fx::birdsong) and
//! sprinkler mist live on the community center and the gateway, which are one
//! per settlement. This is a *secondary* — a street holds several — so an
//! emitter here would stack into a chorus.
//!
//! [`util::bonded_siding`]: crate::catalogue::items::util::bonded_siding
//! [`util::bonded_brick`]: crate::catalogue::items::util::bonded_brick

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, footing, glow, id_quat, lit_interior, nest, plane,
    prim, quat_x, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, CAR_SILVER, HEDGE_GREEN, ROOF_GREY, SIDING_CREAM, WOOD_WHITE, brick, concrete,
    enamel, parked_car, shingle, siding, wood,
};

// --- Shell dimensions. Everything below is derived from these. -------------

/// House body width (X) and depth (Z), and the wall height above the plinth.
const W: f32 = 10.0;
const D: f32 = 8.0;
const BODY_H: f32 = 6.0;
/// Plinth height — the floor level, and the datum every storey is measured
/// from.
const BASE_H: f32 = 0.4;
/// Wall thickness, and so the depth of every window reveal.
const WALL_T: f32 = 0.3;

/// Outer face of the front wall. The porch, door, windows and porch light all
/// look down `-Z` — the render tool's and the settlement placer's hero
/// direction.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane: set back inside the reveal so the wall's thickness reads as
/// thickness rather than as a sticker.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;
/// Centre plane of the proud trim boards. Deep enough that their back faces
/// end up *inside* the wall rather than coplanar with its outer face, which
/// would z-fight.
const TRIM_Z: f32 = FRONT - 0.03;

/// Mid-floor level, above the plinth top — the storey line the band board
/// marks.
const STOREY: f32 = 2.9;
/// Wall-plate level: the top of the piers, and where the roof lands.
const PLATE: f32 = 5.3;

/// Every opening on the hero face is this wide, in three bays.
const OPEN_W: f32 = 1.4;
/// Bay centres in X — left, entrance, right.
const BAY_X: [f32; 3] = [-2.6, 0.0, 2.6];
/// Ground-storey window sill and head, above the plinth top.
const G_SILL: f32 = 1.3;
const G_HEAD: f32 = 2.7;
/// Upper-storey window sill and head.
const U_SILL: f32 = 3.9;
const U_HEAD: f32 = 5.3;
/// Head of the entrance opening — the middle bay runs to the floor.
const DOOR_H: f32 = 2.2;

// --- The garage wing. ------------------------------------------------------

/// Garage width, depth and wall height. The width starts *inside* the house's
/// side wall so the two never share a face plane.
const G_W: f32 = 5.1;
const G_D: f32 = 6.2;
const G_H: f32 = 3.2;
/// Garage centre in X — its `-X` face lands 0.1 inside the house wall.
const G_X: f32 = W * 0.5 - 0.1 + G_W * 0.5;
/// Garage centre in Z. Set back from the house front so the street reads two
/// masses rather than one flat wall.
const G_Z: f32 = -0.4;
/// Garage slab top. Held below the house floor — a real garage floor steps
/// down, and it keeps the two plinths from sharing a horizontal plane where
/// they overlap.
const G_BASE: f32 = 0.34;
/// Outer face of the garage front.
const G_FRONT: f32 = G_Z - G_D * 0.5;

// --- Palette local to this entry. ------------------------------------------

/// Brick length in metres for the flue — a real 215 mm brick.
const BRICK_LEN: f32 = 0.215;
/// Front door paint. The one saturated colour on the elevation, and the thing
/// that tells two otherwise-identical houses apart.
const DOOR_PAINT: [f32; 3] = [0.40, 0.13, 0.12];
/// Porch lamp. Deep-saturated amber rather than the kit's pale
/// [`PORCH_WARM`](super::PORCH_WARM): a small lens at low strength then reads
/// as a warm *colour* under bloom instead of washing to a white blank.
const LAMP_AMBER: [f32; 3] = [1.0, 0.58, 0.20];
/// Window joinery — painted white, as the trim is.
const JOINERY: [f32; 3] = WOOD_WHITE;
/// Domestic concrete: plinth, drive, steps, chimney cap.
const SLAB_GREY: [f32; 3] = [0.60, 0.59, 0.57];

/// Lap siding laid in the wall's own frame — see [`util::bonded_siding`].
/// `face` names the face whose courses this material lines up; with the
/// stagger off there are no U features left, so every side face agrees on
/// `V = -y` and no elevation needs a per-face override.
fn lap(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_siding(siding(SIDING_CREAM), face, center)
}

/// One siding slab of the shell, positioned once so the placement and the UV
/// frame cannot drift apart.
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, lap(center, face))),
        center,
        id_quat(),
    )
}

/// A proud painted board — sill, band, frieze, fascia, door casing. Trim is
/// always oversized against what it laps and always stands off the surface it
/// laps, so it never shares a plane with its host.
fn trim(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, wood(WOOD_WHITE))),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge.
///
/// The coplanar rule, applied to a card: sized to the opening *exactly*, each
/// edge of the quad lands on the reveal's own plane, and a flush edge is a tie
/// the rasteriser has to break — the failure mode is a hairline of whatever
/// stands behind, running down the reveal. Lapping costs nothing to hide,
/// because the frame is opaque and the pier's outer face is nearer the camera
/// than the recessed card, so the overhang is never seen.
const GLAZE_LAP: f32 = 0.06;

/// Clear glazing filling one bay, on a flat quad at [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3]) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            window_card(JOINERY, 2, 2, 0.34, 0.1),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit blind just inside an upper window.
///
/// Depth discipline again, from the other direction: the bedroom lining is
/// seven metres back, so from the pavement the camera looks *up* through the
/// opening and the top panes frame the dim ceiling — the black-rectangle
/// failure the card idiom is meant to avoid, arrived at by way of a room that
/// is simply too deep. A pale blind held a third of a metre behind the glass
/// gives every pane something warm to show, which is also what a house looks
/// like at dusk.
fn blind(center: [f32; 3]) -> Generator {
    prim(
        cuboid_tapered(
            [OPEN_W - 0.1, OPEN_W - 0.1, 0.06],
            0.0,
            lit_interior([0.62, 0.56, 0.46], 0.45),
        ),
        center,
        id_quat(),
    )
}

pub struct SuburbanHouse;

impl CatalogueEntry for SuburbanHouse {
    fn slug(&self) -> &'static str {
        "suburban_house"
    }
    fn name(&self) -> &'static str {
        "Suburban House"
    }
    fn description(&self) -> &'static str {
        "Two-storey sided family house with an attached garage and a lit porch."
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
            clearance: 7.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The house as a tree that stands the way the house does (#970): the plinth
/// at the bottom, and above it three sub-assemblies that each move as one —
/// the shell (with the roof on its wall plate and the flue on the roof), the
/// garage wing (with its drive and the car on it), and the porch.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let plinth = prim(
        solid(cuboid_tapered(
            [W + 0.5, BASE_H, D + 0.5],
            0.0,
            concrete(SLAB_GREY),
        )),
        [0.0, BASE_H * 0.5, 0.0],
        id_quat(),
    );

    let mut yard = crop_tufts(
        [-3.0, 0.0, FRONT - 0.62],
        [3.4, 0.8],
        4,
        1,
        0.85,
        foliage(HEDGE_GREEN),
    );
    yard.push(shell());
    yard.push(garage_wing());
    yard.push(porch());
    // Buried footing under the plinth, so a terrain-snapped house on a slope
    // shows plinth rather than daylight under its downhill edge.
    yard.push(footing(W + 0.5, D + 0.5, [0.0, 0.0], 7.0));

    nest(plinth, yard)
}

// --- The shell. ------------------------------------------------------------

/// Ground floor deck, and under it everything the house is: the walls that
/// frame the openings, the glazing that fills them, the fit-out behind the
/// glass, the upper storey, and — on the wall plate — the roof.
///
/// The deck is the sub-root because it is the lowest piece of the shell and
/// everything else in the shell stands on or above it.
fn shell() -> Generator {
    let inner_w = W - WALL_T * 2.0;
    let inner_d = D - WALL_T * 2.0;
    let mut parts = Vec::new();

    // Back and side walls: a solid box open only where the hero face is cut,
    // so the glazing has an inside to look into. The side walls are shortened
    // in Z so their ends never share a plane with the front and back slabs'
    // outer faces.
    parts.push(wall(
        [W, BODY_H, WALL_T],
        [0.0, BASE_H + BODY_H * 0.5, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, BODY_H, inner_d],
            [sx * (W * 0.5 - WALL_T * 0.5), BASE_H + BODY_H * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    front_elevation(&mut parts);
    ground_fitout(&mut parts, inner_w, inner_d);

    // Ceiling under the roof void, so the upper windows do not look up into
    // the inside of the roof mass.
    parts.push(prim(
        cuboid_tapered(
            [inner_w, 0.1, inner_d],
            0.0,
            lit_interior([0.24, 0.23, 0.21], 0.1),
        ),
        [0.0, BASE_H + BODY_H - 0.2, 0.0],
        id_quat(),
    ));

    parts.push(upper_storey(inner_w, inner_d));
    parts.push(wall_plate());

    // The porch light is mounted on the wall beside the door, not on the
    // porch: a small housing with a smaller lit lens, because a broad panel
    // at strength blooms to white.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.22, 0.3, 0.13],
            0.0,
            enamel([0.24, 0.23, 0.22]),
        )),
        [1.05, BASE_H + 2.05, FRONT - 0.065],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.13, 0.18, 0.06], 0.0, glow(LAMP_AMBER, 2.0)),
        [1.05, BASE_H + 2.05, FRONT - 0.13],
        id_quat(),
    ));

    let deck = prim(
        cuboid_tapered(
            [inner_w, 0.1, inner_d],
            0.0,
            lit_interior([0.26, 0.22, 0.19], 0.12),
        ),
        [0.0, BASE_H + 0.06, 0.0],
        id_quat(),
    );
    nest(deck, parts)
}

/// The hero face, built as the siding that *frames* three bays of openings —
/// four piers, two sill walls under the ground windows, three spandrels
/// between the storeys, and the head band that carries the wall up to the
/// plate — plus the glazing and the front door filling what is left.
///
/// Every piece is coplanar with every other at [`FRONT_MID`] and shares one
/// course frame, so the courses run through pier, sill and spandrel as if the
/// elevation had been clad in one pass. Nothing overlaps: the piers stop at
/// the plate and the head band spans the full width above them, rather than
/// the two crossing and z-fighting where they share the wall plane.
fn front_elevation(parts: &mut Vec<Generator>) {
    // Bay edges left to right: wall end, then each opening's two sides, then
    // the far wall end. The piers are the odd gaps between them.
    let e = [
        -W * 0.5,
        BAY_X[0] - OPEN_W * 0.5,
        BAY_X[0] + OPEN_W * 0.5,
        BAY_X[1] - OPEN_W * 0.5,
        BAY_X[1] + OPEN_W * 0.5,
        BAY_X[2] - OPEN_W * 0.5,
        BAY_X[2] + OPEN_W * 0.5,
        W * 0.5,
    ];
    for (a, b) in [(e[0], e[1]), (e[2], e[3]), (e[4], e[5]), (e[6], e[7])] {
        parts.push(wall(
            [b - a, PLATE, WALL_T],
            [(a + b) * 0.5, BASE_H + PLATE * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Sill walls under the two ground-storey windows. The entrance bay has
    // none, so the door reaches the floor.
    for &x in [BAY_X[0], BAY_X[2]].iter() {
        parts.push(wall(
            [OPEN_W, G_SILL, WALL_T],
            [x, BASE_H + G_SILL * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Spandrels between the storeys — the entrance bay's reaches lower, down
    // to the door head.
    for (x, low) in [(BAY_X[0], G_HEAD), (BAY_X[1], DOOR_H), (BAY_X[2], G_HEAD)] {
        parts.push(wall(
            [OPEN_W, U_SILL - low, WALL_T],
            [x, BASE_H + (low + U_SILL) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Glazing: two ground bays and all three upper bays, each card filling
    // its opening exactly. 1.4 m square openings take two panes each way.
    for &x in [BAY_X[0], BAY_X[2]].iter() {
        parts.push(glazing(
            [OPEN_W, G_HEAD - G_SILL],
            [x, BASE_H + (G_SILL + G_HEAD) * 0.5, GLAZE_Z],
        ));
    }
    for &x in BAY_X.iter() {
        parts.push(glazing(
            [OPEN_W, U_HEAD - U_SILL],
            [x, BASE_H + (U_SILL + U_HEAD) * 0.5, GLAZE_Z],
        ));
    }

    front_door(parts);
    front_trim(parts);
}

/// The entrance: a painted leaf set back in the reveal, cased by two jambs and
/// a head board standing proud of the siding, with a handle proud of the leaf.
/// The leaf is held a hair narrower than the opening so its edges never share
/// a plane with the piers' reveals; the casing covers the resulting slot.
fn front_door(parts: &mut Vec<Generator>) {
    let leaf_z = FRONT + WALL_T * 0.55;
    parts.push(prim(
        solid(cuboid_tapered(
            [OPEN_W - 0.06, DOOR_H - 0.04, 0.08],
            0.0,
            enamel(DOOR_PAINT),
        )),
        [BAY_X[1], BASE_H + (DOOR_H - 0.04) * 0.5, leaf_z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.06, 0.18, 0.06], 0.0, enamel([0.72, 0.66, 0.42])),
        [BAY_X[1] + 0.48, BASE_H + 1.05, leaf_z - 0.09],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim(
            [0.15, DOOR_H + 0.22, 0.1],
            [
                BAY_X[1] + sx * (OPEN_W * 0.5 + 0.075),
                BASE_H + (DOOR_H + 0.22) * 0.5,
                TRIM_Z,
            ],
        ));
    }
    parts.push(trim(
        [OPEN_W + 0.44, 0.16, 0.1],
        [BAY_X[1], BASE_H + DOOR_H + 0.13, TRIM_Z],
    ));
}

/// Horizontal articulation. The two rings — the storey band at the floor line
/// and the frieze under the eaves — wrap all four elevations as single prims,
/// which reads better than four boards per side and costs less; the sills are
/// local to the bays they serve.
fn front_trim(parts: &mut Vec<Generator>) {
    for &x in [BAY_X[0], BAY_X[2]].iter() {
        parts.push(trim([OPEN_W + 0.3, 0.1, 0.1], [x, BASE_H + G_SILL, TRIM_Z]));
    }
    parts.push(trim(
        [(BAY_X[2] - BAY_X[0]) + OPEN_W + 0.3, 0.1, 0.1],
        [BAY_X[1], BASE_H + U_SILL, TRIM_Z],
    ));
    // Band board at the storey line and frieze under the eaves, both proud of
    // the walls on every side.
    parts.push(trim(
        [W + 0.16, 0.14, D + 0.16],
        [0.0, BASE_H + STOREY, 0.0],
    ));
    parts.push(trim(
        [W + 0.2, 0.16, D + 0.2],
        [0.0, BASE_H + U_HEAD + 0.4, 0.0],
    ));
}

/// What the passer-by sees through the ground-floor glass: a dim lit lining
/// and a couple of pieces of furniture held close to the window.
///
/// Depth discipline matters more than quantity. Goods against the back wall of
/// an 8 m room sit six metres behind the glass and shrink to unreadable
/// specks, so the furniture is parked two metres in — close enough that a pane
/// frames a recognisable object.
fn ground_fitout(parts: &mut Vec<Generator>, inner_w: f32, inner_d: f32) {
    parts.push(prim(
        cuboid_tapered(
            [inner_w, STOREY - 0.3, 0.08],
            0.0,
            lit_interior([0.22, 0.20, 0.18], 0.1),
        ),
        [0.0, BASE_H + STOREY * 0.5, D * 0.5 - WALL_T - 0.06],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered(
            [inner_w * 0.45, 0.1, 0.5],
            0.0,
            glow([1.0, 0.82, 0.52], 1.6),
        ),
        [0.0, BASE_H + STOREY - 0.32, -0.6],
        id_quat(),
    ));
    // Lit soffit just inside the window heads. From the pavement the top row
    // of panes looks *up*, straight at the underside of the floor above; left
    // bare that is the darkest surface in the shell and the panes read black,
    // which is the one thing a glazing card must never do.
    parts.push(prim(
        cuboid_tapered(
            [inner_w * 0.8, 0.08, 1.6],
            0.0,
            lit_interior([0.52, 0.47, 0.40], 0.38),
        ),
        [0.0, BASE_H + STOREY - 0.2, FRONT + 1.2],
        id_quat(),
    ));
    // A sofa under the left window and a lamp table under the right.
    parts.push(prim(
        cuboid_tapered(
            [2.3, 0.78, 0.85],
            0.06,
            lit_interior([0.34, 0.27, 0.22], 0.3),
        ),
        [BAY_X[0], BASE_H + 0.5, FRONT + 1.9],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered(
            [0.7, 0.62, 0.7],
            0.0,
            lit_interior([0.30, 0.24, 0.20], 0.28),
        ),
        [BAY_X[2], BASE_H + 0.42, FRONT + 1.8],
        id_quat(),
    ));
    let _ = inner_d;
}

/// The upper storey, hung off its own floor deck so a drag on it takes the
/// bedroom lining and its glow with it.
fn upper_storey(inner_w: f32, inner_d: f32) -> Generator {
    let deck = prim(
        cuboid_tapered(
            [inner_w, 0.12, inner_d],
            0.0,
            lit_interior([0.26, 0.22, 0.19], 0.12),
        ),
        [0.0, BASE_H + STOREY, 0.0],
        id_quat(),
    );
    nest(
        deck,
        vec![
            prim(
                cuboid_tapered(
                    [inner_w, BODY_H - STOREY - 0.5, 0.08],
                    0.0,
                    lit_interior([0.22, 0.21, 0.20], 0.09),
                ),
                [
                    0.0,
                    BASE_H + (STOREY + BODY_H) * 0.5,
                    D * 0.5 - WALL_T - 0.06,
                ],
                id_quat(),
            ),
            prim(
                cuboid_tapered(
                    [inner_w * 0.35, 0.1, 0.45],
                    0.0,
                    glow([1.0, 0.84, 0.58], 1.3),
                ),
                [0.0, BASE_H + BODY_H - 0.45, -0.8],
                id_quat(),
            ),
            prim(
                cuboid_tapered(
                    [2.0, 0.55, 1.5],
                    0.05,
                    lit_interior([0.30, 0.28, 0.30], 0.24),
                ),
                [BAY_X[0], BASE_H + STOREY + 0.4, FRONT + 1.9],
                id_quat(),
            ),
            blind([BAY_X[0], BASE_H + (U_SILL + U_HEAD) * 0.5, GLAZE_Z + 0.33]),
            blind([BAY_X[1], BASE_H + (U_SILL + U_HEAD) * 0.5, GLAZE_Z + 0.33]),
            blind([BAY_X[2], BASE_H + (U_SILL + U_HEAD) * 0.5, GLAZE_Z + 0.33]),
        ],
    )
}

/// The head band closing the wall above the upper windows — and, standing on
/// it, the roof. Literally the wall plate: the piece the roof lands on, which
/// is why the roof is its child.
fn wall_plate() -> Generator {
    let h = BODY_H - PLATE;
    let plate = wall(
        [W, h, WALL_T],
        [0.0, BASE_H + PLATE + h * 0.5, FRONT_MID],
        FaceKey::SideNz,
    );
    nest(plate, vec![roof()])
}

/// A ridged hip roof, its eaves fascia, and the flue that rises through it.
///
/// `taper_xz` is what makes it a *ridge*: X barely pinches while Z pinches
/// almost to a line, so the top collapses to a long line along the house's
/// length instead of the square plateau a uniform taper leaves. The roof sits
/// a few centimetres into the walls, because a base face flush with the wall
/// tops would be two coplanar horizontal faces fighting over the same plane.
fn roof() -> Generator {
    let base_y = BASE_H + BODY_H - 0.06;
    let h = 2.4;
    let deck = prim(
        solid(cuboid_tapered_xz(
            [W + 1.4, h, D + 1.4],
            [0.1, 0.92],
            shingle(ROOF_GREY),
        )),
        [0.0, base_y + h * 0.5, 0.0],
        id_quat(),
    );
    nest(
        deck,
        vec![
            // Fascia board under the eave, proud of the roof's base outline
            // and hanging below it — the shadow line that stops the roof
            // reading as a lid dropped on a box.
            trim([W + 1.5, 0.2, D + 1.5], [0.0, base_y - 0.04, 0.0]),
            flue(base_y + h),
        ],
    )
}

/// Brick flue, cap and pot, rising from the roof slope near the ridge.
///
/// It starts inside the roof mass and clears the ridge by better than half a
/// metre. The version this replaces topped out at 7.9 against a ridge at 9.0 —
/// buried in its own roof, with a nub poking through where the taper let it.
fn flue(ridge_y: f32) -> Generator {
    let top = ridge_y + 0.85;
    let base = BASE_H + BODY_H - 0.4;
    let h = top - base;
    let at = [-3.0, base + h * 0.5, 1.6];
    let stack = prim(
        solid(cuboid_tapered(
            [0.95, h, 0.95],
            0.0,
            util::bonded_brick(brick(BRICK_TAN), BRICK_LEN, FaceKey::SideNz, at),
        )),
        at,
        id_quat(),
    );
    nest(
        stack,
        vec![
            prim(
                solid(cuboid_tapered([1.16, 0.14, 1.16], 0.0, concrete(SLAB_GREY))),
                [at[0], top + 0.07, at[2]],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered(
                    [0.32, 0.24, 0.32],
                    0.0,
                    enamel([0.16, 0.15, 0.15]),
                )),
                [at[0], top + 0.26, at[2]],
                id_quat(),
            ),
        ],
    )
}

// --- The garage wing. ------------------------------------------------------

/// Garage slab, and on it the garage, its gable roof, the drive and the car.
///
/// The slab tucks under the house plinth rather than butting it, and sits low
/// enough that neither its top nor its inner side is ever a face fighting the
/// plinth's for the same plane.
fn garage_wing() -> Generator {
    let slab = prim(
        solid(cuboid_tapered(
            [G_W + 0.5, G_BASE, G_D + 0.5],
            0.0,
            concrete(SLAB_GREY),
        )),
        [G_X, G_BASE * 0.5, G_Z],
        id_quat(),
    );
    nest(slab, vec![garage(), drive()])
}

/// The garage body, its panelled door and its gable-end roof.
///
/// The roof's ridge runs along **Z**, so the wing presents a gable to the
/// street and the elevation reads as two masses meeting rather than one long
/// wall — the flat-topped frustum it replaces read as neither.
fn garage() -> Generator {
    let top = G_BASE + G_H;
    let body = wall(
        [G_W, G_H, G_D],
        [G_X, G_BASE + G_H * 0.5, G_Z],
        FaceKey::SideNz,
    );

    let door_h = G_H - 0.7;
    let door_z = G_FRONT + 0.1;
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [G_W - 1.2, door_h, 0.12],
                0.0,
                enamel([0.86, 0.85, 0.82]),
            )),
            [G_X, G_BASE + door_h * 0.5, door_z],
            id_quat(),
        ),
        trim(
            [G_W - 0.8, 0.14, 0.1],
            [G_X, G_BASE + door_h + 0.09, G_FRONT - 0.05],
        ),
    ];
    // Pressed rails across the door, so it is a door rather than a blank
    // sheet of enamel. They stay inside the reveal.
    for i in 0..3 {
        parts.push(prim(
            cuboid_tapered([G_W - 1.24, 0.09, 0.05], 0.0, enamel([0.78, 0.77, 0.74])),
            [
                G_X,
                G_BASE + door_h * (0.25 + 0.25 * i as f32),
                door_z - 0.075,
            ],
            id_quat(),
        ));
    }

    let roof_h = 1.5;
    let roof_base = top - 0.06;
    parts.push(nest(
        prim(
            solid(cuboid_tapered_xz(
                [G_W + 0.8, roof_h, G_D + 0.8],
                [0.9, 0.08],
                shingle(ROOF_GREY),
            )),
            [G_X, roof_base + roof_h * 0.5, G_Z],
            id_quat(),
        ),
        vec![trim(
            [G_W + 0.9, 0.18, G_D + 0.9],
            [G_X, roof_base - 0.04, G_Z],
        )],
    ));

    nest(body, parts)
}

/// The drive running out from the garage door to the street, and the car
/// standing on it.
///
/// The car used to be parked four metres past the end of the apron, floating
/// over the lawn; here the slab is authored from the garage face outward and
/// the car is placed on it, so the two cannot disagree.
fn drive() -> Generator {
    let z0 = G_FRONT;
    let z1 = G_FRONT - 6.0;
    let top = 0.1;
    let slab = prim(
        solid(cuboid_tapered(
            [G_W + 0.6, 0.2, z0 - z1],
            0.0,
            concrete([0.56, 0.55, 0.54]),
        )),
        [G_X, top - 0.1, (z0 + z1) * 0.5],
        id_quat(),
    );
    nest(slab, parked_car([G_X, top, z1 + 3.1], CAR_SILVER))
}

// --- The porch. ------------------------------------------------------------

/// Two steps up to a covered entrance: square posts on the upper step, a head
/// beam across them, and a pitched roof dying into the wall above the door.
///
/// The lower step is the sub-root — the piece on the ground — so a drag on the
/// porch takes the whole thing. The roof slab is tilted, and it is a leaf:
/// a tilted node spins everything under it, which is the point on a roof and a
/// bug on anything that carries.
fn porch() -> Generator {
    let step2_top = BASE_H - 0.02;
    let post_h = 2.9;
    let beam_y = step2_top + post_h + 0.11;

    let lower = prim(
        solid(cuboid_tapered([3.2, 0.19, 0.6], 0.0, concrete(SLAB_GREY))),
        [0.0, 0.095, FRONT - 1.15],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.6_f32, 1.6] {
        parts.push(prim(
            solid(cuboid_tapered([0.17, post_h, 0.17], 0.0, wood(WOOD_WHITE))),
            [sx, step2_top + post_h * 0.5, FRONT - 0.6],
            id_quat(),
        ));
        parts.push(trim(
            [0.25, 0.1, 0.25],
            [sx, step2_top + post_h - 0.05, FRONT - 0.6],
        ));
    }
    parts.push(trim([3.6, 0.22, 0.22], [0.0, beam_y, FRONT - 0.6]));
    // Pitched roof: it slopes down away from the wall, and its high edge is
    // pushed into the siding so the two never share a face plane.
    parts.push(prim(
        solid(cuboid_tapered([4.0, 0.24, 1.6], 0.0, shingle(ROOF_GREY))),
        [0.0, beam_y + 0.37, FRONT - 0.83],
        quat_x(-0.2),
    ));

    nest(
        lower,
        vec![nest(
            prim(
                solid(cuboid_tapered(
                    [2.9, step2_top, 0.63],
                    0.0,
                    concrete(SLAB_GREY),
                )),
                [0.0, step2_top * 0.5, FRONT - 0.535],
                id_quat(),
            ),
            parts,
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SuburbanHouse.build(""), "suburban_house");
    }

    /// Walk the tree, summing translations, and hand every node to `f` with
    /// its world position — which is what the record spawns, since a child's
    /// transform is relative to its parent's.
    fn walk(g: &Generator, at: [f32; 3], f: &mut impl FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// LESSON 1: every glazing card lives on a flat quad at `uv_scale` 1.0.
    /// The cards upload clamp-to-edge, so anything else smears their edge
    /// texels across the surface; and the exact count fails loudly if one is
    /// ever moved off a quad onto a solid, which is what the five windows
    /// were before the overhaul.
    #[test]
    fn glazing_cards_are_unscaled_quads() {
        let mut planes = 0;
        walk(&SuburbanHouse.build(""), [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                assert_eq!(
                    material.uv_scale.0, 1.0,
                    "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                );
                // And every card oversails its opening, so no edge of it ever
                // lands on the reveal's own plane — the coplanar rule applied
                // to a quad, see [`GLAZE_LAP`].
                assert!(
                    size.0[0] > OPEN_W && size.0[1] > OPEN_W,
                    "card {:?} does not lap its {OPEN_W} m opening",
                    size.0
                );
                planes += 1;
            }
            // And no solid may carry one: a card on a cuboid grows windows on
            // all six faces and punches holes onto whatever is inside.
            if let GeneratorKind::Cuboid { material, .. } = &g.kind {
                assert!(
                    !matches!(material.texture, SovereignTextureConfig::Window(_)),
                    "a Window card on a solid is a frame over nothing"
                );
            }
        });
        assert_eq!(planes, 5, "expected two ground and three upper bays glazed");
    }

    /// LESSON 1, second half: a card only reads if there is something lit
    /// behind it. Assert the fit-out exists, and that it sits *close* to the
    /// glass rather than against the back wall six metres away, where a pane
    /// frames an unreadable speck.
    #[test]
    fn the_fitout_stands_close_behind_the_glass() {
        let mut near = 0;
        walk(&SuburbanHouse.build(""), [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { material, .. } = &g.kind
                && material.emission_strength.0 > 0.15
                && material.emission_strength.0 < 1.0
                && pos[2] < FRONT + 2.5
            {
                near += 1;
            }
        });
        assert!(
            near >= 3,
            "only {near} lit pieces sit within 2.5 m of the glazing — the \
             openings will read as black rectangles"
        );
    }

    /// Collect every siding surface as `(world position, material)`.
    fn siding_surfaces(root: &Generator) -> Vec<([f32; 3], SovereignMaterialSettings)> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { material, .. } = &g.kind
                && let SovereignTextureConfig::Plank(cfg) = &material.texture
                && cfg.plank_count.0 == super::super::SIDING_COURSES
            {
                out.push((pos, material.clone()));
            }
        });
        out
    }

    /// LESSON 2: the elevation is clad in one pass.
    ///
    /// Box projection is prim-local and centred on each prim's own bounds, so
    /// without an offset the fourteen slabs of this shell each restart their
    /// courses at their own centre and every joint reads as a break in the
    /// siding. The offset that puts a face in the shared world frame is that
    /// face's own projection of the slab's position — subtle enough in a
    /// render that only this catches it.
    #[test]
    fn every_siding_surface_sits_in_the_world_course_frame() {
        let surfaces = siding_surfaces(&SuburbanHouse.build(""));
        assert_eq!(
            surfaces.len(),
            14,
            "expected 3 shell walls + 4 piers + 2 sill walls + 3 spandrels + \
             the wall plate + the garage body, all in siding"
        );
        const FACES: [FaceKey; 6] = [
            FaceKey::SideNz,
            FaceKey::SidePz,
            FaceKey::SidePx,
            FaceKey::SideNx,
            FaceKey::Top,
            FaceKey::Bottom,
        ];
        for (pos, m) in &surfaces {
            assert!(
                FACES.iter().any(|&f| {
                    let e = util::face_uv_offset(f, *pos).0;
                    (e[0] - m.uv_offset.0[0]).abs() < 1e-3 && (e[1] - m.uv_offset.0[1]).abs() < 1e-3
                }),
                "siding slab at {pos:?} carries offset {:?}, which is no face's frame",
                m.uv_offset.0
            );
        }
    }

    /// LESSON 2: the courses run unbroken.
    ///
    /// `PlankConfig::stagger` above 0.01 switches on a hard-coded grid of
    /// three butt joints per tile across U. At this tile that is a 557 mm
    /// joint every third of a metre, and the wall stops reading as board and
    /// starts reading as coarse masonry — which is exactly how this house
    /// rendered before. Pinned here because the config makes it look like a
    /// harmless de-correlation knob.
    #[test]
    fn siding_courses_run_unbroken() {
        for (pos, m) in siding_surfaces(&SuburbanHouse.build("")) {
            let SovereignTextureConfig::Plank(cfg) = &m.texture else {
                unreachable!("filtered to Plank above");
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "siding slab at {pos:?} carries end joints — three per tile, \
                 hard-coded, which reads as brick not board"
            );
            // The band grid only tiles in V if the course count is whole.
            assert_eq!(
                cfg.plank_count.0,
                cfg.plank_count.0.round(),
                "siding slab at {pos:?}: a fractional course count breaks the \
                 tile's V seam"
            );
        }
    }

    /// LESSON 2, brick half: the flue's courses lie flat and its bond tiles.
    /// The generator derives columns as `scale × aspect_ratio` while `scale`
    /// *is* the row count, so under the metre-square UV tile the kit's aspect
    /// of 2.0 stood every brick on end; and `5 × 0.5` is not a whole number,
    /// so the bond broke every fifth course.
    #[test]
    fn the_flue_bricks_lie_flat() {
        let mut seen = 0;
        walk(&SuburbanHouse.build(""), [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { material, .. } = &g.kind
                && let SovereignTextureConfig::Brick(cfg) = &material.texture
            {
                let cols = (cfg.scale.0 * cfg.aspect_ratio.0).round();
                assert!(
                    cols < cfg.scale.0,
                    "flue at {pos:?}: {cols} columns to {} rows stands its bricks upright",
                    cfg.scale.0
                );
                assert!(
                    cols >= 4.0,
                    "flue at {pos:?}: {cols} bricks per tile leaves the \
                     seam-straddling brick too large a share of the surface"
                );
                let stagger = cfg.scale.0 * cfg.row_offset.0;
                assert!(
                    (stagger - stagger.round()).abs() < 1e-6,
                    "flue at {pos:?}: scale × row_offset = {stagger} does not tile"
                );
                assert!(
                    cfg.cell_variance.0 <= 0.15,
                    "flue at {pos:?}: cell variance {} makes the tile seam read \
                     as two different bricks",
                    cfg.cell_variance.0
                );
                seen += 1;
            }
        });
        assert_eq!(seen, 1, "expected exactly the chimney stack in brick");
    }

    /// LESSON 3, the editability contract: the sub-assemblies a gizmo drag is
    /// meant to move as one. It is what silently breaks when a later part is
    /// added at the wrong level, and no render shows it.
    #[test]
    fn the_tree_stands_the_way_the_house_does() {
        let root = SuburbanHouse.build("");
        fn size(g: &Generator) -> usize {
            1 + g.children.iter().map(size).sum::<usize>()
        }
        // The plinth's own children: four hedge clumps, the shell, the garage
        // wing, the porch and the buried footing.
        assert_eq!(root.children.len(), 8, "plinth children");

        let shell = &root.children[4];
        let garage = &root.children[5];
        let porch = &root.children[6];

        // The roof hangs off the wall plate, and the flue off the roof, so a
        // drag on the plate carries both.
        let plate = shell
            .children
            .iter()
            .find(|c| size(c) == 6)
            .expect("wall plate → roof → [fascia, flue → [cap, pot]]");
        assert_eq!(size(&plate.children[0]), 5, "roof → fascia + flue subtree");

        // The garage slab carries the garage (with its door and roof) and the
        // drive (with the car on it).
        assert_eq!(garage.children.len(), 2, "garage slab → body, drive");
        assert_eq!(size(&garage.children[1]), 8, "drive → seven-prim car");

        // The porch is one chain: lower step → upper step → everything.
        assert_eq!(porch.children.len(), 1, "porch steps nest");
        assert_eq!(size(porch), 8, "porch: 2 steps, 2 posts + caps, beam, roof");
    }

    /// LESSON 3's other half: the refactor was structure-only. Pin the world
    /// positions the nesting has to reproduce — a rebase that drops a parent's
    /// translation moves a whole sub-assembly, and the tree still looks
    /// perfectly well-formed afterwards.
    #[test]
    fn nesting_reproduces_the_authored_world_frame() {
        let mut hits = 0;
        walk(&SuburbanHouse.build(""), [0.0; 3], &mut |g, pos| {
            let approx = |p: [f32; 3], q: [f32; 3]| {
                (p[0] - q[0]).abs() < 1e-4
                    && (p[1] - q[1]).abs() < 1e-4
                    && (p[2] - q[2]).abs() < 1e-4
            };
            // The wall plate, the roof deck above it, and the flue cap on top
            // of that — three levels of nesting, one per rebase.
            for want in [
                [0.0, BASE_H + PLATE + (BODY_H - PLATE) * 0.5, FRONT_MID],
                [0.0, BASE_H + BODY_H - 0.06 + 1.2, 0.0],
            ] {
                if approx(pos, want) {
                    hits += 1;
                }
            }
            // The car's near-side rear tyre, five levels down: plinth →
            // garage slab → drive → car body → tyre.
            if let GeneratorKind::Cylinder { .. } = &g.kind
                && approx(pos, [G_X - 0.95, 0.48, G_FRONT - 6.0 + 3.1 - 1.3])
            {
                hits += 1;
            }
        });
        assert_eq!(hits, 3, "a nested part drifted out of the authored frame");
    }
}
