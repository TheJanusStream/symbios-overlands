//! Flowering tree - a Japanese flowering cherry in full blossom (Prunus
//! serrulata, a 'Kanzan'-like park cherry in mid April, the leaves just
//! breaking): the owner's choice for this entry (#1763), which keeps its
//! slug because the seeded worlds' blossom stands and Berlin's cherries,
//! apples, pears, hawthorns and rowans are drawn as it. It was built by
//! `docs/agent/examples/trees/cherry.py`, whose docstring lists every
//! variant tried and why it was rejected; change the tree there and judge
//! it in pictures, then copy the generator here. It replaced a fractal
//! grammar that drew a small pink-and-green ball on a thin pole at every
//! age.
//!
//! **Growth.** The [apple](super::lsys_apple)'s grammar (itself the oak's)
//! reshaped into a vase: a dichotomous sympodial crown grown by age, every
//! internode by one law `r*m*min(lm, l0 + dl*age)`, the last fork drawn by
//! its cards alone. Four scaffolds leave a 1.5 m bole at 44-54 degrees from
//! upright, launched from the foot and walked up it with `f` so the trunk
//! is one tube, and there is no leader, so the vase stays open and the
//! crown is wider than tall; an elasticity of 0.07 arches the outer shoots
//! out and down. Every apex and spur carries the plant's age (`A(v,s,d,n)`,
//! `K(v,n)`, stepped on by `st`, `ag` and `kg` even after the crown stops
//! forking), and finalization scales each card by `min(1, 0.4 + 0.075 n)`,
//! so a sapling wears small blossom rather than the adult's. The iteration
//! count is the tree's age: 4 is a sapling, 12 a veteran.
//!
//! **Blossom along the limbs.** Every internode that ends in a fork, from
//! the first fork on, carries four 2-card spurs hung back along it by an
//! `f` that grows with it (so no bracket breaks a tube), and every tip a
//! 5-card mass of blossom; half the tips put two small bronze-green leaf
//! sprays in the mass's two foremost places and carry the blossom on round
//! them, seven cards in all - the leaves just breaking.
//!
//! **Slots.** 0 the bark - a `Lichen` texture used as bark, as the birch's
//! is: a dark glossy red-brown face whose pale tan colonies are the
//! lenticels, squashed into horizontal dashes by `;(4)` on the trunk and
//! `;(4/v)` on a branch of vigour `v` (U wraps once round every tube
//! whatever its girth, so one `;` would stretch them along the twigs); 2
//! a `Twig` card of four pairs of small toothed bronze-green leaves; 3 the
//! blossom - a `Flower` atlas of 4 rows by 3 columns, twelve five-petalled
//! notched pink flowers to a card (an L-system card draws its whole atlas),
//! petals that overlap so the crown holds at a distance under the alpha
//! mask. Slot 1 is empty, so the numbering is the playbook's (0 bark, 2
//! leaf, 3 flower). The re-skins: `white` re-tints the petals
//! ([`tint_flower`]) for the pears and hawthorns, and `summer` turns the
//! blossom cards into leaf sprays.
//!
//! Measured with the render tool: 5.93 m tall and 7.3 x 6.2 m across,
//! 3,308 triangles in 3 parts at iterations 10 and seed 1 (950 of them
//! wood); the trunk's foot is sunk 0.1 m below the origin so it stays
//! buried on a slope.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_flower, tint_twig};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignFlowerConfig,
    SovereignLeafConfig, SovereignLichenConfig, SovereignMaterialSettings, SovereignPetalConfig,
    SovereignTextureConfig, SovereignTwigConfig,
};

/// The leaves' colours just breaking (linear RGB): bronze-green, midrib and
/// edge.
const BRONZE: [[f32; 3]; 2] = [[0.233, 0.214, 0.0272], [0.3424, 0.1626, 0.0331]];

