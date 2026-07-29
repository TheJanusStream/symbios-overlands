//! Detached garage — a Suburban secondary. A standalone sided garage at the
//! back of the lot, its roll-up door **open** on a lit workshop: a bench under
//! a pegboard, a shelf run, and a strip light on.
//!
//! Open is a deliberate choice, not laziness about modelling a door. A closed
//! garage is a sided box with a large flat panel on it, and its side window
//! and the vision light in its man-door are then cards over nothing — the
//! failure the `Window` idiom exists to prevent. Rolling the door up gives the
//! hero face two metres of real depth, turns the same interior that justifies
//! the window into the point of the prop, and is the more honest image of a
//! suburban back lot anyway: the garage you can see into is the one somebody
//! actually uses.
//!
//! Built to the #972 ledger throughout — the front elevation is the siding
//! that *frames* three openings, the courses run unbroken through the whole
//! wall in one frame ([`util::bonded_siding`]), and the tree stands the way
//! the garage does.
//!
//! [`util::bonded_siding`]: crate::catalogue::items::util::bonded_siding

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat,
    lit_interior, nest, plane, prim, quat_x, quat_z, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GLASS_TINT, HEDGE_GREEN, ROOF_GREY, SIDING_SAGE, WOOD_WHITE, concrete, enamel, shingle, siding,
    tinted_glass, wood,
};

// --- Shell dimensions. Everything below is derived from these. -------------

const W: f32 = 6.5;
const D: f32 = 6.5;
const BASE_H: f32 = 0.4;
const BODY_H: f32 = 3.0;
/// Wall thickness, and so the depth of every reveal.
const WALL_T: f32 = 0.25;

/// Outer face of the front wall — the hero direction, `-Z`.
const FRONT: f32 = -D * 0.5;
/// Centre of a wall slab whose outer face lies on [`FRONT`].
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Glazing plane, set back inside the reveal.
const GLAZE_Z: f32 = FRONT + WALL_T * 0.72;
/// Centre plane of the proud trim boards — deep enough that their back faces
/// land *inside* the wall rather than coplanar with its outer face.
const TRIM_Z: f32 = FRONT - 0.03;

/// The vehicle opening: width, centre and head.
const BAY_W: f32 = 2.9;
const BAY_X: f32 = -1.5;
const BAY_HEAD: f32 = 2.35;
/// The man-door beside it.
const MAN_W: f32 = 0.95;
const MAN_X: f32 = 0.85;
const MAN_HEAD: f32 = 2.1;
/// The workshop window at the far end of the elevation.
const WIN_W: f32 = 1.1;
const WIN_X: f32 = 2.35;
const WIN_SILL: f32 = 1.5;
const WIN_HEAD: f32 = 2.4;
/// How far a glazing card oversails its opening, so no edge of the quad lands
/// on the reveal's own plane (#972 lesson 7).
const GLAZE_LAP: f32 = 0.06;

/// Roof rise from eaves to ridge.
const ROOF_H: f32 = 1.9;
/// Roof overhang past the walls, on both axes.
const EAVE: f32 = 0.5;

// --- Palette local to this entry. ------------------------------------------

/// Man-door paint — a stained timber door against the sage siding.
const MAN_PAINT: [f32; 3] = [0.42, 0.28, 0.18];
/// Roll-up door: pale enamel, as sectional doors are.
const ROLL_ENAMEL: [f32; 3] = [0.84, 0.84, 0.81];
/// Domestic concrete for the slab and the apron.
const SLAB_GREY: [f32; 3] = [0.60, 0.59, 0.57];

/// Lap siding laid in the wall's own frame. With the stagger off there are no
/// U features left, so every side face agrees on `V = -y` and the elevation
/// needs no per-face override to carry its courses round a corner.
fn lap(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_siding(siding(SIDING_SAGE), face, center)
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

/// A proud painted board — fascia, barge, corner board, head trim.
fn trim(size: [f32; 3], center: [f32; 3], rotation: crate::pds::Fp4) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, wood(WOOD_WHITE))),
        center,
        rotation,
    )
}

