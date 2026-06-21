//! Stela — a Mesoamerican secondary. A tall carved limestone slab recording
//! a ruler's reign in bands of glyphs, set with a jade mask inlay and paired
//! with a round sacrificial altar stone at its foot.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    JADE_GREEN, LIMESTONE_PALE, STONE_GREY, STUCCO_CREAM, STUCCO_RED, cobble, jade, limestone,
    painted,
};

pub struct Stela;

impl CatalogueEntry for Stela {
    fn slug(&self) -> &'static str {
        "stela"
    }
    fn name(&self) -> &'static str {
        "Stela"
    }
    fn description(&self) -> &'static str {
        "Carved limestone slab in glyph bands with a jade inlay and an altar stone."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 4.8_f32;
    // Carved stela slab — the root. The carved face is the front (−Z).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [1.6, slab_h, 0.6],
            0.07,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    )];
    let fz = -0.30_f32; // front relief plane

    // Raised border frame around the carved field.
    prims.push(prim(
        cuboid_tapered([1.42, 0.16, 0.12], 0.0, limestone(STUCCO_CREAM)),
        [0.0, slab_h - 0.35, fz],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.42, 0.16, 0.12], 0.0, limestone(STUCCO_CREAM)),
        [0.0, 0.35, fz],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.16, slab_h - 0.7, 0.12], 0.0, limestone(STUCCO_CREAM)),
            [sx * 0.62, slab_h * 0.5, fz],
            id_quat(),
        ));
    }

    // Ruler figure carved into the upper field: a wide plumed headdress, a
    // jade mask face, a torso panel with a ceremonial bar and jade pectoral.
    prims.push(prim(
        cuboid_tapered([1.15, 0.55, 0.16], 0.15, painted(STUCCO_RED)),
        [0.0, slab_h - 0.95, fz - 0.02],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.7, 0.1], 0.3, jade(JADE_GREEN)),
            [sx * 0.4, slab_h - 0.45, fz],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.5, 0.6, 0.18], 0.12, jade(JADE_GREEN)),
        [0.0, slab_h - 1.55, fz - 0.04],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.8, 1.4, 0.1], 0.05, limestone(STUCCO_CREAM)),
        [0.0, slab_h - 2.5, fz],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.0, 0.18, 0.14], 0.0, painted(STUCCO_RED)),
        [0.0, slab_h - 2.1, fz - 0.03],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.28, 0.28, 0.14], 0.1, jade(JADE_GREEN)),
        [0.0, slab_h - 2.55, fz - 0.04],
        id_quat(),
    ));

    // Stacked glyph cartouches at the foot — a 2×3 grid of recessed blocks.
    for r in 0..3 {
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                cuboid_tapered([0.34, 0.34, 0.08], 0.0, cobble(STONE_GREY)),
                [sx * 0.28, 0.85 + r as f32 * 0.55, fz],
                id_quat(),
            ));
        }
    }

    // Carved glyph columns down both side (±X) edges.
    for sx in [-1.0_f32, 1.0] {
        for k in 0..5 {
            prims.push(prim(
                cuboid_tapered([0.1, 0.42, 0.32], 0.0, cobble(STONE_GREY)),
                [sx * 0.74, 0.9 + k as f32 * 0.72, 0.0],
                id_quat(),
            ));
        }
    }

    // Round sacrificial altar stone at the foot (front, −Z), banded with a
    // painted rim and crowned by a jade glyph boss.
    prims.push(prim(
        solid(cylinder_tapered(1.0, 0.6, 18, 0.05, limestone(STONE_GREY))),
        [0.0, 0.3, -1.9],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.08, 0.92, painted(STUCCO_RED)),
        [0.0, 0.55, -1.9],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.5, 0.12], 0.1, jade(JADE_GREEN)),
        [0.0, 0.64, -1.9],
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
        assert_sanitize_stable(&Stela.build(""), "stela");
    }
}
