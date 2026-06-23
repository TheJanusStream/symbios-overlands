//! Lantern — a standing lamp post with a warm glowing head. An
//! escalation-Calm scatter prop: maintained street lighting signals a safe,
//! orderly settlement in any setting.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{BRONZE, LANTERN_WARM, bronze};

pub struct Lantern;

impl CatalogueEntry for Lantern {
    fn slug(&self) -> &'static str {
        "lantern"
    }
    fn name(&self) -> &'static str {
        "Lantern"
    }
    fn description(&self) -> &'static str {
        "Standing lamp post with a warm glowing head."
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
            clearance: 0.9,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 2.4;
    let head_y = pole_h + 0.22; // centre of the glowing core
    let half = 0.14; // half-span of the lantern cage

    let mut prims = vec![
        // Weighted base + a proud foot ring.
        prim(
            solid(cylinder_tapered(0.18, 0.2, 12, 0.0, bronze(BRONZE))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.04, 0.2, bronze(BRONZE)),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
        // Pole.
        prim(
            solid(cylinder_tapered(0.06, pole_h, 10, 0.0, bronze(BRONZE))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
        // Bottom collar the lantern head sits on.
        prim(
            solid(cylinder_tapered(0.17, 0.07, 8, 0.0, bronze(BRONZE))),
            [0.0, pole_h, 0.0],
            id_quat(),
        ),
        // Deep-amber glow core, set inside the cage so the bronze frame
        // breaks it up — saturated colour at moderate strength reads
        // incandescent instead of washing to a pale near-white box.
        prim(
            cuboid_tapered([0.21, 0.36, 0.21], 0.0, glow(LANTERN_WARM, 2.6)),
            [0.0, head_y, 0.0],
            id_quat(),
        ),
        // Top frame collar.
        prim(
            solid(cylinder_tapered(0.17, 0.07, 8, 0.0, bronze(BRONZE))),
            [0.0, head_y + 0.24, 0.0],
            id_quat(),
        ),
        // Peaked bronze roof + finial.
        prim(
            cone(0.24, 0.22, 4, bronze(BRONZE)),
            [0.0, head_y + 0.4, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.05, 3, bronze(BRONZE)),
            [0.0, head_y + 0.54, 0.0],
            id_quat(),
        ),
    ];

    // Four bronze cage posts standing proud of the glow at the corners.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.035, 0.36, 0.035], 0.0, bronze(BRONZE))),
            [sx * half, head_y, sz * half],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
