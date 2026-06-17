//! Fountain — a tiered marble basin with a central jet. A prosperity-Rich
//! scatter prop: ornamental waterworks signal civic wealth in any setting.

use crate::catalogue::items::util::{cylinder_tapered, id_quat, prim, solid, sphere, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{MARBLE, WATER_BLUE, marble};

pub struct Fountain;

impl CatalogueEntry for Fountain {
    fn slug(&self) -> &'static str {
        "fountain"
    }
    fn name(&self) -> &'static str {
        "Fountain"
    }
    fn description(&self) -> &'static str {
        "Tiered marble basin with a central water jet."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let basin_r = 1.4;
    let basin_h = 0.45;

    let ped_h = 0.9;
    let bowl_y = basin_h + ped_h;
    super::assemble(vec![
        // Lower basin.
        prim(
            solid(cylinder_tapered(basin_r, basin_h, 24, 0.0, marble(MARBLE))),
            [0.0, basin_h * 0.5, 0.0],
            id_quat(),
        ),
        // Rim lip around the basin.
        prim(
            torus(0.1, basin_r, marble([0.8, 0.79, 0.76])),
            [0.0, basin_h, 0.0],
            id_quat(),
        ),
        // Water surface of the lower basin (a shallow disc).
        prim(
            cylinder_tapered(basin_r - 0.18, 0.06, 24, 0.0, marble(WATER_BLUE)),
            [0.0, basin_h - 0.05, 0.0],
            id_quat(),
        ),
        // Central pedestal.
        prim(
            solid(cylinder_tapered(0.24, ped_h, 16, 0.1, marble(MARBLE))),
            [0.0, basin_h + ped_h * 0.5, 0.0],
            id_quat(),
        ),
        // Upper bowl + its water.
        prim(
            solid(cylinder_tapered(0.6, 0.16, 20, 0.0, marble(MARBLE))),
            [0.0, bowl_y, 0.0],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.5, 0.05, 20, 0.0, marble(WATER_BLUE)),
            [0.0, bowl_y + 0.08, 0.0],
            id_quat(),
        ),
        // Jet rising from the bowl and the spray orb on top.
        prim(
            cylinder_tapered(0.05, 0.5, 8, 0.0, marble(WATER_BLUE)),
            [0.0, bowl_y + 0.3, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.16, 3, marble(WATER_BLUE)),
            [0.0, bowl_y + 0.6, 0.0],
            id_quat(),
        ),
    ])
}
