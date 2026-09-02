//! Beach house — a Coastal-Resort secondary. A pastel stucco bungalow raised
//! on timber stilts above the tide line, with a railed veranda, shuttered
//! windows over planted boxes, a lit front room and a ridged plank roof. The
//! holiday let of the strip.
//!
//! Rebuilt as a shell under #972. Four faults, and the last is the one the
//! guards were written for:
//!
//! 1. **The windows were slabs on a solid wall.** Two `Window`-textured
//!    cuboids pinned to the stucco — the generator masks its panes away, so
//!    each was a frame with holes onto the render behind it.
//! 2. **The roof had a plateau.** `taper` 0.85 pinches a cuboid to a 15 % flat
//!    top: a truncated wedge, not a ridge. At 0.99 it comes to a line, and the
//!    gables it leaves are rendered like the walls.
//! 3. **The railing was a plate.** One 0.5 m slab across seven metres on two
//!    end posts, and only along the front — three sides of a deck 1.6 m off
//!    the sand had nothing at all.
//! 4. **The steps neither started nor finished anywhere.** Two treads at a
//!    round `deck − 0.5` and `deck − 1.0`, floating 0.3 m clear of the deck's
//!    front edge, with a half-metre drop off the top one and three quarters of
//!    a metre off the bottom one to the sand. Every angle in a contact sheet
//!    shows two plausible steps; none of them shows that you cannot use them.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat,
    lit_interior, nest, plane, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DECK_PALE, DECK_WOOD, GLASS_AQUA, LAMP_WARM, STEEL_GREY, pane_grid, plank, steel, stucco,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// Deck plan and how far the stilts carry it above the sand.
const DECK_W: f32 = 7.2;
const DECK_D: f32 = 6.2;
const DECK_Y: f32 = 1.55;
const DECK_T: f32 = 0.3;
/// Top of the deck boards — the floor level, and the datum for everything
/// above.
const FLOOR: f32 = DECK_Y + DECK_T * 0.5;

/// Bungalow plan and wall height above the floor.
const W: f32 = 5.2;
const D: f32 = 4.2;
const WALL_H: f32 = 2.7;
const WALL_T: f32 = 0.24;
const WALL_TOP: f32 = FLOOR + WALL_H;
/// The bungalow sits back on the deck, so the veranda is in front of it.
const HOUSE_Z: f32 = 0.7;

/// Outer face of the shore-facing wall — the `-Z` hero direction the render
/// tool and the settlement placer both look down.
const FRONT: f32 = HOUSE_Z - D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane and the room panel behind it, inside the reveal.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.7;
const ROOM_Z: f32 = FRONT + 0.6;
/// Centre plane of proud trim — shutters, casings, window boxes.
const TRIM_Z: f32 = FRONT - 0.05;

/// Bay centres in X: window, door, window.
const BAY_X: [f32; 3] = [-1.7, 0.0, 1.7];
const DOOR_BAY: usize = 1;
/// Window opening, and the door's.
const WIN_W: f32 = 1.15;
const WIN_H: f32 = 1.2;
const WIN_SILL: f32 = 0.95;
const DOOR_W: f32 = 1.0;
const DOOR_H: f32 = 2.05;

/// Ridge rise above the wall top, and the eaves overhang. There is no rake
/// overhang: the gable triangle *is* the wall carried up, so it lands in the
/// wall's own plane and the barge boards supply the overhang read.
const RIDGE_RISE: f32 = 1.5;
const EAVE_OVER: f32 = 0.55;
/// The Z pinch that takes the roof to a ridge line. Not `1.0`: the record
/// sanitiser clamps `taper` at 0.99, and a value it rewrites fails the
/// entry's own round-trip guard rather than rendering differently.
const RIDGE_TAPER: f32 = 0.99;

/// Veranda rail height above the deck, and the clear width of the gap the
/// steps land in — derived from the flight so the two cannot drift apart.
const RAIL_H: f32 = 1.0;
const STEP_W: f32 = 1.6;

// --- Palette local to this entry. ------------------------------------------

