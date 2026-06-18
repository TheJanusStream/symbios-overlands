//! Cemetery — a Gothic-Horror secondary. A mossy grave plot of leaning
//! headstones behind an iron railing, a stone cross at its heart, mist
//! pooling between the rows. The burial ground of the necropolis.
//!
//! Leaning stones tilt with a single [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STONE_MOSS, fx, iron, mossy};

pub struct Cemetery;

impl CatalogueEntry for Cemetery {
    fn slug(&self) -> &'static str {
        "cemetery"
    }
    fn name(&self) -> &'static str {
        "Cemetery"
    }
    fn description(&self) -> &'static str {
        "Mossy grave plot of leaning headstones behind an iron railing, a stone cross at its heart."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Mossy grave plot — the root.
        prim(
            solid(cuboid_tapered([8.0, 0.2, 6.0], 0.0, mossy(STONE_MOSS))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];

    // Rows of leaning headstones.
    let mut k = 0;
    for gx in [-2.6_f32, -0.9, 0.8, 2.5] {
        for gz in [-1.6_f32, 0.4, 2.0] {
            let tilt = ((k % 3) as f32 - 1.0) * 0.12;
            prims.push(prim(
                solid(cuboid_tapered([0.6, 1.0, 0.16], 0.1, mossy(STONE_MOSS))),
                [gx, 0.6, gz],
                quat_x(tilt),
            ));
            k += 1;
        }
    }

    // Stone cross at the centre-back.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 2.2, 0.3], 0.0, mossy(STONE_MOSS))),
        [0.0, 1.3, -2.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 0.3, 0.3], 0.0, mossy(STONE_MOSS))),
        [0.0, 1.9, -2.2],
        id_quat(),
    ));

    // Iron railing along the front edge.
    prims.push(prim(
        solid(cuboid_tapered([8.0, 0.08, 0.06], 0.0, iron(IRON_BLACK))),
        [0.0, 0.9, 3.0],
        id_quat(),
    ));
    for i in 0..9 {
        let x = -3.8 + i as f32 * 0.95;
        prims.push(prim(
            solid(cylinder_tapered(0.04, 1.0, 6, 0.0, iron(IRON_BLACK))),
            [x, 0.6, 3.0],
            id_quat(),
        ));
    }
    let mut root = assemble(prims);
    // Signature life: mist pooling between the rows.
    root.children
        .push(fx::ground_mist([0.0, 0.3, 0.0], 0x60F0_CE12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Cemetery.build(""), "cemetery");
    }
}
