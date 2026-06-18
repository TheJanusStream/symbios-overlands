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

use super::{CONCRETE_PALE, CROP_GREEN, GLASS_CLEAN, GROW_PINK, concrete, foliage, glass};

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
    // Roof cap.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 0.4, 3.0],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, core_top + 0.2, 0.0],
        id_quat(),
    ));

    // Stacked planted terraces on the +Z front, each with a grow-light.
    for k in 0..4 {
        let y = base_h + 1.4 + k as f32 * 1.9;
        // Terrace shelf.
        prims.push(prim(
            solid(cuboid_tapered(
                [4.6, 0.3, 1.3],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, y, 1.6],
            id_quat(),
        ));
        // Crops on the shelf.
        prims.push(prim(
            solid(cuboid_tapered([4.2, 0.5, 1.0], 0.0, foliage(CROP_GREEN))),
            [0.0, y + 0.35, 1.6],
            id_quat(),
        ));
        // Glass front over the terrace.
        prims.push(prim(
            cuboid_tapered([4.4, 1.4, 0.12], 0.0, glass(GLASS_CLEAN, 1.0)),
            [0.0, y + 0.9, 2.2],
            id_quat(),
        ));
        // Grow-light strip under the shelf above — emissive.
        prims.push(prim(
            cuboid_tapered([4.2, 0.12, 0.9], 0.0, glow(GROW_PINK, 2.5)),
            [0.0, y + 1.4, 1.7],
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
