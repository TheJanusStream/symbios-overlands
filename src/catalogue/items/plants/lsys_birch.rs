//! Birch — a slender pale-barked broadleaf (#910 WS2 expansion). A single
//! airy leader climbs one internode per iteration (iteration count = age),
//! shedding short stochastic laterals whose twigs droop under a light
//! tropism, each dusted with small light-green leaf cards. The white bark
//! with dark lenticel streaking is the species signature; reads as the
//! bright pioneer tree of boreal and temperate woodland edges.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Birch;

impl CatalogueEntry for Birch {
    fn slug(&self) -> &'static str {
        "lsys_birch"
    }
    fn name(&self) -> &'static str {
        "Birch"
    }
    fn description(&self) -> &'static str {
        "Slender white-barked broadleaf with airy drooping twigs."
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
    // 0 — white birch bark, dark lenticel streaks.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.85, 0.84, 0.80]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.92, 0.91, 0.87]),
                color_dark: Fp3([0.18, 0.17, 0.16]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — small fresh light-green leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.44, 0.58, 0.22]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.35, 0.52, 0.18]),
                color_edge: Fp3([0.55, 0.68, 0.28]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Leader A climbs one internode per iteration, stochastically
        // dropping a short lateral B (or skipping one, a3 — the airy gap
        // look), rolled near the golden angle. Laterals recurse weakly into
        // drooping leaf twigs (light -Y tropism supplies the hang). K leaf
        // markers express next iteration; finalization tufts every apex so
        // no age shows bare twig ends.
        source_code: "omega: !(0.14)A(0.85,0.09)\n\
                      a1: 0.45 : A(l,w) -> !(w)F(l)[&(52)B(l*0.55,w*0.6)]/(137)A(l*0.95,w*0.9)\n\
                      a2: 0.4 : A(l,w) -> !(w)F(l)[&(62)B(l*0.5,w*0.55)]/(133)A(l*0.93,w*0.88)\n\
                      a3: 0.15 : A(l,w) -> !(w)F(l)/(141)A(l*0.96,w*0.92)\n\
                      b1: 0.65 : B(l,w) -> !(w)F(l)K[+(35)B(l*0.7,w*0.72)][-(28)B(l*0.66,w*0.72)]\n\
                      b2: 0.35 : B(l,w) -> !(w)F(l*0.9)K[&(20)B(l*0.7,w*0.72)]\n\
                      k1: K -> ,(1)[~(0,19)]\\(120)[&(25)~(0,18)]\\(115)[^(20)~(0,18)]"
            .to_string(),
        finalization_code:
            "A(l,w) : * -> ,(1)[~(0,22)]\\(120)[&(30)~(0,20)]\\(115)[&(35)~(0,20)]\n\
             B(l,w) : * -> ,(1)[~(0,20)]\\(140)[&(20)~(0,18)]\n\
             K -> ,(1)[~(0,19)]\\(120)[&(25)~(0,18)]\\(115)[^(20)~(0,18)]"
                .to_string(),
        iterations: 9,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.14),
        elasticity: Fp(0.10),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.05),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Birch.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
