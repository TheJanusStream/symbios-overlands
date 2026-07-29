//! Grand hotel — the Coastal-Resort landmark and the kit's lit hero. A
//! whitewashed stucco block of three storeys, its seafront elevation cut into
//! five bays by full-height pilasters: a lobby arcade at terrace level under a
//! striped awning, and above it two floors of French doors opening onto
//! continuous balconies. A rooftop sign reads across the bay at dusk, and the
//! pool terrace runs off the podium's own edge.
//!
//! Rebuilt as a **shell** under the standing lessons of #972. What it was:
//!
//! 1. **A lit glass box for a lobby.** The ground floor used the modern-city
//!    [`curtain_wall`](crate::catalogue::items::modern_city::curtain_wall),
//!    which is a lit glass *cuboid* behind proud fins — fine on the tower it
//!    was written for and, as that helper's own note says, wrong at eye level,
//!    because handed a `Window` texture the one thing it cannot be is a
//!    window. It had no entrance at all, and nothing behind the glass.
//! 2. **Balcony doors and side windows stuck on solid walls.** Both storeys'
//!    "lit glass doors" were one 11 m `Window`-textured slab laid on the
//!    façade, and each flank carried a 5.5 m glass slab; the generator masks
//!    its panes *away*, so all four were frames with holes onto the stucco
//!    behind them.
//! 3. **A pool three and a half metres out to sea.** The terrace was placed
//!    at a round number measured off the building, which left a gap of bare
//!    ground between the podium and the deck — the #972 lesson-8 failure, and
//!    invisible unless a tile looks along that edge.
//!
//! What it is: five bays of real openings with cards in their reveals and lit
//! rooms behind them, a glazed entrance on the centre bay over a lobby laid
//! out bay by bay, balconies with baluster railings, and a terrace derived
//! from the podium it runs off.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lit_interior, nest,
    plane, prim, quat_mul, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, POOL_AQUA, SIGN_AMBER, SIGN_GOLD, STEEL_GREY, STUCCO_SAND,
    STUCCO_WHITE, canvas, concrete, fx, pane_grid, steel, stucco, water,
};

// --- Shell dimensions. Everything below derives from these. ----------------

/// Body width (X) and depth (Z).
const W: f32 = 15.0;
const D: f32 = 10.0;
/// Podium height — the terrace level, and the datum every storey is measured
/// from.
const PODIUM_H: f32 = 0.7;
/// How far the podium oversails the body on every side. The terrace, the
/// awning poles and the pool deck all derive from this rather than from the
/// building.
const PODIUM_OVER: f32 = 1.6;
/// Wall thickness, and so the depth of every reveal.
const WALL_T: f32 = 0.32;

/// Ground (lobby) storey height above the podium, and the two guest storeys.
const LOBBY_H: f32 = 3.7;
const STOREY: f32 = 3.2;
/// Top of the stucco above the podium, and the parapet that caps it.
const PLATE: f32 = LOBBY_H + STOREY * 2.0;
const PARAPET_H: f32 = 1.0;

/// Outer face of the seafront elevation — the `-Z` hero direction the render
/// tool and the settlement placer both look down.
const FRONT: f32 = -D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// How far the spandrel bands sit back from the pilasters' plane.
const RECESS: f32 = 0.07;
/// Glazing plane, set back inside the reveal so the wall's thickness reads.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.7;
/// Where a room panel stands behind an opening.
const ROOM_Z: f32 = FRONT + 0.75;
/// Centre plane of proud trim — cornices, bands, casings.
const TRIM_Z: f32 = FRONT - 0.05;

/// Bay centres in X. Five bays at a 2.7 m pitch leave every pilaster exactly
/// 1.2 m wide, corners included.
const BAY_X: [f32; 5] = [-5.4, -2.7, 0.0, 2.7, 5.4];
/// The centre bay is the entrance at lobby level.
const DOOR_BAY: usize = 2;
/// Every opening on the hero face is this wide.
const OPEN_W: f32 = 1.5;
/// Lobby opening: sill and head above the podium. The entrance bay has no
/// sill, so the doors reach the floor.
const LOBBY_SILL: f32 = 0.75;
const LOBBY_HEAD: f32 = 3.0;
/// A French door runs from its balcony floor to this head above it.
const FRENCH_H: f32 = 2.35;

/// Balcony slab depth and the railing height on it.
const BALC_D: f32 = 1.25;
const RAIL_H: f32 = 1.0;

