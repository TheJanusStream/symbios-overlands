//! Jail — a Wild-West secondary. A squat fieldstone lock-up with iron-barred
//! windows, a heavy iron door and a flat tin roof. The marshal's lock-up of
//! the boomtown.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_WHITE, IRON_DARK, STONE_TAN, TIN_GREY, clapboard, iron, stone, tin};

pub struct Jail;

impl CatalogueEntry for Jail {
    fn slug(&self) -> &'static str {
        "jail"
    }
    fn name(&self) -> &'static str {
        "Jail"
    }
    fn description(&self) -> &'static str {
        "Squat fieldstone lock-up with iron-barred windows, a heavy door and a tin roof."
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
            clearance: 5.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body_h = 3.2_f32;
    let body_d = 5.0_f32;
    let front_z = body_d * 0.5;

    let mut prims = vec![
        // Fieldstone body — the root.
        prim(
            solid(cuboid_tapered([6.0, body_h, body_d], 0.0, stone(STONE_TAN))),
            [0.0, body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Flat tin roof with a parapet.
    prims.push(prim(
        solid(cuboid_tapered([6.4, 0.4, body_d + 0.4], 0.0, tin(TIN_GREY))),
        [0.0, body_h + 0.2, 0.0],
        id_quat(),
    ));

    // Iron-barred windows on the front.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.9, 0.9, 0.2],
                0.0,
                stone([0.2, 0.18, 0.16]),
            )),
            [sx * 1.8, body_h * 0.6, front_z + 0.02],
            id_quat(),
        ));
        for bx in [-0.25_f32, 0.0, 0.25] {
            prims.push(prim(
                solid(cuboid_tapered([0.06, 0.9, 0.08], 0.0, iron(IRON_DARK))),
                [sx * 1.8 + bx, body_h * 0.6, front_z + 0.12],
                id_quat(),
            ));
        }
    }
    // Heavy iron door.
    prims.push(prim(
        solid(cuboid_tapered([1.1, 2.2, 0.2], 0.0, iron(IRON_DARK))),
        [0.0, 1.1, front_z + 0.08],
        id_quat(),
    ));
    // "JAIL" sign board over the door.
    prims.push(prim(
        solid(cuboid_tapered([1.8, 0.5, 0.12], 0.0, clapboard(CLAP_WHITE))),
        [0.0, body_h - 0.3, front_z + 0.12],
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
        assert_sanitize_stable(&Jail.build(""), "jail");
    }
}
