//! Tinkerer's shack — the Steampunk *poor* landmark. A patched corrugated-
//! and-plank hut bristling with mismatched pipes, a crooked stovepipe
//! belching smoke and a single grimy lit window. The hardscrabble
//! counterpart to the [`cog_tower`](super::cog_tower): same works, opposite
//! end of the prosperity axis (`Poor`), so a destitute steampunk room grows
//! the soot-yard instead of the running concern.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the shack floor.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, quat_z, solid, torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COPPER_ORANGE, CORRUGATED_RUST, IRON_DARK, WOOD_BROWN, cog, copper, corrugated, fx, iron, plank,
};

pub struct TinkerersShack;

impl CatalogueEntry for TinkerersShack {
    fn slug(&self) -> &'static str {
        "tinkerers_shack"
    }
    fn name(&self) -> &'static str {
        "Tinkerer's Shack"
    }
    fn description(&self) -> &'static str {
        "Patched corrugated hut bristling with pipes, a crooked smoking stovepipe."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.8_f32;
    let wall_top = wall_h;
    // Hero (−Z) front wall face sits at z = -2.0; detail rides proud of it.
    let front = -2.05_f32;

    let mut prims = vec![
        // Corrugated walls — the root, sitting on the ground.
        prim(
            solid(cuboid_tapered(
                [5.0, wall_h, 4.0],
                0.0,
                corrugated(CORRUGATED_RUST),
            )),
            [0.0, wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Patched plank panel + door on the −Z front.
    prims.push(prim(
        solid(cuboid_tapered([2.0, 1.8, 0.12], 0.0, plank(WOOD_BROWN))),
        [-1.1, 1.0, front],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.9, 0.15], 0.0, plank(WOOD_BROWN))),
        [-1.1, 0.95, front - 0.04],
        id_quat(),
    ));
    // Grimy lit window in a dark iron frame so the dim amber reads.
    prims.push(prim(
        solid(cuboid_tapered([1.15, 1.05, 0.1], 0.0, iron(IRON_DARK))),
        [1.2, 1.5, front],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.92, 0.82, 0.12], 0.0, glow([1.0, 0.55, 0.18], 2.4)),
        [1.2, 1.5, front - 0.05],
        id_quat(),
    ));
    // A salvaged cog bolted high on the wall, clear of the window — the
    // tinkerer's mark.
    prims.push(cog(
        [2.0, 2.5, front - 0.06],
        quat_x(-FRAC_PI_2),
        0.38,
        0.14,
        11,
        iron(IRON_DARK),
        copper(COPPER_ORANGE),
    ));

    // Sagging corrugated roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.4, 0.3, 4.4],
            0.1,
            corrugated(CORRUGATED_RUST),
        )),
        [0.0, wall_top + 0.15, 0.0],
        quat_x(0.07),
    ));

    // Crooked hollow stovepipe with a cowl, belching smoke.
    let pipe_x = 1.5_f32;
    prims.push(prim(
        solid(tube(0.18, 0.11, 2.2, 8, iron(IRON_DARK))),
        [pipe_x, wall_top + 1.0, -1.0],
        quat_x(0.18),
    ));
    prims.push(prim(
        solid(torus(0.06, 0.24, iron(IRON_DARK))),
        [pipe_x + 0.2, wall_top + 2.05, -0.8],
        quat_x(0.18),
    ));

    // Mismatched hollow pipes bristling from the walls at jaunty angles —
    // several on the visible −Z and +X faces so they read.
    prims.push(prim(
        solid(tube(0.18, 0.11, 2.6, 8, copper(COPPER_ORANGE))),
        [2.74, 1.5, 0.6],
        quat_z(-0.24),
    ));
    prims.push(prim(
        solid(tube(0.15, 0.09, 2.2, 8, iron(IRON_DARK))),
        [-2.3, 1.4, front + 0.4],
        quat_z(0.3),
    ));
    prims.push(prim(
        solid(tube(0.14, 0.08, 1.7, 8, copper(COPPER_ORANGE))),
        [2.7, 1.4, -0.9],
        quat_x(0.28),
    ));
    // A pipe running across the front above the door, with an elbow down.
    prims.push(prim(
        solid(tube(0.13, 0.08, 2.2, 8, copper(COPPER_ORANGE))),
        [-0.9, 2.4, front + 0.12],
        quat_z(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(tube(0.13, 0.08, 0.8, 8, copper(COPPER_ORANGE))),
        [0.2, 2.1, front + 0.12],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: smoke seeping from the stovepipe.
    root.children.push(fx::furnace_smoke(
        [pipe_x, wall_top + 2.3, -1.0],
        0x500F_5AC4,
    ));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TinkerersShack.build(""), "tinkerers_shack");
    }
}
