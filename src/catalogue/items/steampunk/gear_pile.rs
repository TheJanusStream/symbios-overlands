//! Gear pile — a Steampunk prop. A heap of brass and iron cogs, some stacked
//! flat, one leaning on its edge. Scatter clutter of the works' yard.
//!
//! The leaning cog is a disc stood on its edge with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, brass, iron};

pub struct GearPile;

impl CatalogueEntry for GearPile {
    fn slug(&self) -> &'static str {
        "gear_pile"
    }
    fn name(&self) -> &'static str {
        "Gear Pile"
    }
    fn description(&self) -> &'static str {
        "A heap of brass and iron cogs, some stacked flat, one leaning on its edge."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
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
        // Largest cog lying flat — the root.
        prim(
            solid(cylinder_tapered(0.7, 0.18, 16, 0.0, iron(IRON_DARK))),
            [0.0, 0.09, 0.0],
            id_quat(),
        ),
    ];

    // A few more cogs stacked and offset.
    prims.push(prim(
        solid(cylinder_tapered(0.5, 0.16, 14, 0.0, brass(BRASS))),
        [0.25, 0.27, 0.1],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.4, 0.14, 12, 0.0, iron(IRON_DARK))),
        [-0.4, 0.16, 0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.32, 0.12, 12, 0.0, brass(BRASS))),
        [0.15, 0.42, 0.15],
        id_quat(),
    ));

    // One cog leaning on its edge.
    prims.push(prim(
        solid(cylinder_tapered(0.55, 0.16, 14, 0.0, brass(BRASS))),
        [0.7, 0.5, -0.4],
        quat_x(FRAC_PI_2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GearPile.build(""), "gear_pile");
    }
}
