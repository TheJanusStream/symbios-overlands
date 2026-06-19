//! Well house — a Medieval secondary. The village draw-well: a round
//! fieldstone kerb over dark water, four oak posts carrying a little
//! thatched canopy, and a windlass roller with an iron crank winding a
//! rope down to a hanging bucket. The gathering point of the square.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    IRON_DARK, STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_OAK, iron, rough_stone, stone, thatch,
    timber,
};

pub struct WellHouse;

impl CatalogueEntry for WellHouse {
    fn slug(&self) -> &'static str {
        "well_house"
    }
    fn name(&self) -> &'static str {
        "Well House"
    }
    fn description(&self) -> &'static str {
        "Round fieldstone draw-well under a thatched canopy, with a windlass and bucket."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Still dark well-water, faintly reflective.
fn water() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.05, 0.07, 0.09]),
        roughness: Fp(0.1),
        metallic: Fp(0.3),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let kerb_h = 0.9;
    let post_h = 2.6;
    let roof_y = kerb_h + post_h;

    let mut prims = vec![
        // Fieldstone kerb ring — the root (solid drum).
        prim(
            solid(cylinder_tapered(
                1.0,
                kerb_h,
                16,
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, kerb_h * 0.5, 0.0],
            id_quat(),
        ),
        // Dark water surface, recessed just inside the kerb.
        prim(
            cylinder_tapered(0.82, 0.1, 16, 0.0, water()),
            [0.0, kerb_h - 0.18, 0.0],
            id_quat(),
        ),
        // Dressed coping band around the kerb top.
        prim(
            solid(cylinder_tapered(1.05, 0.18, 16, 0.0, stone(STONE_GREY))),
            [0.0, kerb_h - 0.09, 0.0],
            id_quat(),
        ),
    ];

    // Two oak posts carrying the canopy and the windlass.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.13, post_h, 8, 0.06, timber(WOOD_OAK))),
            [sx * 0.9, kerb_h + post_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Windlass roller across the posts, with an iron crank handle.
    let roller_y = kerb_h + 1.3;
    prims.push(prim(
        solid(cylinder_tapered(0.12, 1.7, 10, 0.0, timber(WOOD_DARK))),
        [0.0, roller_y, 0.0],
        quat_x(std::f32::consts::FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.05, 0.4, 0.05], 0.0, iron(IRON_DARK))),
        [0.95, roller_y - 0.2, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.3, 0.05, 0.05], 0.0, iron(IRON_DARK))),
        [1.1, roller_y - 0.4, 0.0],
        id_quat(),
    ));
    // Rope down to a hanging bucket over the water.
    prims.push(prim(
        cylinder_tapered(0.02, roller_y - kerb_h - 0.1, 6, 0.0, timber(WOOD_DARK)),
        [0.25, kerb_h + (roller_y - kerb_h) * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.18, 0.3, 10, 0.1, timber(WOOD_OAK))),
        [0.25, kerb_h + 0.2, 0.0],
        id_quat(),
    ));
    prims.push(prim(iron_band(), [0.25, kerb_h + 0.32, 0.0], id_quat()));

    // Little thatched canopy on the posts.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 1.2, 2.0], 0.5, thatch(THATCH_STRAW))),
        [0.0, roof_y + 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// A thin iron hoop around the bucket rim.
fn iron_band() -> crate::pds::GeneratorKind {
    crate::catalogue::items::util::torus(0.03, 0.18, iron(IRON_DARK))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WellHouse.build(""), "well_house");
    }
}
