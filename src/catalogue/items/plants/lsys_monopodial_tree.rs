//! Spruce - a dark, conical spruce in tiers: a straight leader ringed by a
//! whorl of branches every step, the oldest and lowest held near level, the
//! youngest angled up into a narrow spire, every branch a bottlebrush of
//! needle sprays. It is the Understory's `dark_conifer` as it stands in the
//! agent Hypha's region (#1708), and it replaced the ABOP Fig 2.6
//! monopodial conifer because the owner judged it the better tree. The slug
//! and the variant names are kept, so every seeded stand that names
//! `lsys_monopodial_tree` grows this tree now. The entry is the generator
//! saved in the Understory's room record, reproduced to the ten-thousandth,
//! root transform included; it was built by
//! `docs/agent/examples/trees/spruce.py` (session 878, its variant `d`),
//! whose docstring lists the variants tried and why each was rejected.
//! Change the tree in the builder and judge it in pictures, then copy the
//! saved record here.
//!
//! **Growth.** Developmental: the iteration count is the tree's age, and
//! everything already placed keeps growing. The leader `A` lays one trunk
//! internode a step and rings it with a whorl of branches `B(m)` - five,
//! four, four with a gap or three, by the stochastic rules `a1`-`a4` - each
//! whorl rolled 137.5 degrees from the last, with a trunk-fill marker `J`
//! half way up the internode. A branch is drawn at birth as two tube
//! segments with seven needle nodes `K` placed by moves, alternately
//! side-up (`k` 0) and side-down (`k` 1). Then every segment re-lengthens
//! with age (`g1`, `g2`: linearly, with no cap), its angle from the trunk
//! sinks from 46 degrees toward 87 (`g3`), and branch and trunk widths
//! thicken by their own factors (`g4`, `g5`), so the cone, the up-angled
//! young top and the level old bottom all fall out of age alone.
//!
//! **Foliage.** Needle cards, all expressed at finalization. Each node is
//! three cards splayed round its branch, four once its card scale (grown
//! by `g6` to a cap) reaches 0.46; `T` is a branch tip's three, `B` the bud
//! form of the whorl born on the last step, `A` the leader's top tuft and
//! `J` three cards filling the trunk at every whorl. Every card carries a
//! roll after its turns, chosen with the builder's card simulator so that
//! no face lies level at any branch pitch: a level card mirrors a low sun
//! straight into the game's camera.
//!
//! **Slots.** 0 bark - a Bark texture in a dark brown under a darker
//! brown-grey base colour; 1 a Needle card of 24 needle pairs, a dark
//! blue-green whose shoot wears the needles' own base colour so no pale
//! midrib shows. The needle card carries its colour in the texture under a
//! white base colour, so the variants re-tint the textures themselves
//! (`tint_needle`, `tint_bark`): a base colour only multiplies, and can
//! darken a green needle but never turn it gold.
//!
//! Measured with the render tool: 9.86 m tall and 5.96 x 5.26 m across,
//! 3,788 triangles in 2 parts (the ABOP conifer it replaced stood 6.5 m
//! tall). The grammar's lengths are metres before the root transform's
//! uniform scale of 1.5, which the saved record carries and the entry
//! keeps. It ships at iterations 12, the record sanitiser's cap and the
//! tree's whorl count, which leaves no headroom for a seeded stand's age:
//! `seeded_defaults::room::build` adds the stand's `iterations_delta` (-1
//! to +1) and clamps the sum to the cap, so a stand that rolls +1 grows
//! this tree at 12 all the same.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_bark, tint_needle};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig,
    SovereignMaterialSettings, SovereignNeedleConfig, SovereignTextureConfig, TransformData,
};

/// The spruce's bark base colour (sRGB), which every variant keeps: the
/// variants recolour the bark through its texture.
const BARK_BASE: [f32; 3] = [0.3, 0.22, 0.16];

/// Recolour the needle card (slot 1): `deep` the needle bases and the shoot
/// they hide, `tip` the lit tips, both linear RGB, under the authored white
/// base colour. [`tint_needle`] leaves the shoot as authored; the spruce's
/// shoot wears the needles' base colour, so a variant that left it there
/// would draw a dark green midrib down a gold card.
fn needles(m: &mut HashMap<u16, SovereignMaterialSettings>, deep: [f32; 3], tip: [f32; 3]) {
    tint_needle(m, 1, [1.0, 1.0, 1.0], deep, tip);
    if let Some(SovereignMaterialSettings {
        texture: SovereignTextureConfig::Needle(needle),
        ..
    }) = m.get_mut(&1)
    {
        needle.color_shoot = Fp3(deep);
    }
}

