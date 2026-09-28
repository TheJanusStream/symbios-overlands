//! Apple tree - Ashmere's old croft apple (Malus domestica) in late
//! September, the fruit ripe, from the agent Reeve's region, a Norfolk manor
//! village of about 1300 (#1496). The entry is the generator saved in
//! Ashmere's room record, reproduced to the ten-thousandth; it was built by
//! `docs/agent/examples/trees/apple.py` (session 879, its variant `a`), whose
//! docstring lists the variants tried - card apples among them - and why
//! each was rejected. Change the tree in the builder and judge it in
//! pictures, then copy the saved record here.
//!
//! **Growth.** The [oak](super::lsys_oak)'s grammar scaled to a fruit tree:
//! a dichotomous sympodial crown grown by age, every internode by one law
//! `r*m*min(lm, l0 + dl*age)`, the last fork drawn by its cards alone, every
//! shoot started at its own width and every limb launched from the foot so
//! the trunk is one tube. Against the oak: short internodes, four scaffolds
//! from a bole that leans 11 degrees, a higher elasticity so the laden outer
//! twigs hang, 7-card tip masses and 4-card side sprays.
//!
//! **Fruit.** At finalization a tip bears no apple, one red, one green or
//! two red, by weight (0.45 / 0.3 / 0.1 / 0.15), each hung out through the
//! lower side of its tip's leaves. An apple is solid: a two-segment tube in
//! slot 2, 0.19 m across (a real one is 7-8 cm; this size reads at 50 m),
//! its skin the rings' vertex colours - a red flank over a yellow-green
//! stalk end and a dark red eye, or all yellow-green.
//!
//! **Slots.** 0 bark - a scaly grey-brown Bark texture in short plates,
//! stretched by `;(0.7)`; 1 a Twig card of seven alternate, oval, finely
//! toothed leaves in mid green; 2 the fruit - no texture at all, white, so
//! the vertex colours are the skin.
//!
//! Measured with the render tool: 5.55 m tall and 7.1 x 6.6 m across,
//! 3,890 triangles in 3 parts; the trunk's foot is sunk 0.1 m below the
//! origin so it stays buried on a slope.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

pub struct AppleTree;

impl CatalogueEntry for AppleTree {
    fn slug(&self) -> &'static str {
        "lsys_apple"
    }
    fn name(&self) -> &'static str {
        "Apple tree"
    }
    fn description(&self) -> &'static str {
        "Old croft apple on a leaning bole - a low rounded crown hung with ripe apples."
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
    // 0 - scaly grey-brown bark: near-black furrows, a strong normal and
    // short plates, so it does not read as pale driftwood.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.95),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                scale: Fp64(3.0),
                warp_u: Fp64(0.2),
                warp_v: Fp64(0.4),
                color_light: Fp3([0.1473, 0.1329, 0.1193]),
                color_dark: Fp3([0.0018, 0.0018, 0.0018]),
                normal_strength: Fp(8.0),
                furrow_multiplier: Fp64(0.9),
                furrow_scale_u: Fp64(3.0),
                furrow_scale_v: Fp64(1.0),
                furrow_shape: Fp64(1.5),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - seven alternate, oval, finely toothed leaves in mid green.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.0397, 0.089, 0.0174]),
                    color_edge: Fp3([0.0637, 0.1128, 0.0196]),
                    serration_strength: Fp64(0.14),
                    vein_angle: Fp64(2.4),
                    micro_detail: Fp64(0.2),
                    normal_strength: Fp(0.4),
                    lobe_count: Fp64(0.0),
                    lobe_depth: Fp64(0.0),
                    petiole_length: Fp64(0.08),
                    petiole_width: Fp64(0.018),
                    midrib_width: Fp64(0.07),
                    vein_count: Fp64(7.0),
                    venule_strength: Fp64(0.15),
                    ..Default::default()
                },
                stem_color: Fp3([0.1005, 0.055, 0.0272]),
                stem_half_width: Fp64(0.012),
                leaf_pairs: 7,
                leaf_angle: Fp64(1.9),
                leaf_scale: Fp64(0.32),
                stem_curve: Fp64(0.01),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 - the apples: untextured white, so a texture-less slot draws the
    // tubes' vertex colours (the skin) times white.
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.55),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    GeneratorKind::LSystem {
        source_code: SOURCE_CODE.to_string(),
        finalization_code: FINALIZATION_CODE.to_string(),
        iterations: 10,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.1),
        // Higher than the oak's: the laden outer twigs hang.
        elasticity: Fp(0.06),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(1.0),
        mesh_resolution: 5,
    }
}

