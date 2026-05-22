//! Ternary tree with gravity tropism — ABOP Fig 2.8. Three-way
//! branching at each step, with a downward tropism vector that pulls
//! the branches into a weeping silhouette. Suits willow / drooping
//! canopy starter.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignBarkConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};

pub struct TernaryGravityTree;

impl CatalogueEntry for TernaryGravityTree {
    fn slug(&self) -> &'static str {
        "lsys_ternary_gravity"
    }
    fn name(&self) -> &'static str {
        "Ternary Tree (Gravity)"
    }
    fn description(&self) -> &'static str {
        "Three-way branching tree with downward tropism — ABOP Fig 2.8."
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
            base_color: Fp3([0.35, 0.2, 0.08]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "#define d1 180\n\
                      #define d2 252\n\
                      #define a 36\n\
                      #define lr 1.07\n\
                      #define vr 1.732\n\
                      #define s 0.5\n\
                      omega: !(1)F(4*s)/(45)A\n\
                      p1: A : * -> !(vr)F(s)[&(a)F(s)A]/(d1)[&(a)F(s)A]/(d2)[&(a)F(s)A]\n\
                      p2: F(l) : * -> F(l*lr)\n\
                      p3: !(w) : * -> !(w*vr)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 6,
        seed: 1,
        angle: Fp(36.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.40),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
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
        let mut g = TernaryGravityTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