/// Conifer re-skins (#910), re-authored for the spruce's textures (#1708):
/// one skeleton covers the cold biomes. Slot 0 is the Bark, slot 1 the
/// Needle card. Every texture colour here is linear RGB, as the textures
/// read them; the sRGB each was chosen in is in its comment.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "pine",
        label: "Pine (warm olive)",
        apply: |m| {
            // Warmer, yellower needles and the red-brown plated bark that
            // separates a pine ridge from a spruce stand at a distance.
            // Needles sRGB (0.12, 0.19, 0.07) to (0.25, 0.31, 0.12); bark
            // (0.50, 0.29, 0.18) over (0.17, 0.08, 0.05).
            needles(m, [0.0134, 0.0301, 0.006], [0.0509, 0.0783, 0.0134]);
            tint_bark(
                m,
                0,
                BARK_BASE,
                [0.214, 0.0684, 0.0272],
                [0.0245, 0.0072, 0.0039],
            );
        },
    },
    PlantVariant {
        name: "larch_gold",
        label: "Larch (autumn gold)",
        apply: |m| {
            // The deciduous conifer: gold needles before drop. Tundra and
            // high alpine read as a different world with this one swap.
            // Needles sRGB (0.52, 0.38, 0.08) to (0.80, 0.62, 0.18); a
            // grey-brown bark, (0.42, 0.36, 0.30) over (0.13, 0.11, 0.09).
            needles(m, [0.233, 0.1193, 0.0072], [0.6038, 0.3424, 0.0272]);
            tint_bark(
                m,
                0,
                BARK_BASE,
                [0.1473, 0.1065, 0.0732],
                [0.0153, 0.0116, 0.0085],
            );
        },
    },
    PlantVariant {
        name: "frosted",
        label: "Frosted spruce",
        apply: |m| {
            // Pale, desaturated and blue-shifted - snow-laden needles for
            // the coldest worlds, where full-chroma green reads wrong.
            // Needles sRGB (0.20, 0.29, 0.31) to (0.52, 0.62, 0.66); a cold
            // grey bark, (0.36, 0.33, 0.31) over (0.11, 0.10, 0.10).
            needles(m, [0.0331, 0.0684, 0.0783], [0.233, 0.3424, 0.3931]);
            tint_bark(
                m,
                0,
                BARK_BASE,
                [0.1065, 0.089, 0.0783],
                [0.0116, 0.01, 0.01],
            );
        },
    },
];

pub struct MonopodialTree;

impl CatalogueEntry for MonopodialTree {
    fn slug(&self) -> &'static str {
        "lsys_monopodial_tree"
    }
    fn name(&self) -> &'static str {
        "Spruce"
    }
    fn description(&self) -> &'static str {
        "Dark spruce - a conical tiered conifer, whorls of level needle-brush branches under a narrow spire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn variants(&self) -> &'static [PlantVariant] {
        VARIANTS
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator {
            // The saved record's root scale: the grammar's lengths are
            // metres before it.
            transform: TransformData {
                scale: Fp3([1.5, 1.5, 1.5]),
                ..Default::default()
            },
            ..Generator::from_kind(build_kind())
        }
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 - dark brown bark: the texture's own brown, sRGB (0.36, 0.27,
    // 0.20) over (0.12, 0.08, 0.06), darkened by a brown-grey base colour.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3(BARK_BASE),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.1065, 0.0593, 0.0331]),
                color_dark: Fp3([0.0134, 0.0072, 0.0049]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - the needle card: 24 pairs of long, fine needles splayed wide off
    // a thin shoot, dark blue-green, sRGB (0.06, 0.17, 0.15) at the base to
    // (0.13, 0.27, 0.25) at the tips (linear RGB here). The shoot wears the
    // base colour, which hides the midrib.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Needle(SovereignNeedleConfig {
                pair_count: 24,
                color_base: Fp3([0.0049, 0.0245, 0.0196]),
                color_tip: Fp3([0.0153, 0.0593, 0.0509]),
                color_shoot: Fp3([0.0049, 0.0245, 0.0196]),
                needle_angle: Fp64(56.0),
                needle_length: Fp64(0.34),
                needle_width: Fp64(0.022),
                // Needles barely shorten toward the tip, so the card reads
                // as a full brush rather than a feather.
                length_taper: Fp64(0.25),
                shoot_length: Fp64(0.8),
                shoot_width: Fp64(0.012),
                normal_strength: Fp(0.5),
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
        angle: Fp(45.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.07),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(1.0),
        mesh_resolution: 5,
    }
}