/// The apple's growth rules: the leaning bole and its four scaffolds and
/// leader `A` in the axiom, then the apex's fork (`f`), elbow (`e`) and
/// stall (`st`) rules by fork depth, and the growth laws `g1`-`g4`.
const SOURCE_CODE: &str = "\
    #define l0 0.35\n\
    #define dl 0.1\n\
    #define lm 1.2\n\
    #define bw 0.075\n\
    #define vb 1.12\n\
    #define vt 1.11\n\
    #define wt 0.11\n\
    #define G 5\n\
    omega: ;(0.7)!(wt*1.4,-1)f(-0.1)[f(0.1833*l0,0.1833,0,1)&(11)f(0.4*l0,0.4,0,1)/(330)&(62)!(bw*0.85/vb,1)f(0)A(0.85,-1,1)][f(0.1833*l0,0.1833,0,1)&(11)f(0.9833*l0,0.9833,0,1)/(30)&(50)!(bw*1/vb,1)f(0)A(1,1,0)][f(0.1833*l0,0.1833,0,1)&(11)f(0.9833*l0,0.9833,0,1)/(150)&(58)!(bw*0.95/vb,1)f(0)A(0.95,-1,0)][f(0.1833*l0,0.1833,0,1)&(11)f(0.9833*l0,0.9833,0,1)/(265)&(46)!(bw*1/vb,1)f(0)A(1,1,0)]!(wt*1.4,-1)F(0.1833*l0,0.1833,0,1)&(11)!(wt*1.1,-1)F(0.4*l0,0.4,0,1)!(wt*1,-1)F(0.5833*l0,0.5833,0,1)&(8)A(0.85,1,1)\n\
    f1: 0.62 : A(v,s,d) : d < 1 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(18)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$[/(s*40)&(32)!(bw*v*0.7626/vb,1)f(0)A(v*0.82,s,d+1)]F(0.01)/(-s*110)&(34)A(v*0.78,-s,d+1)\n\
    e1: 0.24 : A(v,s,d) : d < 1 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(18)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$/(s*40)&(26)A(v*0.9,-s,d+0.5)\n\
    f2: 0.62 : A(v,s,d) : d >= 1 & d < 2 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(18)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$[f(-l0*v*0.45,-0.45,0,v)/(s*140)&(55)K(v)][/(s*40)&(32)!(bw*v*0.7626/vb,1)f(0)A(v*0.82,s,d+1)]F(0.01)/(-s*110)&(34)A(v*0.78,-s,d+1)\n\
    e2: 0.24 : A(v,s,d) : d >= 1 & d < 2 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(18)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$/(s*40)&(26)A(v*0.9,-s,d+0.5)\n\
    f3: 0.62 : A(v,s,d) : d >= 2 & d < 4 -> !(bw*v,1)F(l0*v,1,0,v)$[f(-l0*v*0.45,-0.45,0,v)/(s*140)&(55)K(v)][/(s*40)&(32)!(bw*v*0.82/vb,1)f(0)A(v*0.82,s,d+1)]/(-s*110)&(34)A(v*0.78,-s,d+1)\n\
    e3: 0.24 : A(v,s,d) : d >= 2 & d < 4 -> !(bw*v,1)F(l0*v,1,0,v)$/(s*40)&(26)A(v*0.9,-s,d+0.5)\n\
    f4: 0.62 : A(v,s,d) : d >= 4 & d < 5 -> f(l0*v,1,0,v)$[/(s*40)&(32)A(v*0.82,s,d+1)]/(-s*110)&(34)A(v*0.78,-s,d+1)\n\
    e4: 0.24 : A(v,s,d) : d >= 4 & d < 5 -> f(l0*v,1,0,v)$/(s*40)&(26)A(v*0.9,-s,d+0.5)\n\
    st: 0.14 : A(v,s,d) : d < G -> A(v,s,d)\n\
    g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g3: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g4: !(w,t) : t < 0 -> !(w*vt,t)";

