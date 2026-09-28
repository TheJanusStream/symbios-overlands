//! Oak and young oak - Ashmere's open-grown English oak (Quercus robur) in
//! late September, from the agent Reeve's region, a Norfolk manor village of
//! about 1300 (#1496). Both entries are the generator saved in Ashmere's
//! room record, reproduced to the ten-thousandth; they were built by
//! `docs/agent/examples/trees/oak.py` (session 879: its variant `a`, and its
//! young-oak preset `y`), whose docstring lists every variant tried and why
//! it was rejected, and the engine facts found on the way. Change the tree
//! in the builder and judge it in pictures, then copy the saved record
//! here - not the other way round.
//!
//! **Growth.** A dichotomous sympodial crown on a short bole. Five scaffold
//! limbs leave the trunk, each launched from the foot and walked up to its
//! height with `f`, so the trunk stays one unbroken tube, and a weak leader
//! tops it. An apex `A(v,s,d)` (vigour, flank, forks so far) forks into two
//! unequal shoots, one on the upper flank and one on the lower, so the pair
//! does not climb and the crown's skirt comes down (`f1`-`f3`); or it bends
//! at an elbow without forking (`e1`-`e3`), or stalls a year (`st`), until
//! it has forked `G` times. The last fork is drawn by its cards alone. Every
//! internode grows by one law, `r*m*min(lm, l0 + dl*age)`, so the iteration
//! count is the tree's age: 11 is the oak, 5 the young oak.
//!
//! **Foliage.** At finalization every tip becomes an 11-card mass, every
//! fork's `K` an 8-card side spray (so the limbs are leafy along their outer
//! half, not only at their ends) and the two `J` burrs on the bole
//! epicormic tufts. About a sixth of the tip masses (weight 0.18) take the
//! bronzing slot 2.
//!
//! **Slots.** 0 bark - a Rock texture with its colours swapped, so the
//! ridged multifractal's sharp ridges are near-black fissures across a grey
//! face, stretched up the tubes by `;(0.2)` (the Bark texture drew smooth
//! planed timber in the world's light); 1 a Twig card of eight alternate,
//! lobed, untoothed leaves and a terminal one, in dark green; 2 the same
//! card bronzing.
//!
//! Grammar units are metres before the root transform's 1.08 scale.
//! Measured with the render tool, the oak stands 16.7 m and spreads
//! 21.4 x 21.2 m in 7,674 triangles and 3 parts; the young oak - iterations
//! 5 with every card scaled by 1.6/2.1 (its tip cards at 1.6 m rather than
//! 2.1, the sprays and burr tufts in proportion), since 2.1 m cards make a
//! 9 m tree's leaves look far too big from 15 m - stands 9.2 m and spreads
//! 11.2 x 11.1 m in 3,614 triangles and 3 parts. The trunk's foot is sunk
//! 0.3 m below the origin so it stays buried on a slope. The
//! [yew](super::lsys_yew) is the young oak's tree in yew's materials.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignRockConfig, SovereignTextureConfig, SovereignTwigConfig,
    TransformData,
};

/// Ashmere's oak at iterations 11: the field and hedgerow oak of its crofts
/// and the deer park's wood-pasture.
pub struct Oak;

impl CatalogueEntry for Oak {
    fn slug(&self) -> &'static str {
        "lsys_oak"
    }
    fn name(&self) -> &'static str {
        "Oak"
    }
    fn description(&self) -> &'static str {
        "Open-grown English oak - low crooked limbs, fissured grey bark, a broad lumpy crown."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        oak(11, FINALIZATION_CODE, materials())
    }
}

/// The oak's preset `y`: the same grammar at iterations 5, with every card
/// scaled by 1.6/2.1.
pub struct YoungOak;

impl CatalogueEntry for YoungOak {
    fn slug(&self) -> &'static str {
        "lsys_young_oak"
    }
    fn name(&self) -> &'static str {
        "Young oak"
    }
    fn description(&self) -> &'static str {
        "Young English oak about nine metres tall - the oak at an earlier age."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        young_oak(materials())
    }
}

/// The young oak's tree dressed in `materials` - slot 0 the bark, slots 1
/// and 2 the tip cards. The [yew](super::lsys_yew) grows from here too.
pub(super) fn young_oak(materials: HashMap<u16, SovereignMaterialSettings>) -> Generator {
    oak(5, YOUNG_FINALIZATION_CODE, materials)
}

