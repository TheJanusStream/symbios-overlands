//! Monopodial tree — ABOP Fig 2.6. A central leader trunk with recursive
//! lateral branching gives the conical conifer silhouette; a finalization rule
//! hangs a downward-angled needle spray (Needle cards) off every lateral tip so
//! it reads as a living dark blue-green conifer rather than a bare skeleton.
//! The lsystem-explorer's `s=100, w=10` constants are scaled down 100× for
//! room-scale.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_bark, tint_needle};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignMaterialSettings, SovereignNeedleConfig, SovereignTextureConfig,
};

/// Conifer re-skins (#910): one monopodial skeleton covers the cold biomes.
/// Slot 0 is bark, slot 1 the needle spray — see the material-slot
/// convention in `docs/lsystem-playbook.md`.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "pine",
        label: "Pine (warm olive)",
        apply: |m| {
            // Warmer, yellower needles and the red-brown plated bark that
            // separates a pine ridge from a spruce stand at a distance.
            tint_needle(
                m,
                1,
                [0.20, 0.32, 0.16],
                [0.16, 0.28, 0.12],
                [0.28, 0.38, 0.18],
            );
            tint_bark(
                m,
                0,
                [0.42, 0.26, 0.16],
                [0.50, 0.31, 0.19],
                [0.20, 0.11, 0.06],
            );
        },
    },
    PlantVariant {
        name: "larch_gold",
        label: "Larch (autumn gold)",
        apply: |m| {
            // The deciduous conifer: gold needles before drop. Tundra and
            // high alpine read as a different world with this one swap.
            tint_needle(
                m,
                1,
                [0.62, 0.50, 0.18],
                [0.55, 0.42, 0.13],
                [0.74, 0.62, 0.26],
            );
            tint_bark(
                m,
                0,
                [0.38, 0.29, 0.20],
                [0.45, 0.35, 0.24],
                [0.18, 0.13, 0.09],
            );
        },
    },
    PlantVariant {
        name: "frosted",
        label: "Frosted spruce",
        apply: |m| {
            // Pale, desaturated and blue-shifted — snow-laden needles for
            // the coldest rooms, where full-chroma green reads wrong.
            tint_needle(
                m,
                1,
                [0.24, 0.34, 0.33],
                [0.20, 0.30, 0.30],
                [0.34, 0.44, 0.44],
            );
            tint_bark(
                m,
                0,
                [0.30, 0.25, 0.22],
                [0.36, 0.30, 0.27],
                [0.14, 0.11, 0.09],
            );
        },
    },
];

pub struct MonopodialTree;

impl CatalogueEntry for MonopodialTree {
    fn slug(&self) -> &'static str {
        "lsys_monopodial_tree"
    }
    fn name(&self) -> &'static str {
        "Monopodial Conifer"
    }
    fn description(&self) -> &'static str {
        "Conical single-leader conifer with drooping needle sprays — ABOP Fig 2.6."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn variants(&self) -> &'static [PlantVariant] {
        VARIANTS
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 — brown bark trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.34, 0.22, 0.11]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.40, 0.26, 0.13]),
                color_dark: Fp3([0.16, 0.10, 0.05]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — dark blue-green needle foliage. A real conifer needle cluster
    // (paired needles on a woody shoot) rather than the broadleaf Twig card
    // it used to borrow; the grammar selects it via `,(1)`.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.12, 0.28, 0.22]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Needle(SovereignNeedleConfig {
                // Cool dark blue-green needles, held close to the shoot in
                // the spruce/fir manner.
                color_base: Fp3([0.05, 0.16, 0.12]),
                color_tip: Fp3([0.13, 0.30, 0.23]),
                color_shoot: Fp3([0.18, 0.12, 0.08]),
                needle_angle: Fp64(38.0),
                pair_count: 13,
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    GeneratorKind::LSystem {
        // Monopodial skeleton with stochastic variation (#910): a central
        // leader A drops lateral B branches that sub-branch C/B, shrinking
        // each step for the conical outline. Lateral pitch/length and the
        // leader's phyllotaxis roll are seed-varied (rolls held near the
        // 137.5° golden angle — wider spreads resonate into azimuth notches).
        // The finalization hangs a TWO-card needle spray off every lateral
        // tip, angled steeply down so the foliage droops into tiered conifer
        // skirts — thinned from three larger cards after the WS0 grading
        // called the old crown excessively dense.
        source_code: "#define r1 0.9\n\
                      #define r2 0.6\n\
                      #define a2 45\n\
                      #define wr 0.707\n\
                      omega: A(1.0, 0.1)\n\
                      p1a: 0.4 : A(l, w) -> !(w) F(l) [ &(45) B(l*r2, w*wr) ] / (137.5) A(l*r1, w*wr)\n\
                      p1b: 0.3 : A(l, w) -> !(w) F(l) [ &(38) B(l*r2*0.9, w*wr) ] / (134) A(l*r1, w*wr)\n\
                      p1c: 0.3 : A(l, w) -> !(w) F(l) [ &(52) B(l*r2*1.1, w*wr) ] / (141) A(l*r1, w*wr)\n\
                      p2a: 0.55 : B(l, w) -> !(w) F(l) [ -(a2) $ C(l*r2, w*wr) ] C(l*r1, w*wr)\n\
                      p2b: 0.45 : B(l, w) -> !(w) F(l*0.9) [ -(a2+9) $ C(l*r2*0.9, w*wr) ] C(l*r1, w*wr)\n\
                      p3a: 0.55 : C(l, w) -> !(w) F(l) [ +(a2) $ B(l*r2, w*wr) ] B(l*r1, w*wr)\n\
                      p3b: 0.45 : C(l, w) -> !(w) F(l*0.92) [ +(a2+8) $ B(l*r2*0.92, w*wr) ] B(l*r1, w*wr)"
            .to_string(),
        finalization_code:
            "B(l,w) : * -> ,(1)[&(90)~(0,26)]/(144)[&(103)~(0,26)]\n\
             C(l,w) : * -> ,(1)[&(90)~(0,26)]/(144)[&(103)~(0,26)]"
                .to_string(),
        iterations: 8,
        seed: 1,
        angle: Fp(45.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.0),
        tropism: None,
        materials,
        prop_mappings,
        prop_scale: Fp(0.045),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = MonopodialTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
