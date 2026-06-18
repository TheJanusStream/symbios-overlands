//! Bone pile — a Gothic-Horror *poor* prop. A grim heap of bones and skulls
//! mouldering in the earth. The charnel clutter of the forsaken ground.
//!
//! Scattered long-bones lie tipped with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE, matte};

pub struct BonePile;

impl CatalogueEntry for BonePile {
    fn slug(&self) -> &'static str {
        "bone_pile"
    }
    fn name(&self) -> &'static str {
        "Bone Pile"
    }
    fn description(&self) -> &'static str {
        "Grim heap of bones and skulls mouldering in the earth."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_POOR
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
        // Earthen heap base — the root.
        prim(
            solid(cuboid_tapered(
                [1.2, 0.4, 1.0],
                0.5,
                matte([0.30, 0.26, 0.22]),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Skulls piled on the heap.
    for (sx, sy, sz) in [
        (0.0_f32, 0.5_f32, 0.0_f32),
        (0.3, 0.35, 0.25),
        (-0.3, 0.32, -0.2),
    ] {
        prims.push(prim(
            solid(sphere(0.16, 3, matte(BONE))),
            [sx, sy, sz],
            id_quat(),
        ));
    }

    // Scattered long-bones lying across the heap.
    for (bx, bz, tilt) in [
        (0.4_f32, -0.3_f32, 1.4_f32),
        (-0.4, 0.3, 1.2),
        (0.1, 0.4, 1.5),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(0.05, 0.7, 6, 0.0, matte(BONE))),
            [bx, 0.32, bz],
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
        assert_sanitize_stable(&BonePile.build(""), "bone_pile");
    }
}