/// Pastel coral render of the bungalow walls — a brighter holiday-let plaster
/// than the duskier hamlet sand, so it reads as a cheerful seaside cottage.
const PASTEL_CORAL: [f32; 3] = [0.93, 0.79, 0.71];
/// Painted teal shutters and trim against the coral walls.
const TRIM_TEAL: [f32; 3] = [0.20, 0.50, 0.52];
/// Window-box greenery.
const PLANT_GREEN: [f32; 3] = [0.30, 0.46, 0.24];
/// Front door paint — the one deeper note on the elevation.
const DOOR_PAINT: [f32; 3] = [0.16, 0.38, 0.42];

// --- Shared construction. --------------------------------------------------

/// Pastel render laid in the wall's own frame, so the plaster's grain does
/// not step at every wall break.
fn render_mat(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = stucco(color);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One rendered slab of the shell.
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            render_mat(PASTEL_CORAL, center, face),
        )),
        center,
        id_quat(),
    )
}

/// A proud painted board — shutter, casing, barge board, fascia. Always
/// oversized against what it laps and always standing off the surface it
/// laps, so it never shares a plane with its host.
fn trim(size: [f32; 3], center: [f32; 3]) -> Generator {
    prim(
        solid(cuboid_tapered(
            size,
            0.0,
            render_mat(TRIM_TEAL, center, FaceKey::SideNz),
        )),
        center,
        id_quat(),
    )
}

/// How far a glazing card oversails its opening on every edge (#972 lesson 7).
const GLAZE_LAP: f32 = 0.05;

/// Clear glazing filling one bay, on a flat quad at [`GLAZE_Z`].
fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            [size[0] + GLAZE_LAP, size[1] + GLAZE_LAP],
            pane_grid(GLASS_AQUA, 0.0, panes),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A lit room behind an opening — the surface a card's masked-away panes
/// actually show.
fn room(size: [f32; 2], center: [f32; 3], lit: f32) -> Generator {
    prim(
        cuboid_tapered(
            [size[0], size[1], 0.08],
            0.0,
            lit_interior([0.70, 0.58, 0.40], lit),
        ),
        center,
        id_quat(),
    )
}

pub struct BeachHouse;

impl CatalogueEntry for BeachHouse {
    fn slug(&self) -> &'static str {
        "beach_house"
    }
    fn name(&self) -> &'static str {
        "Beach House"
    }
    fn description(&self) -> &'static str {
        "Pastel stucco bungalow on stilts with a railed veranda and a ridged roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The house as a tree that stands the way it does: a stilt at the bottom
/// carrying the rest, the deck on them, the bungalow on the deck, the roof on
/// the bungalow — with the steps their own sub-assembly off the deck's edge.
fn build_tree() -> Generator {
    let (px, pz) = (DECK_W * 0.5 - 0.55, DECK_D * 0.5 - 0.55);
    let mut parts = Vec::new();
    for (sx, sz) in [
        (1.0_f32, -1.0_f32),
        (1.0, 0.0),
        (1.0, 1.0),
        (-1.0, 0.0),
        (-1.0, 1.0),
    ] {
        parts.push(prim(
            solid(cylinder_tapered(0.19, DECK_Y, 8, 0.08, plank(DECK_WOOD))),
            [sx * px, DECK_Y * 0.5, sz * pz],
            id_quat(),
        ));
    }
    parts.push(deck());
    // Buried footing under the deck's own plan, sized to the drop this
    // footprint spans (#1009): the stilts stand on it, and it closes the gap
    // a terrain-snapped placement leaves under the downhill edge. Authored
    // around y=0 and rebased into the root stilt's frame by `nest`.
    parts.push(footing(DECK_W, DECK_D, [0.0, 0.0], 6.0));

    let root = prim(
        solid(cylinder_tapered(0.19, DECK_Y, 8, 0.08, plank(DECK_WOOD))),
        [-px, DECK_Y * 0.5, -pz],
        id_quat(),
    );
    nest(root, parts)
}

