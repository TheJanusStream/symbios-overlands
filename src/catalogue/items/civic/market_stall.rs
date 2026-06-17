//! Market stall — a counter under a striped awning, goods on display. An
//! escalation-Calm scatter prop: open-air commerce signals a peaceful,
//! trading settlement in any setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{CANVAS_CREAM, CANVAS_RED, WOOD, WOOD_GREY, cloth, foliage, wood};

pub struct MarketStall;

impl CatalogueEntry for MarketStall {
    fn slug(&self) -> &'static str {
        "market_stall"
    }
    fn name(&self) -> &'static str {
        "Market Stall"
    }
    fn description(&self) -> &'static str {
        "Counter under a striped awning with goods on display."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Calm)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let post_h = 2.1;
    let mut prims = vec![
        // Counter.
        prim(
            solid(cuboid_tapered([1.6, 0.9, 0.7], 0.0, wood(WOOD))),
            [0.0, 0.45, 0.2],
            id_quat(),
        ),
    ];

    // Four corner posts.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.06, post_h, 8, 0.0, wood(WOOD_GREY))),
            [sx * 0.8, post_h * 0.5, sz * 0.5],
            id_quat(),
        ));
    }

    // Striped awning — a red field with a cream half overlaid, gently sloped.
    prims.push(prim(
        solid(cuboid_tapered([1.9, 0.07, 1.3], 0.0, cloth(CANVAS_RED))),
        [0.0, post_h + 0.05, 0.0],
        quat_x(0.16),
    ));
    prims.push(prim(
        cuboid_tapered([0.95, 0.08, 1.34], 0.0, cloth(CANVAS_CREAM)),
        [-0.47, post_h + 0.09, 0.0],
        quat_x(0.16),
    ));

    // Goods heaped on the counter.
    for (x, color) in [
        (-0.4_f32, [0.85_f32, 0.4_f32, 0.15_f32]),
        (0.0, [0.7, 0.2, 0.2]),
        (0.4, [0.85, 0.75, 0.2]),
    ] {
        prims.push(prim(
            sphere(0.12, 3, foliage(color)),
            [x, 0.97, 0.2],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
