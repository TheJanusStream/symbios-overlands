//! Shanty — a makeshift lean-to of mismatched boards under a slanted tin
//! roof. A prosperity-Poor scatter prop: it reads as improvised housing in
//! any setting, from a medieval slum to a cyberpunk undercity.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{TIN, WOOD, WOOD_GREY, corrugated, rust_metal, wood};

pub struct Shanty;

impl CatalogueEntry for Shanty {
    fn slug(&self) -> &'static str {
        "shanty"
    }
    fn name(&self) -> &'static str {
        "Shanty"
    }
    fn description(&self) -> &'static str {
        "Makeshift board lean-to under a slanted tin roof."
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
            clearance: 1.7,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 1.8;
    let d = 1.4;
    let h = 1.5;
    let front = -d * 0.5; // the detailed open face points to the -Z render front

    let mut prims = vec![
        // Plank floor pad — the flat root.
        prim(
            solid(cuboid_tapered(
                [w + 0.1, 0.08, d + 0.1],
                0.0,
                wood(WOOD_GREY),
            )),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
        // Back wall (+Z).
        prim(
            solid(cuboid_tapered([w, h, 0.1], 0.0, wood(WOOD))),
            [0.0, h * 0.5, d * 0.5],
            id_quat(),
        ),
        // Side walls in mismatched tones, one a touch shorter.
        prim(
            solid(cuboid_tapered([0.1, h, d], 0.0, wood(WOOD_GREY))),
            [-w * 0.5, h * 0.5, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [0.1, h - 0.1, d],
                0.0,
                wood([0.36, 0.28, 0.18]),
            )),
            [w * 0.5, (h - 0.1) * 0.5, 0.0],
            id_quat(),
        ),
        // Dark interior visible through the doorway gap.
        prim(
            cuboid_tapered([0.85, 1.2, 0.06], 0.0, wood([0.07, 0.06, 0.05])),
            [0.05, 0.62, front + 0.12],
            id_quat(),
        ),
        // Front wall built from two mismatched plank slabs, leaving a
        // doorway gap in the middle.
        prim(
            solid(cuboid_tapered([0.55, h, 0.1], 0.0, wood(WOOD))),
            [-0.6, h * 0.5, front],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.42, h - 0.08, 0.1], 0.0, wood(WOOD_GREY))),
            [0.62, (h - 0.08) * 0.5, front],
            id_quat(),
        ),
        // Lintel bridging the doorway.
        prim(
            solid(cuboid_tapered(
                [0.9, 0.22, 0.12],
                0.0,
                wood([0.3, 0.2, 0.12]),
            )),
            [0.05, h - 0.13, front],
            id_quat(),
        ),
        // A plank door, ajar against the jamb.
        prim(
            solid(cuboid_tapered([0.5, 1.12, 0.05], 0.0, wood(WOOD_GREY))),
            [-0.12, 0.58, front - 0.14],
            super::quat_z(0.1),
        ),
        // Window frame + dark pane in the left front plank.
        prim(
            cuboid_tapered([0.42, 0.42, 0.03], 0.0, wood([0.3, 0.2, 0.12])),
            [-0.6, 0.98, front - 0.05],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.32, 0.32, 0.04], 0.0, wood([0.1, 0.11, 0.13])),
            [-0.6, 0.98, front - 0.07],
            id_quat(),
        ),
        // A nailed-on corrugated patch and a grey board patch.
        prim(
            cuboid_tapered([0.4, 0.34, 0.04], 0.0, corrugated(TIN)),
            [0.55, 0.55, front - 0.07],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.05, 0.7, 0.5], 0.0, wood(WOOD_GREY)),
            [-w * 0.5 - 0.03, 0.6, 0.15],
            id_quat(),
        ),
        // Slanted, overhanging corrugated tin roof (high eave at the front).
        prim(
            solid(cuboid_tapered(
                [w + 0.4, 0.08, d + 0.6],
                0.0,
                corrugated(TIN),
            )),
            [0.0, h + 0.3, 0.05],
            quat_x(0.32),
        ),
        // Ridge cap board along the high front edge.
        prim(
            solid(cuboid_tapered(
                [w + 0.5, 0.1, 0.14],
                0.0,
                wood([0.3, 0.22, 0.14]),
            )),
            [0.0, h + 0.5, front - 0.1],
            id_quat(),
        ),
        // Stovepipe chimney through the roof, with a cap.
        prim(
            solid(cylinder_tapered(
                0.06,
                0.7,
                8,
                0.0,
                rust_metal([0.3, 0.18, 0.12]),
            )),
            [0.62, h + 0.6, 0.35],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.1, 0.06, 8, 0.0, rust_metal([0.2, 0.13, 0.09])),
            [0.62, h + 0.96, 0.35],
            id_quat(),
        ),
        // A leaning support post propping the high front eave.
        prim(
            solid(cuboid_tapered([0.1, h + 0.55, 0.1], 0.0, wood(WOOD_GREY))),
            [w * 0.5 - 0.08, (h + 0.55) * 0.5, front - 0.18],
            quat_x(0.1),
        ),
    ];

    // Mismatched plank battens nailed across the back wall.
    for (dy, tone) in [(0.5_f32, WOOD_GREY), (1.05, [0.36, 0.28, 0.18])] {
        prims.push(prim(
            cuboid_tapered([w - 0.2, 0.12, 0.05], 0.0, wood(tone)),
            [0.0, dy, d * 0.5 + 0.06],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
