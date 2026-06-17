//! Barrel fire — a rusted oil drum with a flame licking out of the top. A
//! prosperity-Poor scatter prop: the universal sign of people keeping warm
//! on the margins, in any setting.

use crate::catalogue::items::util::{cone, cylinder_tapered, glow, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{FIRE, RUST, rust_metal};

pub struct BarrelFire;

impl CatalogueEntry for BarrelFire {
    fn slug(&self) -> &'static str {
        "barrel_fire"
    }
    fn name(&self) -> &'static str {
        "Barrel Fire"
    }
    fn description(&self) -> &'static str {
        "Rusted oil drum with a flame licking out of the top."
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
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let drum_h = 0.9;
    let drum_r = 0.34;

    super::assemble(vec![
        // The drum.
        prim(
            solid(cylinder_tapered(drum_r, drum_h, 14, 0.0, rust_metal(RUST))),
            [0.0, drum_h * 0.5, 0.0],
            id_quat(),
        ),
        // Two raised hoop bands.
        prim(
            cylinder_tapered(drum_r + 0.03, 0.06, 14, 0.0, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.3, 0.0],
            id_quat(),
        ),
        prim(
            cylinder_tapered(drum_r + 0.03, 0.06, 14, 0.0, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.75, 0.0],
            id_quat(),
        ),
        // Flame — a glowing cone with a hot ember core just above the rim.
        prim(
            cone(drum_r * 0.85, 0.75, 10, glow(FIRE, 6.0)),
            [0.0, drum_h + 0.32, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.18, 3, glow([1.0, 0.32, 0.06], 9.0)),
            [0.0, drum_h + 0.12, 0.0],
            id_quat(),
        ),
    ])
}
