//! Beached hulk - a Post-apocalyptic secondary that leads a shore holdout. A
//! rusted coaster run aground on the sand and made a fortified shanty: the
//! hull sits on its keel with its bow to the water (-Z), scrap huts and a
//! tarp stacked on its deck, the old bridge patched up as the hall, a
//! salvage crane swung out over the side, a ladder up from the beach and a
//! lookout nest on the mast with a red lamp at its head.
//!
//! The hull is two half-cylinders laid along Z, round side down: a straight
//! midbody, and a bow whose beam tapers to a point while its depth holds, so
//! it closes on a raked stem rather than a spoon. Everything aboard hangs off
//! the deck, so a ruined holdout leans the ship as one.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest,
    prim, prim_scaled, quat_mul, quat_x, quat_y, quat_z, solid, sphere, strut, superellipsoid,
    torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp2, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CORRUGATED_RUST, DISH_WHITE, FIRE_ORANGE, PLANK_GREY, RUST_BROWN, SIGNAL_RED, STEEL_GREY,
    TARP_FADED, TIRE_BLACK, WORKLIGHT, enamel, fx, plank, rusted, sheet, tarp, tyre_stack,
};

/// Spacing circle the settlement keeps round the hulk.
const CLEARANCE: f32 = 14.5;

/// Half the hull's beam and its depth from the deck line to the keel.
const BEAM: f32 = 4.5;
const DEPTH: f32 = 5.0;
/// Height of the deck line - the hull's cut plane - above grade. The keel
/// lies 0.6 m into the sand.
const DECK: f32 = 4.4;
/// The midbody runs from `MID_FWD` to the transom at `MID_AFT`; the bow from
/// `MID_FWD` forward to its stem at `BOW_TIP`.
const MID_FWD: f32 = -3.5;
const MID_AFT: f32 = 12.5;
const BOW_TIP: f32 = -12.5;
/// How much of its depth the bow keeps at the stem.
const STEM_DEPTH: f32 = 0.75;
/// Top of the deck planking amidships and of the raised forecastle.
const DECK_TOP: f32 = DECK + 0.05;
const FORE_TOP: f32 = DECK + 0.65;

/// Where the mast stands, and its height above the deck.
const MAST_Z: f32 = -2.0;
const MAST_H: f32 = 13.2;
/// Floor of the lookout nest.
const NEST_Y: f32 = DECK_TOP + 9.6;

pub struct BeachedHulk;

impl CatalogueEntry for BeachedHulk {
    fn slug(&self) -> &'static str {
        "beached_hulk"
    }
    fn name(&self) -> &'static str {
        "Beached Hulk"
    }
    fn description(&self) -> &'static str {
        "A rusted ship run aground and made a fortified shanty: huts on its deck, a crane and a lookout mast."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A wreck lies where the sea left it.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Weathered hull plate: rust over what was black paint.
fn hull_plate() -> SovereignMaterialSettings {
    rusted([0.30, 0.21, 0.16])
}

/// Dry beach sand heaped against the hull.
fn sand() -> SovereignMaterialSettings {
    tarp([0.62, 0.55, 0.41])
}

/// The rotation that lays a cylinder's axis along +Z.
fn along_z() -> crate::pds::Fp4 {
    quat_x(FRAC_PI_2)
}

/// Half-width of the midbody's hull at `below` metres under the deck line.
fn hull_half_width(below: f32) -> f32 {
    BEAM * (1.0 - (below / DEPTH).powi(2)).max(0.0).sqrt()
}

