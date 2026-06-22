//! Factory — the Industrial-Park landmark. A long steel-clad works on a brick
//! base, with a roof monitor of grimy clerestory glass, three loading bays, a
//! lit window band, and a tall brick smokestack pouring smoke over a heavy
//! machine hum. It anchors the estate and reads as the plant across the home
//! region.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding,
    concrete, fx, glass, tank_steel,
};

/// Ochre process pipework on the external gantry.
const PIPE_OCHRE: [f32; 3] = [0.62, 0.5, 0.2];

pub struct Factory;

impl CatalogueEntry for Factory {
    fn slug(&self) -> &'static str {
        "factory"
    }
    fn name(&self) -> &'static str {
        "Factory"
    }
    fn description(&self) -> &'static str {
        "Steel-clad works with loading bays and a smoking brick chimney."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 16.0,
            min_spawn_dist: 55.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 20.0_f32;
    let w = 12.0_f32;
    let apron_h = 0.5;
    let brick_h = 1.5;
    let clad_h = 6.5;
    let wall_top = apron_h + brick_h + clad_h;
    // Hero face on -Z (the render front): loading bays, lit windows, sign.
    let front = -w * 0.5;

    let mut prims = vec![
        // Concrete apron — the root.
        prim(
            solid(cuboid_tapered(
                [l + 2.0, apron_h, w + 2.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, apron_h * 0.5, 0.0],
            id_quat(),
        ),
        // Brick base course.
        prim(
            solid(cuboid_tapered([l, brick_h, w], 0.0, brick(BRICK_DARK))),
            [0.0, apron_h + brick_h * 0.5, 0.0],
            id_quat(),
        ),
        // Steel-clad upper body.
        prim(
            solid(cuboid_tapered([l, clad_h, w], 0.0, cladding(STEEL_BLUE))),
            [0.0, apron_h + brick_h + clad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Roof monitor with clerestory glass.
    prims.push(prim(
        solid(cuboid_tapered(
            [l - 4.0, 2.5, 4.0],
            0.0,
            cladding(STEEL_BLUE),
        )),
        [0.0, wall_top + 1.25, 0.0],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([l - 5.0, 1.3, 0.1], 0.0, glass(WINDOW_LIT, 1.6)),
            [0.0, wall_top + 1.3, sz * 2.0],
            id_quat(),
        ));
    }

    // Three roller loading bays on the -Z hero front (proud, toward camera).
    for bx in [-6.0_f32, 0.0, 6.0] {
        prims.push(prim(
            cuboid_tapered([3.2, 4.0, 0.2], 0.0, cladding([0.48, 0.5, 0.52])),
            [bx, apron_h + 2.0, front - 0.1],
            id_quat(),
        ));
        // Door track ribs so the bays read as roller shutters.
        for r in 0..4 {
            prims.push(prim(
                cuboid_tapered([3.0, 0.06, 0.06], 0.0, tank_steel([0.3, 0.32, 0.34])),
                [bx, apron_h + 0.8 + r as f32 * 0.9, front - 0.22],
                id_quat(),
            ));
        }
    }
    // Warm lit window band above the bays — deep-amber so it reads as a plant
    // lit at dusk, not a washed near-white panel.
    prims.push(prim(
        cuboid_tapered([l - 2.0, 1.5, 0.25], 0.0, glass(LAMP_AMBER, 2.4)),
        [0.0, apron_h + brick_h + 4.0, front - 0.14],
        id_quat(),
    ));
    // Sign band.
    prims.push(prim(
        solid(cuboid_tapered(
            [8.0, 1.2, 0.3],
            0.0,
            tank_steel([0.7, 0.72, 0.74]),
        )),
        [0.0, wall_top - 0.5, front - 0.18],
        id_quat(),
    ));

    // External pipe gantry climbing the +X gable end — process pipes feeding
    // the plant, the unmistakable works detail.
    let gx = l * 0.5 + 0.4;
    for pz in [-3.6_f32, 0.0, 3.6] {
        prims.push(prim(
            solid(cylinder_tapered(0.16, 5.4, 8, 0.0, tank_steel(PIPE_GREY))),
            [gx, apron_h + 2.7, pz],
            id_quat(),
        ));
    }
    for (py, color) in [(3.4_f32, PIPE_OCHRE), (4.2, PIPE_GREY)] {
        prims.push(prim(
            solid(cylinder_tapered(0.2, w - 0.5, 12, 0.0, tank_steel(color))),
            [gx, py, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }
    // Riser elbow turning up the wall onto the roof.
    prims.push(prim(
        solid(cylinder_tapered(
            0.2,
            wall_top - 3.0,
            12,
            0.0,
            tank_steel(PIPE_OCHRE),
        )),
        [gx - 0.3, wall_top - 1.2, 3.6],
        id_quat(),
    ));

    // Roof vents / extract ducting on the monitor.
    for vx in [-4.5_f32, 4.5] {
        prims.push(prim(
            solid(tube(0.42, 0.3, 1.6, 14, tank_steel(PIPE_GREY))),
            [vx, wall_top + 2.5 + 0.8, 1.0],
            id_quat(),
        ));
    }

    // Tall brick smokestack at the back corner.
    let stack_x = -l * 0.5 + 2.0;
    let stack_z = w * 0.5 - 2.0;
    let stack_h = 17.0;
    prims.push(prim(
        solid(cylinder_tapered(1.3, stack_h, 16, 0.18, brick(BRICK_DARK))),
        [stack_x, apron_h + stack_h * 0.5, stack_z],
        id_quat(),
    ));
    // Steel band near the top.
    prims.push(prim(
        cuboid_tapered([2.2, 0.4, 2.2], 0.0, tank_steel(PIPE_GREY)),
        [stack_x, apron_h + stack_h - 1.5, stack_z],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: smoke from the stack and the plant's heavy hum.
    root.children.push(fx::stack_smoke(
        [stack_x, apron_h + stack_h + 0.5, stack_z],
        0x5AC0_5E11,
    ));
    root.audio = fx::machine_hum();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Factory.build(""), "factory");
    }

    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Factory.build("")
        ));
    }
}
