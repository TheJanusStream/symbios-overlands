//! Rowing boathouse - a Sports/Recreation secondary that leads a club town
//! on its water. A long stained-timber boathouse opens three wide bays on
//! -Z to its slipway, racked shells showing inside, one bay shut behind
//! braced doors; above them the clubroom's covered balcony stand looks
//! out over the water under the deep front slope of its roof. Shells lie
//! racked outside on the east end, and the club's pennant flies over the
//! slipway.
//!
//! Primitive-built in one flat ground-relative frame. The boathouse, the
//! slipway with its flagpole, and the outside rack each hang off the
//! footing as one nested sub-assembly.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::modern_city::timber;
use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim,
    prim_scaled, quat_y, quat_z, railing, solid, superellipsoid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, LINE_WHITE, SEAT_BLUE, SEAT_RED, STEEL_GREY, concrete, corrugated, painted,
    steel,
};

/// Half the boathouse's length (X) and depth (Z).
const HX: f32 = 10.0;
const HZ: f32 = 4.5;
/// The boat hall's lintel underside, the balcony deck's top, and the eaves.
const BAY_TOP: f32 = 3.6;
const FLOOR: f32 = 4.2;
const DECK_TOP: f32 = 4.45;
const EAVES: f32 = 7.4;
/// The balcony's front edge, where its posts stand.
const BALCONY_FRONT: f32 = -7.0;
/// The slipway's front edge, toward the water.
const SLIP_FRONT: f32 = -9.5;
/// Where the outside shell rack stands, off the east end.
const RACK_X: f32 = 11.4;
/// Spacing circle the settlement keeps round the boathouse.
const CLEARANCE: f32 = 13.0;

/// Stained timber of the walls, and the cream of the trim.
const STAIN: [f32; 3] = [0.40, 0.21, 0.15];
const CREAM: [f32; 3] = [0.88, 0.85, 0.76];
/// The boat-hall doors' club green.
const DOOR_GREEN: [f32; 3] = [0.14, 0.32, 0.22];
/// The roof's grey-green sheet.
const ROOF_GREEN: [f32; 3] = [0.30, 0.40, 0.34];
/// Varnished hulls and oar blades: cedar, white, the club's yellow.
const HULL_CEDAR: [f32; 3] = [0.62, 0.38, 0.18];
const HULL_YELLOW: [f32; 3] = [0.86, 0.70, 0.18];
/// Lamplight in the clubroom, from dusk to dawn, and the bay lamps.
const ROOM_WARM: [f32; 3] = [1.0, 0.78, 0.48];
const LAMP_WARM: [f32; 3] = [1.0, 0.84, 0.56];

pub struct RowingBoathouse;

