//! Bathhouse — an AncientClassical secondary. A small Roman bath: a
//! sandstone block with a barrel-vaulted lead roof, an arched marble
//! entrance flanked by columns, and a still open-air plunge pool in front.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus, with_cut,
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
        // Barrel-vaulted marble roof: a path-cut half-cylinder springing
        // from the wall tops (curved side up), its axis running front-to-back
        // so the entrance reads under a vaulted gable. `path_cut [0.5,1.0]`
        // keeps the local −Z semicircle, which the +90° X-rotation swings to
        // point up; the cut's fan caps give it solid semicircular gable ends.
        // Sunk a touch so the springing line buries into the wall instead of
        // leaving the vault's flat soffit coplanar with the wall top.
        prim(
            solid(with_cut(
                cylinder_tapered(w * 0.55, l + 0.4, 28, 0.0, marble(MARBLE_WHITE)),
                [0.5, 1.0],
                [0.0, 1.0],
                0.0,
            )),
            [0.0, wall_top - 0.25, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Arched marble entrance: a dark recessed doorway under a path-cut torus
    // arch that springs from two flanking columns, crowned by a keystone.
    let col_h = 2.8_f32;
    let col_top = foot_h + col_h;
    let arch_major = 1.25_f32;
    // Dark doorway recess.
    prims.push(prim(
        cuboid_tapered([2.2, col_h, 0.4], 0.0, marble(STONE_VOID)),
        [0.0, foot_h + col_h * 0.5, front - 0.1],
        id_quat(),
    ));
    // Marble arch over the doorway — the top half of a torus standing in the
    // XY plane (`quat_x(-FRAC_PI_2)` lays the local +Z meridian up, `path_cut
    // [0,0.5]` keeps the upper semicircle).
    prims.push(prim(
        with_cut(
            torus(0.24, arch_major, marble(MARBLE_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, col_top, front + 0.05],
        quat_x(-FRAC_PI_2),
    ));
    // Keystone at the arch crown.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 0.5, 0.5], 0.0, marble(MARBLE_WHITE))),
        [0.0, col_top + arch_major, front + 0.05],
        id_quat(),
    ));
    // Two flanking columns the arch springs from.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.28, col_h, 16, 0.1, marble(MARBLE_WHITE))),
            [sx * arch_major, foot_h + col_h * 0.5, front + 0.15],
            id_quat(),
        ));
    }

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
