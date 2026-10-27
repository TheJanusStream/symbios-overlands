//! Container crane quay - an Industrial-Park secondary that leads a port
//! works. A red portal gantry crane stands on rails over a concrete quay
//! edge on -Z, its boom run out over the water with a container on the
//! spreader; a stack of containers waits behind, and a two-storey brick
//! dock office keeps the quay's books. Bollards and fenders line the edge.
//!
//! Primitive-built in one flat ground-relative frame. The quay deck, the
//! crane, the container stack and the office each hang off the footing as
//! one nested sub-assembly, so a ruined quay knocks each as a whole.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, quat_x, solid,
    sphere, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_DARK, CONCRETE_GREY, CONTAINER_BLUE, CONTAINER_GREEN, CONTAINER_RED, CONTAINER_RUST,
    FLOOD_WHITE, LAMP_AMBER, PIPE_GREY, TANK_WHITE, brick, cladding, concrete,
};

/// Top of the quay deck.
const DECK_TOP: f32 = 1.0;
/// Half the deck's width (X); its front edge (the water) and back (Z).
const DECK_HX: f32 = 14.0;
const DECK_FRONT: f32 = -11.0;
const DECK_BACK: f32 = 10.6;
/// The crane's legs stand at +-`LEG_X` on the sea and land rails.
const LEG_X: f32 = 4.5;
const SEA_RAIL: f32 = -8.0;
const LAND_RAIL: f32 = 1.0;
/// Top of the rails, which the crane's bogies ride.
const RAIL_TOP: f32 = DECK_TOP + 0.14;
/// Underside of the portal girders the legs carry, and of the boom.
const PORTAL_Y: f32 = 13.8;
const BOOM_Y: f32 = 15.0;
/// How far the boom runs out over the water, and back over the land.
const BOOM_TIP: f32 = -18.0;
const BOOM_TAIL: f32 = 7.0;
/// Half the gap between the boom's two box girders, centre to centre.
const BOOM_HX: f32 = 1.3;
/// The A-frame's apex above the portal.
const APEX: [f32; 3] = [0.0, 23.0, -3.5];
/// ISO containers: a forty-foot and a twenty-foot box's length, and the
/// height and width they share.
const BOX_L: f32 = 12.2;
const BOX_SHORT: f32 = 6.06;
const BOX_H: f32 = 2.6;
const BOX_W: f32 = 2.44;
/// Spacing circle the settlement keeps round the quay.
const CLEARANCE: f32 = 15.0;

/// The crane's paint: a port's signal red-orange.
const CRANE_RED: [f32; 3] = [0.66, 0.20, 0.12];
/// Black rubber of the fenders and the cast iron of the bollards.
const RUBBER: [f32; 3] = [0.07, 0.07, 0.08];
const IRON: [f32; 3] = [0.16, 0.16, 0.17];
/// Aviation warning red at the crane's apex.
const WARNING_RED: [f32; 3] = [1.0, 0.12, 0.08];
/// Dark cab glass.
const CAB_GLASS: [f32; 3] = [0.08, 0.10, 0.12];
/// The office's lamplight behind its windows.
const OFFICE_WARM: [f32; 3] = [1.0, 0.74, 0.42];
/// Painted name board over the office door.
const BOARD_GREEN: [f32; 3] = [0.14, 0.30, 0.22];

pub struct ContainerCraneQuay;

impl CatalogueEntry for ContainerCraneQuay {
    fn slug(&self) -> &'static str {
        "container_crane_quay"
    }
    fn name(&self) -> &'static str {
        "Container Crane Quay"
    }
    fn description(&self) -> &'static str {
        "Portal gantry crane on rails over a concrete quay, with containers and a brick dock office."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A quay is built at the water, its boom over it.
    fn site(&self) -> SitePreference {
        SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Plain painted steel, untextured: the crane and the fittings are many
/// parts, and a weathered metal texture on each costs the record far more
/// than it shows at the distance they are seen from.
fn plain(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.5),
        ..Default::default()
    }
}

/// Dark glass with no light of its own.
fn cab_glass() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(CAB_GLASS),
        roughness: Fp(0.1),
        metallic: Fp(0.5),
        ..Default::default()
    }
}

