//! Saguaro cactus — a thick green columnar succulent with a few
//! up-curving arms. Not a tree: a low-iteration L-system with a heavy
//! trunk width, an upward tropism to keep the arms vertical, and a flat
//! green succulent material (no bark, no foliage). The signature desert
//! scatter where a broadleaf reads wrong.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings};

pub struct Cactus;

impl CatalogueEntry for Cactus {
    fn slug(&self) -> &'static str {
        "lsys_cactus"
    }
    fn name(&self) -> &'static str {
        "Saguaro Cactus"
    }
    fn description(&self) -> &'static str {
        "Thick green columnar cactus with up-curving arms — a desert succulent."
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
            base_color: Fp3([0.22, 0.40, 0.18]),
            roughness: Fp(0.7),
            uv_scale: Fp(1.0),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        // A heavy trunk that mostly extends; ~40% of segments sprout a
        // pair of opposed arms that go out then turn up (saguaro).
        source_code: "#define s 0.7\n\
                      omega: !(0.6)F(s)F(s)A\n\
                      p1: A : 0.4 -> F(s)[&(80)F(s*0.5)^(85)F(s)]/(180)[&(80)F(s*0.5)^(85)F(s)]F(s)A\n\
                      p2: A : 0.6 -> F(s)A"
            .to_string(),
        finalization_code: String::new(),
        iterations: 5,
        seed: 1,
        angle: Fp(80.0),
        step: Fp(1.0),
        width: Fp(0.6),
        elasticity: Fp(0.15),
        tropism: Some(Fp3([0.0, 1.0, 0.0])),
        materials,
        prop_mappings: HashMap::new(),
        prop_scale: Fp(0.04),
        mesh_resolution: 10,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Cactus.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
