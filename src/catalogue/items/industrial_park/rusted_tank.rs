//! Rusted tank — an Industrial-Park *poor* secondary. A single corroded
//! storage tank with a stove-in top, leaning on a cracked pad in a spreading
//! stain, beside the [`derelict_shed`](super::derelict_shed).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, concrete, rust};

pub struct RustedTank;

impl CatalogueEntry for RustedTank {
    fn slug(&self) -> &'static str {
        "rusted_tank"
    }
    fn name(&self) -> &'static str {
        "Rusted Tank"
    }
    fn description(&self) -> &'static str {
        "Corroded storage tank with a stove-in top, leaning in a spreading stain."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Cracked, stained concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [7.0, 0.3, 7.0],
                0.0,
                concrete([0.34, 0.32, 0.28]),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Corroded tank leaning a little off true.
    let h = 6.0_f32;
    prims.push(prim(
        solid(cylinder_tapered(2.3, h, 20, 0.0, rust(RUST_BROWN))),
        [0.0, 0.3 + h * 0.5, 0.0],
        quat_x(0.05),
    ));
    // Stove-in top: a sunken dark cap.
    prims.push(prim(
        solid(cylinder_tapered(2.1, 0.4, 20, 0.6, rust([0.3, 0.2, 0.12]))),
        [0.15, 0.3 + h - 0.2, 0.0],
        quat_x(0.05),
    ));
    // Hoop bands hanging loose.
    for k in 1..3 {
        prims.push(prim(
            cuboid_tapered([4.7, 0.14, 4.7], 0.0, rust([0.38, 0.24, 0.14])),
            [0.0, 0.3 + h * (k as f32 / 3.0), 0.0],
            quat_x(0.05),
        ));
    }

    // A burst seam panel peeled off at the base.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.4, 1.6, 0.1],
            0.0,
            rust([0.4, 0.26, 0.14]),
        )),
        [1.8, 0.8, 1.4],
        quat_x(0.5),
    ));
    // Spreading dark stain on the pad.
    prims.push(prim(
        cuboid_tapered([3.0, 0.04, 2.2], 0.0, concrete([0.12, 0.11, 0.10])),
        [1.5, 0.32, 1.6],
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
        assert_sanitize_stable(&RustedTank.build(""), "rusted_tank");
    }
}
