//! Church — a Wild-West secondary. A white clapboard chapel with a steepled
//! bell tower, a cross and lit arched windows. The frontier town's chapel;
//! its windows are emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, prim,
    quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, glass, iron, tin};

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
    let body_w = 5.5_f32;
    let body_h = 4.5_f32;
    let body_d = 8.0_f32;
    let body_top = slab_h + body_h;
    // Render FRONT = −Z — the tower entrance and oculus face −Z.
    let front_z = -body_d * 0.5;

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
            [body_w, body_h, body_d],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + body_h * 0.5, 0.0],
        id_quat(),
    ));
    // Pitched tin gable roof — ridge running along X, gables facing ±Z.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [body_w + 0.5, 2.0, body_d + 0.4],
            [0.0, 0.92],
            tin(TIN_GREY),
        )),
        [0.0, body_top + 1.0, 0.0],
        id_quat(),
    ));

    // Lit lancet windows down both nave sides, each in a white surround.
    for sx in [-1.0_f32, 1.0] {
        for z in [-2.0_f32, 0.0, 2.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, 2.3, 0.95], 0.0, clapboard(CLAP_WHITE))),
                [sx * (body_w * 0.5 + 0.01), slab_h + 2.0, z],
                id_quat(),
            ));
            prims.push(prim(
                cuboid_tapered([0.14, 2.0, 0.7], 0.0, glass(GLASS_WARM, 1.8)),
                [sx * (body_w * 0.5 + 0.05), slab_h + 2.0, z],
                id_quat(),
            ));
        }
    }

    // Square bell tower projecting from the front, over the entrance.
    let tower_z = front_z - 0.5;
    let tower_face = tower_z - 1.2;
    let tower_h = 8.5_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, tower_h, 2.4],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + tower_h * 0.5, tower_z],
        id_quat(),
    ));
    // Double doors under a rounded arch.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 2.6, 0.2], 0.0, clapboard(WOOD_RAW))),
        [0.0, slab_h + 1.3, tower_face + 0.02],
        id_quat(),
    ));
    prims.push(prim(
        with_cut(
            torus(0.16, 0.78, clapboard(CLAP_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, slab_h + 2.6, tower_face - 0.04],
        quat_x(-FRAC_PI_2),
    ));
    // Oculus (rose window) above the door: a lit disc in a white ring.
    prims.push(prim(
        torus(0.1, 0.62, clapboard(CLAP_WHITE)),
        [0.0, slab_h + 4.3, tower_face - 0.02],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.52, 0.14, 16, 0.0, glass(GLASS_WARM, 2.0)),
        [0.0, slab_h + 4.3, tower_face - 0.02],
        quat_x(FRAC_PI_2),
    ));
    // Belfry: dark louvered openings and a hanging bell near the top.
    for ly in [slab_h + 6.0, slab_h + 6.35, slab_h + 6.7] {
        prims.push(prim(
            solid(cuboid_tapered(
                [1.4, 0.12, 0.06],
                0.0,
                clapboard([0.2, 0.18, 0.15]),
            )),
            [0.0, ly, tower_face + 0.02],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cone(0.32, 0.5, 12, iron(IRON_DARK))),
        [0.0, slab_h + 6.5, tower_z],
        id_quat(),
    ));

    // Tall sharp spire + cross over the belfry.
    prims.push(prim(
        solid(cone(1.35, 3.6, 12, tin(TIN_GREY))),
        [0.0, slab_h + tower_h + 1.8, tower_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.16, 1.0, 0.16],
            0.0,
            glow([0.95, 0.82, 0.5], 1.4),
        )),
        [0.0, slab_h + tower_h + 4.1, tower_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 0.16, 0.16],
            0.0,
            glow([0.95, 0.82, 0.5], 1.4),
        )),
        [0.0, slab_h + tower_h + 4.2, tower_z],
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
