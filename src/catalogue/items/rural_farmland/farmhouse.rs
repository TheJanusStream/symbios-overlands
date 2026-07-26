//! Farmhouse — a Rural/Farmland secondary. A two-storey clapboard house on a
//! fieldstone foundation under a ridged shingle roof, with a covered porch
//! across the whole front, a fieldstone chimney climbing the gable end, and
//! hearth smoke on the golden-hour air.
//!
//! Rebuilt as a **shell** under the standing lessons of #972, alongside the
//! barn it shares a farmyard with:
//!
//! 1. **The glazing fills real holes.** The front is the clapboard that
//!    *frames* three bays — four piers, sill walls, spandrels and a frieze —
//!    with a [`window_card`] on a flat quad set back in each reveal and a lit
//!    room behind it. The five windows used to be `Window`-textured slabs
//!    pinned to a solid body, and the generator masks its panes *away*, so
//!    each was a frame with holes onto the siding behind it.
//! 2. **The roof has a ridge.** It was a uniformly-tapered block: a hip with
//!    a flat plateau where a ridge belongs, which is what a farmhouse roof
//!    never is. Pinching Z alone gives a real ridge along the house, and the
//!    two gable ends it creates are face-overridden back to clapboard, so the
//!    siding runs from the sill up into the apex in one course frame.
//! 3. **It stands the way a house stands.** Footing → floor deck → walls,
//!    glazing and fit-out → upper storey → wall plate → roof → chimney, with
//!    the porch as its own subtree.
//!
//! The kit-wide half of this pass is in [`clapboard`]: its `stagger` is now
//! zero, so the house wears lap siding rather than the generator's hard-coded
//! three-butt-joints-per-tile grid, which read at a glance as brick.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, lit_interior, nest,
    plane, prim, quat_x, solid, window_card, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAPBOARD_CREAM, ROOF_GREY, STONE_GREY, TRIM_WHITE, clapboard, fx, shingle, stone};

// --- Shell dimensions. Everything below derives from these. ----------------

/// Body width (X) and depth (Z), and the wall height above the footing.
const W: f32 = 10.5;
const D: f32 = 8.5;
const BODY_H: f32 = 6.0;
/// Fieldstone footing — the floor level, and the datum every storey is
/// measured from.
const FOOT_H: f32 = 0.5;
/// Wall thickness, and so the depth of every window reveal.
const WALL_T: f32 = 0.3;

/// Outer face of the front wall — the `-Z` hero direction the render tool and
/// the settlement placer both look down.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane: set back inside the reveal so the wall's thickness reads as
/// thickness rather than as a sticker.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;
/// Where a room panel stands behind an opening.
const ROOM_Z: f32 = FRONT + 0.62;
/// Centre plane of the proud trim boards. Deep enough that their back faces
/// end up inside the wall rather than coplanar with its outer face.
const TRIM_Z: f32 = FRONT - 0.04;

/// Bay centres in X — left, entrance, right.
const BAY_X: [f32; 3] = [-3.1, 0.0, 3.1];
/// The centre bay is the entrance on the ground storey.
const DOOR_BAY: usize = 1;
/// Every opening on the hero face is this wide.
const OPEN_W: f32 = 1.25;
/// Ground-storey window sill and head, above the footing.
const G_SILL: f32 = 1.0;
const G_HEAD: f32 = 2.3;
/// Upper-storey window sill and head.
const U_SILL: f32 = 3.6;
const U_HEAD: f32 = 5.2;
/// Head of the entrance opening — the middle bay runs to the floor.
const DOOR_H: f32 = 2.35;
/// The storey line the belt course marks.
const STOREY: f32 = 3.05;

// --- The roof. -------------------------------------------------------------

/// Rise from the wall plate to the ridge, and the eaves overhang along the
/// front and back.
const ROOF_RISE: f32 = 2.8;
const EAVE_OVER: f32 = 0.62;
/// The Z pinch that takes the roof to a ridge line. Not `1.0`: the record
/// sanitiser clamps `taper` at 0.99, and a value it rewrites fails the
/// entry's own round-trip guard rather than rendering differently.
const RIDGE_TAPER: f32 = 0.99;