/// The quay deck: the concrete slab with its capping beam, fenders,
/// bollards and the crane rails.
fn deck() -> Generator {
    let depth = DECK_BACK - DECK_FRONT;
    let mid_z = (DECK_BACK + DECK_FRONT) * 0.5;
    let mut parts = vec![
        // Capping beam along the water's edge, proud of the deck on every
        // side it meets.
        prim(
            solid(cuboid_tapered(
                [DECK_HX * 2.0 + 0.4, 0.3, 0.6],
                0.0,
                concrete([0.66, 0.65, 0.62]),
            )),
            [0.0, DECK_TOP, DECK_FRONT + 0.1],
            id_quat(),
        ),
    ];
    // Rubber fenders hung on the quay face.
    for i in 0..7 {
        let x = -12.0 + i as f32 * 4.0;
        parts.push(prim(
            solid(cuboid_tapered([1.0, 0.8, 0.4], 0.0, plain(RUBBER))),
            [x, 0.47, DECK_FRONT - 0.1],
            id_quat(),
        ));
    }
    // Mushroom bollards a pace back from the edge.
    for i in 0..6 {
        let x = -12.5 + i as f32 * 5.0;
        parts.push(nest(
            prim(
                solid(cylinder_tapered(0.2, 0.55, 10, 0.1, plain(IRON))),
                [x, DECK_TOP + 0.27, DECK_FRONT + 1.0],
                id_quat(),
            ),
            vec![prim(
                solid(cylinder_tapered(0.3, 0.12, 10, 0.2, plain(IRON))),
                [x, DECK_TOP + 0.58, DECK_FRONT + 1.0],
                id_quat(),
            )],
        ));
    }
    // The crane rails, bedded a little into the deck.
    for z in [SEA_RAIL, LAND_RAIL] {
        parts.push(prim(
            solid(cuboid_tapered(
                [DECK_HX * 2.0 - 0.4, 0.16, 0.14],
                0.0,
                plain(PIPE_GREY),
            )),
            [0.0, RAIL_TOP - 0.08, z],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [DECK_HX * 2.0, DECK_TOP, depth],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, DECK_TOP * 0.5, mid_z],
            id_quat(),
        ),
        parts,
    )
}

