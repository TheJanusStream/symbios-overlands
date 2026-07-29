//! Greenhouse — a Rural/Farmland secondary. A timber-framed span glasshouse on
//! a fieldstone dwarf wall: four sides and both roof slopes glazed between real
//! glazing bars, boarded gables with louvred vents, staging benches of
//! seedlings under a lamp, and a water butt on the standing apron.
//!
//! Rebuilt as a shell under #972, and it is the entry the alpha-card idiom was
//! *made* for — on a glasshouse the glazing is not decoration applied to a
//! wall, it is the building. What shipped was the exact opposite:
//!
//! 1. **Every pane was a solid.** The back wall, both flanks, the door, the
//!    whole roof and a borrowed `modern_city::curtain_wall` all carried the
//!    `Window` texture on **cuboids** (#972 lesson 20). The generator masks its
//!    panes away, so each was a frame with holes onto the next solid behind it,
//!    and the four-angle sheet showed one opaque green mass under a hipped lid.
//! 2. **The roof was a hip with a plateau.** `cuboid_tapered(.., 0.4, ..)`
//!    pinches *both* axes to a square top: a truncated pyramid 60 % of the plan
//!    across, where a span house has a ridge running its full length.
//! 3. **The fit-out was invisible.** Two benches and twelve trays of seedlings
//!    stood inside a mass nothing could see into, and unlit.
//! 4. **Flat list, no bonding, nothing outside.** Thirty prims hanging off the
//!    base, every stone surface restarting its own course frame, and no way to
//!    reach the door.
//!
//! Now the glazing is seventeen cards on flat quads filling real openings
//! between posts, mullions, purlins and rafters; the ends are boarded gables
//! above the eaves (a glasshouse gable is a triangle and a card is a rectangle,
//! so the honest answer there is boarding and a louvre, not a stretched pane);
//! and the benches, soil, trays and pots stand in a lit interior a metre behind
//! the glass, which is what the masked-away panes actually show.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::solarpunk::{CROP_GREEN, crop_tufts, foliage};
use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat,
    lit_interior, nest, plane, prim, quat_mul, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Fp4, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, GLASS_TINT, LAMP_WARM, STONE_GREY, TRIM_WHITE, WOOD_GREY, clapboard, concrete,
    pane_grid, stone, weathered,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// Plan of the house itself: `L` along X (which is also the ridge), `W` across.
const L: f32 = 8.0;
const W: f32 = 5.0;

/// Poured base the whole thing stands on, and the interior floor level it
/// establishes.
const PAD_W: f32 = L + 0.5;
const PAD_D: f32 = W + 0.5;
const PAD_T: f32 = 0.3;
const FLOOR: f32 = PAD_T;

/// Fieldstone dwarf wall — the course a glasshouse is glazed off.
const KNEE_H: f32 = 0.65;
const KNEE_T: f32 = 0.24;
const KNEE_TOP: f32 = FLOOR + KNEE_H;

/// Glazed height from the dwarf wall up to the eaves plate, and the two levels
/// that fall out of it.
const GLAZE_H: f32 = 2.15;
const EAVE: f32 = KNEE_TOP + GLAZE_H;
const RIDGE_RISE: f32 = 1.35;
const RIDGE: f32 = EAVE + RIDGE_RISE;

/// Square stock of the frame — corner posts, mullions, glazing bars.
const POST: f32 = 0.13;

/// Wall planes, and the frame centre planes just inside them.
const FRONT: f32 = -W * 0.5;
const BACK: f32 = W * 0.5;
const FZ: f32 = FRONT + POST * 0.5;
const BZ: f32 = BACK - POST * 0.5;
const EX: f32 = L * 0.5 - POST * 0.5;

/// How far a pane sits inside the frame's outer face — the putty rebate. It is
/// also what makes a card's lap invisible: the bar's own front face is nearer
/// the camera than the glass it holds (#972 lesson 7).
const REBATE: f32 = 0.035;
/// How far a card oversails its opening on every edge.
const GLAZE_LAP: f32 = 0.06;
/// Target pane size, in metres. Pane counts are the one thing a shared card
/// material cannot know, and they are what tell a viewer how big an opening is
/// — so every opening derives its grid from this rather than inheriting a
/// count sized for something else.
const PANE_M: f32 = 0.62;
// The roof is glazed with the **same masked card as the walls**, and that is a
// decision this rebuild made twice. The card pipeline renders at
// `AlphaMode::Mask(0.5)`, so `glass_opacity` is a binary choice: below the
// cutoff the pane is discarded and the card is a frame with real holes in it;
// above it the pane survives and the card is a sheet. The first render of this
// rebuild looked like an open pergola from above, so the roof was cut at 0.58
// — and an opaque pane over a glasshouse reads as PAINTED PANELS, which is
// worse, because the one thing everybody knows about a glasshouse is that you
// can see through it. The lattice was never the card's fault: it is what a
// glazed roof with too little structure behind it looks like, and the answer is
// the rafters, purlins, ridge and propped lights that are there now.

/// The door, and how far open it stands. A glasshouse with a shut door is a
/// green box with a darker rectangle on it.
const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.05;
const DOOR_SWING: f32 = 0.95;

/// Roof oversail past the eaves wall and past the gables.
const EAVE_OVER: f32 = 0.45;
const GABLE_OVER: f32 = 0.25;

/// Standing apron in front of the door — what anything set down outside
/// actually stands on (#972 lesson 19).
const APRON_D: f32 = 1.4;
const APRON_T: f32 = 0.24;
const APRON_Z: f32 = -(PAD_D * 0.5 + APRON_D * 0.5 - 0.25);
const APRON_TOP: f32 = FLOOR - 0.06;

