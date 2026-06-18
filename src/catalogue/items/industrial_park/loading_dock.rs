//! Loading dock — an Industrial-Park secondary. A raised concrete dock with
//! roller bay doors and rubber bumpers under a steel canopy, a side ramp, and
//! a box trailer backed up to one bay.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, PIPE_GREY, STEEL_BLUE, cladding, concrete, tank_steel};

pub struct LoadingDock;

impl CatalogueEntry for LoadingDock {
    fn slug(&self) -> &'static str {
        "loading_dock"
    }
    fn name(&self) -> &'static str {
        "Loading Dock"
    }
    fn description(&self) -> &'static str {
        "Raised concrete dock with roller doors, a canopy, a ramp, and a trailer."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 12.0_f32;
    let d = 7.0_f32;
    let dock_h = 1.2;
    let wall_h = 4.5;
    let back = -d * 0.5;

    let mut prims = vec![
        // Raised concrete dock platform — the root.
        prim(
            solid(cuboid_tapered([l, dock_h, d], 0.0, concrete(CONCRETE_GREY))),
            [0.0, dock_h * 0.5, 0.0],
            id_quat(),
        ),
        // Clad back wall.
        prim(
            solid(cuboid_tapered([l, wall_h, 0.4], 0.0, cladding(STEEL_BLUE))),
            [0.0, dock_h + wall_h * 0.5, back + 0.2],
            id_quat(),
        ),
    ];

    // Three roller bay doors and rubber bumpers.
    for bx in [-4.0_f32, 0.0, 4.0] {
        prims.push(prim(
            cuboid_tapered([2.6, 3.2, 0.2], 0.0, cladding([0.46, 0.48, 0.5])),
            [bx, dock_h + 1.6, back + 0.45],
            id_quat(),
        ));
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.25, 0.6, 0.3],
                    0.0,
                    tank_steel([0.12, 0.12, 0.13]),
                )),
                [bx + sx * 1.5, dock_h + 0.3, d * 0.5 - 0.15],
                id_quat(),
            ));
        }
    }

    // Steel canopy over the bays.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.0, 0.3, 2.4],
            0.0,
            tank_steel(PIPE_GREY),
        )),
        [0.0, dock_h + wall_h - 0.4, back + 1.6],
        id_quat(),
    ));

    // Side ramp sloping down to grade.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 0.35, 4.8],
            0.0,
            concrete([0.5, 0.5, 0.51]),
        )),
        [l * 0.5 + 1.3, dock_h * 0.5, 1.0],
        quat_x(0.22),
    ));

    // Box trailer backed up to the centre bay.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 3.0, 6.5],
            0.0,
            cladding([0.7, 0.7, 0.68]),
        )),
        [0.0, 1.7, d * 0.5 + 3.0],
        id_quat(),
    ));
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, 0.7, 0.7],
                0.0,
                tank_steel([0.1, 0.1, 0.11]),
            )),
            [sx * 1.3, 0.35, d * 0.5 + 3.0 + sz * 1.8],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LoadingDock.build(""), "loading_dock");
    }
}
