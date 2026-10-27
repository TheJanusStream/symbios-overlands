//! River landing - a Wild West leader for the shore. A riverboat landing:
//! a timber wharf on piles with a red freight shed standing on it, cotton
//! bales and crates waiting on the planking, and on the water side (-Z) a
//! stern-wheel paddle steamer hauled out on slipway timbers for repair,
//! propped with shores, her wheel hung over the ground on its outriggers.
//!
//! The steamer is built as she stands: hull on the ways, the main deck on
//! its guards, the boiler deck on stanchions, the texas cabin and pilot
//! house on that, two tall stacks forward and the wheel aft on its
//! outriggers - each deck nested on the one below, so a fought-over landing
//! loses the boat whole rather than leaving her stacks in the air.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::pirate::lantern;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, id_quat, nest, plane, prim, quat_mul,
    quat_x, quat_y, quat_z, railing, solid, strut, superellipsoid, torus, tube, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CANVAS_TAN, CLAP_RED, CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, canvas, fx, glass,
    iron, lap_siding, tin,
};

pub struct RiverLanding;

/// Spacing circle: the footing's half diagonal.
const CLEARANCE: f32 = 17.6;
/// The steamer: hull length, beam, depth, centre (x, z), and the top of
/// the ways her keel rests on.
const HULL_L: f32 = 20.0;
const BEAM: f32 = 5.6;
const HULL_D: f32 = 1.3;
const BOAT_X: f32 = -1.5;
const BOAT_Z: f32 = -7.0;
const WAYS_TOP: f32 = 0.45;
/// The wharf deck's top, and its z span.
const WHARF_TOP: f32 = 1.3;
const WHARF_Z: [f32; 2] = [-3.2, 10.2];
const WHARF_HALF_X: f32 = 13.5;

/// Weathered grey of old river timber.
const TIMBER_GREY: [f32; 3] = [0.46, 0.42, 0.36];
/// A cotton bale's sacking.
const COTTON: [f32; 3] = [0.86, 0.84, 0.78];

impl CatalogueEntry for RiverLanding {
    fn slug(&self) -> &'static str {
        "river_landing"
    }
    fn name(&self) -> &'static str {
        "River Landing"
    }
    fn description(&self) -> &'static str {
        "Timber wharf and freight shed with a stern-wheel steamer hauled out on the ways, cotton bales on the planking."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn timber() -> crate::pds::SovereignMaterialSettings {
    lap_siding(TIMBER_GREY)
}

/// A cotton bale: a pressed, bulging bale of sacking with two iron ties,
/// standing on `base`.
fn bale(base: [f32; 3], yaw: f32) -> Generator {
    let [x, y, z] = base;
    let h = 0.75;
    let mut b = prim(
        solid(superellipsoid(
            [0.75, h * 0.5, 0.45],
            0.45,
            0.45,
            canvas(COTTON),
        )),
        [x, y + h * 0.5, z],
        quat_y(yaw),
    );
    for dx in [-0.3_f32, 0.3] {
        b.children.push(prim(
            cuboid_tapered([0.05, h + 0.02, 0.93], 0.0, iron(IRON_DARK)),
            [dx, 0.0, 0.0],
            id_quat(),
        ));
    }
    b
}

/// A plank crate of `s` on a side standing on `base`.
fn crate_box(base: [f32; 3], s: f32, yaw: f32) -> Generator {
    prim(
        solid(cuboid_tapered([s, s, s], 0.0, lap_siding(WOOD_RAW))),
        [base[0], base[1] + s * 0.5, base[2]],
        quat_y(yaw),
    )
}

/// The steamer's stern wheel at `(x, y, z)`, its shaft along Z: two iron
/// rims, six spoke bars through each, twelve red buckets and the shaft.
fn stern_wheel(x: f32, y: f32, z: f32) -> Vec<Generator> {
    let r = 2.2;
    let half_w = 2.1;
    let red = lap_siding(CLAP_RED);
    let mut parts = vec![prim(
        solid(cylinder_tapered(
            0.16,
            half_w * 2.0 + 0.6,
            10,
            0.0,
            iron(IRON_DARK),
        )),
        [x, y, z],
        quat_x(FRAC_PI_2),
    )];
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            torus(0.07, r, iron(IRON_DARK)),
            [x, y, z + s * half_w],
            quat_x(FRAC_PI_2),
        ));
        for k in 0..3 {
            parts.push(prim(
                cuboid_tapered([r * 2.0, 0.12, 0.1], 0.0, red.clone()),
                [x, y, z + s * half_w],
                quat_z(k as f32 * PI / 3.0),
            ));
        }
    }
    for k in 0..12 {
        let a = k as f32 * PI / 6.0;
        let rb = r - 0.32;
        parts.push(prim(
            cuboid_tapered([0.1, 0.62, half_w * 2.0 + 0.16], 0.0, red.clone()),
            [x + a.cos() * rb, y + a.sin() * rb, z],
            quat_z(a - FRAC_PI_2),
        ));
    }
    parts
}