/// Staging: two benches either side of a central path, their tops just clear
/// of the dwarf wall so the trays read from outside.
const BENCH_Z: f32 = 1.45;
const BENCH_D: f32 = 1.05;
const BENCH_TOP: f32 = FLOOR + 0.82;
const PATH_W: f32 = 1.3;

// --- Palette local to this entry. ------------------------------------------

/// Turned potting soil in the trays.
const SOIL_DARK: [f32; 3] = [0.24, 0.18, 0.13];
/// Terracotta pots along the staging.
const TERRACOTTA: [f32; 3] = [0.62, 0.34, 0.22];
/// Limewashed oak of the water butt and its downpipe — pale enough to read
/// against the dwarf wall it stands beside rather than as a black drum.
const BUTT_OAK: [f32; 3] = [0.66, 0.60, 0.50];
/// Warm interior lining — the potting screen behind the staging.
const LINING_WARM: [f32; 3] = [0.52, 0.42, 0.30];

// --- Derived roof geometry. ------------------------------------------------
//
// The authored quantity is the RISE; the pitch, the slope length and the height
// of the oversailing edge all follow from it, so changing the rise cannot leave
// a rafter, a barge board or a gutter behind. Functions rather than `const`
// because `atan` and `hypot` are not const.

/// Roof pitch, from the rise over the half span.
fn pitch() -> f32 {
    (RIDGE_RISE / (W * 0.5)).atan()
}
/// Half the roof's own span, including the eaves oversail.
fn roof_half() -> f32 {
    W * 0.5 + EAVE_OVER
}
/// Height of the roof's lower edge — **below** the eaves plate, because the
/// slope keeps falling past the wall it oversails.
fn eave_edge_y() -> f32 {
    RIDGE - roof_half() * RIDGE_RISE / (W * 0.5)
}
/// Length of one slope, ridge to oversailing edge.
fn slope_len() -> f32 {
    roof_half().hypot(RIDGE - eave_edge_y())
}
/// Centre of one roof slope, `sz` picking the `−Z` or `+Z` pitch.
fn slope_center(sz: f32) -> [f32; 3] {
    [0.0, (RIDGE + eave_edge_y()) * 0.5, sz * roof_half() * 0.5]
}
/// The turn that lays a quad or a stick **along** one slope, so its local `+Z`
/// runs up the pitch and its local `+Y` is the outward normal.
///
/// `quat_x(θ)` turns `+Y` toward `+Z`, so a prim's local `+Z` end goes *down*
/// for positive θ — see `util::rotate_by`, which is where this family last
/// got the handedness backwards in both the geometry and the guard at once.
fn slope_quat(sz: f32) -> Fp4 {
    quat_x(sz * pitch())
}
/// Unit outward normal of the `sz` slope.
fn slope_normal(sz: f32) -> [f32; 3] {
    let p = pitch();
    [0.0, p.cos(), sz * p.sin()]
}
/// Unit up-slope direction of the `sz` slope — the plane's own local `+Z`.
fn slope_up(sz: f32) -> [f32; 3] {
    let p = pitch();
    [0.0, -sz * p.sin(), p.cos()]
}
/// A point on the `sz` slope: `along` metres from its centre up the pitch,
/// then `off` metres out along its normal (negative goes under the glass).
fn on_slope(sz: f32, along: f32, off: f32) -> [f32; 3] {
    let c = slope_center(sz);
    let u = slope_up(sz);
    let n = slope_normal(sz);
    [
        c[0],
        c[1] + u[1] * along + n[1] * off,
        c[2] + u[2] * along + n[2] * off,
    ]
}

// --- Shared construction. --------------------------------------------------

/// Fieldstone laid in the world's own course frame, so the dwarf wall's stones
/// run through a corner instead of restarting at each segment's centre.
fn knee_mat(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = stone(STONE_GREY);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One segment of the dwarf wall.
///
/// The centre is bound to a local and handed to the material *and* the
/// transform: passing a bonding helper a different reading of "the middle of
/// the wall" is the one way to defeat the frame guard silently (#972
/// lesson 18).
fn knee(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, knee_mat(center, face))),
        center,
        id_quat(),
    )
}

/// A painted timber member of the frame — post, mullion, plate, purlin, cap.
fn timber(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, clapboard(TRIM_WHITE))),
        center,
        id_quat(),
    )
}

/// Concrete laid in the world frame, so the base and the apron share one set
/// of board marks instead of each restarting at its own centre.
fn bonded_concrete(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = concrete(CONCRETE_PALE);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// Which way a pane looks.
///
/// A glasshouse is glazed on every elevation, so unlike the one-hero-face
/// entries this family usually builds, all four uprights are needed — and the
/// `±X` turns are a *composition* rather than a single-axis rotation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Nz,
    Pz,
    Nx,
    Px,
}

impl Look {
    /// The rotation that stands a [`plane`] up facing this way, with the quad's
    /// local Z extent on world `+Y`, so `size` reads as `[width, height]`.
    fn quat(self) -> Fp4 {
        match self {
            Look::Nz => quat_x(-FRAC_PI_2),
            Look::Pz => quat_x(FRAC_PI_2),
            Look::Nx => quat_mul(quat_y(FRAC_PI_2), quat_x(-FRAC_PI_2)),
            Look::Px => quat_mul(quat_y(-FRAC_PI_2), quat_x(-FRAC_PI_2)),
        }
    }
}

/// Panes across and down for an opening, so a 1.5 m sash and an 8 m roof slope
/// get lights of roughly the same size rather than the same *count*.
fn panes(size: [f32; 2]) -> (u32, u32) {
    let n = |m: f32| ((m / PANE_M).round() as u32).clamp(1, 12);
    (n(size[0]), n(size[1]))
}

/// Glazing filling one opening: a card on a flat quad, lapped into the bars
/// either side, on the kit's own glass re-cut to this opening's pane grid.
fn glazing(size: [f32; 2], center: [f32; 3], look: Look) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(GLASS_TINT, 0.22, panes(size)),
        ),
        center,
        look.quat(),
    )
}

