//! Tenement — the Modern-City *poor* landmark. A weathered brick walk-up
//! with grimy windows, a steel fire escape zig-zagging up the street face,
//! and a rooftop water tank. The inner-city counterpart to the
//! [`glass_skyscraper`](super::glass_skyscraper): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows this instead of
//! the corporate tower.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRICK_RED, CAR_GLASS, LAMP_WARM, brick, concrete, glass, steel};

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

    // The −Z render front is the street face — windows, fire escape, stoop.
    let front_z = -d * 0.5;
    let fe_mat = || steel([0.32, 0.28, 0.26]);

    // Grid of grimy windows with brick sills; a few warm-lit at dusk.
    let cols = 4;
    for f in 0..floors {
        let y = base_h + 1.8 + f as f32 * (body_h - 2.5) / floors as f32;
        for c in 0..cols {
            let x = -w * 0.5 + 1.8 + c as f32 * (w - 3.6) / (cols - 1) as f32;
            // A handful of lit windows for inhabited life; the rest dark glass.
            let lit = (f + c) % 5 == 1;
            let pane = if lit {
                glow([1.0, 0.78, 0.45], 1.3)
            } else {
                glass(CAR_GLASS, 0.0)
            };
            prims.push(prim(
                cuboid_tapered([1.1, 1.4, 0.2], 0.0, pane),
                [x, y, front_z - 0.05],
                id_quat(),
            ));
            // Proud brick sill under each window.
            prims.push(prim(
                cuboid_tapered([1.3, 0.18, 0.3], 0.0, brick([0.4, 0.22, 0.17])),
                [x, y - 0.8, front_z - 0.12],
                id_quat(),
            ));
        }
    }

    // Steel fire escape on the left bays: landings, railings, side rails,
    // zig-zag stairs between floors, and a drop ladder at the bottom.
    let fe_x = -2.4_f32;
    let fe_w = 5.0_f32;
    let land_z = front_z - 0.75;
    let rail_z = front_z - 1.32;
    let floor_y = |f: i32| base_h + 1.1 + f as f32 * (body_h - 2.5) / floors as f32;
    for f in 1..floors {
        let y = floor_y(f);
        // Grated landing platform.
        prims.push(prim(
            solid(cuboid_tapered([fe_w, 0.12, 1.3], 0.0, fe_mat())),
            [fe_x, y, land_z],
            id_quat(),
        ));
        // Outer railing.
        prims.push(prim(
            cuboid_tapered([fe_w, 0.8, 0.08], 0.0, fe_mat()),
            [fe_x, y + 0.45, rail_z],
            id_quat(),
        ));
        // Diagonal stair stringer down to the floor below, alternating side.
        let dir = if f % 2 == 0 { 1.0 } else { -1.0 };
        let y_lo = floor_y(f - 1);
        let run = fe_w * 0.7;
        let rise = y - y_lo;
        let stair_len = (run * run + rise * rise).sqrt();
        let angle = rise.atan2(run); // tilt of the stringer
        prims.push(prim(
            cuboid_tapered([stair_len, 0.1, 0.55], 0.0, fe_mat()),
            [fe_x + dir * run * 0.1, (y + y_lo) * 0.5, land_z],
            quat_z(dir * angle),
        ));
    }
    // Two vertical side rails carrying the whole assembly.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, body_h - 2.0, 0.12], 0.0, fe_mat())),
            [fe_x + sx * fe_w * 0.5, base_h + body_h * 0.5, rail_z],
            id_quat(),
        ));
    }
    // Drop ladder hanging below the bottom landing.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 2.6, 0.1], 0.0, fe_mat())),
        [fe_x + fe_w * 0.3, floor_y(1) - 1.5, land_z],
        id_quat(),
    ));

    // Stoop entrance at the centre of the ground floor.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.6, 1.0, 1.4],
            0.0,
            concrete([0.5, 0.5, 0.5]),
        )),
        [2.6, base_h + 0.5, front_z - 0.7],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.5, 2.4, 0.4],
            0.0,
            steel([0.2, 0.16, 0.14]),
        )),
        [2.6, base_h + 1.7, front_z + 0.05],
        id_quat(),
    ));
    // Tired warm stoop light over the door.
    prims.push(prim(
        sphere(0.18, 3, glow(LAMP_WARM, 1.6)),
        [2.6, base_h + 3.0, front_z - 0.2],
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
