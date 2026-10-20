//! Poplar - a columnar poplar (#1713): the Lombardy poplar (Populus nigra
//! 'Italica') and its cross the Berlin poplar (P. x berolinensis), the
//! poplars of Berlin's street-tree inventory. A narrow column of steeply
//! ascending branches round one leader, close-set with glossy, deltoid
//! leaves. Berlin's poplars drew as the catalogue's spruce until this tree
//! (`terrain::derived::core::trees`).
//!
//! **Growth.** The leader `A(k)` lays two internodes a step, each in two
//! halves, with a lateral `B(m)` at every half, rolled round the leader by
//! the golden angle (137.5 degrees; `a2` leaves one out, `a3` adds a small
//! fifth): four laterals a step, about fifty at age 12. Internodes lengthen
//! with age to a cap (`g1`), so the leader keeps a steady pace. A lateral is
//! drawn at birth as four tube segments with its leaf sites placed by
//! moves, and then every segment re-lengthens with age to a cap (`g2`,
//! `g3`): the oldest laterals grow no longer than the middle-aged ones, so
//! the crown is a column rather than a cone, and only the youngest, at the
//! top, are short - its spire. A lateral's angle from the leader opens a
//! little with age (`g4`), and the tropism draws every segment UP (+Y): a
//! lateral leaving the leader at 34 degrees curls back toward it, joint by
//! joint, which is what makes the crown fastigiate. The leader thickens by
//! 18 % a step and a lateral by 14 %, so the bole carries its column.
//!
//! **Foliage.** Twig cards of deltoid, serrate leaves on long petioles, all
//! expressed at finalization: `K` along the laterals, four cards splayed
//! round each site; `S` a short spray off a lateral's middle; `T` its tip;
//! `Q` the leader's young tufts; and `B` and `A` for the lateral and the
//! leader born on the last step, which every marker needs to be seen.
//!
//! **Slots.** 0 bark - grey-brown, deeply furrowed into long, narrow plates
//! along the stem; 1 the Twig card, a dark, glossy green. Both carry their
//! colour in the texture under a white base colour, so the variant re-tints
//! the leaf texture itself (`tint_twig`).
//!
//! Measured with the render tool: 14.6 m tall and 2.9 x 3.1 m across - a
//! column five times as tall as it is wide - in 6,072 triangles in 2 parts,
//! as many as the linden-type `lsys_ternary_props` beside it in Berlin's
//! streets (6,176). It ships at iterations 12, the record sanitiser's cap:
//! no seeded stand grows it, so no stand's age needs the headroom.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_twig};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

/// Poplar re-skins. Slot 1 is the Twig leaf card; every texture colour here
/// is linear RGB, as the textures read them, and each call's first colour is
/// the material's `base_color`, kept white.
static VARIANTS: &[PlantVariant] = &[PlantVariant {
    name: "autumn_gold",
    label: "Autumn gold",
    apply: |m| {
        // The column turned butter-yellow, as poplars line an avenue in
        // October: midrib sRGB (0.84, 0.70, 0.11), edge (0.93, 0.82, 0.22).
        tint_twig(
            m,
            1,
            [1.0, 1.0, 1.0],
            [0.67, 0.45, 0.012],
            [0.85, 0.64, 0.04],
        );
    },
}];

pub struct Poplar;