/// Glaze the clear openings between a run of frame members.
///
/// `posts` are the member centres along the run, so every bay is *derived*
/// from where the frame actually is and cannot drift from it. `skip` names the
/// one opening that is not glazed — the doorway.
fn glazed_run(
    posts: &[f32],
    y0: f32,
    y1: f32,
    plane_at: f32,
    look: Look,
    skip: Option<usize>,
    out: &mut Vec<Generator>,
) {
    for i in 0..posts.len() - 1 {
        if skip == Some(i) {
            continue;
        }
        let a = posts[i] + POST * 0.5;
        let b = posts[i + 1] - POST * 0.5;
        let size = [b - a, y1 - y0];
        let along = (a + b) * 0.5;
        let y = (y0 + y1) * 0.5;
        let center = match look {
            Look::Nz | Look::Pz => [along, y, plane_at],
            Look::Nx | Look::Px => [plane_at, y, along],
        };
        out.push(glazing(size, center, look));
    }
}

pub struct Greenhouse;

impl CatalogueEntry for Greenhouse {
    fn slug(&self) -> &'static str {
        "greenhouse"
    }
    fn name(&self) -> &'static str {
        "Greenhouse"
    }
    fn description(&self) -> &'static str {
        "Timber-framed span glasshouse on a stone dwarf wall, with staging under glass."
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
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The house as a tree that stands the way it does: the poured base at the
/// bottom, the dwarf wall on it, the floor and the frame on the dwarf wall,
/// the roof on the frame — with the standing apron its own sub-assembly, so
/// everything set down outside is checked against the paving it stands on
/// rather than against the building beside it (#972 lesson 19).
fn build_tree() -> Generator {
    let pad_c = [0.0, PAD_T * 0.5, 0.0];
    let pad = prim(
        solid(cuboid_tapered(
            [PAD_W, PAD_T, PAD_D],
            0.0,
            bonded_concrete(pad_c, FaceKey::Top),
        )),
        pad_c,
        id_quat(),
    );
    // Buried footing under the poured base, so a glasshouse snapped to the
    // high point of a slope keeps its ground under the downhill edge.
    nest(
        pad,
        vec![
            apron(),
            dwarf_wall(),
            footing(PAD_W, PAD_D, [0.0, 0.0], 6.0),
        ],
    )
}

/// The standing apron in front of the door, and everything that stands on it.
fn apron() -> Generator {
    let center = [0.0, APRON_TOP - APRON_T * 0.5, APRON_Z];
    let slab = prim(
        solid(cuboid_tapered(
            [PAD_W, APRON_T, APRON_D],
            0.0,
            bonded_concrete(center, FaceKey::Top),
        )),
        center,
        id_quat(),
    );

    // Water butt under the downpipe, placed from the apron's own extent rather
    // than at a round number measured off the house (#972 lesson 8).
    let butt_r = 0.4;
    let butt_h = 0.9;
    let bx = PAD_W * 0.5 - butt_r - 0.45;
    let bz = APRON_Z + APRON_D * 0.5 - butt_r - 0.05;
    let butt = prim(
        solid(cylinder_tapered(
            butt_r,
            butt_h,
            14,
            0.04,
            weathered(BUTT_OAK),
        )),
        [bx, APRON_TOP + butt_h * 0.5, bz],
        id_quat(),
    );
    let lid = prim(
        solid(cylinder_tapered(
            butt_r * 0.94,
            0.07,
            14,
            0.0,
            weathered(BUTT_OAK),
        )),
        [bx, APRON_TOP + butt_h + 0.035, bz],
        id_quat(),
    );
    // Downpipe: its head comes off the gutter's own height and its foot off the
    // lid it discharges onto, so neither end can be left behind.
    let head = eave_edge_y() - 0.09;
    let foot = APRON_TOP + butt_h;
    let pipe = prim(
        solid(cylinder_tapered(
            0.055,
            head - foot,
            8,
            0.0,
            weathered(BUTT_OAK),
        )),
        [bx, (head + foot) * 0.5, bz],
        id_quat(),
    );

    nest(slab, vec![nest(butt, vec![lid]), pipe])
}

