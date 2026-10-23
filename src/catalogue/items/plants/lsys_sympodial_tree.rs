//! Sympodial tree - a London plane (Platanus x hispanica), the street and
//! park plane in high summer: the owner's choice for this entry (#1764),
//! which keeps its slug because the seeded worlds' broadleaf stands and
//! Berlin's planes, horse chestnuts, beeches and elms are drawn as it. It
//! was built by `docs/agent/examples/trees/plane.py`, whose docstring lists
//! every variant tried and why it was rejected; change the tree there and
//! judge it in pictures, then copy the generator here. It replaced a fractal
//! grammar that drew the same small leaf ball on a thin pole at every age.
//!
//! **Growth.** The [oak](super::lsys_oak)'s grammar grown into a plane: a
//! dichotomous sympodial crown grown by age, every internode by one law
//! `r*m*min(lm, l0 + dl*age)` (branch internodes a further `lb` shorter
//! than the trunk's), the last fork drawn by its cards alone. Against the
//! oak: a clean bole of 3.8 m, five heavy limbs launched from the foot and
//! walked up it with `f` so the trunk is one tube, leaving it at 34-44
//! degrees from upright instead of the oak's low crooked spread, each fork's
//! two shoots rolled 65 and 120 degrees off straight up so a limb spreads
//! sideways into a high, broad, flat-bottomed dome, a weaker leader (vigour
//! 0.85) so the top rounds off, stouter tubes, 9-card tip masses and 7-card
//! side sprays at the forks, and no burrs. The iteration count is the
//! tree's age: 5 is a young tree with a round head, 11 a broader veteran.
//!
//! **Slots.** 0 the camouflage bark - a `Lichen` texture used as bark, as
//! the birch's is: an old olive-grey face flaked away in irregular cream and
//! pale khaki-green patches with khaki margins, stretched by `;(0.8)`; 1 a
//! `Twig` card of five big alternate leaves with three to five points
//! (`lobe_count` 1.5 on the Leaf texture's lobing along the midrib), in
//! mid to dark green. The re-skins tint both with [`tint_twig`] and
//! [`tint_lichen`]: `tint_leaf` and `tint_bark` would change only the base
//! colour of these textures.
//!
//! Grammar units are metres before the root transform's 1.02 scale.
//! Measured with the render tool: 14.7 m tall and 16.7 x 14.9 m across,
//! 7,648 triangles in 2 parts at iterations 10 and seed 6 (the seed is part
//! of the budget: another seed draws another tree, up to a fifth dearer);
//! the trunk's foot is sunk 0.3 m below the origin so it stays buried on a
//! slope.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_lichen, tint_twig};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignLeafConfig,
    SovereignLichenConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
    TransformData,
};

/// The plane's re-skins (#910, #1764), the names the seeded pools use. Slot
/// 0 bark, slot 1 leaf.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "autumn",
        label: "Autumn (russet)",
        apply: |m| {
            // A plane turns russet and yellow-brown, not red.
            tint_twig(
                m,
                1,
                [1.0, 1.0, 1.0],
                [0.233, 0.1329, 0.0196],
                [0.1706, 0.0637, 0.0116],
            );
        },
    },
    PlantVariant {
        name: "blossom_pale",
        label: "Pale spring flush",
        apply: |m| {
            // Fresh yellow-green growth over paler, newly flaked bark - the
            // same crown in early spring, before the leaves darken.
            tint_twig(
                m,
                1,
                [1.0, 1.0, 1.0],
                [0.233, 0.3185, 0.055],
                [0.3185, 0.3931, 0.0732],
            );
            tint_lichen(
                m,
                0,
                [1.0, 1.0, 1.0],
                [0.2633, 0.2429, 0.1549],
                [[0.7484, 0.6921, 0.42], [0.448, 0.477, 0.214]],
                [0.3424, 0.2957, 0.1626],
            );
        },
    },
];

pub struct SympodialTree;

impl CatalogueEntry for SympodialTree {
    fn slug(&self) -> &'static str {
        "lsys_sympodial_tree"
    }
    fn name(&self) -> &'static str {
        "London Plane"
    }
    fn description(&self) -> &'static str {
        "Tall London plane - mottled flaking bark, ascending limbs, a broad high dome of big leaves."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn variants(&self) -> &'static [PlantVariant] {
        VARIANTS
    }
    fn build(&self, _local_did: &str) -> Generator {
        let mut prop_mappings = HashMap::new();
        prop_mappings.insert(0, PropMeshType::Twig);

        Generator {
            transform: TransformData {
                scale: Fp3([1.02, 1.02, 1.02]),
                ..Default::default()
            },
            ..Generator::from_kind(GeneratorKind::LSystem {
                source_code: SOURCE_CODE.to_string(),
                finalization_code: FINALIZATION_CODE.to_string(),
                iterations: 10,
                seed: 6,
                angle: Fp(30.0),
                step: Fp(1.0),
                width: Fp(0.1),
                // Lower than the oak's: a plane's limbs are stiff.
                elasticity: Fp(0.03),
                tropism: Some(Fp3([0.0, -1.0, 0.0])),
                materials: materials(),
                prop_mappings,
                prop_scale: Fp(1.0),
                mesh_resolution: 5,
            })
        }
    }
}

