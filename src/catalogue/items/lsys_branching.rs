//! Branching pattern — ABOP Fig 1.39. Bracketed L-system that
//! recursively binary-splits at every step, producing a flat
//! 2D branching diagram. Initial A argument rescaled 100× downward
//! from the lsystem-explorer preset for room-scale rendering.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct BranchingPattern;

impl CatalogueEntry for BranchingPattern {
    fn slug(&self) -> &'static str {
        "lsys_branching"
    }
    fn name(&self) -> &'static str {
        "Branching Pattern"
    }
    fn description(&self) -> &'static str {
        "Flat bracketed binary-branch diagram — ABOP Fig 1.39."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Patterns
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
            base_color: Fp3([0.9, 0.6, 0.3]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::None,
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "#define R 1.456\n\
                      omega: A(1.5)\n\
                      A(s) -> F(s)[+A(s/R)][-A(s/R)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 12,
        seed: 1,
        angle: Fp(85.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.0),
        tropism: None,
        materials,
        prop_mappings: HashMap::new(),
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
        let mut g = BranchingPattern.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
