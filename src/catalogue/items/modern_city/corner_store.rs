//! Corner store — a Modern-City *poor* secondary. A single-storey brick shop
//! with a half-dropped roll shutter, a steel awning, and a tired lit sign
//! over the door. The bodega beside the [`tenement`](super::tenement).

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CAR_GLASS, STEEL_GREY, brick, concrete, glass, steel};

/// Tired warm sign light.
const SIGN_GLOW: [f32; 3] = [1.0, 0.62, 0.30];

pub struct CornerStore;

impl CatalogueEntry for CornerStore {
    fn slug(&self) -> &'static str {
        "corner_store"
    }
    fn name(&self) -> &'static str {
        "Corner Store"
    }
    fn description(&self) -> &'static str {
        "Brick shop with a half-dropped shutter, awning, and a tired lit sign."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_POOR
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
    let d = 7.0_f32;
    let base_h = 0.4;
    let body_h = 4.0;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [w + 0.4, base_h, d + 0.4],
                0.0,
                concrete([0.45, 0.45, 0.46]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Brick body.
        prim(
            solid(cuboid_tapered([w, body_h, d], 0.0, brick(BRICK_RED))),
            [0.0, base_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
        // Parapet.
        prim(
            solid(cuboid_tapered(
                [w + 0.3, 0.6, d + 0.3],
                0.0,
                brick([0.4, 0.22, 0.17]),
            )),
            [0.0, base_h + body_h + 0.3, 0.0],
            id_quat(),
        ),
    ];

    // Storefront glazing and a half-dropped steel roll shutter over it.
    prims.push(prim(
        cuboid_tapered([5.5, 2.2, 0.15], 0.0, glass(CAR_GLASS, 0.6)),
        [0.0, base_h + 1.2, d * 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [5.6, 1.1, 0.18],
            0.0,
            steel([0.5, 0.5, 0.52]),
        )),
        [0.0, base_h + 1.75, d * 0.5 + 0.05],
        id_quat(),
    ));

    // Steel awning over the front.
    prims.push(prim(
        solid(cuboid_tapered([6.4, 0.18, 1.6], 0.0, steel(STEEL_GREY))),
        [0.0, base_h + 2.7, d * 0.5 + 0.8],
        id_quat(),
    ));
    // Tired lit sign band above the awning.
    prims.push(prim(
        cuboid_tapered([4.5, 0.7, 0.15], 0.0, glow(SIGN_GLOW, 2.0)),
        [0.0, base_h + 3.4, d * 0.5 + 0.1],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CornerStore.build(""), "corner_store");
    }
}
