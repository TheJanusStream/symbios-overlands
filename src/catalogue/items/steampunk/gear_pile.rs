//! Gear pile — a Steampunk prop. A heap of brass and iron toothed cogs, some
//! stacked flat, one leaning on its edge. Scatter clutter of the works' yard.
//!
//! Each cog is built by the shared [`cog`] helper; the leaning one is stood on
//! its edge with a [`quat_x`] of π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{assemble, id_quat, quat_x};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, brass, cog, iron};

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
        // Largest toothed cog lying flat — the root.
        cog(
            [0.0, 0.1, 0.0],
            id_quat(),
            0.72,
            0.18,
            16,
            iron(IRON_DARK),
            brass(BRASS),
        ),
    ];

    // A few more toothed cogs stacked and offset.
    prims.push(cog(
        [0.34, 0.32, 0.12],
        id_quat(),
        0.5,
        0.16,
        13,
        brass(BRASS),
        iron(IRON_DARK),
    ));
    prims.push(cog(
        [-0.44, 0.16, 0.34],
        id_quat(),
        0.4,
        0.14,
        11,
        iron(IRON_DARK),
        brass(BRASS),
    ));
    prims.push(cog(
        [0.18, 0.5, 0.16],
        id_quat(),
        0.32,
        0.12,
        9,
        brass(BRASS),
        iron(IRON_DARK),
    ));

    // One cog leaning on its edge.
    prims.push(cog(
        [0.78, 0.52, -0.42],
        quat_x(FRAC_PI_2),
        0.56,
        0.16,
        13,
        brass(BRASS),
        iron(IRON_DARK),
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