/// The two halves of the hull shell, round side down.
fn hull_shell() -> Vec<Generator> {
    let squash = [1.0, 1.0, DEPTH / BEAM];
    let mid_len = MID_AFT - MID_FWD;
    let bow_len = MID_FWD - BOW_TIP;
    let midbody = prim_scaled(
        solid(with_cut(
            cylinder_tapered(BEAM, mid_len, 24, 0.0, hull_plate()),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, DECK, (MID_FWD + MID_AFT) * 0.5],
        along_z(),
        squash,
    );
    // The bow: the same section, its beam drawn to a point at the stem while
    // its depth holds three quarters, turned to run forward to -Z.
    let mut bow_kind = with_cut(
        cylinder_tapered(BEAM, bow_len, 24, 0.0, hull_plate()),
        [0.0, 0.5],
        [0.0, 1.0],
        0.0,
    );
    if let Some(t) = bow_kind.torture_mut() {
        t.taper = Fp2([0.96, 1.0 - STEM_DEPTH]);
    }
    let bow = prim_scaled(
        solid(bow_kind),
        [0.0, DECK, (MID_FWD + BOW_TIP) * 0.5],
        quat_mul(quat_y(PI), along_z()),
        squash,
    );
    vec![midbody, bow]
}

/// A lean of `len` along a run from `from` toward `to` in plan: the yaw that
/// turns a box's local Z onto it.
fn yaw_to(from: [f32; 3], to: [f32; 3]) -> crate::pds::Fp4 {
    quat_y((to[0] - from[0]).atan2(to[2] - from[2]))
}

/// The plating along the hull: rubbing strakes, welded patches, a cut-in
/// door with its gangplank, the hawse pipe and the anchor run out on the
/// sand ahead of the bow.
fn hull_fittings() -> Vec<Generator> {
    let mut v = Vec::new();
    let mid_len = MID_AFT - MID_FWD;
    let mid_z = (MID_FWD + MID_AFT) * 0.5;
    for side in [-1.0_f32, 1.0] {
        // Rubbing strake a metre under the deck line.
        v.push(prim(
            cylinder_tapered(0.13, mid_len - 0.4, 8, 0.0, rusted(RUST_BROWN)),
            [side * (hull_half_width(0.8) + 0.04), DECK - 0.8, mid_z],
            along_z(),
        ));
        // Welded sheet patches over holes in the plate, leaning with it.
        for (k, &(below, z, w)) in [(1.9_f32, 1.0_f32, 2.6_f32), (3.2, 8.0, 1.8)]
            .iter()
            .enumerate()
        {
            let z = if side > 0.0 {
                z
            } else {
                z + 2.5 * k as f32 - 1.0
            };
            let x = hull_half_width(below);
            let lean = (BEAM * below / (DEPTH * DEPTH) / (x / BEAM).max(0.2)).atan();
            v.push(prim(
                cuboid_tapered([0.12, 1.4, w], 0.0, sheet(CORRUGATED_RUST)),
                [side * (x + 0.03), DECK - below, z],
                quat_z(-side * lean.min(0.9)),
            ));
        }
    }
    // A door burnt through the starboard (+X) side low down, boarded round,
    // and a gangplank up to it from the sand.
    let door_below = 2.9;
    let door_x = hull_half_width(door_below);
    let door_z = 5.5;
    v.push(prim(
        cuboid_tapered([0.5, 2.0, 1.3], 0.0, tarp([0.04, 0.035, 0.03])),
        [door_x - 0.12, DECK - door_below + 0.2, door_z],
        id_quat(),
    ));
    for dz in [-0.78_f32, 0.78] {
        v.push(prim(
            solid(cuboid_tapered([0.6, 2.4, 0.18], 0.0, plank(PLANK_GREY))),
            [door_x - 0.05, DECK - door_below + 0.3, door_z + dz],
            id_quat(),
        ));
    }
    let sill = [door_x + 0.1, DECK - door_below - 0.75, door_z];
    let foot = [door_x + 3.4, 0.05, door_z];
    let run = [foot[0] - sill[0], foot[1] - sill[1]];
    v.push(prim(
        solid(cuboid_tapered(
            [(run[0] * run[0] + run[1] * run[1]).sqrt(), 0.1, 1.1],
            0.0,
            plank(PLANK_GREY),
        )),
        [(sill[0] + foot[0]) * 0.5, (sill[1] + foot[1]) * 0.5, door_z],
        quat_z(run[1].atan2(run[0])),
    ));
    // The stem bar the bow closes on.
    let stem_foot = DECK - DEPTH * STEM_DEPTH;
    v.push(prim(
        solid(cuboid_tapered(
            [0.34, FORE_TOP + 0.9 - stem_foot, 0.34],
            0.0,
            rusted(RUST_BROWN),
        )),
        [0.0, (stem_foot + FORE_TOP + 0.9) * 0.5, BOW_TIP + 0.12],
        id_quat(),
    ));
    // Hawse pipe on the port bow, the chain run out ahead to an anchor
    // lying half in the sand.
    let hawse = [-2.0, DECK - 0.7, -8.6];
    v.push(prim(
        torus(0.1, 0.32, rusted(STEEL_GREY)),
        [hawse[0] - 0.08, hawse[1], hawse[2]],
        quat_z(FRAC_PI_2 - 0.4),
    ));
    let anchor = [-3.4, 0.2, -14.6];
    v.push(strut(
        [hawse[0] - 0.05, hawse[1] - 0.1, hawse[2]],
        anchor,
        0.07,
        6,
        rusted(STEEL_GREY),
    ));
    v.push(prim(
        solid(cuboid_tapered([0.28, 0.28, 1.9], 0.0, rusted(RUST_BROWN))),
        [anchor[0], 0.1, anchor[2] - 0.8],
        quat_y(0.3),
    ));
    v.push(prim(
        cuboid_tapered([1.6, 0.24, 0.3], 0.3, rusted(RUST_BROWN)),
        [anchor[0] - 0.5, 0.08, anchor[2] - 1.6],
        quat_y(0.3),
    ));
    v
}

/// The bulwarks round the deck: straight down the midbody, raised and
/// converging round the forecastle to the stem.
fn bulwarks() -> Vec<Generator> {
    let mut v = Vec::new();
    let mid_len = MID_AFT - MID_FWD;
    let mid_z = (MID_FWD + MID_AFT) * 0.5;
    let x = BEAM - 0.12;
    for side in [-1.0_f32, 1.0] {
        v.push(prim(
            solid(cuboid_tapered(
                [0.12, 1.25, mid_len - 0.3],
                0.0,
                rusted(RUST_BROWN),
            )),
            [side * x, DECK + 0.5, mid_z],
            id_quat(),
        ));
        let from = [side * (x - 0.2), FORE_TOP, MID_FWD];
        let to = [side * 0.12, FORE_TOP, BOW_TIP + 0.4];
        let len = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
        v.push(prim(
            solid(cuboid_tapered([0.12, 1.3, len], 0.0, rusted(RUST_BROWN))),
            [
                (from[0] + to[0]) * 0.5,
                FORE_TOP + 0.5,
                (from[2] + to[2]) * 0.5,
            ],
            yaw_to(from, to),
        ));
    }
    // Transom rail across the stern, a little lower than the bulwarks it
    // runs between.
    v.push(prim(
        solid(cuboid_tapered(
            [2.0 * x, 1.19, 0.12],
            0.0,
            rusted(RUST_BROWN),
        )),
        [0.0, DECK + 0.5, MID_AFT - 0.2],
        id_quat(),
    ));
    v
}

/// A scrap hut: a corrugated box with a plank door and a lit window on its
/// -Z face and a sheet roof pitched to the back, nested under its floor.
fn hut(center: [f32; 3], size: [f32; 3], door: bool) -> Generator {
    let [cx, base, cz] = center;
    let [w, h, d] = size;
    let front = cz - d * 0.5;
    let body = prim(
        solid(cuboid_tapered(size, 0.0, sheet(CORRUGATED_RUST))),
        [cx, base + h * 0.5, cz],
        id_quat(),
    );
    let mut parts = vec![
        // Roof sheet, pitched down to the back and oversailing the walls.
        prim(
            cuboid_tapered([w + 0.4, 0.08, d + 0.5], 0.0, sheet(STEEL_GREY)),
            [cx, base + h + 0.12, cz],
            quat_x(-0.12),
        ),
        // A window: the lit room behind a gap in the sheet.
        prim(
            cuboid_tapered(
                [0.9, 0.6, 0.06],
                0.0,
                lit_from_dusk(glow([1.0, 0.55, 0.22], 1.6)),
            ),
            [cx + w * 0.22, base + h * 0.62, front - 0.03],
            id_quat(),
        ),
        // A plank batten over the window head.
        prim(
            cuboid_tapered([1.2, 0.12, 0.1], 0.0, plank(PLANK_GREY)),
            [cx + w * 0.22, base + h * 0.62 + 0.38, front - 0.05],
            id_quat(),
        ),
    ];
    if door {
        parts.push(prim(
            cuboid_tapered([0.9, 1.9, 0.08], 0.0, plank(PLANK_GREY)),
            [cx - w * 0.25, base + 0.95, front - 0.04],
            id_quat(),
        ));
    }
    nest(body, parts)
}

/// The old bridge aft, chipped white over rust: its block, the wheelhouse on
/// it with a worklight, the stub of the funnel, and a lit room behind one of
/// the broken windows.
fn bridge() -> Generator {
    let (w, h, d) = (7.2_f32, 3.2_f32, 3.8_f32);
    let z = MID_AFT - 0.4 - d * 0.5;
    let front = z - d * 0.5;
    let block = prim(
        solid(cuboid_tapered([w, h, d], 0.0, enamel(DISH_WHITE))),
        [0.0, DECK_TOP + h * 0.5, z],
        id_quat(),
    );
    let top = DECK_TOP + h;
    let mut parts = Vec::new();
    // A row of window holes across its front, some boarded, one lit.
    for i in 0..5 {
        let x = -2.6 + i as f32 * 1.3;
        let mat = match i {
            1 => plank(PLANK_GREY),
            3 => lit_from_dusk(glow([1.0, 0.55, 0.22], 1.6)),
            _ => tarp([0.03, 0.03, 0.035]),
        };
        parts.push(prim(
            cuboid_tapered([0.85, 0.75, 0.06], 0.0, mat),
            [x, DECK_TOP + h * 0.62, front - 0.03],
            id_quat(),
        ));
    }
    // Wheelhouse, its windows a dark band round the front.
    let (ww, wh, wd) = (4.6_f32, 2.3_f32, 2.4_f32);
    let wz = z - 0.3;
    parts.push(nest(
        prim(
            solid(cuboid_tapered([ww, wh, wd], 0.0, enamel(DISH_WHITE))),
            [0.0, top + wh * 0.5, wz],
            id_quat(),
        ),
        vec![
            prim(
                cuboid_tapered([ww - 0.6, 0.7, 0.06], 0.0, tarp([0.03, 0.03, 0.035])),
                [0.0, top + wh * 0.66, wz - wd * 0.5 - 0.03],
                id_quat(),
            ),
            prim(
                cuboid_tapered([ww + 0.5, 0.12, wd + 0.6], 0.0, rusted(STEEL_GREY)),
                [0.0, top + wh + 0.06, wz],
                id_quat(),
            ),
            // A salvaged worklight on the roof, aimed down over the deck.
            prim(
                cylinder_tapered(0.05, 0.9, 6, 0.0, rusted(STEEL_GREY)),
                [1.4, top + wh + 0.55, wz - 0.6],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.5, 0.32, 0.36], 0.0, glow(WORKLIGHT, 3.0)),
                [1.4, top + wh + 1.1, wz - 0.75],
                quat_x(-0.45),
            ),
        ],
    ));
    // Funnel stub behind the wheelhouse, soot-black at its lip.
    let fz = z + 1.2;
    parts.push(nest(
        prim(
            solid(cylinder_tapered(0.75, 2.6, 14, 0.06, rusted(RUST_BROWN))),
            [0.0, top + 1.3, fz],
            id_quat(),
        ),
        vec![prim(
            tube(0.74, 0.6, 0.5, 14, tarp(TIRE_BLACK)),
            [0.0, top + 2.42, fz],
            id_quat(),
        )],
    ));
    nest(block, parts)
}

