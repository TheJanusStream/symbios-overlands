//! Salvage shack — a Post-apocalyptic secondary. A hovel of welded corrugated
//! sheet and salvaged plank under a sagging tarp, a stovepipe leaking smoke
//! and a dim-lit window. The shelter of the holdout; its window is emissive
//! trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_y, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, STEEL_GREY, TARP_FADED, TIRE_BLACK,
    WORKLIGHT, concrete, plank, rusted, sheet, tarp,
};

pub struct SalvageShack;

impl CatalogueEntry for SalvageShack {
    fn slug(&self) -> &'static str {
        "salvage_shack"
    }
    fn name(&self) -> &'static str {
        "Salvage Shack"
    }
    fn description(&self) -> &'static str {
        "Hovel of welded sheet and plank under a sagging tarp, a stovepipe and a dim window."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.4_f32;
    let wall_top = wall_h;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [4.6, 0.2, 4.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];

    // Corrugated sheet walls.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.0, wall_h, 3.4],
            0.0,
            sheet(CORRUGATED_RUST),
        )),
        [0.0, 0.2 + wall_h * 0.5, 0.0],
        id_quat(),
    ));
    // Salvaged plank door + a mismatched welded sheet patch on the front (−Z).
    prims.push(prim(
        solid(cuboid_tapered([1.4, 1.9, 0.12], 0.0, plank(PLANK_GREY))),
        [-1.1, 0.2 + 0.95, -1.75],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.0, 1.3, 0.1], 0.0, rusted(STEEL_GREY))),
        [0.05, 0.2 + 0.7, -1.78],
        quat_y(0.04),
    ));
    // Dim-lit window, framed in salvaged plank — emissive (−Z front).
    prims.push(prim(
        solid(cuboid_tapered([0.95, 0.85, 0.08], 0.0, plank(PLANK_GREY))),
        [1.15, 0.2 + 1.5, -1.74],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.7, 0.6, 0.14], 0.0, glow(WORKLIGHT, 1.4)),
        [1.15, 0.2 + 1.5, -1.78],
        id_quat(),
    ));

    // Sagging tarp roof, weighted down with a salvaged tyre and a loose rock.
    prims.push(prim(
        solid(cuboid_tapered([4.8, 0.15, 4.2], 0.1, tarp(TARP_FADED))),
        [0.0, wall_top + 0.3, 0.0],
        quat_x(0.08),
    ));
    prims.push(prim(
        solid(torus(0.12, 0.34, tarp(TIRE_BLACK))),
        [-1.2, wall_top + 0.42, 0.6],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.4, 0.3, 0.4],
            0.3,
            concrete(CONCRETE_GREY),
        )),
        [1.0, wall_top + 0.46, -0.5],
        quat_y(0.5),
    ));

    // Rusted stovepipe with a salvaged cap, leaning off the back.
    prims.push(prim(
        solid(cylinder_tapered(0.14, 1.6, 8, 0.0, rusted(RUST_BROWN))),
        [1.4, wall_top + 0.9, 1.0],
        quat_x(-0.12),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.34, 0.08, 0.34], 0.0, rusted(STEEL_GREY))),
        [1.55, wall_top + 1.72, 1.0],
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
        assert_sanitize_stable(&SalvageShack.build(""), "salvage_shack");
    }
}