// --- The porch. ------------------------------------------------------------

/// Porch depth from the house front, deck thickness, and the head height its
/// beam sits at.
const PORCH_D: f32 = 2.6;
const PORCH_BEAM: f32 = 2.55;
/// Fall across the porch roof — a shed slope, derived once so the roof, the
/// beam and the fascia all agree.
const PORCH_FALL: f32 = 0.8;

/// Where the porch roof's *back* edge meets the house.
///
/// The number that has to be checked, and the one it is natural to get
/// wrong: a shed roof pitched away from the wall is **highest** where it
/// lands on it, so raising the porch head or steepening the fall pushes this
/// up into the first-floor sills — and from the pavement the roof then reads
/// as slicing the bottom off every upper window. Derived here so the guard
/// can state the relationship rather than a magic number.
fn porch_roof_back() -> f32 {
    FOOT_H + PORCH_BEAM + 0.7
}
const PORCH_X: [f32; 4] = [-4.5, -1.5, 1.5, 4.5];

// --- Palette local to this entry. ------------------------------------------

/// Front door paint — the one saturated colour on the elevation.
const DOOR_PAINT: [f32; 3] = [0.34, 0.16, 0.12];
/// Porch lamp. Deep-saturated amber rather than the kit's paler
/// `LAMP_WARM`: a small lens at low strength reads as a warm *colour* under
/// bloom instead of washing to a white blank.
const LAMP_AMBER: [f32; 3] = [1.0, 0.60, 0.22];
/// Porch decking — bare weathered boards, not the house's paint.
const DECK_GREY: [f32; 3] = [0.52, 0.48, 0.42];

// --- Shared construction. --------------------------------------------------

/// Lap siding laid in the wall's own frame — see [`util::bonded_siding`].
/// With the stagger off there are no U features left, so every side face
/// agrees on `V = -y` and no elevation needs a per-face override.
///
/// [`util::bonded_siding`]: crate::catalogue::items::util::bonded_siding
fn lap(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_siding(clapboard(CLAPBOARD_CREAM), face, center)
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

/// A proud painted board — sill, casing, belt course, frieze, fascia, rake.
/// Trim is always oversized against what it laps and always stands off the
/// surface it laps, so it never shares a plane with its host.
fn trim(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            util::bonded_siding(clapboard(TRIM_WHITE), FaceKey::SideNz, center),
        )),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge — the coplanar
/// rule applied to a card (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Clear glazing filling one bay, on a flat quad at [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3]) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            window_card(TRIM_WHITE, 2, 3, 0.34, 0.1),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit room behind one opening — the surface a card's masked-away panes
/// actually show, and the reason the shell is worth building.
fn room(size: [f32; 2], center: [f32; 3], warm: bool) -> Generator {
    let mat = if warm {
        lit_interior([0.68, 0.56, 0.36], 0.42)
    } else {
        lit_interior([0.34, 0.31, 0.28], 0.16)
    };
    prim(
        cuboid_tapered([size[0], size[1], 0.08], 0.0, mat),
        center,
        id_quat(),
    )
}

/// Every glazed opening on the hero face: bay centre, sill, head, and whether
/// a lamp is on in that room.
///
/// One list, because the elevation, the glazing, the rooms and the guards all
/// have to agree about where the holes are.
fn openings() -> Vec<(f32, f32, f32, bool)> {
    let mut out = Vec::new();
    for (b, &x) in BAY_X.iter().enumerate() {
        if b != DOOR_BAY {
            out.push((x, G_SILL, G_HEAD, b == 0));
        }
        out.push((x, U_SILL, U_HEAD, b == DOOR_BAY));
    }
    out
}

pub struct Farmhouse;

impl CatalogueEntry for Farmhouse {
    fn slug(&self) -> &'static str {
        "farmhouse"
    }
    fn name(&self) -> &'static str {
        "Farmhouse"
    }
    fn description(&self) -> &'static str {
        "Two-storey clapboard farmhouse with a covered porch and a smoking chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The house as a tree that stands the way it does: the fieldstone footing at