/// The steamer on her ways: hull, guards, the boiler deck on stanchions,
/// the texas cabin and pilot house, the stacks and the wheel.
fn steamer() -> Generator {
    let (x0, z0) = (BOAT_X, BOAT_Z);
    let keel = WAYS_TOP;
    let main = keel + HULL_D;
    let white = lap_siding(CLAP_WHITE);
    let bow_l = 2.4;
    // Hull: the box of her midbody and a raked spoon bow at -X.
    let body_l = HULL_L - bow_l;
    let hull_x = x0 + bow_l * 0.5;
    // The wedge turned over (its full face up and to the body) and swung to
    // run along X: the deck reaches out over a keel that stops short.
    let bow_h = HULL_D - 0.02;
    let mut hull_parts = vec![prim(
        solid(wedge([BEAM - 0.04, bow_h, bow_l], white.clone())),
        [
            x0 - HULL_L * 0.5 + bow_l * 0.5 + 0.02,
            keel + bow_h * 0.5,
            z0,
        ],
        quat_mul(quat_y(FRAC_PI_2), quat_x(PI)),
    )];
    // Main deck on its guards, overhanging the hull all round.
    let deck_t = 0.16;
    let main_deck = prim(
        solid(cuboid_tapered(
            [HULL_L + 0.4, deck_t, BEAM + 1.0],
            0.0,
            timber(),
        )),
        [x0, main + deck_t * 0.5, z0],
        id_quat(),
    );
    let boiler = main + deck_t + 2.4;
    let mut on_main = Vec::new();
    // Stanchions carrying the boiler deck, along both guards.
    for k in 0..7 {
        let x = x0 - HULL_L * 0.5 + 2.0 + k as f32 * (HULL_L - 3.2) / 6.0;
        for s in [-1.0_f32, 1.0] {
            on_main.push(prim(
                solid(cylinder_tapered(
                    0.08,
                    boiler - main - deck_t,
                    6,
                    0.0,
                    white.clone(),
                )),
                [
                    x,
                    (main + deck_t + boiler) * 0.5,
                    z0 + s * (BEAM * 0.5 + 0.35),
                ],
                id_quat(),
            ));
        }
    }
    // The main-deck cabin amidships, boarded, with its freight doors.
    on_main.push(prim(
        solid(cuboid_tapered([9.0, 2.4, BEAM - 1.4], 0.0, white.clone())),
        [x0 + 0.5, main + deck_t + 1.2, z0],
        id_quat(),
    ));
    for k in 0..3 {
        on_main.push(prim(
            cuboid_tapered([1.6, 1.9, 0.06], 0.0, lap_siding(CLAP_RED)),
            [
                x0 - 2.6 + k as f32 * 3.1,
                main + deck_t + 0.95,
                z0 - (BEAM - 1.4) * 0.5 - 0.03,
            ],
            id_quat(),
        ));
    }
    // Boiler deck, and what stands on it.
    let boiler_deck = prim(
        solid(cuboid_tapered(
            [HULL_L - 2.4, deck_t, BEAM + 1.0],
            0.0,
            timber(),
        )),
        [x0 + 0.6, boiler + deck_t * 0.5, z0],
        id_quat(),
    );
    let up = boiler + deck_t;
    let mut on_boiler = Vec::new();
    let rail = lap_siding(CLAP_WHITE);
    let edge = (BEAM + 1.0) * 0.5 - 0.1;
    let (rx0, rx1) = (
        x0 + 0.6 - (HULL_L - 2.4) * 0.5 + 0.2,
        x0 + 0.6 + (HULL_L - 2.4) * 0.5 - 0.2,
    );
    on_boiler.extend(railing(
        [rx0, up, z0 - edge],
        [rx1, up, z0 - edge],
        0.95,
        1.1,
        rail.clone(),
    ));
    on_boiler.extend(railing(
        [rx0, up, z0 + edge],
        [rx1, up, z0 + edge],
        0.95,
        1.1,
        rail,
    ));
    // The texas: a long cabin with its row of windows to the river side.
    let texas_l = 10.0;
    let texas_h = 2.0;
    let texas_x = x0 + 1.0;
    let texas_w = 3.4;
    on_boiler.push(prim(
        solid(cuboid_tapered(
            [texas_l, texas_h, texas_w],
            0.0,
            white.clone(),
        )),
        [texas_x, up + texas_h * 0.5, z0],
        id_quat(),
    ));
    for k in 0..6 {
        let x = texas_x - texas_l * 0.5 + 0.9 + k as f32 * (texas_l - 1.8) / 5.0;
        on_boiler.push(prim(
            plane([0.9, 1.0], glass(GLASS_WARM, 0.0)),
            [x, up + 1.15, z0 - texas_w * 0.5 - 0.02],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Texas roof, the pilot house on it, and its little hipped cap.
    let roof_y = up + texas_h;
    let mut on_roof = vec![];
    let ph_x = texas_x - 2.6;
    let ph = 1.7;
    on_roof.push(prim(
        solid(cuboid_tapered([2.4, ph, 2.4], 0.0, white.clone())),
        [ph_x, roof_y + 0.12 + ph * 0.5, z0],
        id_quat(),
    ));
    for (dz, yaw) in [(-1.0_f32, 0.0_f32), (1.0, PI)] {
        on_roof.push(prim(
            plane([1.9, 0.8], glass(GLASS_WARM, 0.0)),
            [ph_x, roof_y + 0.12 + 1.1, z0 + dz * 1.22],
            quat_mul(quat_y(yaw), quat_x(-FRAC_PI_2)),
        ));
    }
    on_roof.push(prim(
        solid(cuboid_tapered([2.9, 0.5, 2.9], 0.8, tin(TIN_GREY))),
        [ph_x, roof_y + 0.12 + ph + 0.25, z0],
        id_quat(),
    ));
    let texas_roof = nest(
        prim(
            solid(cuboid_tapered(
                [texas_l + 0.6, 0.12, texas_w + 0.6],
                0.0,
                tin(TIN_GREY),
            )),
            [texas_x, roof_y + 0.06, z0],
            id_quat(),
        ),
        on_roof,
    );
    on_boiler.push(texas_roof);
    // Two tall stacks forward, with their crowns and a spreader between.
    let stack_x = x0 - HULL_L * 0.5 + 5.2;
    let stack_h = 7.4;
    for s in [-1.0_f32, 1.0] {
        let sz = z0 + s * 1.2;
        on_boiler.push(nest(
            prim(
                solid(cylinder_tapered(0.38, stack_h, 14, 0.0, iron(IRON_DARK))),
                [stack_x, up + stack_h * 0.5, sz],
                id_quat(),
            ),
            vec![prim(
                solid(tube(0.62, 0.36, 0.55, 14, iron(IRON_DARK))),
                [stack_x, up + stack_h - 0.2, sz],
                id_quat(),
            )],
        ));
    }
    on_boiler.push(prim(
        cuboid_tapered([0.08, 0.08, 2.4 - 0.7], 0.0, iron(IRON_DARK)),
        [stack_x, up + stack_h - 1.6, z0],
        id_quat(),
    ));
    on_main.push(nest(boiler_deck, on_boiler));

    // The wheel aft on two outrigger timbers.
    let wheel_x = x0 + HULL_L * 0.5 + 2.3;
    // The shaft rests on the outrigger timbers, which lie on the deck.
    let rig_h = 0.32;
    let rig_y = main + deck_t + rig_h * 0.5;
    let wheel_y = rig_y + rig_h * 0.5 + 0.16;
    let mut aft = stern_wheel(wheel_x, wheel_y, z0);
    for s in [-1.0_f32, 1.0] {
        let oz = z0 + s * 2.45;
        aft.push(prim(
            solid(cuboid_tapered([4.2, rig_h, 0.24], 0.0, timber())),
            [x0 + HULL_L * 0.5 + 1.5, rig_y, oz],
            id_quat(),
        ));
    }
    on_main.extend(aft);
    hull_parts.push(nest(main_deck, on_main));
    // A dark rubbing strake along the sheer, proud of the hull sides.
    hull_parts.push(prim(
        cuboid_tapered([body_l + 0.04, 0.22, BEAM + 0.12], 0.0, iron(IRON_DARK)),
        [hull_x, main - 0.2, z0],
        id_quat(),
    ));
    nest(
        prim(
            solid(cuboid_tapered([body_l, HULL_D, BEAM], 0.0, white)),
            [hull_x, keel + HULL_D * 0.5, z0],
            id_quat(),
        ),
        hull_parts,
    )
}

/// The ways under the steamer: four timber cradles across her beam, and a
/// line of shores propping each side.
fn ways() -> Vec<Generator> {
    let mut out = Vec::new();
    for k in 0..4 {
        let x = BOAT_X - 7.5 + k as f32 * 5.0;
        out.push(prim(
            solid(cuboid_tapered([0.5, WAYS_TOP, BEAM + 2.2], 0.0, timber())),
            [x, WAYS_TOP * 0.5, BOAT_Z],
            id_quat(),
        ));
    }
    for k in 0..3 {
        let x = BOAT_X - 5.0 + k as f32 * 5.0;
        for s in [-1.0_f32, 1.0] {
            let foot = [x, 0.15, BOAT_Z + s * (BEAM * 0.5 + 1.0)];
            let head = [x, WAYS_TOP + 0.9, BOAT_Z + s * (BEAM * 0.5 - 0.04)];
            out.push(nest(
                prim(
                    solid(cuboid_tapered([0.36, 0.12, 0.36], 0.0, timber())),
                    [foot[0], 0.21, foot[2]],
                    id_quat(),
                ),
                vec![strut(foot, head, 0.1, 6, timber())],
            ));
        }
    }
    out
}

/// The freight shed on the wharf: boarded red walls, a wide doorway to the
/// river with crates inside, a sign board over it and a tin roof.
fn freight_shed() -> Generator {
    let (cx, cz) = (-5.0, 6.4);
    let (w, d, h) = (12.0, 6.6, 3.6);
    let base = WHARF_TOP;
    let front = cz - d * 0.5;
    let red = lap_siding(CLAP_RED);
    let wt = 0.2;
    let door_w = 4.0;
    let door_h = 3.0;
    let mut parts = Vec::new();
    // Back and end walls.
    parts.push(prim(
        solid(cuboid_tapered([w, h, wt], 0.0, red.clone())),
        [cx, base + h * 0.5, cz + d * 0.5 - wt * 0.5],
        id_quat(),
    ));
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([wt, h, d - 2.0 * wt], 0.0, red.clone())),
            [cx + s * (w * 0.5 - wt * 0.5), base + h * 0.5, cz],
            id_quat(),
        ));
        // Gable ends over the end walls.
        parts.push(prim(
            solid(crate::catalogue::items::util::cuboid_tapered_xz(
                [wt, 1.6, d],
                [0.0, 0.94],
                red.clone(),
            )),
            [cx + s * (w * 0.5 - wt * 0.5), base + h + 0.8, cz],
            id_quat(),
        ));
    }
    // Front wall either side of the doorway, and the header over it.
    let side_w = (w - door_w) * 0.5;
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([side_w, h, wt], 0.0, red.clone())),
            [
                cx + s * (door_w * 0.5 + side_w * 0.5),
                base + h * 0.5,
                front + wt * 0.5,
            ],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([door_w, h - door_h, wt], 0.0, red.clone())),
        [cx, base + door_h + (h - door_h) * 0.5, front + wt * 0.5],
        id_quat(),
    ));
    // Freight inside the door.
    parts.push(crate_box([cx - 1.0, base + 0.06, cz + 1.0], 1.1, 0.2));
    parts.push(bale([cx + 1.1, base + 0.06, cz + 0.6], 0.1));
    // The sign on the header over the door: a pale board on a dark backing.
    let sign_y = base + door_h + (h - door_h) * 0.5;
    parts.push(prim(
        cuboid_tapered([4.6, 0.5, 0.08], 0.0, canvas([0.20, 0.14, 0.10])),
        [cx, sign_y, front - 0.02],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([4.3, 0.36, 0.05], 0.0, canvas(CANVAS_TAN)),
        [cx, sign_y, front - 0.075],
        id_quat(),
    ));
    parts.push(gable_roof(
        [w + 0.8, 1.7, d + 0.8],
        [cx, base + h + 0.85, cz],
        tin(TIN_GREY),
    ));
    // The shed's dark floor, the root everything stands on.
    nest(
        prim(
            solid(cuboid_tapered(
                [w - 0.02, 0.06, d - 0.02],
                0.0,
                canvas([0.12, 0.10, 0.08]),
            )),
            [cx, base + 0.03, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The wharf: a plank deck on three rows of piles, a fender along its river
/// edge, the cargo waiting on it and two lanterns on posts.
fn wharf() -> Generator {
    let t = 0.22;
    let (z_lo, z_hi) = (WHARF_Z[0], WHARF_Z[1]);
    let deck = prim(
        solid(cuboid_tapered(
            [WHARF_HALF_X * 2.0, t, z_hi - z_lo],
            0.0,
            timber(),
        )),
        [0.0, WHARF_TOP - t * 0.5, (z_lo + z_hi) * 0.5],
        id_quat(),
    );
    let mut parts = Vec::new();
    let pile_h = WHARF_TOP - t;
    for z in [z_lo + 0.4, (z_lo + z_hi) * 0.5, z_hi - 0.4] {
        for k in 0..6 {
            let x = -WHARF_HALF_X + 0.5 + k as f32 * (WHARF_HALF_X * 2.0 - 1.0) / 5.0;
            parts.push(prim(
                solid(cylinder_tapered(0.18, pile_h + 0.02, 8, 0.0, timber())),
                [x, pile_h * 0.5 - 0.01, z],
                id_quat(),
            ));
        }
    }
    parts.push(prim(
        solid(cuboid_tapered(
            [WHARF_HALF_X * 2.0 + 0.2, 0.4, 0.25],
            0.0,
            timber(),
        )),
        [0.0, WHARF_TOP - 0.25, z_lo - 0.12],
        id_quat(),
    ));
    // Cotton bales and crates along the river edge.
    for (x, z, yaw) in [
        (2.0_f32, -1.6_f32, 0.0_f32),
        (3.7, -1.6, 0.08),
        (5.4, -1.5, -0.05),
        (2.8, -0.3, 0.1),
        (4.6, -0.3, -0.1),
        (9.5, 1.2, 1.5),
    ] {
        parts.push(bale([x, WHARF_TOP, z], yaw));
    }
    parts.push(bale([3.7, WHARF_TOP + 0.75, -1.0], 0.05));
    for (x, z, s, yaw) in [
        (7.8_f32, -1.8_f32, 1.0_f32, 0.2_f32),
        (8.9, -1.7, 0.8, -0.3),
        (7.9, -0.6, 0.8, 0.5),
        (-10.5, -1.6, 1.0, 0.1),
    ] {
        parts.push(crate_box([x, WHARF_TOP, z], s, yaw));
    }
    // Lanterns on posts at the head of the landing.
    for (k, x) in [-0.6_f32, 11.5].into_iter().enumerate() {
        let post_h = 2.6;
        parts.push(nest(
            prim(
                solid(cylinder_tapered(0.09, post_h, 8, 0.0, timber())),
                [x, WHARF_TOP + post_h * 0.5, z_lo + 0.4],
                id_quat(),
            ),
            vec![
                prim(
                    cuboid_tapered([0.06, 0.06, 0.6], 0.0, iron(IRON_DARK)),
                    [x, WHARF_TOP + post_h - 0.1, z_lo + 0.15],
                    id_quat(),
                ),
                prim(
                    cylinder_tapered(0.015, 0.2, 5, 0.0, iron(IRON_DARK)),
                    [x, WHARF_TOP + post_h - 0.22, z_lo - 0.1],
                    id_quat(),
                ),
                lantern(
                    [x, WHARF_TOP + post_h - 0.32 - 0.25 - 0.015, z_lo - 0.1],
                    0.5,
                    0x7A + k as u32,
                ),
            ],
        ));
    }
    // A gangplank from the wharf up to the steamer's guard.
    parts.push(strut(
        [-6.0, WHARF_TOP + 0.04, z_lo + 0.6],
        [
            -6.0,
            WAYS_TOP + HULL_D + 0.16,
            BOAT_Z + (BEAM + 1.0) * 0.5 - 0.3,
        ],
        0.12,
        4,
        timber(),
    ));
    nest(deck, parts)
}

fn build_tree() -> Generator {
    let mut root = footing(28.4, 21.8, [0.0, -0.3], CLEARANCE);
    for w in ways() {
        attach(&mut root, w);
    }
    attach(&mut root, steamer());
    attach(&mut root, wharf());
    attach(&mut root, freight_shed());
    attach(&mut root, bale([-9.0, 0.15, -10.7], 0.3));
    attach(&mut root, fx::dust_drift([0.0, 0.5, 3.0], 0x21_7E12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RiverLanding.build(""), "river_landing");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&RiverLanding.build(""), "river_landing");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&RiverLanding.build(""), "river_landing");
    }

    #[test]
    fn its_lanterns_are_lit() {
        assert!(has_emissive(&RiverLanding.build("")));
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&RiverLanding.build(""))
            .expect("serialises");
        eprintln!("river_landing record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
