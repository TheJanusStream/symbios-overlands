//! Flowering tree — a small ornamental blossom tree (#910 WS2 expansion)
//! for meadows and lush valleys. A short trunk carries a stochastic
//! sympodial crown (the same organic fork family as the redesigned
//! sympodial broadleaf, scaled down); at finalization every apex blooms
//! into a mixed dome of green leaf cards and pink blossom cards driven by
//! the upstream Flower texture generator.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignFlowerConfig,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct FloweringTree;

impl CatalogueEntry for FloweringTree {
    fn slug(&self) -> &'static str {
        "lsys_flowering_tree"
    }
    fn name(&self) -> &'static str {
        "Flowering Tree"
    }
    fn description(&self) -> &'static str {
        "Small ornamental tree crowned in green leaves and pink blossom."
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
    // 0 — dark ornamental bark.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.26, 0.18, 0.12]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.32, 0.22, 0.15]),
                color_dark: Fp3([0.13, 0.09, 0.06]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — fresh green crown leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.30, 0.46, 0.17]),
            roughness: Fp(0.65),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.22, 0.40, 0.13]),
                color_edge: Fp3([0.36, 0.52, 0.19]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — pink blossom cluster (Flower sprite; the default petal palette
    // is already the soft cherry pink).
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.96, 0.80, 0.86]),
            roughness: Fp(0.55),
            texture: SovereignTextureConfig::Flower(SovereignFlowerConfig::default()),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Short axiom trunk, then the stochastic sympodial fork family
        // (two-fork / three-fork / extend) at reduced scale — a garden
        // tree, not a park giant. Finalization blooms each apex into a
        // mixed dome: green leaf ring below (,(1)), pink blossom cards
        // above and outward (,(2)) so the crown reads leaf-lined with
        // blossom froth on top.
        source_code: "omega: !(0.2)F(0.9)F(0.7)/(60)A(0.75,0.07)\n\
                      x1: 0.4 : A(l,w) -> !(w)F(l)[&(38)/(90)A(l*0.78,w*0.72)][^(12)/(215)A(l*0.72,w*0.7)]\n\
                      x2: 0.35 : A(l,w) -> !(w)F(l)[&(45)/(130)A(l*0.75,w*0.7)][&(18)/(250)A(l*0.76,w*0.72)][^(25)/(20)A(l*0.6,w*0.62)]\n\
                      x3: 0.25 : A(l,w) -> !(w)&(8)F(l*1.05)/(120)A(l*0.82,w*0.76)"
            .to_string(),
        finalization_code:
            "A(l,w) : * -> ,(1)[&(48)~(0,22)]/(95)[&(52)~(0,22)]/(110)[&(45)~(0,21)],(2)[^(12)~(0,20)]/(120)[&(70)~(0,19)]/(105)[~(0,20)]/(95)[^(30)~(0,18)]"
                .to_string(),
        iterations: 7,
        seed: 1,
        angle: Fp(18.0),
        step: Fp(1.0),
        width: Fp(0.2),
        elasticity: Fp(0.08),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
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
        let mut g = FloweringTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
