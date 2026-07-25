//! Corner store — a Modern-City *poor* secondary. A single-storey brick
//! bodega whose shopfront is a genuine hole in the wall: brick piers, a
//! stall riser and a lintel framing it, glazing cards filling the gap.
//! The bodega beside the [`tenement`](super::tenement).
//!
//! This entry is the reference for the `Window` texture idiom — see
//! [`crate::catalogue::items::util::window_card`] for the rules
//! it follows. The short version: the generator's panes are alpha-masked
//! *away*, so the card is a frame with real holes in it. That only reads if
//! there is an opening for it to fill and an interior behind it worth
//! seeing, which is why this store is built as a shell — four walls, a roof,
//! a lit fit-out with stocked shelves — instead of a solid block with glass
//! slabs pinned to the front.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, plane, prim, quat_x, solid, tiles_per_metre,
    window_card, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{
    Fp, Fp2, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, LAMP_WARM, brick, concrete, enamel, steel};

/// Tired warm sign light — deep-saturated amber so the lit face reads as a
/// colour under bloom rather than washing to a near-white blank.
const SIGN_GLOW: [f32; 3] = [1.0, 0.46, 0.13];
/// Awning stripe colours.
const AWNING_RED: [f32; 3] = [0.52, 0.13, 0.12];
const AWNING_CREAM: [f32; 3] = [0.82, 0.78, 0.68];
/// Shopfront joinery — the anodised frame of the glazing cards, dark enough
/// to draw the opening against the brick.
const SHOPFRONT: [f32; 3] = [0.20, 0.21, 0.23];

// --- Shell dimensions. Everything below is derived from these. -------------

const W: f32 = 8.0;
const D: f32 = 7.0;
const BASE_H: f32 = 0.4;
const BODY_H: f32 = 4.0;
/// Brick wall thickness. Also the depth of the shopfront reveal.
const WALL_T: f32 = 0.35;

/// Outer face of the front wall. The shopfront looks down `-Z`, the render
/// tool's and the settlement placer's hero direction.
const FRONT: f32 = -D * 0.5;
/// Centre of a slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing sits back from the outer brick face, so the reveal reads as
/// thickness rather than as a sticker.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;

/// How far behind the glazing the display run sits. Close enough that a
/// pane frames a whole object rather than a speck of the back wall.
const DISPLAY_Z: f32 = FRONT + 1.15;

/// Width of the brick piers flanking the shopfront.
const PIER_W: f32 = 0.8;
/// The shopfront opening spans this in X, between the two piers.
const OPEN_X0: f32 = -W * 0.5 + PIER_W;
const OPEN_X1: f32 = W * 0.5 - PIER_W;
/// Where the display bay ends and the door bay begins.
const DOOR_X0: f32 = 1.75;
/// Head height of the whole shopfront opening.
const HEAD_Y: f32 = BASE_H + 2.65;
/// Top of the stall riser under the display window. The door bay has none,
/// so the door reaches the floor.
const SILL_Y: f32 = BASE_H + 0.65;

// --- Brickwork (#966). ------------------------------------------------------