/// The mast forward of the huts: its pole, a yard, the rungs up its back,
/// stays to the bulwarks and the stem, and the lookout nest with a red lamp
/// above it.
fn mast() -> Generator {
    let pole = prim(
        solid(cylinder_tapered(0.22, MAST_H, 10, 0.3, rusted(STEEL_GREY))),
        [0.0, DECK_TOP + MAST_H * 0.5, MAST_Z],
        id_quat(),
    );
    let top = DECK_TOP + MAST_H;
    let mut parts = Vec::new();
    // Rungs welded to its aft (+Z) face, up to the nest.
    let mut y = DECK_TOP + 0.5;
    while y < NEST_Y - 0.2 {
        parts.push(prim(
            cuboid_tapered([0.5, 0.05, 0.2], 0.0, tarp(RUST_BROWN)),
            [0.0, y, MAST_Z + 0.22],
            id_quat(),
        ));
        y += 0.7;
    }
    // A yard across it, a rag of tarp hanging off one arm.
    let yard_y = NEST_Y - 2.4;
    parts.push(prim(
        cylinder_tapered(0.09, 4.6, 6, 0.0, rusted(STEEL_GREY)),
        [0.0, yard_y, MAST_Z],
        quat_z(FRAC_PI_2),
    ));
    parts.push(prim(
        cuboid_tapered([0.9, 1.6, 0.03], 0.3, tarp(TARP_FADED)),
        [-1.6, yard_y - 0.8, MAST_Z],
        id_quat(),
    ));
    // Stays down to the bulwarks and forward to the stem.
    for (to, r) in [
        ([BEAM - 0.15, DECK + 1.1, MAST_Z + 3.0], 0.03),
        ([-(BEAM - 0.15), DECK + 1.1, MAST_Z + 3.0], 0.03),
        ([0.0, FORE_TOP + 1.25, BOW_TIP + 0.15], 0.035),
    ] {
        parts.push(strut(
            [0.0, top - 0.6, MAST_Z],
            to,
            r,
            5,
            rusted(STEEL_GREY),
        ));
    }
    // The lookout nest: a floor, a scrap-steel drum round it and a tarp
    // shade on poles.
    let nest_floor = prim(
        solid(cylinder_tapered(1.1, 0.16, 16, 0.0, rusted(STEEL_GREY))),
        [0.0, NEST_Y, MAST_Z],
        id_quat(),
    );
    let mut nest_parts = vec![prim(
        tube(1.15, 1.05, 1.05, 16, sheet(CORRUGATED_RUST)),
        [0.0, NEST_Y + 0.6, MAST_Z],
        id_quat(),
    )];
    for k in 0..3 {
        let a = k as f32 * 2.094 + 0.5;
        nest_parts.push(prim(
            cylinder_tapered(0.04, 1.9, 5, 0.0, rusted(STEEL_GREY)),
            [a.cos() * 1.0, NEST_Y + 1.03, MAST_Z + a.sin() * 1.0],
            id_quat(),
        ));
    }
    nest_parts.push(prim(
        crate::catalogue::items::util::cone(1.5, 0.7, 12, tarp(TARP_FADED)),
        [0.0, NEST_Y + 2.25, MAST_Z],
        id_quat(),
    ));
    parts.push(nest(nest_floor, nest_parts));
    // The red lamp on the pole's head, above the shade.
    parts.push(prim(
        sphere(0.2, 6, glow(SIGNAL_RED, 4.0)),
        [0.0, top + 0.12, MAST_Z],
        id_quat(),
    ));
    nest(pole, parts)
}

