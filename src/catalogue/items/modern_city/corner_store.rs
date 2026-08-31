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
    self, assemble, cuboid_tapered, footing, glow, id_quat, lit_interior, plane, prim, quat_x,
    solid, window_card, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
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

/// The store's brickwork: the kit's [`brick`] with its courses laid **flat**,
/// at a real brick's size, and its bond continued into the wall's own frame.
///
/// The whole recipe — the inverted aspect, the ten-row tile, the integral
/// half-bond, the tamed cell jitter and the per-face offset that carries one
/// frame across the joints — now lives in [`util::bonded_brick`], which every
/// later brick entry shares. What is local here is only the brick's *size*.
///
/// [`util::bonded_brick`]: crate::catalogue::items::util::bonded_brick
fn bonded_brick(color: [f32; 3], center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    util::bonded_brick(brick(color), BRICK_LEN, face, center)
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

/// Dim warm interior surface — the shared [`lit_interior`] idiom, which this
/// entry is the reference for: the shell is enclosed and nothing lights it,
/// so the surfaces seen through the glazing carry a low self-lit term of
/// their own. Without it the openings read as black rectangles and every
/// bit of work behind the glass is invisible.
fn interior(color: [f32; 3], lit: f32) -> SovereignMaterialSettings {
    lit_interior(color, lit)
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

    // Buried footing under the concrete base.
    prims.push(footing(W + 0.4, D + 0.4, [0.0, 0.0], 5.0));

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
    /// own projection of the slab's position ([`util::face_uv_offset`]). A slab
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
        let expected = |face: FaceKey, pos: &[f32; 3]| util::face_uv_offset(face, *pos);
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

    /// #968 / #1167: the bricks lie flat and the bond tiles.
    ///
    /// This test used to pin two numbers instead — `cols >= 4` and
    /// `cell_variance <= 0.15` — neither of which was about how the wall
    /// should look. The generator hashed each brick's raw cell index, so the
    /// one straddling the tile's U seam drew as two half-bricks of different
    /// colour, and the only defence available here was dilution: more bricks
    /// per tile, less colour between them. symbios-texture 0.4.3 wraps the
    /// index modulo the column count (its #12), so those two ceilings were
    /// holding a fixed defect down and the standing catalogue overhaul (#972)
    /// no longer has to re-apply them item by item.
    ///
    /// What is left is what the bond actually requires: a brick wider than it
    /// is tall, and a stagger that carries across the V seam. The V constraint
    /// is the generator's and it is *not* fixed — `scale × row_offset` must be
    /// a whole number or course 0 sits on course `scale - 1` at the wrong
    /// offset (symbios-texture #14).
    #[test]
    fn the_brick_bond_lies_flat_and_tiles() {
        let mut slabs = Vec::new();
        brick_slabs(&CornerStore.build(""), [0.0; 3], &mut slabs);
        assert!(!slabs.is_empty(), "the corner store is a brick building");
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
            let stagger = cfg.scale.0 * cfg.row_offset.0;
            assert!(
                (stagger - stagger.round()).abs() < 1e-6,
                "slab at {pos:?}: scale × row_offset = {stagger} does not tile in V"
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