// --- Palette local to this entry. ------------------------------------------

/// Window joinery — the painted white frames the cards carry.
const JOINERY: [f32; 3] = [0.95, 0.94, 0.9];
/// Guest rooms behind the French doors: a warm lamplit interior.
const ROOM_WARM: [f32; 3] = [0.66, 0.54, 0.36];

// --- Shared construction. --------------------------------------------------

/// Whitewashed stucco laid in the wall's own frame. Stucco is near-scaleless,
/// so the offset buys little on its own — but keeping every slab in one frame
/// is what stops the render's grain stepping at each wall break, and it costs
/// one call.
fn render_mat(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = stucco(color);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One stucco slab of the shell.
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            render_mat(STUCCO_WHITE, center, face),
        )),
        center,
        id_quat(),
    )
}

/// A proud sand-stucco band — cornice, string course, casing, coping. Always
/// oversized against what it laps and always standing off the surface it
/// laps, so it never shares a plane with its host.
fn band(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            render_mat(STUCCO_SAND, center, FaceKey::SideNz),
        )),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Clear glazing filling one bay, on a flat quad at [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(JOINERY, 0.0, panes),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// The same card turned onto a flank. `sx` is the side it faces; `size` still
/// reads as `[width, height]`, with the width running along the building.
fn side_glazing(size: [f32; 2], center: [f32; 3], sx: f32) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(JOINERY, 0.0, (2, 5)),
        ),
        center,
        quat_mul(quat_y(-sx * FRAC_PI_2), quat_x(-FRAC_PI_2)),
    )
}

/// A lit room behind one opening — the surface a card's masked-away panes
/// actually show. Nothing lights the inside of an enclosed prop, so these
/// carry a low self-lit term of their own.
fn room(size: [f32; 2], center: [f32; 3], color: [f32; 3], lit: f32) -> Generator {
    prim(
        cuboid_tapered([size[0], size[1], 0.1], 0.0, lit_interior(color, lit)),
        center,
        id_quat(),
    )
}

/// Every glazed opening on the hero face: bay centre, sill and head above the
/// podium, and the pane grid it wants.
///
/// One list, because the elevation, the glazing, the rooms and the guards all
/// have to agree about where the holes are.
fn openings() -> Vec<(f32, f32, f32, (u32, u32))> {
    let mut out = Vec::new();
    for (b, &x) in BAY_X.iter().enumerate() {
        let sill = if b == DOOR_BAY { 0.0 } else { LOBBY_SILL };
        out.push((x, sill, LOBBY_HEAD, (3, 3)));
        for f in 0..2 {
            let floor = LOBBY_H + STOREY * f as f32;
            out.push((x, floor + 0.1, floor + 0.1 + FRENCH_H, (2, 4)));
        }
    }
    out
}

pub struct GrandHotel;

impl CatalogueEntry for GrandHotel {
    fn slug(&self) -> &'static str {
        "grand_hotel"
    }
    fn name(&self) -> &'static str {
        "Grand Hotel"
    }
    fn description(&self) -> &'static str {
        "Whitewashed seafront hotel with tiered balconies, a lit lobby and a glowing rooftop sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 15.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The hotel as a tree that stands the way it does: the podium at the bottom,
/// the shell on it (carrying the balconies, the parapet and the sign), and
/// the pool terrace as its own sub-assembly off the podium's edge.
///
/// Written outermost-last, because [`nest`] rebases a subtree that already
/// carries its own world translation.
fn build_tree() -> Generator {
    let podium = prim(
        solid(cuboid_tapered(
            [W + PODIUM_OVER * 2.0, PODIUM_H, D + PODIUM_OVER * 2.0],
            0.0,
            render_mat(STUCCO_SAND, [0.0, PODIUM_H * 0.5, 0.0], FaceKey::Top),
        )),
        [0.0, PODIUM_H * 0.5, 0.0],
        id_quat(),
    );
    let mut root = nest(
        podium,
        vec![
            // Buried plinth so a terrain-snapped placement shows footing on a
            // slope rather than daylight under its downhill edge. Its depth is
            // sized to the drop this footprint spans rather than picked by eye
            // (#1009).
            footing(
                W + PODIUM_OVER * 2.0,
                D + PODIUM_OVER * 2.0,
                [0.0, 0.0],
                15.0,
            ),
            shell(),
            terrace(),
        ],
    );
    // Signature life: a soft sea breeze breathing over the frontage.
    root.audio = fx::sea_breeze();
    root
}

