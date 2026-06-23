//! Minivan — a Suburban prop. The family hauler: a tall boxy body with a
//! glazed greenhouse and dark wheels, parked at the kerb.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, enamel, glass};

/// Minivan body colour.
const VAN_BODY: [f32; 3] = [0.36, 0.40, 0.46];
/// Tyre black.
const TIRE: [f32; 3] = [0.06, 0.06, 0.07];

pub struct Minivan;

impl CatalogueEntry for Minivan {
    fn slug(&self) -> &'static str {
        "minivan"
    }
    fn name(&self) -> &'static str {
        "Minivan"
    }
    fn description(&self) -> &'static str {
        "Tall boxy family minivan with a glazed greenhouse, parked at the kerb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Lower body — the root.
        prim(
            solid(cuboid_tapered([4.6, 0.95, 2.0], 0.06, enamel(VAN_BODY))),
            [0.0, 0.78, 0.0],
            id_quat(),
        ),
        // Tall cabin.
        prim(
            solid(cuboid_tapered([3.9, 1.05, 1.92], 0.1, enamel(VAN_BODY))),
            [-0.15, 1.62, 0.0],
            id_quat(),
        ),
        // Glazed greenhouse.
        prim(
            cuboid_tapered([3.7, 0.7, 1.96], 0.1, glass(GLASS_TINT, 0.0)),
            [-0.15, 1.6, 0.0],
            id_quat(),
        ),
    ];

    // Pillars breaking the side glazing into door windows.
    for px in [-1.7_f32, -0.3, 1.1] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.74, 1.98], 0.0, enamel(VAN_BODY))),
            [px, 1.6, 0.0],
            id_quat(),
        ));
    }

    // Front grille flanked by warm headlights.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.16, 0.4, 1.0],
            0.0,
            enamel([0.12, 0.12, 0.14]),
        )),
        [2.32, 0.95, 0.0],
        id_quat(),
    ));
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, 0.22, 0.34], 0.0, glow([1.0, 0.95, 0.8], 2.2)),
            [2.36, 0.95, sz * 0.72],
            id_quat(),
        ));
    }
    // Red tail-lights at the rear.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, 0.22, 0.34], 0.0, glow([0.95, 0.12, 0.1], 2.0)),
            [-2.36, 0.95, sz * 0.72],
            id_quat(),
        ));
    }
    // Dark bumpers front and rear.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.22, 0.3, 1.9],
                0.0,
                enamel([0.2, 0.2, 0.22]),
            )),
            [sx * 2.32, 0.45, 0.0],
            id_quat(),
        ));
    }

    // Four round tyres, axle along Z (round from the side).
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.42, 0.3, 12, 0.0, enamel(TIRE))),
            [sx * 1.5, 0.4, sz * 1.0],
            quat_x(FRAC_PI_2),
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
        assert_sanitize_stable(&Minivan.build(""), "minivan");
    }
}