/// The fieldstone dwarf wall: the back run is the sub-root, and the front
/// segments, the flanks, the interior and the whole frame stand on it.
fn dwarf_wall() -> Generator {
    let back_c = [0.0, FLOOR + KNEE_H * 0.5, BACK - KNEE_T * 0.5];
    let root = knee([L, KNEE_H, KNEE_T], back_c, FaceKey::SidePz);

    let mut parts = Vec::new();
    // Front, in two runs either side of the doorway. The dwarf wall stopping
    // at the door is the whole reason the doorway is a way in rather than a
    // panel on a wall.
    for sx in [-1.0_f32, 1.0] {
        let a = sx * DOOR_W * 0.5;
        let b = sx * L * 0.5;
        parts.push(knee(
            [(b - a).abs(), KNEE_H, KNEE_T],
            [(a + b) * 0.5, FLOOR + KNEE_H * 0.5, FRONT + KNEE_T * 0.5],
            FaceKey::SideNz,
        ));
    }
    // Flanks, inset in Z so they butt the front and back runs.
    for sx in [-1.0_f32, 1.0] {
        parts.push(knee(
            [KNEE_T, KNEE_H, W - KNEE_T * 2.0],
            [sx * (L * 0.5 - KNEE_T * 0.5), FLOOR + KNEE_H * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    parts.push(interior());
    parts.push(frame());
    nest(root, parts)
}

// --- The frame, and the glass in it. ---------------------------------------

/// Front frame member centres: corners, two mullions, and the door jambs.
fn front_posts() -> [f32; 6] {
    [-EX, -2.35, -DOOR_W * 0.5, DOOR_W * 0.5, 2.35, EX]
}
/// Back frame member centres — no doorway, so an even four bays.
fn back_posts() -> [f32; 5] {
    [-EX, -2.0, 0.0, 2.0, EX]
}
/// Flank frame member centres, in Z.
fn end_posts() -> [f32; 3] {
    [-(W * 0.5 - POST * 0.5), 0.0, W * 0.5 - POST * 0.5]
}
/// Index of the doorway among [`front_posts`]'s openings.
const DOOR_BAY: usize = 2;

/// The glazed frame: posts, mullions, plates, every card, the door, and the
/// roof that stands on it. The `−X` front corner post is the sub-root — it
/// stands on the dwarf wall and everything above hangs off it.
fn frame() -> Generator {
    let root = timber(
        [POST, EAVE - KNEE_TOP, POST],
        [-EX, (KNEE_TOP + EAVE) * 0.5, FZ],
    );

    let mut parts = Vec::new();
    // Eaves and head plates, all four sides.
    parts.push(timber([L, 0.14, 0.14], [0.0, EAVE - 0.07, FZ]));
    parts.push(timber([L, 0.14, 0.14], [0.0, EAVE - 0.07, BZ]));
    for sx in [-1.0_f32, 1.0] {
        parts.push(timber([0.14, 0.14, W - 0.28], [sx * EX, EAVE - 0.07, 0.0]));
    }

    // Uprights. The two door jambs start at the floor, where the dwarf wall is
    // interrupted; every other post stands on the stone.
    for (i, &x) in front_posts().iter().enumerate() {
        if i == 0 {
            continue; // the sub-root
        }
        let jamb = i == DOOR_BAY || i == DOOR_BAY + 1;
        let y0 = if jamb { FLOOR } else { KNEE_TOP };
        parts.push(timber([POST, EAVE - y0, POST], [x, (y0 + EAVE) * 0.5, FZ]));
    }
    for &x in &back_posts() {
        parts.push(timber(
            [POST, GLAZE_H, POST],
            [x, (KNEE_TOP + EAVE) * 0.5, BZ],
        ));
    }
    // Flank mullions only — the corners are already carried by the front and
    // back runs.
    for sx in [-1.0_f32, 1.0] {
        parts.push(timber(
            [POST, GLAZE_H, POST],
            [sx * EX, (KNEE_TOP + EAVE) * 0.5, 0.0],
        ));
    }

    // Glazing: four elevations, every opening derived from the frame that
    // makes it.
    glazed_run(
        &front_posts(),
        KNEE_TOP,
        EAVE,
        FZ + REBATE,
        Look::Nz,
        Some(DOOR_BAY),
        &mut parts,
    );
    glazed_run(
        &back_posts(),
        KNEE_TOP,
        EAVE,
        BZ - REBATE,
        Look::Pz,
        None,
        &mut parts,
    );
    for sx in [-1.0_f32, 1.0] {
        glazed_run(
            &end_posts(),
            KNEE_TOP,
            EAVE,
            sx * (EX - REBATE),
            if sx > 0.0 { Look::Px } else { Look::Nx },
            None,
            &mut parts,
        );
    }

    doorway(&mut parts);
    parts.push(roof());
    nest(root, parts)
}

/// The doorway: a head beam, a transom light over it, and the boarded leaf
/// standing open against the front.
///
/// The leaf pivots about its **hinge edge**, which is the one point its centre
/// and its rotation both have to agree about, so both are authored from one
/// direction vector (#972 lesson 21). `arm` runs hinge → free edge; `quat_y(φ)`
/// sends the leaf's local `+X` to `(cos φ, 0, −sin φ)`, which is `arm` exactly
/// at `φ = DOOR_SWING`.
fn doorway(parts: &mut Vec<Generator>) {
    let head = FLOOR + DOOR_H;
    parts.push(timber([DOOR_W + 0.3, 0.14, 0.16], [0.0, head + 0.07, FZ]));

    let transom = [DOOR_W - POST, EAVE - head - 0.14];
    parts.push(glazing(
        transom,
        [0.0, (head + 0.14 + EAVE) * 0.5, FZ + REBATE],
        Look::Nz,
    ));

    let hinge = [-DOOR_W * 0.5 + POST * 0.5, FZ - 0.06];
    let arm = [DOOR_SWING.cos(), -DOOR_SWING.sin()];
    let leaf_w = DOOR_W - POST;
    let center = [
        hinge[0] + arm[0] * leaf_w * 0.5,
        FLOOR + (DOOR_H - 0.1) * 0.5,
        hinge[1] + arm[1] * leaf_w * 0.5,
    ];
    parts.push(prim(
        solid(cuboid_tapered(
            [leaf_w, DOOR_H - 0.1, 0.055],
            0.0,
            util::upright_boards(clapboard(TRIM_WHITE)),
        )),
        center,
        quat_y(DOOR_SWING),
    ));
    // Ledge board across the leaf, riding the same turn off the same arm.
    parts.push(prim(
        solid(cuboid_tapered(
            [leaf_w - 0.06, 0.13, 0.04],
            0.0,
            clapboard(WOOD_GREY),
        )),
        [
            center[0] - arm[1] * 0.05,
            FLOOR + 0.55,
            center[2] + arm[0] * 0.05,
        ],
        quat_y(DOOR_SWING),
    ));
}

// --- The roof. -------------------------------------------------------------

/// Ridge beam, and on it both glazed slopes, their rafters and purlins, the
/// boarded gables, the barge boards, the gutters and the ridge lights.
fn roof() -> Generator {
    let root = timber([L + 0.2, 0.17, 0.17], [0.0, RIDGE - 0.085, 0.0]);
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [L + GABLE_OVER * 2.0, 0.1, 0.34],
            0.0,
            clapboard(TRIM_WHITE),
        )),
        [0.0, RIDGE + 0.06, 0.0],
        id_quat(),
    )];

    let sl = slope_len();
    for sz in [-1.0_f32, 1.0] {
        let c = slope_center(sz);
        let size = [L + GABLE_OVER * 2.0, sl];
        parts.push(prim(
            plane(size, pane_grid(GLASS_TINT, 0.22, panes(size))),
            c,
            slope_quat(sz),
        ));
        // Rafters, under the glass and along the slope.
        for k in -2..=2 {
            let at = on_slope(sz, 0.0, -0.075);
            parts.push(prim(
                solid(cuboid_tapered([0.09, 0.08, sl], 0.0, clapboard(TRIM_WHITE))),
                [k as f32 * (L * 0.25), at[1], at[2]],
                slope_quat(sz),
            ));
        }
        // Purlins across them, a fifth of the slope either side of centre.
        for f in [-0.22_f32, 0.22] {
            let at = on_slope(sz, f * sl, -0.14);
            parts.push(timber([L, 0.09, 0.09], [0.0, at[1], at[2]]));
        }
        // Gutter at the slope's own lower edge.
        parts.push(timber(
            [L + GABLE_OVER * 2.0, 0.12, 0.17],
            [0.0, eave_edge_y() - 0.09, sz * (roof_half() - 0.085)],
        ));
    }

    for sx in [-1.0_f32, 1.0] {
        parts.push(gable(sx));
        // Barge boards, taking their tilt from the roof's own pitch.
        for sz in [-1.0_f32, 1.0] {
            let c = slope_center(sz);
            parts.push(prim(
                solid(cuboid_tapered([0.13, 0.19, sl], 0.0, clapboard(TRIM_WHITE))),
                [sx * (L * 0.5 + GABLE_OVER - 0.065), c[1] - 0.13, c[2]],
                slope_quat(sz),
            ));
        }
    }

    // Two ridge lights propped open on the `−Z` slope — the one moving part a
    // glasshouse has, and the reason its ridge is not a solid line.
    for x in [-2.1_f32, 2.1] {
        parts.extend(ridge_vent(x));
    }
    nest(root, parts)
}

