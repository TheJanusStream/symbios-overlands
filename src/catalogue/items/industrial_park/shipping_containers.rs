//! Shipping containers — an Industrial-Park prop. A few corrugated steel
//! intermodal containers stacked and offset, in mismatched faded liveries.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONTAINER_BLUE, CONTAINER_GREEN, CONTAINER_RED, CONTAINER_RUST, cladding, tank_steel};

pub struct ShippingContainers;

impl CatalogueEntry for ShippingContainers {
    fn slug(&self) -> &'static str {
        "shipping_containers"
    }
    fn name(&self) -> &'static str {
        "Shipping Containers"
    }
    fn description(&self) -> &'static str {
        "Corrugated steel intermodal containers stacked in mismatched liveries."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 22.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let cw = 6.0_f32;
    let ch = 2.6_f32;
    let cd = 2.5_f32;

    // One container = a corrugated box with a steel door end and corner
    // castings.
    let container = |pos: [f32; 3], color: [f32; 3]| -> Vec<Generator> {
        let mut v = vec![prim(
            solid(cuboid_tapered([cw, ch, cd], 0.0, cladding(color))),
            pos,
            id_quat(),
        )];
        // Door-end vertical bars.
        for bx in [-0.4_f32, 0.4] {
            v.push(prim(
                cuboid_tapered([0.12, ch - 0.2, 0.1], 0.0, tank_steel([0.3, 0.3, 0.32])),
                [pos[0] + cw * 0.5, pos[1], pos[2] + bx],
                id_quat(),
            ));
        }
        v
    };

    let mut prims = Vec::new();
    // Bottom row: two containers side by side.
    prims.extend(container([0.0, ch * 0.5, -1.35], CONTAINER_RED));
    prims.extend(container([0.0, ch * 0.5, 1.35], CONTAINER_BLUE));
    // Top: one offset container.
    prims.extend(container([0.6, ch * 1.5, -0.6], CONTAINER_GREEN));
    // A single end one set apart.
    prims.extend(container([-0.5, ch * 0.5, 4.0], CONTAINER_RUST));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ShippingContainers.build(""), "shipping_containers");
    }
}
