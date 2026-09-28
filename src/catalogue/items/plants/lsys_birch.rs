//! Birch - a silver birch (Betula pendula): a white trunk marked with black
//! lenticel dashes over a black foot, and long hanging strands of small
//! leaves. It is the Understory's `pale_birch` as it stands in Ashmere, the
//! agent Reeve's region (#1496), and it replaced the #910 birch because the
//! owner judged it the better tree. The slug and the variant names are kept,
//! so every seeded stand that names `lsys_birch` grows this tree now. The
//! entry is the generator saved in Ashmere's room record, reproduced to the
//! ten-thousandth; it was built by `docs/agent/examples/trees/birch.py`
//! (session 878, its variant `a`), whose docstring lists the variants and
//! settings tried and why each was rejected. Change the tree in the builder
//! and judge it in pictures, then copy the saved record here.
//!
//! **Growth.** The leader `A(k)` lays two trunk internodes a step, each
//! carrying a main branch `B(m)` rolled round the trunk by the golden angle
//! (137.5 degrees; `a2` skips one, `a3` adds a third) and a leaf tuft `Q`.
//! A branch is drawn at birth as four tube segments with its leaf sites
//! placed by moves, and then every segment re-lengthens with age (`g1`,
//! `g2`: growing to a cap and dying back past age 5, as a shaded low limb
//! does), its angle from the trunk sinks (`g3`) and its joints droop (`g4`),
//! slowly, so young limbs ascend and only old ones arch over. New wood is
//! tinted dark red-brown by a four-argument vertex colour that `g10` whitens
//! a step at a time: the trunk and old limbs turn white, the fine
//! branchlets stay dark.
//!
//! **Foliage.** Hanging Twig-card strands, all expressed at finalization:
//! `K` along the limbs (two cards end to end, rolled apart so a strand is
//! not one flat ladder of leaves), `S` short branchlets with strands of
//! their own, `T` the limb tips and `Q` the young leader's tufts. Strands
//! grow with age, shrink on old low limbs past age 6 and lose their second
//! strand from age 9, so the crown is ovoid, widest a little under half way
//! up. Every card hangs some 45 degrees or more off level, so none flares
//! white toward a low sun.
//!
//! **Slots.** 0 bark - a Lichen texture used as bark: white "rock" with dark
//! colonies, squashed along each tube by `;(3)` into horizontal lenticel
//! dashes, with vertex-colour rings for the black foot and two dark bands
//! under the lowest limbs; 1 a Twig card, a thin zigzag stem with eight
//! alternate deltoid, serrate leaves. Both carry their colour in the
//! texture, under a near-white base colour that only multiplies it, so the
//! variants re-tint the textures themselves (`tint_lichen`, `tint_twig`): a
//! base colour can darken a green leaf but never turn it gold.
//!
//! Measured with the render tool: 9.2 m tall and 4.4 x 4.8 m across, 4,316
//! triangles in 2 parts (the #910 birch was 6.9 m tall, in 5,166
//! triangles). It ships at iterations 12, the record sanitiser's cap, which
//! leaves no headroom for a seeded stand's age:
//! `seeded_defaults::room::build` adds the stand's `iterations_delta` (-1
//! to +1) and clamps the sum to the cap, so a stand that rolls +1 grows
//! this tree at 12 all the same.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_lichen, tint_twig};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignLeafConfig,
    SovereignLichenConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
};

/// Birch re-skins (#910), re-authored for this birch's textures (#1496).
/// Slot 0 is the Lichen bark, slot 1 the Twig leaf card. Every texture
/// colour here is linear RGB, as the textures read them; each call's first
/// colour is the material's `base_color`, which is sRGB, kept at the
/// authored near-white.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "autumn_gold",
        label: "Autumn gold",
        apply: |m| {
            // The species' signature season - butter-yellow leaves on the
            // white bark, the birch that reads instantly at any distance.
            // Midrib sRGB (0.86, 0.68, 0.12), edge (0.94, 0.80, 0.24); the
            // base colour stays white so the leaf's own colour shows.
            tint_twig(
                m,
                1,
                [1.0, 1.0, 1.0],
                [0.71, 0.42, 0.013],
                [0.87, 0.60, 0.047],
            );
        },
    },
    PlantVariant {
        name: "dark_bark",
        label: "Dark-barked (river birch)",
        apply: |m| {
            // Warm, dark, shaggy bark instead of the chalk-white - a
            // different species read from the same skeleton, for wetter
            // ground. The bark's face is a red-brown, sRGB (0.42, 0.27,
            // 0.19); the colonies that were lenticels are darker flakes, and
            // their pale salmon rims, sRGB (0.80, 0.62, 0.50), read as the
            // papery curls a river birch sheds. Its leaf is a deeper green.
            tint_lichen(
                m,
                0,
                [0.95, 0.95, 0.93],
                [0.147, 0.059, 0.030],
                [[0.022, 0.010, 0.006], [0.055, 0.022, 0.012]],
                [0.60, 0.34, 0.21],
            );
            tint_twig(
                m,
                1,
                [1.0, 1.0, 1.0],
                [0.055, 0.162, 0.017],
                [0.095, 0.214, 0.027],
            );
        },
    },
];

