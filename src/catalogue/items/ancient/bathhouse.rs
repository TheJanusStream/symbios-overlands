//! Bathhouse — an AncientClassical secondary. A small Roman bath: a
//! sandstone block with a barrel-vaulted lead roof, an arched marble
//! entrance flanked by columns, and a still open-air plunge pool in front.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_GOLD, STONE_VOID, marble, sandstone};

pub struct Bathhouse;

impl CatalogueEntry for Bathhouse {
    fn slug(&self) -> &'static str {
        "bathhouse"
    }
    fn name(&self) -> &'static str {
        "Bathhouse"
    }
    fn description(&self) -> &'static str {
        "Sandstone bath with a barrel-vault roof, columned marble entrance, and a plunge pool."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.5,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Still pool water, faintly reflective turquoise.
fn pool_water() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.12, 0.32, 0.34]),
        roughness: Fp(0.08),
        metallic: Fp(0.3),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let l = 6.0_f32; // along X
    let w = 5.0_f32; // along Z, entrance faces +Z
    let foot_h = 0.4;
    let wall_h = 3.4;
    let wall_top = foot_h + wall_h;
    let front = w * 0.5;

    let mut prims = vec![
        // Sandstone footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.8, foot_h, w + 0.8],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Bath block body.
        prim(
            solid(cuboid_tapered(
                [l, wall_h, w],
                0.0,
                sandstone(SANDSTONE_GOLD),
            )),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
        // Barrel-vaulted lead roof: a half-cylinder along X.
        prim(
            solid(cylinder_tapered(
                w * 0.55,
                l + 0.4,
                16,
                0.0,
                marble(MARBLE_WHITE),
            )),
            [0.0, wall_top, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Arched marble entrance recess on the front, flanked by two columns.
    prims.push(prim(
        cuboid_tapered([2.0, 2.6, 0.3], 0.0, marble(STONE_VOID)),
        [0.0, foot_h + 1.3, front + 0.02],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.3, 3.0, 16, 0.1, marble(MARBLE_WHITE))),
            [sx * 1.5, foot_h + 1.5, front + 0.2],
            id_quat(),
        ));
    }
    // Lintel over the entrance columns.
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.4, 0.7], 0.0, marble(MARBLE_WHITE))),
        [0.0, foot_h + 3.1, front + 0.2],
        id_quat(),
    ));

    // Open-air plunge pool in front: a sunken marble kerb around dark water.
    let pool_z = front + 2.4;
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.4, 2.4], 0.0, marble(MARBLE_WHITE))),
        [0.0, 0.2, pool_z],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.0, 0.12, 1.8], 0.0, pool_water()),
        [0.0, 0.34, pool_z],
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
        assert_sanitize_stable(&Bathhouse.build(""), "bathhouse");
    }
}