/// Brick length in metres — a real 215 mm brick. The kit's shared sizing
/// lays a 172 mm one, small enough at street distance to mip toward flat
/// colour.
const BRICK_LEN: f32 = 0.215;
/// Brick rows per texture tile — the generator's `scale`. Ten rather than
/// the kit's five: the tile has to hold enough bricks that the seam artifact
/// below is rare, and rows and columns scale together.
const BRICK_ROWS: f64 = 10.0;
/// Brick columns per tile — `round(scale × aspect_ratio)`, and the number of
/// bricks a tile spans across the wall.
///
/// Four is the smallest count that keeps the tile seam quiet. The generator
/// colours each brick by hashing its **raw** cell index, and a brick
/// straddling the tile's U seam is indexed `0` on one side and `cols` on the
/// other — so it renders as two half-bricks of different colour. One brick
/// per tile per staggered course always straddles; at two columns that was
/// every fourth brick on the wall, and the eye reads it immediately. Raising
/// the count dilutes it (and kills the two-brick colour repeat that banded
/// the wall into vertical stripes) without changing the brick's size, since
/// [`bonded_brick`] derives `uv_scale` from the column count.
const BRICK_COLS: f32 = 4.0;
/// Cell aspect. **Inverted from what the generator's own doc suggests**: it
/// derives columns as `scale × aspect_ratio` while `scale` *is* the row
/// count, so under this app's uniform metre mapping (a UV tile is square in
/// metres) a value above 1 makes each cell taller than it is wide. `0.4`
/// gives 4 columns to 10 rows — a brick 2.5× longer than it is tall, laid
/// flat.
const BRICK_ASPECT: f64 = 0.4;
/// Bond stagger per course, as a fraction of brick length — the classic
/// half-bond. The generator needs `scale × row_offset` to be a whole number
/// to tile cleanly in V; `10 × 0.5` is, where the kit's `5 × 0.5` was not.
const BRICK_BOND: f64 = 0.5;
/// Per-brick colour jitter, below the kit's `0.2`. It is what makes a wall
/// read as fired clay rather than paint, but it is also the *only* thing
/// that makes a seam-straddling brick visible — the two halves differ by up
/// to twice this. Low enough that the survivors read as shading, high enough
/// that the wall still varies.
const BRICK_VARIANCE: f64 = 0.15;

/// The store's brickwork: the kit's [`brick`] with its courses laid **flat**,
/// at a real brick's size, and its bond continued into the wall's own frame.
///
/// # Laying the courses flat
///
/// The generator counts `scale` rows up V and `scale × aspect_ratio` columns
/// across U. Since #933 a UV tile is *square in metres*, so ten columns to
/// five rows makes every brick twice as tall as it is wide — upright, which
/// no bricklayer has ever produced. Flipping the aspect (see
/// [`BRICK_ASPECT`]) turns the cell without turning the *bond*.
///
/// A 90° `uv_rotation` looks like the obvious fix and is not: it spins the
/// running bond with the bricks, so the stagger ends up between vertical
/// strips instead of between courses, and the wall reads as continuous
/// vertical mortar lines running its full height. Rotation and a correct
/// bond are mutually exclusive here — the stagger is applied along U by the
/// generator itself.
///
/// # The seam the generator cannot hide
///
/// Per-brick colour comes from hashing the **raw** cell index, so the brick
/// that straddles a tile's U seam is hashed twice — once as index `0`, once
/// as index `cols` — and renders as two half-bricks of different colour. It
/// is unavoidable at this level: a running bond shifts each course by half a
/// brick, so some course always crosses the seam mid-brick, and only the
/// generator itself could fix it (by hashing the index *modulo* the column
/// count, which would make both halves agree). [`BRICK_COLS`] and
/// [`BRICK_VARIANCE`] are chosen to make what remains read as shading.
///
/// # Bonding the slabs together
///
/// Each projection is prim-local and centred on the prim's own bounds, so
/// four slabs framing a shopfront each restart the bond at their own centre
/// and the joints between them read as breaks in the wall. Shifting each
/// slab's UVs by its own position puts every piece in one shared frame — the
/// courses then run through a pier, across a lintel and past a riser as if
/// the wall were cut from one mass.
///
/// The mesher's Box projection reads a *different pair of local axes* per
/// face, though (`(−x, −y)` on a −Z wall, `(x, z)` looking down on a top
/// face, `(−z, −y)` on a +X side), so one offset can only serve one face —
/// which is what [`face_offset`] resolves, and why a slab whose corner is
/// visible carries a per-face override for the face it turns onto.
fn bonded_brick(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = SovereignMaterialSettings {
        uv_scale: tiles_per_metre(BRICK_LEN * BRICK_COLS),
        uv_offset: face_offset(face, center),
        ..brick(color)
    };
    if let SovereignTextureConfig::Brick(cfg) = &mut m.texture {
        cfg.scale = Fp64(BRICK_ROWS);
        cfg.aspect_ratio = Fp64(BRICK_ASPECT);
        cfg.row_offset = Fp64(BRICK_BOND);
        cfg.cell_variance = Fp64(BRICK_VARIANCE);
    }
    m
}

