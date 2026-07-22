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
    assemble, cuboid_tapered, glow, id_quat, plane, prim, quat_x, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
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

    prims.push(prim(
        solid(cuboid_tapered([W, BODY_H, WALL_T], 0.0, brick(BRICK_RED))),
        [0.0, mid_y, D * 0.5 - WALL_T * 0.5],
        id_quat(),
    ));
    // Side walls, shortened in Z so their ends never share a plane with the
    // front and back slabs' outer faces.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [WALL_T, BODY_H, D - WALL_T * 2.0],
                0.0,
                brick(BRICK_RED),
            )),
            [sx * (W * 0.5 - WALL_T * 0.5), mid_y, 0.0],
            id_quat(),
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
    prims.push(prim(
        solid(cuboid_tapered(
            [W + 0.3, 0.55, D + 0.3],
            0.0,
            brick([0.4, 0.22, 0.17]),
        )),
        [0.0, BASE_H + BODY_H + 0.4, 0.0],
        id_quat(),
    ));
}

/// The front wall, built as the four brick pieces that *frame* the opening —
/// two piers, a lintel, a stall riser — plus the glazing cards filling it.
fn shopfront(prims: &mut Vec<Generator>) {
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [PIER_W, BODY_H, WALL_T],
                0.0,
                brick(BRICK_RED),
            )),
            [
                sx * (W * 0.5 - PIER_W * 0.5),
                BASE_H + BODY_H * 0.5,
                FRONT_MID,
            ],
            id_quat(),
        ));
    }
    // Lintel over the opening, carrying the wall up to the parapet.
    let lintel_h = BASE_H + BODY_H - HEAD_Y;
    prims.push(prim(
        solid(cuboid_tapered(
            [OPEN_X1 - OPEN_X0, lintel_h, WALL_T],
            0.0,
            brick(BRICK_RED),
        )),
        [
            (OPEN_X0 + OPEN_X1) * 0.5,
            HEAD_Y + lintel_h * 0.5,
            FRONT_MID,
        ],
        id_quat(),
    ));
    // Stall riser under the display bay only.
    let riser_w = DOOR_X0 - OPEN_X0;
    prims.push(prim(
        solid(cuboid_tapered(
            [riser_w, SILL_Y - BASE_H, WALL_T],
            0.0,
            brick([0.4, 0.22, 0.17]),
        )),
        [OPEN_X0 + riser_w * 0.5, (BASE_H + SILL_Y) * 0.5, FRONT_MID],
        id_quat(),
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
}