/// The leaves' colours in summer (linear RGB), midrib and edge.
const SUMMER: [[f32; 3]; 2] = [[0.0331, 0.1065, 0.0134], [0.055, 0.1473, 0.0174]];

/// The cherry's re-skins (#1763): white blossom for the pears and
/// hawthorns, and the same tree in summer leaf.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "white",
        label: "White blossom (pear, hawthorn)",
        apply: |m| {
            tint_flower(
                m,
                3,
                [1.0, 1.0, 1.0],
                [
                    [0.9331, 0.9114, 0.8689],
                    [0.9774, 0.9551, 0.9331],
                    [0.8689, 0.8276, 0.7106],
                ],
                [0.6038, 0.3424, 0.2633],
            );
        },
    },
    PlantVariant {
        name: "summer",
        label: "Summer green",
        apply: summer,
    },
];

/// The cherry in summer: its leaves green, and every blossom card a full
/// leaf spray of eight pairs - a tint of the blossom would leave green
/// flowers. Fewer, smaller leaves (the apple's seven pairs at 0.32) left
/// the crown thin and see-through at 30 m.
fn summer(materials: &mut HashMap<u16, SovereignMaterialSettings>) {
    tint_twig(materials, 2, [1.0, 1.0, 1.0], SUMMER[0], SUMMER[1]);
    materials.insert(3, leaf_card(SUMMER, 8, 0.5));
}

pub struct FloweringTree;

impl CatalogueEntry for FloweringTree {
    fn slug(&self) -> &'static str {
        "lsys_flowering_tree"
    }
    fn name(&self) -> &'static str {
        "Japanese Cherry"
    }
    fn description(&self) -> &'static str {
        "Japanese cherry in full blossom - a spreading vase of dark limbs clouded in pink."
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

        Generator::from_kind(GeneratorKind::LSystem {
            source_code: SOURCE_CODE.to_string(),
            finalization_code: FINALIZATION_CODE.to_string(),
            iterations: 10,
            seed: 1,
            angle: Fp(30.0),
            step: Fp(1.0),
            width: Fp(0.1),
            // Higher than the apple's: the outer shoots arch out and down.
            elasticity: Fp(0.07),
            tropism: Some(Fp3([0.0, -1.0, 0.0])),
            materials: materials(),
            prop_mappings,
            prop_scale: Fp(1.0),
            mesh_resolution: 5,
        })
    }
}

