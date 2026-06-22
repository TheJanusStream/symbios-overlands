//! Loading dock — an Industrial-Park secondary. A raised concrete dock with
//! roller bay doors and rubber bumpers under a steel canopy, a side ramp, and
//! a box trailer backed up to one bay.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, cladding, concrete, tank_steel};

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
    // Working face (roller doors, trucks) on -Z — the render hero front.
    let face = -d * 0.5;

    let mut prims = vec![
        // Raised concrete dock platform — the root.
        prim(
            solid(cuboid_tapered([l, dock_h, d], 0.0, concrete(CONCRETE_GREY))),
            [0.0, dock_h * 0.5, 0.0],
            id_quat(),
        ),
        // Clad wall holding the doors, just inside the -Z edge.
        prim(
            solid(cuboid_tapered([l, wall_h, 0.4], 0.0, cladding(STEEL_BLUE))),
            [0.0, dock_h + wall_h * 0.5, face + 0.25],
            id_quat(),
        ),
    ];

    // Three roller bay doors facing -Z, each with slat ribs and a lit dock
    // lamp hooded above it.
    for bx in [-4.0_f32, 0.0, 4.0] {
        prims.push(prim(
            cuboid_tapered([2.6, 3.2, 0.2], 0.0, cladding([0.44, 0.46, 0.48])),
            [bx, dock_h + 1.6, face - 0.02],
            id_quat(),
        ));
        // Roller-shutter slat ribs.
        for r in 0..4 {
            prims.push(prim(
                cuboid_tapered([2.5, 0.05, 0.06], 0.0, tank_steel([0.28, 0.3, 0.32])),
                [bx, dock_h + 0.5 + r as f32 * 0.75, face - 0.14],
                id_quat(),
            ));
        }
        // Rubber bumpers either side of the bay at the dock lip.
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.25, 0.6, 0.3],
                    0.0,
                    tank_steel([0.12, 0.12, 0.13]),
                )),
                [bx + sx * 1.5, dock_h + 0.3, face + 0.1],
                id_quat(),
            ));
        }
        // Hooded dock lamp glowing just above the door (clear of the canopy
        // above, so the amber reads on the hero front).
        prims.push(prim(
            solid(cuboid_tapered(
                [0.7, 0.12, 0.35],
                0.0,
                tank_steel([0.15, 0.15, 0.17]),
            )),
            [bx, dock_h + 3.4, face - 0.3],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.52, 0.2, 0.06], 0.0, glow(LAMP_AMBER, 3.8)),
            [bx, dock_h + 3.28, face - 0.36],
            id_quat(),
        ));
    }

    // Steel canopy projecting out over the trucks.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.0, 0.3, 2.4],
            0.0,
            tank_steel(PIPE_GREY),
        )),
        [0.0, dock_h + wall_h - 0.4, face - 1.3],
        id_quat(),
    ));

    // Side ramp sloping down to grade.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 0.35, 4.8],
            0.0,
            concrete([0.5, 0.5, 0.51]),
        )),
        [l * 0.5 + 1.3, dock_h * 0.5, -1.0],
        quat_x(-0.22),
    ));

    // Box trailer backed up to the left bay (leaves the centre/right doors
    // reading on the hero front).
    let tx = -4.0_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 3.0, 6.2],
            0.0,
            cladding([0.7, 0.7, 0.68]),
        )),
        [tx, 1.7, face - 3.4],
        id_quat(),
    ));
    // Door seam + handles on the trailer's -Z rear.
    for sx in [-0.6_f32, 0.6] {
        prims.push(prim(
            cuboid_tapered([0.1, 2.6, 0.1], 0.0, tank_steel([0.4, 0.4, 0.42])),
            [tx + sx, 1.7, face - 6.5],
            id_quat(),
        ));
    }
    // Wheels.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, 0.7, 0.7],
                0.0,
                tank_steel([0.1, 0.1, 0.11]),
            )),
            [tx + sx * 1.3, 0.35, face - 3.4 + sz * 1.8],
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
