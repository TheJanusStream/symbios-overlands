//! Dead shrub — a leafless gnarled scrub. The gravity-tropism ternary
//! grammar with no foliage, weathered grey-brown deadwood, and a tight
//! iteration count so it reads as a low twisted skeleton. The stubborn
//! survivor of arid, volcanic and badland ground.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignBarkConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};

pub struct DeadShrub;

impl CatalogueEntry for DeadShrub {
    fn slug(&self) -> &'static str {
        "lsys_dead_shrub"
    }
    fn name(&self) -> &'static str {
        "Dead Shrub"
    }
    fn description(&self) -> &'static str {
        "Leafless gnarled deadwood scrub — the survivor of dry, scorched ground."
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
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.46, 0.42, 0.36]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        source_code: "#define d1 180\n\
                      #define d2 252\n\
                      #define a 40\n\
                      #define lr 1.05\n\
                      #define vr 1.732\n\
                      #define s 0.5\n\
                      omega: !(1)F(3*s)/(45)A\n\
                      p1: A : * -> !(vr)F(s)[&(a)F(s)A]/(d1)[&(a)F(s)A]/(d2)[&(a)F(s)A]\n\
                      p2: F(l) : * -> F(l*lr)\n\
                      p3: !(w) : * -> !(w*vr)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 4,
        seed: 1,
        angle: Fp(40.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.55),
        tropism: Some(Fp3([0.1, -1.0, 0.0])),
        materials,
        prop_mappings: HashMap::new(),
        prop_scale: Fp(0.04),
        mesh_resolution: 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = DeadShrub.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
