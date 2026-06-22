//! Bone pile — a Gothic-Horror *poor* prop. A grim heap of bones and skulls
//! mouldering in the earth. The charnel clutter of the forsaken ground.
//!
//! Scattered long-bones lie tipped with a [`quat_x`].

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE, matte};

/// A grim little skull facing the -Z front: a bone cranium, a muzzle, a brow
/// ridge and dark eye sockets / nasal cavity set into the face. `tip` leans it.
fn skull(c: [f32; 3], tip: f32) -> Vec<Generator> {
    let [x, y, z] = c;
    let b = || matte(BONE);
    let dark = || matte([0.1, 0.09, 0.08]);
    vec![
        prim(solid(sphere(0.15, 6, b())), [x, y, z], quat_x(tip)),
        prim(
            solid(cuboid_tapered([0.17, 0.1, 0.16], 0.2, b())),
            [x, y - 0.13, z - 0.06],
            quat_x(tip),
        ),
        prim(
            solid(cuboid_tapered([0.21, 0.04, 0.05], 0.0, b())),
            [x, y + 0.05, z - 0.12],
            quat_x(tip),
        ),
        prim(
            solid(sphere(0.05, 6, dark())),
            [x - 0.07, y, z - 0.12],
            id_quat(),
        ),
        prim(
            solid(sphere(0.05, 6, dark())),
            [x + 0.07, y, z - 0.12],
            id_quat(),
        ),
        prim(
            solid(sphere(0.035, 6, dark())),
            [x, y - 0.06, z - 0.14],
            id_quat(),
        ),
    ]
}

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
    let b = || matte(BONE);
    let mut prims = vec![
        // Earthen heap base — the root.
        prim(
            solid(cuboid_tapered(
                [1.3, 0.42, 1.1],
                0.5,
                matte([0.30, 0.26, 0.22]),
            )),
            [0.0, 0.21, 0.0],
            id_quat(),
        ),
    ];

    // Skulls heaped on top, glaring out front.
    prims.extend(skull([0.0, 0.55, -0.05], 0.1));
    prims.extend(skull([0.34, 0.42, 0.22], -0.3));

    // A half-buried ribcage arcing out of the heap.
    let rz0 = -0.12_f32;
    for i in 0..4 {
        prims.push(prim(
            solid(with_cut(
                torus(0.02, 0.16, b()),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [-0.4, 0.36, rz0 + i as f32 * 0.13],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Spine ridging the ribs.
    prims.push(prim(
        solid(cylinder_tapered(0.03, 0.55, 6, 0.0, b())),
        [-0.4, 0.5, rz0 + 0.2],
        quat_x(FRAC_PI_2),
    ));

    // Knobby long-bones (femurs) scattered across the heap.
    // One laid along X.
    prims.push(prim(
        solid(cylinder_tapered(0.045, 0.6, 6, 0.0, b())),
        [0.15, 0.34, 0.35],
        quat_z(FRAC_PI_2),
    ));
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(sphere(0.06, 6, b())),
            [0.15 + s * 0.3, 0.34, 0.35],
            id_quat(),
        ));
    }
    // One laid along Z.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 0.5, 6, 0.0, b())),
        [0.45, 0.3, -0.25],
        quat_x(FRAC_PI_2),
    ));
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(sphere(0.055, 6, b())),
            [0.45, 0.3, -0.25 + s * 0.25],
            id_quat(),
        ));
    }

    // A pelvis bone half-sunk at the foot of the heap.
    prims.push(prim(
        solid(torus(0.05, 0.16, b())),
        [-0.1, 0.16, 0.42],
        quat_x(1.2),
    ));

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