pub struct DetachedGarage;

impl CatalogueEntry for DetachedGarage {
    fn slug(&self) -> &'static str {
        "detached_garage"
    }
    fn name(&self) -> &'static str {
        "Detached Garage"
    }
    fn description(&self) -> &'static str {
        "Standalone sided garage, its roll-up door open on a lit workshop."
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
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Slab at the bottom, and on it the apron, the shell (which carries the roof)
/// and a shrub. Written outermost-last, because [`nest`] rebases a subtree
/// that already carries its own world translation.
fn build_tree() -> Generator {
    let slab = prim(
        solid(cuboid_tapered(
            [W + 0.6, BASE_H, D + 0.6],
            0.0,
            concrete(SLAB_GREY),
        )),
        [0.0, BASE_H * 0.5, 0.0],
        id_quat(),
    );

    // Apron running out from the vehicle bay to meet the drive. A garage with
    // no way to reach it reads as a shed.
    //
    // Mostly buried: its top sits 60 mm above grade, so the visible edge is a
    // kerb rather than a slab. Held proud the way the plinth is, a 3.6 m tongue
    // of concrete floating a hand's width off the ground reads as a ramp.
    let apron = prim(
        solid(cuboid_tapered(
            [BAY_W + 0.9, 0.26, 4.2],
            0.0,
            concrete([0.56, 0.55, 0.54]),
        )),
        [BAY_X, -0.07, FRONT - 2.0],
        id_quat(),
    );

    // Buried footing under the slab, so a terrain-snapped garage on a slope
    // shows plinth rather than daylight under its downhill edge.
    let mut parts = vec![apron, shell(), footing(W + 0.6, D + 0.6, [0.0, 0.0], 5.0)];
    parts.extend(crop_tufts(
        [-W * 0.5 + 0.6, 0.0, D * 0.5 + 0.1],
        [1.2, 0.9],
        2,
        2,
        0.8,
        foliage(HEDGE_GREEN),
    ));

    nest(slab, parts)
}

