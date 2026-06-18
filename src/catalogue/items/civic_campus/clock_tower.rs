//! Clock tower — a Civic/Campus secondary. A tall brick campanile on a
//! stone base, a lit clock face on each of its four sides and a verdigris
//! copper pyramid roof with a finial. A soft mechanism hum lingers in the
//! belfry. Its clock faces are emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CLOCK_LIT, COPPER_VERDIGRIS, STONE_PALE, brick, copper, fx, stone};

pub struct ClockTower;

impl CatalogueEntry for ClockTower {
    fn slug(&self) -> &'static str {
        "clock_tower"
    }
    fn name(&self) -> &'static str {
        "Clock Tower"
    }
    fn description(&self) -> &'static str {
        "Brick campanile with a lit clock face on each side and a copper pyramid roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let shaft_h = 12.0_f32;
    let shaft_top = base_h + shaft_h;
    let clock_y = shaft_top - 1.4;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([3.6, base_h, 3.6], 0.0, stone(STONE_PALE))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Brick shaft.
    prims.push(prim(
        solid(cuboid_tapered([2.6, shaft_h, 2.6], 0.02, brick(BRICK_RED))),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));
    // Stone belfry band near the top.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 1.6, 3.0], 0.0, stone(STONE_PALE))),
        [0.0, clock_y, 0.0],
        id_quat(),
    ));

    // Lit clock faces on all four sides — emissive trim.
    prims.push(prim(
        cuboid_tapered([1.4, 1.4, 0.12], 0.0, glow(CLOCK_LIT, 3.0)),
        [0.0, clock_y, 1.45],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.4, 1.4, 0.12], 0.0, glow(CLOCK_LIT, 3.0)),
        [0.0, clock_y, -1.45],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.12, 1.4, 1.4], 0.0, glow(CLOCK_LIT, 3.0)),
        [1.45, clock_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.12, 1.4, 1.4], 0.0, glow(CLOCK_LIT, 3.0)),
        [-1.45, clock_y, 0.0],
        id_quat(),
    ));

    // Copper pyramid roof + finial.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.4, 2.6, 3.4],
            0.85,
            copper(COPPER_VERDIGRIS),
        )),
        [0.0, shaft_top + 1.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.22, 3, copper(COPPER_VERDIGRIS)),
        [0.0, shaft_top + 2.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the belfry's soft resonant hum.
    root.audio = fx::tower_resonance();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ClockTower.build(""), "clock_tower");
    }

    #[test]
    fn has_lit_clock() {
        assert!(super::super::has_emissive(&ClockTower.build("")));
    }
}
