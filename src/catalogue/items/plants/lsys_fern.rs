//! Fern — a ground-level rosette of arching fronds (#910 WS2 expansion),
//! shade-layer flora for jungle and wetland floors. The crown adds one
//! frond per iteration (iteration count = age: sprout → spreading rosette)
//! while every existing frond extends with decaying segment length and a
//! strong downward tropism arches it over; leaflet pairs run out each side
//! of the rachis.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignFrondConfig,
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
    // 1 — deep forest-floor green leaflet. A fine pinnatifid pinna: narrow,
    // with a lobed (crenate) margin so each leaflet reads as a fern pinnule
    // rather than a smooth broadleaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.15, 0.34, 0.11]),
            roughness: Fp(0.65),
            texture: SovereignTextureConfig::Frond(SovereignFrondConfig {
                color_base: Fp3([0.10, 0.28, 0.08]),
                color_edge: Fp3([0.20, 0.40, 0.13]),
                width: Fp64(0.11),
                tip_taper: Fp64(1.6),
                vein_count: Fp64(7.0),
                lobe_count: Fp64(5.0),
                lobe_depth: Fp64(0.4),
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
        // frond D(s,u) extends one decaying segment per iteration (s*0.82,
        // bounding total length) with leaflet pairs P at every node.
        //
        // CIRCINATE VERNATION (#917): `u` is the frond's residual coil
        // angle, applied as the per-segment pitch and decaying 0.62× per
        // step. A freshly emitted frond still carries ~55° of curl per
        // segment — the tight crozier/fiddlehead of a young fern — while
        // the oldest frond has relaxed to a few degrees and lies open.
        // Because the rosette emits one frond per iteration, every age of
        // frond coexists on one plant, which is exactly how a real fern
        // reads. Finalization tips every rachis and expresses fresh P.
        source_code: "omega: !(0.03)C\n\
                      c1: 0.6 : C -> [&(40)D(0.18,55)]/(137.5)C\n\
                      c2: 0.4 : C -> [&(55)D(0.16,62)]/(138.2)C\n\
                      d1: D(s,u) -> !(0.022)F(s)P&(u)D(s*0.82,u*0.62)\n\
                      p1: P -> ,(1)[+(48)~(0,14)][-(48)~(0,14)]"
            .to_string(),
        finalization_code: "D(s,u) : * -> ,(1)~(0,12)\n\
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
