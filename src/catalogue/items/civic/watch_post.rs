//! Watch post — a stilted timber platform with a railing and a pyramidal
//! roof. An escalation-Conflict scatter prop: a hasty lookout reads the
//! same whether it overlooks a medieval road or a cyberpunk checkpoint.

use crate::catalogue::items::util::{cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD, WOOD_GREY, wood};

pub struct WatchPost;

impl CatalogueEntry for WatchPost {
    fn slug(&self) -> &'static str {
        "watch_post"
    }
    fn name(&self) -> &'static str {
        "Watch Post"
    }
    fn description(&self) -> &'static str {
        "Stilted timber platform with a railing and a peaked roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Conflict)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let leg_h = 2.0;
    let half = 0.55;
    let leg = || solid(cylinder_tapered(0.08, leg_h, 8, 0.0, wood(WOOD)));
    let bar = |sx: f32, sz: f32| solid(cuboid_tapered([sx, 0.08, sz], 0.0, wood(WOOD)));

    let mut prims = Vec::new();
    // Four stilt legs.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(leg(), [sx * half, leg_h * 0.5, sz * half], id_quat()));
    }
    // Platform deck.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.12, 1.4], 0.0, wood(WOOD_GREY))),
        [0.0, leg_h, 0.0],
        id_quat(),
    ));
    // Railing — four bars around the deck.
    let rail_y = leg_h + 0.42;
    prims.push(prim(bar(1.4, 0.08), [0.0, rail_y, -0.66], id_quat()));
    prims.push(prim(bar(1.4, 0.08), [0.0, rail_y, 0.66], id_quat()));
    prims.push(prim(bar(0.08, 1.4), [-0.66, rail_y, 0.0], id_quat()));
    prims.push(prim(bar(0.08, 1.4), [0.66, rail_y, 0.0], id_quat()));
    // Pyramidal roof.
    prims.push(prim(
        cone(1.1, 0.7, 4, wood([0.3, 0.2, 0.12])),
        [0.0, leg_h + 0.95, 0.0],
        id_quat(),
    ));

    super::assemble(prims)
}
