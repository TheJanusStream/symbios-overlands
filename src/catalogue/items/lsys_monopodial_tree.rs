//! Monopodial tree — ABOP Fig 2.6. A central leader trunk with
//! recursive lateral branching, producing a conifer-like silhouette.
//! Bark material on the only material slot; the lsystem-explorer's
//! `s=100, w=10` constants are scaled down 100× for room-scale.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignBarkConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};

pub struct MonopodialTree;

impl CatalogueEntry for MonopodialTree {
    fn slug(&self) -> &'static str {
        "lsys_monopodial_tree"
    }
    fn name(&self) -> &'static str {
        "Monopodial Tree"
    }
    fn description(&self) -> &'static str {
        "Conifer-like single-leader trunk with recursive lateral branching — ABOP Fig 2.6."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Plants
    }
    fn build(&self) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.55, 0.27, 0.07]),
            roughness: Fp(0.85),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "#define r1 0.9\n\
                      #define r2 0.6\n\
                      #define a0 45\n\
                      #define a2 45\n\
                      #define d 137.5\n\
                      #define wr 0.707\n\
                      omega: A(1.0, 0.1)\n\
                      p1: A(l, w) -> !(w) F(l) [ &(a0) B(l*r2, w*wr) ] / (d) A(l*r1, w*wr)\n\
                      p2: B(l, w) -> !(w) F(l) [ -(a2) $ C(l*r2, w*wr) ] C(l*r1, w*wr)\n\
                      p3: C(l, w) -> !(w) F(l) [ +(a2) $ B(l*r2, w*wr) ] B(l*r1, w*wr)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 8,
        seed: 1,
        angle: Fp(45.0),
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
        let mut g = MonopodialTree.build();
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