/// the bottom, the shell on it (carrying the roof and the chimney), and the
/// porch as its own sub-assembly.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let footing = prim(
        solid(cuboid_tapered(
            [W + 0.6, FOOT_H, D + 0.6],
            0.0,
            stone(STONE_GREY),
        )),
        [0.0, FOOT_H * 0.5, 0.0],
        id_quat(),
    );
    let mut root = nest(footing, vec![shell(), porch()]);
    // Signature life: hearth smoke off the gable-end chimney.
    root.children.push(fx::chimney_smoke(
        [chimney_x(), chimney_top() + 0.5, -0.6],
        0x0FA1_5E11,
    ));
    root
}

/// The chimney's X centre — clear of the gable wall by half its own breadth,
/// so it stands *against* the house rather than inside it (#972 lesson 11).
fn chimney_x() -> f32 {
    -(W * 0.5 + 0.55)
}

/// Where the chimney tops out. Derived from the ridge, because a flue that
/// does not clear its own roof draws smoke back down it — and the old one
/// stopped 1.4 m under the ridge line with the smoke plume starting inside
/// the roof mass.
fn chimney_top() -> f32 {
    FOOT_H + BODY_H + ROOF_RISE + 0.75
}

// --- The shell. ------------------------------------------------------------

/// Ground floor deck, and under it everything the house is: the walls that
/// frame the openings, the glazing, the rooms behind the glass, the belt
/// course, and — on the wall plate — the roof and the chimney.
fn shell() -> Generator {
    let mut parts = Vec::new();
    let mid_y = FOOT_H + BODY_H * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Back and side walls: a solid box open only where the hero face is cut.
    // The side walls are shortened in Z so their ends never share a plane
    // with the front and back slabs' outer faces.
    parts.push(wall(
        [W, BODY_H, WALL_T],
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, BODY_H, inner_d],
            [sx * (W * 0.5 - WALL_T * 0.5), mid_y, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    front_elevation(&mut parts);
    for (x, sill, head, warm) in openings() {
        let cy = FOOT_H + (sill + head) * 0.5;
        parts.push(glazing([OPEN_W, head - sill], [x, cy, GLAZE_Z]));
        parts.push(room(
            [OPEN_W + 0.45, head - sill + 0.45],
            [x, cy, ROOM_Z],
            warm,
        ));
        // Sill below and casing head above, both proud of the siding.
        parts.push(trim(
            [OPEN_W + 0.42, 0.14, 0.3],
            [x, FOOT_H + sill - 0.07, TRIM_Z],
        ));
        parts.push(trim(
            [OPEN_W + 0.42, 0.16, 0.22],
            [x, FOOT_H + head + 0.08, TRIM_Z],
        ));
    }
    entrance(&mut parts);

    // Belt course at the storey line and a frieze under the eaves, both
    // ringing all four elevations as single boards.
    for (y, h) in [(STOREY, 0.18_f32), (BODY_H - 0.16, 0.24)] {
        parts.push(trim([W + 0.22, h, D + 0.22], [0.0, FOOT_H + y, 0.0]));
    }

    parts.push(roof());
    parts.push(chimney());

    let deck = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.1, inner_d],
            0.0,
            lit_interior([0.30, 0.25, 0.20], 0.12),
        ),
        [0.0, FOOT_H + 0.05, 0.0],
        id_quat(),
    );
    nest(deck, parts)
}

