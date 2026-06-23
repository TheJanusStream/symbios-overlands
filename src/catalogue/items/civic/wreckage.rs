//! Wreckage — a collapsed wall, a fallen beam and scattered rubble. An
//! escalation-Conflict scatter prop: the aftermath of fighting reads the
//! same in any setting (and the escalation finish scorches it further).

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{WOOD_GREY, brick, quat_z, rust_metal, stone, wood};

const RUBBLE: [f32; 3] = [0.40, 0.38, 0.35];
const BRICK: [f32; 3] = [0.52, 0.30, 0.24];

pub struct Wreckage;

impl CatalogueEntry for Wreckage {
    fn slug(&self) -> &'static str {
        "wreckage"
    }
    fn name(&self) -> &'static str {
        "Wreckage"
    }
    fn description(&self) -> &'static str {
        "Collapsed wall, a fallen beam and scattered rubble."
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
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    use std::f32::consts::FRAC_PI_2;
    let mut prims = vec![
        // Flat debris pad — the level root, so the toppled pieces lean off
        // it rather than the whole scene inheriting a tilted root.
        prim(
            solid(cuboid_tapered(
                [1.7, 0.16, 1.4],
                0.2,
                stone([0.33, 0.31, 0.28]),
            )),
            [0.0, 0.08, 0.0],
            id_quat(),
        ),
        // Toppling broken brick wall section, scorched along one edge.
        prim(
            solid(cuboid_tapered([1.3, 1.3, 0.24], 0.0, brick(BRICK))),
            [0.0, 0.78, 0.1],
            quat_z(0.2),
        ),
        prim(
            cuboid_tapered([1.34, 0.3, 0.26], 0.0, stone([0.12, 0.1, 0.09])),
            [-0.16, 1.3, 0.1],
            quat_z(0.2),
        ),
        // Jagged remnant of a second wall.
        prim(
            solid(cuboid_tapered(
                [0.6, 0.85, 0.22],
                0.3,
                brick([0.46, 0.28, 0.22]),
            )),
            [0.95, 0.5, -0.4],
            quat_z(-0.15),
        ),
        // Fallen roof beam lying across the rubble.
        prim(
            solid(cuboid_tapered([0.16, 1.8, 0.16], 0.0, wood(WOOD_GREY))),
            [-0.2, 0.28, 0.5],
            quat_z(FRAC_PI_2 - 0.2),
        ),
    ];

    // Exposed rebar bent out of the snapped concrete top.
    for (dx, lean, twist) in [
        (-0.35_f32, 0.5_f32, 0.2_f32),
        (-0.05, 0.35, -0.3),
        (0.25, 0.6, 0.1),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.02,
                0.6,
                6,
                0.0,
                rust_metal([0.42, 0.24, 0.14]),
            )),
            [dx, 1.4, 0.1],
            quat_mul(quat_z(lean), quat_x(twist)),
        ));
    }

    // Scattered rubble chunks in varied tones.
    for (dx, dy, dz, s, tone) in [
        (-0.7_f32, 0.22_f32, -0.2_f32, 0.34_f32, RUBBLE),
        (0.55, 0.2, 0.55, 0.3, [0.46, 0.3, 0.24]),
        (0.7, 0.18, -0.6, 0.26, [0.42, 0.4, 0.36]),
        (-0.55, 0.18, 0.45, 0.24, BRICK),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([s, s * 0.85, s * 0.9], 0.2, stone(tone))),
            [dx, dy, dz],
            id_quat(),
        ));
    }
    prims.push(prim(
        sphere(0.18, 3, stone([0.3, 0.28, 0.26])),
        [-0.32, 0.2, -0.55],
        id_quat(),
    ));

    super::assemble(prims)
}
