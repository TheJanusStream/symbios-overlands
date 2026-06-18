//! Bell tower — a Gothic-Horror secondary. A tall dark stone campanile with
//! louvered belfry openings, a hung bronze bell, a lit lancet low on the shaft
//! and a steep pinnacle. A cold wind keens through it. Its window is emissive
//! trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STAINED_TINT, STONE_DARK, fx, iron, stained, stone};

pub struct BellTower;

impl CatalogueEntry for BellTower {
    fn slug(&self) -> &'static str {
        "bell_tower"
    }
    fn name(&self) -> &'static str {
        "Bell Tower"
    }
    fn description(&self) -> &'static str {
        "Dark stone campanile with louvered belfry, a hung bronze bell and a steep pinnacle."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.6_f32;
    let shaft_h = 11.0_f32;
    let shaft_top = base_h + shaft_h;
    let belfry_y = shaft_top - 1.6;

    let mut prims = vec![
        // Stone base — the root.
        prim(
            solid(cuboid_tapered([3.6, base_h, 3.6], 0.0, stone(STONE_DARK))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Stone shaft.
    prims.push(prim(
        solid(cuboid_tapered([2.8, shaft_h, 2.8], 0.03, stone(STONE_DARK))),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));

    // Louvered belfry openings (dark iron) on the four faces.
    for (dx, dz) in [(0.0_f32, 1.45_f32), (0.0, -1.45), (1.45, 0.0), (-1.45, 0.0)] {
        let (sx, sz) = if dx == 0.0 { (1.2, 0.12) } else { (0.12, 1.2) };
        prims.push(prim(
            cuboid_tapered([sx, 1.6, sz], 0.0, iron(IRON_BLACK)),
            [dx, belfry_y, dz],
            id_quat(),
        ));
    }

    // Hung bronze bell in the belfry.
    prims.push(prim(
        solid(cylinder_tapered(
            0.55,
            0.9,
            10,
            0.5,
            iron([0.45, 0.35, 0.18]),
        )),
        [0.0, belfry_y - 0.3, 0.0],
        id_quat(),
    ));

    // Lit lancet window low on the shaft — emissive.
    prims.push(prim(
        cuboid_tapered([0.7, 2.0, 0.18], 0.0, stained(STAINED_TINT, 2.0)),
        [0.0, base_h + 2.2, 1.42],
        id_quat(),
    ));

    // Steep pinnacle roof + corner spikes.
    prims.push(prim(
        solid(cone(2.1, 4.0, 8, stone(STONE_DARK))),
        [0.0, shaft_top + 2.0, 0.0],
        id_quat(),
    ));
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cone(0.3, 1.4, 6, stone(STONE_DARK))),
            [sx * 1.3, shaft_top + 0.7, sz * 1.3],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: a cold wind keening through the belfry.
    root.audio = fx::cold_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BellTower.build(""), "bell_tower");
    }
}