// --- The shell. ------------------------------------------------------------

/// Lobby floor, and on it everything the hotel is: the stucco that frames the
/// openings, the glazing, the rooms behind it, the balconies, and — on the
/// plate — the cornice, the parapet and the sign.
fn shell() -> Generator {
    let mut parts = Vec::new();
    let mid_y = PODIUM_H + PLATE * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Back wall — solid; only the seafront is cut.
    parts.push(wall(
        [W, PLATE, WALL_T],
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    flanks(&mut parts, inner_d);
    seafront(&mut parts);
    lobby_fitout(&mut parts);

    for (x, sill, head, panes) in openings() {
        let cy = PODIUM_H + (sill + head) * 0.5;
        parts.push(glazing([OPEN_W, head - sill], [x, cy, GLAZE_Z], panes));
        // Lobby bays get a pale daylit interior; guest rooms a warm lamplit
        // one, so the elevation reads as a hotel at dusk rather than as one
        // uniform tone (#972 lesson 9).
        let (color, lit) = if head <= LOBBY_HEAD {
            ([0.74, 0.70, 0.60], 0.4)
        } else {
            (ROOM_WARM, 0.5)
        };
        parts.push(room(
            [OPEN_W + 0.5, head - sill + 0.5],
            [x, cy, ROOM_Z],
            color,
            lit,
        ));
    }

    parts.push(cornice());
    for f in 0..2 {
        parts.push(balcony(LOBBY_H + STOREY * f as f32));
    }
    parts.push(awning());

    let floor = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.12, inner_d],
            0.0,
            lit_interior([0.62, 0.56, 0.44], 0.28),
        ),
        [0.0, PODIUM_H + 0.06, 0.0],
        id_quat(),
    );
    nest(floor, parts)
}

/// The two flanks, each framing one tall stair window that spans both guest
/// storeys.
///
/// One opening a side rather than a punched grid: a stair window is what a
/// hotel actually shows on its gable, and it is four wall slabs instead of the
/// twenty a bay-by-bay flank would cost. The 5.5 m glass slabs it replaces
/// were `Window` cards laid on solid stucco.
fn flanks(parts: &mut Vec<Generator>, inner_d: f32) {
    let sill = LOBBY_H + 0.6;
    let head = PLATE - 0.9;
    let (za, zb) = (-0.9_f32, 0.9_f32);
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * (W * 0.5 - WALL_T * 0.5);
        let face = if sx > 0.0 {
            FaceKey::SidePx
        } else {
            FaceKey::SideNx
        };
        // Fore and aft of the opening, full height.
        for (a, b) in [(-inner_d * 0.5, za), (zb, inner_d * 0.5)] {
            parts.push(wall(
                [WALL_T, PLATE, b - a],
                [cx, PODIUM_H + PLATE * 0.5, (a + b) * 0.5],
                face,
            ));
        }
        // Under and over it.
        parts.push(wall(
            [WALL_T, sill, zb - za],
            [cx, PODIUM_H + sill * 0.5, 0.0],
            face,
        ));
        parts.push(wall(
            [WALL_T, PLATE - head, zb - za],
            [cx, PODIUM_H + (head + PLATE) * 0.5, 0.0],
            face,
        ));
        let cy = PODIUM_H + (sill + head) * 0.5;
        parts.push(side_glazing(
            [zb - za, head - sill],
            [cx - sx * 0.08, cy, 0.0],
            sx,
        ));
        parts.push(prim(
            cuboid_tapered(
                [0.1, head - sill + 0.5, zb - za + 0.5],
                0.0,
                lit_interior([0.70, 0.62, 0.46], 0.34),
            ),
            [cx + sx * 0.4, cy, 0.0],
            id_quat(),
        ));
    }
}

