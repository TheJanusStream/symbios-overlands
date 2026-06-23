//! Acacia — a flat-crowned savanna tree. A short trunk that splays into
//! four near-horizontal limbs, each forking into leafy twigs tipped with
//! broad gold-green leaf props, giving the wide umbrella silhouette of the
//! savanna. The crown is built deterministically so it stays flat and wide.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Acacia;

impl CatalogueEntry for Acacia {
    fn slug(&self) -> &'static str {
        "lsys_acacia"
    }
    fn name(&self) -> &'static str {
        "Acacia"
    }
    fn description(&self) -> &'static str {
        "Flat-crowned umbrella acacia with gold-green foliage — the savanna tree."
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
    // 0 — pale savanna bark.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.40, 0.30, 0.20]),
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
    // 1 — dusty gold-green savanna leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.50, 0.54, 0.28]),
            roughness: Fp(0.7),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.36, 0.42, 0.16]),
                color_edge: Fp3([0.58, 0.54, 0.24]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // Short trunk, then THREE interleaved tiers of near-horizontal limbs
        // (8 long M at &82, 8 mid P at &76, 6 inner Q at &70) plus a flat
        // central fill R — each carrying overlapping leaf rosettes (N/Nf) — so
        // the foliage merges into a DENSE, level, opaque parasol plate with a
        // filled centre rather than a sparse star-burst. The iconic flat-crowned
        // umbrella acacia, much wider than it is tall. Tropism stays None so the
        // canopy stays flat and up.
        source_code: "#define s 0.8\n\
                      omega: !(0.34)F(s)F(s)/(30)C\n\
                      p1: C -> [&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(45)[&(82)M]/(22)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(45)[&(76)P]/(30)[&(70)Q]/(60)[&(70)Q]/(60)[&(70)Q]/(60)[&(70)Q]/(60)[&(70)Q]/(60)[&(70)Q]R\n\
                      p2: M -> !(0.16)N F(s*1.1)N F(s*1.0)N[+(35)F(s*0.6)N][-(35)F(s*0.6)N]F(s*0.9)N &(18)F(s*0.7)N\n\
                      p3: P -> !(0.14)N F(s*0.85)N F(s*0.75)N F(s*0.6)N\n\
                      p4: Q -> !(0.12)N F(s*0.55)N F(s*0.4)N\n\
                      p5: R -> ,(1)Nf[&(55)F(s*0.4)Nf]/(72)[&(80)F(s*0.6)Nf]/(72)[&(80)F(s*0.6)Nf]/(72)[&(80)F(s*0.6)Nf]/(72)[&(80)F(s*0.6)Nf]/(72)[&(80)F(s*0.6)Nf]\n\
                      N -> ,(1)[~(0,36)][^(10)~(0,34)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]\n\
                      Nf -> ,(1)[~(0,36)][~(0,34)]/(60)[~(0,34)][~(0,36)]/(60)[~(0,34)][~(0,36)]/(60)[~(0,34)][~(0,36)]/(60)[~(0,34)][~(0,36)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 4,
        seed: 1,
        angle: Fp(70.0),
        step: Fp(1.0),
        width: Fp(0.34),
        elasticity: Fp(0.0),
        tropism: None,
        materials,
        prop_mappings,
        prop_scale: Fp(0.067),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Acacia.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
