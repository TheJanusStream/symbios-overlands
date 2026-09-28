//! Bush - a leafy rounded shrub (#910 WS2 expansion), the first true woody
//! understory species. Four stems splay from a common base and fork
//! stochastically each iteration into a dense dome of mid-green foliage
//! about 1.6 m tall - hedge-scale filler between grass and trees in
//! temperate biomes.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

pub struct Bush;

impl CatalogueEntry for Bush {
    fn slug(&self) -> &'static str {
        "lsys_bush"
    }
    fn name(&self) -> &'static str {
        "Bush"
    }
    fn description(&self) -> &'static str {
        "Rounded leafy shrub - multi-stemmed woody understory filler."
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
    // 0 - grey-brown twiggy bark.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.32, 0.26, 0.18]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.38, 0.31, 0.22]),
                color_dark: Fp3([0.17, 0.13, 0.09]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - mid-green hedge leaf.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.24, 0.42, 0.16]),
            roughness: Fp(0.7),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.16, 0.34, 0.11]),
                color_edge: Fp3([0.28, 0.46, 0.16]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    grammar(materials, 0.045, 8)
}

/// The bush's growth rules. Four stems splay from the base; each stem `S`
/// forks stochastically (two-fork / two-fork-tilted / extend) with leaf
/// markers `K` at every node.
///
/// RETROSPECTIVE GROWTH (#917, ABOP eq. 1.10): new internodes are
/// emitted at a fixed base length and then every ALREADY-PLACED
/// internode elongates (`F(l) -> F(l*lr)`) and thickens
/// (`!(w) -> !(w*vr)`) on every subsequent step. ABOP proves this
/// yields the same geometry as computing a tip's final size at
/// birth, but as a developmental sequence rather than a fractal -
/// so iteration count IS the plant's age, old wood is genuinely
/// thicker than new wood, and the base accumulates girth. `vr` is
/// under the da Vinci binary value (√2 ≈ 1.414) because a shrub's
/// stems stay whippy rather than reading as load-bearing trunks.
///
/// BASITONY: the shrub habit. Lateral vigour must FALL with height
/// so the lowest branches are longest and the silhouette domes -
/// hence contraction (0.78–0.86) on every fork with no privileged
/// leader, the inverse of a tree's acrotonic crown.
const SOURCE_CODE: &str = "#define lr 1.06\n\
    #define vr 1.16\n\
    omega: !(0.07)[&(24)S(0.2,0.045)]/(95)[&(28)S(0.21,0.045)]/(120)[&(22)S(0.2,0.045)]/(85)[&(8)S(0.23,0.05)]\n\
    s1: 0.45 : S(l,w) -> !(w)F(l)K[+(38)/(60)S(l*0.82,w*0.78)][-(32)/(110)S(l*0.8,w*0.78)]\n\
    s2: 0.3 : S(l,w) -> !(w)F(l*0.9)K[&(28)/(85)S(l*0.85,w*0.8)][^(24)/(200)S(l*0.78,w*0.75)]\n\
    s3: 0.25 : S(l,w) -> !(w)F(l)K/(120)S(l*0.86,w*0.8)\n\
    g1: F(l) : * -> F(l*lr)\n\
    g2: !(w) : * -> !(w*vr)\n\
    k1: K -> ,(1)[~(0,15)]\\(115)[&(20)~(0,14)]\\(125)[^(18)~(0,14)]";

/// The bush's finalization: tufts every stem apex and expresses the leaf
/// markers born on the last step.
const FINALIZATION_CODE: &str = "S(l,w) : * -> ,(1)[~(0,16)]\\(120)[~(0,15)]\\(120)[&(15)~(0,14)]\n\
    K -> ,(1)[~(0,15)]\\(115)[&(20)~(0,14)]\\(125)[^(18)~(0,14)]";

/// The bush's grammar dressed in `materials` (slot 0 the bark, slot 1 the
/// leaf card), a leaf card `prop_scale` metres to the unit and each tube
/// `mesh_resolution` segments round.
///
/// Every species that is this shrub in other clothes grows from here, so
/// the grammar has one copy: Ashmere's [hazel](super::lsys_hazel) and
/// [gorse](super::lsys_gorse) (#1496) are the bush in their own materials,
/// with coarser tubes, each stretched to its own size by its root
/// transform - the hazel's leaf card cut to match its stretch, so its
/// leaves stay the bush's size.
pub(super) fn grammar(
    materials: HashMap<u16, SovereignMaterialSettings>,
    prop_scale: f32,
    mesh_resolution: u32,
) -> GeneratorKind {
    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        source_code: SOURCE_CODE.to_string(),
        finalization_code: FINALIZATION_CODE.to_string(),
        iterations: 6,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.07),
        elasticity: Fp(0.05),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(prop_scale),
        mesh_resolution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Bush.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