/// The oak's grammar grown `iterations` years, its buds expressed by
/// `finalization_code` and dressed in `materials`, under the 1.08 root
/// scale its grammar units were drawn for.
fn oak(
    iterations: u32,
    finalization_code: &str,
    materials: HashMap<u16, SovereignMaterialSettings>,
) -> Generator {
    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);

    Generator {
        transform: TransformData {
            scale: Fp3([1.08, 1.08, 1.08]),
            ..Default::default()
        },
        ..Generator::from_kind(GeneratorKind::LSystem {
            source_code: SOURCE_CODE.to_string(),
            finalization_code: finalization_code.to_string(),
            iterations,
            seed: 1,
            angle: Fp(30.0),
            step: Fp(1.0),
            width: Fp(0.1),
            // Droop per tube segment: the long low limbs sag.
            elasticity: Fp(0.035),
            tropism: Some(Fp3([0.0, -1.0, 0.0])),
            materials,
            prop_mappings,
            prop_scale: Fp(1.0),
            mesh_resolution: 5,
        })
    }
}

/// The oak's growth rules: the bole, its burrs `J`, the scaffold limbs and
/// leader `A` in the axiom, then the apex's fork (`f`), elbow (`e`) and
/// stall (`st`) rules by fork depth, and the growth laws `g1`-`g4` that
/// lengthen every internode and thicken every tube with age.
const SOURCE_CODE: &str = "\
    #define l0 0.9\n\
    #define dl 0.25\n\
    #define lm 3.4\n\
    #define bw 0.2\n\
    #define vb 1.13\n\
    #define vt 1.12\n\
    #define wt 0.3\n\
    #define G 6\n\
    omega: ;(0.2)!(wt*1.6,-1)f(-0.3)[f(0.9412*l0,0.9412,0,1)/(70)&(80)J(0.55)][f(0.9412*l0,0.9412,0,1)/(215)&(80)J(0.6)][f(0.9412*l0,0.9412,0,1)/(20)&(74)!(bw*1*1.1/vb,1)f(0)A(1,1,0)][f(0.9412*l0,0.9412,0,1)/(160)&(68)!(bw*0.95*1.1/vb,1)f(0)A(0.95,-1,0)][f(1.1176*l0,1.1176,0,1)/(250)&(76)!(bw*1*1.1/vb,1)f(0)A(1,1,0)][f(1.2647*l0,1.2647,0,1)/(330)&(64)!(bw*0.9*1.1/vb,1)f(0)A(0.9,-1,1)][f(1.2647*l0,1.2647,0,1)/(95)&(72)!(bw*0.9*1.1/vb,1)f(0)A(0.9,1,1)]!(wt*1.6,-1)F(0.1471*l0,0.1471,0,1)!(wt*1.2,-1)F(0.1176*l0,0.1176,0,1)!(wt*1,-1)F(0.6765*l0,0.6765,0,1)!(wt*0.96,-1)F(0.1765*l0,0.1765,0,1)!(wt*0.92,-1)F(0.1471*l0,0.1471,0,1)!(wt*0.88,-1)F(0.1176*l0,0.1176,0,1)&(6)A(0.85,1,1)\n\
    f1: 0.6 : A(v,s,d) : d < 3 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(20)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$[f(-l0*v*0.45,-0.45,0,v)/(s*140)&(55)K(v)][/(s*45)&(34)!(bw*v*0.744/vb,1)f(0)A(v*0.8,s,d+1)]F(0.01)/(-s*100)&(34)A(v*0.76,-s,d+1)\n\
    e1: 0.24 : A(v,s,d) : d < 3 -> !(bw*v,1)F(l0*v*0.5,0.5,0,v)$/(-s*70)&(20)!(bw*v*0.93,1)F(l0*v*0.5,0.5,0,v)$/(s*35)&(24)A(v*0.88,-s,d+0.5)\n\
    f2: 0.6 : A(v,s,d) : d >= 3 & d < 5 -> !(bw*v,1)F(l0*v,1,0,v)$[f(-l0*v*0.45,-0.45,0,v)/(s*140)&(55)K(v)][/(s*45)&(34)!(bw*v*0.8/vb,1)f(0)A(v*0.8,s,d+1)]/(-s*100)&(34)A(v*0.76,-s,d+1)\n\
    e2: 0.24 : A(v,s,d) : d >= 3 & d < 5 -> !(bw*v,1)F(l0*v,1,0,v)$/(s*35)&(24)A(v*0.88,-s,d+0.5)\n\
    f3: 0.6 : A(v,s,d) : d >= 5 & d < 6 -> f(l0*v,1,0,v)$[/(s*45)&(34)A(v*0.8,s,d+1)]/(-s*100)&(34)A(v*0.76,-s,d+1)\n\
    e3: 0.24 : A(v,s,d) : d >= 5 & d < 6 -> f(l0*v,1,0,v)$/(s*35)&(24)A(v*0.88,-s,d+0.5)\n\
    st: 0.16 : A(v,s,d) : d < G -> A(v,s,d)\n\
    g1: F(x,r,g,m) -> F(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g2: f(x,r,g,m) -> f(r*m*min(lm,l0+dl*(g+1)),r,g+1,m)\n\
    g3: !(w,t) : t > 0 -> !(w*vb,t)\n\
    g4: !(w,t) : t < 0 -> !(w*vt,t)";