/// The hero elevation: six full-height pilasters standing [`RECESS`] proud of
/// the spandrel bands behind them, framing five bays of openings on three
/// levels.
///
/// The same scheme the tenement uses, and for the same reason: a five-bay,
/// three-storey grid framed slab-by-slab is thirty-odd wall pieces, where
/// continuous pilasters over recessed spandrels is nine — and on a stucco
/// building the pilaster order is what the elevation is *about*.
fn seafront(parts: &mut Vec<Generator>) {
    let mut edges = vec![-W * 0.5];
    for &x in BAY_X.iter() {
        edges.push(x - OPEN_W * 0.5);
        edges.push(x + OPEN_W * 0.5);
    }
    edges.push(W * 0.5);
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        parts.push(wall(
            [b - a, PLATE, WALL_T],
            [(a + b) * 0.5, PODIUM_H + PLATE * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Recessed spandrel bands behind the pilasters, full width. `spandrel` is
    // (low, high) above the podium.
    let spandrel = |lo: f32, hi: f32| {
        wall(
            [W - 0.24, hi - lo, WALL_T],
            [0.0, PODIUM_H + (lo + hi) * 0.5, FRONT_MID + RECESS],
            FaceKey::SideNz,
        )
    };
    // Under the lobby windows (the entrance bay is cut out of it below), over
    // them to the first balcony, between the storeys, and the frieze on top.
    for (b, &x) in BAY_X.iter().enumerate() {
        if b == DOOR_BAY {
            continue;
        }
        parts.push(wall(
            [OPEN_W, LOBBY_SILL, WALL_T],
            [x, PODIUM_H + LOBBY_SILL * 0.5, FRONT_MID + RECESS],
            FaceKey::SideNz,
        ));
    }
    parts.push(spandrel(LOBBY_HEAD, LOBBY_H + 0.1));
    parts.push(spandrel(LOBBY_H + 0.1 + FRENCH_H, LOBBY_H + STOREY + 0.1));
    parts.push(spandrel(LOBBY_H + STOREY + 0.1 + FRENCH_H, PLATE));

    // A string course at each balcony line, proud of the pilasters, and the
    // casing head over the entrance.
    for f in 0..2 {
        let y = PODIUM_H + LOBBY_H + STOREY * f as f32;
        parts.push(band(
            [W + 0.3, 0.2, WALL_T * 0.9],
            [0.0, y - 0.1, TRIM_Z + 0.05],
        ));
    }
    parts.push(band(
        [OPEN_W + 0.7, 0.24, 0.34],
        [BAY_X[DOOR_BAY], PODIUM_H + LOBBY_HEAD + 0.12, TRIM_Z],
    ));
}

/// The lobby behind the arcade: a lit reception counter in the entrance bay
/// and a lounge group in the bays either side.
///
/// #972 lesson 9 — a fit-out authored for "the lobby" leaves whichever bay it
/// was not written for a black rectangle beside one that reads beautifully.
/// This is laid out bay by bay.
fn lobby_fitout(parts: &mut Vec<Generator>) {
    let floor = PODIUM_H;
    // Reception counter, centred on the entrance so it is what the doors
    // frame.
    parts.push(prim(
        solid(cuboid_tapered(
            [3.0, 1.05, 0.7],
            0.0,
            lit_interior([0.48, 0.36, 0.24], 0.3),
        )),
        [BAY_X[DOOR_BAY], floor + 0.52, ROOM_Z + 0.9],
        id_quat(),
    ));
    // Lounge seating in the outer bays, low enough to read under the heads.
    for &x in [BAY_X[0], BAY_X[4]].iter() {
        parts.push(prim(
            solid(cuboid_tapered(
                [2.0, 0.7, 0.8],
                0.0,
                lit_interior([0.40, 0.34, 0.30], 0.26),
            )),
            [x, floor + 0.35, ROOM_Z + 0.6],
            id_quat(),
        ));
    }
    // Potted palms in the two remaining bays, so no bay is left empty.
    for &x in [BAY_X[1], BAY_X[3]].iter() {
        parts.push(prim(
            solid(cylinder_tapered(
                0.3,
                0.6,
                10,
                0.15,
                lit_interior([0.46, 0.38, 0.28], 0.24),
            )),
            [x, floor + 0.3, ROOM_Z + 0.5],
            id_quat(),
        ));
        parts.push(prim(
            cone(0.62, 1.15, 8, lit_interior([0.24, 0.36, 0.24], 0.3)),
            [x, floor + 1.2, ROOM_Z + 0.5],
            id_quat(),
        ));
    }
    // A warm ceiling wash across the whole lobby — one strip, so every bay
    // gets the same light rather than one bay getting all of it.
    parts.push(prim(
        cuboid_tapered([W - 2.0, 0.14, 1.2], 0.0, glow(SIGN_GOLD, 1.4)),
        [0.0, floor + LOBBY_HEAD - 0.35, ROOM_Z + 0.5],
        id_quat(),
    ));
    // Glazed entrance leaves, proud of the arcade glazing so they read in
    // front of it, lapped below the floor so no edge is coplanar with it.
    parts.push(prim(
        plane(
            [OPEN_W + 0.2, LOBBY_HEAD - 0.55],
            pane_grid([0.86, 0.85, 0.8], 0.0, (2, 3)),
        ),
        [
            BAY_X[DOOR_BAY],
            floor + (LOBBY_HEAD - 0.55) * 0.5 - 0.04,
            FRONT - 0.16,
        ],
        quat_x(-FRAC_PI_2),
    ));
}

/// One continuous seafront balcony at `y` above the podium: the slab, a
/// baluster railing on it, and the underside soffit.
///
/// A railing is a *railing* — a top rail, a bottom rail and balusters. The
/// single 0.55 m plate this replaces read as a parapet wall and hid the
/// French doors behind it, which is the one thing a balcony must not do.
fn balcony(y: f32) -> Generator {
    let base = PODIUM_H + y;
    let bz = FRONT - BALC_D * 0.5;
    let w = W - 1.0;
    let mut parts = Vec::new();
    for ry in [0.18_f32, RAIL_H - 0.06] {
        parts.push(prim(
            cuboid_tapered([w, 0.09, 0.1], 0.0, steel(STEEL_GREY)),
            [0.0, base + 0.12 + ry, FRONT - BALC_D + 0.12],
            id_quat(),
        ));
    }
    let n = 22;
    for i in 0..n {
        let x = -w * 0.5 + w * (i as f32 + 0.5) / n as f32;
        parts.push(prim(
            cuboid_tapered([0.05, RAIL_H - 0.1, 0.05], 0.0, steel(STEEL_GREY)),
            [x, base + 0.12 + RAIL_H * 0.5 - 0.03, FRONT - BALC_D + 0.12],
            id_quat(),
        ));
    }
    // End posts, a touch heavier than the balusters.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.1, RAIL_H, 0.1], 0.0, steel(STEEL_GREY))),
            [
                sx * w * 0.5,
                base + 0.12 + RAIL_H * 0.5,
                FRONT - BALC_D + 0.12,
            ],
            id_quat(),
        ));
    }
    let slab = prim(
        solid(cuboid_tapered(
            [w, 0.24, BALC_D],
            0.0,
            render_mat(STUCCO_WHITE, [0.0, base, bz], FaceKey::Top),
        )),
        [0.0, base, bz],
        id_quat(),
    );
    nest(slab, parts)
}

