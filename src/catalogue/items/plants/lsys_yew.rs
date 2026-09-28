//! Yew - the churchyard yew (Taxus baccata) of Ashmere, the agent Reeve's
//! region, a Norfolk manor village of about 1300 (#1496): a tree, where the
//! churchyard first had the smooth dark egg of a blob. It is the
//! [young oak](super::lsys_oak)'s tree - a broad low crown on a short bole,
//! 9.2 m tall, grown by the same grammar to the same age - dressed in yew's
//! materials: its leaf cards swapped for Needle cards in yew's near-black
//! green, its bark for a red-brown one. The entry is the generator saved in
//! Ashmere's room record, reproduced to the ten-thousandth; it was built by
//! `exports/reeve/b/yew.py` (outside the tracked tree) from the oak
//! builder's young-oak preset, `docs/agent/examples/trees/oak.py` `y`.
//!
//! **Slots.** 0 bark - the oak's swapped Rock texture in a dark red-brown
//! (its ridges the darker colour, the face the lighter); 1 and 2 the same
//! Needle card - flat needles in pairs along a woody shoot, splayed 70
//! degrees from it. The pairs are as many and the needles as broad as the
//! texture allows (`pair_count` 24 and `needle_width` 0.03, the maxima of
//! its envelope), at its default length, 0.3 of the card; `yew.py` notes
//! that short needles left the crown see-through. The young oak's 18 % of
//! bronzing tip masses land on slot 2, so here they are yew too. The prop
//! mapping stays `Twig`: a card of that shape, which the Needle texture
//! paints.
//!
//! Measured with the render tool: 9.2 m tall and 11.2 x 11.1 m across,
//! 3,614 triangles in 3 parts (the young oak's, since only materials
//! changed).

use std::collections::HashMap;

use crate::catalogue::items::plants::lsys_oak;
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignNeedleConfig,
    SovereignRockConfig, SovereignTextureConfig,
};

pub struct Yew;

impl CatalogueEntry for Yew {
    fn slug(&self) -> &'static str {
        "lsys_yew"
    }
    fn name(&self) -> &'static str {
        "Yew"
    }
    fn description(&self) -> &'static str {
        "Churchyard yew - a broad, dense crown of near-black needles on a red-brown trunk."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Plant
    }
    fn build(&self, _local_did: &str) -> Generator {
        lsys_oak::young_oak(materials())
    }
}

/// The yew's red-brown bark and its needle card, twice.
fn materials() -> HashMap<u16, SovereignMaterialSettings> {
    let mut materials = HashMap::new();
    // 0 - red-brown bark: the oak's swapped Rock texture, dark red-brown in
    // the ridges ("gaps") over a warmer red-brown face.
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
                color_light: Fp3([0.0331, 0.0085, 0.0049]),
                color_dark: Fp3([0.196, 0.0637, 0.0331]),
                normal_strength: Fp(2.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    // 1 and 2 - the needle card: both of the young oak's leaf slots.
    materials.insert(1, needle_card());
    materials.insert(2, needle_card());
    materials
}

/// Yew's flat needles along a woody shoot, near-black green (linear RGB).
fn needle_card() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(1.0),
        texture: SovereignTextureConfig::Needle(SovereignNeedleConfig {
            pair_count: 24,
            color_base: Fp3([0.0024, 0.0072, 0.0031]),
            color_tip: Fp3([0.006, 0.0196, 0.006]),
            color_shoot: Fp3([0.022, 0.0134, 0.006]),
            needle_angle: Fp64(70.0),
            // As wide as the texture's envelope allows.
            needle_width: Fp64(0.03),
            length_taper: Fp64(0.2),
            shoot_length: Fp64(0.85),
            shoot_width: Fp64(0.012),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{GeneratorKind, sanitize_generator};

    /// The saved record's generator is already what a record keeps: the
    /// sanitiser has nothing to clamp, cut or drop.
    #[test]
    fn yew_is_kept_as_built() {
        let built = Yew.build("");
        assert!(matches!(built.kind, GeneratorKind::LSystem { .. }));
        let mut kept = built.clone();
        sanitize_generator(&mut kept);
        assert_eq!(kept, built, "the sanitiser changed the yew");
    }
}
