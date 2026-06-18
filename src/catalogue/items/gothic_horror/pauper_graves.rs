//! Pauper's graves — a Gothic-Horror *poor* secondary. A cluster of crude
//! wooden grave markers leaning over bare dirt mounds, a rough cross at the
//! head. The unmarked burials of the forsaken ground.
//!
//! Markers lean with a [`quat_x`].

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, matte, wood};

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
    let mut prims = vec![
        // A dirt mound — the root.
        prim(
            solid(cuboid_tapered([1.4, 0.3, 0.8], 0.3, matte(DIRT))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // More dirt mounds in a loose row.
    for (mx, mz) in [(1.8_f32, 0.3_f32), (-1.7, -0.2), (0.4, 1.8)] {
        prims.push(prim(
            solid(cuboid_tapered([1.3, 0.28, 0.75], 0.3, matte(DIRT))),
            [mx, 0.14, mz],
            id_quat(),
        ));
    }

    // Crude leaning plank markers at the head of each mound.
    for (i, (gx, gz)) in [(0.0_f32, -0.5_f32), (1.8, -0.2), (-1.7, -0.7), (0.4, 1.3)]
        .into_iter()
        .enumerate()
    {
        let tilt = ((i % 3) as f32 - 1.0) * 0.18;
        prims.push(prim(
            solid(cuboid_tapered([0.4, 0.9, 0.1], 0.0, wood(DEADWOOD))),
            [gx, 0.5, gz],
            quat_x(tilt),
        ));
    }

    // A rough wooden cross at the head of the plot.
    prims.push(prim(
        solid(cuboid_tapered([0.14, 1.4, 0.14], 0.0, wood(DEADWOOD))),
        [-2.4, 0.7, -0.4],
        quat_x(0.15),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.14, 0.14], 0.0, wood(DEADWOOD))),
        [-2.4, 1.1, -0.35],
        quat_x(0.15),
    ));

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
