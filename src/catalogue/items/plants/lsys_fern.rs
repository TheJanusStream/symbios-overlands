//! Fern — a ground-level rosette of arching fronds (#910 WS2 expansion),
//! shade-layer flora for jungle and wetland floors. The crown adds one
//! frond per iteration (iteration count = age: sprout → spreading rosette)
//! while every existing frond extends with decaying segment length and a
//! strong downward tropism arches it over; leaflet pairs run out each side
//! of the rachis.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Fern;

impl CatalogueEntry for Fern {
    fn slug(&self) -> &'static str {
        "lsys_fern"
    }
    fn name(&self) -> &'static str {
        "Fern"
    }
    fn description(&self) -> &'static str {
        "Ground rosette of arching leafy fronds — forest-floor shade flora."
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
    // 0 — green rachis stem (leaf-toned so the thin stalk blends).
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.16, 0.30, 0.10]),
            roughness: Fp(0.8),
            ..Default::default()
        },
    );
    // 1 — deep forest-floor green leaflet.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.15, 0.34, 0.11]),
            roughness: Fp(0.65),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.10, 0.28, 0.08]),
                color_edge: Fp3([0.20, 0.40, 0.13]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // C emits one frond per iteration at a stochastic launch pitch,
        // rolled near the golden angle so the rosette fills evenly. Each
        // frond D(s) extends one decaying segment per iteration (s*0.8 —
        // bounds total length) with leaflet pairs P at every node; the
        // strong -Y tropism arches mature fronds over. Finalization tips
        // every rachis and expresses fresh P markers.
        source_code: "omega: !(0.03)C\n\
                      c1: 0.6 : C -> [&(40)D(0.18)]/(137)C\n\
                      c2: 0.4 : C -> [&(55)D(0.16)]/(151)C\n\
                      d1: D(s) -> !(0.022)F(s)P&(12)D(s*0.8)\n\
                      p1: P -> ,(1)[+(48)~(0,14)][-(48)~(0,14)]"
            .to_string(),
        finalization_code: "D(s) : * -> ,(1)~(0,12)\n\
             P -> ,(1)[+(48)~(0,14)][-(48)~(0,14)]"
            .to_string(),
        iterations: 7,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.03),
        elasticity: Fp(0.30),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
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
        let mut g = Fern.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