/// The UV offset that puts **one face** of a slab into the shared world
/// course frame (#969).
///
/// The Box projection is prim-local and per-face: each of the six regions
/// reads its own pair of local axes, in its own sign convention — `(−x, −y)`
/// on a −Z wall, `(x, z)` on a top face, `(−z, −y)` on a +X side. It is also
/// *linear* in position, which is what makes this a one-liner: the offset
/// that turns a prim-local UV into a world-frame one is the very same
/// projection applied to the slab's own centre.
///
/// So a slab's brickwork lines up with its neighbours' on the face this
/// names, and only that face. A sill wants `Top`, the outer return of a pier
/// wants the side it turns onto, and the base material serves whichever face
/// people mostly look at.
fn face_offset(face: FaceKey, center: [f32; 3]) -> Fp2 {
    let [x, y, z] = center;
    Fp2(match face {
        FaceKey::SidePx => [-z, -y],
        FaceKey::SideNx => [z, -y],
        FaceKey::Top => [x, z],
        FaceKey::Bottom => [x, -z],
        FaceKey::SidePz => [x, -y],
        // `SideNz` — the shopfront convention — and anything else.
        _ => [-x, -y],
    })
}

/// One brick slab of the shell, bonded into the shared course frame — the
/// position is given once and drives both the placement and the UV offsets,
/// so the two cannot drift apart.
///
/// `facing` names the face the slab's *base* material serves: the one people
/// mostly look at. `wraps` lists the other faces of the same slab that meet
/// brick at a corner someone can see, each of which gets a per-face override
/// (#955) carrying the same brick at its own offset. Every entry costs a
/// draw call, so this is a list of corners that actually read, not of every
/// face that exists.
///
/// Which is a shorter list than it first appears, because the four *side*
/// faces all put the courses on `V = −y`: turning a vertical corner, the
/// courses line up on the base offset alone, and only the column phase
/// differs — which matters solely where two slabs are **coplanar** (a pier
/// and the side wall behind it). Horizontal corners are the ones that always
/// need a wrap: a `Top` or `Bottom` face reads depth where its neighbour
/// reads height, so nothing about it follows from the base.
fn brick_slab(
    size: [f32; 3],
    color: [f32; 3],
    center: [f32; 3],
    facing: FaceKey,
    wraps: &[FaceKey],
) -> Generator {
    let mut kind = solid(cuboid_tapered(
        size,
        0.0,
        bonded_brick(color, center, facing),
    ));
    for &face in wraps {
        kind = with_face(kind, face, bonded_brick(color, center, face));
    }
    prim(kind, center, id_quat())
}

/// Dim warm interior surface. The shell is enclosed and nothing lights it,
/// so the surfaces seen through the glazing carry a low self-lit term of
/// their own. Without it the openings read as black rectangles and every
/// bit of work behind the glass is invisible.
fn interior(color: [f32; 3], lit: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3([color[0] * 1.1, color[1], color[2] * 0.85]),
        emission_strength: Fp(lit),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

pub struct CornerStore;

impl CatalogueEntry for CornerStore {
    fn slug(&self) -> &'static str {
        "corner_store"
    }
    fn name(&self) -> &'static str {
        "Corner Store"
    }
    fn description(&self) -> &'static str {
        "Brick bodega with a glazed shopfront, striped awning, and a tired lit sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Concrete base — the root, and flat, so no child inherits a tilt.
        prim(
            solid(cuboid_tapered(
                [W + 0.4, BASE_H, D + 0.4],
                0.0,
                concrete([0.45, 0.45, 0.46]),
            )),
            [0.0, BASE_H * 0.5, 0.0],
            id_quat(),
        ),
    ];

    shell(&mut prims);
    shopfront(&mut prims);
    interior_fitout(&mut prims);
    street_furniture(&mut prims);

    assemble(prims)
}

