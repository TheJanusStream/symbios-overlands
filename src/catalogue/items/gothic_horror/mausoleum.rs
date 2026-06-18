//! Mausoleum — a Gothic-Horror secondary. A columned stone tomb under a
//! pediment, an iron gate barring its door and a small lit window above. The
//! family crypt of the necropolis; its window is emissive trim the ruin pass
//! can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STAINED_TINT, STONE_DARK, fx, iron, stained, stone};

pub struct Mausoleum;

impl CatalogueEntry for Mausoleum {
    fn slug(&self) -> &'static str {
        "mausoleum"
    }
    fn name(&self) -> &'static str {
        "Mausoleum"
    }
    fn description(&self) -> &'static str {
        "Columned stone tomb under a pediment with an iron gate and a small lit window."
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
            clearance: 5.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let body_h = 3.4_f32;
    let body_top = base_h + body_h;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([6.0, base_h, 5.0], 0.0, stone(STONE_DARK))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Tomb body.
    prims.push(prim(
        solid(cuboid_tapered([4.5, body_h, 4.0], 0.0, stone(STONE_DARK))),
        [0.0, base_h + body_h * 0.5, -0.3],
        id_quat(),
    ));
    // Iron gate over the doorway.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 2.6, 0.2], 0.0, iron(IRON_BLACK))),
        [0.0, base_h + 1.3, 1.75],
        id_quat(),
    ));
    // Small lit window above the gate — emissive.
    prims.push(prim(
        cuboid_tapered([0.9, 0.9, 0.15], 0.0, stained(STAINED_TINT, 1.8)),
        [0.0, base_h + 2.9, 1.75],
        id_quat(),
    ));

    // Two front columns.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.3, body_h, 10, 0.04, stone(STONE_DARK))),
            [sx * 1.7, base_h + body_h * 0.5, 1.9],
            id_quat(),
        ));
    }
    // Pediment over the columns.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 1.4, 1.2], 0.85, stone(STONE_DARK))),
        [0.0, body_top + 0.7, 1.4],
        id_quat(),
    ));

    // Stone roof slab + urn finial.
    prims.push(prim(
        solid(cuboid_tapered([4.8, 0.4, 4.2], 0.0, stone(STONE_DARK))),
        [0.0, body_top + 0.2, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.5, 1.0, 8, stone(STONE_DARK))),
        [0.0, body_top + 0.9, -0.3],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: mist creeping around the tomb.
    root.children
        .push(fx::ground_mist([0.0, 0.3, 3.0], 0x60F0_3A12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Mausoleum.build(""), "mausoleum");
    }
}