/// The portal gantry crane, rooted on its sea-side west bogie.
fn crane() -> Generator {
    let paint = || plain(CRANE_RED);
    let bogie_y = RAIL_TOP + 0.33;
    let mut parts = Vec::new();
    // Bogies on the rails, each with its pair of wheel discs, and the four
    // legs they carry.
    let mut bogies = Vec::new();
    for z in [SEA_RAIL, LAND_RAIL] {
        for x in [-LEG_X, LEG_X] {
            let mut wheels = Vec::new();
            for dx in [-0.8_f32, 0.8] {
                for dz in [-0.42_f32, 0.42] {
                    wheels.push(prim(
                        cylinder_tapered(0.32, 0.06, 14, 0.0, plain(IRON)),
                        [x + dx, bogie_y - 0.1, z + dz],
                        quat_x(FRAC_PI_2),
                    ));
                }
            }
            let leg_h = PORTAL_Y + 0.2 - (bogie_y + 0.3);
            wheels.push(prim(
                solid(cuboid_tapered([0.9, leg_h, 0.9], 0.0, paint())),
                [x, bogie_y + 0.3 + leg_h * 0.5, z],
                id_quat(),
            ));
            bogies.push(nest(
                prim(
                    solid(cuboid_tapered([2.6, 0.7, 0.8], 0.0, plain(IRON))),
                    [x, bogie_y, z],
                    id_quat(),
                ),
                wheels,
            ));
        }
    }
    let root = bogies.remove(0);
    parts.extend(bogies);
    let gauge = LAND_RAIL - SEA_RAIL;
    let mid_z = (LAND_RAIL + SEA_RAIL) * 0.5;
    for x in [-LEG_X, LEG_X] {
        // Portal girder along each side, over the legs.
        parts.push(prim(
            solid(cuboid_tapered([1.0, 1.2, gauge + 1.4], 0.0, paint())),
            [x, PORTAL_Y + 0.6, mid_z],
            id_quat(),
        ));
        // A sill tie half way up, and the side portal's diagonal.
        parts.push(prim(
            solid(cuboid_tapered([0.6, 0.6, gauge - 0.4], 0.0, paint())),
            [x, 7.0, mid_z],
            id_quat(),
        ));
        parts.push(strut(
            [x, 7.3, SEA_RAIL + 0.3],
            [x, PORTAL_Y - 0.1, LAND_RAIL - 0.3],
            0.18,
            10,
            paint(),
        ));
    }
    // Cross ties over the rails, which the boom rests on.
    for z in [SEA_RAIL, LAND_RAIL] {
        parts.push(prim(
            solid(cuboid_tapered([LEG_X * 2.0 - 0.6, 1.0, 0.8], 0.0, paint())),
            [0.0, BOOM_Y - 0.45, z],
            id_quat(),
        ));
    }
    // The boom: two box girders braced across, a head block at the tip.
    let boom_len = BOOM_TAIL - BOOM_TIP;
    let boom_cy = BOOM_Y + 0.8;
    for x in [-BOOM_HX, BOOM_HX] {
        parts.push(prim(
            solid(cuboid_tapered([0.7, 1.6, boom_len], 0.0, paint())),
            [x, boom_cy, (BOOM_TAIL + BOOM_TIP) * 0.5],
            id_quat(),
        ));
    }
    let mut z = BOOM_TIP + 1.5;
    while z < BOOM_TAIL - 1.0 {
        parts.push(prim(
            cuboid_tapered([BOOM_HX * 2.0 - 0.6, 0.3, 0.3], 0.0, paint()),
            [0.0, BOOM_Y + 1.4, z],
            id_quat(),
        ));
        z += 3.5;
    }
    parts.push(prim(
        solid(cuboid_tapered([3.4, 1.8, 0.8], 0.0, paint())),
        [0.0, boom_cy, BOOM_TIP + 0.2],
        id_quat(),
    ));
    // The A-frame over the portal, its apex beam and warning light, and the
    // stays that hold the boom out.
    let [_, ay, az] = APEX;
    parts.push(prim(
        solid(cuboid_tapered(
            [BOOM_HX * 2.0 + 1.0, 0.8, 0.8],
            0.0,
            paint(),
        )),
        APEX,
        id_quat(),
    ));
    for s in [-1.0_f32, 1.0] {
        for z in [SEA_RAIL, LAND_RAIL] {
            parts.push(strut(
                [s * LEG_X, PORTAL_Y + 1.1, z],
                [s * (BOOM_HX + 0.2), ay, az],
                0.28,
                10,
                paint(),
            ));
        }
        for end in [BOOM_TIP + 1.0, BOOM_TAIL - 0.6] {
            parts.push(strut(
                [s * BOOM_HX, ay, az],
                [s * BOOM_HX, BOOM_Y + 1.55, end],
                0.11,
                8,
                plain(PIPE_GREY),
            ));
        }
    }
    parts.push(prim(
        sphere(0.22, 3, glow(WARNING_RED, 5.0)),
        [0.0, ay + 0.55, az],
        id_quat(),
    ));
    // Machinery house on the boom's tail, narrow enough for the backstays
    // to pass it.
    parts.push(prim(
        solid(cuboid_tapered([2.2, 3.0, 5.0], 0.0, cladding(TANK_WHITE))),
        [0.0, BOOM_Y + 1.6 + 1.45, BOOM_TAIL - 3.0],
        id_quat(),
    ));
    // Floodlights under the boom, lit at every hour as lamps are.
    for (x, z) in [
        (-BOOM_HX, -15.0),
        (BOOM_HX, -15.0),
        (-BOOM_HX, -5.0),
        (BOOM_HX, -5.0),
    ] {
        parts.push(prim(
            cuboid_tapered([0.5, 0.3, 0.5], 0.0, glow(FLOOD_WHITE, 4.0)),
            [x, BOOM_Y - 0.13, z],
            id_quat(),
        ));
    }
    // The trolley between the girders, its cab slung under it, and the
    // hoist ropes down to a spreader holding a container over the water.
    let trolley_z = -11.5;
    parts.push(prim(
        solid(cuboid_tapered(
            [BOOM_HX * 2.0 - 0.6, 1.0, 3.0],
            0.0,
            cladding(TANK_WHITE),
        )),
        [0.0, BOOM_Y - 0.4, trolley_z],
        id_quat(),
    ));
    parts.push(nest(
        prim(
            solid(cuboid_tapered([1.8, 1.9, 1.6], 0.0, cladding(TANK_WHITE))),
            [0.0, BOOM_Y - 1.8, trolley_z + 1.0],
            id_quat(),
        ),
        vec![prim(
            cuboid_tapered([1.5, 1.0, 0.06], 0.0, cab_glass()),
            [0.0, BOOM_Y - 1.6, trolley_z + 0.19],
            id_quat(),
        )],
    ));
    let spreader_y = 9.5;
    let rope_top = BOOM_Y - 0.85;
    let rope_len = rope_top - (spreader_y + 0.15);
    for x in [-0.6_f32, 0.6] {
        for dz in [-1.0_f32, -0.3] {
            parts.push(prim(
                cylinder_tapered(0.04, rope_len + 0.1, 6, 0.0, plain(IRON)),
                [x, spreader_y + 0.15 + rope_len * 0.5, trolley_z + dz],
                id_quat(),
            ));
        }
    }
    let load_z = trolley_z - 0.65;
    parts.push(prim(
        solid(cuboid_tapered(
            [BOX_L - 0.6, 0.4, BOX_W - 0.2],
            0.0,
            plain(IRON),
        )),
        [0.0, spreader_y, load_z],
        id_quat(),
    ));
    parts.push(container(
        [0.0, spreader_y - 0.2 - BOX_H * 0.5 + 0.05, load_z],
        BOX_L,
        CONTAINER_BLUE,
    ));
    nest(root, parts)
}