/// The oak's finalization: 2.1 m tip cards, an 11-card mass at every tip
/// (18 % of them bronzing), an 8-card spray at every fork and the burrs.
const FINALIZATION_CODE: &str = "\
    0.82 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,2.1)][,(1)/(180)&(30)/(-90)~(0,1.995)][,(1)/(60)&(45)/(40)~(0,2.1)][,(1)/(120)&(50)/(-40)~(0,1.995)][,(1)/(240)&(50)/(40)~(0,1.995)][,(1)/(300)&(45)/(-40)~(0,2.1)][,(1)/(45)&(95)/(-90)~(0,1.89)][,(1)/(315)&(95)/(-90)~(0,1.89)][,(1)f(-0.8)/(30)&(110)/(-80)~(0,1.89)][,(1)f(-0.8)/(210)&(115)/(-80)~(0,1.785)][,(1)/(0)&(75)/(-90)~(0,1.89)]\n\
    0.18 : A(v,s,d) : * -> $[,(2)/(0)&(25)/(-90)~(0,2.1)][,(2)/(180)&(30)/(-90)~(0,1.995)][,(2)/(60)&(45)/(40)~(0,2.1)][,(2)/(120)&(50)/(-40)~(0,1.995)][,(2)/(240)&(50)/(40)~(0,1.995)][,(2)/(300)&(45)/(-40)~(0,2.1)][,(2)/(45)&(95)/(-90)~(0,1.89)][,(2)/(315)&(95)/(-90)~(0,1.89)][,(2)f(-0.8)/(30)&(110)/(-80)~(0,1.89)][,(2)f(-0.8)/(210)&(115)/(-80)~(0,1.785)][,(2)/(0)&(75)/(-90)~(0,1.89)]\n\
    K(v) : * -> $f(0.4)[,(1)/(0)&(25)/(-90)~(0,1.785)][,(1)/(180)&(30)/(-90)~(0,1.6957)][,(1)/(60)&(45)/(40)~(0,1.785)][,(1)/(120)&(50)/(-40)~(0,1.6957)][,(1)/(240)&(50)/(40)~(0,1.6957)][,(1)/(300)&(45)/(-40)~(0,1.785)][,(1)/(45)&(95)/(-90)~(0,1.6065)][,(1)/(315)&(95)/(-90)~(0,1.6065)]\n\
    J(c) : * -> $f(0.55)[,(1)/(0)&(25)/(-90)~(0,c*1.26)][,(1)/(180)&(30)/(-90)~(0,c*1.197)][,(1)/(60)&(45)/(40)~(0,c*1.26)][,(1)/(120)&(50)/(-40)~(0,c*1.197)][,(1)/(240)&(50)/(40)~(0,c*1.197)][,(1)/(300)&(45)/(-40)~(0,c*1.26)][,(1)/(45)&(95)/(-90)~(0,c*1.134)]";

