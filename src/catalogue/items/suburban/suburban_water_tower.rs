//! Suburban water tower - a Suburban secondary that leads a suburb from its
//! hill. A cream steel sphere banded in blue stands on six legs braced in
//! X panels round a central riser, a railed balcony at its waist and a
//! warning lamp on its crown; a ladder climbs the front leg, and a small
//! brick pump house at its foot is piped to the riser.
//!
//! Primitive-built in one flat ground-relative frame. The tower hangs off
//! the footing as one sub-assembly rooted on its riser, so a ruined suburb
//! leans it whole; the pump house is the other.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, solid,
    sphere, strut, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_TAN, PORCH_WARM, ROOF_GREY, WOOD_WHITE, brick, concrete, enamel, shingle};

/// The tank's radius and the height of its waist.
const TANK_R: f32 = 6.5;
const WAIST: f32 = 26.0;
/// The legs: how many, the circle they stand on, their radius.
const LEGS: usize = 6;
const LEG_RING: f32 = 6.3;
const LEG_R: f32 = 0.35;
/// The waist balcony's deck, below the band.
const BALCONY: f32 = WAIST - 1.6;
/// Ground the legs and riser stand on: the footing's top.
const GRADE: f32 = 0.15;
/// The pump house's centre (X), half width, depth and wall height.
const PUMP_X: f32 = 9.5;
const PUMP_HX: f32 = 2.0;
const PUMP_D: f32 = 3.6;
const PUMP_H: f32 = 3.0;
/// Spacing circle the settlement keeps round the tower.
const CLEARANCE: f32 = 11.0;

/// The tank's cream and its painted band's blue.
const TANK_CREAM: [f32; 3] = [0.88, 0.86, 0.78];
const BAND_BLUE: [f32; 3] = [0.20, 0.38, 0.62];
/// Grey of the legs, braces, riser and balcony steel.
const STEEL: [f32; 3] = [0.56, 0.58, 0.60];
/// The warning lamp on the crown, lit at every hour.
const WARNING_RED: [f32; 3] = [1.0, 0.12, 0.08];
/// The pump house's green door.
const DOOR_GREEN: [f32; 3] = [0.18, 0.34, 0.24];

pub struct SuburbanWaterTower;

impl CatalogueEntry for SuburbanWaterTower {
    fn slug(&self) -> &'static str {
        "suburban_water_tower"
    }
    fn name(&self) -> &'static str {
        "Suburban Water Tower"
    }
    fn description(&self) -> &'static str {
        "Banded steel sphere on braced legs over the suburb, with a ladder and a brick pump house."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// The hill above the houses, for the head of water.
    fn site(&self) -> SitePreference {
        SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
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

/// Plain grey steelwork paint, untextured: the braces and ladder are many
/// thin parts, and a texture would cost bytes and read as nothing.
fn steel() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(STEEL),
        roughness: Fp(0.5),
        metallic: Fp(0.6),
        ..Default::default()
    }
}

/// Where leg `k` stands, its first a half step round from +X so one leg
/// stands on the -Z front for the ladder.
fn leg_at(k: usize) -> [f32; 2] {
    let a = TAU * (k as f32 + 0.5) / LEGS as f32;
    [LEG_RING * a.cos(), LEG_RING * a.sin()]
}

