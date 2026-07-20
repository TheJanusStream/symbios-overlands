//! Acacia — a flat-crowned savanna tree. A short trunk forks into a
//! seed-varied set of limbs whose pitch decays toward horizontal as they
//! extend one segment per iteration (#910), sprouting gold-green leaf-rosette
//! twigs along the way — so iteration count reads as age (sapling fork →
//! spreading juvenile → dense mature parasol) and the crown is irregular
//! rather than a deterministic starburst.

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
        // Age-progressive umbrella acacia (#910). A 3–4 segment trunk (with a
        // stochastic low-fork lean variant a2), then B forks once into 3–5
        // limbs at seed-varied azimuths. Each limb U(a,w) self-extends one
        // segment per iteration while its pitch increment decays
        // geometrically (a*0.55) — the limb launches steeply out (&47–60 from
        // vertical) and asymptotes to horizontal, which is what flattens the
        // crown into the parasol plate. Limbs sprout leaf-rosette twigs V as
        // they grow (the canopy densifies with age) and terminate
        // stochastically (u4, p=0.15) so old crowns stay bounded. The
        // finalization pass caps still-growing tips and expresses freshly
        // spawned V/N markers so no age renders bare tips.
        source_code: "#define s 0.8\n\
                      omega: !(0.34)F(s)F(s*0.95)A\n\
                      a1: 0.5 : A -> F(s*0.9)/(25)B\n\
                      a2: 0.5 : A -> &(4)F(s*0.9)F(s*0.8)/(140)B\n\
                      b1: 0.45 : B -> [&(50)!(0.2)F(s*0.7)U(15,0.17)]/(95)[&(56)!(0.19)F(s*0.65)U(14,0.16)]/(88)[&(52)!(0.2)F(s*0.7)U(16,0.17)]/(105)[&(58)!(0.18)F(s*0.6)U(13,0.15)]\n\
                      b2: 0.4 : B -> [&(54)!(0.2)F(s*0.7)U(14,0.17)]/(75)[&(49)!(0.19)F(s*0.68)U(15,0.16)]/(110)[&(57)!(0.2)F(s*0.66)U(13,0.17)]/(92)[&(51)!(0.18)F(s*0.62)U(15,0.15)]/(70)[&(60)!(0.17)F(s*0.58)U(12,0.14)]\n\
                      b3: 0.15 : B -> [&(47)!(0.21)F(s*0.75)U(16,0.18)]/(120)[&(58)!(0.19)F(s*0.65)U(13,0.16)]/(85)[&(53)!(0.2)F(s*0.7)U(15,0.17)]\n\
                      u1: 0.35 : U(a,w) -> !(w)F(s*0.8)&(a)[+(48)V]U(a*0.55,w*0.85)\n\
                      u2: 0.3 : U(a,w) -> !(w)F(s*0.75)&(a)[-(52)V]U(a*0.55,w*0.85)\n\
                      u3: 0.2 : U(a,w) -> !(w)F(s*0.85)&(a)[+(40)V][-(45)V]U(a*0.5,w*0.82)\n\
                      u4: 0.15 : U(a,w) -> !(w)F(s*0.6)[+(35)V][-(38)V],(1)~(0,30)\n\
                      v1: V -> !(0.1)F(s*0.5)N[+(30)F(s*0.35)N][-(35)F(s*0.3)N]\n\
                      N -> ,(1)[~(0,36)][^(10)~(0,34)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]"
            .to_string(),
        finalization_code: "U(a,w) : * -> ,(1)~(0,30)\n\
             V -> ,(1)~(0,28)\n\
             N -> ,(1)[~(0,36)][^(10)~(0,34)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]/(51)[^(10)~(0,34)][~(0,36)]"
            .to_string(),
        iterations: 6,
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
