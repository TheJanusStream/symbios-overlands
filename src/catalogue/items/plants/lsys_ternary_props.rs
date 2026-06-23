//! Ternary tree with foliage props — variant of ABOP Fig 2.8 with
//! stochastic alternatives, decomposition rules emitting leaf props,
//! and a three-slot material stack (bark / twig / leaf).
//!
//! This is the LSystem entry that previously lived as the hard-coded
//! `default_lsystem_kind` under `crate::ui::room::widgets` — it's
//! the richest of the lsystem-explorer presets and the most visually
//! complete starter for "I want a tree".

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

pub struct TernaryPropsTree;

impl CatalogueEntry for TernaryPropsTree {
    fn slug(&self) -> &'static str {
        "lsys_ternary_props"
    }
    fn name(&self) -> &'static str {
        "Ternary Tree (Foliage)"
    }
    fn description(&self) -> &'static str {
        "Stochastic three-way branching tree with bark + twig + leaf material stack."
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
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.35, 0.2, 0.08]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    // 1 — twig cluster, tinted a saturated healthy green (the default texture
    // skews olive-gold and bleaches pale on lit sides).
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.32, 0.46, 0.22]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.18, 0.36, 0.13]),
                    color_edge: Fp3([0.30, 0.46, 0.18]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — leaf, deep summer green.
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.30, 0.46, 0.20]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.18, 0.36, 0.13]),
                color_edge: Fp3([0.30, 0.48, 0.18]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        source_code: "#define d1 180\n\
                      #define th 0.035\n\
                      #define d2 252\n\
                      #define a 36\n\
                      #define lr 1.12\n\
                      #define vr 1.532\n\
                      #define ps 60.0\n\
                      #define s 0.5\n\
                      #define ir 10.0\n\
                      omega: C(0.0)!(th)F(4*s)/(45)A[B]\n\
                      p0: A : 0.7 -> !(th*vr)F(s)[&(a)F(s)A[B]]/(d1)[&(a)F(s)A[B]]/(d2)[&(a)F(s)A[B]]\n\
                      p1: A : 0.3 -> !(th*vr)F(s)A[B]\n\
                      p2: F(l) : * -> F(l*lr)\n\
                      p3: !(w) : * -> !(w*vr)\n\
                      p4: B : * -> \n\
                      p5: B -> \n\
                      p6: C(x) : 0.7 -> C(x)\n\
                      p7: C(x) : 0.3 -> C(x-ir)"
            .to_string(),
        finalization_code: "p8: B : * -> ,(1)~(0,ps)\np9: C(x) : * -> /(x)".to_string(),
        iterations: 6,
        seed: 1,
        angle: Fp(36.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.05),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.04),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = TernaryPropsTree.build("");
        sanitize_generator(&mut g);
        match &g.kind {
            GeneratorKind::LSystem {
                materials,
                prop_mappings,
                ..
            } => {
                assert!(materials.contains_key(&0));
                assert!(materials.contains_key(&1));
                assert!(materials.contains_key(&2));
                assert_eq!(prop_mappings.get(&0), Some(&PropMeshType::Twig));
                assert_eq!(prop_mappings.get(&1), Some(&PropMeshType::Leaf));
            }
            other => panic!("ternary-props root must remain LSystem; got {other:?}"),
        }
    }
}
