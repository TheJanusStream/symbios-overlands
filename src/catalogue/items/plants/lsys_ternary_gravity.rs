//! Weeping willow — a trunk that rises and splits into arching limbs which
//! send long thin whips cascading straight down in a leafy curtain. Built on a
//! gravity tropism: the whips (E) extend one segment and drop a small leaf node
//! (K) per iteration, and strong downward tropism bends them into the weeping
//! drape. Keeps the historic `lsys_ternary_gravity` slug (seeded scatters key
//! off it) though it is no longer the literal ABOP Fig 2.8 ternary.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct TernaryGravityTree;

impl CatalogueEntry for TernaryGravityTree {
    fn slug(&self) -> &'static str {
        "lsys_ternary_gravity"
    }
    fn name(&self) -> &'static str {
        "Weeping Willow"
    }
    fn description(&self) -> &'static str {
        "Arching limbs cascading into a curtain of weeping leafy whips."
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
    // 0 — medium-brown willow bark.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.36, 0.24, 0.12]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.42, 0.30, 0.16]),
                color_dark: Fp3([0.18, 0.11, 0.05]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — light yellow-green willow leaf (slot 2 matches the grammar's `,(2)`).
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.50, 0.64, 0.28]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.42, 0.58, 0.22]),
                color_edge: Fp3([0.60, 0.72, 0.34]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Trunk drawn in the axiom (never expanded), then A splits into five
        // scaffold limbs that lift (^18) and arch out (&). Each B fans
        // stochastically into 3–5 thin whips; each whip E self-extends one
        // segment per iteration and drops a leaf node K (two stochastic card
        // layouts), occasionally terminating in a leaf tuft (e3). The
        // finalization pass (#910) tufts every still-growing whip tip and
        // expresses fresh K markers so the stem→leaf transition reads
        // finished at every age. Strong downward tropism bends the whips
        // into a vertical cascade around an open centre.
        source_code: "#define s 0.7\n\
                      omega: !(0.45)F(2.4)F(1.8)/(45)A\n\
                      p1: A -> [^(18)&(35)B]/(72)[^(18)&(40)B]/(98)[^(18)&(35)B]/(85)[^(18)&(42)B]/(105)[^(18)&(38)B]\n\
                      b1: 0.4 : B -> !(0.12)F(1.4)[&(18)E]/(95)[&(25)E]/(95)[&(20)E]/(95)[&(24)E]\n\
                      b2: 0.35 : B -> !(0.12)F(1.3)[&(20)E]/(80)[&(27)E]/(110)[&(22)E]/(75)[&(25)E]/(88)[&(19)E]\n\
                      b3: 0.25 : B -> !(0.11)F(1.5)[&(16)E]/(115)[&(24)E]/(105)[&(21)E]\n\
                      e1: 0.5 : E -> F(0.42)K E\n\
                      e2: 0.35 : E -> F(0.38)&(4)K E\n\
                      e3: 0.15 : E -> F(0.3)K ,(2)[~(1,14)]\\(120)[&(15)~(1,12)]\\(115)[^(10)~(1,12)]\n\
                      k1: 0.55 : K -> ,(2)[~(1,13)]\\(70)[~(1,13)]\\(70)[~(1,13)]\\(70)[~(1,13)]\n\
                      k2: 0.45 : K -> ,(2)[^(12)~(1,12)]\\(85)[&(14)~(1,13)]\\(95)[~(1,12)]"
            .to_string(),
        finalization_code:
            "E -> ,(2)[~(1,14)]\\(120)[&(15)~(1,12)]\\(115)[^(10)~(1,12)]\n\
             K -> ,(2)[~(1,13)]\\(70)[~(1,13)]\\(70)[~(1,13)]\\(70)[~(1,13)]"
                .to_string(),
        iterations: 10,
        seed: 1,
        angle: Fp(45.0),
        step: Fp(1.0),
        width: Fp(0.45),
        elasticity: Fp(0.70),
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
        let mut g = TernaryGravityTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
