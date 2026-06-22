//! Rusted tank — an Industrial-Park *poor* secondary. A single corroded
//! storage tank with a stove-in top, leaning on a cracked pad in a spreading
//! stain, beside the [`derelict_shed`](super::derelict_shed).

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
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

    // Corroded tank leaning a little off true — a subtree so its dished cap
    // and round hoops lean with it (the old square cuboid bands jutted their
    // corners past the wall).
    let h = 6.0_f32;
    let mut tank = prim(
        solid(cylinder_tapered(2.3, h, 20, 0.0, rust(RUST_BROWN))),
        [0.0, 0.3 + h * 0.5, 0.0],
        quat_x(0.05),
    );
    // Stove-in top: a sunken dark cap (local, at the tank crown).
    tank.children.push(prim(
        solid(cylinder_tapered(2.1, 0.4, 20, 0.6, rust([0.3, 0.2, 0.12]))),
        [0.0, h * 0.5 - 0.2, 0.0],
        id_quat(),
    ));
    // Two round hoop bands still gripping, plus a third slipped loose and
    // sagging off the shell.
    for k in 1..3 {
        tank.children.push(prim(
            torus(0.08, 2.37, rust([0.38, 0.24, 0.14])),
            [0.0, -h * 0.5 + h * (k as f32 / 3.0), 0.0],
            id_quat(),
        ));
    }
    tank.children.push(prim(
        torus(0.08, 2.42, rust([0.36, 0.23, 0.13])),
        [0.0, -h * 0.5 + 0.7, 0.25],
        quat_x(0.22),
    ));
    prims.push(tank);

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
    // A hoop band that fell clean off, lying buckled on the pad.
    prims.push(prim(
        torus(0.08, 1.7, rust([0.34, 0.22, 0.12])),
        [-2.0, 0.36, 1.6],
        quat_x(1.45),
    ));
    // Scattered scale and a fallen panel.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.2, 0.12, 0.8],
            0.0,
            rust([0.36, 0.23, 0.13]),
        )),
        [2.3, 0.4, -1.4],
        quat_z(0.3),
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