/// The cherry's growth rules: the bole, its four scaffolds in the axiom,
/// then the apex's fork (`f`), elbow (`e`) and stall (`st`) rules by fork
/// depth, the age steps `ag` and `kg`, and the growth laws `g1`-`g4` that
/// lengthen every internode and thicken every tube with age.
const SOURCE_CODE: &str = "\
    #define l0 0.4\n\
    #define dl 0.1\n\
    #define lm 1.3\n\
    #define bw 0.075\n\
    #define vb 1.12\n\
    #define vt 1.11\n\
    #define wt 0.11\n\
    #define G 5\n\
    #define ka 0.4\n\
    #define kb 0.075\n\
    omega: ;(4)!(wt*1.4,-1)f(-0.1)[f(0.1692*l0,0.1692,0,1)&(4)f(1.0615*l0,1.0615,0,1)/(20)&(46)!(bw*1/vb,1)f(0)A(1,1,0,0)][f(0.1692*l0,0.1692,0,1)&(4)f(1.0615*l0,1.0615,0,1)/(110)&(54)!(bw*0.95/vb,1)f(0)A(0.95,-1,0,0)][f(0.1692*l0,0.1692,0,1)&(4)f(1.0615*l0,1.0615,0,1)/(205)&(44)!(bw*1/vb,1)f(0)A(1,1,0,0)][f(0.1692*l0,0.1692,0,1)&(4)f(1.0615*l0,1.0615,0,1)/(295)&(52)!(bw*0.95/vb,1)f(0)A(0.95,-1,0,0)]!(wt*1.4,-1)F(0.1692*l0,0.1692,0,1)&(4)!(wt*1.1,-1)F(0.3692*l0,0.3692,0,1)!(wt*1,-1)F(0.6923*l0,0.6923,0,1)\n\
    f1: 0.62 : A(v,s,d,n) : d < 1 -> ;(4/v)!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(16)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$[/(s*35)&(32)!(bw*v*0.7812/vb,1)f(0)A(v*0.84,s,d+1,n+1)]F(0.01)/(-s*105)&(38)A(v*0.8,-s,d+1,n+1)\n\
    e1: 0.24 : A(v,s,d,n) : d < 1 -> ;(4/v)!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(16)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$/(s*40)&(22)A(v*0.9,-s,d+0.5,n+1)\n\
    f2: 0.62 : A(v,s,d,n) : d >= 1 & d < 2 -> ;(4/v)!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(16)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$[f(-l0*v*0.2,-0.2,0,v)/(s*40)&(60)K(v,n+1)][f(-l0*v*0.45,-0.45,0,v)/(s*177.5)&(55)K(v,n+1)][f(-l0*v*0.7,-0.7,0,v)/(s*315)&(60)K(v,n+1)][f(-l0*v*0.95,-0.95,0,v)/(s*92.5)&(58)K(v,n+1)][/(s*35)&(32)!(bw*v*0.7812/vb,1)f(0)A(v*0.84,s,d+1,n+1)]F(0.01)/(-s*105)&(38)A(v*0.8,-s,d+1,n+1)\n\
    e2: 0.24 : A(v,s,d,n) : d >= 1 & d < 2 -> ;(4/v)!(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(16)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$/(s*40)&(22)A(v*0.9,-s,d+0.5,n+1)\n\
    f3: 0.62 : A(v,s,d,n) : d >= 2 & d < 4 -> ;(4/v)!(bw*v,1)F(l0*v,1,0,v)$[f(-l0*v*0.2,-0.2,0,v)/(s*40)&(60)K(v,n+1)][f(-l0*v*0.45,-0.45,0,v)/(s*177.5)&(55)K(v,n+1)][f(-l0*v*0.7,-0.7,0,v)/(s*315)&(60)K(v,n+1)][f(-l0*v*0.95,-0.95,0,v)/(s*92.5)&(58)K(v,n+1)][/(s*35)&(32)!(bw*v*0.84/vb,1)f(0)A(v*0.84,s,d+1,n+1)]/(-s*105)&(38)A(v*0.8,-s,d+1,n+1)\n\
    e3: 0.24 : A(v,s,d,n) : d >= 2 & d < 4 -> ;(4/v)!(bw*v,1)F(l0*v,1,0,v)$/(s*40)&(22)A(v*0.9,-s,d+0.5,n+1)\n\
    f4: 0.62 : A(v,s,d,n) : d >= 4 & d < 5 -> f(l0*v,1,0,v)$[f(-l0*v*0.2,-0.2,0,v)/(s*40)&(60)K(v,n+1)][f(-l0*v*0.45,-0.45,0,v)/(s*177.5)&(55)K(v,n+1)][f(-l0*v*0.7,-0.7,0,v)/(s*315)&(60)K(v,n+1)][f(-l0*v*0.95,-0.95,0,v)/(s*92.5)&(58)K(v,n+1)][/(s*35)&(32)A(v*0.84,s,d+1,n+1)]/(-s*105)&(38)A(v*0.8,-s,d+1,n+1)\n\
    e4: 0.24 : A(v,s,d,n) : d >= 4 & d < 5 -> f(l0*v,1,0,v)$/(s*40)&(22)A(v*0.9,-s,d+0.5,n+1)\n\
    st: 0.14 : A(v,s,d,n) : d < G -> A(v,s,d,n+1)\n\
    ag: A(v,s,d,n) : d >= G -> A(v,s,d,n+1)\n\
    kg: K(v,n) -> K(v,n+1)\n\
    g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g3: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g4: !(w,t) : t < 0 -> !(w*vt,t)";

