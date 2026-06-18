//! Ruined chapel — the Gothic-Horror *poor* landmark. A roofless, crumbling
//! little chapel of broken mossy walls and a shattered arch, rubble strewn at
//! its foot and a leaning grave-cross. The forsaken counterpart to the
//! [`cathedral`](super::cathedral): same faith, opposite end of the prosperity
//! axis (`Poor`), so a destitute gothic room grows the abandoned ruin instead
//! of the consecrated cathedral.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the footing.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, STONE_MOSS, mossy, wood};

pub struct RuinedChapel;

impl CatalogueEntry for RuinedChapel {
    fn slug(&self) -> &'static str {
        "ruined_chapel"
    }
    fn name(&self) -> &'static str {
        "Ruined Chapel"
    }
    fn description(&self) -> &'static str {
        "Roofless crumbling chapel of broken mossy walls and a shattered arch."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let foot_h = 0.4_f32;

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered([7.0, foot_h, 5.0], 0.0, mossy(STONE_MOSS))),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Broken side walls at varying heights.
    for (x, h) in [(-3.0_f32, 3.0_f32), (-3.0, 1.4), (3.0, 2.4), (3.0, 0.9)] {
        let z = if h > 2.0 { -1.2 } else { 1.2 };
        prims.push(prim(
            solid(cuboid_tapered([0.5, h, 2.0], 0.0, mossy(STONE_MOSS))),
            [x, foot_h + h * 0.5, z],
            id_quat(),
        ));
    }
    // A low back wall.
    prims.push(prim(
        solid(cuboid_tapered([6.0, 1.6, 0.5], 0.0, mossy(STONE_MOSS))),
        [0.0, foot_h + 0.8, -2.2],
        id_quat(),
    ));

    // Shattered pointed arch at the front: two jambs, one broken short.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 2.8, 0.6], 0.0, mossy(STONE_MOSS))),
        [-1.0, foot_h + 1.4, 2.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 1.6, 0.6], 0.1, mossy(STONE_MOSS))),
        [1.0, foot_h + 0.8, 2.0],
        id_quat(),
    ));

    // Rubble piles.
    for (rx, rz) in [(0.6_f32, 1.4_f32), (-1.6, -0.4), (1.8, 0.6)] {
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.5, 0.9], 0.4, mossy(STONE_MOSS))),
            [rx, foot_h + 0.25, rz],
            id_quat(),
        ));
    }

    // Leaning grave-cross of dead wood.
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.6, 0.16], 0.0, wood(DEADWOOD))),
        [-2.4, foot_h + 0.8, 1.6],
        quat_x(0.2),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.16, 0.16], 0.0, wood(DEADWOOD))),
        [-2.4, foot_h + 1.3, 1.7],
        quat_x(0.2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RuinedChapel.build(""), "ruined_chapel");
    }
}
