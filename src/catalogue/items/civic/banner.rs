//! Banner — a tall pole flying a long hanging banner under a gilt finial. A
//! prosperity-Rich scatter prop: heraldic / civic display signals pride and
//! means in any setting.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
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
    use std::f32::consts::PI;
    let pole_h = 3.4;
    let bz = -0.10; // banner hangs forward of the pole, toward the -Z front.

    let crossarm_y = pole_h - 0.25;
    let banner_top = crossarm_y - 0.05;
    let field_drop = 1.66;
    let chief_h = 0.22;
    let field_y = banner_top - chief_h - field_drop * 0.5;
    let field_bottom = banner_top - chief_h - field_drop;

    let mut prims = vec![
        // Pole.
        prim(
            solid(cylinder_tapered(0.08, pole_h, 10, 0.0, wood(WOOD))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
        // Crossbar the gonfalon hangs from, bridging pole to banner.
        prim(
            solid(cuboid_tapered([1.0, 0.06, 0.06], 0.0, wood(WOOD))),
            [0.0, crossarm_y, bz * 0.5],
            id_quat(),
        ),
        // Chief band (the contrasting top stripe).
        prim(
            cuboid_tapered([0.95, chief_h, 0.04], 0.0, cloth(GOLD)),
            [0.0, banner_top - chief_h * 0.5, bz],
            id_quat(),
        ),
        // Main red field.
        prim(
            cuboid_tapered([0.95, field_drop, 0.04], 0.0, cloth(CANVAS_RED)),
            [0.0, field_y, bz],
            id_quat(),
        ),
        // Gilt fringe band near the foot of the field.
        prim(
            cuboid_tapered([0.95, 0.08, 0.05], 0.0, cloth(GOLD)),
            [0.0, field_bottom + 0.06, bz],
            id_quat(),
        ),
        // Gold emblem charge — a disc straddling both faces so it reads
        // front and back, not painted on one side.
        prim(
            solid(cylinder_tapered(0.26, 0.12, 12, 0.0, bronze(GOLD))),
            [0.0, field_y + 0.1, bz],
            quat_x(PI * 0.5),
        ),
        // Spear-point finial.
        prim(
            sphere(0.09, 3, bronze(GOLD)),
            [0.0, pole_h + 0.04, 0.0],
            id_quat(),
        ),
        prim(
            cone(0.07, 0.3, 8, bronze(GOLD)),
            [0.0, pole_h + 0.24, 0.0],
            quat_x(PI),
        ),
        // Decorative pole bands.
        prim(
            torus(0.02, 0.085, bronze(GOLD)),
            [0.0, crossarm_y - 0.4, 0.0],
            id_quat(),
        ),
        prim(torus(0.02, 0.085, bronze(GOLD)), [0.0, 0.5, 0.0], id_quat()),
    ];

    // Swallowtail tails with the central notch between them.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.34, 0.5, 0.04], 0.0, cloth(CANVAS_RED)),
            [sx * 0.28, field_bottom - 0.25, bz],
            id_quat(),
        ));
        // Tassel hanging from each tail.
        prims.push(prim(
            sphere(0.05, 3, bronze(GOLD)),
            [sx * 0.28, field_bottom - 0.52, bz],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