/// The salvage crane on the starboard quarter: its post and cab, a jib
/// swung out over the side and a car door hanging on its hook.
fn crane() -> Generator {
    let base = [2.9, DECK_TOP, 4.2];
    let post_h = 3.2;
    let post = prim(
        solid(cylinder_tapered(0.32, post_h, 10, 0.1, rusted(RUST_BROWN))),
        [base[0], base[1] + post_h * 0.5, base[2]],
        id_quat(),
    );
    let head = [base[0], base[1] + post_h, base[2]];
    let tip = [8.0, DECK + 8.4, 2.4];
    let mut parts = vec![
        prim(
            solid(cuboid_tapered([1.4, 1.3, 1.6], 0.0, sheet(STEEL_GREY))),
            [base[0] + 0.3, base[1] + post_h - 0.4, base[2] + 0.5],
            id_quat(),
        ),
        strut(head, tip, 0.17, 8, rusted(SIGNAL_RED_DULL)),
        // The jib's back-stay down to the cab.
        strut(
            [tip[0] - 0.1, tip[1] - 0.05, tip[2]],
            [base[0] - 0.4, base[1] + post_h + 0.2, base[2] + 0.6],
            0.04,
            5,
            rusted(STEEL_GREY),
        ),
    ];
    // Hook cable and the load on it: a car door off the scrap heap.
    let load_y = 2.6;
    parts.push(prim(
        cylinder_tapered(0.025, tip[1] - load_y - 0.5, 4, 0.0, rusted(STEEL_GREY)),
        [tip[0], (tip[1] + load_y + 0.5) * 0.5, tip[2]],
        id_quat(),
    ));
    parts.push(prim(
        torus(0.05, 0.16, rusted(STEEL_GREY)),
        [tip[0], load_y + 0.55, tip[2]],
        quat_x(FRAC_PI_2),
    ));
    parts.push(prim(
        cuboid_tapered([1.3, 1.0, 0.1], 0.0, rusted([0.36, 0.40, 0.34])),
        [tip[0], load_y, tip[2]],
        quat_y(0.4),
    ));
    nest(post, parts)
}