impl CatalogueEntry for RowingBoathouse {
    fn slug(&self) -> &'static str {
        "rowing_boathouse"
    }
    fn name(&self) -> &'static str {
        "Rowing Boathouse"
    }
    fn description(&self) -> &'static str {
        "Long timber boathouse with wide bays to the water, a covered balcony stand, racked shells."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// Its bays open on the water.
    fn site(&self) -> SitePreference {
        SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
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

/// A rowing shell lying along Z on its rack or bracket, centred on `at`.
fn shell(at: [f32; 3], len: f32, color: [f32; 3]) -> Generator {
    prim(
        superellipsoid([0.22, 0.14, len * 0.5], 1.0, 1.0, painted(color)),
        at,
        id_quat(),
    )
}

/// The boat hall's front: four piers, the lintel over the bays, the shut
/// middle bay's braced doors, and a lamp over each bay under the balcony.
fn hall_front() -> Vec<Generator> {
    let wall = || timber(STAIN);
    let pier_w = 1.2;
    let bay_w = (HX * 2.0 - 4.0 * pier_w) / 3.0;
    let front = -HZ + 0.15;
    let mut out = Vec::new();
    for i in 0..4 {
        let x = -HX + pier_w * 0.5 + i as f32 * (pier_w + bay_w);
        out.push(prim(
            solid(cuboid_tapered([pier_w, BAY_TOP, 0.3], 0.0, wall())),
            [x, BAY_TOP * 0.5, front],
            id_quat(),
        ));
    }
    out.push(prim(
        solid(cuboid_tapered(
            [HX * 2.0, FLOOR - BAY_TOP, 0.3],
            0.0,
            wall(),
        )),
        [0.0, (BAY_TOP + FLOOR) * 0.5, front],
        id_quat(),
    ));
    // The middle bay shut: a pair of green doors, a cream brace across
    // each leaf and the meeting stile between them.
    let door_h = BAY_TOP - 0.18;
    out.push(prim(
        solid(cuboid_tapered(
            [bay_w - 0.06, door_h, 0.1],
            0.0,
            painted(DOOR_GREEN),
        )),
        [0.0, 0.2 + door_h * 0.5, front + 0.05],
        id_quat(),
    ));
    let lean = (door_h - 0.4).atan2(bay_w * 0.5 - 0.3);
    let brace = ((door_h - 0.4).powi(2) + (bay_w * 0.5 - 0.3).powi(2)).sqrt();
    for (k, s) in [-1.0_f32, 1.0].into_iter().enumerate() {
        out.push(prim(
            cuboid_tapered([brace, 0.16, 0.06], 0.0, painted(CREAM)),
            [
                s * bay_w * 0.25,
                0.2 + door_h * 0.5,
                front - 0.025 - 0.01 * k as f32,
            ],
            quat_z(s * lean),
        ));
    }
    out.push(prim(
        cuboid_tapered([0.1, door_h, 0.08], 0.0, painted(CREAM)),
        [0.0, 0.2 + door_h * 0.5, front - 0.04],
        id_quat(),
    ));
    // Bay lamps under the balcony's soffit, lit at every hour.
    for i in 0..3 {
        let x = -HX + pier_w + bay_w * 0.5 + i as f32 * (pier_w + bay_w);
        out.push(prim(
            cuboid_tapered([0.36, 0.24, 0.36], 0.0, glow(LAMP_WARM, 3.0)),
            [x, FLOOR - 0.1, -HZ - 0.6],
            id_quat(),
        ));
    }
    out
}

/// The racked shells inside the boat hall: two to each bay on brackets
/// from the back wall, lying along X.
fn hall_racks() -> Vec<Generator> {
    let mut out = Vec::new();
    let back = HZ - 0.3;
    for bx in [-6.27_f32, 0.0, 6.27] {
        for (k, y) in [1.3_f32, 2.5].into_iter().enumerate() {
            for dx in [-1.6_f32, 1.6] {
                out.push(prim(
                    solid(cuboid_tapered([0.12, 0.1, 1.3], 0.0, steel(STEEL_GREY))),
                    [bx + dx, y, back - 0.6],
                    id_quat(),
                ));
            }
            let color = if k == 0 { HULL_CEDAR } else { LINE_WHITE };
            out.push(prim(
                superellipsoid([2.3, 0.14, 0.22], 1.0, 1.0, painted(color)),
                [bx, y + 0.18, back - 0.7],
                id_quat(),
            ));
        }
    }
    out
}

/// The clubroom storey: its front wall with lit windows, the balcony deck
/// on its posts, the stand's two rows of benches, its railing, and the
/// club board under the eaves with crossed oars.
fn clubroom() -> Vec<Generator> {
    let front = -HZ + 0.15;
    let upper_h = EAVES + 0.05 - FLOOR;
    let mut out = vec![prim(
        solid(cuboid_tapered([HX * 2.0, upper_h, 0.3], 0.0, timber(STAIN))),
        [0.0, FLOOR + upper_h * 0.5, front],
        id_quat(),
    )];
    for i in 0..5 {
        let x = -8.0 + i as f32 * 4.0;
        out.push(prim(
            solid(cuboid_tapered([1.9, 1.9, 0.06], 0.0, painted(CREAM))),
            [x, 6.1, -HZ - 0.015],
            id_quat(),
        ));
        out.push(prim(
            cuboid_tapered(
                [1.6, 1.6, 0.08],
                0.0,
                crate::catalogue::items::street::lit_from_dusk(glow(ROOM_WARM, 1.4)),
            ),
            [x, 6.1, -HZ - 0.04],
            id_quat(),
        ));
    }
    // The balcony deck, reaching into the wall behind it.
    let deck_back = -HZ + 0.2;
    let deck_d = deck_back - (BALCONY_FRONT - 0.2);
    out.push(prim(
        solid(cuboid_tapered(
            [HX * 2.0 + 0.4, DECK_TOP - FLOOR, deck_d],
            0.0,
            timber([0.46, 0.32, 0.20]),
        )),
        [0.0, (FLOOR + DECK_TOP) * 0.5, deck_back - deck_d * 0.5],
        id_quat(),
    ));
    // Posts from the slipway to the eaves.
    for i in 0..5 {
        let x = -9.8 + i as f32 * 4.9;
        out.push(prim(
            solid(cuboid_tapered(
                [0.25, EAVES + 0.05 - 0.1, 0.25],
                0.0,
                painted(CREAM),
            )),
            [x, 0.1 + (EAVES + 0.05 - 0.1) * 0.5, BALCONY_FRONT],
            id_quat(),
        ));
    }
    // The stand: a riser against the wall and two rows of club benches.
    out.push(prim(
        solid(cuboid_tapered(
            [HX * 2.0 - 0.6, 0.3, 1.0],
            0.0,
            timber([0.46, 0.32, 0.20]),
        )),
        [0.0, DECK_TOP + 0.15, -HZ - 0.5],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered([18.0, 0.45, 0.45], 0.0, painted(SEAT_BLUE))),
        [0.0, DECK_TOP + 0.225, -6.3],
        id_quat(),
    ));
    out.push(prim(
        solid(cuboid_tapered([17.6, 0.45, 0.45], 0.0, painted(SEAT_BLUE))),
        [0.0, DECK_TOP + 0.3 + 0.225, -HZ - 0.5],
        id_quat(),
    ));
    out.extend(railing(
        [-9.8, DECK_TOP, BALCONY_FRONT],
        [9.8, DECK_TOP, BALCONY_FRONT],
        1.0,
        0.42,
        painted(CREAM),
    ));
    // The club board under the eaves, and crossed oars on it, blades up.
    // Each blade is a sibling of its loom: under the turned loom it would
    // be turned with it.
    let board_z = BALCONY_FRONT - 0.15;
    out.push(prim(
        cuboid_tapered([7.0, 0.7, 0.08], 0.0, painted(DOOR_GREEN)),
        [0.0, EAVES - 0.45, board_z],
        id_quat(),
    ));
    for (k, s) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let z = board_z - 0.07 - 0.03 * k as f32;
        out.push(prim(
            cylinder_tapered(0.03, 2.6, 6, 0.0, painted(CREAM)),
            [0.0, EAVES - 0.45, z],
            quat_z(FRAC_PI_2 + s * 0.24),
        ));
        out.push(prim(
            cuboid_tapered([0.5, 0.16, 0.03], 0.0, painted(HULL_YELLOW)),
            [s * 1.35, EAVES - 0.45 + 0.32, z],
            quat_z(s * 0.24),
        ));
    }
    out
}

