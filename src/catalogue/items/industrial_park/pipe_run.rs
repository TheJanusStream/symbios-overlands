//! Pipe run — an Industrial-Park prop. A short rack of process pipes on steel
//! trestles, with a hand-wheel valve and a riser elbow.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{PIPE_GREY, tank_steel};

/// Painted pipe liveries.
const PIPE_YELLOW: [f32; 3] = [0.72, 0.60, 0.18];
const PIPE_RUSTRED: [f32; 3] = [0.5, 0.28, 0.2];

pub struct PipeRun;

impl CatalogueEntry for PipeRun {
    fn slug(&self) -> &'static str {
        "pipe_run"
    }
    fn name(&self) -> &'static str {
        "Pipe Run"
    }
    fn description(&self) -> &'static str {
        "Rack of process pipes on steel trestles with a valve and a riser."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 6.0_f32;

    // Steel trestle frame (root) at one end.
    let trestle = |z: f32| -> Vec<Generator> {
        let mut v = Vec::new();
        for sx in [-1.0_f32, 1.0] {
            v.push(prim(
                solid(cuboid_tapered(
                    [0.12, 2.4, 0.12],
                    0.0,
                    tank_steel(PIPE_GREY),
                )),
                [sx * 0.8, 1.2, z],
                id_quat(),
            ));
        }
        v.push(prim(
            solid(cuboid_tapered(
                [1.9, 0.12, 0.12],
                0.0,
                tank_steel(PIPE_GREY),
            )),
            [0.0, 2.3, z],
            id_quat(),
        ));
        v
    };

    let mut prims = trestle(-span * 0.4);
    prims.extend(trestle(span * 0.4));

    // Three pipes running along Z on the trestles.
    let pipes = [
        (-0.5_f32, 1.6_f32, 0.22_f32, PIPE_YELLOW),
        (0.0, 2.1, 0.26, PIPE_GREY),
        (0.5, 2.1, 0.2, PIPE_RUSTRED),
    ];
    for (x, y, r, color) in pipes {
        prims.push(prim(
            solid(cylinder_tapered(r, span, 12, 0.0, tank_steel(color))),
            [x, y, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // Riser elbow off the middle pipe with a hand-wheel valve.
    prims.push(prim(
        solid(cylinder_tapered(0.24, 1.4, 12, 0.0, tank_steel(PIPE_GREY))),
        [0.0, 2.8, 1.5],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.06, 0.32, tank_steel([0.7, 0.2, 0.16])),
        [0.0, 3.5, 1.5],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PipeRun.build(""), "pipe_run");
    }
}
