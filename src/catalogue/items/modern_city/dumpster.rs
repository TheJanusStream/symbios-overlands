//! Dumpster — a Modern-City prop. A steel waste container with slanted
//! plastic lids and small caster wheels, parked in the alley behind the
//! buildings.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DUMPSTER_GREEN, TIRE_BLACK, enamel};

pub struct Dumpster;

impl CatalogueEntry for Dumpster {
    fn slug(&self) -> &'static str {
        "dumpster"
    }
    fn name(&self) -> &'static str {
        "Dumpster"
    }
    fn description(&self) -> &'static str {
        "Steel waste container with slanted lids on small caster wheels."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Body — the root, slightly battered with a sloped (tapered) profile.
        prim(
            solid(cuboid_tapered(
                [2.4, 1.4, 1.5],
                0.06,
                enamel(DUMPSTER_GREEN),
            )),
            [0.0, 0.85, 0.0],
            id_quat(),
        ),
    ];

    // Ribbed steel front (−Z) with two forklift pockets.
    let front_z = -0.75_f32;
    for x in [-0.9_f32, -0.3, 0.3, 0.9] {
        prims.push(prim(
            cuboid_tapered([0.1, 1.2, 0.1], 0.0, enamel([0.13, 0.26, 0.18])),
            [x, 0.85, front_z - 0.04],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.6, 0.26, 0.18],
                0.0,
                enamel([0.07, 0.1, 0.08]),
            )),
            [sx * 0.6, 0.45, front_z - 0.02],
            id_quat(),
        ));
    }
    // Lift bar across the top front for the truck to grab.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.5, 0.12, 0.12],
            0.0,
            enamel([0.2, 0.2, 0.21]),
        )),
        [0.0, 1.5, front_z - 0.08],
        id_quat(),
    ));

    // Two slanted lids meeting at the centre.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [2.4, 0.1, 0.85],
                0.0,
                enamel([0.12, 0.2, 0.14]),
            )),
            [0.0, 1.6, sz * 0.4],
            quat_x(sz * 0.18),
        ));
    }

    // Caster wheels.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.22, 0.3, 0.3], 0.0, enamel(TIRE_BLACK))),
            [sx * 1.0, 0.15, sz * 0.6],
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
        assert_sanitize_stable(&Dumpster.build(""), "dumpster");
    }
}
