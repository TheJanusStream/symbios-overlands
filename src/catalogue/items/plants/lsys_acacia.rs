//! Acacia — a flat-crowned savanna tree. A short trunk that splays into
//! four near-horizontal limbs, each forking into leafy twigs tipped with
//! broad gold-green leaf props, giving the wide umbrella silhouette of the
//! savanna. The crown is built deterministically so it stays flat and wide.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Acacia;

impl CatalogueEntry for Acacia {
    fn slug(&self) -> &'static str {
        "lsys_acacia"
    }
    fn name(&self) -> &'static str {
        "Acacia"
    }
    fn description(&self) -> &'static str {
        "Flat-crowned umbrella acacia with gold-green foliage — the savanna tree."
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
    // 0 — pale savanna bark.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.44, 0.34, 0.22]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    // 1 — dusty gold-green leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.46, 0.50, 0.26]),
            roughness: Fp(0.7),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig::default()),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Short trunk, four near-horizontal limbs (&75), each forking into
        // three leafy twigs — a flat wide umbrella.
        source_code: "#define s 0.8\n\
                      omega: !(0.32)F(s)F(s)F(s)/(30)C\n\
                      p1: C -> [&(75)F(s)E]/(90)[&(75)F(s)E]/(90)[&(75)F(s)E]/(90)[&(75)F(s)E]\n\
                      p2: E -> F(s*0.8)[&(80)G]/(120)[&(80)G]/(120)[&(80)G]\n\
                      p3: G -> F(s*0.5),(1)~(0,55)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 4,
        seed: 1,
        angle: Fp(75.0),
        step: Fp(1.0),
        width: Fp(0.16),
        elasticity: Fp(0.1),
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
        let mut g = Acacia.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
