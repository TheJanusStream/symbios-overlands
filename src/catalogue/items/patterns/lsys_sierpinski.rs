//! Sierpinski gasket — recursive triangle fractal (ABOP Fig 1.10b).
//! Uses decomposition rules to collapse left/right marker symbols to
//! a single F-segment at the final iteration; the production rules
//! and decomposition rules are split between `source_code` and
//! `finalization_code` to match the symbios-overlands LSystem schema.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct SierpinskiGasket;

impl CatalogueEntry for SierpinskiGasket {
    fn slug(&self) -> &'static str {
        "lsys_sierpinski"
    }
    fn name(&self) -> &'static str {
        "Sierpinski Gasket"
    }
    fn description(&self) -> &'static str {
        "Recursive triangle fractal — ABOP Fig 1.10(b)."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Pattern
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
            base_color: Fp3([0.9, 0.3, 0.6]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::None,
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "omega: Fr\n\
                      Fl -> Fr+Fl+Fr\n\
                      Fr -> Fl-Fr-Fl"
            .to_string(),
        finalization_code: "Fr -> F\nFl -> F".to_string(),
        iterations: 5,
        seed: 1,
        angle: Fp(60.0),
        step: Fp(0.5),
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
        let mut g = SierpinskiGasket.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