pub struct Birch;

impl CatalogueEntry for Birch {
    fn slug(&self) -> &'static str {
        "lsys_birch"
    }
    fn name(&self) -> &'static str {
        "Birch"
    }
    fn description(&self) -> &'static str {
        "Silver birch - a white, black-marked trunk under hanging strands of small leaves."
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
    // 0 - white bark: a Lichen texture's pale "rock" marked with near-black
    // colonies (the lenticels) edged in grey-brown.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.95, 0.95, 0.93]),
            roughness: Fp(0.9),
            texture: SovereignTextureConfig::Lichen(SovereignLichenConfig {
                // Fewer, larger colonies: black dashes at 15 m.
                patch_scale: Fp64(1.75),
                patch_octaves: 3,
                coverage: Fp64(0.32),
                rim_width: Fp64(0.1),
                species_scale: Fp64(1.5),
                color_rock: Fp3([0.8689, 0.8481, 0.7874]),
                color_lichen_a: Fp3([0.01, 0.0085, 0.0085]),
                color_lichen_b: Fp3([0.0732, 0.0593, 0.0509]),
                color_rim: Fp3([0.3185, 0.2957, 0.2633]),
                grain_scale: Fp64(30.0),
                grain_strength: Fp64(0.25),
                relief: Fp64(0.4),
                normal_strength: Fp(1.2),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - the hanging strand's card: eight alternate deltoid, serrate leaves
    // and a terminal one, all pointing down a slightly zigzag stem, fresh
    // green (linear RGB).
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.1329, 0.2738, 0.022]),
                    color_edge: Fp3([0.233, 0.3672, 0.0397]),
                    serration_strength: Fp64(0.16),
                    vein_angle: Fp64(2.2),
                    micro_detail: Fp64(0.2),
                    // Weak veins, so the strand reads as a twig rather than
                    // a pinnate leaf.
                    normal_strength: Fp(0.35),
                    lobe_count: Fp64(0.0),
                    lobe_depth: Fp64(0.0),
                    petiole_length: Fp64(0.06),
                    petiole_width: Fp64(0.018),
                    midrib_width: Fp64(0.07),
                    vein_count: Fp64(7.0),
                    venule_strength: Fp64(0.1),
                    ..Default::default()
                },
                // Thin and light, so a card seen edge-on is a faint line.
                stem_color: Fp3([0.1329, 0.0732, 0.0509]),
                stem_half_width: Fp64(0.006),
                // Eight at 0.30 of the card: smaller leaves mip away and
                // left bare crowns at 55 m.
                leaf_pairs: 8,
                leaf_angle: Fp64(2.2),
                leaf_scale: Fp64(0.3),
                stem_curve: Fp64(0.008),
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
        elasticity: Fp(0.06),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(1.0),
        mesh_resolution: 5,
    }
}

