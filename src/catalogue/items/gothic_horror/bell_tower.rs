//! Bell tower — a Gothic-Horror secondary. A tall dark stone campanile with
//! louvered belfry openings, a hung bronze bell, a lit lancet low on the shaft
//! and a steep pinnacle. A cold wind keens through it. Its window is emissive
//! trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STONE_DARK, fx, iron, lancet, pointed_arch, spire, stone};

pub struct BellTower;

impl CatalogueEntry for BellTower {
    fn slug(&self) -> &'static str {
        "bell_tower"
    }
    fn name(&self) -> &'static str {
        "Bell Tower"
    }
    fn description(&self) -> &'static str {
        "Dark stone campanile with louvered belfry, a hung bronze bell and a steep pinnacle."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.7_f32;
    let floor_y = 9.0_f32; // belfry floor (top of the solid shaft)
    let belfry_h = 2.6_f32; // open belfry stage
    let cornice_y = floor_y + belfry_h;
    let shaft_h = floor_y - base_h;
    let st = || stone(STONE_DARK);
    let bronze = || iron([0.46, 0.34, 0.16]);

    let mut prims = vec![
        // Stepped stone base — the root.
        prim(
            solid(cuboid_tapered([3.8, base_h, 3.8], 0.0, st())),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Plinth step.
    prims.push(prim(
        solid(cuboid_tapered([3.2, 0.4, 3.2], 0.0, st())),
        [0.0, base_h + 0.2, 0.0],
        id_quat(),
    ));

    // Solid stone shaft up to the belfry floor.
    prims.push(prim(
        solid(cuboid_tapered([2.8, shaft_h, 2.8], 0.03, st())),
        [0.0, base_h + shaft_h * 0.5, 0.0],
        id_quat(),
    ));

    // Corner colonnettes (clasping buttress shafts) up the edges.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.16, shaft_h, 8, 0.02, st())),
            [sx * 1.45, base_h + shaft_h * 0.5, sz * 1.45],
            id_quat(),
        ));
    }

    // String courses banding the shaft.
    for y in [5.2_f32, 7.6] {
        prims.push(prim(
            solid(cuboid_tapered([3.0, 0.22, 3.0], 0.0, st())),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Lit lancet low on the -Z hero face.
    prims.extend(lancet(0.0, base_h + 1.7, -1.42, 0.42, 1.7, 2.0));

    // ---- Open belfry stage: corner piers framing a visible hung bell. ----
    // Belfry floor slab.
    prims.push(prim(
        solid(cuboid_tapered([2.8, 0.25, 2.8], 0.0, st())),
        [0.0, floor_y + 0.12, 0.0],
        id_quat(),
    ));
    // Four corner piers.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.55, belfry_h, 0.55], 0.0, st())),
            [sx * 1.12, floor_y + belfry_h * 0.5, sz * 1.12],
            id_quat(),
        ));
    }
    // Pointed-arch heads over the front (-Z) and back (+Z) openings.
    for nz in [-1.0_f32, 1.0] {
        prims.extend(pointed_arch(
            [0.0, floor_y + 0.95, nz * 1.18],
            0.82,
            0.11,
            st(),
        ));
    }
    // Hung bronze bell in the open centre, visible through every arch.
    {
        let by = floor_y + belfry_h * 0.46;
        // Yoke beam across the belfry, carried on the piers.
        prims.push(prim(
            solid(cuboid_tapered([2.4, 0.2, 0.24], 0.0, iron(IRON_BLACK))),
            [0.0, floor_y + belfry_h * 0.92, 0.0],
            id_quat(),
        ));
        // Headstock + hanger strap.
        prims.push(prim(
            solid(cuboid_tapered([0.46, 0.24, 0.24], 0.0, bronze())),
            [0.0, by + 0.62, 0.0],
            id_quat(),
        ));
        // Bell skirt (flared frustum, mouth down).
        prims.push(prim(
            solid(cylinder_tapered(0.5, 0.9, 16, 0.42, bronze())),
            [0.0, by, 0.0],
            id_quat(),
        ));
        // Domed crown.
        prims.push(prim(
            solid(with_cut(
                sphere(0.31, 6, bronze()),
                [0.0, 1.0],
                [0.5, 1.0],
                0.0,
            )),
            [0.0, by + 0.45, 0.0],
            id_quat(),
        ));
        // Sound-bow mouth ring.
        prims.push(prim(
            solid(torus(0.06, 0.49, bronze())),
            [0.0, by - 0.45, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // Belfry cornice.
    prims.push(prim(
        solid(cuboid_tapered([2.9, 0.3, 2.9], 0.0, st())),
        [0.0, cornice_y + 0.12, 0.0],
        id_quat(),
    ));

    // Tall broach spire + four corner pinnacles.
    prims.extend(spire([0.0, cornice_y + 0.27, 0.0], 1.45, 4.4, st()));
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.extend(spire(
            [sx * 1.12, cornice_y + 0.27, sz * 1.12],
            0.3,
            1.5,
            st(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: a cold wind keening through the belfry.
    root.audio = fx::cold_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BellTower.build(""), "bell_tower");
    }
}
