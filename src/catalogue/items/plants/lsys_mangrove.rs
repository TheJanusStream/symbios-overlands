//! Mangrove — a gnarled wetland tree on splayed stilt roots. Three
//! arching prop roots drawn from the base in the axiom lift a
//! gravity-tropism ternary crown of dark red-brown wood. The signature
//! wetland scatter standing out of the shallows.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, SovereignBarkConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
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
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.32, 0.18, 0.12]),
            roughness: Fp(0.92),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        // Three arching stilt roots from the base, then a gnarled gravity
        // ternary crown.
        source_code: "#define d1 180\n\
                      #define d2 252\n\
                      #define a 38\n\
                      #define lr 1.06\n\
                      #define vr 1.6\n\
                      #define s 0.5\n\
                      omega: [&(70)F(2*s)]/(120)[&(70)F(2*s)]/(120)[&(70)F(2*s)]!(1)F(4*s)/(45)A\n\
                      p1: A : * -> !(vr)F(s)[&(a)F(s)A]/(d1)[&(a)F(s)A]/(d2)[&(a)F(s)A]\n\
                      p2: F(l) : * -> F(l*lr)\n\
                      p3: !(w) : * -> !(w*vr)"
            .to_string(),
        finalization_code: String::new(),
        iterations: 5,
        seed: 1,
        angle: Fp(38.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.35),
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
        let mut g = Mangrove.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
