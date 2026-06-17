//! Banner — a tall pole flying a long hanging banner under a gilt finial. A
//! prosperity-Rich scatter prop: heraldic / civic display signals pride and
//! means in any setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{CANVAS_RED, GOLD, WOOD, bronze, cloth, wood};

pub struct Banner;

impl CatalogueEntry for Banner {
    fn slug(&self) -> &'static str {
        "banner"
    }
    fn name(&self) -> &'static str {
        "Banner"
    }
    fn description(&self) -> &'static str {
        "Tall pole flying a long banner under a gilt finial."
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
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 3.4;

    let bar_y = pole_h - 0.3;
    let banner_drop = 1.8;
    let banner_y = bar_y - banner_drop * 0.5 - 0.03;
    super::assemble(vec![
        // Pole.
        prim(
            solid(cylinder_tapered(0.08, pole_h, 10, 0.0, wood(WOOD))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
        // Crossbar near the top the banner hangs from.
        prim(
            solid(cuboid_tapered([1.0, 0.06, 0.06], 0.0, wood(WOOD))),
            [0.35, bar_y, 0.0],
            id_quat(),
        ),
        // Banner cloth, plus a contrasting band near its foot.
        prim(
            cuboid_tapered([0.9, banner_drop, 0.04], 0.0, cloth(CANVAS_RED)),
            [0.35, banner_y, 0.0],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.9, 0.18, 0.05], 0.0, cloth(GOLD)),
            [0.35, banner_y - banner_drop * 0.5 + 0.12, 0.0],
            id_quat(),
        ),
        // Gilt finial.
        prim(
            sphere(0.12, 3, bronze(GOLD)),
            [0.0, pole_h + 0.06, 0.0],
            id_quat(),
        ),
    ])
}