/// The veranda deck, and on it the bungalow, the railing and the steps.
fn deck() -> Generator {
    let center = [0.0, DECK_Y, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [DECK_W, DECK_T, DECK_D],
            0.0,
            util::bonded_siding(plank(DECK_PALE), FaceKey::Top, center),
        )),
        center,
        id_quat(),
    );

    let mut parts = vec![bungalow(), steps()];

    // Railing round the veranda: both flanks in full, and the front in two
    // runs either side of the step gap. The back edge is closed by the house.
    let (hx, hz) = (DECK_W * 0.5 - 0.12, DECK_D * 0.5 - 0.12);
    let rail = |a: [f32; 3], b: [f32; 3]| {
        util::railing(a, b, RAIL_H, util::BALUSTER_PITCH, plank(DECK_PALE))
    };
    for sx in [-1.0_f32, 1.0] {
        parts.extend(rail(
            [sx * hx, FLOOR, -hz],
            [sx * hx, FLOOR, HOUSE_Z + D * 0.5 - 0.2],
        ));
        parts.extend(rail([sx * hx, FLOOR, -hz], [sx * STEP_W * 0.5, FLOOR, -hz]));
    }
    // A pair of deck chairs' worth of life: a low table on the veranda.
    parts.push(prim(
        solid(cylinder_tapered(0.42, 0.1, 12, 0.0, plank(DECK_WOOD))),
        [-2.1, FLOOR + 0.52, -1.5],
        id_quat(),
    ));
    parts.push(prim(
        solid(cylinder_tapered(0.07, 0.5, 8, 0.2, steel(STEEL_GREY))),
        [-2.1, FLOOR + 0.25, -1.5],
        id_quat(),
    ));

    nest(deck, parts)
}

// --- The bungalow. ---------------------------------------------------------

