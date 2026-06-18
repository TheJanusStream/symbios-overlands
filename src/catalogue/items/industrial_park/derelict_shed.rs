//! Derelict shed — the Industrial-Park *poor* landmark. An abandoned
//! corrugated works: rust-streaked walls with a panel torn out, a sagging
//! half-collapsed roof, and a leaning vent. The derelict counterpart to the
//! [`factory`](super::factory): same theme, opposite end of the prosperity
//! axis (`Poor`), so a destitute room grows this instead of the working
//! plant.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUST_BROWN, concrete, rust};

pub struct DerelictShed;

impl CatalogueEntry for DerelictShed {
    fn slug(&self) -> &'static str {
        "derelict_shed"
    }
    fn name(&self) -> &'static str {
        "Derelict Shed"
    }
    fn description(&self) -> &'static str {
        "Abandoned corrugated works with torn-out panels and a collapsing roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 12.0_f32;
    let w = 8.0_f32;
    let foot_h = 0.4;
    let wall_h = 5.0;
    let wall_top = foot_h + wall_h;

    let mut prims = vec![
        // Cracked concrete slab — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, foot_h, w + 0.6],
                0.0,
                concrete([0.46, 0.46, 0.47]),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Three standing rust walls (the fourth, front, is left open/torn out).
    prims.push(prim(
        solid(cuboid_tapered([l, wall_h, 0.35], 0.0, rust(RUST_BROWN))),
        [0.0, foot_h + wall_h * 0.5, -(w * 0.5 - 0.18)],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.35, wall_h, w], 0.0, rust(RUST_BROWN))),
            [sx * (l * 0.5 - 0.18), foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    // A torn-out gap (dark) in one side wall.
    prims.push(prim(
        cuboid_tapered([0.4, 2.4, 2.0], 0.0, concrete([0.08, 0.08, 0.09])),
        [-(l * 0.5 - 0.18), foot_h + 1.4, 1.0],
        id_quat(),
    ));
    // A partial low front wall (the rest collapsed).
    prims.push(prim(
        solid(cuboid_tapered([4.0, 2.2, 0.3], 0.0, rust(RUST_BROWN))),
        [-l * 0.5 + 2.5, foot_h + 1.1, w * 0.5 - 0.15],
        id_quat(),
    ));

    // Sagging half-collapsed roof: one tilted slab dropping to the open front.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 1.0, 0.3, w + 1.0],
            0.0,
            rust([0.4, 0.26, 0.15]),
        )),
        [0.0, wall_top - 0.6, 0.0],
        quat_x(0.16),
    ));

    // Leaning vent pipe.
    prims.push(prim(
        solid(cylinder_tapered(
            0.18,
            2.5,
            8,
            0.0,
            rust([0.38, 0.24, 0.14]),
        )),
        [-2.5, wall_top + 0.4, -2.0],
        quat_x(0.25),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DerelictShed.build(""), "derelict_shed");
    }
}
