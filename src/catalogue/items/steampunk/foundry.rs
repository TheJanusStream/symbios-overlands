//! Foundry — a Steampunk secondary. A sooty brick hall with two banded brick
//! chimneys belching smoke, copper pipes up the wall and a glowing furnace
//! door. The roaring heart of the works.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the hall.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, FURNACE_ORANGE, IRON_DARK, brass, brick, copper, fx, iron,
};

pub struct Foundry;

impl CatalogueEntry for Foundry {
    fn slug(&self) -> &'static str {
        "foundry"
    }
    fn name(&self) -> &'static str {
        "Foundry"
    }
    fn description(&self) -> &'static str {
        "Sooty brick hall with smoking chimneys, copper pipes and a glowing furnace door."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let hall_h = 5.0_f32;

    let mut prims = vec![
        // Sooty brick hall — the root.
        prim(
            solid(cuboid_tapered([12.0, hall_h, 8.0], 0.0, brick(BRICK_SOOT))),
            [0.0, hall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Iron roof cap.
    prims.push(prim(
        solid(cuboid_tapered([12.4, 0.5, 8.4], 0.0, iron(IRON_DARK))),
        [0.0, hall_h + 0.25, 0.0],
        id_quat(),
    ));

    // Glowing furnace door on the +Z face — emissive.
    prims.push(prim(
        cuboid_tapered([2.4, 2.6, 0.2], 0.0, glow(FURNACE_ORANGE, 3.0)),
        [0.0, 1.4, 4.05],
        id_quat(),
    ));
    // Iron lintel over the door.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.5, 0.4], 0.0, iron(IRON_DARK))),
        [0.0, 2.9, 4.1],
        id_quat(),
    ));

    // Copper pipes climbing the wall.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.22,
                hall_h,
                8,
                0.0,
                copper(COPPER_ORANGE),
            )),
            [sx * 4.5, hall_h * 0.5, 4.1],
            id_quat(),
        ));
    }

    // Two banded brick chimneys rising from the roof.
    let mut smoky = None;
    for (i, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let cx = sx * 4.0;
        let chimney_h = 8.0;
        let top = hall_h + 0.5 + chimney_h;
        prims.push(prim(
            solid(cylinder_tapered(
                0.9,
                chimney_h,
                12,
                0.18,
                brick(BRICK_SOOT),
            )),
            [cx, hall_h + 0.5 + chimney_h * 0.5, -2.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.12, 0.78, brass(BRASS))),
            [cx, top - 0.6, -2.0],
            id_quat(),
        ));
        if i == 0 {
            smoky = Some([cx, top + 0.4, -2.0]);
        }
    }

    let mut root = assemble(prims);
    // Signature life: smoke off a chimney, the boiler hiss.
    root.audio = fx::boiler_hiss();
    if let Some(p) = smoky {
        root.children.push(fx::furnace_smoke(p, 0x500F_F0D1));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Foundry.build(""), "foundry");
    }
}