/// A container `len` long lying along X, centred on `at`: a corrugated
/// box, its four corner posts standing a hair proud of its sides (and a
/// hair short of its top and bottom, so a stacked box's posts never meet
/// them face to face), and its door end on -X with the locking rods.
fn container(at: [f32; 3], len: f32, color: [f32; 3]) -> Generator {
    let [x, y, z] = at;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered([0.24, BOX_H - 0.04, 0.24], 0.0, plain(IRON))),
                [x + sx * (len * 0.5 - 0.1), y, z + sz * (BOX_W * 0.5 - 0.1)],
                id_quat(),
            ));
        }
    }
    let door = crate::catalogue::items::util::tint(color, [0.8, 0.8, 0.8]);
    parts.push(prim(
        cuboid_tapered([0.05, BOX_H - 0.4, BOX_W - 0.5], 0.0, plain(door)),
        [x - len * 0.5 - 0.02, y, z],
        id_quat(),
    ));
    for dz in [-0.5_f32, 0.5] {
        parts.push(prim(
            cylinder_tapered(0.04, BOX_H - 0.5, 6, 0.0, plain(PIPE_GREY)),
            [x - len * 0.5 - 0.08, y, z + dz],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered([len, BOX_H, BOX_W], 0.0, cladding(color))),
            at,
            id_quat(),
        ),
        parts,
    )
}

/// The container stack waiting on the quay behind the crane, rooted on its
/// first box.
fn stack() -> Generator {
    let x0 = LEG_X + 1.6;
    let z0 = LAND_RAIL + 1.6;
    // Rows of twenty-foot boxes across the quay (Z), each lying along it
    // (X), three high at most.
    let rows: [&[[f32; 3]]; 3] = [
        &[CONTAINER_RED, CONTAINER_GREEN, CONTAINER_RUST],
        &[CONTAINER_BLUE, TANK_WHITE],
        &[CONTAINER_GREEN, CONTAINER_RED, CONTAINER_BLUE],
    ];
    let mut boxes = Vec::new();
    for (r, colors) in rows.iter().enumerate() {
        let z = z0 + BOX_W * 0.5 + r as f32 * (BOX_W + 0.16);
        for (k, color) in colors.iter().enumerate() {
            let y = DECK_TOP + BOX_H * 0.5 + k as f32 * BOX_H;
            boxes.push(container([x0 + BOX_SHORT * 0.5, y, z], BOX_SHORT, *color));
        }
    }
    let root = boxes.remove(0);
    nest(root, boxes)
}