/// The spruce's growth rules: the bare trunk foot in the axiom, the four
/// whorl patterns `a1`-`a4`, the branch `b1` as drawn at birth, and the
/// growth laws `g1`-`g8` that age everything already placed.
const SOURCE_CODE: &str = "\
    #define l0 0.28\n\
    #define dl 0.118\n\
    #define a0 46\n\
    #define da 4.6\n\
    #define am 87\n\
    #define c0 0.3\n\
    #define dc 0.034\n\
    #define cm 0.62\n\
    #define vb 1.15\n\
    #define vt 1.155\n\
    #define ih 0.44\n\
    #define wt 0.034\n\
    #define bw 0.018\n\
    omega: !(wt,-1)F(0.55)/(23)A\n\
    a1: 0.4 : A -> !(wt,-1)[f(ih*0.5)J(0)]F(ih)[&(a0,0,0)B(1)]/(71)[&(a0+4,0,4)B(0.82)]/(73)[&(a0-3,0,-3)B(1.12)]/(70)[&(a0+2,0,2)B(0.9)]/(74)[&(a0-2,0,-2)B(1.05)]/(137.5)A\n\
    a2: 0.3 : A -> !(wt,-1)[f(ih*0.5)J(0)]F(ih)[&(a0+1,0,1)B(1.1)]/(88)[&(a0-3,0,-3)B(0.78)]/(92)[&(a0+3,0,3)B(1)]/(89)[&(a0,0,0)B(0.92)]/(137.5)A\n\
    a3: 0.18 : A -> !(wt,-1)[f(ih*0.5)J(0)]F(ih)[&(a0-2,0,-2)B(0.85)]/(70)[&(a0+2,0,2)B(1.15)]/(144)[&(a0+4,0,4)B(0.76)]/(72)[&(a0,0,0)B(1.02)]/(137.5)A\n\
    a4: 0.12 : A -> !(wt,-1)[f(ih*0.5)J(0)]F(ih)[&(a0+2,0,2)B(1.08)]/(95)[&(a0-3,0,-3)B(0.8)]/(150)[&(a0,0,0)B(1.12)]/(137.5)A\n\
    b1: B(m) -> $!(bw,1)[f(l0*m*0.05,0.05,0,m)K(0,c0,0)][f(l0*m*0.19,0.19,0,m)K(1,c0,23)][f(l0*m*0.33,0.33,0,m)K(0,c0,-17)][f(l0*m*0.47,0.47,0,m)K(1,c0,31)]F(l0*m*0.55,0.55,0,m)!(bw*0.45,1)[f(l0*m*0.06,0.06,0,m)K(0,c0,-9)][f(l0*m*0.2,0.2,0,m)K(1,c0,14)][f(l0*m*0.33,0.33,0,m)K(0,c0,-26)]F(l0*m*0.45,0.45,0,m)T(c0)\n\
    g1: F(x,r,g,m) -> F(r*m*(l0+dl*(g+1)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*(l0+dl*(g+1)),r,g+1,m)\n\
    g3: &(a,g,j) -> &(min(am,a0+j+da*(g+1)),g+1,j)\n\
    g4: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g5: !(w,t) : t < 0 -> !(w*vt,t)\n\
    g6: K(k,c,j) -> K(k,min(cm,c+dc),j)\n\
    g7: T(c) -> T(min(cm,c+dc))\n\
    g8: J(n) -> J(n+1)";