/// The cherry's finalization: a 5-card blossom mass at every tip, half of
/// them with two leaf sprays in their foremost places and the blossom
/// carried on round them (7 cards), and a 2-card spur at every `K`, each
/// card scaled by the plant's age.
const FINALIZATION_CODE: &str = "\
    0.5 : A(v,s,d,n) : * -> $[,(3)/(0)&(25)/(-90)~(0,0.6*min(1,ka+kb*n))][,(3)/(180)&(30)/(-90)~(0,0.57*min(1,ka+kb*n))][,(3)/(60)&(45)/(40)~(0,0.6*min(1,ka+kb*n))][,(3)/(120)&(50)/(-40)~(0,0.57*min(1,ka+kb*n))][,(3)/(240)&(50)/(40)~(0,0.57*min(1,ka+kb*n))]\n\
    0.5 : A(v,s,d,n) : * -> $[,(2)/(0)&(25)/(-90)~(0,0.5*min(1,ka+kb*n))][,(2)/(180)&(30)/(-90)~(0,0.475*min(1,ka+kb*n))]$[,(3)/(60)&(45)/(40)~(0,0.6*min(1,ka+kb*n))][,(3)/(120)&(50)/(-40)~(0,0.57*min(1,ka+kb*n))][,(3)/(240)&(50)/(40)~(0,0.57*min(1,ka+kb*n))][,(3)/(300)&(45)/(-40)~(0,0.6*min(1,ka+kb*n))][,(3)/(45)&(95)/(-90)~(0,0.54*min(1,ka+kb*n))]\n\
    K(v,n) : * -> $f(0.1)[,(3)/(0)&(25)/(-90)~(0,0.45*min(1,ka+kb*n))][,(3)/(180)&(30)/(-90)~(0,0.4275*min(1,ka+kb*n))]";

/// A `Twig` card of `pairs` pairs of small toothed leaves, `scale` the card
/// across, the leaf running from `colours[0]` at the midrib to `colours[1]`
/// at the edge (linear RGB).
fn leaf_card(colours: [[f32; 3]; 2], pairs: u32, scale: f64) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(1.0),
        texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
            leaf: SovereignLeafConfig {
                color_base: Fp3(colours[0]),
                color_edge: Fp3(colours[1]),
                serration_strength: Fp64(0.14),
                vein_angle: Fp64(2.4),
                micro_detail: Fp64(0.2),
                normal_strength: Fp(0.4),
                lobe_count: Fp64(0.0),
                lobe_depth: Fp64(0.0),
                lobe_sharpness: Fp64(1.0),
                petiole_length: Fp64(0.08),
                petiole_width: Fp64(0.018),
                midrib_width: Fp64(0.07),
                vein_count: Fp64(7.0),
                venule_strength: Fp64(0.15),
                ..Default::default()
            },
            stem_color: Fp3([0.1005, 0.0397, 0.022]),
            stem_half_width: Fp64(0.012),
            leaf_pairs: pairs,
            leaf_angle: Fp64(1.6),
            leaf_scale: Fp64(scale),
            stem_curve: Fp64(0.01),
            sympodial: true,
        }),
        ..Default::default()
    }
}

