//! Pole barn — a Rural/Farmland *poor* secondary. A cheap open lean-to: a row
//! of poles carrying a sloped corrugated roof over a part-walled back, with a
//! few hay bales stored under it, pitched beside the
//! [`homestead_shack`](super::homestead_shack).

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAY_GOLD, ROOF_GREY, WOOD_GREY, metal_roof, weathered};

pub struct PoleBarn;

impl CatalogueEntry for PoleBarn {
    fn slug(&self) -> &'static str {
        "pole_barn"
    }
    fn name(&self) -> &'static str {
        "Pole Barn"
    }
    fn description(&self) -> &'static str {
        "Open lean-to of poles under a sloped corrugated roof with stored hay."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 8.0_f32;
    let d = 6.0_f32;
    let back_h = 3.8_f32;
    let front_h = 2.8_f32;

    let mut prims = vec![
        // Gravel pad — the root.
        prim(
            solid(cuboid_tapered(
                [w + 0.5, 0.3, d + 0.5],
                0.0,
                weathered([0.42, 0.40, 0.37]),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Back (tall) and front (short) pole rows.
    for sx in [-1.0_f32, 0.0, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, back_h, 0.18],
                0.0,
                weathered(WOOD_GREY),
            )),
            [sx * w * 0.42, 0.3 + back_h * 0.5, -d * 0.5 + 0.3],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, front_h, 0.18],
                0.0,
                weathered(WOOD_GREY),
            )),
            [sx * w * 0.42, 0.3 + front_h * 0.5, d * 0.5 - 0.3],
            id_quat(),
        ));
    }

    // Part-height corrugated back wall.
    prims.push(prim(
        solid(cuboid_tapered(
            [w, back_h, 0.15],
            0.0,
            metal_roof([0.5, 0.5, 0.52]),
        )),
        [0.0, 0.3 + back_h * 0.5, -d * 0.5 + 0.2],
        id_quat(),
    ));

    // Sloped corrugated roof from the back down to the front.
    let mid_y = 0.3 + (back_h + front_h) * 0.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.6, 0.25, d + 0.8],
            0.0,
            metal_roof(ROOF_GREY),
        )),
        [0.0, mid_y + 0.4, 0.0],
        quat_x(0.16),
    ));

    // A few hay bales stored under it: a bottom row of three and one on top.
    let bale = || solid(cuboid_tapered([0.95, 0.55, 0.45], 0.0, weathered(HAY_GOLD)));
    for x in [-2.0_f32, -1.0, 0.0] {
        prims.push(prim(bale(), [x, 0.3 + 0.28, -1.0], id_quat()));
    }
    prims.push(prim(bale(), [-1.5, 0.3 + 0.83, -1.0], id_quat()));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PoleBarn.build(""), "pole_barn");
    }
}