/// One boarded gable triangle, with its louvred vent.
///
/// A card is a rectangle and a gable is a triangle, so the honest end to a span
/// house is boarding: `cuboid_tapered_xz` pinched in **Z alone** gives the
/// triangular profile, and the face the approach sees carries the kit's
/// clapboard in the wall's own frame.
fn gable(sx: f32) -> Generator {
    let center = [sx * (L * 0.5 - 0.06), EAVE + RIDGE_RISE * 0.5, 0.0];
    let face = if sx > 0.0 {
        FaceKey::SidePx
    } else {
        FaceKey::SideNx
    };
    let kind = solid(cuboid_tapered_xz(
        [0.12, RIDGE_RISE, W],
        [0.0, 0.99],
        util::bonded_siding(clapboard(TRIM_WHITE), face, center),
    ));

    // Louvre, sized against the gable's half width **at the vent's own head**
    // rather than at the eaves, because the panel it hangs on pinches with
    // height (#972 lesson 16).
    let vent_h = 0.52;
    let vent_y = EAVE + 0.34;
    let frame_c = [sx * (L * 0.5 + 0.02), vent_y, 0.0];
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [0.08, vent_h, 0.74],
            0.0,
            util::bonded_siding(weathered(WOOD_GREY), face, frame_c),
        )),
        frame_c,
        id_quat(),
    )];
    for k in 0..3 {
        parts.push(prim(
            cuboid_tapered([0.11, 0.05, 0.66], 0.0, clapboard(TRIM_WHITE)),
            [
                sx * (L * 0.5 + 0.05),
                vent_y - vent_h * 0.3 + k as f32 * vent_h * 0.3,
                0.0,
            ],
            id_quat(),
        ));
    }
    nest(prim(kind, center, id_quat()), parts)
}

/// A ridge light propped open above the `−Z` slope: the sash frame, its
/// glazing and the stay holding it up.
fn ridge_vent(x: f32) -> Vec<Generator> {
    let open = 0.5;
    // 0.7 m down-slope from the ridge, then lifted along the slope's normal.
    let c = on_slope(-1.0, roof_half() * 0.5 - 0.7, 0.3);
    let size = [1.25_f32, 0.9];
    let q = quat_x(-(pitch() - open));
    vec![
        prim(
            plane(size, pane_grid(GLASS_TINT, 0.22, panes(size))),
            [x, c[1], c[2]],
            q,
        ),
        prim(
            solid(cuboid_tapered(
                [size[0] + 0.1, 0.06, size[1] + 0.1],
                0.0,
                clapboard(TRIM_WHITE),
            )),
            [x, c[1] - 0.05, c[2] + 0.02],
            q,
        ),
        prim(
            cuboid_tapered([0.05, 0.34, 0.05], 0.0, weathered(WOOD_GREY)),
            [x + 0.5, c[1] - 0.2, c[2] + 0.18],
            id_quat(),
        ),
    ]
}

// --- What the panes actually show. -----------------------------------------

