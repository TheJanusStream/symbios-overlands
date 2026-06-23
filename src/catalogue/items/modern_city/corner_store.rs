//! Corner store — a Modern-City *poor* secondary. A single-storey brick shop
//! with a half-dropped roll shutter, a steel awning, and a tired lit sign
//! over the door. The bodega beside the [`tenement`](super::tenement).

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CAR_GLASS, brick, concrete, enamel, glass, steel};

/// Tired warm sign light — deep-saturated amber so the broad lit band reads as
/// a colour under bloom rather than washing to a near-white blank.
const SIGN_GLOW: [f32; 3] = [1.0, 0.46, 0.13];
/// Awning stripe colours.
const AWNING_RED: [f32; 3] = [0.52, 0.13, 0.12];
const AWNING_CREAM: [f32; 3] = [0.82, 0.78, 0.68];

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

    // The −Z render front is the shopfront.
    let front_z = -d * 0.5;

    // Storefront: a low brick bulkhead, a big mullioned display window, and an
    // off-centre glazed door.
    prims.push(prim(
        solid(cuboid_tapered(
            [6.0, 0.7, 0.3],
            0.0,
            brick([0.4, 0.22, 0.17]),
        )),
        [0.0, base_h + 0.35, front_z - 0.1],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([5.6, 1.9, 0.15], 0.0, glass(CAR_GLASS, 0.7)),
        [0.0, base_h + 1.65, front_z],
        id_quat(),
    ));
    // Display-window mullions + transom.
    for x in [-2.6_f32, -1.0, 1.4, 2.6] {
        prims.push(prim(
            cuboid_tapered([0.14, 1.9, 0.28], 0.0, steel([0.45, 0.45, 0.47])),
            [x, base_h + 1.65, front_z - 0.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([5.7, 0.16, 0.28], 0.0, steel([0.45, 0.45, 0.47])),
        [0.0, base_h + 2.55, front_z - 0.1],
        id_quat(),
    ));
    // Glazed entrance door on the right.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, 2.3, 0.12],
            0.0,
            steel([0.2, 0.2, 0.22]),
        )),
        [2.0, base_h + 1.15, front_z - 0.12],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.8, 1.9, 0.1], 0.0, glass([0.16, 0.2, 0.22], 0.8)),
        [2.0, base_h + 1.2, front_z - 0.2],
        id_quat(),
    ));

    // A half-dropped roll shutter and its housing above the storefront.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.9, 0.4, 0.4],
            0.0,
            steel([0.42, 0.42, 0.44]),
        )),
        [0.0, base_h + 2.75, front_z - 0.18],
        id_quat(),
    ));

    // Striped sloped fabric awning projecting over the front.
    for (i, x) in [-2.4_f32, -1.2, 0.0, 1.2, 2.4].iter().enumerate() {
        let col = if i % 2 == 0 { AWNING_RED } else { AWNING_CREAM };
        prims.push(prim(
            solid(cuboid_tapered([1.2, 0.12, 1.7], 0.0, enamel(col))),
            [*x, base_h + 3.05, front_z - 0.85],
            quat_x(-0.22),
        ));
    }
    // Awning valance lip.
    prims.push(prim(
        solid(cuboid_tapered([6.0, 0.3, 0.1], 0.0, enamel(AWNING_RED))),
        [0.0, base_h + 2.85, front_z - 1.6],
        id_quat(),
    ));

    // Tired lit sign band above the awning, on the parapet.
    prims.push(prim(
        cuboid_tapered([4.6, 0.7, 0.16], 0.0, glow(SIGN_GLOW, 1.5)),
        [0.0, base_h + body_h + 0.1, front_z - 0.18],
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