/// Depth of the entrance awning, and how far its poles stand in from its own
/// leading edge. Both feed [`awning`]; the pole position is derived so the
/// feet cannot end up past the podium they stand on.
const AWNING_D: f32 = 2.6;
const AWNING_POLE_IN: f32 = 0.45;

/// The striped entrance awning on two poles, slung over the centre bay.
///
/// The poles are placed from the podium's own edge rather than from the
/// canopy's: slung far enough out, their feet land beyond the paving and the
/// awning stands on nothing — a half-metre float that no head-on angle shows,
/// because the canopy is directly above it.
fn awning() -> Generator {
    let y = PODIUM_H + LOBBY_HEAD + 0.5;
    let podium_front = -(D * 0.5 + PODIUM_OVER);
    let pole_z = podium_front + 0.25;
    let z = pole_z + AWNING_D * 0.5 - AWNING_POLE_IN;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.11, y - PODIUM_H, 0.11],
                0.0,
                steel(STEEL_GREY),
            )),
            [sx * 2.2, PODIUM_H + (y - PODIUM_H) * 0.5, pole_z],
            id_quat(),
        ));
    }
    // Scalloped valance at the leading edge — the one thing that stops a
    // canopy reading as a flat red rectangle at this size.
    parts.push(prim(
        cuboid_tapered([5.2, 0.3, 0.08], 0.12, canvas(AWNING_WHITE, AWNING_RED)),
        [0.0, y - AWNING_D * 0.5 * 0.26 - 0.14, z - AWNING_D * 0.5],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([5.2, 0.18, AWNING_D], 0.0, canvas(AWNING_RED, AWNING_WHITE)),
        [0.0, y, z],
        quat_x(-0.26),
    ));

    // The sub-root is the head rail across the poles, **not** the canopy. A
    // tilted sub-root spins everything nested under it, so hanging the poles
    // off the sloping canvas turned them 15° and slid their feet off the
    // podium — and the footprint guard, which walks translations only,
    // reported them exactly where they were authored. Both the render and the
    // check agreed with a record that was wrong.
    let head = prim(
        solid(cuboid_tapered([5.4, 0.14, 0.14], 0.0, steel(STEEL_GREY))),
        [0.0, y + 0.12, pole_z],
        id_quat(),
    );
    nest(head, parts)
}