/// Floor deck, and under it the whole garage: the walls that frame the three
/// openings, the doors and glazing filling them, the workshop behind, and —
/// standing on the walls — the roof.
fn shell() -> Generator {
    let inner = [W - WALL_T * 2.0, D - WALL_T * 2.0];
    let mut parts = Vec::new();

    // Back and side walls. The sides are shortened in Z so their ends never
    // share a plane with the front and back slabs' outer faces.
    parts.push(wall(
        [W, BODY_H, WALL_T],
        [0.0, BASE_H + BODY_H * 0.5, D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, BODY_H, inner[1]],
            [sx * (W * 0.5 - WALL_T * 0.5), BASE_H + BODY_H * 0.5, 0.0],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }

    front_elevation(&mut parts);
    workshop(&mut parts, inner);
    parts.push(roof());

    // Deliberately dark. Nothing lights a shell, so its surfaces carry their
    // own low emissive term — but the *inside* of a garage in daylight is
    // darker than its sunlit siding, and a floor pitched to read comfortably
    // on its own comes out brighter than the wall around the opening, which
    // flattens the two metres of depth the open door exists to show.
    let deck = prim(
        cuboid_tapered(
            [inner[0], 0.08, inner[1]],
            0.0,
            lit_interior([0.17, 0.165, 0.16], 0.08),
        ),
        [0.0, BASE_H + 0.05, 0.0],
        id_quat(),
    );
    nest(deck, parts)
}

/// The hero face, built as the siding that *frames* the vehicle bay, the
/// man-door and the workshop window — four piers full height, with infill over
/// each opening and under the window — plus what fills them.
///
/// The piers run the full wall height and the infill sits strictly between
/// them, so no two slabs overlap in the wall plane: coplanar overlap is the
/// one thing that would z-fight here, and the layout rules it out rather than
/// relying on a nudge.
fn front_elevation(parts: &mut Vec<Generator>) {
    let e = [
        -W * 0.5,
        BAY_X - BAY_W * 0.5,
        BAY_X + BAY_W * 0.5,
        MAN_X - MAN_W * 0.5,
        MAN_X + MAN_W * 0.5,
        WIN_X - WIN_W * 0.5,
        WIN_X + WIN_W * 0.5,
        W * 0.5,
    ];
    for (a, b) in [(e[0], e[1]), (e[2], e[3]), (e[4], e[5]), (e[6], e[7])] {
        parts.push(wall(
            [b - a, BODY_H, WALL_T],
            [(a + b) * 0.5, BASE_H + BODY_H * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    // Infill over each opening, and the sill wall under the window.
    for (x, w, low, high) in [
        (BAY_X, BAY_W, BAY_HEAD, BODY_H),
        (MAN_X, MAN_W, MAN_HEAD, BODY_H),
        (WIN_X, WIN_W, WIN_HEAD, BODY_H),
        (WIN_X, WIN_W, 0.0, WIN_SILL),
    ] {
        parts.push(wall(
            [w, high - low, WALL_T],
            [x, BASE_H + (low + high) * 0.5, FRONT_MID],
            FaceKey::SideNz,
        ));
    }

    rolled_door(parts);
    man_door(parts);

    // Workshop window, glazed over the lit interior.
    parts.push(prim(
        plane(
            [WIN_W + GLAZE_LAP, WIN_HEAD - WIN_SILL + GLAZE_LAP],
            window_card(WOOD_WHITE, 2, 2, 0.34, 0.1),
        ),
        [WIN_X, BASE_H + (WIN_SILL + WIN_HEAD) * 0.5, GLAZE_Z],
        quat_x(-FRAC_PI_2),
    ));
    parts.push(trim(
        [WIN_W + 0.3, 0.1, 0.1],
        [WIN_X, BASE_H + WIN_SILL, TRIM_Z],
        id_quat(),
    ));
    // Head trim across the vehicle bay, the one lintel big enough to read.
    parts.push(trim(
        [BAY_W + 0.4, 0.16, 0.11],
        [BAY_X, BASE_H + BAY_HEAD + 0.09, TRIM_Z],
        id_quat(),
    ));
    // Corner boards closing the two front corners.
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim(
            [0.14, BODY_H, 0.14],
            [sx * (W * 0.5 - 0.05), BASE_H + BODY_H * 0.5, FRONT + 0.02],
            id_quat(),
        ));
    }
}

/// The sectional door, rolled up under the head: the coiled drum and the last
/// panel hanging below it, with a track down each jamb.
///
/// The drum's axis runs along X, which [`quat_z`] at a right angle gives —
/// same trick as the kit's car tyres. A cylinder left on its default Y axis
/// would read as a bollard standing in the opening.
fn rolled_door(parts: &mut Vec<Generator>) {
    let drum_y = BASE_H + BAY_HEAD - 0.24;
    parts.push(prim(
        solid(cylinder_tapered(
            0.2,
            BAY_W - 0.12,
            12,
            0.0,
            enamel(ROLL_ENAMEL),
        )),
        [BAY_X, drum_y, GLAZE_Z + 0.14],
        quat_z(FRAC_PI_2),
    ));
    // The panel still hanging out of the coil, so the drum reads as a door
    // and not as a pipe.
    parts.push(prim(
        cuboid_tapered([BAY_W - 0.16, 0.18, 0.07], 0.0, enamel([0.70, 0.70, 0.67])),
        [BAY_X, drum_y - 0.22, GLAZE_Z + 0.06],
        id_quat(),
    ));
    // Tracks down the jambs, inside the reveal.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered(
                [0.07, BAY_HEAD - 0.2, 0.09],
                0.0,
                enamel([0.55, 0.56, 0.58]),
            ),
            [
                BAY_X + sx * (BAY_W * 0.5 - 0.07),
                BASE_H + (BAY_HEAD - 0.2) * 0.5,
                GLAZE_Z + 0.06,
            ],
            id_quat(),
        ));
    }
}

