//! Laundry line — two leaning posts strung with a sagging rope and a few
//! mismatched garments. A prosperity-Poor scatter prop signalling crowded,
//! make-do living in any setting.

use crate::catalogue::items::util::{cuboid_tapered, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{CANVAS_CREAM, CANVAS_RED, WOOD_GREY, cloth, wood};

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
    let post = || solid(cylinder_tapered(0.06, post_h, 8, 0.0, wood(WOOD_GREY)));

    let mut prims = vec![
        // The two posts, symmetric about the origin.
        prim(post(), [-span * 0.5, post_h * 0.5, 0.0], id_quat()),
        prim(post(), [span * 0.5, post_h * 0.5, 0.0], id_quat()),
        // The line itself — a thin dark cord just below the post tops.
        prim(
            cuboid_tapered([span + 0.1, 0.03, 0.03], 0.0, wood([0.15, 0.12, 0.08])),
            [0.0, line_y, 0.0],
            id_quat(),
        ),
    ];

    // Hanging garments at varied positions / colours / sizes.
    let garments = [
        (-0.95_f32, 0.7_f32, 0.55_f32, CANVAS_RED),
        (-0.25, 0.45, 0.7, CANVAS_CREAM),
        (0.4, 0.6, 0.5, [0.28, 0.38, 0.58]),
        (0.95, 0.5, 0.45, [0.5, 0.45, 0.3]),
    ];
    for (x, w, drop, color) in garments {
        prims.push(prim(
            cuboid_tapered([w, drop, 0.04], 0.0, cloth(color)),
            [x, line_y - drop * 0.5, 0.0],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