/// The cornice, and everything the roof carries: the parapet ring, its
/// coping, and the rooftop sign.
fn cornice() -> Generator {
    let y = PODIUM_H + PLATE;
    let corona = band([W + 0.7, 0.34, D + 0.7], [0.0, y + 0.17, 0.0]);
    let mut parts = Vec::new();
    // Parapet ring: four walls, each with its own coping, rather than one
    // slab across the roof — a cap would hide the deck from every angle the
    // contact sheet takes.
    let p_t = 0.34;
    let top = y + 0.34;
    for sz in [-1.0_f32, 1.0] {
        let cz = sz * (D * 0.5 + 0.1 - p_t * 0.5);
        parts.push(wall(
            [W + 0.2, PARAPET_H, p_t],
            [0.0, top + PARAPET_H * 0.5, cz],
            if sz > 0.0 {
                FaceKey::SidePz
            } else {
                FaceKey::SideNz
            },
        ));
        parts.push(band(
            [W + 0.42, 0.14, p_t + 0.18],
            [0.0, top + PARAPET_H + 0.07, cz],
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * (W * 0.5 + 0.1 - p_t * 0.5);
        let len = D + 0.2 - p_t * 2.0;
        parts.push(wall(
            [p_t, PARAPET_H, len],
            [cx, top + PARAPET_H * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
        parts.push(band(
            [p_t + 0.18, 0.14, len],
            [cx, top + PARAPET_H + 0.07, 0.0],
        ));
    }
    // Roof deck, held just below the parapet's foot so the two never share a
    // horizontal plane.
    parts.push(prim(
        solid(cuboid_tapered(
            [W - 0.2, 0.16, D - 0.2],
            0.0,
            concrete([0.78, 0.76, 0.71]),
        )),
        [0.0, top - 0.02, 0.0],
        id_quat(),
    ));
    // The sign: a framed board on two posts, with a smaller lit face inside
    // the frame. A broad panel at strength blooms to a white blank; a frame
    // round a smaller lens reads as a sign.
    let sign_y = top + PARAPET_H + 1.05;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.18, 1.5, 0.18], 0.0, steel(STEEL_GREY))),
            [sx * 2.9, top + PARAPET_H * 0.5 + 0.55, FRONT + 1.1],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered(
            [7.0, 1.3, 0.26],
            0.0,
            steel([0.36, 0.38, 0.4]),
        )),
        [0.0, sign_y, FRONT + 1.1],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([6.3, 0.78, 0.16], 0.0, glow(SIGN_AMBER, 2.4)),
        [0.0, sign_y, FRONT + 0.98],
        id_quat(),
    ));
    nest(corona, parts)
}

// --- The pool terrace. -----------------------------------------------------