/// The plane's growth rules: the bole, its five limbs and leader `A` in the
/// axiom, then the apex's fork (`f`), elbow (`e`) and stall (`st`) rules by
/// fork depth, and the growth laws `g1`-`g4` that lengthen every internode
/// and thicken every tube with age.
const SOURCE_CODE: &str = "\
    #define l0 0.9\n\
    #define dl 0.25\n\
    #define lm 3.4\n\
    #define bw 0.28\n\
    #define vb 1.13\n\
    #define vt 1.12\n\
    #define wt 0.33\n\
    #define G 6\n\
    #define lb 0.82\n\
    omega: ;(0.8)!(wt*1.35,-1)f(-0.3)[f(1.1765*l0,1.1765,0,1)/(20)&(40)!(bw*1*1.1/vb,1)f(0)A(1,1,0)][f(1.1765*l0,1.1765,0,1)/(165)&(38)!(bw*0.95*1.1/vb,1)f(0)A(0.95,-1,0)][f(1.3235*l0,1.3235,0,1)/(95)&(44)!(bw*0.95*1.1/vb,1)f(0)A(0.95,1,0)][f(1.3235*l0,1.3235,0,1)/(250)&(42)!(bw*1*1.1/vb,1)f(0)A(1,-1,0)][f(1.5*l0,1.5,0,1)/(325)&(34)!(bw*0.9*1.1/vb,1)f(0)A(0.9,1,1)]!(wt*1.35,-1)F(0.1324*l0,0.1324,0,1)!(wt*1.15,-1)F(0.1029*l0,0.1029,0,1)!(wt*1.04,-1)F(0.1765*l0,0.1765,0,1)!(wt*1,-1)F(0.7647*l0,0.7647,0,1)!(wt*0.96,-1)F(0.1471*l0,0.1471,0,1)!(wt*0.92,-1)F(0.1765*l0,0.1765,0,1)!(wt*0.88,-1)F(0.2059*l0,0.2059,0,1)&(3)A(0.85,1,0)\n\
    f1: 0.62 : A(v,s,d) : d < 3 -> !(bw*v,1)F(l0*v*lb*0.5,0.5,0,v*lb)$/(-s*70)&(12)!(bw*v*0.93,1)F(l0*v*lb*0.5,0.5,0,v*lb)$[f(-l0*v*lb*0.45,-0.45,0,v*lb)/(s*140)&(55)K(v)][/(s*65)&(32)!(bw*v*0.7626/vb,1)f(0)A(v*0.82,s,d+1)]F(0.01)/(-s*120)&(32)A(v*0.78,-s,d+1)\n\
    e1: 0.24 : A(v,s,d) : d < 3 -> !(bw*v,1)F(l0*v*lb*0.5,0.5,0,v*lb)$/(-s*70)&(12)!(bw*v*0.93,1)F(l0*v*lb*0.5,0.5,0,v*lb)$/(s*35)&(20)A(v*0.9,-s,d+0.5)\n\
    f2: 0.62 : A(v,s,d) : d >= 3 & d < 5 -> !(bw*v,1)F(l0*v*lb,1,0,v*lb)$[f(-l0*v*lb*0.45,-0.45,0,v*lb)/(s*140)&(55)K(v)][/(s*65)&(32)!(bw*v*0.82/vb,1)f(0)A(v*0.82,s,d+1)]/(-s*120)&(32)A(v*0.78,-s,d+1)\n\
    e2: 0.24 : A(v,s,d) : d >= 3 & d < 5 -> !(bw*v,1)F(l0*v*lb,1,0,v*lb)$/(s*35)&(20)A(v*0.9,-s,d+0.5)\n\
    f3: 0.62 : A(v,s,d) : d >= 5 & d < 6 -> f(l0*v*lb,1,0,v*lb)$[/(s*65)&(32)A(v*0.82,s,d+1)]/(-s*120)&(32)A(v*0.78,-s,d+1)\n\
    e3: 0.24 : A(v,s,d) : d >= 5 & d < 6 -> f(l0*v*lb,1,0,v*lb)$/(s*35)&(20)A(v*0.9,-s,d+0.5)\n\
    st: 0.14 : A(v,s,d) : d < G -> A(v,s,d)\n\
    g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g3: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g4: !(w,t) : t < 0 -> !(w*vt,t)";