/// The tower, rooted on its riser: the legs on their pads, the bracing,
/// the tank with its band, balcony and crown, and the front ladder.
fn tower() -> Generator {
    let bottom = WAIST - TANK_R;
    let mut parts = Vec::new();
    for k in 0..LEGS {
        let [x, z] = leg_at(k);
        // A concrete pad under the leg, and the leg up into the tank.
        parts.push(prim(
            solid(cuboid_tapered(
                [1.2, 0.6, 1.2],
                0.0,
                concrete([0.66, 0.65, 0.62]),
            )),
            [x, GRADE + 0.25, z],
            id_quat(),
        ));
        parts.push(prim(
            solid(cylinder_tapered(
                LEG_R,
                WAIST - GRADE,
                14,
                0.0,
                enamel(STEEL),
            )),
            [x, (WAIST + GRADE) * 0.5, z],
            id_quat(),
        ));
    }
    // Bracing up to the tank's underside: a ring tie between neighbouring
    // legs at each panel line, and a pair of crossed rods in every panel,
    // the rods a little apart in depth so they pass one another.
    let panels = [GRADE + 0.6, 6.6, 12.6, 18.6];
    for k in 0..LEGS {
        let [ax, az] = leg_at(k);
        let [bx, bz] = leg_at((k + 1) % LEGS);
        for &y in &panels[1..] {
            parts.push(strut([ax, y, az], [bx, y, bz], 0.14, 8, steel()));
        }
        for pair in panels.windows(2) {
            let (y0, y1) = (pair[0], pair[1]);
            parts.push(strut([ax, y0, az], [bx, y1, bz], 0.06, 6, steel()));
            let n = [(ax + bx) * 0.5, (az + bz) * 0.5];
            let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
            let out = [n[0] / len * 0.1, n[1] / len * 0.1];
            parts.push(strut(
                [bx + out[0], y0, bz + out[1]],
                [ax + out[0], y1, az + out[1]],
                0.06,
                6,
                steel(),
            ));
        }
    }
    // The tank, its band, and the balcony deck and railing at its waist.
    parts.push(prim(
        solid(sphere(TANK_R, 6, enamel(TANK_CREAM))),
        [0.0, WAIST, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        tube(TANK_R + 0.05, TANK_R - 0.4, 1.8, 64, enamel(BAND_BLUE)),
        [0.0, WAIST + 0.5, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        solid(tube(TANK_R + 1.1, TANK_R - 0.5, 0.14, 64, steel())),
        [0.0, BALCONY, 0.0],
        id_quat(),
    ));
    let rail_r = TANK_R + 1.0;
    for dy in [0.5_f32, 1.0] {
        parts.push(prim(
            torus(0.05, rail_r, steel()),
            [0.0, BALCONY + dy, 0.0],
            id_quat(),
        ));
    }
    for i in 0..16 {
        let a = TAU * (i as f32 + 0.25) / 16.0;
        parts.push(prim(
            cylinder_tapered(0.045, 1.0, 6, 0.0, steel()),
            [rail_r * a.cos(), BALCONY + 0.55, rail_r * a.sin()],
            id_quat(),
        ));
    }
    // The crown: a hatch drum, its cap, and the warning lamp.
    let crown = WAIST + TANK_R;
    parts.push(prim(
        solid(cylinder_tapered(0.7, 0.7, 16, 0.0, enamel(TANK_CREAM))),
        [0.0, crown + 0.25, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        cone(0.9, 0.6, 16, enamel(BAND_BLUE)),
        [0.0, crown + 0.85, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        sphere(0.2, 3, glow(WARNING_RED, 5.0)),
        [0.0, crown + 1.2, 0.0],
        id_quat(),
    ));
    // The ladder up the front leg to the balcony, on brackets off the leg.
    let front = leg_at(LEGS * 3 / 4);
    let lz = front[1] - LEG_R - 0.25;
    let ladder_h = BALCONY - GRADE;
    for dx in [-0.25_f32, 0.25] {
        parts.push(prim(
            cylinder_tapered(0.04, ladder_h, 6, 0.0, steel()),
            [front[0] + dx, GRADE + ladder_h * 0.5, lz],
            id_quat(),
        ));
    }
    let rungs = (ladder_h / 0.7) as usize;
    for i in 1..rungs {
        parts.push(prim(
            cuboid_tapered([0.5, 0.04, 0.04], 0.0, steel()),
            [front[0], GRADE + i as f32 * 0.7, lz],
            id_quat(),
        ));
    }
    for y in [3.0_f32, 10.0, 17.0, 23.0] {
        parts.push(prim(
            cuboid_tapered([0.6, 0.06, 0.42], 0.0, steel()),
            [front[0], y, lz + 0.18],
            id_quat(),
        ));
    }
    // The supply main from the riser's foot to the pump house.
    let main_len = PUMP_X - PUMP_HX + 0.1;
    parts.push(prim(
        cylinder_tapered(0.25, main_len, 12, 0.0, enamel(BAND_BLUE)),
        [main_len * 0.5, GRADE + 0.6, 0.0],
        crate::catalogue::items::util::quat_z(FRAC_PI_2),
    ));
    let riser_h = bottom + 0.3 - GRADE;
    nest(
        prim(
            solid(cylinder_tapered(0.8, riser_h, 20, 0.0, enamel(STEEL))),
            [0.0, GRADE + riser_h * 0.5, 0.0],
            id_quat(),
        ),
        parts,
    )
}

/// The pump house: a brick box under a shingled gable, a door and a small
/// window on -Z, and a lamp over the door.
fn pump_house() -> Generator {
    let front = -PUMP_D * 0.5;
    nest(
        prim(
            solid(cuboid_tapered(
                [PUMP_HX * 2.0, PUMP_H, PUMP_D],
                0.0,
                brick(BRICK_TAN),
            )),
            [PUMP_X, GRADE + PUMP_H * 0.5 - 0.05, 0.0],
            id_quat(),
        ),
        vec![
            gable_roof(
                [PUMP_HX * 2.0 + 0.5, 1.4, PUMP_D + 0.5],
                [PUMP_X, GRADE + PUMP_H - 0.07 + 0.7, 0.0],
                shingle(ROOF_GREY),
            ),
            prim(
                solid(cuboid_tapered([1.0, 2.1, 0.1], 0.0, enamel(DOOR_GREEN))),
                [PUMP_X - 0.6, GRADE + 1.03, front - 0.03],
                id_quat(),
            ),
            prim(
                cuboid_tapered(
                    [0.8, 0.7, 0.08],
                    0.0,
                    crate::catalogue::items::street::lit_from_dusk(glow(PORCH_WARM, 1.3)),
                ),
                [PUMP_X + 1.0, GRADE + 1.9, front - 0.03],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered([1.0, 0.1, 0.2], 0.0, enamel(WOOD_WHITE))),
                [PUMP_X + 1.0, GRADE + 1.5, front - 0.08],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.26, 0.26, 0.26], 0.0, glow(PORCH_WARM, 3.0)),
                [PUMP_X - 0.6, GRADE + 2.45, front - 0.12],
                id_quat(),
            ),
        ],
    )
}

fn build_tree() -> Generator {
    let reach = LEG_RING + 0.7;
    let x1 = PUMP_X + PUMP_HX + 0.3;
    let mut root = footing(
        x1 + reach,
        reach * 2.0,
        [(x1 - reach) * 0.5, 0.0],
        CLEARANCE,
    );
    attach(&mut root, tower());
    attach(&mut root, pump_house());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SuburbanWaterTower.build(""), "suburban_water_tower");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = SuburbanWaterTower.build("");
        assert_no_z_fighting(&built, "suburban_water_tower");
        assert_nothing_floats(&built, "suburban_water_tower");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&SuburbanWaterTower.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(
            bytes < 60_000,
            "suburban_water_tower serialises to {bytes} B"
        );
    }
}
