//! Market stall — a counter under a striped awning, goods on display. An
//! escalation-Calm scatter prop: open-air commerce signals a peaceful,
//! trading settlement in any setting.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
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
    // The open serving side faces the -Z render front: counter and goods up
    // front, vendor shelf and stock at the back (+Z).
    let mut prims = vec![
        // Counter, its serving face toward the front.
        prim(
            solid(cuboid_tapered([1.6, 0.9, 0.55], 0.0, wood(WOOD))),
            [0.0, 0.45, -0.2],
            id_quat(),
        ),
        // Counter top board, slightly proud.
        prim(
            solid(cuboid_tapered([1.66, 0.06, 0.6], 0.0, wood(WOOD_GREY))),
            [0.0, 0.92, -0.2],
            id_quat(),
        ),
        // Back stock shelf between the rear posts.
        prim(
            solid(cuboid_tapered([1.45, 0.05, 0.26], 0.0, wood(WOOD_GREY))),
            [0.0, 1.35, 0.42],
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

    // Striped awning — a red field with a cream half overlaid, sloping down
    // toward the front so it shades the goods.
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

    // Front valance hem with dagged scallops.
    let valance_y = post_h - 0.06;
    prims.push(prim(cloth_strip(), [0.0, valance_y, -0.66], id_quat()));
    for i in 0..5 {
        let x = -0.76 + i as f32 * 0.38;
        prims.push(prim(
            cone(0.12, 0.2, 3, cloth(CANVAS_CREAM)),
            [x, valance_y - 0.18, -0.66],
            id_quat(),
        ));
    }

    // Produce heaped on the counter, facing the customer.
    for (x, color) in [
        (-0.4_f32, [0.85_f32, 0.4_f32, 0.15_f32]),
        (0.0, [0.7, 0.2, 0.2]),
        (0.4, [0.85, 0.75, 0.2]),
    ] {
        prims.push(prim(
            sphere(0.12, 3, foliage(color)),
            [x, 1.0, -0.32],
            id_quat(),
        ));
    }

    // Stock on the back shelf.
    for (x, color) in [
        (-0.45_f32, [0.6_f32, 0.5_f32, 0.2_f32]),
        (0.1, [0.8, 0.3, 0.2]),
        (0.5, [0.5, 0.6, 0.25]),
    ] {
        prims.push(prim(
            sphere(0.1, 3, foliage(color)),
            [x, 1.45, 0.42],
            id_quat(),
        ));
    }

    // Crates of goods stacked beside the counter on the ground.
    for (sx, dy) in [(-1.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 0.32)] {
        prims.push(prim(
            solid(cuboid_tapered([0.32, 0.3, 0.32], 0.0, wood(WOOD))),
            [sx * 0.62, 0.15 + dy, -0.42],
            id_quat(),
        ));
    }
    prims.push(prim(
        sphere(0.11, 3, foliage([0.85, 0.55, 0.2])),
        [-0.62, 0.36, -0.42],
        id_quat(),
    ));

    super::assemble(prims)
}

/// The valance backing strip — a long cream cloth band.
fn cloth_strip() -> crate::pds::GeneratorKind {
    cuboid_tapered([1.9, 0.16, 0.03], 0.0, cloth(CANVAS_CREAM))
}
