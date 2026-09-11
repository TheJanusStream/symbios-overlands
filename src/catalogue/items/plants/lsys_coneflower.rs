//! Coneflower — a clump-forming prairie perennial (#972, the civic garden
//! bed's flowers): a basal rosette of long leaves, and a clump of upright
//! stems each carrying alternate leaves and one flower head. Pink by
//! default (echinacea); [`VARIANTS`] re-skin it gold (rudbeckia) or white
//! without touching a symbol of the grammar.
//!
//! **Growth.** The crown `C(n)` is the whole-plant clock (playbook §1:
//! `age` is per-rewrite, so the plant carries its own count). Every step it
//! puts out one basal leaf at the golden angle; from its second step it also
//! bolts flowering stems, stochastically (a skipped step is the topology
//! budget of §7), each launched at a one-shot base tilt and rolled so the
//! clump opens out evenly. A stem `S(k)` grows one internode a step with an
//! alternate leaf at each node; from [`STEM_NODES_MIN`] nodes it may head
//! (`H`) and by [`STEM_NODES_MAX`] it must, so the heads stand at different
//! heights, and about a third of the time a lateral peduncle `P` breaks
//! below the head and carries a second, later one. So the stems bolted early are in flower
//! and the late ones still in green bud when derivation stops — a clump in
//! mid-season, not a bunch cut to one length.
//!
//! **Uprightness.** A vertical axis is tropism's fixpoint (§4), so a stem
//! launched straight up would stay a rod at any elasticity; the base tilt is
//! what lets a little gravity bow the stems outward. Elasticity is low: a
//! coneflower stem is stiff.
//!
//! **Heads.** A flower card is centred on its stem tip — pitched to face up
//! and out, then stepped back half its height with `f(-h)` so the card's
//! base is not what sits on the stem — and scaled per axis
//! (`~(id, sx, sy, sz)`) so the round sprite is not stretched to the leaf
//! card's 0.5 × 0.8.
//!
//! Slots: 0 bark (the green stem), 2 leaf, 3 flower. Slot 1 (twig) unused.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::PlantVariant;
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PropMeshType, SovereignFlowerConfig,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignPetalConfig, SovereignTextureConfig,
};

pub struct Coneflower;

impl CatalogueEntry for Coneflower {
    fn slug(&self) -> &'static str {
        "lsys_coneflower"
    }
    fn name(&self) -> &'static str {
        "Coneflower"
    }
    fn description(&self) -> &'static str {
        "Clump of upright flowering stems over a rosette of leaves."
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

/// Internodes a flowering stem grows before it may head, and the most it
/// grows before it must.
const STEM_NODES_MIN: u32 = 3;
const STEM_NODES_MAX: u32 = 5;

/// Flower re-skins: the petal, edge and throat colours and the cone's.
pub(crate) static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "gold",
        label: "Gold (rudbeckia)",
        apply: |m| {
            tint_flower(
                m,
                [0.96, 0.70, 0.10],
                [0.90, 0.58, 0.06],
                [0.70, 0.36, 0.04],
                [0.16, 0.09, 0.04],
            )
        },
    },
    PlantVariant {
        name: "white",
        label: "White",
        apply: |m| {
            tint_flower(
                m,
                [0.95, 0.94, 0.88],
                [0.88, 0.87, 0.80],
                [0.92, 0.86, 0.60],
                [0.80, 0.52, 0.12],
            )
        },
    },
];

/// Re-colour slot 3's flower sprite: petal base, edge and throat, and the
/// cone at the centre. The lit `base_color` follows the petal.
fn tint_flower(
    materials: &mut HashMap<u16, SovereignMaterialSettings>,
    petal: [f32; 3],
    edge: [f32; 3],
    throat: [f32; 3],
    cone: [f32; 3],
) {
    let Some(m) = materials.get_mut(&3) else {
        return;
    };
    m.base_color = Fp3(petal);
    if let SovereignTextureConfig::Flower(f) = &mut m.texture {
        f.petal.color_base = Fp3(petal);
        f.petal.color_edge = Fp3(edge);
        f.petal.color_throat = Fp3(throat);
        f.center_color = Fp3(cone);
    }
}

