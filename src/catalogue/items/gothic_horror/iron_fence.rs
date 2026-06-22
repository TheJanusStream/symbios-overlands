//! Iron fence — a Gothic-Horror prop. A section of black wrought-iron railing
//! with spear-tip finials between two posts. Scatter clutter bounding the
//! necropolis.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, iron, pointed_arch};

pub struct IronFence;

impl CatalogueEntry for IronFence {
    fn slug(&self) -> &'static str {
        "iron_fence"
    }
    fn name(&self) -> &'static str {
        "Iron Fence"
    }
    fn description(&self) -> &'static str {
        "Section of black wrought-iron railing with spear-tip finials between two posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ir = || iron(IRON_BLACK);
    let mut prims = vec![
        // Top rail — the root.
        prim(
            solid(cuboid_tapered([3.6, 0.08, 0.08], 0.0, ir())),
            [0.0, 1.25, 0.0],
            id_quat(),
        ),
    ];
    // Mid + bottom rails.
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.07, 0.07], 0.0, ir())),
        [0.0, 0.78, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.08, 0.08], 0.0, ir())),
        [0.0, 0.3, 0.0],
        id_quat(),
    ));

    // Ornate end posts: stepped base, shaft, cap, urn-and-spear finial.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.24, 0.2, 0.24], 0.0, ir())),
            [s * 1.8, 0.1, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.16, 1.6, 0.16], 0.0, ir())),
            [s * 1.8, 0.9, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.22, 0.14, 0.22], 0.0, ir())),
            [s * 1.8, 1.74, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(sphere(0.1, 6, ir())),
            [s * 1.8, 1.88, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.08, 0.34, 6, ir())),
            [s * 1.8, 2.1, 0.0],
            id_quat(),
        ));
    }

    // Vertical bars with spear-tip finials.
    for i in 0..7 {
        let x = -1.5 + i as f32 * 0.5;
        prims.push(prim(
            solid(cylinder_tapered(0.035, 1.5, 6, 0.0, ir())),
            [x, 0.6, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.06, 0.24, 6, ir())),
            [x, 1.46, 0.0],
            id_quat(),
        ));
    }

    // Gothic tracery: small pointed arches across the upper panel.
    for i in 0..3 {
        let cx = -1.0 + i as f32 * 1.0;
        prims.extend(pointed_arch([cx, 0.85, 0.0], 0.25, 0.028, ir()));
    }
    // Wrought scroll rings on the lower panel.
    for i in 0..3 {
        let cx = -1.0 + i as f32 * 1.0;
        prims.push(prim(
            solid(torus(0.03, 0.14, ir())),
            [cx, 0.54, 0.0],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&IronFence.build(""), "iron_fence");
    }
}
