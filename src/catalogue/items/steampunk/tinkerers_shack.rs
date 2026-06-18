//! Tinkerer's shack — the Steampunk *poor* landmark. A patched corrugated-
//! and-plank hut bristling with mismatched pipes, a crooked stovepipe
//! belching smoke and a single grimy lit window. The hardscrabble
//! counterpart to the [`cog_tower`](super::cog_tower): same works, opposite
//! end of the prosperity axis (`Poor`), so a destitute steampunk room grows
//! the soot-yard instead of the running concern.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the shack floor.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COPPER_ORANGE, CORRUGATED_RUST, GLASS_AMBER, IRON_DARK, WOOD_BROWN, copper, corrugated, fx,
    glass, iron, plank,
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

    // Patched plank panel on the front.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 1.8, 0.12], 0.0, plank(WOOD_BROWN))),
        [-1.0, 1.0, 2.05],
        id_quat(),
    ));
    // Grimy lit window.
    prims.push(prim(
        cuboid_tapered([1.0, 0.9, 0.12], 0.0, glass(GLASS_AMBER, 0.9)),
        [1.3, 1.6, 2.05],
        id_quat(),
    ));
    // Plank door.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.9, 0.15], 0.0, plank(WOOD_BROWN))),
        [-1.0, 0.95, 2.08],
        id_quat(),
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

    // Crooked stovepipe belching smoke.
    let pipe_x = 1.5_f32;
    prims.push(prim(
        solid(cylinder_tapered(0.18, 2.2, 8, 0.0, iron(IRON_DARK))),
        [pipe_x, wall_top + 1.0, -1.0],
        quat_x(0.18),
    ));

    // A couple of mismatched pipes bolted to the wall.
    prims.push(prim(
        solid(cylinder_tapered(0.14, 2.4, 8, 0.0, copper(COPPER_ORANGE))),
        [-2.3, 1.4, 1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: smoke seeping from the stovepipe.
    root.children.push(fx::furnace_smoke(
        [pipe_x, wall_top + 2.2, -1.0],
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