pub(crate) fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();
    // 0 — green flowering stem.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.26, 0.36, 0.14]),
            roughness: Fp(0.8),
            ..Default::default()
        },
    );
    // 2 — long, lanceolate, finely toothed dark leaf.
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.22, 0.38, 0.14]),
            roughness: Fp(0.75),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.13, 0.28, 0.08]),
                color_edge: Fp3([0.20, 0.34, 0.10]),
                serration_strength: Fp64(0.1),
                lobe_count: Fp64(0.0),
                lobe_depth: Fp64(0.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 3 — the flower head: narrow petals round a big, raised-looking cone.
    // One flower per card (a 1 × 1 atlas), so a head is one bloom.
    materials.insert(
        3,
        SovereignMaterialSettings {
            base_color: Fp3([0.88, 0.46, 0.66]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Flower(SovereignFlowerConfig {
                variant_rows: 1,
                variant_cols: 1,
                petal: SovereignPetalConfig {
                    color_base: Fp3([0.88, 0.46, 0.66]),
                    color_edge: Fp3([0.80, 0.36, 0.58]),
                    color_throat: Fp3([0.70, 0.26, 0.46]),
                    width: Fp64(0.34),
                    ..Default::default()
                },
                petal_count: 12,
                center_radius: Fp64(0.26),
                center_color: Fp3([0.52, 0.22, 0.06]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(0, PropMeshType::Leaf);

    // A head is `fh` tall and as wide (sx = 1.6·sy squares the 0.5 × 0.8
    // card); `hh` is half its height in metres (fh · 0.8 · prop_scale / 2),
    // for stepping back onto the tip.
    GeneratorKind::LSystem {
        source_code: format!(
            "#define il 0.085\n\
             #define kmin {STEM_NODES_MIN}\n\
             #define kmax {STEM_NODES_MAX}\n\
             omega: !(0.004)C(0)\n\
             c1: C(n) : n < 1 -> R/(137.5)R/(137.5)C(n+1)\n\
             c2: 0.85 : C(n) : n >= 1 -> R/(137.5)[&(24)S(0)]/(137.5)C(n+1)\n\
             c3: 0.15 : C(n) : n >= 1 -> R/(137.5)C(n+1)\n\
             s1: S(k) : k < kmin -> !(0.009)F(il)L/(137.5)S(k+1)\n\
             s2: 0.4 : S(k) : k >= kmin -> H\n\
             s3: 0.35 : S(k) : k >= kmin -> [&(40)P(0)]/(180)H\n\
             s4: 0.25 : S(k) : k >= kmin & k < kmax -> !(0.009)F(il)L/(137.5)S(k+1)\n\
             p1: P(k) : k < 2 -> !(0.006)F(il*0.8)P(k+1)\n\
             p2: P(k) : k >= 2 -> H"
        ),
        finalization_code: "#define fh 2.9\n\
             #define hh 0.052\n\
             r1: R -> ,(2)[&(66)~(0,2.4,4.0,1)]\n\
             l1: L -> ,(2)[&(46)~(0,1.5,2.5,1)]\n\
             h1: H -> ,(3)[&(48)f(-hh)~(0,fh*1.6,fh,1)]\n\
             s5: S(k) : * -> ,(2)[~(0,1.0,1.4,1)]\n\
             p3: P(k) : * -> ,(2)[~(0,0.9,1.2,1)]\n\
             c4: C(n) : * -> "
            .to_string(),
        iterations: 11,
        seed: 1,
        angle: Fp(30.0),
        step: Fp(1.0),
        width: Fp(0.004),
        elasticity: Fp(0.05),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.045),
        mesh_resolution: 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Coneflower.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }

    /// Every re-skin keeps the flower sprite and changes its colour.
    #[test]
    fn every_variant_recolours_the_flower_slot() {
        for v in VARIANTS {
            let GeneratorKind::LSystem { mut materials, .. } = build_kind() else {
                unreachable!()
            };
            let before = materials[&3].clone();
            (v.apply)(&mut materials);
            let after = &materials[&3];
            assert!(
                matches!(after.texture, SovereignTextureConfig::Flower(_)),
                "{}: the flower sprite was replaced",
                v.name
            );
            assert_ne!(after, &before, "{}: nothing changed", v.name);
        }
    }
}