/// The hero face, built as the siding that *frames* three bays: four piers,
/// two sill walls under the ground windows, three spandrels between the
/// storeys, and the head band that carries the wall up to the plate.
///
/// Every piece is coplanar at [`FRONT_MID`] and shares one course frame, so
/// the lap runs through pier, sill and spandrel as if the elevation had been
/// clad in one pass. Nothing overlaps: the piers stop at the plate and the
/// bands fill only the bays.
fn front_elevation(parts: &mut Vec<Generator>) {
    let mut edges = vec![-W * 0.5];
    for &x in BAY_X.iter() {
        edges.push(x - OPEN_W * 0.5);
        edges.push(x + OPEN_W * 0.5);
    }
    edges.push(W * 0.5);
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        parts.push(wall(
            [b - a, BODY_H, WALL_T],
            [(a + b) * 0.5, FOOT_H + BODY_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    // Sill walls under the two ground-storey windows. The entrance bay has
    // none, so the door reaches the floor.
    for (b, &x) in BAY_X.iter().enumerate() {
        if b == DOOR_BAY {
            continue;
        }
        parts.push(wall(
            [OPEN_W, G_SILL, WALL_T],
            [x, FOOT_H + G_SILL * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    // Spandrels between the storeys — the entrance bay's reaches lower, down
    // to the door head — and the frieze band over the top row.
    for (b, &x) in BAY_X.iter().enumerate() {
        let low = if b == DOOR_BAY { DOOR_H } else { G_HEAD };
        parts.push(wall(
            [OPEN_W, U_SILL - low, WALL_T],
            [x, FOOT_H + (low + U_SILL) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
        parts.push(wall(
            [OPEN_W, BODY_H - U_HEAD, WALL_T],
            [x, FOOT_H + (U_HEAD + BODY_H) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
}

/// The front door in the centre bay: a painted leaf in the reveal, a lit hall
/// behind it, casings, and the porch lamp beside it.
fn entrance(parts: &mut Vec<Generator>) {
    let x = BAY_X[DOOR_BAY];
    // The hall behind the door, lit, so the doorway is depth rather than a
    // painted rectangle when anything looks past the leaf.
    parts.push(room(
        [OPEN_W + 0.5, DOOR_H + 0.4],
        [x, FOOT_H + DOOR_H * 0.5, ROOM_Z],
        true,
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [OPEN_W + 0.06, DOOR_H - 0.34, 0.1],
            0.0,
            glow(DOOR_PAINT, 0.0),
        )),
        [x, FOOT_H + (DOOR_H - 0.34) * 0.5, GLAZE_Z - 0.05],
        id_quat(),
    ));
    // Transom light over the leaf — depth discipline for a doorway (#972
    // lesson 6): the camera looking up through the head gets something warm
    // rather than the underside of the landing.
    parts.push(glazing(
        [OPEN_W, 0.24],
        [x, FOOT_H + DOOR_H - 0.16, GLAZE_Z],
    ));
    parts.push(trim(
        [OPEN_W + 0.5, 0.18, 0.26],
        [x, FOOT_H + DOOR_H + 0.09, TRIM_Z],
    ));
    // A small housing with a smaller lit lens: a broad panel at strength
    // blooms to white, a small one reads as a warm colour.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.2, 0.26, 0.12],
            0.0,
            stone([0.3, 0.29, 0.27]),
        )),
        [x + 1.05, FOOT_H + 2.05, FRONT - 0.06],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.12, 0.16, 0.06], 0.0, glow(LAMP_AMBER, 2.0)),
        [x + 1.05, FOOT_H + 2.05, FRONT - 0.12],
        id_quat(),
    ));
}

/// The ridged shingle roof, its gable ends clad back in siding, plus the
/// eaves fascias and the rake boards.
///
/// Pinching **Z alone** is what makes this a roof rather than a hip with a
/// plateau: the top face collapses to a line along X and the `±X` faces
/// become the triangles a gable is. Those two faces then take a per-face
/// override (#955) carrying the wall's own siding at its own offset, so the
/// lap runs from the sill straight up into the apex.
fn roof() -> Generator {
    let plate = FOOT_H + BODY_H;
    let center = [0.0, plate + ROOF_RISE * 0.5, 0.0];
    // No rake overhang in X: the gable triangle *is* the wall carried up, so
    // it has to land in the wall's own plane. The rake boards below supply
    // the overhang read.
    let mut kind = solid(cuboid_tapered_xz(
        [W, ROOF_RISE, D + EAVE_OVER * 2.0],
        [0.0, RIDGE_TAPER],
        shingle(ROOF_GREY),
    ));
    for face in [FaceKey::SidePx, FaceKey::SideNx] {
        kind = with_face(kind, face, lap(center, face));
    }
    let mut parts = Vec::new();
    // Eaves fascia along the front and back, hung off the roof's own edge.
    for sz in [-1.0_f32, 1.0] {
        parts.push(trim(
            [W + 0.3, 0.26, 0.12],
            [0.0, plate + 0.06, sz * (D * 0.5 + EAVE_OVER - 0.06)],
        ));
    }
    // Rake boards up both gables, tilted by the roof's *own* pitch. A
    // hand-picked angle silently stops matching its gable the moment either
    // the rise or the span changes.
    let half = D * 0.5 + EAVE_OVER;
    let slope = ROOF_RISE.hypot(half);
    let pitch = ROOF_RISE.atan2(half);
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.12, 0.26, slope],
                    0.0,
                    util::bonded_siding(
                        clapboard(TRIM_WHITE),
                        if sx > 0.0 {
                            FaceKey::SidePx
                        } else {
                            FaceKey::SideNx
                        },
                        [sx * (W * 0.5 + 0.06), plate, 0.0],
                    ),
                )),
                [
                    sx * (W * 0.5 + 0.06),
                    plate + ROOF_RISE * 0.5,
                    sz * half * 0.5,
                ],
                quat_x(sz * pitch),
            ));
        }
    }
    nest(prim(kind, center, id_quat()), parts)
}

