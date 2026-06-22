//! Crystal cluster — a High-Fantasy prop. A knot of glowing crystal shards
//! jutting from a rocky base at wild angles. Scatter clutter of the arcane
//! quarter; the shards are emissive trim the ruin pass can darken.
//!
//! The leaning shards are cones tilted with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CRYSTAL_CYAN, STONE_GREY, crystal, stone};

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
        // Rocky base — the root, a low dark mound.
        prim(
            solid(cylinder_tapered(0.62, 0.4, 7, 0.25, stone(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];
    // A couple of boulders breaking up the base silhouette.
    for (bx, bz, br) in [(0.34_f32, -0.18_f32, 0.26_f32), (-0.32, 0.12, 0.22)] {
        prims.push(prim(
            solid(sphere(br, 5, stone(STONE_GREY))),
            [bx, 0.22, bz],
            id_quat(),
        ));
    }

    // A tall faceted central shard.
    prims.push(crystal(
        [0.0, 0.36, 0.0],
        0.24,
        1.9,
        id_quat(),
        glow(CRYSTAL_CYAN, 1.8),
    ));
    // Leaning faceted side shards splaying out at wild angles.
    for (cx, cz, h, tilt, axis_z) in [
        (0.36_f32, 0.1_f32, 1.15_f32, 0.42_f32, false),
        (-0.32, 0.2, 0.92, -0.46, false),
        (0.08, -0.36, 1.0, 0.34, true),
        (0.28, 0.34, 0.7, -0.3, true),
    ] {
        let lean = if axis_z { quat_x(tilt) } else { quat_z(tilt) };
        prims.push(crystal(
            [cx, 0.36, cz],
            0.15,
            h,
            lean,
            glow(CRYSTAL_CYAN, 1.6),
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
