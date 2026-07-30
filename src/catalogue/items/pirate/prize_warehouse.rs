//! Prize Warehouse — where a taken cargo goes ashore.
//!
//! A tall bonded store on the quay: a rubble base course under ship-built
//! boarding, a cart door standing open on a lit floor of casks and bales, a
//! loading door at the head of the wall with a hoist beam projecting over it,
//! and a bale swinging on the fall.
//!
//! # The hoist beam is the subject
//!
//! Everything else here is a shed. What makes it a *warehouse* is the gib
//! projecting from the gable with a block on its end and a load hanging under
//! it — one assembly, at the top of the tallest wall, doing something. A
//! warehouse with a blank gable is a barn.
//!
//! So the beam is built as a working chain: gib → strop → block → fall →
//! hook → bale, each part seated on the one above it, and a guard walks that
//! chain rather than checking positions (#972 lesson 33 — for anything whose
//! whole read is a hanging load, assert the load is actually hung).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, bonded_boards, bonded_siding, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered,
    face_uv_offset, footing, id_quat, lit_interior, nest, plane, prim, quat_x, quat_z, solid,
    sphere, torus, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, GLASS_AMBER, HULL_OAK, HULL_TAR, IRON_BLACK, PORT_BAND,
    ROPE_HEMP, SHINGLE_GREY, STONE_LIME, STONE_QUAY, WHARF_GREY, ashlar, board, cobbles, fx, hemp,
    iron, lantern, pane_grid, sailcloth, shingle, strake,
};

/// Cobbled apron — the sub-root every footprint guard measures against.
///
/// Its DEPTH is set by the loading ramp, not by the shed. The ramp's run is
/// derived from the base course's height (even risers, so the flight always
/// lands flush), which means the apron has to be deep enough to hold whatever
/// that comes to — at 12 m the bottom tread hung 40 mm off the paving, which
/// the footprint guard caught and which no camera angle here would show.
const APRON: [f32; 3] = [13.0, 0.30, 13.4];
const GROUND: f32 = APRON[1];

/// Rubble base course, standing PROUD of the boarding above it.
///
/// A base course flush with its wall is a coplanar seam running the whole
/// perimeter on the most looked-at part of the building, and it is invisible
/// in a still (#972).
const BASE: [f32; 3] = [9.6, 1.05, 8.0];
const BASE_TOP: f32 = GROUND + BASE[1];
/// How far the base stands proud of the boarded wall.
const BASE_PROUD: f32 = 0.14;
/// How far the necking ring stands proud of the base course it caps.
const RING_PROUD: f32 = 0.07;

/// The boarded store above it: width, height to the wall plate, depth.
const WALL: [f32; 3] = [BASE[0] - BASE_PROUD * 2.0, 6.4, BASE[2] - BASE_PROUD * 2.0];
const PLATE: f32 = BASE_TOP + WALL[1];
/// Hero plane — the quay elevation.
const FRONT_Z: f32 = -WALL[2] * 0.5;

/// Cart door at the foot of the wall, and the loading door at its head.
const CART_W: f32 = 2.9;
const CART_H: f32 = 3.1;
const LOAD_W: f32 = 1.9;
const LOAD_H: f32 = 2.0;
/// Loading-door sill — a full storey up, which is what the hoist is for.
const LOAD_SILL: f32 = BASE_TOP + 3.7;

/// Office light beside the cart door.
const WIN_W: f32 = 1.2;
const WIN_H: f32 = 1.1;
const WIN_SILL: f32 = BASE_TOP + 1.4;

/// Reveal depth, and how far a card laps past its opening (#972 lesson 7).
const REVEAL: f32 = 0.26;
const CARD_LAP: f32 = 0.06;

/// How far the store's back lining stands in front of the rear wall.
const ROOM_BACK: f32 = FRONT_Z + 4.0;
/// How far interior surfaces stay behind the wall face they meet — the
/// coplanar rule's indoor half (#1028).
const FLOOR_INSET: f32 = 0.06;

