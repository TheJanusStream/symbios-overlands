//! Church — a Wild-West secondary. A white clapboard chapel with a steepled
//! bell tower, a cross and lit arched windows. The frontier town's chapel;
//! its windows are emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_WHITE, GLASS_WARM, TIN_GREY, WOOD_RAW, clapboard, glass, tin};

pub struct Church;

impl CatalogueEntry for Church {
    fn slug(&self) -> &'static str {
        "church"
    }
    fn name(&self) -> &'static str {
        "Church"
    }
    fn description(&self) -> &'static str {
        "White clapboard chapel with a steepled bell tower, a cross and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_h = 4.5_f32;
    let body_d = 8.0_f32;
    let body_top = slab_h + body_h;
    let front_z = body_d * 0.5;

    let mut prims = vec![
        // Clapboard slab — the root.
        prim(
            solid(cuboid_tapered([7.0, slab_h, 9.0], 0.0, clapboard(WOOD_RAW))),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // White clapboard nave.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.5, body_h, body_d],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Pitched tin roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [6.0, 1.8, body_d + 0.4],
            0.55,
            tin(TIN_GREY),
        )),
        [0.0, body_top + 0.9, 0.0],
        id_quat(),
    ));

    // Lit arched windows down both sides.
    for sx in [-1.0_f32, 1.0] {
        for z in [-2.0_f32, 0.0, 2.0] {
            prims.push(prim(
                cuboid_tapered([0.15, 2.0, 0.7], 0.0, glass(GLASS_WARM, 1.4)),
                [sx * (5.5 * 0.5 + 0.02), slab_h + 2.0, z],
                id_quat(),
            ));
        }
    }
    // Double doors at the front.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 2.4, 0.2], 0.0, clapboard(WOOD_RAW))),
        [0.0, slab_h + 1.2, front_z + 0.02],
        id_quat(),
    ));

    // Bell tower over the entrance.
    prims.push(prim(
        solid(cuboid_tapered([2.0, 7.0, 2.0], 0.0, clapboard(CLAP_WHITE))),
        [0.0, slab_h + 3.5, front_z - 0.4],
        id_quat(),
    ));
    // Spire + cross.
    prims.push(prim(
        solid(cone(1.4, 2.6, 8, tin(TIN_GREY))),
        [0.0, slab_h + 8.3, front_z - 0.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.16, 1.0, 0.16],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + 10.2, front_z - 0.4],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 0.16, 0.16],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + 10.3, front_z - 0.4],
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
        assert_sanitize_stable(&Church.build(""), "church");
    }
}
