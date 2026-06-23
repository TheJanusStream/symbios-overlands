//! Sympodial tree — ABOP Fig 2.7. Sympodial branching where the main axis is
//! replaced at each step by two daughter branches that take over leader duty in
//! turn, producing a spreading, vase-like broadleaf crown. A short trunk in the
//! axiom lifts the crown, and a finalization rule blooms every branch tip into
//! a dome of green leaf cards so it reads as a full leafy tree rather than a
//! bare skeleton.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
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
        "Spreading-canopy leafy broadleaf with sympodial branching — ABOP Fig 2.7."
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
    // 0 — brown bark trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.34, 0.22, 0.11]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.40, 0.26, 0.14]),
                color_dark: Fp3([0.16, 0.10, 0.05]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — deep broadleaf green (slot 2 matches the finalization's `,(2)`).
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.20, 0.40, 0.14]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.12, 0.30, 0.09]),
                color_edge: Fp3([0.22, 0.40, 0.13]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // A short trunk (F(2.0)F(1.6)) in the axiom lifts the sympodial crown.
        // The unexpanded `B(l,w)` apices are the branch tips; the finalization
        // rule blooms each into a two-ring dome of leaf cards (inner &40,
        // outer &80, plus a couple underside cards) so the tips merge into a
        // continuous spreading green canopy over a visible trunk.
        source_code: "#define r1 0.9\n\
                      #define r2 0.7\n\
                      #define a1 10\n\
                      #define a2 60\n\
                      #define wr 0.707\n\
                      omega: !(0.32)F(2.0)F(1.6)A(1.0, 0.1)\n\
                      p1: A(l,w) -> !(w)F(l)[&(a1)B(l*r1,w*wr)] /(180)[&(a2)B(l*r2,w*wr)]\n\
                      p2: B(l,w) -> !(w)F(l)[+(a1)$B(l*r1,w*wr)] [-(a2)$B(l*r2,w*wr)]"
            .to_string(),
        finalization_code:
            "B(l,w) : * -> ,(2)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(45)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(45)[&(95)~(1,18)]/(180)[&(95)~(1,18)]"
                .to_string(),
        iterations: 9,
        seed: 1,
        angle: Fp(18.0),
        step: Fp(1.0),
        width: Fp(0.32),
        elasticity: Fp(0.0),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.05),
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
