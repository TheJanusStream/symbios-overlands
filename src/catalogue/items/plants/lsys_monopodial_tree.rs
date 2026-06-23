//! Monopodial tree — ABOP Fig 2.6. A central leader trunk with recursive
//! lateral branching gives the conical conifer silhouette; a finalization rule
//! hangs a downward-angled needle spray (Twig cards) off every lateral tip so
//! it reads as a living dark blue-green conifer rather than a bare skeleton.
//! The lsystem-explorer's `s=100, w=10` constants are scaled down 100× for
//! room-scale.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

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
    // 1 — dark blue-green needle foliage (Twig card; base_color tints the
    // twig sprite, which the grammar selects via `,(1)`).
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.12, 0.28, 0.22]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                // Cool dark blue-green needles (the default twig leaf skews
                // warm olive and drifts yellow-green on lit sides).
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.08, 0.22, 0.18]),
                    color_edge: Fp3([0.14, 0.30, 0.24]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    GeneratorKind::LSystem {
        // Unchanged monopodial skeleton: a central leader A drops lateral B
        // branches that sub-branch C/B, shrinking each step for the conical
        // outline. The finalization hangs a three-card needle spray off every
        // lateral tip (B and C), angled steeply DOWN (&88–108) so the foliage
        // droops into tiered conifer skirts and the leader stays pointed.
        source_code: "#define r1 0.9\n\
                      #define r2 0.6\n\
                      #define a0 45\n\
                      #define a2 45\n\
                      #define d 137.5\n\
                      #define wr 0.707\n\
                      omega: A(1.0, 0.1)\n\
                      p1: A(l, w) -> !(w) F(l) [ &(a0) B(l*r2, w*wr) ] / (d) A(l*r1, w*wr)\n\
                      p2: B(l, w) -> !(w) F(l) [ -(a2) $ C(l*r2, w*wr) ] C(l*r1, w*wr)\n\
                      p3: C(l, w) -> !(w) F(l) [ +(a2) $ B(l*r2, w*wr) ] B(l*r1, w*wr)"
            .to_string(),
        finalization_code:
            "B(l,w) : * -> ,(1)[&(88)~(0,30)]/(120)[&(98)~(0,30)]/(120)[&(108)~(0,30)]\n\
             C(l,w) : * -> ,(1)[&(88)~(0,30)]/(120)[&(98)~(0,30)]/(120)[&(108)~(0,30)]"
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
