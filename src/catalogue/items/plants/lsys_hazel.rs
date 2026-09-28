//! Hazel - the coppiced hazel (Corylus avellana) under the oak standards of
//! Ashmere's woods, in the agent Reeve's region, a Norfolk manor village of
//! about 1300 (#1496): a spreading many-stemmed shrub in late September, its
//! leaves turning yellow at the edges. The entry is the generator saved in
//! Ashmere's room record, reproduced to the ten-thousandth.
//!
//! It is the catalogue's own [bush](super::lsys_bush) - the same grammar,
//! shared rather than copied ([`lsys_bush::grammar`]) - in Ashmere's
//! materials, with coarser tubes (5 segments round against 8) and a root
//! transform that stretches the bush's dome into a coppice stool 2.3 times
//! as wide and 2.6 times as tall. Its leaf card is cut to 0.02 m to the unit
//! against the bush's 0.045, by about the factor that transform stretches
//! it, so in the world its leaves stay the bush's size (0.046 to 0.052 m to
//! the unit) rather than growing with the stool.
//!
//! **Slots.** 0 bark - the bush's Bark texture, its light colour a warmer
//! red-brown; 1 a Leaf card running from olive green at the midrib to
//! yellowing edges (linear RGB, under a white base colour).
//!
//! Measured with the render tool: 3.35 m tall and 3.85 x 3.65 m across,
//! 3,070 triangles in 2 parts.

use std::collections::HashMap;

use crate::catalogue::items::plants::lsys_bush;
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Generator, SovereignBarkConfig, SovereignLeafConfig, SovereignMaterialSettings,
    SovereignTextureConfig, TransformData,
};

pub struct Hazel;

impl CatalogueEntry for Hazel {
    fn slug(&self) -> &'static str {
        "lsys_hazel"
    }
    fn name(&self) -> &'static str {
        "Hazel"
    }
    fn description(&self) -> &'static str {
        "Coppiced hazel - a spreading many-stemmed woodland shrub, its leaves yellowing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator {
            transform: TransformData {
                scale: Fp3([2.3, 2.6, 2.3]),
                ..Default::default()
            },
            ..Generator::from_kind(lsys_bush::grammar(materials(), 0.02, 5))
        }
    }
}

fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - the bush's bark, warmer and redder in its light colour.
    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.32, 0.26, 0.18]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                color_light: Fp3([0.233, 0.1473, 0.089]),
                color_dark: Fp3([0.17, 0.13, 0.09]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 - olive-green leaves yellowing at the edges: late September.
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.85),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig {
                color_base: Fp3([0.0732, 0.1193, 0.0134]),
                color_edge: Fp3([0.196, 0.1789, 0.022]),
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
    fn hazel_is_kept_as_built() {
        let built = Hazel.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the hazel");
    }
}