/// The plane's finalization: a 9-card mass of 2.1 m cards at every tip and
/// a 7-card spray at every fork.
const FINALIZATION_CODE: &str = "\
    A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,2.1)][,(1)/(180)&(30)/(-90)~(0,1.995)][,(1)/(60)&(45)/(40)~(0,2.1)][,(1)/(120)&(50)/(-40)~(0,1.995)][,(1)/(240)&(50)/(40)~(0,1.995)][,(1)/(300)&(45)/(-40)~(0,2.1)][,(1)/(45)&(95)/(-90)~(0,1.89)][,(1)/(315)&(95)/(-90)~(0,1.89)][,(1)f(-0.8)/(30)&(110)/(-80)~(0,1.89)]\n\
    K(v) : * -> $f(0.4)[,(1)/(0)&(25)/(-90)~(0,1.785)][,(1)/(180)&(30)/(-90)~(0,1.6957)][,(1)/(60)&(45)/(40)~(0,1.785)][,(1)/(120)&(50)/(-40)~(0,1.6957)][,(1)/(240)&(50)/(40)~(0,1.6957)][,(1)/(300)&(45)/(-40)~(0,1.785)][,(1)/(45)&(95)/(-90)~(0,1.6065)]";

/// The plane's camouflage bark and its leaf card.
fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - camouflage bark: the old olive-grey face flaked away in cream and
    // pale khaki-green patches with khaki margins.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Lichen(SovereignLichenConfig {
                seed: 11,
                patch_scale: Fp64(1.3),
                patch_octaves: 3,
                coverage: Fp64(0.42),
                rim_width: Fp64(0.12),
                species_scale: Fp64(2.0),
                color_rock: Fp3([0.1473, 0.1329, 0.0783]),
                color_lichen_a: Fp3([0.6038, 0.5382, 0.2846]),
                color_lichen_b: Fp3([0.2957, 0.3303, 0.1128]),
                color_rim: Fp3([0.233, 0.196, 0.0946]),
                grain_scale: Fp64(30.0),
                grain_strength: Fp64(0.12),
                relief: Fp64(0.25),
                normal_strength: Fp(1.0),
            }),
            ..Default::default()
        },
    );
    // 1 - five big alternate leaves with three to five points, mid to dark
    // green.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
                leaf: SovereignLeafConfig {
                    color_base: Fp3([0.0039, 0.055, 0.0049]),
                    color_edge: Fp3([0.0072, 0.0732, 0.006]),
                    serration_strength: Fp64(0.12),
                    vein_angle: Fp64(2.0),
                    micro_detail: Fp64(0.2),
                    normal_strength: Fp(0.3),
                    lobe_count: Fp64(1.5),
                    lobe_depth: Fp64(0.4),
                    lobe_sharpness: Fp64(1.0),
                    petiole_length: Fp64(0.06),
                    petiole_width: Fp64(0.02),
                    midrib_width: Fp64(0.05),
                    vein_count: Fp64(7.0),
                    venule_strength: Fp64(0.1),
                    ..Default::default()
                },
                stem_color: Fp3([0.1065, 0.089, 0.0331]),
                stem_half_width: Fp64(0.008),
                leaf_pairs: 5,
                leaf_angle: Fp64(1.9),
                leaf_scale: Fp64(0.44),
                stem_curve: Fp64(0.01),
                sympodial: true,
            }),
            ..Default::default()
        },
    );
    materials
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::plants::variant::{apply_named, split_colours};
    use crate::pds::sanitize_generator;

    /// The builder's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn plane_is_kept_as_built() {
        let built = SympodialTree.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the plane");
    }

    /// Each re-skin recolours the TEXTURE of the slots it names, and only
    /// those: both slots carry their colour in the texture under a white
    /// base colour, so a variant that reached `base_color` alone - as the
    /// old tree's `tint_leaf` and `tint_bark` would on a Twig and a Lichen
    /// slot - could only darken the authored green, never turn it russet.
    #[test]
    fn every_variant_recolours_the_textures_it_names() {
        let expected: &[(&str, &[u16])] = &[("autumn", &[1]), ("blossom_pale", &[0, 1])];
        assert_eq!(
            VARIANTS.iter().map(|v| v.name).collect::<Vec<_>>(),
            expected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            "a plane variant without a line here is not checked"
        );
        let authored = materials();
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

    /// A name the plane does not know leaves its authored materials alone,
    /// the fallback the seeded pools rely on.
    #[test]
    fn an_unknown_variant_changes_nothing() {
        let authored = materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "no_such_variant", &mut m);
        assert_eq!(m, authored);
    }
}
