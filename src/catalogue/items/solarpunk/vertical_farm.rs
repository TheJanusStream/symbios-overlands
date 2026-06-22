//! Vertical farm — a Solarpunk secondary. A concrete core tower hung with
//! stacked planted terraces behind glass, each shelf lit pink by grow-lights.
//! The food tower of the eco-quarter; its grow-lights are emissive trim the
//! ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the core base.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, GLASS_CLEAN, GROW_PINK, LEAF_GREEN, concrete, crop_tufts, foliage,
    glass,
};

pub struct VerticalFarm;

impl CatalogueEntry for VerticalFarm {
    fn slug(&self) -> &'static str {
        "vertical_farm"
    }
    fn name(&self) -> &'static str {
        "Vertical Farm"
    }
    fn description(&self) -> &'static str {
        "Concrete tower of stacked planted terraces behind glass, lit by grow-lights."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.4_f32;
    let core_h = 9.0_f32;
    let core_top = base_h + core_h;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [4.5, base_h, 4.5],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Concrete core column.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.6, core_h, 2.6],
            0.02,
            concrete(CONCRETE_PALE),
        )),
        [0.0, base_h + core_h * 0.5, 0.0],
        id_quat(),
    ));
    // Roof cap + a green roof-garden crown.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 0.4, 3.0],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, core_top + 0.2, 0.0],
        id_quat(),
    ));
    prims.extend(crop_tufts(
        [0.0, core_top + 0.4, 0.0],
        [2.4, 2.4],
        4,
        4,
        0.55,
        foliage(CROP_GREEN),
    ));

    // Climbing-green facade up the ±X side faces — living curtain of vines.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, core_h * 0.92, 1.9],
                0.0,
                foliage(LEAF_GREEN),
            )),
            [sx * 1.34, base_h + core_h * 0.46, 0.0],
            id_quat(),
        ));
        // Trailing leaf clumps so the curtain reads planted, not a flat slab.
        prims.extend(crop_tufts(
            [sx * 1.4, base_h + core_h * 0.46, 0.0],
            [0.0, 1.6],
            1,
            6,
            0.5,
            foliage(LEAF_GREEN),
        ));
    }

    // Stacked planted terraces on the −Z hero front, each lit by a grow-light.
    let zf = -1.6_f32;
    for k in 0..4 {
        let y = base_h + 1.4 + k as f32 * 1.9;
        // Terrace shelf.
        prims.push(prim(
            solid(cuboid_tapered(
                [4.6, 0.3, 1.3],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, y, zf],
            id_quat(),
        ));
        // Rows of leafy crops on the shelf.
        prims.extend(crop_tufts(
            [0.0, y + 0.15, zf],
            [4.0, 0.9],
            7,
            2,
            0.55,
            foliage(CROP_GREEN),
        ));
        // Glass front over the terrace (proud of the −Z shelf edge).
        prims.push(prim(
            cuboid_tapered([4.4, 1.4, 0.12], 0.0, glass(GLASS_CLEAN, 1.0)),
            [0.0, y + 0.9, zf - 0.62],
            id_quat(),
        ));
        // Grow-light strip under the shelf above — emissive.
        prims.push(prim(
            cuboid_tapered([4.2, 0.12, 0.9], 0.0, glow(GROW_PINK, 2.2)),
            [0.0, y + 1.4, zf - 0.1],
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
        assert_sanitize_stable(&VerticalFarm.build(""), "vertical_farm");
    }
}
