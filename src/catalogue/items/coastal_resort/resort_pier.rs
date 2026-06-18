//! Resort pier — a Coastal-Resort secondary. A long timber-plank deck on
//! concrete pilings striding out over the surf to a canvas-roofed pavilion
//! at its head, railed the whole way. Sea spray flings up off the end
//! pilings and a slow surf wash rolls under the deck, the signature life of
//! the seafront.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the deck.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_TEAL, AWNING_WHITE, DECK_PALE, PILING_GREY, STEEL_GREY, canvas, concrete, fx, plank,
    steel,
};

pub struct ResortPier;

impl CatalogueEntry for ResortPier {
    fn slug(&self) -> &'static str {
        "resort_pier"
    }
    fn name(&self) -> &'static str {
        "Resort Pier"
    }
    fn description(&self) -> &'static str {
        "Long plank deck on concrete pilings reaching out to a canvas-roofed pavilion."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_y = 2.0_f32;
    let length = 24.0_f32;
    let z0 = -1.0_f32; // shore end
    let center_z = z0 + length * 0.5;

    let mut prims = vec![
        // Plank deck — the root, raised on the pilings.
        prim(
            solid(cuboid_tapered([4.0, 0.3, length], 0.0, plank(DECK_PALE))),
            [0.0, deck_y, center_z],
            id_quat(),
        ),
    ];

    // Concrete pilings in pairs marching out under the deck.
    for k in 0..7 {
        let z = z0 + 1.0 + k as f32 * 3.6;
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cylinder_tapered(
                    0.35,
                    deck_y,
                    10,
                    0.05,
                    concrete(PILING_GREY),
                )),
                [sx * 1.6, deck_y * 0.5, z],
                id_quat(),
            ));
        }
    }

    // Side railings: a top rail and regular posts down both edges.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.12, 0.1, length], 0.0, steel(STEEL_GREY)),
            [sx * 1.95, deck_y + 0.95, center_z],
            id_quat(),
        ));
        for k in 0..7 {
            let z = z0 + 1.0 + k as f32 * 3.6;
            prims.push(prim(
                solid(cuboid_tapered([0.1, 1.0, 0.1], 0.0, steel(STEEL_GREY))),
                [sx * 1.95, deck_y + 0.5, z],
                id_quat(),
            ));
        }
    }

    // Pavilion at the head: a wider platform, four posts, a canvas roof.
    let pav_z = z0 + length - 2.5;
    prims.push(prim(
        solid(cuboid_tapered([5.0, 0.3, 5.0], 0.0, plank(DECK_PALE))),
        [0.0, deck_y, pav_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.14, 3.0, 0.14], 0.0, steel(STEEL_GREY))),
                [sx * 2.2, deck_y + 1.5, pav_z + sz * 2.2],
                id_quat(),
            ));
        }
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [5.6, 0.25, 5.6],
            0.1,
            canvas(AWNING_TEAL, AWNING_WHITE),
        )),
        [0.0, deck_y + 3.1, pav_z],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: surf wash under the deck, sea spray off the head.
    root.audio = fx::surf_wash();
    root.children
        .push(fx::sea_mist([0.0, deck_y - 0.4, z0 + length], 0x05EA_1DE0));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ResortPier.build(""), "resort_pier");
    }
}
