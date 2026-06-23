//! Bench — a slatted seat on iron end-frames. An escalation-Calm scatter
//! prop: public seating signals a settled, unthreatened place to linger in
//! any setting.

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD, bronze, wood};

const IRON: [f32; 3] = [0.12, 0.12, 0.13];

pub struct Bench;

impl CatalogueEntry for Bench {
    fn slug(&self) -> &'static str {
        "bench"
    }
    fn name(&self) -> &'static str {
        "Bench"
    }
    fn description(&self) -> &'static str {
        "Slatted wooden seat on cast-iron end-frames."
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
            clearance: 1.1,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = Vec::new();

    // Seat slats running the length of the bench, with gaps between them.
    for dz in [-0.18_f32, -0.06, 0.06, 0.18] {
        prims.push(prim(
            solid(cuboid_tapered([1.34, 0.05, 0.09], 0.0, wood(WOOD))),
            [0.0, 0.52, dz],
            id_quat(),
        ));
    }

    // Backrest slats, leaning back a touch.
    for dy in [0.66_f32, 0.82, 0.98] {
        prims.push(prim(
            solid(cuboid_tapered([1.34, 0.1, 0.05], 0.0, wood(WOOD))),
            [0.0, dy, -0.22],
            quat_x(-0.1),
        ));
    }

    // Shaped cast-iron end frames: legs, seat rail, arm post and armrest.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.7;
        // Front leg.
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.52, 0.07], 0.0, bronze(IRON))),
            [x, 0.26, 0.2],
            id_quat(),
        ));
        // Back leg, taller to carry the backrest.
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.8, 0.07], 0.0, bronze(IRON))),
            [x, 0.4, -0.22],
            id_quat(),
        ));
        // Seat rail tying the legs together.
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.07, 0.5], 0.0, bronze(IRON))),
            [x, 0.49, 0.0],
            id_quat(),
        ));
        // Front arm post.
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.32, 0.07], 0.0, bronze(IRON))),
            [x, 0.66, 0.2],
            id_quat(),
        ));
        // Armrest.
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.06, 0.5], 0.0, bronze(IRON))),
            [x, 0.82, 0.0],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