/// The young oak's finalization: the oak's with every card scaled by
/// 1.6/2.1 - 1.6 m tip cards, the side sprays and burr tufts in proportion.
const YOUNG_FINALIZATION_CODE: &str = "\
    0.82 : A(v,s,d) : * -> $[,(1)/(0)&(25)/(-90)~(0,1.6)][,(1)/(180)&(30)/(-90)~(0,1.52)][,(1)/(60)&(45)/(40)~(0,1.6)][,(1)/(120)&(50)/(-40)~(0,1.52)][,(1)/(240)&(50)/(40)~(0,1.52)][,(1)/(300)&(45)/(-40)~(0,1.6)][,(1)/(45)&(95)/(-90)~(0,1.44)][,(1)/(315)&(95)/(-90)~(0,1.44)][,(1)f(-0.8)/(30)&(110)/(-80)~(0,1.44)][,(1)f(-0.8)/(210)&(115)/(-80)~(0,1.36)][,(1)/(0)&(75)/(-90)~(0,1.44)]\n\
    0.18 : A(v,s,d) : * -> $[,(2)/(0)&(25)/(-90)~(0,1.6)][,(2)/(180)&(30)/(-90)~(0,1.52)][,(2)/(60)&(45)/(40)~(0,1.6)][,(2)/(120)&(50)/(-40)~(0,1.52)][,(2)/(240)&(50)/(40)~(0,1.52)][,(2)/(300)&(45)/(-40)~(0,1.6)][,(2)/(45)&(95)/(-90)~(0,1.44)][,(2)/(315)&(95)/(-90)~(0,1.44)][,(2)f(-0.8)/(30)&(110)/(-80)~(0,1.44)][,(2)f(-0.8)/(210)&(115)/(-80)~(0,1.36)][,(2)/(0)&(75)/(-90)~(0,1.44)]\n\
    K(v) : * -> $f(0.4)[,(1)/(0)&(25)/(-90)~(0,1.36)][,(1)/(180)&(30)/(-90)~(0,1.292)][,(1)/(60)&(45)/(40)~(0,1.36)][,(1)/(120)&(50)/(-40)~(0,1.292)][,(1)/(240)&(50)/(40)~(0,1.292)][,(1)/(300)&(45)/(-40)~(0,1.36)][,(1)/(45)&(95)/(-90)~(0,1.224)][,(1)/(315)&(95)/(-90)~(0,1.224)]\n\
    J(c) : * -> $f(0.55)[,(1)/(0)&(25)/(-90)~(0,c*0.96)][,(1)/(180)&(30)/(-90)~(0,c*0.912)][,(1)/(60)&(45)/(40)~(0,c*0.96)][,(1)/(120)&(50)/(-40)~(0,c*0.912)][,(1)/(240)&(50)/(40)~(0,c*0.912)][,(1)/(300)&(45)/(-40)~(0,c*0.96)][,(1)/(45)&(95)/(-90)~(0,c*0.864)]";

/// The oak's bark and its two leaf cards.
fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - fissured grey-brown bark: a Rock texture with its colours swapped,
    // near-black in the ridges ("gaps") over a grey stone face.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.95),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Rock(SovereignRockConfig {
                scale: Fp64(5.0),
                octaves: 6,
                attenuation: Fp64(3.0),
                color_light: Fp3([0.0024, 0.0024, 0.0024]),
                color_dark: Fp3([0.0684, 0.0684, 0.0637]),
                normal_strength: Fp(2.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - dark green leaves, with almost no red in them: the Leaf texture
    // adds its veins' yellow cast on top.
    materials.insert(
        1,
        leaf_card([0.0024, 0.047, 0.0049], [0.0039, 0.0637, 0.0049]),
    );
    // 2 - the same card bronzing: late September.
    materials.insert(
        2,
        leaf_card([0.0174, 0.0397, 0.0031], [0.047, 0.055, 0.0031]),
    );
    materials
}

/// A Twig card of eight alternate, lobed, untoothed oak leaves, the leaf
/// running from `base` at the midrib to `edge` (linear RGB).
fn leaf_card(base: [f32; 3], edge: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(1.0),
        texture: SovereignTextureConfig::Twig(SovereignTwigConfig {
            leaf: SovereignLeafConfig {
                color_base: Fp3(base),
                color_edge: Fp3(edge),
                serration_strength: Fp64(0.0),
                vein_angle: Fp64(2.0),
                micro_detail: Fp64(0.2),
                normal_strength: Fp(0.3),
                lobe_depth: Fp64(0.34),
                lobe_sharpness: Fp64(0.7),
                petiole_length: Fp64(0.04),
                petiole_width: Fp64(0.02),
                midrib_width: Fp64(0.05),
                vein_count: Fp64(9.0),
                venule_strength: Fp64(0.1),
                ..Default::default()
            },
            stem_color: Fp3([0.0732, 0.0637, 0.0272]),
            stem_half_width: Fp64(0.008),
            leaf_pairs: 8,
            leaf_angle: Fp64(2.0),
            leaf_scale: Fp64(0.27),
            stem_curve: Fp64(0.01),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    fn assert_kept_as_built(entry: &dyn CatalogueEntry) {
        let built = entry.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(
            kept,
            built,
            "{}: the sanitiser changed the build",
            entry.slug()
        );
    }

    #[test]
    fn oak_is_kept_as_built() {
        assert_kept_as_built(&Oak);
    }

    #[test]
    fn young_oak_is_kept_as_built() {
        assert_kept_as_built(&YoungOak);
    }
}