/// The spruce's finalization: every marker becomes its needle cards.
const FINALIZATION_CODE: &str = "\
    K(k,c,j) : k < 0.5 & c < 0.46 -> [,(1)/(j+70)&(50)~(0,c)][,(1)/(j-70)&(50)~(0,c)][,(1)/(j*0.4)&(30)/(52)~(0,c*0.8)]\n\
    K(k,c,j) : k >= 0.5 & c < 0.46 -> [,(1)/(j+100)&(30)/(-15)~(0,c)][,(1)/(j-100)&(30)/(15)~(0,c)][,(1)/(j*0.4+180)&(12)/(55)~(0,c*0.65)]\n\
    K(k,c,j) : k < 0.5 & c >= 0.46 -> [,(1)/(j+70)&(50)~(0,c)][,(1)/(j-70)&(50)~(0,c)][,(1)/(j*0.4)&(30)/(52)~(0,c*0.8)][,(1)/(j+35)&(25)/(45)~(0,c*0.9)]\n\
    K(k,c,j) : k >= 0.5 & c >= 0.46 -> [,(1)/(j+100)&(30)/(-15)~(0,c)][,(1)/(j-100)&(30)/(15)~(0,c)][,(1)/(j*0.4+180)&(12)/(55)~(0,c*0.65)][,(1)/(j-35)&(25)/(-45)~(0,c*0.9)]\n\
    T(c) : * -> [,(1)/(50)~(0,c*0.9)][,(1)+(28)/(52)~(0,c*0.75)][,(1)+(-28)/(-52)~(0,c*0.75)]\n\
    B(m) : * -> [,(1)&(10)/(45)~(0,0.3)][,(1)^(10)/(-30)~(0,0.24)]\n\
    A : * -> !(wt)[f(0.05)[,(1)&(42)~(0,0.4)][,(1)/(90)&(44)~(0,0.4)][,(1)/(180)&(40)~(0,0.4)][,(1)/(270)&(43)~(0,0.4)]][f(0.19)[,(1)/(45)&(34)~(0,0.36)][,(1)/(165)&(32)~(0,0.36)][,(1)/(285)&(35)~(0,0.36)]][f(0.33)[,(1)/(100)&(24)~(0,0.32)][,(1)/(220)&(22)~(0,0.32)][,(1)/(340)&(25)~(0,0.32)]]F(0.44)[,(1)~(0,0.3)][,(1)/(90)~(0,0.3)][,(1)&(12)~(0,0.24)][,(1)/(180)&(12)~(0,0.24)]\n\
    J(n) : * -> [,(1)&(48)/(36)~(0,0.44)][,(1)/(120)&(45)/(-36)~(0,0.44)][,(1)/(240)&(50)/(36)~(0,0.44)]";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::plants::variant::apply_named;
    use crate::pds::sanitize_generator;

    fn authored_materials() -> HashMap<u16, SovereignMaterialSettings> {
        let GeneratorKind::LSystem { materials, .. } = build_kind() else {
            panic!("the spruce is an L-system");
        };
        materials
    }

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn spruce_is_kept_as_built() {
        let built = MonopodialTree.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the spruce");
    }

    /// A texture's colours taken out of it: its colour fields in order, and
    /// the texture with each of them blacked out, so that everything else in
    /// it can be compared whole. The spruce wears only these two kinds.
    fn split_colours(texture: &SovereignTextureConfig) -> (Vec<Fp3>, SovereignTextureConfig) {
        let mut rest = texture.clone();
        let fields = match &mut rest {
            SovereignTextureConfig::Bark(bark) => vec![&mut bark.color_light, &mut bark.color_dark],
            SovereignTextureConfig::Needle(needle) => vec![
                &mut needle.color_base,
                &mut needle.color_tip,
                &mut needle.color_shoot,
            ],
            other => panic!("the spruce wears a Bark and a Needle texture, not {other:?}"),
        };
        let colours = fields
            .into_iter()
            .map(|colour| std::mem::replace(colour, Fp3([0.0; 3])))
            .collect();
        (colours, rest)
    }

    /// Each re-skin recolours the TEXTURE of the slots it names, and only
    /// those: a named slot's texture colours change, an unnamed slot's stay
    /// as authored, and no variant touches a texture's other fields. The
    /// needle card carries its colour in the texture under a white base
    /// colour, so a variant that reached `base_color` alone - as the #910
    /// variants did, written for the ABOP conifer's tinted cards - could only
    /// multiply the texture's own colours: darken a green needle, never turn
    /// it gold.
    #[test]
    fn every_variant_recolours_the_textures_it_names() {
        let expected: &[(&str, &[u16])] = &[
            ("pine", &[0, 1]),
            ("larch_gold", &[0, 1]),
            ("frosted", &[0, 1]),
        ];
        assert_eq!(
            VARIANTS.iter().map(|v| v.name).collect::<Vec<_>>(),
            expected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            "a spruce variant without a line here is not checked"
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
            // The base colours stay as authored: white over the needles,
            // the brown-grey over the bark.
            for slot in [0u16, 1] {
                assert_eq!(
                    m[&slot].base_color, authored[&slot].base_color,
                    "{}: slot {slot}'s base colour changed",
                    variant.name
                );
            }
        }
    }

    /// A name the spruce does not know leaves its authored materials alone,
    /// the fallback the seeded pools rely on.
    #[test]
    fn an_unknown_variant_changes_nothing() {
        let authored = authored_materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "no_such_variant", &mut m);
        assert_eq!(m, authored);
    }
}
