//! Skull rack — a Mesoamerican prop. A tzompantli: a timber frame strung
//! with rows of skulls threaded on horizontal poles, displayed at the edge
//! of the sacred precinct.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE_WHITE, TIMBER_BROWN, painted, timber};

pub struct SkullRack;

impl CatalogueEntry for SkullRack {
    fn slug(&self) -> &'static str {
        "skull_rack"
    }
    fn name(&self) -> &'static str {
        "Skull Rack"
    }
    fn description(&self) -> &'static str {
        "Timber tzompantli frame strung with rows of skulls."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 3.0_f32;
    let post_h = 2.4;

    // Ground sill — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [span + 0.4, 0.2, 0.4],
            0.0,
            timber(TIMBER_BROWN),
        )),
        [0.0, 0.1, 0.0],
        id_quat(),
    )];
    // Two posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.13,
                post_h,
                8,
                0.05,
                timber(TIMBER_BROWN),
            )),
            [sx * span * 0.5, post_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Horizontal stringing poles with rows of threaded skulls.
    let rows = [1.9_f32, 1.35, 0.8];
    for ry in rows {
        prims.push(prim(
            solid(cuboid_tapered(
                [span, 0.09, 0.09],
                0.0,
                timber(TIMBER_BROWN),
            )),
            [0.0, ry, 0.0],
            id_quat(),
        ));
        let skulls = 7;
        for k in 0..skulls {
            let x = -span * 0.5 + 0.25 + k as f32 * (span - 0.5) / (skulls - 1) as f32;
            prims.push(prim(
                sphere(0.16, 3, painted(BONE_WHITE)),
                [x, ry - 0.16, 0.0],
                id_quat(),
            ));
        }
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SkullRack.build(""), "skull_rack");
    }
}