/// The fieldstone chimney climbing the gable end: a broad shoulder, a
/// narrower shaft, a cap and two pots.
///
/// Its breast stands *against* the gable rather than inside it, and its top
/// is derived from the ridge rather than picked, which are the two ways a
/// chimney goes wrong invisibly.
fn chimney() -> Generator {
    let x = chimney_x();
    let top = chimney_top();
    let z = -0.6;
    let shoulder_h = FOOT_H + BODY_H * 0.62;
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [0.95, top - shoulder_h, 1.05],
                0.0,
                stone(STONE_GREY),
            )),
            [x, (shoulder_h + top) * 0.5, z],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [1.16, 0.18, 1.26],
                0.0,
                stone([0.6, 0.58, 0.54]),
            )),
            [x, top + 0.09, z],
            id_quat(),
        ),
    ];
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.15,
                0.42,
                8,
                0.1,
                stone([0.42, 0.32, 0.28]),
            )),
            [x, top + 0.39, z + sz * 0.28],
            id_quat(),
        ));
    }
    let breast = prim(
        solid(cuboid_tapered(
            [1.25, shoulder_h, 1.35],
            0.0,
            stone(STONE_GREY),
        )),
        [x, shoulder_h * 0.5, z],
        id_quat(),
    );
    nest(breast, parts)
}

// --- The porch. ------------------------------------------------------------