/// Gable ridge height above the wall plate.
const RIDGE_H: f32 = 2.4;
/// How far the hoist gib projects from the gable.
const GIB_REACH: f32 = 2.2;
/// Height of the gib above the loading-door head — enough for the block, the
/// fall and a bale to hang clear of the sill.
const GIB_Y: f32 = PLATE + 0.55;

pub struct PrizeWarehouse;

impl CatalogueEntry for PrizeWarehouse {
    fn slug(&self) -> &'static str {
        "prize_warehouse"
    }
    fn name(&self) -> &'static str {
        "Prize Warehouse"
    }
    fn description(&self) -> &'static str {
        "A bonded store on the quay, its cart door open on a floor of casks and a bale swinging \
         from the hoist beam."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Ship-built boarding, standing up, in the shared world course frame.
fn clad(center: [f32; 3], color: [f32; 3]) -> crate::pds::SovereignMaterialSettings {
    bonded_boards(strake(color), FaceKey::SideNz, center)
}

/// The quay elevation: piers and infill framing the cart door, the office
/// light and the loading door above.
fn quay_elevation() -> Vec<Generator> {
    let z = FRONT_Z + WALL[2] * 0.25;
    let d = WALL[2] * 0.5;
    let mut out = Vec::new();

    // The cart door sits left of centre and the office light right of it, so
    // the elevation is not symmetrical — a warehouse is a working face, and a
    // symmetrical one reads as a chapel.
    let cart_x = -1.5_f32;
    let win_x = 3.1_f32;

    let mut edges = vec![-WALL[0] * 0.5, WALL[0] * 0.5];
    edges.push(cart_x - CART_W * 0.5);
    edges.push(cart_x + CART_W * 0.5);
    edges.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    for pair in edges.chunks(2) {
        let (a, b) = (pair[0], pair[1]);
        let w = b - a;
        if w < 0.05 {
            continue;
        }
        let cx = (a + b) * 0.5;
        if (cx - cart_x).abs() < 0.05 {
            continue;
        }
        let c = [cx, BASE_TOP + CART_H * 0.5, z];
        out.push(prim(
            solid(cuboid_tapered([w, CART_H, d], 0.0, clad(c, HULL_OAK))),
            c,
            id_quat(),
        ));
    }
    // Everything above the cart-door head, in one band.
    let band_c = [0.0, (BASE_TOP + CART_H + PLATE) * 0.5, z];
    out.push(prim(
        solid(cuboid_tapered(
            [WALL[0], PLATE - BASE_TOP - CART_H, d],
            0.0,
            clad(band_c, HULL_OAK),
        )),
        band_c,
        id_quat(),
    ));

    // Office light, punched through that band over a small lit counting room.
    out.push(prim(
        plane(
            [WIN_W + CARD_LAP * 2.0, WIN_H + CARD_LAP * 2.0],
            pane_grid(GLASS_AMBER, 0.0, (2, 2)),
        ),
        [win_x, WIN_SILL + WIN_H * 0.5, FRONT_Z + REVEAL],
        quat_x(-FRAC_PI_2),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [WIN_W + 0.6, WIN_H + 0.5, 0.1],
            0.0,
            lit_interior([0.50, 0.35, 0.20], 0.46),
        )),
        [win_x, WIN_SILL + WIN_H * 0.5, FRONT_Z + 0.9],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [WIN_W + 0.34, 0.12, 0.3],
            0.0,
            ashlar(STONE_LIME, 0x81),
        )),
        [win_x, WIN_SILL + 0.06, FRONT_Z + 0.08],
        id_quat(),
    ));

    // Loading door: a real hole at the head of the wall, its leaves swung
    // back against the boarding. A loading door needs no glazing — it is a
    // hole a bale goes through, and the bale is what fills it (#972 lesson
    // 24: ask what the real thing does).
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [LOAD_W * 0.5, LOAD_H - 0.08, 0.1],
                0.0,
                board(WHARF_GREY),
            )),
            [
                sx * (LOAD_W * 0.76),
                LOAD_SILL + LOAD_H * 0.5,
                FRONT_Z - 0.06,
            ],
            id_quat(),
        ));
    }
    // Lit loft behind the loading door — the sightline from the quay goes UP
    // through it, so what it frames is the loft floor and whatever is stacked
    // on it (#972 lesson 6's vertical half).
    out.push(prim(
        solid(cuboid_tapered(
            [LOAD_W + 1.4, 0.12, 2.2],
            0.0,
            lit_interior([0.46, 0.33, 0.20], 0.4),
        )),
        [0.0, LOAD_SILL + 0.06, FRONT_Z + 1.3],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [LOAD_W + 1.4, LOAD_H + 0.5, 0.1],
            0.0,
            lit_interior([0.52, 0.36, 0.21], 0.5),
        )),
        [0.0, LOAD_SILL + LOAD_H * 0.5, FRONT_Z + 2.3],
        id_quat(),
    ));
    for dx in [-0.7_f32, 0.55] {
        out.push(prim(
            solid(cuboid_tapered(
                [0.8, 0.62, 0.7],
                0.06,
                sailcloth(CANVAS_BONE, CANVAS_SHADE),
            )),
            [dx, LOAD_SILL + 0.43, FRONT_Z + 1.5],
            id_quat(),
        ));
    }
    // Sill beam under the loading door, which is what a bale is landed on.
    out.push(prim(
        solid(cuboid_tapered(
            [LOAD_W + 1.0, 0.2, 0.55],
            0.0,
            board(HULL_OAK),
        )),
        [0.0, LOAD_SILL - 0.1, FRONT_Z + 0.05],
        id_quat(),
    ));
    out
}