/// The dock office: a two-storey brick block with lit windows from dusk,
/// a name board and a lamp over its door, a flat roof with a parapet cap.
fn office() -> Generator {
    let (cx, cz) = (-10.5_f32, 5.5_f32);
    let (w, d, h) = (6.0_f32, 6.0_f32, 6.6_f32);
    let front = cz - d * 0.5;
    let side = cx - w * 0.5;
    let pane = || crate::catalogue::items::street::lit_from_dusk(glow(OFFICE_WARM, 1.3));
    let mut parts = vec![
        // Parapet cap.
        prim(
            solid(cuboid_tapered(
                [w + 0.3, 0.3, d + 0.3],
                0.0,
                concrete([0.6, 0.59, 0.56]),
            )),
            [cx, DECK_TOP + h + 0.1, cz],
            id_quat(),
        ),
        // Roof plant box.
        prim(
            solid(cuboid_tapered([1.6, 0.9, 1.2], 0.0, cladding(PIPE_GREY))),
            [cx + 1.2, DECK_TOP + h + 0.65, cz + 1.0],
            id_quat(),
        ),
        // Door.
        prim(
            solid(cuboid_tapered([1.1, 2.1, 0.1], 0.0, plain(BOARD_GREEN))),
            [cx, DECK_TOP + 1.07, front - 0.03],
            id_quat(),
        ),
        // Name board over it, and its lamp on a bracket.
        prim(
            cuboid_tapered([3.2, 0.6, 0.1], 0.0, plain(BOARD_GREEN)),
            [cx, DECK_TOP + 3.0, front - 0.04],
            id_quat(),
        ),
        nest(
            prim(
                cuboid_tapered([0.08, 0.08, 0.5], 0.0, plain(IRON)),
                [cx, DECK_TOP + 2.5, front - 0.25],
                id_quat(),
            ),
            vec![prim(
                cuboid_tapered([0.3, 0.2, 0.3], 0.0, glow(LAMP_AMBER, 3.0)),
                [cx, DECK_TOP + 2.38, front - 0.45],
                id_quat(),
            )],
        ),
    ];
    // Front windows: two either side of the door below, three above.
    let mut front_windows = vec![(cx - 1.9, DECK_TOP + 1.6), (cx + 1.9, DECK_TOP + 1.6)];
    for dx in [-1.9_f32, 0.0, 1.9] {
        front_windows.push((cx + dx, DECK_TOP + 4.7));
    }
    for (x, y) in front_windows {
        parts.push(prim(
            cuboid_tapered([1.1, 1.3, 0.08], 0.0, pane()),
            [x, y, front - 0.03],
            id_quat(),
        ));
        parts.push(prim(
            solid(cuboid_tapered(
                [1.3, 0.12, 0.22],
                0.0,
                concrete([0.6, 0.59, 0.56]),
            )),
            [x, y - 0.71, front - 0.1],
            id_quat(),
        ));
    }
    // Side windows looking down the quay.
    for y in [DECK_TOP + 1.6, DECK_TOP + 4.7] {
        for dz in [-1.4_f32, 1.4] {
            parts.push(prim(
                cuboid_tapered([0.08, 1.3, 1.1], 0.0, pane()),
                [side - 0.03, y, cz + dz],
                id_quat(),
            ));
        }
    }
    nest(
        prim(
            solid(cuboid_tapered([w, h, d], 0.0, brick(BRICK_DARK))),
            [cx, DECK_TOP + h * 0.5 - 0.02, cz],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let mut root = footing(
        DECK_HX * 2.0,
        DECK_BACK - DECK_FRONT,
        [0.0, (DECK_BACK + DECK_FRONT) * 0.5],
        CLEARANCE,
    );
    for part in [deck(), crane(), stack(), office()] {
        attach(&mut root, part);
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ContainerCraneQuay.build(""), "container_crane_quay");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = ContainerCraneQuay.build("");
        assert_no_z_fighting(&built, "container_crane_quay");
        assert_nothing_floats(&built, "container_crane_quay");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&ContainerCraneQuay.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(
            bytes < 60_000,
            "container_crane_quay serialises to {bytes} B"
        );
    }
}
