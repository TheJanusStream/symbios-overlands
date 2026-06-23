//! Mangrove — a gnarled wetland tree on splayed stilt roots. Five arching prop
//! roots drawn from the base in the axiom splay out and pitch down to ground
//! level like flying buttresses, lifting a dark red-brown trunk that carries a
//! compact gnarled crown of dark wetland-green foliage. The signature wetland
//! scatter standing out of the shallows.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Mangrove;

impl CatalogueEntry for Mangrove {
    fn slug(&self) -> &'static str {
        "lsys_mangrove"
    }
    fn name(&self) -> &'static str {
        "Mangrove"
    }
    fn description(&self) -> &'static str {
        "Gnarled wetland tree on splayed stilt roots — stands out of the shallows."
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
    // 0 — dark red-brown mangrove bark (roots + trunk).
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.32, 0.13, 0.08]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.40, 0.17, 0.10]),
                color_dark: Fp3([0.16, 0.06, 0.04]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — dark wetland-green leaf (slot 2 matches the grammar's `,(2)`).
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.16, 0.32, 0.14]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.08, 0.24, 0.09]),
                color_edge: Fp3([0.14, 0.33, 0.12]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Five arching prop roots (R) splay from the base and pitch down to the
        // ground (the &(25)…&(30) chain), forming the open stilt cage. The
        // trunk rises clear of the cage, then a short gnarled crown (A→C with a
        // central leader) carries dark-green leaf rosettes (L) as the outer
        // shell. A mild downward tropism adds gnarl/sag.
        source_code: "#define s 0.5\n\
                      omega: [&(55)R]/(80)[&(58)R]/(85)[&(55)R]/(80)[&(60)R]/(75)[&(57)R] !(0.40)F(1.8)/(45)A\n\
                      p1: R -> !(0.12)F(0.9)&(25)F(0.9)&(30)F(0.8)&(30)F(0.7)\n\
                      p2: A -> !(0.22)F(s)L[&(33)F(s*0.8)C]/(115)[&(36)F(s*0.8)C]/(125)[&(31)F(s*0.8)C]/(118)[&(38)F(s*0.8)C][^(6)F(s*0.6)L C]\n\
                      p3: C -> F(s*0.6)L[&(40)/(125)F(s*0.55)C][^(20)/(235)F(s*0.55)C][&(20)/(180)F(s*0.5)C]\n\
                      p4: L -> ,(2)[~(1,21)]/(72)[&(20)~(1,21)]/(95)[~(1,21)]/(110)[&(45)~(1,21)]/(120)[^(35)~(1,21)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 5,
        seed: 1,
        angle: Fp(45.0),
        step: Fp(1.0),
        width: Fp(0.32),
        elasticity: Fp(0.20),
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
        let mut g = Mangrove.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