/// Back and side walls, roof and parapet — the box the shopfront is cut out
/// of. Built as separate slabs rather than one solid mass precisely so the
/// inside is hollow and the glazing has something to look into.
fn shell(prims: &mut Vec<Generator>) {
    let mid_y = BASE_H + BODY_H * 0.5;

    // The back wall is seen from behind, so its +Z face is the one whose
    // frame the material serves.
    prims.push(brick_slab(
        [W, BODY_H, WALL_T],
        BRICK_RED,
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
        &[],
    ));
    // Side walls, shortened in Z so their ends never share a plane with the
    // front and back slabs' outer faces. Each one's OUTER face is what the
    // street sees, and it is coplanar with the pier that closes the corner
    // in front of it — so both must sit in that side's frame, not the
    // shopfront's, or the two halves of one elevation disagree.
    for sx in [-1.0_f32, 1.0] {
        prims.push(brick_slab(
            [WALL_T, BODY_H, D - WALL_T * 2.0],
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
    // Roof deck, held a hair inside the walls, then the parapet over it.
    prims.push(prim(
        solid(cuboid_tapered(
            [W - 0.04, 0.25, D - 0.04],
            0.0,
            concrete([0.38, 0.38, 0.39]),
        )),
        [0.0, BASE_H + BODY_H + 0.125, 0.0],
        id_quat(),
    ));
    // Parapet: a band proud of the walls on every side, so it joins nothing
    // in-plane and needs no wrap. Its four sides all read `V = −y`, which is
    // what carries the courses round a vertical corner, so they already line
    // up across its mitres — see [`brick_slab`].
    prims.push(brick_slab(
        [W + 0.3, 0.55, D + 0.3],
        [0.4, 0.22, 0.17],
        [0.0, BASE_H + BODY_H + 0.4, 0.0],
        FaceKey::SideNz,
        &[],
    ));
}

/// The front wall, built as the four brick pieces that *frame* the opening —
/// two piers, a lintel, a stall riser — plus the glazing cards filling it.
fn shopfront(prims: &mut Vec<Generator>) {
    // The piers close the building's two front corners: each shows its
    // shopfront face and, around the corner, the outer return that carries on
    // into the side wall behind it.
    for sx in [-1.0_f32, 1.0] {
        prims.push(brick_slab(
            [PIER_W, BODY_H, WALL_T],
            BRICK_RED,
            [
                sx * (W * 0.5 - PIER_W * 0.5),
                BASE_H + BODY_H * 0.5,
                FRONT_MID,
            ],
            FaceKey::SideNz,
            &[if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            }],
        ));
    }
    // Lintel over the opening, carrying the wall up to the parapet. Its bond
    // continues the piers' (see [`bonded_brick`]), so the three read as one
    // wall with a hole in it rather than as three slabs.
    // Its soffit is the head of the opening — the wall turning the corner
    // over the glazing, and read from the pavement below.
    let lintel_h = BASE_H + BODY_H - HEAD_Y;
    prims.push(brick_slab(
        [OPEN_X1 - OPEN_X0, lintel_h, WALL_T],
        BRICK_RED,
        [
            (OPEN_X0 + OPEN_X1) * 0.5,
            HEAD_Y + lintel_h * 0.5,
            FRONT_MID,
        ],
        FaceKey::SideNz,
        &[FaceKey::Bottom],
    ));
    // Stall riser under the display bay only. Same brick as the piers and
    // the lintel: it is part of the same wall plane, and a darker one read
    // as a different material bolted under the window rather than as the
    // wall carrying on below the sill (#968). The parapet keeps its darker
    // brick — that one is a coping band on top of the building, not part of
    // this face.
    // Its top is the sill the shopper leans over, and the face where the
    // corner wrap is most obvious: the bricks turning onto it must be the
    // same bricks that end the face below (#969).
    let riser_w = DOOR_X0 - OPEN_X0;
    prims.push(brick_slab(
        [riser_w, SILL_Y - BASE_H, WALL_T],
        BRICK_RED,
        [OPEN_X0 + riser_w * 0.5, (BASE_H + SILL_Y) * 0.5, FRONT_MID],
        FaceKey::SideNz,
        &[FaceKey::Top],
    ));

    // --- The glazing: one card per bay, each filling its opening exactly.

    // Display window, 4.95 × 2.0 — five panes across by two up come out
    // near-square at that aspect. Opacity below the 0.5 mask cutoff, so the
    // panes are genuinely open and the fit-out shows through them.
    let disp_w = DOOR_X0 - OPEN_X0;
    let disp_h = HEAD_Y - SILL_Y;
    prims.push(prim(
        plane([disp_w, disp_h], window_card(SHOPFRONT, 5, 2, 0.34, 0.035)),
        [OPEN_X0 + disp_w * 0.5, SILL_Y + disp_h * 0.5, GLAZE_Z],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));

    // Glazed door, 1.45 × 2.65 — upright, so one pane across by three up. A
    // wider frame fraction than the display card: a door stile really is
    // chunkier than a shopfront mullion.
    let door_w = OPEN_X1 - DOOR_X0;
    let door_h = HEAD_Y - BASE_H;
    prims.push(prim(
        plane([door_w, door_h], window_card(SHOPFRONT, 1, 3, 0.34, 0.09)),
        [DOOR_X0 + door_w * 0.5, BASE_H + door_h * 0.5, GLAZE_Z],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));
    // Door pull, proud of the glazing so it never shares its plane.
    prims.push(prim(
        cuboid_tapered([0.06, 0.9, 0.06], 0.0, steel([0.62, 0.63, 0.65])),
        [DOOR_X0 + 0.28, BASE_H + 1.15, GLAZE_Z - 0.09],
        id_quat(),
    ));
}

/// What the shopper sees through the open panes: a stocked display run
/// immediately behind the glass, a counter mid-shop, a lit ceiling strip.
/// All of it lives inside the shell and is reachable only by eye, through
/// the shopfront — which is the payoff the `Window` card is built for.
///
/// Depth discipline matters more than quantity here. Goods parked against
/// the back wall of a 7 m shop sit five metres behind the glass and shrink
/// to unreadable specks; the display run is held [`DISPLAY_Z`] back instead,
/// close enough that a pane frames a recognisable object.
fn interior_fitout(prims: &mut Vec<Generator>) {
    let inner_w = W - WALL_T * 2.0;

    // Floor and rear lining — the dim envelope everything else reads against.
    prims.push(prim(
        cuboid_tapered(
            [inner_w, 0.06, D - WALL_T * 2.0],
            0.0,
            interior([0.24, 0.22, 0.20], 0.12),
        ),
        [0.0, BASE_H + 0.03, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered(
            [inner_w, BODY_H - 0.4, 0.08],
            0.0,
            interior([0.20, 0.18, 0.16], 0.10),
        ),
        [0.0, BASE_H + BODY_H * 0.5, D * 0.5 - WALL_T - 0.06],
        id_quat(),
    ));
    // Ceiling strip light.
    prims.push(prim(
        cuboid_tapered([inner_w * 0.7, 0.1, 0.35], 0.0, glow(LAMP_WARM, 2.2)),
        [0.0, BASE_H + BODY_H - 0.45, -0.4],
        id_quat(),
    ));

    // Display run right behind the glazing: a plinth at sill height and a
    // shelf above it, both only as wide as the display bay.
    let disp_w = DOOR_X0 - OPEN_X0;
    let disp_cx = OPEN_X0 + disp_w * 0.5;
    prims.push(prim(
        cuboid_tapered(
            [disp_w - 0.2, SILL_Y - BASE_H + 0.12, 0.55],
            0.0,
            interior([0.28, 0.25, 0.22], 0.14),
        ),
        [disp_cx, BASE_H + (SILL_Y - BASE_H + 0.12) * 0.5, DISPLAY_Z],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered(
            [disp_w - 0.2, 0.09, 0.5],
            0.0,
            interior([0.30, 0.27, 0.24], 0.14),
        ),
        [disp_cx, BASE_H + 1.75, DISPLAY_Z],
        id_quat(),
    ));

    // Goods. The only saturated colour inside, sized so one box roughly
    // fills a pane — smaller reads as noise through the mullions.
    let goods = [
        (-2.55_f32, 0.92_f32, [0.74, 0.22, 0.16_f32]),
        (-1.65, 0.92, [0.88, 0.72, 0.22]),
        (-0.6, 0.92, [0.22, 0.46, 0.68]),
        (0.45, 0.92, [0.80, 0.44, 0.14]),
        (1.3, 0.92, [0.30, 0.56, 0.28]),
        (-2.3, 2.02, [0.84, 0.66, 0.26]),
        (-1.1, 2.02, [0.58, 0.24, 0.52]),
        (0.35, 2.02, [0.24, 0.52, 0.62]),
        (1.35, 2.02, [0.76, 0.34, 0.20]),
    ];
    for (x, y, c) in goods {
        prims.push(prim(
            cuboid_tapered([0.6, 0.45, 0.4], 0.0, interior(c, 0.55)),
            [x, BASE_H + y, DISPLAY_Z],
            id_quat(),
        ));
    }

    // Counter mid-shop, reading as depth behind the display run.
    prims.push(prim(
        cuboid_tapered(
            [inner_w * 0.7, 1.05, 0.6],
            0.0,
            interior([0.26, 0.23, 0.21], 0.12),
        ),
        [0.0, BASE_H + 0.525, 1.3],
        id_quat(),
    ));
}

/// Awning and sign — the street-facing dressing over the shopfront.
fn street_furniture(prims: &mut Vec<Generator>) {
    // Striped sloped awning projecting over the pavement, clear of the
    // opening head so it shades the glazing instead of cutting into it.
    let awning_y = HEAD_Y + 0.32;
    for (i, x) in [-2.4_f32, -1.2, 0.0, 1.2, 2.4].iter().enumerate() {
        let col = if i % 2 == 0 { AWNING_RED } else { AWNING_CREAM };
        prims.push(prim(
            solid(cuboid_tapered([1.2, 0.1, 1.7], 0.0, enamel(col))),
            [*x, awning_y, FRONT - 0.75],
            quat_x(-0.24),
        ));
    }
    // Valance lip along the awning's leading edge.
    prims.push(prim(
        solid(cuboid_tapered([6.0, 0.28, 0.1], 0.0, enamel(AWNING_RED))),
        [0.0, awning_y - 0.24, FRONT - 1.55],
        id_quat(),
    ));

    // Box sign on the lintel: a steel housing with an inset lit face, not a
    // bare glowing slab. The housing reads at every hour, and only the
    // smaller face glows — a broad flat panel at strength blooms to white.
    let sign_y = BASE_H + BODY_H - 0.42;
    prims.push(prim(
        solid(cuboid_tapered(
            [4.8, 0.78, 0.22],
            0.0,
            steel([0.26, 0.26, 0.28]),
        )),
        [0.0, sign_y, FRONT - 0.11],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([4.4, 0.52, 0.1], 0.0, glow(SIGN_GLOW, 1.4)),
        [0.0, sign_y, FRONT - 0.24],
        id_quat(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CornerStore.build(""), "corner_store");
    }

    /// Guards the two `Window`-card rules this entry exists to demonstrate:
    /// glazing lives on a flat quad, and its UVs are never scaled (the card
    /// uploads clamp-to-edge, so anything but `1.0` smears its edge texels
    /// across the surface).
    ///
    /// Only `Plane` nodes are inspected — `GeneratorKind` has no material
    /// accessor to sweep every variant with — but the exact-count assertion
    /// still fails loudly if a card is ever moved off a quad onto a solid.
    #[test]
    fn glazing_cards_are_unscaled_quads() {
        fn walk(g: &Generator, seen: &mut usize) {
            if let GeneratorKind::Plane { material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                assert_eq!(
                    material.uv_scale.0, 1.0,
                    "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                );
                *seen += 1;
            }
            for c in &g.children {
                walk(c, seen);
            }
        }
        let mut seen = 0;
        walk(&CornerStore.build(""), &mut seen);
        assert_eq!(
            seen, 2,
            "expected the display window and the door as Plane-borne cards"
        );
    }

    /// Collect every brick slab as `(world position, material)`, walking the
    /// assembled tree so a child's rebased transform is accounted for.
    fn brick_slabs(
        g: &Generator,
        at: [f32; 3],
        out: &mut Vec<([f32; 3], SovereignMaterialSettings)>,
    ) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let GeneratorKind::Cuboid { material, .. } = &g.kind
            && matches!(material.texture, SovereignTextureConfig::Brick(_))
        {
            out.push((here, material.clone()));
        }
        for c in &g.children {
            brick_slabs(c, here, out);
        }
    }

    /// Collect every brick slab's face overrides as
    /// `(world position, face, material)`.
    fn brick_faces(
        g: &Generator,
        at: [f32; 3],
        out: &mut Vec<([f32; 3], FaceKey, SovereignMaterialSettings)>,
    ) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let GeneratorKind::Cuboid {
            material, faces, ..
        } = &g.kind
            && matches!(material.texture, SovereignTextureConfig::Brick(_))
        {
            for ov in faces {
                out.push((here, ov.face, ov.material.clone()));
            }
        }
        for c in &g.children {
            brick_faces(c, here, out);
        }
    }

    /// #966 / #969: every brick surface sits in the one world course frame —
    /// for the face it serves.
    ///
    /// The Box projection reads different local axes per face, so "the shared
    /// frame" is not one offset but one *rule*: the offset must be the face's
    /// own projection of the slab's position ([`face_offset`]). A slab
    /// authored with a bare `prim(...)`, or a face override copied from its
    /// neighbour, breaks it — and the joint is subtle enough in a render that
    /// only this catches it.
    #[test]
    fn every_brick_surface_sits_in_the_world_course_frame() {
        let root = CornerStore.build("");
        let mut slabs = Vec::new();
        brick_slabs(&root, [0.0; 3], &mut slabs);
        assert_eq!(
            slabs.len(),
            8,
            "expected 4 shell slabs + 4 shopfront pieces in brick"
        );

        // Summing the transforms down the tree undoes `assemble`'s rebase
        // exactly, so a walked position IS the authored one the offsets were
        // derived from.
        let expected = |face: FaceKey, pos: &[f32; 3]| face_offset(face, *pos);
        const FACES: [FaceKey; 6] = [
            FaceKey::SideNz,
            FaceKey::SidePz,
            FaceKey::SidePx,
            FaceKey::SideNx,
            FaceKey::Top,
            FaceKey::Bottom,
        ];

        // A base material serves whichever face its slab mostly shows; the
        // record does not say which, so any one of the six is a pass.
        for (pos, m) in &slabs {
            assert!(
                FACES.iter().any(|&f| {
                    let e = expected(f, pos).0;
                    (e[0] - m.uv_offset.0[0]).abs() < 1e-3 && (e[1] - m.uv_offset.0[1]).abs() < 1e-3
                }),
                "slab at {pos:?} carries offset {:?}, which is no face's frame",
                m.uv_offset.0
            );
        }

        // An override names its face, so it is checked exactly.
        let mut overrides = Vec::new();
        brick_faces(&root, [0.0; 3], &mut overrides);
        assert_eq!(
            overrides.len(),
            4,
            "expected the sill, the soffit and both pier returns to wrap — \
             and nothing else, since a vertical corner carries its courses \
             on the base offset alone"
        );
        for (pos, face, m) in &overrides {
            let e = expected(*face, pos).0;
            assert!(
                (e[0] - m.uv_offset.0[0]).abs() < 1e-3 && (e[1] - m.uv_offset.0[1]).abs() < 1e-3,
                "{face:?} of the slab at {pos:?} carries offset {:?}, not its \
                 frame's {e:?}",
                m.uv_offset.0
            );
        }
    }

    /// #966: the courses lie flat. The generator derives its column count as
    /// `scale × aspect_ratio` while `scale` *is* the row count, so under the
    /// metre-square UV tile an aspect above 1 stands every brick on end —
    /// the state this entry was in before the overhaul.
    #[test]
    fn brick_courses_lie_flat() {
        let mut slabs = Vec::new();
        brick_slabs(&CornerStore.build(""), [0.0; 3], &mut slabs);
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
            // A whole-number product is what lets the pattern repeat in V
            // without a visible break every `scale` courses.
            let stagger = cfg.scale.0 * cfg.row_offset.0;
            assert!(
                (stagger - stagger.round()).abs() < 1e-6,
                "slab at {pos:?}: scale × row_offset = {stagger} does not tile"
            );
        }
    }

    /// #968: keep the tile's U seam quiet. The generator colours a brick by
    /// hashing its raw cell index, so the one straddling the seam is hashed
    /// as two different bricks and renders as two half-bricks of different
    /// colour — one per tile per staggered course, which at two columns was
    /// every fourth brick on the wall.
    ///
    /// Neither number can remove the artifact (only the generator could, by
    /// hashing the index modulo the column count); together they dilute it
    /// and drop its contrast until it reads as shading. Tuning either one
    /// back toward the kit defaults brings the split bricks back, so both
    /// are pinned here.
    #[test]
    fn brick_tiling_keeps_seam_splits_subtle() {
        let mut slabs = Vec::new();
        brick_slabs(&CornerStore.build(""), [0.0; 3], &mut slabs);
        for (pos, m) in &slabs {
            let SovereignTextureConfig::Brick(cfg) = &m.texture else {
                unreachable!("filtered to Brick above");
            };
            let cols = (cfg.scale.0 * cfg.aspect_ratio.0).round();
            assert!(
                cols >= 4.0,
                "slab at {pos:?}: {cols} bricks per tile leaves the seam-straddling \
                 brick too large a share of the wall"
            );
            assert!(
                cfg.cell_variance.0 <= 0.15,
                "slab at {pos:?}: cell variance {} makes the seam split read as \
                 two different bricks",
                cfg.cell_variance.0
            );
        }
    }

    /// #968: the shopfront is one wall, so its four brick pieces are one
    /// brick. The riser used to carry the parapet's darker red, which read
    /// as a different material bolted under the window rather than as the
    /// wall carrying on below the sill.
    #[test]
    fn the_shopfront_pieces_share_one_brick_colour() {
        let mut slabs = Vec::new();
        brick_slabs(&CornerStore.build(""), [0.0; 3], &mut slabs);
        // The front face's own slabs: the two piers, the lintel and the
        // riser all sit on the outer face of the front wall.
        let front: Vec<_> = slabs
            .iter()
            .filter(|(pos, _)| (pos[2] - FRONT_MID).abs() < 1e-3)
            .collect();
        assert_eq!(front.len(), 4, "expected piers, lintel and riser");
        for (pos, m) in &front {
            assert_eq!(
                m.base_color.0, BRICK_RED,
                "shopfront slab at {pos:?} breaks the wall's colour"
            );
        }
    }
}