/// The store floor, seen through the open cart door.
fn store() -> Vec<Generator> {
    let mut out = vec![
        // Held FLOOR_INSET behind the wall face — run to FRONT_Z exactly, the
        // floor's leading edge and the elevation's front share one plane and
        // z-fight along the cart-door sill (#1028, same fault as the tavern).
        prim(
            solid(cuboid_tapered(
                [WALL[0] - 1.0, 0.1, ROOM_BACK - FRONT_Z - FLOOR_INSET],
                0.0,
                lit_interior([0.30, 0.25, 0.19], 0.16),
            )),
            [
                0.0,
                BASE_TOP + 0.05,
                (FRONT_Z + FLOOR_INSET + ROOM_BACK) * 0.5,
            ],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [WALL[0] - 1.0, CART_H, 0.12],
                0.0,
                lit_interior([0.48, 0.34, 0.20], 0.36),
            )),
            [0.0, BASE_TOP + CART_H * 0.5, ROOM_BACK],
            id_quat(),
        ),
    ];
    // Casks in two courses on a stillage, and bales beside them — held
    // forward of the lining so the depth reads (#972 lesson 6).
    for (course, y) in [(0_usize, 0.42_f32), (1, 1.2)] {
        for dx in [-2.4_f32, -1.5, -0.6] {
            let x = dx + course as f32 * 0.45;
            out.push(prim(
                solid(cylinder_tapered(0.4, 0.9, 12, -0.1, board(HULL_OAK))),
                [x, BASE_TOP + y, ROOM_BACK - 1.1],
                quat_z(FRAC_PI_2),
            ));
            out.push(prim(
                torus(0.04, 0.41, iron(IRON_BLACK, 0x82)),
                [x, BASE_TOP + y, ROOM_BACK - 1.1],
                quat_z(FRAC_PI_2),
            ));
        }
    }
    for (i, dx) in [1.2_f32, 2.3, 1.75].into_iter().enumerate() {
        out.push(prim(
            solid(cuboid_tapered(
                [0.95, 0.7, 0.85],
                0.05,
                sailcloth(CANVAS_BONE, CANVAS_SHADE),
            )),
            [dx, BASE_TOP + 0.35 + (i / 2) as f32 * 0.72, ROOM_BACK - 1.3],
            id_quat(),
        ));
    }
    out.push(lantern([-3.1, BASE_TOP + 1.9, ROOM_BACK - 0.7], 0.56, 0x83));
    out
}

