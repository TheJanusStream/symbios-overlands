//! Gorse - the furze (Ulex) of the heath in Ashmere, the agent Reeve's
//! region, a Norfolk manor village of about 1300 (#1496): low, dense,
//! spiny clumps in a dark evergreen, between the ling and the bracken. The
//! heath's gorse is the yellow-flowered furze, but this bush carries no
//! flower: it is the late-September gorse in its dark green. The entry is
//! the generator saved in Ashmere's room record, reproduced to the
//! ten-thousandth.
//!
//! It is the catalogue's own [bush](super::lsys_bush) - the same grammar,
//! shared rather than copied ([`lsys_bush::grammar`]) - re-tinted dark, with
//! coarser tubes (5 segments round against 8) and a root transform that
//! spreads the bush's dome 1.15 times as wide at the same height.
//!
//! **Slots.** 0 bark - the bush's own; 1 a Leaf card in a dark, dull green
//! under a grey-green base colour.
//!
//! Measured with the render tool: 1.6 m tall and 2.7 x 2.6 m across,
//! 3,070 triangles in 2 parts.

use std::collections::HashMap;

use crate::catalogue::items::plants::lsys_bush;
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, SovereignBarkConfig, SovereignLeafConfig, SovereignMaterialSettings,
    SovereignTextureConfig, TransformData,
};

pub struct Gorse;

impl CatalogueEntry for Gorse {
    fn slug(&self) -> &'static str {
        "lsys_gorse"
    }
    fn name(&self) -> &'static str {
        "Gorse"
    }
    fn description(&self) -> &'static str {
        "Low, dense clump of dark spiny furze - the heathland shrub."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator {
            transform: TransformData {
                scale: Fp3([1.15, 1.0, 1.15]),
                ..Default::default()
            },
            ..Generator::from_kind(lsys_bush::grammar(materials(), 0.045, 5))
        }
    }
}

fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - the bush's grey-brown twiggy bark, unchanged.
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
    // 1 - dark, dull evergreen.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.3, 0.16]),
            roughness: Fp(0.8),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.07, 0.15, 0.06]),
                color_edge: Fp3([0.13, 0.22, 0.08]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    materials
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{GeneratorKind, sanitize_generator};

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn gorse_is_kept_as_built() {
        let built = Gorse.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the gorse");
    }
}
