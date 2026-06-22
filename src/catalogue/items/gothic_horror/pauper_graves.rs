//! Pauper's graves — a Gothic-Horror *poor* secondary. A cluster of crude
//! wooden grave markers leaning over bare dirt mounds, a rough cross at the
//! head. The unmarked burials of the forsaken ground.
//!
//! Markers lean with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE, DEADWOOD, matte, wood};

/// Bare turned-earth brown of the grave mounds.
const DIRT: [f32; 3] = [0.32, 0.26, 0.20];

pub struct PauperGraves;

impl CatalogueEntry for PauperGraves {
    fn slug(&self) -> &'static str {
        "pauper_graves"
    }
    fn name(&self) -> &'static str {
        "Pauper's Graves"
    }
    fn description(&self) -> &'static str {
        "Cluster of crude wooden grave markers leaning over bare dirt mounds."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let dirt = || matte(DIRT);
    let dw = || wood(DEADWOOD);
    let mut prims = vec![
        // A rounded dirt mound — the root.
        prim(
            solid(cuboid_tapered([1.4, 0.34, 0.85], 0.5, dirt())),
            [0.0, 0.16, 0.0],
            id_quat(),
        ),
    ];

    // More rounded mounds in a loose, uneven row.
    for (mx, mz, w, l) in [
        (1.85_f32, 0.3_f32, 1.3_f32, 0.8_f32),
        (-1.7, -0.2, 1.35, 0.78),
        (0.5, 1.9, 1.25, 0.82),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([w, 0.3, l], 0.5, dirt())),
            [mx, 0.15, mz],
            id_quat(),
        ));
    }

    // Crude leaning markers at the -Z head of each mound; some are crosses,
    // some are splintered broken stubs.
    for (i, (gx, gz, h, cross)) in [
        (0.0_f32, -0.55_f32, 0.95_f32, true),
        (1.85, -0.25, 0.6, false),
        (-1.7, -0.75, 0.85, true),
        (0.5, 1.35, 0.5, false),
    ]
    .into_iter()
    .enumerate()
    {
        let tilt = ((i % 3) as f32 - 1.0) * 0.18;
        prims.push(prim(
            solid(cuboid_tapered([0.4, h, 0.1], 0.05, dw())),
            [gx, 0.2 + h * 0.5, gz],
            quat_x(tilt),
        ));
        if cross {
            prims.push(prim(
                solid(cuboid_tapered([0.5, 0.12, 0.1], 0.0, dw())),
                [gx, 0.2 + h * 0.78, gz],
                quat_x(tilt),
            ));
        }
    }

    // A rough wooden cross at the head of the plot.
    prims.push(prim(
        solid(cuboid_tapered([0.14, 1.4, 0.14], 0.0, dw())),
        [-2.4, 0.9, -0.5],
        quat_x(0.16),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.14, 0.14], 0.0, dw())),
        [-2.4, 1.3, -0.4],
        quat_x(0.16),
    ));

    // A gravedigger's spade left stuck in a fresh mound.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 1.1, 6, 0.0, dw())),
        [1.75, 0.75, 0.55],
        quat_x(0.28),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.22, 0.32, 0.04],
            0.12,
            matte([0.32, 0.32, 0.34]),
        )),
        [1.75, 0.26, 0.42],
        quat_x(0.28),
    ));

    // A skull half-sunk in the bare earth.
    prims.push(prim(
        solid(sphere(0.14, 6, matte(BONE))),
        [-0.85, 0.2, 0.7],
        id_quat(),
    ));
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(sphere(0.04, 6, matte([0.1, 0.09, 0.08]))),
            [-0.85 + s * 0.06, 0.21, 0.58],
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
        assert_sanitize_stable(&PauperGraves.build(""), "pauper_graves");
    }
}
