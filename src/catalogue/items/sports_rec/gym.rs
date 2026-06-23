//! Gym — a Sports/Recreation secondary. A big corrugated-clad sports hall
//! with a clerestory glazing band, a lit glass entrance under a concrete
//! canopy and a colour sign band. The indoor training shed of the complex.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::modern_city::curtain_wall;
use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_GREY, GLASS_TINT, SCORE_LIT, SEAT_BLUE, STEEL_GREY, concrete,
    corrugated, enamel, glass, steel,
};

pub struct Gym;

impl CatalogueEntry for Gym {
    fn slug(&self) -> &'static str {
        "gym"
    }
    fn name(&self) -> &'static str {
        "Gym"
    }
    fn description(&self) -> &'static str {
        "Corrugated sports hall with a clerestory band and a lit glass entrance."
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
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.4_f32;
    let body_h = 6.0_f32;
    let body_top = slab_h + body_h;
    let fz = -1.0_f32; // hero faces the −Z render front

    let mut prims = vec![
        // Concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [14.0, slab_h, 10.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Corrugated body.
    prims.push(prim(
        solid(cuboid_tapered(
            [12.0, body_h, 8.0],
            0.0,
            corrugated(CORRUGATED_GREY),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Roof cap and two rooftop plant boxes.
    prims.push(prim(
        solid(cuboid_tapered(
            [12.4, 0.4, 8.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.7, 1.4], 0.0, steel(STEEL_GREY))),
            [sx * 3.0, body_top + 0.75, 0.0],
            id_quat(),
        ));
    }

    // Gridded clerestory glazing band high on the front (curtain-wall strip).
    prims.extend(curtain_wall(
        [0.0, slab_h + 5.0, fz * 4.05],
        [11.0, 1.0],
        (8, 1),
        fz * 0.18,
        glass(GLASS_TINT, 1.2),
        steel(STEEL_GREY),
    ));
    // Glazed entrance — a steel-mullioned curtain wall facing the front.
    prims.extend(curtain_wall(
        [0.0, slab_h + 1.5, fz * 4.05],
        [3.6, 2.6],
        (3, 2),
        fz * 0.2,
        glass(GLASS_TINT, 1.3),
        steel(STEEL_GREY),
    ));
    // Concrete entrance canopy proud of the front.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.6, 0.3, 1.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, slab_h + 3.0, fz * 4.7],
        id_quat(),
    ));
    // Club-colour fascia band with a lit, deep-saturated name plate so the
    // sign reads lit without blooming to a flat white slab.
    prims.push(prim(
        solid(cuboid_tapered([7.0, 0.9, 0.12], 0.0, enamel(SEAT_BLUE))),
        [0.0, slab_h + 3.7, fz * 4.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.0, 0.55, 0.1], 0.0, glow(SCORE_LIT, 1.8)),
        [0.0, slab_h + 3.7, fz * 4.13],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Gym.build(""), "gym");
    }
}
