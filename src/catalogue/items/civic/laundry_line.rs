//! Laundry line — two leaning posts strung with a sagging rope and a few
//! mismatched garments. A prosperity-Poor scatter prop signalling crowded,
//! make-do living in any setting.

use crate::catalogue::items::util::{cuboid_tapered, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{CANVAS_CREAM, CANVAS_RED, WOOD_GREY, cloth, quat_z, wood};

pub struct LaundryLine;

impl CatalogueEntry for LaundryLine {
    fn slug(&self) -> &'static str {
        "laundry_line"
    }
    fn name(&self) -> &'static str {
        "Laundry Line"
    }
    fn description(&self) -> &'static str {
        "Two posts strung with a sagging line of mismatched garments."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Poor)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 2.6;
    let post_h = 2.0;
    let line_y = post_h - 0.15;
    let cord = |color: [f32; 3]| solid(cuboid_tapered([1.0, 0.03, 0.03], 0.0, wood(color)));
    let dark = [0.15, 0.12, 0.08];

    let mut prims = vec![
        // Post[0] — vertical, the flat root.
        prim(
            solid(cylinder_tapered(0.06, post_h, 8, 0.0, wood(WOOD_GREY))),
            [-span * 0.5, post_h * 0.5, 0.0],
            id_quat(),
        ),
        // Post[1] — leaning a touch, make-do.
        prim(
            solid(cylinder_tapered(0.06, post_h, 8, 0.0, wood(WOOD_GREY))),
            [span * 0.5, post_h * 0.5, 0.0],
            quat_z(-0.07),
        ),
        // Sagging cord — three segments dipping to a low centre (catenary).
        prim(cord(dark), [-0.78, line_y - 0.06, 0.0], quat_z(-0.2)),
        prim(
            solid(cuboid_tapered([0.8, 0.03, 0.03], 0.0, wood(dark))),
            [0.0, line_y - 0.23, 0.0],
            id_quat(),
        ),
        prim(cord(dark), [0.78, line_y - 0.06, 0.0], quat_z(0.2)),
    ];

    // A shaped garment — body, shoulder yoke and two angled sleeves.
    let mut shirt = |cx: f32, cy: f32, color: [f32; 3]| {
        prims.push(prim(
            cuboid_tapered([0.46, 0.5, 0.04], 0.0, cloth(color)),
            [cx, cy, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.62, 0.12, 0.04], 0.0, cloth(color)),
            [cx, cy + 0.27, 0.0],
            id_quat(),
        ));
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.15, 0.26, 0.04], 0.0, cloth(color)),
                [cx + sx * 0.31, cy + 0.12, 0.0],
                quat_z(sx * 0.4),
            ));
        }
        // Clothes peg.
        prims.push(prim(
            cuboid_tapered([0.05, 0.08, 0.05], 0.0, wood([0.55, 0.42, 0.24])),
            [cx, cy + 0.34, 0.0],
            id_quat(),
        ));
    };
    shirt(-0.8, 1.5, CANVAS_RED);

    // A broad sheet hanging at the low centre.
    prims.push(prim(
        cuboid_tapered([0.72, 0.7, 0.04], 0.0, cloth(CANVAS_CREAM)),
        [-0.05, 1.27, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.05, 0.08, 0.05], 0.0, wood([0.55, 0.42, 0.24])),
            [-0.05 + sx * 0.28, 1.64, 0.0],
            id_quat(),
        ));
    }

    // Trousers — waistband and two legs.
    let trews = [0.28_f32, 0.38, 0.58];
    prims.push(prim(
        cuboid_tapered([0.4, 0.12, 0.04], 0.0, cloth(trews)),
        [0.62, 1.58, 0.0],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.16, 0.5, 0.04], 0.0, cloth(trews)),
            [0.62 + sx * 0.1, 1.27, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.05, 0.08, 0.05], 0.0, wood([0.55, 0.42, 0.24])),
        [0.62, 1.66, 0.0],
        id_quat(),
    ));

    // A small towel near the leaning post.
    prims.push(prim(
        cuboid_tapered([0.22, 0.34, 0.04], 0.0, cloth([0.5, 0.55, 0.32])),
        [1.05, 1.5, 0.0],
        id_quat(),
    ));

    // A wicker basket of washing at the foot of the line.
    prims.push(prim(
        solid(cylinder_tapered(
            0.3,
            0.32,
            12,
            0.12,
            wood([0.58, 0.44, 0.24]),
        )),
        [-0.5, 0.16, 0.45],
        id_quat(),
    ));
    for (dx, color) in [(-0.08_f32, CANVAS_CREAM), (0.08, [0.5, 0.45, 0.3])] {
        prims.push(prim(
            cuboid_tapered([0.2, 0.16, 0.18], 0.0, cloth(color)),
            [-0.5 + dx, 0.36, 0.45],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
