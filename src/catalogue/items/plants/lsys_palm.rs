//! Palm — a tall bare trunk crowned by a whorl of drooping fronds. A
//! fixed-length trunk (drawn in the axiom so iteration count can't blow it
//! up) carries a deterministic crown of five fronds that radiate and droop,
//! each tipped with a broad leaf prop. The signature coastal / tropical
//! scatter where a conifer reads wrong.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Palm;

impl CatalogueEntry for Palm {
    fn slug(&self) -> &'static str {
        "lsys_palm"
    }
    fn name(&self) -> &'static str {
        "Palm"
    }
    fn description(&self) -> &'static str {
        "Tall bare trunk crowned by a whorl of drooping fronds — a coastal palm."
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
    // 0 — fibrous brown trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.42, 0.30, 0.16]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.45, 0.32, 0.18]),
                color_dark: Fp3([0.20, 0.13, 0.07]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 — deep tropical green frond leaflet.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.24, 0.46, 0.18]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.12, 0.34, 0.10]),
                color_edge: Fp3([0.20, 0.42, 0.12]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Fixed six-segment bare trunk in the axiom (so iteration count can't
        // blow it up), then a whorl C of six fronds spread radially by roll.
        // Each frond launches up-and-out (^18 &32) then its rachis D pitches
        // ever more steeply down (&3→48) so the FEATHERED blade arcs over and
        // droops past horizontal — leaflet pairs P run out each side. A gravity
        // tropism adds the graceful sag of a coastal palm fountain crown.
        source_code: "#define s 0.9\n\
                      omega: !(0.26)F(s)F(s)F(s)F(s)F(s)F(s)C\n\
                      p1: C -> /(60)[^(18)&(32)D]/(60)[^(18)&(32)D]/(60)[^(18)&(32)D]/(60)[^(18)&(32)D]/(60)[^(18)&(32)D]/(60)[^(18)&(32)D]\n\
                      p2: D -> !(0.06)F(s*0.95)&(3)F(s*0.9)P&(7)F(s*0.9)P&(12)F(s*0.85)P&(22)F(s*0.75)P&(34)F(s*0.6)P&(48)F(s*0.45)P&(20)F(s*0.35)P\n\
                      p3: P -> ,(1)[+(55)~(0,36)][-(55)~(0,36)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 3,
        seed: 1,
        angle: Fp(60.0),
        step: Fp(1.0),
        width: Fp(0.26),
        elasticity: Fp(0.45),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.055),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Palm.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
