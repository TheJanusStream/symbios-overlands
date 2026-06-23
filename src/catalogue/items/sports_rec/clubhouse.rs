//! Clubhouse — a Sports/Recreation secondary. A low cream pavilion with a
//! covered veranda, lit windows, a gable clock and a pitched corrugated roof.
//! The changing rooms and social club of the ground.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the plinth.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_y, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_GREY, GLASS_TINT, STEEL_GREY, concrete, corrugated, glass, painted,
    steel,
};

/// Cream of the painted clubhouse weatherboard.
const CLUB_CREAM: [f32; 3] = [0.86, 0.83, 0.74];

pub struct Clubhouse;

impl CatalogueEntry for Clubhouse {
    fn slug(&self) -> &'static str {
        "clubhouse"
    }
    fn name(&self) -> &'static str {
        "Clubhouse"
    }
    fn description(&self) -> &'static str {
        "Cream pavilion with a covered veranda, lit windows and a gable clock."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plinth_h = 0.4_f32;
    let body_h = 3.0_f32;
    let body_top = plinth_h + body_h;
    // The veranda, glazing and clock are authored on the −Z render front; the
    // body is set back toward +Z behind the veranda.

    let mut prims = vec![
        // Concrete plinth — the root.
        prim(
            solid(cuboid_tapered(
                [12.0, plinth_h, 7.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, plinth_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Painted weatherboard body, set back behind the veranda.
    prims.push(prim(
        solid(cuboid_tapered(
            [10.0, body_h, 5.0],
            0.0,
            painted(CLUB_CREAM),
        )),
        [0.0, plinth_h + body_h * 0.5, 0.6],
        id_quat(),
    ));
    // Lit window band on the front (gridded glass survives the glow).
    prims.push(prim(
        cuboid_tapered([8.5, 1.2, 0.15], 0.0, glass(GLASS_TINT, 1.2)),
        [0.0, plinth_h + 1.6, -1.95],
        id_quat(),
    ));

    // Covered veranda: a corrugated roof on steel posts, out over the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [11.0, 0.2, 2.4],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, body_top - 0.3, -2.6],
        id_quat(),
    ));
    for x in [-5.0_f32, -1.7, 1.7, 5.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, body_h - 0.3, 0.12],
                0.0,
                steel(STEEL_GREY),
            )),
            [x, plinth_h + (body_h - 0.3) * 0.5, -3.6],
            id_quat(),
        ));
        // Front fascia rail tying the post tops, just under the veranda roof.
    }
    prims.push(prim(
        solid(cuboid_tapered([11.0, 0.2, 0.12], 0.0, steel(STEEL_GREY))),
        [0.0, body_top - 0.55, -3.6],
        id_quat(),
    ));

    // Pitched corrugated gable roof (ridge along the long X axis).
    prims.push(gable_roof(
        [10.4, 1.8, 5.4],
        [0.0, body_top + 0.9, 0.6],
        corrugated(CORRUGATED_GREY),
    ));

    // Gable clocks — a real dial on each ±X gable end, in the triangle above
    // the eaves (the veranda roof covers the front wall, so a front-wall clock
    // is occluded; the gable end reads clearly in the side / three-quarter
    // tiles). Dark rim, pale face, hub and crossed hands, proud of the gable.
    let clock_y = 4.2_f32;
    let clock_z = 0.6_f32;
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * 5.3;
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 0.8, 0.8],
                0.0,
                painted([0.95, 0.94, 0.90]),
            )),
            [cx, clock_y, clock_z],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.05, 0.4, painted([0.16, 0.16, 0.18]))),
            [cx + sx * 0.06, clock_y, clock_z],
            quat_y(FRAC_PI_2),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [0.1, 0.1, 0.1],
                0.0,
                painted([0.16, 0.16, 0.18]),
            )),
            [cx + sx * 0.09, clock_y, clock_z],
            id_quat(),
        ));
        // Hour hand (up) and minute hand (across, along Z).
        prims.push(prim(
            solid(cuboid_tapered(
                [0.05, 0.3, 0.05],
                0.0,
                painted([0.16, 0.16, 0.18]),
            )),
            [cx + sx * 0.12, clock_y + 0.13, clock_z],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [0.05, 0.05, 0.36],
                0.0,
                painted([0.16, 0.16, 0.18]),
            )),
            [cx + sx * 0.12, clock_y, clock_z + 0.14],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Clubhouse.build(""), "clubhouse");
    }
}
