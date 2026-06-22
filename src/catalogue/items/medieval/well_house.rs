//! Well house — a Medieval secondary. The village draw-well: a round
//! fieldstone kerb over dark water, four oak posts under a steep thatched
//! gable canopy, a windlass roller with an iron crank winding a rope down to
//! a hanging bucket, and a second pail resting on the coping. The gathering
//! point of the square.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, id_quat, prim, quat_x, solid,
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
        "Round fieldstone draw-well under a steep thatched gable canopy, with a windlass and bucket."
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
            clearance: 3.0,
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

/// An iron-hooped oak pail at `center`: a small bellied drum with a hoop and
/// a bail handle, returned as one [`Generator`] for the assemble list.
fn pail(center: [f32; 3]) -> Generator {
    let mut b = prim(
        solid(cylinder_tapered(0.18, 0.3, 10, 0.1, timber(WOOD_OAK))),
        center,
        id_quat(),
    );
    b.children.push(prim(
        crate::catalogue::items::util::torus(0.025, 0.18, iron(IRON_DARK)),
        [0.0, 0.1, 0.0],
        id_quat(),
    ));
    b
}

fn build_tree() -> Generator {
    let kerb_h = 0.9;
    let post_h = 2.6;
    let post_top = kerb_h + post_h;
    let phw = 0.95; // post half-spacing X
    let phz = 0.85; // post half-spacing Z

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

    // Four oak corner posts carrying the canopy.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.11, post_h, 8, 0.06, timber(WOOD_OAK))),
            [sx * phw, kerb_h + post_h * 0.5, sz * phz],
            id_quat(),
        ));
    }
    // Tie-beams along the ridge axis (X) linking the post tops.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [phw * 2.0 + 0.2, 0.12, 0.1],
                0.0,
                timber(WOOD_DARK),
            )),
            [0.0, post_top - 0.1, sz * phz],
            id_quat(),
        ));
    }

    // Steep thatched gable canopy (ridge ‖ X) + gable-end infill panels.
    let roof_rise = 1.3;
    prims.push(gable_roof(
        [phw * 2.0 + 1.0, roof_rise, phz * 2.0 + 1.0],
        [0.0, post_top + roof_rise * 0.5, 0.0],
        thatch(THATCH_STRAW),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.18, roof_rise, phz * 2.0 + 1.0],
                [0.0, 0.94],
                thatch(THATCH_STRAW),
            )),
            [sx * (phw + 0.5), post_top + roof_rise * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Windlass roller across the posts (axis along Z), with an iron crank.
    let roller_y = kerb_h + 1.4;
    prims.push(prim(
        solid(cylinder_tapered(
            0.13,
            phz * 2.0 + 0.3,
            10,
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, roller_y, 0.0],
        quat_x(std::f32::consts::FRAC_PI_2),
    ));
    // Crank: a stub off the +Z end and a perpendicular handle.
    prims.push(prim(
        solid(cuboid_tapered([0.05, 0.05, 0.4], 0.0, iron(IRON_DARK))),
        [0.0, roller_y, phz + 0.35],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.05, 0.4, 0.05], 0.0, iron(IRON_DARK))),
        [0.18, roller_y - 0.2, phz + 0.5],
        id_quat(),
    ));

    // Rope down to a hanging bucket over the water.
    prims.push(prim(
        cylinder_tapered(0.02, roller_y - kerb_h - 0.1, 6, 0.0, timber(WOOD_DARK)),
        [0.0, kerb_h + (roller_y - kerb_h) * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(pail([0.0, kerb_h + 0.15, 0.0]));

    // A second pail resting on the coping.
    prims.push(pail([0.7, kerb_h + 0.12, 0.55]));

    assemble(prims)
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
