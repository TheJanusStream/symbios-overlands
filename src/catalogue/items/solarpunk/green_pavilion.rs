//! Green-roof pavilion — a Solarpunk secondary. An open timber pavilion under
//! a living turf roof, ringed by crop planters, birdsong in the rafters. The
//! shaded commons of the eco-quarter.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the deck.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, MOSS_GREEN, TIMBER_WARM, concrete, crop_tufts, foliage, fx, timber,
};

pub struct GreenPavilion;

impl CatalogueEntry for GreenPavilion {
    fn slug(&self) -> &'static str {
        "green_pavilion"
    }
    fn name(&self) -> &'static str {
        "Green-Roof Pavilion"
    }
    fn description(&self) -> &'static str {
        "Open timber pavilion under a living turf roof, ringed by crop planters."
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
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let deck_h = 0.3_f32;
    let post_h = 3.0_f32;
    let roof_y = deck_h + post_h;

    let mut prims = vec![
        // Concrete deck — the root.
        prim(
            solid(cuboid_tapered(
                [8.0, deck_h, 6.0],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, deck_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Timber posts.
    for sx in [-1.0_f32, 0.0, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.22, post_h, 0.22],
                    0.0,
                    timber(TIMBER_WARM),
                )),
                [sx * 3.4, deck_h + post_h * 0.5, sz * 2.4],
                id_quat(),
            ));
        }
    }

    // Timber ceiling deck spanning the posts.
    prims.push(prim(
        solid(cuboid_tapered([8.6, 0.3, 6.6], 0.0, timber(TIMBER_WARM))),
        [0.0, roof_y + 0.15, 0.0],
        id_quat(),
    ));
    // Pitched living turf roof (a gentle gable, not a flat slab) — eaves at the
    // ceiling, ridge along X.
    let rise = 1.3_f32;
    prims.push(gable_roof(
        [8.8, rise, 6.8],
        [0.0, roof_y + 0.3 + rise * 0.5, 0.0],
        foliage(MOSS_GREEN),
    ));
    // Timber ridge beam riding the apex.
    prims.push(prim(
        solid(cuboid_tapered([8.9, 0.18, 0.22], 0.0, timber(TIMBER_WARM))),
        [0.0, roof_y + 0.3 + rise, 0.0],
        id_quat(),
    ));

    // Crop planters along the two long edges, planted with leafy rows.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.0, 0.6, 5.2], 0.0, timber(TIMBER_WARM))),
            [sx * 3.6, deck_h + 0.3, 0.0],
            id_quat(),
        ));
        prims.extend(crop_tufts(
            [sx * 3.6, deck_h + 0.6, 0.0],
            [0.7, 4.8],
            2,
            8,
            0.55,
            foliage(CROP_GREEN),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: birdsong in the rafters.
    root.audio = fx::birdsong();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GreenPavilion.build(""), "green_pavilion");
    }
}
