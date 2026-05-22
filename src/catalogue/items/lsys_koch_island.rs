//! Quadratic Koch Island — closed-curve recursive fractal (ABOP Fig 1.6).
//! Each iteration replaces every line segment with a bumpy variant,
//! producing a coastline-like silhouette. Initial F arguments rescaled
//! 100× downward from the lsystem-explorer preset so the result sits
//! at room scale instead of debug-camera scale.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct QuadraticKochIsland;

impl CatalogueEntry for QuadraticKochIsland {
    fn slug(&self) -> &'static str {
        "lsys_koch_island"
    }
    fn name(&self) -> &'static str {
        "Quadratic Koch Island"
    }
    fn description(&self) -> &'static str {
        "Closed coastline-like fractal curve — ABOP Fig 1.6."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Patterns
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
            base_color: Fp3([0.3, 0.6, 0.9]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::None,
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "omega: F(1)-F(1)-F(1)-F(1)\n\
                      F(s) -> F(s/3)+F(s/3)-F(s/3)-F(s/3)F(s/3)+F(s/3)+F(s/3)-F(s/3)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 3,
        seed: 1,
        angle: Fp(90.0),
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
        let mut g = QuadraticKochIsland.build();
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
