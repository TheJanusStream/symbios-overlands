//! Bamboo — a clump of green canes (#910 WS2 expansion) for jungle
//! groves. The clump adds one cane per iteration (iteration count = age:
//! lone shoot → dense stand); each cane is drawn at full height the
//! moment it appears — real bamboo shoots reach final height in one
//! season — with a stochastic lean and narrow leaf sprays at the upper
//! nodes. Width pulses between segments hint at the nodal rings.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Bamboo;

impl CatalogueEntry for Bamboo {
    fn slug(&self) -> &'static str {
        "lsys_bamboo"
    }
    fn name(&self) -> &'static str {
        "Bamboo"
    }
    fn description(&self) -> &'static str {
        "Clump of segmented green canes with narrow leaf sprays."
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
    // 0 — green cane, darker nodal banding from the bark grain.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.42, 0.56, 0.24]),
            roughness: Fp(0.6),
            uv_scale: Fp(1.2),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.48, 0.64, 0.28]),
                color_dark: Fp3([0.24, 0.38, 0.14]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — narrow blue-green bamboo leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.30, 0.48, 0.18]),
            roughness: Fp(0.65),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.22, 0.42, 0.14]),
                color_edge: Fp3([0.36, 0.54, 0.20]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // G is the clump cursor: each iteration it plants one cane C at a
        // stochastic horizontal offset ([&(90)f(d)^(90)…] — pitch down,
        // move, pitch back so the offset is lateral, bracketed so the
        // cursor stays put) and rolls on. C draws the full cane in one
        // expansion: slight lean, segments with width pulses for the
        // nodal look, narrow leaf sprays K at the upper nodes. The
        // finalization expresses canes and sprays spawned on the last
        // iteration so the youngest shoot is never invisible.
        source_code: "omega: !(0.09)G\n\
                      g1: 0.5 : G -> [&(90)f(0.22)^(90)C]/(97)G\n\
                      g2: 0.3 : G -> [&(90)f(0.34)^(90)&(3)C]/(143)G\n\
                      g3: 0.2 : G -> [&(90)f(0.28)^(90)^(3)C]/(61)G\n\
                      c1: C -> &(2)!(0.09)F(1.0)!(0.078)F(0.95)[&(65)/(60)K]!(0.088)F(0.9)[&(55)K]!(0.074)F(0.85)[&(60)/(120)K]!(0.08)F(0.8)[&(50)/(240)K][&(58)/(80)K]K\n\
                      k1: K -> ,(1)[~(0,24)]\\(130)[&(25)~(0,21)]\\(110)[~(0,20)]\\(95)[^(15)~(0,20)]"
            .to_string(),
        finalization_code: "C -> &(2)!(0.09)F(1.0)!(0.078)F(0.95)!(0.088)F(0.9)!(0.074)F(0.85),(1)[~(0,20)]\\(120)[&(25)~(0,18)]\n\
             K -> ,(1)[~(0,24)]\\(130)[&(25)~(0,21)]\\(110)[~(0,20)]\\(95)[^(15)~(0,20)]"
            .to_string(),
        iterations: 6,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.09),
        elasticity: Fp(0.06),
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
        let mut g = Bamboo.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