/// The cherry's bark, its breaking leaves and its blossom.
fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - a dark glossy red-brown face, its pale tan colonies the lenticels.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.6),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Lichen(SovereignLichenConfig {
                patch_scale: Fp64(1.75),
                patch_octaves: 3,
                coverage: Fp64(0.25),
                rim_width: Fp64(0.1),
                species_scale: Fp64(1.5),
                color_rock: Fp3([0.055, 0.0116, 0.01]),
                color_lichen_a: Fp3([0.2957, 0.196, 0.1329]),
                color_lichen_b: Fp3([0.1789, 0.1193, 0.0835]),
                color_rim: Fp3([0.0331, 0.01, 0.0072]),
                grain_scale: Fp64(30.0),
                grain_strength: Fp64(0.0),
                relief: Fp64(0.4),
                normal_strength: Fp(0.6),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 - four pairs of small toothed leaves, bronze-green, just breaking.
    materials.insert(2, leaf_card(BRONZE, 4, 0.45));
    // 3 - twelve five-petalled notched pink flowers to a card.
    materials.insert(
        3,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.9),
            texture: SovereignTextureConfig::Flower(SovereignFlowerConfig {
                seed: 3,
                variant_rows: 4,
                variant_cols: 3,
                petal: SovereignPetalConfig {
                    seed: 5,
                    variant_rows: 1,
                    variant_cols: 1,
                    color_base: Fp3([0.89, 0.2957, 0.5071]),
                    color_edge: Fp3([0.9331, 0.448, 0.6383]),
                    color_throat: Fp3([0.8689, 0.2738, 0.477]),
                    length: Fp64(1.0),
                    width: Fp64(0.95),
                    peak: Fp64(0.65),
                    tip_notch: Fp64(0.12),
                    curl: Fp64(0.0),
                    asymmetry: Fp64(0.15),
                    normal_strength: Fp(0.6),
                },
                petal_count: 5,
                center_radius: Fp64(0.12),
                center_color: Fp3([0.8276, 0.214, 0.3424]),
                dot_density: Fp64(0.5),
                normal_strength: Fp(0.6),
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
    fn cherry_is_kept_as_built() {
        let built = FloweringTree.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the cherry");
    }

    /// White blossom recolours the blossom's texture and nothing else: the
    /// petals and their middles change, the flowers' shape and atlas, the
    /// bark and the leaves stay as authored.
    #[test]
    fn white_recolours_the_blossom_alone() {
        let authored = materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "white", &mut m);
        let (before, rest_before) = split_colours(&authored[&3].texture);
        let (after, rest_after) = split_colours(&m[&3].texture);
        assert_eq!(rest_after, rest_before, "only the blossom's colours change");
        assert_ne!(after, before, "the blossom is recoloured");
        for slot in [0u16, 2] {
            assert_eq!(m[&slot], authored[&slot], "slot {slot} is untouched");
        }
    }

    /// Summer turns every blossom card into a leaf spray of the summer
    /// leaves' colour - a card of eight pairs, fuller than the breaking
    /// leaves' four - turns the breaking leaves green too, and leaves the
    /// bark alone. A summer cherry has no flowers left: no slot wears one.
    #[test]
    fn summer_is_the_tree_in_leaf() {
        let authored = materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "summer", &mut m);
        assert_eq!(m[&0], authored[&0], "the bark is untouched");
        let leaves = |slot: u16| match &m[&slot].texture {
            SovereignTextureConfig::Twig(twig) => twig.clone(),
            other => panic!("slot {slot} is a leaf card in summer, not {other:?}"),
        };
        let (breaking, blossom) = (leaves(2), leaves(3));
        for twig in [&breaking, &blossom] {
            assert_eq!(twig.leaf.color_base.0, SUMMER[0]);
            assert_eq!(twig.leaf.color_edge.0, SUMMER[1]);
        }
        assert!(blossom.leaf_pairs > breaking.leaf_pairs, "a full spray");
        assert!(
            m.values()
                .all(|s| !matches!(s.texture, SovereignTextureConfig::Flower(_))),
            "no flowers left"
        );
    }

    /// A name the cherry does not know leaves its authored materials alone,
    /// the fallback the seeded pools rely on.
    #[test]
    fn an_unknown_variant_changes_nothing() {
        let authored = materials();
        let mut m = authored.clone();
        apply_named(VARIANTS, "no_such_variant", &mut m);
        assert_eq!(m, authored);
    }
}
