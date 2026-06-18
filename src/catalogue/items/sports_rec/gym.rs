//! Gym — a Sports/Recreation secondary. A big corrugated-clad sports hall
//! with a clerestory glazing band, a lit glass entrance under a concrete
//! canopy and a colour sign band. The indoor training shed of the complex.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_GREY, GLASS_TINT, SEAT_BLUE, concrete, corrugated, enamel, glass,
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
    // Clerestory glazing band near the top.
    prims.push(prim(
        cuboid_tapered([11.0, 1.0, 0.15], 0.0, glass(GLASS_TINT, 1.2)),
        [0.0, slab_h + 5.0, 4.05],
        id_quat(),
    ));
    // Lit glass entrance + concrete canopy.
    prims.push(prim(
        cuboid_tapered([3.4, 2.6, 0.2], 0.0, glass(GLASS_TINT, 1.3)),
        [0.0, slab_h + 1.3, 4.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [4.4, 0.3, 1.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, slab_h + 2.8, 4.7],
        id_quat(),
    ));
    // Colour sign band over the entrance.
    prims.push(prim(
        cuboid_tapered([6.0, 0.8, 0.1], 0.0, enamel(SEAT_BLUE)),
        [0.0, slab_h + 3.6, 4.05],
        id_quat(),
    ));
    // Roof cap.
    prims.push(prim(
        solid(cuboid_tapered(
            [12.4, 0.4, 8.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
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
