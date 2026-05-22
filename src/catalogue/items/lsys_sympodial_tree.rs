//! Sympodial tree — ABOP Fig 2.7. Sympodial branching pattern where
//! the main axis is replaced at each step by two daughter branches
//! that take over leader duty in turn, producing a rounded canopy.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignBarkConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};

pub struct SympodialTree;

impl CatalogueEntry for SympodialTree {
    fn slug(&self) -> &'static str {
        "lsys_sympodial_tree"
    }
    fn name(&self) -> &'static str {
        "Sympodial Tree"
    }
    fn description(&self) -> &'static str {
        "Rounded-canopy tree with sympodial branching — ABOP Fig 2.7."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Plants
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
            base_color: Fp3([0.4, 0.25, 0.1]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "#define r1 0.9\n\
                      #define r2 0.7\n\
                      #define a1 10\n\
                      #define a2 60\n\
                      #define wr 0.707\n\
                      omega: A(1.0, 0.1)\n\
                      p1: A(l,w) -> !(w)F(l)[&(a1)B(l*r1,w*wr)] /(180)[&(a2)B(l*r2,w*wr)]\n\
                      p2: B(l,w) -> !(w)F(l)[+(a1)$B(l*r1,w*wr)] [-(a2)$B(l*r2,w*wr)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 10,
        seed: 1,
        angle: Fp(18.0),
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
        let mut g = SympodialTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