/// The lit interior: floor, central path, the two staging benches with their
/// trays and pots, the potting screen behind them, and the lamp under the
/// ridge.
///
/// Laid out **bay by bay** (#972 lesson 9) — the door bay gets the path and the
/// pots rather than a black rectangle, and both flanking bays get a bench whose
/// top stands just clear of the dwarf wall, which is the line a glasshouse
/// actually reads along from outside.
fn interior() -> Generator {
    let floor_c = [0.0, FLOOR + 0.03, 0.0];
    let floor = prim(
        cuboid_tapered(
            [L - KNEE_T * 2.0, 0.06, W - KNEE_T * 2.0],
            0.0,
            lit_interior([0.34, 0.30, 0.25], 0.14),
        ),
        floor_c,
        id_quat(),
    );

    let mut parts = vec![
        // Central path, brighter than the beds either side of it — it is what
        // the open door frames.
        prim(
            cuboid_tapered(
                [L - 1.0, 0.05, PATH_W],
                0.0,
                lit_interior([0.62, 0.58, 0.50], 0.26),
            ),
            [0.0, FLOOR + 0.09, 0.0],
            id_quat(),
        ),
        // Potting screen closing the back of the staging, so the lower part of
        // every pane shows a lit interior rather than daylight straight through
        // the house (#972 lesson 6).
        prim(
            cuboid_tapered([L - 0.6, 1.05, 0.07], 0.0, lit_interior(LINING_WARM, 0.30)),
            [0.0, FLOOR + 0.52, BACK - KNEE_T - 0.1],
            id_quat(),
        ),
    ];
    // A pot shelf on the screen, so it reads as the back of a potting bench
    // from inside and not as a blind from the far elevation.
    parts.push(prim(
        solid(cuboid_tapered(
            [L - 1.2, 0.06, 0.3],
            0.0,
            weathered(WOOD_GREY),
        )),
        [0.0, FLOOR + 1.0, BACK - KNEE_T - 0.22],
        id_quat(),
    ));
    for k in 0..7 {
        let px = (k as f32 - 3.0) * 0.95;
        parts.push(prim(
            solid(cylinder_tapered(0.13, 0.19, 8, 0.3, weathered(TERRACOTTA))),
            [px, FLOOR + 1.12, BACK - KNEE_T - 0.22],
            id_quat(),
        ));
    }
    for sz in [-1.0_f32, 1.0] {
        parts.push(bench(sz));
    }

    // Lamp hung under the ridge, with a hanging basket either side of it.
    parts.push(prim(
        solid(cylinder_tapered(0.03, 0.9, 6, 0.0, weathered(WOOD_GREY))),
        [0.0, RIDGE - 0.55, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.3, 0.2, 0.3], 0.55, glow(LAMP_WARM, 2.4)),
        [0.0, RIDGE - 1.05, 0.0],
        id_quat(),
    ));
    for bx in [-2.6_f32, 2.6] {
        parts.push(prim(
            solid(cylinder_tapered(0.02, 0.7, 6, 0.0, weathered(WOOD_GREY))),
            [bx, RIDGE - 0.75, 0.0],
            id_quat(),
        ));
        parts.push(prim(
            solid(cylinder_tapered(0.26, 0.24, 10, 0.5, weathered(TERRACOTTA))),
            [bx, RIDGE - 1.22, 0.0],
            id_quat(),
        ));
        parts.extend(crop_tufts(
            [bx, RIDGE - 1.14, 0.0],
            [0.3, 0.3],
            2,
            2,
            0.3,
            foliage(CROP_GREEN),
        ));
    }

    nest(floor, parts)
}

