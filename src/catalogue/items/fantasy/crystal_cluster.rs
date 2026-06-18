//! Crystal cluster — a High-Fantasy prop. A knot of glowing crystal shards
//! jutting from a rocky base at wild angles. Scatter clutter of the arcane
//! quarter; the shards are emissive trim the ruin pass can darken.
//!
//! The leaning shards are cones tilted with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, STONE_GREY, stone};

pub struct CrystalCluster;

impl CatalogueEntry for CrystalCluster {
    fn slug(&self) -> &'static str {
        "crystal_cluster"
    }
    fn name(&self) -> &'static str {
        "Crystal Cluster"
    }
    fn description(&self) -> &'static str {
        "Knot of glowing crystal shards jutting from a rocky base at wild angles."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Rocky base — the root.
        prim(
            solid(cylinder_tapered(0.6, 0.4, 8, 0.2, stone(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // A tall central shard.
    prims.push(prim(
        cone(0.22, 1.6, 6, glow(CRYSTAL_CYAN, 3.0)),
        [0.0, 1.0, 0.0],
        id_quat(),
    ));
    // Leaning side shards tilted around X.
    for (cx, cz, h, tilt) in [
        (0.35_f32, 0.1_f32, 1.0_f32, 0.4_f32),
        (-0.3, 0.2, 0.8, -0.4),
        (0.05, -0.35, 0.9, 0.3),
    ] {
        prims.push(prim(
            cone(0.16, h, 6, glow(CRYSTAL_CYAN, 2.6)),
            [cx, 0.4 + h * 0.4, cz],
            quat_x(tilt),
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
        assert_sanitize_stable(&CrystalCluster.build(""), "crystal_cluster");
    }
}
