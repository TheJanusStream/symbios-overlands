//! Gargoyle — a Gothic-Horror prop. A crouched stone grotesque on a plinth,
//! wings half-spread, snout jutting. Scatter clutter watching from the
//! necropolis.
//!
//! The wings tilt with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_DARK, stone};

pub struct Gargoyle;

impl CatalogueEntry for Gargoyle {
    fn slug(&self) -> &'static str {
        "gargoyle"
    }
    fn name(&self) -> &'static str {
        "Gargoyle"
    }
    fn description(&self) -> &'static str {
        "Crouched stone grotesque on a plinth, wings half-spread, snout jutting."
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
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Stone plinth — the root.
        prim(
            solid(cuboid_tapered([0.9, 1.2, 0.9], 0.0, stone(STONE_DARK))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
    ];

    // Crouched body.
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.6, 0.8], 0.1, stone(STONE_DARK))),
        [0.0, 1.5, 0.0],
        id_quat(),
    ));
    // Head + jutting snout.
    prims.push(prim(
        solid(cuboid_tapered([0.45, 0.45, 0.45], 0.1, stone(STONE_DARK))),
        [0.0, 2.0, 0.25],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.18, 0.5, 6, stone(STONE_DARK))),
        [0.0, 2.0, 0.6],
        quat_x(1.4),
    ));
    // Half-spread wings.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.9, 0.7], 0.4, stone(STONE_DARK))),
            [sx * 0.45, 1.7, -0.3],
            quat_x(-0.4),
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
        assert_sanitize_stable(&Gargoyle.build(""), "gargoyle");
    }
}
