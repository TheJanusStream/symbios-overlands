//! Barrel fire — a rusted oil drum with a flame licking out of the top. A
//! prosperity-Poor scatter prop: the universal sign of people keeping warm
//! on the margins, in any setting.

use crate::catalogue::items::util::{cylinder_tapered, glow, id_quat, prim, solid, sphere, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{EMBER, FIRE, RUST, quat_z, rust_metal};

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

    let mut prims = vec![
        // The drum.
        prim(
            solid(cylinder_tapered(drum_r, drum_h, 14, 0.0, rust_metal(RUST))),
            [0.0, drum_h * 0.5, 0.0],
            id_quat(),
        ),
        // Two raised hoop bands (round, proud of the wall).
        prim(
            torus(0.035, drum_r + 0.01, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.3, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.035, drum_r + 0.01, rust_metal([0.3, 0.16, 0.1])),
            [0.0, drum_h * 0.72, 0.0],
            id_quat(),
        ),
        // Charred top rim.
        prim(
            torus(0.04, drum_r, rust_metal([0.12, 0.1, 0.09])),
            [0.0, drum_h, 0.0],
            id_quat(),
        ),
        // Glowing coal bed at the rim — a deep-saturated ember mass at
        // moderate strength so it reads hot, not a washed near-white blob.
        prim(
            sphere(0.24, 3, glow(EMBER, 3.5)),
            [0.0, drum_h + 0.02, 0.0],
            id_quat(),
        ),
    ];

    // Small bright coals nestled in the bed.
    for (dx, dz) in [(-0.12_f32, 0.06_f32), (0.13, -0.05), (0.0, 0.13)] {
        prims.push(prim(
            sphere(0.07, 3, glow([1.0, 0.34, 0.06], 3.0)),
            [dx, drum_h + 0.06, dz],
            id_quat(),
        ));
    }

    // Flame tongues — thin tapered cylinders pinching to points, deep
    // orange. Several of varied height beat one smooth cone.
    for (dx, dz, r, h) in [
        (0.0_f32, 0.0_f32, 0.13_f32, 0.78_f32),
        (-0.1, 0.05, 0.1, 0.52),
        (0.1, -0.06, 0.1, 0.56),
        (0.05, 0.12, 0.09, 0.44),
        (-0.08, -0.1, 0.08, 0.4),
    ] {
        prims.push(prim(
            cylinder_tapered(r, h, 8, 0.88, glow(FIRE, 3.2)),
            [dx, drum_h + 0.06 + h * 0.5, dz],
            id_quat(),
        ));
    }

    // Inner yellow-hot tips.
    for (dx, dz, h) in [(0.0_f32, 0.0_f32, 0.5_f32), (-0.06, 0.04, 0.34)] {
        prims.push(prim(
            cylinder_tapered(0.05, h, 6, 0.9, glow([1.0, 0.66, 0.18], 3.0)),
            [dx, drum_h + 0.12 + h * 0.5, dz],
            id_quat(),
        ));
    }

    // A charred stick poking out of the barrel.
    prims.push(prim(
        solid(cylinder_tapered(
            0.025,
            0.55,
            6,
            0.0,
            rust_metal([0.18, 0.12, 0.08]),
        )),
        [0.22, drum_h + 0.02, 0.05],
        quat_z(0.6),
    ));

    super::assemble(prims)
}
