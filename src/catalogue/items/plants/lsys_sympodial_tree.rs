//! Sympodial tree — a round-crowned park broadleaf. Sympodial branching where
//! each apex is stochastically replaced by two or three rolled/pitched
//! daughters (or extends un-forked), so the crown grows outward organically
//! with iteration count as its age (#910) and the seed varies the whole
//! silhouette — replacing the old planar `$`-flattened ABOP Fig 2.7 zigzag
//! that read as artificial. A short trunk in the axiom lifts the crown, and a
//! finalization rule blooms every apex into a dome of green leaf cards.

use std::collections::HashMap;

use crate::catalogue::items::plants::variant::{PlantVariant, tint_bark, tint_leaf};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignLeafConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};

/// Round-crown broadleaf re-skins (#910). Slot 0 bark, slot 2 leaf.
static VARIANTS: &[PlantVariant] = &[
    PlantVariant {
        name: "autumn",
        label: "Autumn (deep red)",
        apply: |m| {
            tint_leaf(
                m,
                2,
                [0.56, 0.20, 0.14],
                [0.46, 0.13, 0.09],
                [0.70, 0.32, 0.18],
            );
        },
    },
    PlantVariant {
        name: "blossom_pale",
        label: "Pale spring flush",
        apply: |m| {
            // Fresh yellow-green growth over pale bark — the same crown in
            // early spring, before the leaves darken.
            tint_leaf(
                m,
                2,
                [0.52, 0.62, 0.26],
                [0.44, 0.56, 0.20],
                [0.64, 0.72, 0.34],
            );
            tint_bark(
                m,
                0,
                [0.44, 0.36, 0.27],
                [0.52, 0.43, 0.33],
                [0.20, 0.15, 0.11],
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
        "Sympodial Tree"
    }
    fn description(&self) -> &'static str {
        "Round-crowned leafy broadleaf with stochastic sympodial branching."
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
    // 0 — brown bark trunk.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.34, 0.22, 0.11]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.40, 0.26, 0.14]),
                color_dark: Fp3([0.16, 0.10, 0.05]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 2 — deep broadleaf green (slot 2 matches the finalization's `,(2)`).
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([0.20, 0.40, 0.14]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.12, 0.30, 0.09]),
                color_edge: Fp3([0.22, 0.40, 0.13]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let mut prop_mappings = HashMap::new();
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        // A short trunk (F(2.0)F(1.6)) in the axiom lifts the sympodial crown.
        // The unexpanded `B(l,w)` apices are the branch tips; the finalization
        // rule blooms each into a two-ring dome of leaf cards (inner &40,
        // outer &80, plus a couple underside cards) so the tips merge into a
        // continuous spreading green canopy over a visible trunk.
        source_code: "#define wr 0.72\n\
                      omega: !(0.3)F(1.4)F(1.1)/(45)A(1.0,0.09)\n\
                      x1: 0.4 : A(l,w) -> !(w)F(l)[&(32)/(94)A(l*0.8,w*wr)][^(8)/(212)A(l*0.75,w*wr)]\n\
                      x2: 0.35 : A(l,w) -> !(w)F(l)[&(40)/(133)A(l*0.78,w*wr)][&(14)/(255)A(l*0.8,w*wr)][^(22)/(28)A(l*0.62,w*0.65)]\n\
                      x3: 0.25 : A(l,w) -> !(w)&(6)F(l*1.1)/(120)A(l*0.86,w*0.8)"
            .to_string(),
        finalization_code:
            "A(l,w) : * -> ,(2)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(90)[&(40)~(1,30)]/(45)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(90)[&(80)~(1,30)]/(45)[&(95)~(1,18)]/(180)[&(95)~(1,18)]"
                .to_string(),
        iterations: 8,
        seed: 1,
        angle: Fp(18.0),
        step: Fp(1.0),
        width: Fp(0.32),
        elasticity: Fp(0.08),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.05),
        mesh_resolution: 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = SympodialTree.build("");
        sanitize_generator(&mut g);
        assert!(matches!(g.kind, GeneratorKind::LSystem { .. }));
    }
}