/// The birch's growth rules: the bare trunk's colour rings in the axiom,
/// the leader `a1`-`a3`, the branch `b1` as drawn at birth, and the growth
/// laws `g1`-`g11` that age everything already placed.
const SOURCE_CODE: &str = "\
    #define l0 0.75\n\
    #define dl 0.4\n\
    #define lm 2.35\n\
    #define a0 34\n\
    #define da 4\n\
    #define am 68\n\
    #define ar 1.3\n\
    #define ax 11\n\
    #define c0 0.4\n\
    #define dc 0.06\n\
    #define cm 0.72\n\
    #define s0 0.3\n\
    #define ds 0.14\n\
    #define sm 1.3\n\
    #define vb 1.16\n\
    #define vt 1.13\n\
    #define ih 0.27\n\
    #define wt 0.03\n\
    #define bw 0.012\n\
    omega: ;(3)'(0.12)!(wt,-1)F(0.34)&(7)'(0.4)!(wt,-1)F(0.22)'(0.95)!(wt,-1)F(0.29)'(1)!(wt,-1)F(0.49)'(0.22)!(wt,-1)F(0.08)'(1)!(wt,-1)F(0.08)'(1)!(wt,-1)F(0.52)'(0.16)!(wt,-1)F(0.08)'(0.2)!(wt,-1)F(0.1)'(1)!(wt,-1)F(0.1)A(0)\n\
    a1: 0.5 : A(k) -> '(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+0))&(a0,0,0)B(1)][/(137.5*(k+0)+70)Q(0)]'(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+1))&(a0+4,0,4)B(0.86)][/(137.5*(k+1)+70)Q(0)]A(k+2)\n\
    a2: 0.3 : A(k) -> '(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+0))&(a0-3,0,-3)B(1.08)][/(137.5*(k+0)+70)Q(0)]'(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+1)+70)Q(0)]A(k+2)\n\
    a3: 0.2 : A(k) -> '(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+0))&(a0+2,0,2)B(0.92)][/(137.5*(k+0)+70)Q(0)]'(0.42,0.33,0.28,1)!(wt,-1)^(0.35)F(ih)[/(137.5*(k+1))&(a0-2,0,-2)B(1)][/(97)/(137.5*(k+1))&(a0+6,0,6)B(0.7)][/(137.5*(k+1)+70)Q(0)]A(k+2)\n\
    b1: B(m) -> '(0.42,0.33,0.28,1)$!(bw,1)[f(l0*m*0.18,0.18,0,m)K(c0,0,m*0.72,0)][f(l0*m*0.29,0.29,0,m)S(s0,1,0)]F(l0*m*0.32,0.32,0,m)!(bw*0.744,1)^(0,0,0.8)[f(l0*m*0.05,0.05,0,m)K(c0,160,m*0.86,0)][f(l0*m*0.14,0.14,0,m)K(c0,-40,m*0.8,0)][f(l0*m*0.23,0.23,0,m)S(s0,-1,0)]F(l0*m*0.27,0.27,0,m)!(bw*0.528,1)^(0,0,1.2)[f(l0*m*0.03,0.03,0,m)K(c0,100,m*1.04,0)][f(l0*m*0.11,0.11,0,m)K(c0,-120,m*0.92,0)][f(l0*m*0.19,0.19,0,m)K(c0,30,m*1.12,0)]F(l0*m*0.23,0.23,0,m)!(bw*0.344,1)^(0,0,1.6)[f(l0*m*0.04,0.04,0,m)K(c0,200,m*0.98,0)][f(l0*m*0.11,0.11,0,m)K(c0,-70,m*1.08,0)]F(l0*m*0.18,0.18,0,m)T(c0)\n\
    g1: F(x,r,g,m) -> F(r*m*max(0.35*lm,min(lm,l0+dl*(g+1))-0.25*max(0,g+1-5)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*max(0.35*lm,min(lm,l0+dl*(g+1))-0.25*max(0,g+1-5)),r,g+1,m)\n\
    g3: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)\n\
    g4: ^(a,g,k) -> ^(min(ax,ar*(g+1))*k,g+1,k)\n\
    g5: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g6: !(w,t) : t < 0 -> !(w*vt,t)\n\
    g7: K(c,j,q,g) -> K(max(0.6*c0,min(cm,c0+dc*(g+1))-0.05*max(0,g+1-6)),j,q,g+1)\n\
    g8: T(c) -> T(min(cm,c+dc))\n\
    g9: S(c,s,g) -> S(max(s0,min(sm,s0+ds*(g+1))-0.1*max(0,g+1-5)),s,g+1)\n\
    g10: '(r,g,b,a) -> '(min(0.97,r+0.275),min(0.96,g+0.315),min(0.94,b+0.33),a)\n\
    g11: Q(g) -> Q(g+1)";