/// The apple's finalization: a 7-card mass at every tip, bearing its
/// apples by weight, and a 4-card spray at every fork `K`.
const FINALIZATION_CODE: &str = "\
    0.45 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,0.95)][,(1)/(180)&(30)/(-90)~(0,0.9025)][,(1)/(60)&(45)/(40)~(0,0.95)][,(1)/(120)&(50)/(-40)~(0,0.9025)][,(1)/(240)&(50)/(40)~(0,0.9025)][,(1)/(300)&(45)/(-40)~(0,0.95)][,(1)/(45)&(95)/(-90)~(0,0.855)]\n\
    0.3 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,0.95)][,(1)/(180)&(30)/(-90)~(0,0.9025)][,(1)/(60)&(45)/(40)~(0,0.95)][,(1)/(120)&(50)/(-40)~(0,0.9025)][,(1)/(240)&(50)/(40)~(0,0.9025)][,(1)/(300)&(45)/(-40)~(0,0.95)][,(1)/(45)&(95)/(-90)~(0,0.855)][,(2)$/(20)^(48)f(0.6)'(0.233,0.2738,0.022)!(0.1045)f(0)'(0.214,0.0049,0.0039)!(0.19)F(0.0855)'(0.1065,0.0031,0.0031)!(0.095)F(0.0855)]\n\
    0.1 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,0.95)][,(1)/(180)&(30)/(-90)~(0,0.9025)][,(1)/(60)&(45)/(40)~(0,0.95)][,(1)/(120)&(50)/(-40)~(0,0.9025)][,(1)/(240)&(50)/(40)~(0,0.9025)][,(1)/(300)&(45)/(-40)~(0,0.95)][,(1)/(45)&(95)/(-90)~(0,0.855)][,(2)$/(20)^(48)f(0.6)'(0.2738,0.3424,0.0272)!(0.1045)f(0)'(0.253,0.2957,0.0174)!(0.19)F(0.0855)'(0.1329,0.196,0.01)!(0.095)F(0.0855)]\n\
    0.15 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,0.95)][,(1)/(180)&(30)/(-90)~(0,0.9025)][,(1)/(60)&(45)/(40)~(0,0.95)][,(1)/(120)&(50)/(-40)~(0,0.9025)][,(1)/(240)&(50)/(40)~(0,0.9025)][,(1)/(300)&(45)/(-40)~(0,0.95)][,(1)/(45)&(95)/(-90)~(0,0.855)][,(2)$/(20)^(48)f(0.6)'(0.233,0.2738,0.022)!(0.1045)f(0)'(0.214,0.0049,0.0039)!(0.19)F(0.0855)'(0.1065,0.0031,0.0031)!(0.095)F(0.0855)][,(2)$/(160)^(65)f(0.55)'(0.233,0.2738,0.022)!(0.1045)f(0)'(0.214,0.0049,0.0039)!(0.19)F(0.0855)'(0.1065,0.0031,0.0031)!(0.095)F(0.0855)]\n\
    K(v) : * -> $f(0.2)[,(1)/(0)&(25)/(-90)~(0,0.76)][,(1)/(180)&(30)/(-90)~(0,0.722)][,(1)/(60)&(45)/(40)~(0,0.76)][,(1)/(120)&(50)/(-40)~(0,0.722)]";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn apple_tree_is_kept_as_built() {
        let built = AppleTree.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the apple tree");
    }
}