impl CatalogueEntry for Poplar {
    fn slug(&self) -> &'static str {
        "lsys_poplar"
    }
    fn name(&self) -> &'static str {
        "Poplar"
    }
    fn description(&self) -> &'static str {
        "Lombardy poplar - a narrow column of ascending branches, close-set with glossy leaves."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn variants(&self) -> &'static [PlantVariant] {
        VARIANTS
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 - bark: grey-brown plates, long and narrow along the stem, between
    // near-black furrows.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.95),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.17, 0.155, 0.13]),
                color_dark: Fp3([0.028, 0.024, 0.02]),
                normal_strength: Fp(1.4),
                furrow_multiplier: Fp64(0.8),
                furrow_scale_u: Fp64(6.0),
                furrow_scale_v: Fp64(0.7),
                furrow_shape: Fp64(0.55),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - the leaf card: seven pairs of deltoid, finely serrate leaves on
    // long petioles, dark glossy green, sRGB (0.16, 0.32, 0.12) at the
    // midrib (linear RGB here).
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.8),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.022, 0.085, 0.014]),
                    color_edge: Fp3([0.04, 0.125, 0.02]),
                    serration_strength: Fp64(0.12),
                    vein_angle: Fp64(2.0),
                    micro_detail: Fp64(0.15),
                    normal_strength: Fp(0.45),
                    lobe_count: Fp64(0.0),
                    lobe_depth: Fp64(0.0),
                    petiole_length: Fp64(0.12),
                    petiole_width: Fp64(0.016),
                    midrib_width: Fp64(0.06),
                    vein_count: Fp64(6.0),
                    venule_strength: Fp64(0.08),
                    ..Default::default()
                },
                stem_color: Fp3([0.06, 0.05, 0.03]),
                stem_half_width: Fp64(0.006),
                leaf_pairs: 7,
                leaf_angle: Fp64(1.7),
                leaf_scale: Fp64(0.42),
                stem_curve: Fp64(0.006),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    GeneratorKind::LSystem {
        source_code: SOURCE_CODE.to_string(),
        finalization_code: FINALIZATION_CODE.to_string(),
        iterations: 12,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.2),
        tropism: Some(Fp3([0.0, 1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(1.0),
        mesh_resolution: 5,
    }
}

/// The poplar's growth rules: the bare bole in the axiom, the leader
/// `a1`-`a3`, the lateral `b1` as drawn at birth, and the growth laws
/// `g1`-`g10` that age everything already placed.
const SOURCE_CODE: &str = "\
    #define ih 0.25\n\
    #define di 0.055\n\
    #define il 0.62\n\
    #define l0 0.7\n\
    #define dl 0.2\n\
    #define lm 2.4\n\
    #define a0 34\n\
    #define da 0.5\n\
    #define am 40\n\
    #define c0 0.5\n\
    #define dc 0.05\n\
    #define cm 0.9\n\
    #define vb 1.14\n\
    #define vt 1.18\n\
    #define wt 0.032\n\
    #define bw 0.014\n\
    omega: !(wt,-1)F(0.6)!(wt,-1)F(0.6)!(wt,-1)F(0.5)A(0)\n\
    a1: 0.6 : A(k) -> !(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*k)&(a0,0,0)B(1)][/(137.5*k+70)Q(0)]F(ih*0.5,-0.5,0,1)[/(137.5*(k+1))&(a0,0,0)B(0.9)]!(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*(k+2))&(a0,0,0)B(0.95)][/(137.5*(k+2)+70)Q(0)]F(ih*0.5,-0.5,0,1)[/(137.5*(k+3))&(a0,0,0)B(1.05)]A(k+4)\n\
    a2: 0.25 : A(k) -> !(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*k)&(a0,0,0)B(1.08)][/(137.5*k+70)Q(0)]F(ih*0.5,-0.5,0,1)!(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*(k+2))&(a0,0,0)B(0.95)][/(137.5*(k+2)+70)Q(0)]F(ih*0.5,-0.5,0,1)[/(137.5*(k+3))&(a0,0,0)B(1)]A(k+4)\n\
    a3: 0.15 : A(k) -> !(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*k)&(a0,0,0)B(1)][/(137.5*k+97)&(a0+4,0,4)B(0.7)][/(137.5*k+70)Q(0)]F(ih*0.5,-0.5,0,1)[/(137.5*(k+1))&(a0,0,0)B(0.9)]!(wt,-1)F(ih*0.5,-0.5,0,1)[/(137.5*(k+2))&(a0,0,0)B(0.95)][/(137.5*(k+2)+70)Q(0)]F(ih*0.5,-0.5,0,1)[/(137.5*(k+3))&(a0,0,0)B(1.05)]A(k+4)\n\
    b1: B(m) -> $!(bw,1)F(l0*m*0.28,0.28,0,m)[f(l0*m*0.14,0.14,0,m)K(c0,0,0)]K(c0,137,0)!(bw*0.8,1)F(l0*m*0.26,0.26,0,m)[f(l0*m*0.13,0.13,0,m)K(c0,275,0)]K(c0,52,0)[/(90)&(38)S(c0,0)]!(bw*0.6,1)F(l0*m*0.24,0.24,0,m)[f(l0*m*0.12,0.12,0,m)K(c0,190,0)]K(c0,327,0)!(bw*0.4,1)F(l0*m*0.22,0.22,0,m)T(c0)\n\
    g1: F(x,r,g,m) : r < 0 -> F(-r*min(il,ih+di*(g+1)),r,g+1,m)\n\
    g2: F(x,r,g,m) : r >= 0 -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g3: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g4: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)\n\
    g5: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g6: !(w,t) : t < 0 -> !(w*vt,t)\n\
    g7: K(c,j,g) -> K(min(cm,c+dc),j,g+1)\n\
    g8: T(c) -> T(min(cm,c+dc))\n\
    g9: S(c,g) -> S(min(cm,c+dc),g+1)\n\
    g10: Q(g) -> Q(g+1)";

/// The poplar's finalization: every marker becomes its leaf cards.
const FINALIZATION_CODE: &str = "\
    K(c,j,g) : * -> [,(1)&(55)/(j)~(0,c)][,(1)&(68)/(j+95)~(0,c*0.85)][,(1)&(48)/(j-110)~(0,c*0.8)][,(1)&(75)/(j+200)~(0,c*0.7)]\n\
    T(c) : * -> [,(1)&(16)~(0,c)][,(1)&(40)/(120)~(0,c*0.85)][,(1)&(36)/(-120)~(0,c*0.8)]\n\
    S(c,g) : * -> !(0.006)F(c*0.5)[,(1)&(58)/(30)~(0,c*0.75)][,(1)&(62)/(170)~(0,c*0.7)]F(c*0.4)[,(1)&(30)/(-60)~(0,c*0.7)][,(1)&(55)/(100)~(0,c*0.65)]\n\
    B(m) : * -> [,(1)&(30)~(0,0.38)][,(1)&(44)/(137)~(0,0.34)]\n\
    Q(g) : g < 3 -> [,(1)&(26)~(0,0.38)][,(1)/(140)&(32)~(0,0.34)]\n\
    Q(g) : g >= 3 -> \n\
    A(k) : * -> !(0.016)F(0.3)[,(1)&(18)~(0,0.42)][,(1)/(120)&(24)~(0,0.38)][,(1)/(240)&(22)~(0,0.38)]!(0.01)F(0.2)[,(1)/(60)&(10)~(0,0.36)]";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::plants::variant::apply_named;
    use crate::pds::sanitize_generator;

    fn authored_materials() -> HashMap<u16, SovereignMaterialSettings> {
        let GeneratorKind::LSystem { materials, .. } = build_kind() else {
            panic!("the poplar is an L-system");
        };
        materials
    }

    /// What a record keeps is what was built: the sanitiser has nothing to
    /// clamp, cut or drop.
    #[test]
    fn poplar_is_kept_as_built() {
        let built = Poplar.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the poplar");
    }

    /// A texture's colours taken out of it: its colour fields in order, and
    /// the texture with each of them blacked out, so that everything else in
    /// it can be compared whole. The poplar wears only these two kinds.
    fn split_colours(texture: &SovereignTextureConfig) -> (Vec<Fp3>, SovereignTextureConfig) {
        let mut rest = texture.clone();
        let fields = match &mut rest {
            SovereignTextureConfig::Bark(bark) => vec![&mut bark.color_light, &mut bark.color_dark],
            SovereignTextureConfig::Twig(twig) => vec![
                &mut twig.stem_color,
                &mut twig.leaf.color_base,
                &mut twig.leaf.color_edge,
            ],
            other => panic!("the poplar wears a Bark and a Twig texture, not {other:?}"),
        };
        let colours = fields
            .into_iter()
            .map(|colour| std::mem::replace(colour, Fp3([0.0; 3])))
            .collect();
        (colours, rest)
    }

    /// Each re-skin recolours the texture of the slots it names, and only
    /// those, and touches nothing of a texture but its colours.
    #[test]
    fn every_variant_recolours_the_textures_it_names() {
        let expected: &[(&str, &[u16])] = &[("autumn_gold", &[1])];
        assert_eq!(
            VARIANTS.iter().map(|v| v.name).collect::<Vec<_>>(),
            expected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            "a poplar variant without a line here is not checked"
        );
        let authored = authored_materials();
        for (variant, (_, slots)) in VARIANTS.iter().zip(expected) {
            let mut m = authored.clone();
            (variant.apply)(&mut m);
            for slot in [0u16, 1] {
                let (colours_before, rest_before) = split_colours(&authored[&slot].texture);
                let (colours_after, rest_after) = split_colours(&m[&slot].texture);
                assert_eq!(
                    rest_after, rest_before,
                    "{}: slot {slot}'s texture changed beyond its colours",
                    variant.name
                );
                assert_eq!(
                    colours_after != colours_before,
                    slots.contains(&slot),
                    "{}: slot {slot}'s texture colours",
                    variant.name
                );
            }
        }
    }

    /// A name the poplar does not know leaves its authored materials alone.
    #[test]
    fn an_unknown_variant_changes_nothing() {
        let authored = authored_materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "no_such_variant", &mut m);
        assert_eq!(m, authored);
    }
}
