//! Sandbag wall — a staggered, stacked-bag emplacement. An
//! escalation-Conflict scatter prop: improvised fortification reads the same
//! across every setting.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid, sphere, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{SANDBAG, WOOD, bronze, cloth, wood};

pub struct SandbagWall;

impl CatalogueEntry for SandbagWall {
    fn slug(&self) -> &'static str {
        "sandbag_wall"
    }
    fn name(&self) -> &'static str {
        "Sandbag Wall"
    }
    fn description(&self) -> &'static str {
        "Staggered courses of stacked sandbags."
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
    let bag_w = 0.5;
    let bag_h = 0.24;
    let bag_d = 0.34;
    // Three close sandbag tones so the stack reads as individual filled bags.
    let tone = |i: usize| match i % 3 {
        0 => cloth(SANDBAG),
        1 => cloth([0.56, 0.49, 0.33]),
        _ => cloth([0.66, 0.58, 0.42]),
    };
    let bag = |i: usize| solid(cuboid_tapered([bag_w, bag_h, bag_d], 0.18, tone(i)));

    // Four staggered courses, narrowing toward the top with a firing gap
    // left in the centre of the top course. Courses overlap a touch in Y so
    // no two bag faces sit flush.
    let courses: [&[f32]; 4] = [
        &[-1.0, -0.5, 0.0, 0.5, 1.0],
        &[-0.75, -0.25, 0.25, 0.75],
        &[-0.5, 0.0, 0.5],
        &[-0.55, 0.55],
    ];

    let mut bags = Vec::new();
    let mut i = 0usize;
    for (row, xs) in courses.iter().enumerate() {
        let y = (bag_h - 0.04) * row as f32 + bag_h * 0.5;
        for &x in *xs {
            // Every third bag is laid as a header (turned 90°) so the
            // coursing reads as a woven stack, not a row of identical slabs.
            let rot = if i % 3 == 2 {
                quat_y(std::f32::consts::FRAC_PI_2)
            } else {
                id_quat()
            };
            bags.push(prim(bag(i), [x, y, 0.0], rot));
            i += 1;
        }
    }

    // A rope tie cinching one of the top bags.
    bags.push(prim(
        solid(with_cut(
            cylinder_tapered(0.27, 0.05, 12, 0.0, wood([0.32, 0.26, 0.16])),
            [0.0, 1.0],
            [0.0, 1.0],
            0.7,
        )),
        [-0.55, (bag_h - 0.04) * 3.0 + bag_h * 0.5, 0.0],
        quat_y(std::f32::consts::FRAC_PI_2),
    ));

    // A wooden ammo crate set beside the wall.
    bags.push(prim(
        solid(cuboid_tapered([0.46, 0.3, 0.32], 0.0, wood(WOOD))),
        [1.25, 0.15, 0.18],
        id_quat(),
    ));
    bags.push(prim(
        solid(cuboid_tapered(
            [0.48, 0.05, 0.34],
            0.0,
            wood([0.3, 0.22, 0.13]),
        )),
        [1.25, 0.32, 0.18],
        id_quat(),
    ));

    // A steel helmet hung on the firing step (a domed upper hemisphere).
    bags.push(prim(
        solid(with_cut(
            sphere(0.16, 6, bronze([0.22, 0.25, 0.2])),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, (bag_h - 0.04) * 2.0 + bag_h + 0.1, 0.05],
        id_quat(),
    ));
    bags.push(prim(
        cylinder_tapered(0.21, 0.03, 12, 0.0, bronze([0.2, 0.23, 0.18])),
        [0.0, (bag_h - 0.04) * 2.0 + bag_h + 0.08, 0.05],
        id_quat(),
    ));

    super::assemble(bags)
}