/// Floor, and on it everything the house is: the render that frames the three
/// bays, the glazing, the lit room behind it, and — on the wall plate — the
/// roof.
fn bungalow() -> Generator {
    let mut parts = Vec::new();
    let mid_y = FLOOR + WALL_H * 0.5;
    let inner_d = D - WALL_T * 2.0;

    // Back and side walls — solid; only the shore face is cut.
    parts.push(wall(
        [W, WALL_H, WALL_T],
        [0.0, mid_y, HOUSE_Z + D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, WALL_H, inner_d],
            [sx * (W * 0.5 - WALL_T * 0.5), mid_y, HOUSE_Z],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    shore_elevation(&mut parts);
    parts.push(roof());

    let floor = prim(
        cuboid_tapered(
            [W - WALL_T * 2.0, 0.06, inner_d],
            0.0,
            lit_interior([0.44, 0.36, 0.28], 0.16),
        ),
        [0.0, FLOOR + 0.03, HOUSE_Z],
        id_quat(),
    );
    nest(floor, parts)
}

/// The hero face: the render that *frames* two windows and the door, with
/// shutters, casings and planted boxes proud of it.
fn shore_elevation(parts: &mut Vec<Generator>) {
    // Piers between and outside the three openings.
    let mut edges = vec![-W * 0.5];
    for (b, &x) in BAY_X.iter().enumerate() {
        let half = if b == DOOR_BAY { DOOR_W } else { WIN_W } * 0.5;
        edges.push(x - half);
        edges.push(x + half);
    }
    edges.push(W * 0.5);
    for i in (0..edges.len() - 1).step_by(2) {
        let (a, b) = (edges[i], edges[i + 1]);
        parts.push(wall(
            [b - a, WALL_H, WALL_T],
            [(a + b) * 0.5, FLOOR + WALL_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    // Over the door, and under and over each window.
    parts.push(wall(
        [DOOR_W, WALL_H - DOOR_H, WALL_T],
        [BAY_X[DOOR_BAY], FLOOR + (DOOR_H + WALL_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    let win_head = WIN_SILL + WIN_H;
    for (b, &x) in BAY_X.iter().enumerate() {
        if b == DOOR_BAY {
            continue;
        }
        parts.push(wall(
            [WIN_W, WIN_SILL, WALL_T],
            [x, FLOOR + WIN_SILL * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
        parts.push(wall(
            [WIN_W, WALL_H - win_head, WALL_T],
            [x, FLOOR + (win_head + WALL_H) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));

        // The glazing, its lit room, and the joinery round it.
        let cy = FLOOR + WIN_SILL + WIN_H * 0.5;
        parts.push(glazing([WIN_W, WIN_H], [x, cy, GLAZE_Z], (2, 2)));
        parts.push(room([WIN_W + 0.5, WIN_H + 0.5], [x, cy, ROOM_Z], 0.42));
        parts.push(trim(
            [WIN_W + 0.4, 0.14, 0.2],
            [x, cy + WIN_H * 0.5 + 0.12, TRIM_Z],
        ));
        // Shutters either side, hung on the render rather than in the reveal.
        for sh in [-1.0_f32, 1.0] {
            parts.push(trim(
                [0.24, WIN_H + 0.06, 0.06],
                [x + sh * (WIN_W * 0.5 + 0.15), cy, TRIM_Z - 0.02],
            ));
        }
        // Window box and greenery, proud below the sill.
        parts.push(prim(
            solid(cuboid_tapered(
                [WIN_W + 0.16, 0.24, 0.3],
                0.0,
                plank(DECK_WOOD),
            )),
            [x, cy - WIN_H * 0.5 - 0.2, TRIM_Z - 0.13],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered([WIN_W + 0.04, 0.22, 0.24], 0.6, stucco(PLANT_GREEN)),
            [x, cy - WIN_H * 0.5 - 0.02, TRIM_Z - 0.13],
            id_quat(),
        ));
    }

    // The door: a painted leaf in the reveal, with a glazed light over it and
    // a lit hall behind, so the doorway is depth rather than a flat panel.
    let dx = BAY_X[DOOR_BAY];
    parts.push(room(
        [DOOR_W + 0.5, DOOR_H + 0.4],
        [dx, FLOOR + DOOR_H * 0.5, ROOM_Z],
        0.36,
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [DOOR_W + 0.06, DOOR_H - 0.42, 0.08],
            0.0,
            glow(DOOR_PAINT, 0.0),
        )),
        [dx, FLOOR + (DOOR_H - 0.42) * 0.5, GLAZE_Z - 0.05],
        id_quat(),
    ));
    parts.push(glazing(
        [DOOR_W, 0.3],
        [dx, FLOOR + DOOR_H - 0.19, GLAZE_Z],
        (2, 1),
    ));
    parts.push(trim(
        [DOOR_W + 0.44, 0.16, 0.22],
        [dx, FLOOR + DOOR_H + 0.11, TRIM_Z],
    ));
    // Veranda lantern beside the door — a small lens in a housing.
    parts.push(prim(
        solid(cuboid_tapered(
            [0.18, 0.24, 0.13],
            0.0,
            steel([0.3, 0.3, 0.32]),
        )),
        [dx + 0.85, FLOOR + 1.85, FRONT - 0.065],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.11, 0.14, 0.05], 0.0, glow(LAMP_WARM, 2.2)),
        [dx + 0.85, FLOOR + 1.83, FRONT - 0.13],
        id_quat(),
    ));
}

/// The ridged plank roof, its gable ends rendered like the walls, plus barge
/// boards at the roof's own pitch and an eaves fascia.
///
/// Pinching **Z alone** is what makes this a ridge rather than a truncated
/// wedge; the `±X` faces it leaves are the gables, and they take a per-face
/// override (#955) carrying the wall's own render at its own offset.
fn roof() -> Generator {
    let center = [0.0, WALL_TOP + RIDGE_RISE * 0.5, HOUSE_Z];
    let mut kind = solid(cuboid_tapered_xz(
        [W, RIDGE_RISE, D + EAVE_OVER * 2.0],
        [0.0, RIDGE_TAPER],
        util::bonded_siding(plank(DECK_WOOD), FaceKey::Top, center),
    ));
    for face in [FaceKey::SidePx, FaceKey::SideNx] {
        kind = util::with_face(kind, face, render_mat(PASTEL_CORAL, center, face));
    }

    let mut parts = Vec::new();
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([W + 0.1, 0.14, 0.1], 0.0, plank(DECK_PALE))),
            [
                0.0,
                WALL_TOP + 0.03,
                HOUSE_Z + sz * (D * 0.5 + EAVE_OVER - 0.05),
            ],
            id_quat(),
        ));
    }
    let half = D * 0.5 + EAVE_OVER;
    let slope = RIDGE_RISE.hypot(half);
    let pitch = RIDGE_RISE.atan2(half);
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered([0.1, 0.18, slope], 0.0, plank(DECK_PALE))),
                [
                    sx * (W * 0.5 + 0.05),
                    WALL_TOP + RIDGE_RISE * 0.5,
                    HOUSE_Z + sz * half * 0.5,
                ],
                quat_x(sz * pitch),
            ));
        }
    }
    // Ridge cap along the apex.
    parts.push(prim(
        solid(cuboid_tapered([W + 0.06, 0.14, 0.3], 0.0, plank(DECK_PALE))),
        [0.0, WALL_TOP + RIDGE_RISE + 0.05, HOUSE_Z],
        id_quat(),
    ));

    nest(prim(kind, center, id_quat()), parts)
}

/// Steps down off the veranda.
///
/// Both ends are derived: the top tread meets the deck's own top, the bottom
/// one meets the sand, and the risers between are equal. The shipped pair
/// floated 0.3 m clear of the deck edge with a half-metre drop off the top
/// and three quarters of a metre off the bottom, which no contact-sheet angle
/// distinguishes from a usable flight (#972 lesson 8).
fn steps() -> Generator {
    let risers = 5;
    let rise = FLOOR / risers as f32;
    let going = 0.32;
    let front_edge = -DECK_D * 0.5;

    let mut parts = Vec::new();
    for i in 0..risers - 1 {
        let top = (i + 1) as f32 * rise;
        parts.push(prim(
            solid(cuboid_tapered([STEP_W, top, going], 0.0, plank(DECK_WOOD))),
            [
                0.0,
                top * 0.5,
                front_edge - going * (risers - 1 - i) as f32 - going * 0.5,
            ],
            id_quat(),
        ));
    }
    // Cheek rails either side of the flight, standing on the treads.
    let run = going * risers as f32;
    for sx in [-1.0_f32, 1.0] {
        for pz in [front_edge - 0.25, front_edge - run + 0.25] {
            parts.push(prim(
                solid(cuboid_tapered([0.09, 0.95, 0.09], 0.0, plank(DECK_PALE))),
                [sx * (STEP_W * 0.5 - 0.06), 0.48, pz],
                id_quat(),
            ));
        }
        parts.push(prim(
            cuboid_tapered([0.08, 0.08, run - 0.4], 0.0, plank(DECK_PALE)),
            [
                sx * (STEP_W * 0.5 - 0.06),
                0.93,
                front_edge - run * 0.5 + 0.05,
            ],
            id_quat(),
        ));
    }

    // The top tread is the sub-root: what the flight hangs off, lapping under
    // the deck's own edge so no tread can float.
    let top_step = prim(
        solid(cuboid_tapered(
            [STEP_W, FLOOR, going + 0.12],
            0.0,
            plank(DECK_WOOD),
        )),
        [0.0, FLOOR * 0.5, front_edge - going * 0.5 + 0.06],
        id_quat(),
    );
    nest(top_step, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable,
    };
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
        assert_sanitize_stable(&BeachHouse.build(""), "beach_house");
    }

    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&BeachHouse.build(""), "beach_house");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&BeachHouse.build(""), "beach_house");
    }

    #[test]
    fn glazed_surfaces_do_not_collide() {
        assert_cards_do_not_overlap(&BeachHouse.build(""), "beach_house");
    }

    /// #972 lesson 1: two windows and the door's fanlight, each a card on a
    /// `Plane` at `uv_scale` 1.0 over a real opening.
    #[test]
    fn every_opening_is_a_card_on_a_plane() {
        let mut cards = 0;
        walk(&BeachHouse.build(""), [0.0; 3], &mut |g, _| {
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in crate::pds::material_finish::node_materials_mut(&mut g.kind.clone()) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane");
                    assert_eq!(m.uv_scale.0, 1.0, "cards are clamp-to-edge");
                    cards += 1;
                }
            }
        });
        assert_eq!(cards, 3, "two windows and the door's fanlight");
    }

    /// The roof comes to a ridge, and the gables it leaves are rendered like
    /// the walls. `taper` 0.85 — what this carried — leaves a flat top 15 % of
    /// the house's depth wide, which reads as a botched hip from anything
    /// above eye level.
    #[test]
    fn the_roof_comes_to_a_ridge_over_rendered_gables() {
        let mut found = false;
        walk(&BeachHouse.build(""), [0.0; 3], &mut |g, _| {
            let GeneratorKind::Cuboid {
                common: PrimCommon { torture, faces, .. },
                ..
            } = &g.kind
            else {
                return;
            };
            let [tx, tz] = torture.taper.0;
            // Select the roof by *size*, not by taper alone: a tapered
            // window box is also a pinched cuboid, and the first version of
            // this guard reported the greenery as a hipped roof.
            if tz < 0.5 || !matches!(&g.kind, GeneratorKind::Cuboid { size, .. } if size.0[0] > 3.0)
            {
                return;
            }
            found = true;
            assert_eq!(tx, 0.0, "the roof is pinched in X too — that is a hip");
            assert!(tz > 0.9, "the ridge taper {tz} leaves a plateau on top");
            let clad: Vec<_> = faces.iter().map(|o| o.face).collect();
            assert!(
                clad.contains(&FaceKey::SidePx) && clad.contains(&FaceKey::SideNx),
                "the gables wear roof planking instead of render: {clad:?}"
            );
        });
        assert!(found, "no ridged roof in the tree");
    }

    /// #972 lesson 8: the steps meet the deck at the top and the sand at the
    /// bottom, in equal risers, and start at the deck's own front edge. The
    /// shipped pair did none of those things.
    #[test]
    fn the_steps_reach_the_deck_in_even_risers() {
        let root = BeachHouse.build("");
        let mut treads: Vec<[f32; 3]> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            if (size.0[0] - STEP_W).abs() < 1e-3 && size.0[2] < 0.5 {
                treads.push([at[0], at[1] + size.0[1] * 0.5, at[2]]);
            }
        });
        assert_eq!(treads.len(), 5, "the flight has five treads");
        treads.sort_by(|a, b| a[1].partial_cmp(&b[1]).unwrap());
        assert!(
            (treads.last().unwrap()[1] - FLOOR).abs() < 1e-3,
            "the top tread at {} does not meet the deck at {FLOOR}",
            treads.last().unwrap()[1]
        );
        let rise = treads[0][1];
        assert!(rise < 0.36, "a {rise} m riser is a climb, not a step");
        for pair in treads.windows(2) {
            assert!(
                (pair[1][1] - pair[0][1] - rise).abs() < 1e-3,
                "uneven riser between {} and {}",
                pair[0][1],
                pair[1][1]
            );
        }
        for t in &treads {
            assert!(
                t[2] < -DECK_D * 0.5 + 0.13,
                "a tread at z {} sits under the deck rather than off its edge",
                t[2]
            );
        }
    }

    /// The veranda is railed on the three open sides, with posts and
    /// balusters, and the gap in the front rail is the one the steps land in.
    #[test]
    fn the_veranda_is_railed_on_every_open_side() {
        let root = BeachHouse.build("");
        let mut posts: Vec<[f32; 3]> = Vec::new();
        let mut balusters = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            // Balusters are `RAIL_H` less the handrail's own stock, so an
            // exact height match finds only the end posts — which is how the
            // first version of this guard counted zero balusters on a railed
            // veranda.
            if at[1] < FLOOR || !(RAIL_H * 0.7..=RAIL_H + 0.01).contains(&sy) {
                return;
            }
            if (sx - 0.11).abs() < 1e-3 && (sz - 0.11).abs() < 1e-3 {
                posts.push(at);
            } else if sx < 0.09 && sz < 0.09 {
                balusters += 1;
            }
        });
        assert!(balusters >= 24, "only {balusters} balusters on the veranda");
        // Both flanks and both front runs.
        for sx in [-1.0_f32, 1.0] {
            assert!(
                posts
                    .iter()
                    .any(|p| (p[0] - sx * (DECK_W * 0.5 - 0.12)).abs() < 0.02),
                "no railing down the {} flank",
                if sx > 0.0 { "+X" } else { "-X" }
            );
        }
        assert!(
            posts
                .iter()
                .any(|p| (p[0].abs() - STEP_W * 0.5).abs() < 0.02),
            "the front rail does not stop either side of the step gap"
        );
    }

    /// The house keeps its lit rooms and lantern — escalation's
    /// broken-emissive ruin pass needs something to snuff.
    #[test]
    fn has_lit_rooms() {
        assert!(crate::catalogue::items::util::has_emissive(
            &BeachHouse.build("")
        ));
    }

    /// The editability contract: a stilt carries the deck, the deck carries
    /// the bungalow and the steps, the bungalow carries the roof.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = BeachHouse.build("");
        let deck = root
            .children
            .iter()
            .find(|c| c.children.len() > 20)
            .expect("a stilt carries the deck");
        let house = deck
            .children
            .iter()
            .find(|c| c.children.len() > 15)
            .expect("the deck carries the bungalow");
        assert!(
            house.children.iter().any(|c| c.children.len() >= 7),
            "the bungalow carries a roof that carries its own trim"
        );
        assert!(count(&root) > 80, "the house lost most of its parts");
    }
}