/// The birch's finalization: every marker becomes its hanging strands.
const FINALIZATION_CODE: &str = "\
    K(c,j,q,g) : g < 9 -> [,(1)^(112)/(j)~(0,c)f(c*0.82)/(55)~(0,c*q*0.75)][,(1)^(97)/(j+95)~(0,c*0.8)f(c*0.8*0.82)/(-55)~(0,c*0.8*0.6)][,(1)^(124)/(j-110)~(0,c*0.7)]\n\
    K(c,j,q,g) : g >= 9 -> [,(1)^(112)/(j)~(0,c)f(c*0.82)/(55)~(0,c*q*0.75)][,(1)^(124)/(j-110)~(0,c*0.7)]\n\
    T(c) : * -> [,(1)^(45)/(70)~(0,c*0.8)][,(1)^(89.6)/(60)~(0,c*0.9)f(c*0.9*0.82)/(55)~(0,c*0.9*0.75)][,(1)^(100.8)/(-50)~(0,c)][,(1)^(70)/(-110)~(0,c*0.7)]\n\
    S(c,s,g) : c >= 0.55 -> '(0.42,0.34,0.3)!(0.009)+(s*50)$^(14)[f(c*0.45)[,(1)^(107)/(40)~(0,c*0.6)f(c*0.6*0.82)/(-55)~(0,c*0.6*0.75)][,(1)^(97)/(-60)~(0,c*0.55)]]F(c)[,(1)^(87)/(-30)~(0,c*0.65)f(c*0.65*0.82)/(55)~(0,c*0.65*0.75)][,(1)^(67)/(80)~(0,c*0.55)][,(1)^(40)/(20)~(0,c*0.5)]\n\
    S(c,s,g) : c < 0.55 -> +(s*50)$^(14)[,(1)^(107)/(40)~(0,c*0.6)f(c*0.6*0.82)/(-55)~(0,c*0.6*0.75)][,(1)^(97)/(-60)~(0,c*0.55)][,(1)^(87)/(-30)~(0,c*0.65)f(c*0.65*0.82)/(55)~(0,c*0.65*0.75)][,(1)^(67)/(80)~(0,c*0.55)][,(1)^(40)/(20)~(0,c*0.5)]\n\
    B(m) : * -> $[,(1)^(10)~(0,0.42)][,(1)^(60)/(90)~(0,0.4)][,(1)^(80)/(-80)~(0,0.38)]\n\
    Q(g) : g < 4 -> [,(1)&(38)~(0,0.44)][,(1)/(150)&(42)~(0,0.4)][,(1)/(-100)&(34)~(0,0.36)]\n\
    Q(g) : g >= 4 -> \n\
    A(k) : * -> !(0.02)[[,(1)&(35)~(0,0.45)][,(1)/(120)&(40)~(0,0.45)][,(1)/(240)&(38)~(0,0.42)]]F(0.28)[,(1)/(60)&(22)~(0,0.42)][,(1)/(180)&(25)~(0,0.4)][,(1)/(300)&(20)~(0,0.4)]!(0.012)F(0.2)[,(1)/(30)&(10)~(0,0.36)][,(1)/(200)~(0,0.38)]";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::plants::variant::apply_named;
    use crate::pds::sanitize_generator;

    fn authored_materials() -> HashMap<u16, SovereignMaterialSettings> {
        let GeneratorKind::LSystem { materials, .. } = build_kind() else {
            panic!("the birch is an L-system");
        };
        materials
    }

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn birch_is_kept_as_built() {
        let built = Birch.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the birch");
    }

    /// A texture's colours taken out of it: its colour fields in order, and
    /// the texture with each of them blacked out, so that everything else in
    /// it can be compared whole. The birch wears only these two kinds.
    fn split_colours(texture: &SovereignTextureConfig) -> (Vec<Fp3>, SovereignTextureConfig) {
        let mut rest = texture.clone();
        let fields = match &mut rest {
            SovereignTextureConfig::Lichen(lichen) => vec![
                &mut lichen.color_rock,
                &mut lichen.color_lichen_a,
                &mut lichen.color_lichen_b,
                &mut lichen.color_rim,
            ],
            SovereignTextureConfig::Twig(twig) => vec![
                &mut twig.stem_color,
                &mut twig.leaf.color_base,
                &mut twig.leaf.color_edge,
            ],
            other => panic!("the birch wears a Lichen and a Twig texture, not {other:?}"),
        };
        let colours = fields
            .into_iter()
            .map(|colour| std::mem::replace(colour, Fp3([0.0; 3])))
            .collect();
        (colours, rest)
    }

    /// Each re-skin recolours the TEXTURE of the slots it names, and only
    /// those: a named slot's texture colours change, an unnamed slot's stay
    /// as authored, and no variant touches a texture's other fields. Both
    /// slots carry their colour in the texture under a near-white base
    /// colour, so a variant that reached `base_color` alone - as the #910
    /// variants would have, calling `tint_bark` and `tint_leaf` on a Lichen
    /// and a Twig slot - could only multiply the texture's own colours:
    /// darken a green leaf, never turn it gold.
    #[test]
    fn every_variant_recolours_the_textures_it_names() {
        let expected: &[(&str, &[u16])] = &[("autumn_gold", &[1]), ("dark_bark", &[0, 1])];
        assert_eq!(
            VARIANTS.iter().map(|v| v.name).collect::<Vec<_>>(),
            expected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            "a birch variant without a line here is not checked"
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
                let recoloured = colours_after != colours_before;
                assert_eq!(
                    recoloured,
                    slots.contains(&slot),
                    "{}: slot {slot}'s texture colours {}",
                    variant.name,
                    if recoloured {
                        "changed, and the variant does not name it"
                    } else {
                        "did not change"
                    }
                );
            }
        }
    }

    /// A name the birch does not know leaves its authored materials alone,
    /// the fallback the seeded pools rely on.
    #[test]
    fn an_unknown_variant_changes_nothing() {
        let authored = authored_materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "no_such_variant", &mut m);
        assert_eq!(m, authored);
    }
}