/// The boathouse, rooted on its boat-hall floor: walls, front, racks,
/// clubroom, the intermediate floor and the roof.
fn boathouse() -> Generator {
    let full = EAVES + 0.05;
    let mut parts = vec![
        // Back wall, standing a hair proud of the end walls' backs.
        prim(
            solid(cuboid_tapered([HX * 2.0, full, 0.3], 0.0, timber(STAIN))),
            [0.0, full * 0.5, HZ - 0.18],
            id_quat(),
        ),
        // The boat hall's ceiling, the clubroom's floor.
        prim(
            solid(cuboid_tapered(
                [HX * 2.0 - 0.6, 0.25, HZ * 2.0 - 0.6],
                0.0,
                timber([0.30, 0.20, 0.14]),
            )),
            [0.0, FLOOR + 0.1, 0.0],
            id_quat(),
        ),
        gable_roof(
            [HX * 2.0 + 0.8, 2.6, HZ - BALCONY_FRONT + 0.7],
            [0.0, EAVES + 1.3, (HZ + 0.4 + BALCONY_FRONT - 0.3) * 0.5],
            corrugated(ROOF_GREEN),
        ),
    ];
    // End walls, their outer faces a hair proud of the front's ends, each
    // with a pair of the clubroom's lit windows.
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.32, full, HZ * 2.0 - 0.1],
                0.0,
                timber(STAIN),
            )),
            [s * (HX - 0.14), full * 0.5, 0.0],
            id_quat(),
        ));
        let face = s * (HX + 0.02);
        for z in [-1.8_f32, 1.8] {
            parts.push(prim(
                solid(cuboid_tapered([0.06, 1.7, 1.5], 0.0, painted(CREAM))),
                [face + s * 0.015, 6.0, z],
                id_quat(),
            ));
            parts.push(prim(
                cuboid_tapered(
                    [0.08, 1.4, 1.2],
                    0.0,
                    crate::catalogue::items::street::lit_from_dusk(glow(ROOM_WARM, 1.4)),
                ),
                [face + s * 0.04, 6.0, z],
                id_quat(),
            ));
        }
    }
    parts.extend(hall_front());
    parts.extend(hall_racks());
    parts.extend(clubroom());
    nest(
        prim(
            solid(cuboid_tapered(
                [HX * 2.0 + 0.2, 0.2, HZ * 2.0 + 0.2],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        parts,
    )
}

/// The slipway apron, its ramp down to the water, and the club's flagpole
/// with its pennant.
fn slipway() -> Generator {
    let back = -HZ - 0.1;
    let d = back - SLIP_FRONT;
    let (fx, fz) = (-11.2_f32, SLIP_FRONT + 0.7);
    let pole_h = 9.0;
    let pennant = prim_scaled(
        cone(0.45, 2.0, 12, painted(SEAT_RED)),
        [fx + 1.05, pole_h - 0.5, fz],
        quat_z(-FRAC_PI_2),
        [1.0, 1.0, 0.06],
    );
    nest(
        prim(
            solid(cuboid_tapered(
                [HX * 2.0 + 3.0, 0.18, d],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [-0.5, 0.09, back - d * 0.5],
            id_quat(),
        ),
        vec![
            // The ramp, its high edge under the apron's front.
            prim(
                solid(wedge([12.0, 0.36, 4.0], concrete([0.52, 0.51, 0.49]))),
                [0.0, 0.0, SLIP_FRONT - 1.95],
                quat_y(std::f32::consts::PI),
            ),
            nest(
                prim(
                    solid(cylinder_tapered(0.09, pole_h, 10, 0.25, steel(STEEL_GREY))),
                    [fx, pole_h * 0.5 + 0.1, fz],
                    id_quat(),
                ),
                vec![
                    pennant,
                    prim(
                        cylinder_tapered(0.12, 0.12, 10, 0.0, painted(HULL_YELLOW)),
                        [fx, pole_h + 0.12, fz],
                        id_quat(),
                    ),
                ],
            ),
        ],
    )
}

/// The rack off the east end: two posts with three arms each, and a shell
/// on every arm.
fn rack() -> Generator {
    let mut parts = Vec::new();
    for z in [-2.6_f32, 2.6] {
        if z > 0.0 {
            parts.push(prim(
                solid(cuboid_tapered([0.15, 2.8, 0.15], 0.0, steel(STEEL_GREY))),
                [RACK_X, 1.5, z],
                id_quat(),
            ));
        }
        for y in [0.9_f32, 1.6, 2.3] {
            parts.push(prim(
                solid(cuboid_tapered([1.4, 0.1, 0.12], 0.0, steel(STEEL_GREY))),
                [RACK_X, y, z],
                id_quat(),
            ));
        }
    }
    for (y, color) in [(0.9_f32, HULL_YELLOW), (1.6, LINE_WHITE), (2.3, HULL_CEDAR)] {
        parts.push(shell([RACK_X, y + 0.18, 0.0], 8.0, color));
    }
    nest(
        prim(
            solid(cuboid_tapered([0.15, 2.8, 0.15], 0.0, steel(STEEL_GREY))),
            [RACK_X, 1.5, -2.6],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let (x0, x1) = (-12.2_f32, RACK_X + 0.9);
    let (z0, z1) = (SLIP_FRONT, HZ + 0.2);
    let mut root = footing(
        x1 - x0,
        z1 - z0,
        [(x0 + x1) * 0.5, (z0 + z1) * 0.5],
        CLEARANCE,
    );
    for part in [boathouse(), slipway(), rack()] {
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
        assert_sanitize_stable(&RowingBoathouse.build(""), "rowing_boathouse");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = RowingBoathouse.build("");
        assert_no_z_fighting(&built, "rowing_boathouse");
        assert_nothing_floats(&built, "rowing_boathouse");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&RowingBoathouse.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(bytes < 60_000, "rowing_boathouse serialises to {bytes} B");
    }
}