/// The pool terrace, running off the podium's **own** front edge.
///
/// It used to be placed at `front - 6.5` — a round number measured off the
/// building — which left three and a half metres of bare ground between the
/// podium and the deck. Deriving the deck from the podium's edge is #972
/// lesson 8, and the sub-root is the deck itself, so one drag takes the pool,
/// the coping and the parasols with it.
fn terrace() -> Generator {
    let podium_front = -(D * 0.5 + PODIUM_OVER);
    let deck_d = 7.0_f32;
    let deck_z = podium_front - deck_d * 0.5 + 0.2;
    let deck = prim(
        solid(cuboid_tapered(
            [W - 1.0, 0.22, deck_d],
            0.0,
            concrete([0.88, 0.85, 0.78]),
        )),
        [0.0, 0.11, deck_z],
        id_quat(),
    );

    let pool_z = deck_z - 0.3;
    let (pool_w, pool_l) = (6.0_f32, 3.6_f32);
    let mut parts = vec![
        // Sunk basin shell under the water, so the pool reads as depth.
        prim(
            solid(cuboid_tapered(
                [pool_w + 0.4, 0.34, pool_l + 0.4],
                0.0,
                concrete([0.36, 0.56, 0.62]),
            )),
            [0.0, 0.06, pool_z],
            id_quat(),
        ),
        // Water surface, set just below the coping.
        prim(
            cuboid_tapered([pool_w, 0.12, pool_l], 0.0, water(POOL_AQUA)),
            [0.0, 0.2, pool_z],
            id_quat(),
        ),
    ];
    // Proud coping rim framing the water — raised, so nothing is flush.
    for (size, pos) in [
        (
            [pool_w + 0.8, 0.16, 0.34],
            [0.0, 0.3, pool_z - pool_l * 0.5 - 0.23],
        ),
        (
            [pool_w + 0.8, 0.16, 0.34],
            [0.0, 0.3, pool_z + pool_l * 0.5 + 0.23],
        ),
        (
            [0.34, 0.16, pool_l + 0.12],
            [-pool_w * 0.5 - 0.23, 0.3, pool_z],
        ),
        (
            [0.34, 0.16, pool_l + 0.12],
            [pool_w * 0.5 + 0.23, 0.3, pool_z],
        ),
    ] {
        parts.push(prim(
            solid(cuboid_tapered(size, 0.0, stucco(STUCCO_WHITE))),
            pos,
            id_quat(),
        ));
    }
    // Steps from the podium down onto the terrace, on the entrance bay's
    // centreline and derived from the drop between the two decks — the
    // podium stands 0.7 off the ground and the terrace 0.22, and a doorway
    // opening onto a half-metre drop is the same fault the fishing shack had.
    let drop = PODIUM_H - 0.22;
    for i in 0..2 {
        let top = 0.22 + drop * (2 - i) as f32 / 3.0;
        parts.push(prim(
            solid(cuboid_tapered(
                [3.0, top, 0.36],
                0.0,
                concrete([0.84, 0.81, 0.75]),
            )),
            [
                BAY_X[DOOR_BAY],
                top * 0.5,
                podium_front - 0.18 - 0.36 * i as f32,
            ],
            id_quat(),
        ));
    }

    // Two parasols, set inside the deck's own edge.
    for sx in [-1.0_f32, 1.0] {
        let px = sx * (W * 0.5 - 1.4);
        parts.push(prim(
            solid(cylinder_tapered(0.05, 2.2, 8, 0.0, steel(STEEL_GREY))),
            [px, 1.32, pool_z],
            id_quat(),
        ));
        parts.push(prim(
            cone(1.05, 0.5, 14, canvas(AWNING_RED, AWNING_WHITE)),
            [px, 2.5, pool_z],
            id_quat(),
        ));
    }
    nest(deck, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, window_cards,
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
        assert_sanitize_stable(&GrandHotel.build(""), "grand_hotel");
    }

    #[test]
    fn has_lit_sign() {
        assert!(crate::catalogue::items::util::has_emissive(
            &GrandHotel.build("")
        ));
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&GrandHotel.build(""), "grand_hotel");
    }

    /// #972 lesson 1: every `Window` card sits on a `Plane` at `uv_scale` 1.0
    /// — one per opening, plus the entrance leaves. The exact count is what
    /// bites: a card on a solid still renders, it just renders as a frame with
    /// holes onto the stucco behind it, which is what all four of this
    /// entry's glazed surfaces used to be.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&GrandHotel.build(""), [0.0; 3], &mut |g, _| {
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
            openings().len() + 3,
            "one per seafront opening, plus the entrance leaves and both stair windows"
        );
    }

    /// The hotel does **not** reach for `modern_city::curtain_wall`. That
    /// helper is a lit glass *cuboid* behind proud fins — right on the tower
    /// it was written for, wrong at eye level, and it was standing in for this
    /// entry's whole lobby. A `Window` texture on a solid is the failure the
    /// card idiom exists to prevent, so this is worth pinning by name rather
    /// than leaving to the plane check above.
    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&GrandHotel.build(""), "grand_hotel");
    }

    /// The standing ROTATED-ROOT gotcha, finally guarded: a tilted parent
    /// spins everything it carries, and the translation-only walks every other
    /// guard here uses would report those children where they were authored
    /// rather than where they render.
    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&GrandHotel.build(""), "grand_hotel");
    }

    /// #972 lesson 7: cards lap their openings and stand clear of the rooms
    /// they frame.
    #[test]
    fn cards_lap_their_openings() {
        for c in window_cards(&GrandHotel.build("")) {
            assert!(
                c.size[0] > OPEN_W + 1e-4 || c.center[2].abs() < D * 0.5 - 1e-3,
                "a seafront card {:?} is flush with its reveal",
                c.size
            );
        }
    }

    /// #972 lesson 8: the pool terrace runs off the podium's own edge. It used
    /// to sit at a round number measured off the *building*, which left 3.5 m
    /// of bare ground between the two — invisible unless a contact-sheet tile
    /// looks along that edge.
    #[test]
    fn the_terrace_meets_the_podium() {
        let root = GrandHotel.build("");
        let terrace = root.children.last().expect("the podium carries a terrace");
        let GeneratorKind::Cuboid { size, .. } = &terrace.kind else {
            panic!("the terrace sub-root is its deck");
        };
        let deck_back = terrace.transform.translation.0[2] + size.0[2] * 0.5;
        let podium_front = -(D * 0.5 + PODIUM_OVER);
        assert!(
            deck_back > podium_front - 0.05,
            "the pool deck's back edge at {deck_back} leaves a gap to the \
             podium at {podium_front}"
        );
        assert!(
            deck_back < podium_front + 0.6,
            "the pool deck at {deck_back} runs under the podium at {podium_front}"
        );
    }

    /// Everything that stands on the podium stands *on* it. The awning's
    /// poles are the piece that goes wrong: slung from the canopy's own edge
    /// rather than from the paving, their feet landed half a metre past the
    /// podium and half a metre above the terrace, which is invisible from
    /// every angle because the canopy is directly over them.
    #[test]
    fn the_awning_poles_stand_on_the_podium() {
        let root = GrandHotel.build("");
        let half = [
            (W + PODIUM_OVER * 2.0) * 0.5,
            0.0,
            (D + PODIUM_OVER * 2.0) * 0.5,
        ];
        let mut poles = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if sx > 0.2 || sz > 0.2 || sy < 1.5 || at[2] > FRONT {
                return;
            }
            poles += 1;
            assert!(
                at[2] - sz * 0.5 > -half[2] - 1e-3 && at[2] + sz * 0.5 < half[2] + 1e-3,
                "an awning pole at {at:?} stands past the podium's edge at {}",
                -half[2]
            );
            assert!(
                (at[1] - sy * 0.5 - PODIUM_H).abs() < 1e-3,
                "an awning pole's foot at {} does not rest on the podium at {PODIUM_H}",
                at[1] - sy * 0.5
            );
        });
        assert_eq!(poles, 2, "the awning stands on two poles");
    }

    /// A balcony railing is a railing: two rails and balusters, not a plate.
    /// The 0.55 m panel this replaces read as a parapet wall and hid the
    /// French doors behind it — the one thing a balcony must not do.
    #[test]
    fn the_balcony_railings_are_open() {
        let root = GrandHotel.build("");
        let mut balusters = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if at[2] < FRONT && sx < 0.08 && sz < 0.08 && sy > 0.5 {
                balusters += 1;
            }
        });
        assert!(
            balusters >= 40,
            "only {balusters} balusters across two balconies — the railing is a plate"
        );
    }

    /// #972 lesson 9: the lobby is laid out bay by bay. A fit-out authored for
    /// "the lobby" leaves whichever bay it was not written for a black
    /// rectangle beside one that reads beautifully.
    #[test]
    fn every_lobby_bay_has_something_behind_it() {
        let root = GrandHotel.build("");
        let mut lit: Vec<[f32; 3]> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let material = match &g.kind {
                GeneratorKind::Cuboid { material, .. } => material,
                GeneratorKind::Cone { material, .. } => material,
                GeneratorKind::Cylinder { material, .. } => material,
                _ => return,
            };
            if material.emission_strength.0 > 0.15 && at[2] > FRONT && at[1] < PODIUM_H + LOBBY_HEAD
            {
                lit.push(at);
            }
        });
        for &x in BAY_X.iter() {
            assert!(
                lit.iter().any(|p| (p[0] - x).abs() < OPEN_W),
                "lobby bay at x {x} has nothing lit behind it"
            );
        }
    }

    /// The editability contract: the podium carries the shell and the
    /// terrace, the cornice carries the parapet and the sign, each balcony
    /// slab carries its own railing.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = GrandHotel.build("");
        assert_eq!(
            root.children.len(),
            3,
            "podium carries the foundation, the shell and the terrace"
        );
        let shell = &root.children[1];
        assert!(
            shell.children.iter().any(|c| c.children.len() >= 12),
            "the cornice carries the parapet, the deck and the sign"
        );
        assert!(
            shell
                .children
                .iter()
                .filter(|c| c.children.len() >= 20)
                .count()
                >= 2,
            "each balcony slab carries its own railing"
        );
        assert!(count(&root) > 120, "the hotel lost most of its parts");
    }
}
