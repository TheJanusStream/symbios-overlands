//! Palm — a tall bare trunk crowned by a whorl of drooping fronds. A
//! fixed-length trunk (drawn in the axiom so iteration count can't blow it
//! up) carries a deterministic crown of five fronds that radiate and droop,
//! each tipped with a broad leaf prop. The signature coastal / tropical
//! scatter where a conifer reads wrong.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Palm;

impl CatalogueEntry for Palm {
    fn slug(&self) -> &'static str {
        "lsys_palm"
    }
    fn name(&self) -> &'static str {
        "Palm"
    }
    fn description(&self) -> &'static str {
        "Tall bare trunk crowned by a whorl of drooping fronds — a coastal palm."
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
    // 0 — fibrous brown trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.42, 0.30, 0.16]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    // 1 — green frond leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.24, 0.46, 0.18]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig::default()),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Fixed five-segment trunk in the axiom, then a crown C of five
        // radiating fronds; each frond D droops (&) and tips a leaf prop.
        source_code: "#define s 0.9\n\
                      omega: !(0.24)F(s)F(s)F(s)F(s)F(s)C\n\
                      p1: C -> /(72)[&(55)D]/(72)[&(55)D]/(72)[&(55)D]/(72)[&(55)D]/(72)[&(55)D]\n\
                      p2: D -> F(s*0.6),(1)~(0,45)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 3,
        seed: 1,
        angle: Fp(55.0),
        step: Fp(1.0),
        width: Fp(0.24),
        elasticity: Fp(0.15),
        tropism: None,
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
        let mut g = Palm.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