/// Faded red lead on the crane jib.
const SIGNAL_RED_DULL: [f32; 3] = [0.50, 0.18, 0.12];

/// The barrel fire on the deck ahead of the huts, its drum, coals and flame.
fn barrel_fire(at: [f32; 2]) -> Generator {
    let drum = prim(
        solid(cylinder_tapered(0.38, 0.95, 12, 0.0, rusted(RUST_BROWN))),
        [at[0], DECK_TOP + 0.475, at[1]],
        id_quat(),
    );
    let mut fire = prim(
        cylinder_tapered(0.33, 0.4, 10, 0.0, glow(FIRE_ORANGE, 4.2)),
        [at[0], DECK_TOP + 1.05, at[1]],
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    nest(
        drum,
        vec![
            fire,
            fx::fire_flame([at[0], DECK_TOP + 1.3, at[1]], 0x0B1C_F112),
        ],
    )
}

/// Everything aboard, nested under the deck planking so the ship holds
/// together.
fn ship() -> Generator {
    let mid_len = MID_AFT - MID_FWD;
    let mid_z = (MID_FWD + MID_AFT) * 0.5;
    let deck = prim(
        solid(cuboid_tapered(
            [2.0 * (BEAM - 0.3), 1.0, mid_len - 0.2],
            0.0,
            plank([0.36, 0.33, 0.29]),
        )),
        [0.0, DECK_TOP - 0.5, mid_z],
        id_quat(),
    );
    let mut parts = hull_shell();
    // The raised forecastle deck, its beam drawn in with the bow's.
    let fore_len = MID_FWD - BOW_TIP - 0.6;
    parts.push(prim(
        solid(cuboid_tapered_xz(
            [2.0 * (BEAM - 0.3), fore_len, 1.6],
            [0.96, 0.0],
            rusted(RUST_BROWN),
        )),
        [0.0, FORE_TOP - 0.8, MID_FWD + 0.1 - fore_len * 0.5],
        quat_x(-FRAC_PI_2),
    ));
    parts.extend(hull_fittings());
    parts.extend(bulwarks());
    // A barricade of scrap sheet across the forecastle, a firing gap in it.
    for (x, h, yaw) in [(-1.7_f32, 2.5_f32, 0.08_f32), (1.6, 2.2, -0.1)] {
        parts.push(prim(
            solid(cuboid_tapered([1.9, h, 0.1], 0.0, sheet(CORRUGATED_RUST))),
            [x, FORE_TOP + h * 0.5, -6.2],
            quat_y(yaw),
        ));
    }
    parts.extend(tyre_stack([0.0, FORE_TOP + 0.18, -5.6], 0.36));

    // Huts stacked amidships, a tarp run from the upper one down to the
    // port bulwark.
    let lower = [-0.2, DECK_TOP, 2.8];
    let lower_size = [4.6, 2.6, 4.2];
    let upper_top = DECK_TOP + lower_size[1];
    let mut lower_hut = hut(lower, lower_size, true);
    attach(
        &mut lower_hut,
        hut([0.3, upper_top, 2.4], [3.2, 2.2, 3.0], false),
    );
    parts.push(lower_hut);
    parts.push(hut([2.5, DECK_TOP, -0.6], [2.6, 2.3, 2.4], true));
    let tarp_from = [-2.5, DECK_TOP + 2.0, 2.8];
    let tarp_to = [-(BEAM - 0.1), DECK + 1.15, 2.8];
    let tarp_run = [tarp_to[0] - tarp_from[0], tarp_to[1] - tarp_from[1]];
    parts.push(prim(
        cuboid_tapered(
            [
                (tarp_run[0].powi(2) + tarp_run[1].powi(2)).sqrt() + 0.3,
                0.05,
                3.6,
            ],
            0.0,
            tarp(TARP_FADED),
        ),
        [
            (tarp_from[0] + tarp_to[0]) * 0.5,
            (tarp_from[1] + tarp_to[1]) * 0.5 + 0.05,
            tarp_from[2],
        ],
        quat_z(tarp_run[1].atan2(tarp_run[0]) + PI),
    ));

    parts.push(bridge());
    parts.push(mast());
    parts.push(crane());
    parts.push(barrel_fire([1.0, -2.6]));

    // A ladder up the starboard side from the beach, its rails resting on
    // the bulwark.
    let (lz0, lz1) = (-1.4_f32, -0.8_f32);
    let foot_x = 6.3;
    let top = [BEAM - 0.08, DECK + 1.1];
    for z in [lz0, lz1] {
        parts.push(strut(
            [foot_x, 0.0, z],
            [top[0], top[1], z],
            0.05,
            5,
            plank(PLANK_GREY),
        ));
    }
    let rungs = 9;
    for i in 1..rungs {
        let f = i as f32 / rungs as f32;
        parts.push(prim(
            cuboid_tapered([0.06, 0.06, lz1 - lz0], 0.0, tarp(PLANK_GREY)),
            [
                foot_x + (top[0] - foot_x) * f,
                top[1] * f,
                (lz0 + lz1) * 0.5,
            ],
            id_quat(),
        ));
    }
    nest(deck, parts)
}

/// Sand drifted against the hull where it lies, and the beach junk round it.
fn beach() -> Vec<Generator> {
    let mut v = Vec::new();
    for side in [-1.0_f32, 1.0] {
        for &(z, len) in &[(-6.0_f32, 6.0_f32), (5.0, 9.0)] {
            v.push(prim(
                superellipsoid([1.9, 0.75, len], 1.0, 1.0, sand()),
                [side * 2.6, 0.0, z],
                quat_y(side * 0.04),
            ));
        }
    }
    v.push(prim(
        superellipsoid([3.4, 0.8, 1.6], 1.0, 1.0, sand()),
        [0.0, 0.0, MID_AFT + 0.6],
        id_quat(),
    ));
    v.extend(tyre_stack([-5.6, 0.18, 7.5], 0.36));
    v
}

fn build_tree() -> Generator {
    // The footing runs under the keel, inside the hull's bilge where the
    // shell meets the sand, and the drifted sand hides its ends.
    let mut root = footing(3.0, 16.6, [0.0, 3.8], CLEARANCE);
    root.audio = fx::desolate_wind();
    attach(&mut root, ship());
    for part in beach() {
        attach(&mut root, part);
    }
    attach(&mut root, fx::ash_drift([0.0, 1.0, -6.0], 0x0B1C_A5D1));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BeachedHulk.build(""), "beached_hulk");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = BeachedHulk.build("");
        assert_nothing_floats(&g, "beached_hulk");
        assert_no_z_fighting(&g, "beached_hulk");
    }

    /// The fire, the worklight and the mast lamp are what a ruined holdout
    /// puts out.
    #[test]
    fn keeps_its_fire_and_lamps() {
        assert!(has_emissive(&BeachedHulk.build("")));
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&BeachedHulk.build("")).unwrap().len();
        eprintln!("beached_hulk record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
