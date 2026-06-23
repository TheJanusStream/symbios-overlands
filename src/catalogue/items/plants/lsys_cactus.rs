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
            // Desert sage / blue-green succulent — flat matte, no texture.
            base_color: Fp3([0.24, 0.42, 0.30]),
            roughness: Fp(0.7),
            uv_scale: Fp(1.0),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        // A dominant THICK column (tw=2.6) that mostly rises; ~40% of steps
        // sprout a distinctly THINNER arm (aw=1.25) that bends out and runs
        // four segments to clear the trunk (a real elbow with daylight), turns
        // back to vertical over a rounded two-step `^`, then rises parallel (U)
        // — the saguaro candelabra. The golden-angle roll spirals successive
        // arms around the column; upward tropism keeps everything vertical. The
        // finalization tapers the trunk apex and arm tips into rounded domes.
        source_code: "#define s 0.85\n\
                      #define g 137\n\
                      #define tw 2.6\n\
                      #define aw 1.25\n\
                      omega: !(tw)F(s)F(s)F(s)A\n\
                      p1: A : 0.4 -> /(g)[!(aw)&(70)F(s)F(s)F(s)F(s)F(s)^(35)F(s)^(35)F(s)U]!(tw)F(s)A\n\
                      p2: A : 0.6 -> /(g)!(tw)F(s)A\n\
                      p3: U -> !(aw)F(s*1.25)U"
            .to_string(),
        finalization_code: "A -> !(tw)F(s)!(tw*0.7)F(s*0.55)!(tw*0.45)F(s*0.4)!(tw*0.22)F(s*0.25)\n\
             U -> !(aw)F(s)!(aw*0.6)F(s*0.45)!(aw*0.28)F(s*0.25)"
            .to_string(),
        iterations: 7,
        seed: 3,
        angle: Fp(80.0),
        step: Fp(1.0),
        width: Fp(1.6),
        elasticity: Fp(0.12),
        tropism: Some(Fp3([0.0, 1.0, 0.0])),
        materials,
        prop_mappings: HashMap::new(),
        prop_scale: Fp(0.04),
        mesh_resolution: 12,
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