/// The man-door: a leaf set back in its reveal, a tinted vision light and a
/// handle proud of it.
///
/// The light is [`tinted_glass`], a dark **solid**, not a `Window` card — a
/// card on a 0.4 m panel with a door leaf immediately behind it would mask its
/// panes away onto the timber, which is exactly the "frame over nothing" the
/// idiom warns about. At this size a tinted solid reads as glass from any
/// angle and costs nothing.
fn man_door(parts: &mut Vec<Generator>) {
    let leaf_z = FRONT + WALL_T * 0.55;
    parts.push(prim(
        solid(cuboid_tapered(
            [MAN_W - 0.06, MAN_HEAD - 0.04, 0.07],
            0.0,
            enamel(MAN_PAINT),
        )),
        [MAN_X, BASE_H + (MAN_HEAD - 0.04) * 0.5, leaf_z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.42, 0.42, 0.05], 0.0, tinted_glass(GLASS_TINT)),
        [MAN_X, BASE_H + MAN_HEAD - 0.5, leaf_z - 0.06],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.05, 0.14, 0.05], 0.0, enamel([0.70, 0.64, 0.40])),
        [MAN_X + 0.33, BASE_H + 1.0, leaf_z - 0.07],
        id_quat(),
    ));
    // Casing round the opening — two jambs and a head, standing proud.
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim(
            [0.12, MAN_HEAD + 0.2, 0.1],
            [
                MAN_X + sx * (MAN_W * 0.5 + 0.06),
                BASE_H + (MAN_HEAD + 0.2) * 0.5,
                TRIM_Z,
            ],
            id_quat(),
        ));
    }
    parts.push(trim(
        [MAN_W + 0.36, 0.13, 0.1],
        [MAN_X, BASE_H + MAN_HEAD + 0.1, TRIM_Z],
        id_quat(),
    ));
}

/// What the open door and the window show: a lit workshop with a bench under a
/// pegboard, a shelf run and stacked stock.
///
/// Held toward the *front* half of the floor on purpose. The bay is 6 m deep
/// and anything against the back wall is six metres behind the opening, small
/// and dim; the bench and the shelves sit where a pane frames a recognisable
/// object (#972 lesson 6).
fn workshop(parts: &mut Vec<Generator>, inner: [f32; 2]) {
    // Dim lining and ceiling — the envelope everything else reads against.
    parts.push(prim(
        cuboid_tapered(
            [inner[0], BODY_H - 0.25, 0.08],
            0.0,
            // Warmer than the floor and the ceiling on purpose: three surfaces
            // at one tone make the bay a flat grey box however well lit it is.
            lit_interior([0.29, 0.26, 0.22], 0.11),
        ),
        [0.0, BASE_H + BODY_H * 0.5, D * 0.5 - WALL_T - 0.06],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered(
            [inner[0], 0.08, inner[1]],
            0.0,
            lit_interior([0.20, 0.20, 0.19], 0.09),
        ),
        [0.0, BASE_H + BODY_H - 0.14, 0.0],
        id_quat(),
    ));
    // Strip light, hung *below* the opening's head. Fixed to the ceiling it
    // sat at 2.98 against a 2.75 head, so the infill over the bay hid the one
    // thing that was meant to say the workshop is lit — the same "what does
    // the camera see through the opening" question as the house's blinds, from
    // the inside. A garage light hangs on drop rods anyway.
    parts.push(prim(
        cuboid_tapered([2.6, 0.09, 0.3], 0.0, glow([1.0, 0.94, 0.78], 1.8)),
        [BAY_X, BASE_H + 1.78, -0.5],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered(
                [0.04, 0.9, 0.04],
                0.0,
                lit_interior([0.26, 0.26, 0.25], 0.1),
            ),
            [BAY_X + sz * 1.1, BASE_H + 2.38, -0.5],
            id_quat(),
        ));
    }

    // A lamp clamped to the bench.
    //
    // The strip light above is honest but nearly unseeable from the pavement:
    // it hangs at the opening's head height, and the rolled door's drum sits
    // right across that sightline — a real garage hides its own ceiling light
    // the same way. A small source down at bench level cannot be occluded by
    // anything, so *something* in the bay is visibly lit from any angle.
    parts.push(prim(
        cuboid_tapered([0.16, 0.14, 0.14], 0.0, glow([1.0, 0.92, 0.72], 1.6)),
        [BAY_X + 1.1, BASE_H + 1.05, 1.6],
        id_quat(),
    ));
    // Bench along the back of the bay, with a pegboard over it.
    parts.push(prim(
        cuboid_tapered([3.2, 0.9, 0.65], 0.0, lit_interior([0.40, 0.32, 0.24], 0.3)),
        [BAY_X, BASE_H + 0.5, 1.9],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered(
            [3.0, 1.0, 0.06],
            0.0,
            lit_interior([0.26, 0.31, 0.34], 0.26),
        ),
        [BAY_X, BASE_H + 1.6, 2.3],
        id_quat(),
    ));
    // Shelf run down the far wall, and stock on it.
    parts.push(prim(
        cuboid_tapered(
            [0.55, 1.9, 2.4],
            0.0,
            lit_interior([0.36, 0.33, 0.29], 0.26),
        ),
        [-W * 0.5 + WALL_T + 0.32, BASE_H + 0.95, 0.2],
        id_quat(),
    ));
    for (i, (z, y, c)) in [
        (-0.4_f32, 1.35_f32, [0.62, 0.28, 0.20_f32]),
        (0.35, 1.35, [0.28, 0.42, 0.55]),
        (0.9, 0.6, [0.58, 0.52, 0.24]),
    ]
    .into_iter()
    .enumerate()
    {
        parts.push(prim(
            cuboid_tapered([0.4, 0.34, 0.42], 0.0, lit_interior(c, 0.42)),
            [-W * 0.5 + WALL_T + 0.3 + i as f32 * 0.02, BASE_H + y, z],
            id_quat(),
        ));
    }
    // A drum of something in the corner, because a workshop is never tidy.
    parts.push(prim(
        solid(cylinder_tapered(
            0.28,
            0.8,
            12,
            0.0,
            lit_interior([0.30, 0.38, 0.30], 0.24),
        )),
        [W * 0.5 - WALL_T - 0.45, BASE_H + 0.4, 1.9],
        id_quat(),
    ));
}