/// The hoist: gib, strop, block, fall, hook and the bale on the end of it.
///
/// Written as a chain from the gable outward, because that is what the guard
/// walks. Each piece is placed from the one above it rather than from a
/// height of its own, so nothing can end up hanging on air.
fn hoist() -> Vec<Generator> {
    let tip_z = FRONT_Z - GIB_REACH;
    let mut out = Vec::new();

    // Gib: a heavy beam out through the gable.
    out.push(prim(
        solid(cuboid_tapered(
            [0.26, 0.3, GIB_REACH + 1.0],
            0.0,
            board(HULL_OAK),
        )),
        [0.0, GIB_Y, FRONT_Z - GIB_REACH * 0.5 + 0.5],
        id_quat(),
    ));
    // Knee brace back to the wall — the member that makes a cantilever look
    // like it could carry something.
    // Top toward the gib tip (−Z, outward): `quat_x(θ)` turns +Y toward +Z
    // for positive θ, so the outward lean is NEGATIVE — the first build had
    // the sign flipped and the brace leaned back into the wall it sprang
    // from, propping nothing (#1028's rotation family; the strut helper now
    // exists for exactly this class, but a cuboid brace keeps its named
    // angle and the comment carries the handedness).
    out.push(prim(
        solid(cuboid_tapered([0.18, 1.5, 0.22], 0.0, board(HULL_OAK))),
        [0.0, GIB_Y - 0.75, FRONT_Z - 0.62],
        quat_x(-0.62),
    ));
    // Strop and block at the tip.
    let block_y = GIB_Y - 0.42;
    out.push(prim(
        torus(0.035, 0.16, iron(IRON_BLACK, 0x84)),
        [0.0, GIB_Y - 0.17, tip_z],
        quat_z(FRAC_PI_2),
    ));
    out.push(prim(
        solid(cuboid_tapered([0.14, 0.34, 0.3], 0.1, board(WHARF_GREY))),
        [0.0, block_y, tip_z],
        id_quat(),
    ));
    out.push(prim(
        solid(cylinder_tapered(0.1, 0.16, 10, 0.0, iron(IRON_BLACK, 0x85))),
        [0.0, block_y, tip_z],
        quat_z(FRAC_PI_2),
    ));
    // Fall: the rope from the block down to the hook.
    let hook_y = LOAD_SILL + 1.0;
    let fall_len = block_y - hook_y;
    out.push(prim(
        cylinder_tapered(0.03, fall_len, 6, 0.0, hemp(ROPE_HEMP)),
        [0.0, (block_y + hook_y) * 0.5, tip_z],
        id_quat(),
    ));
    out.push(prim(
        torus(0.028, 0.1, iron(IRON_BLACK, 0x86)),
        [0.0, hook_y, tip_z],
        quat_x(FRAC_PI_2),
    ));
    // And the load on the end of it — the whole point of the assembly.
    out.push(prim(
        solid(cuboid_tapered(
            [1.0, 0.8, 0.9],
            0.05,
            sailcloth(CANVAS_BONE, CANVAS_SHADE),
        )),
        [0.0, hook_y - 0.5, tip_z],
        id_quat(),
    ));
    for dy in [-0.28_f32, 0.18] {
        out.push(prim(
            cylinder_tapered(0.022, 1.05, 6, 0.0, hemp(ROPE_HEMP)),
            [0.0, hook_y - 0.5 + dy, tip_z],
            quat_z(FRAC_PI_2),
        ));
    }
    // The hauling part, belayed to a cleat on the wall.
    out.push(prim(
        cylinder_tapered(0.028, GIB_Y - BASE_TOP - 1.4, 6, 0.0, hemp(ROPE_HEMP)),
        [0.55, (GIB_Y + BASE_TOP + 1.4) * 0.5, FRONT_Z - 0.12],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [0.34, 0.1, 0.12],
            0.0,
            iron(IRON_BLACK, 0x87),
        )),
        [0.55, BASE_TOP + 1.4, FRONT_Z - 0.1],
        id_quat(),
    ));
    out
}

