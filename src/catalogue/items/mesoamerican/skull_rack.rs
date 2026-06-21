//! Skull rack — a Mesoamerican prop. A tzompantli: a timber frame strung
//! with rows of skulls threaded on horizontal poles, displayed at the edge
//! of the sacred precinct.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BONE_WHITE, LIMESTONE_PALE, STONE_GREY, STUCCO_RED, TIMBER_BROWN, cobble, limestone, painted,
    timber,
};

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
    let span = 3.4_f32;
    let post_h = 2.5;
    let plat_h = 0.5_f32;

    // Stone platform — the root, giving the rack its precinct weight.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [span + 1.4, plat_h, 1.6],
            0.05,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, plat_h * 0.5, 0.0],
        id_quat(),
    )];
    // Red moulding course straddling the platform head.
    prims.push(prim(
        solid(cuboid_tapered(
            [span + 1.5, 0.14, 1.72],
            0.0,
            painted(STUCCO_RED),
        )),
        [0.0, plat_h, 0.0],
        id_quat(),
    ));

    // Two stout posts on cobble footings, each crowned by an impaled skull.
    for sx in [-1.0_f32, 1.0] {
        let px = sx * span * 0.5;
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.4, 0.5], 0.1, cobble(STONE_GREY))),
            [px, plat_h + 0.2, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(
                0.16,
                post_h,
                8,
                0.06,
                timber(TIMBER_BROWN),
            )),
            [px, plat_h + 0.4 + post_h * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.2, 4, painted(BONE_WHITE)),
            [px, plat_h + 0.4 + post_h + 0.12, 0.0],
            id_quat(),
        ));
    }
    // Top lintel beam joining the post heads.
    let top_y = plat_h + 0.4 + post_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [span + 0.3, 0.18, 0.22],
            0.0,
            timber(TIMBER_BROWN),
        )),
        [0.0, top_y, 0.0],
        id_quat(),
    ));

    // Three stringing poles, each threaded with a row of skulls pierced
    // ear-to-ear by the pole, two dark eye sockets carved into the front face.
    let rows = [top_y - 0.55, top_y - 1.15, top_y - 1.75];
    for ry in rows {
        prims.push(prim(
            solid(cuboid_tapered(
                [span, 0.08, 0.08],
                0.0,
                timber(TIMBER_BROWN),
            )),
            [0.0, ry, 0.0],
            id_quat(),
        ));
        let skulls = 6;
        for k in 0..skulls {
            let x = -span * 0.5 + 0.35 + k as f32 * (span - 0.7) / (skulls - 1) as f32;
            prims.push(prim(
                sphere(0.18, 4, painted(BONE_WHITE)),
                [x, ry, 0.0],
                id_quat(),
            ));
            for ex in [-1.0_f32, 1.0] {
                prims.push(prim(
                    sphere(0.05, 3, painted([0.08, 0.06, 0.05])),
                    [x + ex * 0.06, ry + 0.03, -0.15],
                    id_quat(),
                ));
            }
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