/// Front-gable shingle roof, its eaves fascia and the two barge boards down
/// the gable.
///
/// The ridge runs along **Z**, which is what `taper_xz` pinching *X* gives, so
/// the triangle faces the door — the elevation the settlement placer and the
/// render tool both look at. The roof sits a few centimetres into the walls,
/// because a base face flush with the wall tops would be two coplanar
/// horizontal faces fighting over the same plane.
fn roof() -> Generator {
    let base_y = BASE_H + BODY_H - 0.05;
    let deck = prim(
        solid(cuboid_tapered_xz(
            [W + EAVE * 2.0, ROOF_H, D + EAVE * 2.0],
            [0.94, 0.0],
            shingle(ROOF_GREY),
        )),
        [0.0, base_y + ROOF_H * 0.5, 0.0],
        id_quat(),
    );

    // Barge boards following the gable slope, on the hero face. The slope is
    // the roof's own geometry, so the angle is derived from it rather than
    // guessed: a board authored at a hand-picked tilt drifts the moment the
    // roof's rise or overhang changes.
    let half = (W + EAVE * 2.0) * 0.5;
    let pitch = (ROOF_H / half).atan();
    let run = (half * half + ROOF_H * ROOF_H).sqrt();
    let mut parts = vec![trim(
        [W + EAVE * 2.0 + 0.06, 0.13, D + EAVE * 2.0 + 0.06],
        [0.0, base_y - 0.02, 0.0],
        id_quat(),
    )];
    for sx in [-1.0_f32, 1.0] {
        parts.push(trim(
            [run, 0.16, 0.1],
            [
                sx * half * 0.5,
                base_y + ROOF_H * 0.5,
                -(D + EAVE * 2.0) * 0.5 - 0.04,
            ],
            quat_z(-sx * pitch),
        ));
    }
    nest(deck, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DetachedGarage.build(""), "detached_garage");
    }

    fn walk(g: &Generator, at: [f32; 3], f: &mut impl FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// LESSON 1: the one `Window` card lives on a flat quad at `uv_scale` 1.0,
    /// laps its opening, and no solid carries one.
    ///
    /// The side window and the man-door's vision light were both cards on
    /// cuboids before the overhaul, masking their panes away onto the solid
    /// wall behind them. The window is now a real opening; the vision light is
    /// a tinted solid, which is what a 0.4 m pane over a door leaf wants.
    #[test]
    fn the_only_card_is_the_workshop_window() {
        let mut planes = 0;
        walk(
            &DetachedGarage.build(""),
            [0.0; 3],
            &mut |g, _| match &g.kind {
                GeneratorKind::Plane { size, material, .. }
                    if matches!(material.texture, SovereignTextureConfig::Window(_)) =>
                {
                    assert_eq!(
                        material.uv_scale.0, 1.0,
                        "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                    );
                    assert!(
                        size.0[0] > WIN_W && size.0[1] > WIN_HEAD - WIN_SILL,
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
            },
        );
        assert_eq!(planes, 1, "expected exactly the workshop window glazed");
    }

    /// LESSON 1's other half: the door is open, so there has to be a workshop
    /// worth seeing behind it — and it has to sit where the opening frames it,
    /// not against the back wall six metres in.
    #[test]
    fn the_workshop_stands_where_the_opening_frames_it() {
        let mut near = 0;
        walk(&DetachedGarage.build(""), [0.0; 3], &mut |g, pos| {
            let m = match &g.kind {
                GeneratorKind::Cuboid { material, .. }
                | GeneratorKind::Cylinder { material, .. } => material,
                _ => return,
            };
            if m.emission_strength.0 > 0.2 && m.emission_strength.0 < 1.0 && pos[2] < 2.6 {
                near += 1;
            }
        });
        assert!(
            near >= 5,
            "only {near} lit pieces stand in the front of the bay — the open \
             door will read as a black hole"
        );
    }

    /// LESSON 2: the elevation is clad in one pass, and its courses run
    /// unbroken.
    ///
    /// Box projection is prim-local and centred on each prim's own bounds, so
    /// without an offset the eleven slabs of this shell each restart their
    /// courses at their own centre and every joint reads as a break. And
    /// `PlankConfig::stagger` above 0.01 adds a hard-coded three-butt-joints-
    /// per-tile grid that turns lap siding into coarse masonry.
    #[test]
    fn every_siding_surface_sits_in_the_world_course_frame() {
        let mut surfaces = Vec::new();
        walk(&DetachedGarage.build(""), [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { material, .. } = &g.kind
                && let SovereignTextureConfig::Plank(cfg) = &material.texture
                && cfg.plank_count.0 == super::super::SIDING_COURSES
            {
                surfaces.push((pos, material.clone()));
            }
        });
        assert_eq!(
            surfaces.len(),
            11,
            "expected 3 shell walls + 4 piers + 3 infills + the window's sill wall"
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
            let SovereignTextureConfig::Plank(cfg) = &m.texture else {
                unreachable!("filtered to Plank above");
            };
            assert_eq!(
                cfg.stagger.0, 0.0,
                "siding slab at {pos:?} carries end joints — three per tile, \
                 hard-coded, which reads as brick not board"
            );
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

    /// The front wall's slabs tile the elevation exactly: they fill it edge to
    /// edge with no gap daylight shows through, and — the failure that
    /// z-fights — no two of them overlap in the wall plane.
    #[test]
    fn the_front_wall_slabs_tile_without_overlapping() {
        let mut spans: Vec<([f32; 2], [f32; 2])> = Vec::new();
        walk(&DetachedGarage.build(""), [0.0; 3], &mut |g, pos| {
            if let GeneratorKind::Cuboid { size, material, .. } = &g.kind
                && matches!(material.texture, SovereignTextureConfig::Plank(_))
                && (pos[2] - FRONT_MID).abs() < 1e-3
            {
                spans.push((
                    [pos[0] - size.0[0] * 0.5, pos[0] + size.0[0] * 0.5],
                    [pos[1] - size.0[1] * 0.5, pos[1] + size.0[1] * 0.5],
                ));
            }
        });
        assert_eq!(spans.len(), 8, "expected 4 piers + 3 infills + 1 sill wall");
        let overlaps = |p: [f32; 2], q: [f32; 2]| p[0].max(q[0]) < p[1].min(q[1]) - 1e-4;
        for (i, a) in spans.iter().enumerate() {
            for b in &spans[i + 1..] {
                assert!(
                    !(overlaps(a.0, b.0) && overlaps(a.1, b.1)),
                    "front slabs {a:?} and {b:?} share the wall plane and will z-fight"
                );
            }
        }
        // Total slab area plus the three openings must be the whole elevation.
        let slab_area: f32 = spans
            .iter()
            .map(|(x, y)| (x[1] - x[0]) * (y[1] - y[0]))
            .sum();
        let openings = BAY_W * BAY_HEAD + MAN_W * MAN_HEAD + WIN_W * (WIN_HEAD - WIN_SILL);
        assert!(
            (slab_area + openings - W * BODY_H).abs() < 1e-3,
            "slabs cover {slab_area} + {openings} of openings, not the {} m² elevation",
            W * BODY_H
        );
    }

    /// LESSON 3, the editability contract: the sub-assemblies a gizmo drag has
    /// to move as one. It is what silently breaks when a later part is added at
    /// the wrong level, and no render shows it.
    #[test]
    fn the_tree_stands_the_way_the_garage_does() {
        fn size(g: &Generator) -> usize {
            1 + g.children.iter().map(size).sum::<usize>()
        }
        let root = DetachedGarage.build("");
        // Slab's own children: the apron, the shell, the buried footing, and
        // four shrub clumps.
        assert_eq!(root.children.len(), 7, "slab children");
        let shell = &root.children[1];
        assert!(
            size(shell) > 30,
            "the shell subtree lost its walls or its workshop"
        );
        // The roof carries its own fascia and both barge boards.
        let roof = shell
            .children
            .iter()
            .find(|c| size(c) == 4)
            .expect("roof → fascia + two barge boards");
        assert_eq!(roof.children.len(), 3);
        // And the barges are tilted while the roof itself is not: a tilted
        // sub-root would spin everything under it.
        assert_eq!(
            roof.transform.rotation.0,
            id_quat().0,
            "the roof must stay axis-aligned — it carries the barge boards"
        );
        assert_eq!(
            roof.children
                .iter()
                .filter(|c| c.transform.rotation.0[2].abs() > 1e-4)
                .count(),
            2,
            "expected exactly the two barge boards tilted"
        );
    }

    /// The barge boards follow the roof's *actual* slope. Authored at a
    /// hand-picked angle they drift silently the moment the rise or the
    /// overhang changes, and a board floating off its own gable is the kind of
    /// thing only a render catches — once someone happens to look.
    #[test]
    fn the_barge_boards_follow_the_gable() {
        let expected = (ROOF_H / ((W + EAVE * 2.0) * 0.5)).atan();
        let root = DetachedGarage.build("");
        let mut tilts = Vec::new();
        walk(&root, [0.0; 3], &mut |g, _| {
            let q = g.transform.rotation.0;
            if q[2].abs() > 1e-4 && q[0].abs() < 1e-4 {
                // Half-angle from the z component of the quaternion.
                tilts.push(2.0 * q[2].abs().asin());
            }
        });
        // The rolled door's drum also turns about Z, by a right angle.
        tilts.retain(|t| (t - FRAC_PI_2).abs() > 1e-3);
        assert_eq!(tilts.len(), 2, "expected two barge boards");
        for t in tilts {
            assert!(
                (t - expected).abs() < 1e-3,
                "barge board at {t} rad does not match the {expected} rad gable"
            );
        }
    }
}