/// One staging bench: a leg at the bottom, the other legs and the top on it,
/// and the trays, soil and pots on the top.
fn bench(sz: f32) -> Generator {
    let z = sz * BENCH_Z;
    let leg_x = L * 0.5 - 1.0;
    let leg = |x: f32, lz: f32| {
        prim(
            solid(cuboid_tapered(
                [0.08, BENCH_TOP - FLOOR, 0.08],
                0.0,
                weathered(WOOD_GREY),
            )),
            [x, (FLOOR + BENCH_TOP) * 0.5, lz],
            id_quat(),
        )
    };
    let top_c = [0.0, BENCH_TOP - 0.06, z];
    let top = prim(
        solid(cuboid_tapered(
            [L - 1.5, 0.12, BENCH_D],
            0.0,
            util::bonded_siding(weathered(WOOD_GREY), FaceKey::Top, top_c),
        )),
        top_c,
        id_quat(),
    );

    // Soil tray running the bench, with rows of seedlings in it, and a stand
    // of pots along the path edge where the open leaf frames them.
    let mut load = vec![prim(
        cuboid_tapered(
            [L - 1.9, 0.1, BENCH_D - 0.2],
            0.0,
            lit_interior(SOIL_DARK, 0.10),
        ),
        [0.0, BENCH_TOP + 0.05, z],
        id_quat(),
    )];
    load.extend(crop_tufts(
        [0.0, BENCH_TOP + 0.1, z],
        [L - 2.6, BENCH_D - 0.45],
        8,
        2,
        0.3,
        foliage(CROP_GREEN),
    ));
    for (k, px) in [-0.55_f32, 0.0, 0.55].iter().enumerate() {
        load.push(prim(
            solid(cylinder_tapered(
                0.15,
                0.22 + k as f32 * 0.02,
                10,
                0.32,
                weathered(TERRACOTTA),
            )),
            [*px, BENCH_TOP + 0.11, z - sz * 0.28],
            id_quat(),
        ));
    }

    let mut parts = vec![nest(top, load)];
    for (i, x) in [-leg_x, 0.0, leg_x].iter().enumerate() {
        for (j, lz) in [z - BENCH_D * 0.35, z + BENCH_D * 0.35].iter().enumerate() {
            if i == 0 && j == 0 {
                continue; // the sub-root
            }
            parts.push(leg(*x, *lz));
        }
    }
    nest(leg(-leg_x, z - BENCH_D * 0.35), parts)
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
    /// anything with no footprint a translation-only walk can honestly report.
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
        assert_sanitize_stable(&Greenhouse.build(""), "greenhouse");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&Greenhouse.build(""), "greenhouse");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Greenhouse.build(""), "greenhouse");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&Greenhouse.build(""), "greenhouse");
    }

    #[test]
    fn keeps_its_lamp() {
        assert!(
            has_emissive(&Greenhouse.build("")),
            "the glasshouse lost the lamp under its ridge"
        );
    }

    /// #972 lesson 1: every pane is a card on a flat quad at `uv_scale` 1.0 —
    /// four elevations, both roof slopes, the transom and two ridge lights.
    #[test]
    fn every_pane_is_a_card_on_a_quad() {
        let mut cards = 0;
        walk(&Greenhouse.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "a Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    assert_eq!(
                        m.uv_offset.0,
                        [0.0, 0.0],
                        "a clamp-to-edge card has no world frame to join"
                    );
                    cards += 1;
                }
            }
        });
        // 4 front + 4 back + 2 × 2 flank + transom + 2 slopes + 2 ridge lights.
        assert_eq!(cards, 17, "the glasshouse lost glazing");
    }

    /// Every glazing card is strictly larger than the opening the frame leaves
    /// it, so no edge lands on the reveal plane (#972 lesson 7) — checked
    /// against the *frame member centres*, which is where the opening actually
    /// comes from.
    #[test]
    fn every_card_laps_into_its_frame() {
        let root = Greenhouse.build("");
        let mut widths: Vec<f32> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, _| {
            if let GeneratorKind::Plane { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Window(_))
            {
                widths.push(size.0[0]);
            }
        });
        let posts = front_posts();
        for i in 0..posts.len() - 1 {
            if i == DOOR_BAY {
                continue;
            }
            let clear = posts[i + 1] - posts[i] - POST;
            assert!(
                widths.iter().any(|w| (w - clear - GLAZE_LAP).abs() < 1e-4),
                "no card laps the {clear} m opening between the front posts"
            );
            assert!(
                !widths.iter().any(|w| (w - clear).abs() < 1e-4),
                "a card sized exactly to a {clear} m opening ties with its own reveal"
            );
        }
    }

    /// #972 lesson 18: a bonded surface's material centre and its placement are
    /// one expression, so every cladding slab's `uv_offset` must be some face's
    /// projection of the position the **built tree** puts it at — read from the
    /// composed translation, not from the constants the placement used (#972
    /// lesson 21).
    #[test]
    fn every_clad_surface_shares_one_world_frame() {
        use FaceKey::*;
        let mut checked = 0;
        walk(&Greenhouse.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            // A turned slab has no shared face frame, and small stock (posts,
            // bars, rails, plates, gutters) is not a coursed surface. Select by
            // what defines a cladding slab: a run of real wall, and a second
            // dimension that is a surface rather than a stick.
            //
            // A first draft asked for two dimensions over 0.9 and found six of
            // eleven — it had quietly excluded the whole dwarf wall, which is
            // 0.65 m high and the most coursed surface on the building. Suspect
            // the selector before the content (#972 lesson 24).
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] {
                return;
            }
            let mut dims = size.0;
            dims.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if dims[2] < 1.5
                || dims[1] < 0.5
                || !matches!(
                    material.texture,
                    SovereignTextureConfig::Cobblestone(_)
                        | SovereignTextureConfig::Plank(_)
                        | SovereignTextureConfig::Concrete(_)
                )
            {
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
                "greenhouse: a clad slab at {at:?} carries uv_offset {:?}, which is no \
                 face's projection of where the built tree puts it — its material \
                 centre and its placement are two different expressions",
                material.uv_offset.0
            );
        });
        assert_eq!(
            checked, 11,
            "the base, the apron, five dwarf-wall runs, two gables and two bench tops — \
             {checked} found, so suspect the selector before the content"
        );
    }

    /// #972 lesson 8: everything that stands on the poured base has its
    /// footprint inside the base's, expressed against the pad's own extent.
    #[test]
    fn everything_standing_on_the_pad_is_on_it() {
        let mut checked = 0;
        walk(&Greenhouse.build(""), [0.0; 3], &mut |g, at| {
            let Some((hx, hy, hz)) = footprint(g) else {
                return;
            };
            if (at[1] - hy - FLOOR).abs() > 0.03 {
                return;
            }
            checked += 1;
            assert!(
                at[0].abs() + hx <= PAD_W * 0.5 + 1e-3 && at[2].abs() + hz <= PAD_D * 0.5 + 1e-3,
                "greenhouse: a part at {at:?} (half {hx} × {hz}) stands on the base and \
                 hangs off it"
            );
        });
        assert!(
            checked >= 6,
            "only {checked} parts found standing on the base"
        );
    }

    /// #972 lesson 19: the sub-root **is** the surface, so every descendant of
    /// the apron is checked against the apron rather than against the building
    /// it happens to stand beside.
    #[test]
    fn everything_on_the_apron_is_on_the_apron() {
        let root = Greenhouse.build("");
        let base = root.transform.translation.0;
        let apron = root
            .children
            .iter()
            .find(|c| c.transform.translation.0[2] < -1.0)
            .expect("the base carries the apron");
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
                    at[0].abs() + hx <= PAD_W * 0.5 + 1e-3
                        && (at[2] - APRON_Z).abs() + hz <= APRON_D * 0.5 + 1e-3,
                    "greenhouse: a part at {at:?} hangs off the apron it stands on"
                );
            });
        }
        assert!(n >= 3, "only {n} parts on the apron");
    }

    /// #972 lessons 21 and 23: the leaf's centre and its rotation are two
    /// decisions that must agree, and the only point both have to be right
    /// about is the **hinge edge**. Read the built node's own quaternion and
    /// half-extent and turn it with the one shared [`rotate_by`], rather than
    /// re-deriving the free edge from the constants the placement used.
    #[test]
    fn the_door_is_hung_on_its_jamb_and_stands_open() {
        let root = Greenhouse.build("");
        let mut leaf: Option<([f32; 3], [f32; 4], f32)> = None;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            // The leaf is the one turned slab of door height.
            if g.transform.rotation.0[1].abs() > 1e-3 && size.0[1] > 1.5 {
                leaf = Some((at, g.transform.rotation.0, size.0[0] * 0.5));
            }
        });
        let (at, q, half) = leaf.expect("no door leaf in the tree");
        let arm = rotate_by(q, [half, 0.0, 0.0]);
        let ends = [
            [at[0] - arm[0], at[2] - arm[2]],
            [at[0] + arm[0], at[2] + arm[2]],
        ];
        let hinge = [-DOOR_W * 0.5 + POST * 0.5, FZ - 0.06];
        assert!(
            ends.iter()
                .any(|e| (e[0] - hinge[0]).abs() < 0.02 && (e[1] - hinge[1]).abs() < 0.02),
            "greenhouse: the leaf's ends are at {ends:?}, neither on the hinge at \
             {hinge:?} — the door is hung on nothing"
        );
        let free = ends
            .iter()
            .find(|e| (e[0] - hinge[0]).abs() >= 0.02 || (e[1] - hinge[1]).abs() >= 0.02)
            .unwrap();
        assert!(
            free[1] < FZ - 0.5,
            "greenhouse: the leaf's free edge at z {} is still on the wall — a shut \
             door is a darker rectangle, not a way in",
            free[1]
        );
    }

    /// The roof's authored quantity is the RISE; the pitch, the slope length
    /// and the oversailing edge all follow. Read each slope out of the built
    /// tree, turn its own half-extent with its own quaternion, and assert the
    /// two ends land on the ridge and on the eave the rise implies — which is
    /// what catches a slope tilted the wrong way, the failure that agreed with
    /// its own guard on the lifeguard tower (#972 lesson 23).
    #[test]
    fn both_slopes_run_from_the_ridge_to_the_eave() {
        let root = Greenhouse.build("");
        let mut slopes: Vec<([f32; 3], [f32; 4], f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Plane { size, .. } = &g.kind else {
                return;
            };
            // Select by span: the slopes are the only quads as wide as the
            // house, and a ridge light is also a tilted card (#972 lesson 24).
            if size.0[0] > L {
                slopes.push((at, g.transform.rotation.0, size.0[1] * 0.5));
            }
        });
        assert_eq!(slopes.len(), 2, "a span roof has two slopes");
        for (at, q, half) in slopes {
            let arm = rotate_by(q, [0.0, 0.0, half]);
            let a = [at[1] + arm[1], at[2] + arm[2]];
            let b = [at[1] - arm[1], at[2] - arm[2]];
            let (top, bot) = if a[0] > b[0] { (a, b) } else { (b, a) };
            assert!(
                (top[0] - RIDGE).abs() < 2e-3 && top[1].abs() < 2e-3,
                "a slope's upper edge is at {top:?}, not on the ridge at {RIDGE}"
            );
            assert!(
                (bot[0] - eave_edge_y()).abs() < 2e-3 && (bot[1].abs() - roof_half()).abs() < 2e-3,
                "a slope's lower edge is at {bot:?}, not the eave its rise implies \
                 ({}, {})",
                eave_edge_y(),
                roof_half()
            );
        }
    }

    /// #972 lesson 16: the gable is a triangle, so a part hung on it has to be
    /// checked against the panel's half width **at the part's own head**, not
    /// at the eaves where the panel is widest.
    #[test]
    fn the_louvre_fits_the_gable_at_its_own_head() {
        let mut vents = 0;
        walk(&Greenhouse.build(""), [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, material, .. } = &g.kind else {
                return;
            };
            // The louvre frame: an upright board of vent height, standing
            // outside the gable plane above the eaves.
            if at[1] < EAVE
                || at[0].abs() < L * 0.5
                || size.0[1] < 0.3
                || g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0]
                || !matches!(material.texture, SovereignTextureConfig::Plank(_))
            {
                return;
            }
            vents += 1;
            let head = at[1] + size.0[1] * 0.5;
            let half_at_head = W * 0.5 * (1.0 - (head - EAVE) / RIDGE_RISE);
            assert!(
                size.0[2] * 0.5 <= half_at_head,
                "greenhouse: a {} m louvre with its head at {head} is wider than the \
                 {half_at_head} m of gable left there",
                size.0[2]
            );
        });
        assert_eq!(vents, 2, "one louvre per gable");
    }

    /// The editability contract (#972 lesson 3): the base carries the apron and
    /// the dwarf wall, the dwarf wall carries the interior and the frame, the
    /// frame carries the roof, the roof carries its gables.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = Greenhouse.build("");
        assert_eq!(
            root.children.len(),
            3,
            "the base carries apron + dwarf wall + buried footing"
        );
        let knee = root
            .children
            .iter()
            .max_by_key(|c| count(c))
            .expect("a dwarf wall");
        // Select each sub-root by the property that *defines* it — the frame
        // hangs off a corner post, the roof off the ridge beam. Counting
        // children instead picked the interior the moment it gained a pot
        // shelf, which is #972 lesson 24 arriving on schedule.
        let sized = |g: &Generator, want: [f32; 3]| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => size
                .0
                .iter()
                .zip(want.iter())
                .all(|(a, b)| (a - b).abs() < 1e-3),
            _ => false,
        };
        let frame = knee
            .children
            .iter()
            .find(|c| sized(c, [POST, EAVE - KNEE_TOP, POST]))
            .expect("the dwarf wall carries the frame's corner post");
        let roof = frame
            .children
            .iter()
            .find(|c| sized(c, [L + 0.2, 0.17, 0.17]))
            .expect("the frame carries the roof's ridge beam");
        assert!(
            roof.children.len() > 25,
            "the ridge carries both slopes, their bars, the gables and the vents"
        );
        assert!(
            roof.children.iter().any(|c| !c.children.is_empty()),
            "the roof carries gables that carry their own louvres"
        );
        assert!(count(&root) > 110, "the glasshouse lost most of its parts");
    }
}
