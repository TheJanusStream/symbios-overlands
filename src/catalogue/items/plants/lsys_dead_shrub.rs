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
            // Dry, sun-bleached grey-brown deadwood.
            base_color: Fp3([0.50, 0.46, 0.40]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.56, 0.52, 0.45]),
                color_dark: Fp3([0.30, 0.27, 0.23]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    GeneratorKind::LSystem {
        // A short stub that immediately forks three ways and WIDE, then BINARY
        // stochastic twisting splits (the \\ and / rolls twist the branch
        // plane out of flat). The apex carries its own width A(w) and shrinks
        // it each split, so branches taper to solid thin twigs instead of
        // collapsing into a fat clod — an open, gnarled, leafless skeleton with
        // sky between the twigs. A windswept sideways+down tropism adds lean.
        source_code: "#define a 55\n\
                      #define s 0.5\n\
                      omega: !(0.2)F(0.35)[+(38)\\(60)A(0.16)][-(34)/(80)A(0.16)][&(20)/(120)A(0.15)]\n\
                      p1: A(w) : 0.5 -> !(w)F(s)[+(a)\\(90)A(w*0.66)][-(a+22)/(70)A(w*0.6)]\n\
                      p2: A(w) : 0.5 -> !(w)F(s)[&(a-12)A(w*0.64)][^(a+16)\\(50)A(w*0.58)]"
            .to_string(),
        finalization_code: String::new(),
        iterations: 5,
        seed: 5,
        angle: Fp(55.0),
        step: Fp(1.0),
        width: Fp(0.2),
        elasticity: Fp(0.4),
        tropism: Some(Fp3([0.15, -1.0, 0.0])),
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