fn build_tree() -> Generator {
    let apron_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0x80);
    paving.uv_offset = face_uv_offset(FaceKey::Top, apron_c);

    let base_c = [0.0, GROUND + BASE[1] * 0.5, 0.0];
    let mut on_base = Vec::new();
    on_base.extend(quay_elevation());
    on_base.extend(store());
    on_base.extend(hoist());

    // Flanks and rear as single slabs — no openings, so a punched grid would
    // cost twenty prims to say nothing.
    for sx in [-1.0_f32, 1.0] {
        let c = [sx * (WALL[0] * 0.5 - 0.2), BASE_TOP + WALL[1] * 0.5, 0.3];
        on_base.push(prim(
            solid(cuboid_tapered(
                [0.4, WALL[1], WALL[2] - 0.6],
                0.0,
                clad(c, HULL_OAK),
            )),
            c,
            id_quat(),
        ));
    }
    let back_c = [0.0, BASE_TOP + WALL[1] * 0.5, WALL[2] * 0.5 - 0.2];
    on_base.push(prim(
        solid(cuboid_tapered(
            [WALL[0], WALL[1], 0.4],
            0.0,
            clad(back_c, HULL_OAK),
        )),
        back_c,
        id_quat(),
    ));

    // Gable roof, ridged ALONG the building (Z pinched alone) so the quay
    // elevation is a gable end — which is what puts the loading door and its
    // hoist under an apex instead of under an eaves line.
    on_base.push(prim(
        solid(cuboid_tapered_xz(
            [WALL[0] + 0.5, RIDGE_H, WALL[2] + 0.5],
            [0.94, 0.0],
            shingle(SHINGLE_GREY),
        )),
        [0.0, PLATE + RIDGE_H * 0.5, 0.0],
        id_quat(),
    ));
    // Gable infill under the apex, front and back, clad like the walls.
    for sz in [-1.0_f32, 1.0] {
        let c = [0.0, PLATE + RIDGE_H * 0.42, sz * (WALL[2] * 0.5 - 0.12)];
        on_base.push(prim(
            solid(cuboid_tapered_xz(
                [WALL[0], RIDGE_H * 0.84, 0.24],
                [0.9, 0.0],
                clad(c, HULL_OAK),
            )),
            c,
            id_quat(),
        ));
    }
    // Barge boards taking their tilt from the roof's own rise and run — a
    // hand-picked angle silently stops matching its gable the moment either
    // changes (#972's garage note).
    let rake = (RIDGE_H / (WALL[0] * 0.5 + 0.25)).atan();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let run = (WALL[0] * 0.5 + 0.25) / rake.cos();
            on_base.push(prim(
                solid(cuboid_tapered([run, 0.2, 0.14], 0.0, board(WHARF_GREY))),
                [
                    sx * (WALL[0] * 0.25 + 0.12),
                    PLATE + RIDGE_H * 0.5,
                    sz * (WALL[2] * 0.5 + 0.3),
                ],
                quat_z(-sx * rake),
            ));
        }
    }
    // Necking ring at the head of the base course, PROUD of it on all four
    // sides. The first build sized it exactly `BASE` — four faces coplanar
    // with the four faces of the block it wrapped, z-fighting round the whole
    // perimeter (#1028). A ring's projection goes into its SIZE (#972 lesson
    // 31); flush is not a ring, it is a stripe painted on the seam.
    let ring_c = [0.0, BASE_TOP - 0.1, 0.0];
    on_base.push(prim(
        solid(with_face(
            cuboid_tapered(
                [BASE[0] + RING_PROUD * 2.0, 0.2, BASE[2] + RING_PROUD * 2.0],
                0.0,
                ashlar(STONE_LIME, 0x88),
            ),
            FaceKey::Top,
            bonded_siding(ashlar(STONE_LIME, 0x88), FaceKey::Top, ring_c),
        )),
        ring_c,
        id_quat(),
    ));

    let mut carried = vec![
        footing(BASE[0], BASE[2], [0.0, 0.0], 8.0),
        nest(
            prim(
                solid(cuboid_tapered(BASE, 0.0, cobbles(STONE_QUAY, 0x89))),
                base_c,
                id_quat(),
            ),
            on_base,
        ),
    ];
    // A loading ramp up to the cart door, with even risers derived from the
    // base's own height — the authored quantity is the riser, because that is
    // the thing that has to be right; picking the tread count instead leaves
    // the riser to fall out at whatever the numbers give.
    let rise = BASE_TOP - GROUND;
    let treads = (rise / 0.19).round().max(2.0) as usize;
    let riser = rise / treads as f32;
    for i in 0..treads {
        let h = riser * (i + 1) as f32;
        carried.push(prim(
            solid(cuboid_tapered(
                [CART_W + 0.9, h, 0.34],
                0.0,
                cobbles(STONE_QUAY, 0x8A + i as u32),
            )),
            [
                -1.5,
                GROUND + h * 0.5,
                FRONT_Z - 0.14 - (treads - 1 - i) as f32 * 0.34 - 0.17,
            ],
            id_quat(),
        ));
    }
    // Quayside dunnage — on the apron AND clear of the base course. The
    // first build derived only from the apron's edge and walked its casks
    // straight into the base's corner (#1028; #972 lesson 8's other half,
    // the same fault the tavern's tuns had). Everything here stays forward
    // of the base's front face, which is the one line all four pieces can
    // be stated against.
    let dunnage_z = -(BASE[2] * 0.5 + 1.1);
    for (i, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let x = sx * (APRON[0] * 0.5 - 1.5);
        carried.push(prim(
            solid(cuboid_tapered([1.2, 0.5, 1.0], 0.06, board(DECK_HOLY))),
            [x, GROUND + 0.25, dunnage_z - 0.7],
            id_quat(),
        ));
        carried.push(prim(
            solid(cylinder_tapered(0.38, 0.82, 12, -0.08, board(HULL_OAK))),
            [x + sx * 0.1, GROUND + 0.41, dunnage_z + 0.55],
            id_quat(),
        ));
        carried.push(prim(
            torus(0.05, 0.3, hemp(ROPE_HEMP)),
            [x - sx * 1.0, GROUND + 0.05, dunnage_z - 0.4],
            id_quat(),
        ));
        carried.push(prim(
            sphere(0.14, 3, iron(IRON_BLACK, 0x90 + i as u32)),
            [x - sx * 1.0, GROUND + 0.14, dunnage_z + 0.6],
            id_quat(),
        ));
    }

    let mut root = nest(
        prim(
            solid(cuboid_tapered(APRON, 0.0, paving)),
            apron_c,
            id_quat(),
        ),
        carried,
    );
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered([0.9, 0.5, 0.9], 0.1, strake(HULL_TAR))),
            [3.6, GROUND + 0.25, 2.6],
            id_quat(),
        ),
    );
    root.audio = fx::rigging_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_cards_do_not_overlap, assert_no_glazing_on_solids, assert_no_tilted_parents,
        assert_sanitize_stable, has_emissive, window_cards,
    };

    fn built() -> Generator {
        PrizeWarehouse.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "prize_warehouse");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "prize_warehouse");
    }

    /// One card, over the office light, and nothing solid wearing one.
    ///
    /// The count matters as much as the prohibition here (#972 lesson 20 plus
    /// its census half): a warehouse's other two openings are a cart door and
    /// a loading door, and both are supposed to be *holes*. If a second card
    /// ever appears it means somebody glazed a doorway.
    #[test]
    fn the_only_glazing_is_the_office_light() {
        let g = built();
        assert_no_glazing_on_solids(&g, "prize_warehouse");
        assert_cards_do_not_overlap(&g, "prize_warehouse");
        let cards = window_cards(&g);
        assert_eq!(
            cards.len(),
            1,
            "a warehouse has one window and two doorways; found {} cards",
            cards.len()
        );
        assert!(
            cards[0].size[0] > WIN_W + CARD_LAP && cards[0].size[1] > WIN_H + CARD_LAP,
            "the office card does not lap its opening"
        );
    }

    /// The hoist is an unbroken chain from the gable to the load (#972 lesson
    /// 33, in its hanging form).
    ///
    /// This is the assembly the whole entry is built around, and every part of
    /// it is placed relative to the one above — so the way it fails is not a
    /// wrong number but a broken link, and a bale floating a foot under its
    /// own hook is invisible in a four-angle sheet. Walked as levels rather
    /// than compared against constants.
    #[test]
    fn the_bale_hangs_from_the_gib_by_an_unbroken_fall() {
        let g = built();
        let solids = measure::solids(&g);
        let at_tip =
            |p: &measure::SolidPiece| (p.bounds.center().z - (FRONT_Z - GIB_REACH)).abs() < 0.4;
        let gib = solids
            .iter()
            .find(|p| p.bounds.size().z > GIB_REACH && p.bounds.center().y > PLATE)
            .expect("the gib is in the tree");
        let block = solids
            .iter()
            .filter(|p| at_tip(p) && p.bounds.max.y < gib.bounds.min.y + 1e-3)
            .max_by(|a, b| a.bounds.max.y.partial_cmp(&b.bounds.max.y).expect("finite"))
            .expect("a block hangs under the gib");
        let bale = solids
            .iter()
            .filter(|p| at_tip(p) && p.bounds.size().x > 0.8 && p.bounds.size().y > 0.6)
            .min_by(|a, b| a.bounds.min.y.partial_cmp(&b.bounds.min.y).expect("finite"))
            .expect("a bale hangs on the fall");
        // Gib → block: the block's head reaches the beam.
        assert!(
            gib.bounds.min.y - block.bounds.max.y < 0.25,
            "the block's head is {} below the gib at {} — it is hanging on air",
            block.bounds.max.y,
            gib.bounds.min.y
        );
        // Block → bale: a rope spans the whole gap, with no daylight at either
        // end.
        let fall = solids
            .iter()
            .filter(|p| at_tip(p) && p.bounds.size().x < 0.12 && p.bounds.size().y > 0.5)
            .max_by(|a, b| {
                a.bounds
                    .size()
                    .y
                    .partial_cmp(&b.bounds.size().y)
                    .expect("finite")
            })
            .expect("the fall is in the tree");
        assert!(
            block.bounds.min.y - fall.bounds.max.y < 0.2,
            "the fall does not reach its block"
        );
        assert!(
            fall.bounds.min.y - bale.bounds.max.y < 0.3,
            "the bale's head is {} and the fall ends at {} — the load is not \
             on the rope",
            bale.bounds.max.y,
            fall.bounds.min.y
        );
        // And the load hangs clear of the sill it is being landed on, or the
        // hoist is not hoisting.
        assert!(
            bale.bounds.min.y > LOAD_SILL - 0.6,
            "the bale has sunk below the loading sill"
        );
    }

    /// The store is lit and its contents stand forward of the back wall
    /// (#972 lesson 6).
    #[test]
    fn the_store_is_lit_and_its_goods_are_held_forward() {
        let g = built();
        assert!(has_emissive(&g), "the warehouse lost its lantern");
        let goods: Vec<_> = measure::solids(&g)
            .into_iter()
            .filter(|p| {
                let c = p.bounds.center();
                c.z > FRONT_Z && c.z < ROOM_BACK && c.y > BASE_TOP && c.y < BASE_TOP + CART_H
            })
            .collect();
        assert!(
            goods.len() > 8,
            "only {} things inside the store — a cart door onto an empty floor \
             is a darker rectangle on the wall",
            goods.len()
        );
        for p in &goods {
            assert!(
                p.bounds.center().z < ROOM_BACK - 0.3,
                "a cask at {:?} is against the back lining, where it is an \
                 unreadable speck from the quay",
                p.bounds.center()
            );
        }
    }

    /// The base course stands proud of the boarding it carries.
    ///
    /// Flush is a coplanar seam running the full perimeter, on the most
    /// looked-at part of the building, and it is invisible in a still.
    #[test]
    fn the_base_course_stands_proud_of_the_wall() {
        const _: () = assert!(
            BASE[0] > WALL[0] && BASE[2] > WALL[2],
            "the base course is not proud of the wall above it"
        );
        let g = built();
        let solids = measure::solids(&g);
        let base = solids
            .iter()
            .find(|p| (p.bounds.size().y - BASE[1]).abs() < 1e-3)
            .expect("the base course is in the tree");
        assert!(
            base.bounds.size().x > WALL[0] + 0.1,
            "the base measures {} against a {} wall",
            base.bounds.size().x,
            WALL[0]
        );
        // And the necking ring stands proud of the BASE in turn, on both
        // axes. Sized exactly equal, its four faces were coplanar with the
        // four faces of the block it wrapped — a z-fight round the whole
        // perimeter that shipped in-world (#1028).
        let ring = solids
            .iter()
            .find(|p| (p.bounds.size().y - 0.2).abs() < 0.02 && p.bounds.center().y < BASE_TOP)
            .expect("the necking ring is in the tree");
        assert!(
            ring.bounds.size().x > base.bounds.size().x + 0.05
                && ring.bounds.size().z > base.bounds.size().z + 0.05,
            "the ring ({} x {}) does not stand proud of the base ({} x {}) — \
             flush is four coplanar seams",
            ring.bounds.size().x,
            ring.bounds.size().z,
            base.bounds.size().x,
            base.bounds.size().z
        );
    }

    /// The store floor sits behind the wall face, and the dunnage stands
    /// clear of the base course (#1028 — both were coplanar/overlap faults
    /// visible in-world).
    #[test]
    fn the_floor_is_inset_and_the_dunnage_clears_the_base() {
        let g = built();
        let solids = measure::solids(&g);
        let floor = solids
            .iter()
            .find(|p| {
                let sz = p.bounds.size();
                sz.x > 7.0 && sz.y < 0.2 && (p.bounds.center().y - BASE_TOP).abs() < 0.3
            })
            .expect("the store floor is in the tree");
        assert!(
            floor.bounds.min.z > FRONT_Z + FLOOR_INSET * 0.5,
            "the floor's leading edge is at {} — on the wall face at {FRONT_Z}",
            floor.bounds.min.z
        );
        let bh = [BASE[0] * 0.5, BASE[2] * 0.5];
        let mut furniture = 0;
        for p in &solids {
            if !matches!(p.kind_tag, "Cylinder" | "Torus" | "Sphere") {
                continue;
            }
            let b = &p.bounds;
            // Feet on the apron — the store's own casks stand on the base, a
            // course of masonry higher (the tavern's selector lesson).
            if b.min.y > BASE_TOP - 0.05 || b.center().y < GROUND {
                continue;
            }
            furniture += 1;
            let hits = b.max.x > -bh[0] && b.min.x < bh[0] && b.max.z > -bh[1] && b.min.z < bh[1];
            assert!(
                !hits,
                "{} at {:?} runs into the base course (±{} x ±{})",
                p.kind_tag,
                b.center(),
                bh[0],
                bh[1]
            );
        }
        assert!(
            furniture >= 4,
            "only {furniture} pieces of dunnage examined — the selector has \
             stopped finding the casks and coils"
        );
    }

    /// Everything on the ground stands on the apron (#972 lessons 8 and 19).
    #[test]
    fn every_ground_part_stands_on_the_apron() {
        let g = built();
        let half = [APRON[0] * 0.5, APRON[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            if p.bounds.center().y > BASE_TOP + 0.5 {
                continue;
            }
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the apron in X",
                p.kind_tag,
                p.bounds.center()
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the apron in Z",
                p.kind_tag,
                p.bounds.center()
            );
        }
        assert!(
            checked > 8,
            "only {checked} ground parts examined — the selector has stopped \
             finding the ramp and the dunnage"
        );
    }
}
