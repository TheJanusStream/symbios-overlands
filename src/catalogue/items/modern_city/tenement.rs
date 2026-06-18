//! Tenement — the Modern-City *poor* landmark. A weathered brick walk-up
//! with grimy windows, a steel fire escape zig-zagging up the street face,
//! and a rooftop water tank. The inner-city counterpart to the
//! [`glass_skyscraper`](super::glass_skyscraper): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows this instead of
//! the corporate tower.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CAR_GLASS, brick, concrete, glass, steel};

pub struct Tenement;

impl CatalogueEntry for Tenement {
    fn slug(&self) -> &'static str {
        "tenement"
    }
    fn name(&self) -> &'static str {
        "Tenement"
    }
    fn description(&self) -> &'static str {
        "Weathered brick walk-up with a steel fire escape and a rooftop water tank."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 12.0_f32;
    let d = 9.0_f32;
    let base_h = 0.5;
    let body_h = 16.0;
    let floors = 5;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [w + 0.6, base_h, d + 0.6],
                0.0,
                concrete([0.42, 0.42, 0.43]),
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
    ];

    // Grid of grimy windows on the street face (+Z).
    let cols = 4;
    for f in 0..floors {
        let y = base_h + 1.8 + f as f32 * (body_h - 2.5) / floors as f32;
        for c in 0..cols {
            let x = -w * 0.5 + 1.8 + c as f32 * (w - 3.6) / (cols - 1) as f32;
            prims.push(prim(
                cuboid_tapered([1.1, 1.4, 0.2], 0.0, glass(CAR_GLASS, 0.0)),
                [x, y, d * 0.5],
                id_quat(),
            ));
        }
    }

    // Steel fire escape: a stack of landings and rails on the front.
    for f in 1..floors {
        let y = base_h + 1.1 + f as f32 * (body_h - 2.5) / floors as f32;
        prims.push(prim(
            solid(cuboid_tapered(
                [5.0, 0.12, 1.4],
                0.0,
                steel([0.34, 0.30, 0.28]),
            )),
            [-1.0, y, d * 0.5 + 0.8],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([5.0, 0.9, 0.08], 0.0, steel([0.34, 0.30, 0.28])),
            [-1.0, y + 0.5, d * 0.5 + 1.45],
            id_quat(),
        ));
    }
    // Vertical fire-escape ladders linking the landings.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.12, body_h - 2.0, 0.12],
            0.0,
            steel([0.34, 0.30, 0.28]),
        )),
        [-3.2, base_h + body_h * 0.5, d * 0.5 + 1.45],
        id_quat(),
    ));

    // Parapet and rooftop water tank on legs.
    let roof_y = base_h + body_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.3, 0.6, d + 0.3],
            0.0,
            brick(BRICK_RED),
        )),
        [0.0, roof_y + 0.3, 0.0],
        id_quat(),
    ));
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, 1.2, 0.18],
                0.0,
                steel([0.3, 0.26, 0.22]),
            )),
            [3.0 + sx * 0.9, roof_y + 1.2, sz * 0.9],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(
            1.1,
            2.2,
            12,
            0.12,
            steel([0.32, 0.26, 0.2]),
        )),
        [3.0, roof_y + 2.9, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            1.1,
            0.9,
            12,
            0.8,
            steel([0.3, 0.24, 0.18]),
        )),
        [3.0, roof_y + 4.4, 0.0],
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
        assert_sanitize_stable(&Tenement.build(""), "tenement");
    }
}
