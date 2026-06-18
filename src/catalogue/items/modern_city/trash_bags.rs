//! Trash bags — a Modern-City *poor* prop. A heap of black refuse sacks
//! against a tipped-over steel can with a spill of litter: the alley clutter
//! of the inner city.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STEEL_GREY, enamel, steel};

/// Glossy black bin-bag plastic.
const BAG_BLACK: [f32; 3] = [0.08, 0.08, 0.09];
/// Scattered paper litter.
const LITTER_PALE: [f32; 3] = [0.72, 0.70, 0.64];

pub struct TrashBags;

impl CatalogueEntry for TrashBags {
    fn slug(&self) -> &'static str {
        "trash_bags"
    }
    fn name(&self) -> &'static str {
        "Trash Bags"
    }
    fn description(&self) -> &'static str {
        "Heap of black refuse sacks by a tipped steel can with spilled litter."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let bag = |r: f32| solid(sphere(r, 3, enamel(BAG_BLACK)));

    // Big bag — the root.
    let mut prims = vec![prim(bag(0.5), [0.0, 0.45, 0.0], id_quat())];
    // More bags heaped around it.
    prims.push(prim(bag(0.42), [0.7, 0.38, 0.2], id_quat()));
    prims.push(prim(bag(0.4), [-0.55, 0.36, 0.3], id_quat()));
    prims.push(prim(bag(0.38), [0.2, 0.8, 0.1], id_quat()));

    // Tipped steel can on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.4, 1.0, 12, 0.0, steel(STEEL_GREY))),
        [-1.3, 0.4, -0.3],
        quat_x(FRAC_PI_2),
    ));

    // A spill of pale litter.
    for (x, z) in [(-1.9_f32, -0.2_f32), (-1.7, 0.3), (0.9, -0.5)] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.04, 0.18], 0.0, enamel(LITTER_PALE)),
            [x, 0.04, z],
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
        assert_sanitize_stable(&TrashBags.build(""), "trash_bags");
    }
}