/// The covered porch: a boarded deck, four posts with brackets, railing runs
/// between the outer pairs, a pitched shed roof, and stone steps.
///
/// The deck is the sub-root, so one drag takes the posts, the roof and the
/// steps with it. Everything is derived from the deck's own extent — the
/// steps land on it, the posts stand inside its edge and the roof's fall
/// gives the fascia its height (#972 lesson 8).
fn porch() -> Generator {
    let deck_z = FRONT - PORCH_D * 0.5;
    let deck = prim(
        solid(cuboid_tapered(
            [W + 0.5, 0.2, PORCH_D],
            0.0,
            util::bonded_siding(
                clapboard(DECK_GREY),
                FaceKey::Top,
                [0.0, FOOT_H - 0.1, deck_z],
            ),
        )),
        [0.0, FOOT_H - 0.1, deck_z],
        id_quat(),
    );

    let mut parts = Vec::new();
    for px in PORCH_X {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.18, PORCH_BEAM, 0.18],
                0.0,
                util::bonded_siding(
                    clapboard(TRIM_WHITE),
                    FaceKey::SideNz,
                    [px, FOOT_H + PORCH_BEAM * 0.5, FRONT - PORCH_D + 0.24],
                ),
            )),
            [px, FOOT_H + PORCH_BEAM * 0.5, FRONT - PORCH_D + 0.24],
            id_quat(),
        ));
        // Sawn bracket in the head of each post — the one piece of ornament
        // a farmhouse porch actually has.
        parts.push(trim(
            [0.5, 0.16, 0.1],
            [px, FOOT_H + PORCH_BEAM - 0.24, FRONT - PORCH_D + 0.18],
        ));
    }
    // Head beam carrying the roof.
    parts.push(trim(
        [W + 0.5, 0.24, 0.18],
        [0.0, FOOT_H + PORCH_BEAM + 0.12, FRONT - PORCH_D + 0.24],
    ));
    // Railing between the outer post pairs; the centre bay is the entry.
    for sx in [-1.0_f32, 1.0] {
        let (a, b) = (sx * PORCH_X[0].abs(), sx * PORCH_X[1].abs());
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let cx = (lo + hi) * 0.5;
        for y in [0.62_f32, 1.02] {
            parts.push(trim(
                [hi - lo, 0.1, 0.12],
                [cx, FOOT_H + y, FRONT - PORCH_D + 0.24],
            ));
        }
        for i in 0..5 {
            let x = lo + (hi - lo) * (i as f32 + 0.5) / 5.0;
            parts.push(trim(
                [0.07, 0.42, 0.07],
                [x, FOOT_H + 0.82, FRONT - PORCH_D + 0.24],
            ));
        }
    }
    // Shed roof, tilted by its own fall so the fascia and the pitch cannot
    // disagree. The slope falls toward the yard, which is `-Z`, so the tilt
    // is negative.
    let fall = PORCH_FALL.atan2(PORCH_D);
    parts.push(prim(
        solid(cuboid_tapered(
            [W + 0.9, 0.2, PORCH_D.hypot(PORCH_FALL) + 0.35],
            0.0,
            shingle(ROOF_GREY),
        )),
        [
            0.0,
            porch_roof_back() - PORCH_FALL * 0.5,
            FRONT - PORCH_D * 0.5 - 0.1,
        ],
        quat_x(-fall),
    ));
    // Fascia closing the porch roof's leading edge, so the slab reads as a
    // roof with a thickness rather than as an awning.
    parts.push(trim(
        [W + 0.9, 0.2, 0.1],
        [
            0.0,
            porch_roof_back() - PORCH_FALL - 0.02,
            FRONT - PORCH_D - 0.26,
        ],
    ));
    // Stone steps down off the deck, stepping forward from its own front
    // edge so they can never hang past it, and *evenly*: the deck top
    // divided by the flight, not a pair of guessed drops.
    let front_edge = FRONT - PORCH_D;
    let deck_top = FOOT_H;
    for i in 0..2 {
        let top = deck_top * (2 - i) as f32 / 3.0;
        parts.push(prim(
            solid(cuboid_tapered(
                [2.1 + 0.3 * i as f32, top, 0.36],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, top * 0.5, front_edge - 0.18 - 0.36 * i as f32],
            id_quat(),
        ));
    }
    nest(deck, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_sanitize_stable, window_cards,
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
        assert_sanitize_stable(&Farmhouse.build(""), "farmhouse");
    }

    /// #972 lesson 1: one `Window` card per opening plus the door's transom,
    /// each on a `Plane` at `uv_scale` 1.0. The exact count is the part that
    /// bites — a card moved onto a solid still renders, it just renders as a
    /// frame with holes onto the siding behind it.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&Farmhouse.build(""), [0.0; 3], &mut |g, _| {
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
        assert_eq!(cards, openings().len() + 1, "five windows and a transom");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Farmhouse.build(""), "farmhouse");
    }

    /// #972 lesson 7: cards lap their openings rather than sitting flush in
    /// the reveal, and stand clear of the rooms they frame.
    #[test]
    fn cards_lap_their_openings() {
        for c in window_cards(&Farmhouse.build("")) {
            assert!(
                c.size[0] > OPEN_W + 1e-4,
                "a card {:?} is flush with its reveal",
                c.size
            );
            assert!(
                c.center[2] < ROOM_Z - 0.2,
                "a card at z {} is not clear of the room behind it",
                c.center[2]
            );
        }
    }

    /// #972 lesson 4: the siding is `stagger`-free and every slab shares one
    /// course frame, so the lap runs unbroken through pier, sill and gable.
    /// The offset is far too subtle to catch in a render.
    #[test]
    fn siding_shares_one_course_frame() {
        let mut checked = 0;
        walk(&Farmhouse.build(""), [0.0; 3], &mut |g, at| {
            let (material, faces) = match &g.kind {
                GeneratorKind::Cuboid {
                    material, faces, ..
                } => (material, faces),
                _ => return,
            };
            let SovereignTextureConfig::Plank(cfg) = &material.texture else {
                return;
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "a staggered plank at {at:?} brings back the butt-joint grid"
            );
            assert_eq!(
                material.uv_rotation.0, 0.0,
                "lap siding at {at:?} has been stood on end"
            );
            if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0] {
                let want: Vec<_> = [
                    FaceKey::SideNz,
                    FaceKey::SidePz,
                    FaceKey::SideNx,
                    FaceKey::SidePx,
                    FaceKey::Top,
                ]
                .into_iter()
                .map(|f| util::face_uv_offset(f, at).0)
                .collect();
                let got = material.uv_offset.0;
                assert!(
                    want.iter()
                        .any(|w| (w[0] - got[0]).abs() < 1e-3 && (w[1] - got[1]).abs() < 1e-3),
                    "siding at {at:?} carries uv_offset {got:?}, which is no face's \
                     projection of its own position — its courses restart at its centre"
                );
            }
            for o in faces {
                let w = util::face_uv_offset(o.face, at).0;
                let g = o.material.uv_offset.0;
                assert!(
                    (w[0] - g[0]).abs() < 1e-3 && (w[1] - g[1]).abs() < 1e-3,
                    "the {:?} gable override at {at:?} carries {g:?}, not {w:?}",
                    o.face
                );
            }
            checked += 1;
        });
        assert!(checked > 20, "only {checked} sided slabs found");
    }

    /// The roof has a ridge, not a plateau: pinching Z alone collapses the
    /// top to a line, and the two triangles it leaves are the gables. A
    /// uniform taper — what this used to carry — gives a square-topped hip,
    /// which no farmhouse has and which a head-on render cannot distinguish.
    #[test]
    fn the_roof_is_ridged_and_its_gables_are_clad() {
        let root = Farmhouse.build("");
        let mut found = false;
        walk(&root, [0.0; 3], &mut |g, _| {
            let GeneratorKind::Cuboid {
                torture,
                faces,
                material,
                ..
            } = &g.kind
            else {
                return;
            };
            if !matches!(material.texture, SovereignTextureConfig::Shingle(_)) {
                return;
            }
            let [tx, tz] = torture.taper.0;
            if tz < 0.5 {
                return; // the porch's shed roof
            }
            found = true;
            assert_eq!(tx, 0.0, "the roof is pinched in X too — that is a hip");
            assert!(tz > 0.9, "the ridge taper {tz} leaves a plateau on top");
            let clad: Vec<_> = faces.iter().map(|o| o.face).collect();
            assert!(
                clad.contains(&FaceKey::SidePx) && clad.contains(&FaceKey::SideNx),
                "the gable triangles wear shingle instead of siding: {clad:?}"
            );
        });
        assert!(found, "no ridged roof in the tree");
    }

    /// The flue clears its own roof. It used to stop 1.4 m under the ridge
    /// line, which puts the smoke plume inside the roof mass — and no angle
    /// the contact sheet takes looks along the ridge to show it.
    #[test]
    fn the_chimney_clears_the_ridge() {
        let ridge = FOOT_H + BODY_H + ROOF_RISE;
        assert!(
            chimney_top() > ridge + 0.5,
            "the chimney tops out at {}, only {} above the ridge at {ridge}",
            chimney_top(),
            chimney_top() - ridge
        );
        // ...and stands *against* the gable: keyed a little way into the
        // wall so no daylight gap opens between them, and proud enough to
        // read as a chimney rather than as a pilaster. Both sides of that
        // matter — the breast is 1.25 m across, so an eye-picked x is as
        // likely to bury it as to float it.
        let half = 0.625;
        let inner = chimney_x() + half;
        let proud = -W * 0.5 - (chimney_x() - half);
        assert!(
            (-W * 0.5..-W * 0.5 + 0.2).contains(&inner),
            "the chimney breast's inner face at {inner} does not key into the \
             gable wall at {}",
            -W * 0.5
        );
        assert!(
            proud > 0.8,
            "the chimney stands only {proud} m proud of the gable"
        );
    }

    /// The porch roof passes *under* the first-floor sills. A shed roof is
    /// highest where it meets the wall, so the head height and the fall push
    /// this up together — and a roof crossing the sill line reads from the
    /// pavement as slicing the bottom off every upper window, which a
    /// three-quarter render angle hides almost completely.
    #[test]
    fn the_porch_roof_clears_the_upper_sills() {
        let back = porch_roof_back() + 0.1; // half the slab's own thickness
        let sill = FOOT_H + U_SILL;
        assert!(
            back < sill - 0.1,
            "the porch roof lands at {back}, into the upper sills at {sill}"
        );
        // ...and its head beam passes over everything on the ground storey,
        // rather than cutting across the window and door heads it stands in
        // front of.
        let beam_soffit = FOOT_H + PORCH_BEAM - 0.12;
        let highest_head = FOOT_H + G_HEAD.max(DOOR_H);
        assert!(
            beam_soffit > highest_head,
            "the porch beam's soffit at {beam_soffit} crosses the ground-storey \
             heads at {highest_head}"
        );
    }

    /// #972 lesson 8: everything the porch carries stands inside the deck it
    /// stands on. The steps are the piece that used to be measured off the
    /// house, and a step hanging past the deck is invisible unless a
    /// contact-sheet tile happens to look along that edge.
    #[test]
    fn the_porch_stands_on_its_own_deck() {
        let root = Farmhouse.build("");
        let porch = &root.children[1];
        let mut deck: Option<([f32; 3], [f32; 3])> = None;
        let mut steps = Vec::new();
        walk(porch, root.transform.translation.0, &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            let half = [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5];
            if deck.is_none() {
                deck = Some((at, half));
            } else if matches!(material.texture, SovereignTextureConfig::Cobblestone(_)) {
                steps.push((at, half));
            }
        });
        let (dc, dh) = deck.expect("the porch stands on a deck");
        assert_eq!(steps.len(), 2, "the porch has two stone steps");
        for (c, h) in steps {
            assert!(
                c[0] - h[0] > dc[0] - dh[0] - 1e-3 && c[0] + h[0] < dc[0] + dh[0] + 1e-3,
                "a step at {c:?} is wider than the deck it comes off"
            );
            assert!(
                c[2] + h[2] < dc[2] - dh[2] + 1e-3,
                "a step at {c:?} sits under the deck rather than in front of it"
            );
        }
    }

    /// The house keeps a lit window and a lit lamp — escalation's
    /// broken-emissive ruin pass needs something to snuff.
    #[test]
    fn has_lit_rooms() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Farmhouse.build("")
        ));
    }

    /// The editability contract: dragging the roof takes its rake boards,
    /// dragging the chimney takes its cap and pots, dragging the deck takes
    /// the posts and the steps.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Farmhouse.build("");
        assert_eq!(
            root.children.len(),
            3,
            "footing carries shell, porch, smoke"
        );
        let shell = &root.children[0];
        assert!(
            shell.children.iter().any(|c| c.children.len() == 4),
            "the roof carries its four rake boards"
        );
        assert!(
            shell.children.iter().any(|c| c.children.len() == 4),
            "the chimney carries its shaft, cap and pots"
        );
        assert!(
            root.children[1].children.len() > 20,
            "the porch carries its posts, railing, roof and steps"
        );
        assert!(count(&root) > 60, "the house lost most of its parts");
    }
}
