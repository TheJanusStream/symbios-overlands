//! Plant material variants — the **re-skin lever** (#910 WS2 part C).
//!
//! A [`PlantVariant`] swaps a species' bark/foliage materials without
//! touching a single symbol of its grammar. Because the L-system mesh cache
//! is keyed on the geometry fingerprint while materials live in a separate
//! cache (see [`crate::world_builder::lsystem`]), a variant is very nearly
//! free: it re-skins a plant without re-deriving or re-meshing it.
//!
//! This is what lets a pool of ~14 grammars cover every biome. One conifer
//! skeleton is a blue-green spruce in the boreal taiga, a warm olive pine on
//! an alpine ridge, and a gold larch in the tundra; one broadleaf is summer
//! green in a lush valley and rust-orange in a temperate autumn. Per
//! `docs/lsystem-playbook.md`, appearance belongs to materials and the
//! finalization pass — never to the growth rules.
//!
//! Variants are looked up **by name**, not by index, so reordering a
//! species' variant list can't silently repaint a biome. An unknown name
//! resolves to the species' authored default materials.

use std::collections::HashMap;

use crate::pds::{Fp3, SovereignMaterialSettings, SovereignTextureConfig};

/// One named re-skin of a plant species.
///
/// `apply` mutates the generator's material map in place. It receives the
/// species' own authored materials, so a variant only has to override what
/// it actually changes.
pub struct PlantVariant {
    /// Stable identifier referenced by the seeded species pools. Renaming
    /// one silently drops the biome back to default materials, so treat it
    /// as part of the pool's contract.
    pub name: &'static str,
    /// Human-readable label for tooling.
    pub label: &'static str,
    /// Mutates the species' material slots into this variant's look.
    pub apply: fn(&mut HashMap<u16, SovereignMaterialSettings>),
}

/// Re-tint a slot carrying a [`SovereignTextureConfig::Leaf`] texture.
///
/// `base` tints the lit material; `deep` / `edge` drive the procedural leaf
/// sprite's interior and rim. A slot whose texture is not a `Leaf` keeps its
/// texture and only takes the `base_color` change — so calling this on a
/// Twig- or Flower-textured slot degrades gracefully instead of erasing the
/// species' authored sprite.
pub fn tint_leaf(
    materials: &mut HashMap<u16, SovereignMaterialSettings>,
    slot: u16,
    base: [f32; 3],
    deep: [f32; 3],
    edge: [f32; 3],
) {
    let Some(m) = materials.get_mut(&slot) else {
        return;
    };
    m.base_color = Fp3(base);
    if let SovereignTextureConfig::Leaf(leaf) = &mut m.texture {
        leaf.color_base = Fp3(deep);
        leaf.color_edge = Fp3(edge);
    }
}

/// Re-tint a slot carrying a [`SovereignTextureConfig::Twig`] texture (a
/// needle/twig card whose nested leaf config drives the sprite).
pub fn tint_twig(
    materials: &mut HashMap<u16, SovereignMaterialSettings>,
    slot: u16,
    base: [f32; 3],
    deep: [f32; 3],
    edge: [f32; 3],
) {
    let Some(m) = materials.get_mut(&slot) else {
        return;
    };
    m.base_color = Fp3(base);
    if let SovereignTextureConfig::Twig(twig) = &mut m.texture {
        twig.leaf.color_base = Fp3(deep);
        twig.leaf.color_edge = Fp3(edge);
    }
}

/// Re-tint a slot carrying a [`SovereignTextureConfig::Bark`] texture.
pub fn tint_bark(
    materials: &mut HashMap<u16, SovereignMaterialSettings>,
    slot: u16,
    base: [f32; 3],
    light: [f32; 3],
    dark: [f32; 3],
) {
    let Some(m) = materials.get_mut(&slot) else {
        return;
    };
    m.base_color = Fp3(base);
    if let SovereignTextureConfig::Bark(bark) = &mut m.texture {
        bark.color_light = Fp3(light);
        bark.color_dark = Fp3(dark);
    }
}

/// Resolve a variant by name and apply it. An empty or unknown name leaves
/// the species' authored materials untouched, which is the correct fallback
/// for every pool entry that doesn't ask for a re-skin.
pub fn apply_named(
    variants: &'static [PlantVariant],
    name: &str,
    materials: &mut HashMap<u16, SovereignMaterialSettings>,
) {
    if name.is_empty() {
        return;
    }
    if let Some(v) = variants.iter().find(|v| v.name == name) {
        (v.apply)(materials);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, SovereignBarkConfig, SovereignLeafConfig};

    fn leaf_slot() -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            base_color: Fp3([0.1, 0.2, 0.3]),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig::default()),
            ..Default::default()
        }
    }

    #[test]
    fn tint_leaf_rewrites_base_and_sprite_colors() {
        let mut m = HashMap::from([(1u16, leaf_slot())]);
        tint_leaf(
            &mut m,
            1,
            [0.9, 0.5, 0.1],
            [0.8, 0.4, 0.05],
            [0.95, 0.6, 0.2],
        );
        let slot = &m[&1];
        assert_eq!(slot.base_color.0, [0.9, 0.5, 0.1]);
        let SovereignTextureConfig::Leaf(leaf) = &slot.texture else {
            panic!("texture kind changed");
        };
        assert_eq!(leaf.color_base.0, [0.8, 0.4, 0.05]);
        assert_eq!(leaf.color_edge.0, [0.95, 0.6, 0.2]);
    }

    #[test]
    fn tint_on_mismatched_texture_keeps_the_sprite() {
        // A Bark-textured slot handed to `tint_leaf` must keep its bark
        // texture — a variant must never erase the species' authored sprite.
        let mut m = HashMap::from([(
            0u16,
            SovereignMaterialSettings {
                texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
                    color_light: Fp3([0.4, 0.3, 0.2]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]);
        tint_leaf(&mut m, 0, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        let SovereignTextureConfig::Bark(bark) = &m[&0].texture else {
            panic!("bark texture was replaced");
        };
        assert_eq!(bark.color_light.0, [0.4, 0.3, 0.2]);
        assert_eq!(m[&0].base_color.0, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn missing_slot_is_a_no_op() {
        let mut m: HashMap<u16, SovereignMaterialSettings> = HashMap::new();
        tint_leaf(&mut m, 7, [0.0; 3], [0.0; 3], [0.0; 3]);
        assert!(m.is_empty());
    }

    #[test]
    fn unknown_and_empty_variant_names_leave_materials_untouched() {
        fn paint(m: &mut HashMap<u16, SovereignMaterialSettings>) {
            m.get_mut(&1).unwrap().roughness = Fp(0.123);
        }
        static VARIANTS: &[PlantVariant] = &[PlantVariant {
            name: "autumn",
            label: "Autumn",
            apply: paint,
        }];

        let mut m = HashMap::from([(1u16, leaf_slot())]);
        apply_named(VARIANTS, "", &mut m);
        assert_ne!(m[&1].roughness.0, 0.123);
        apply_named(VARIANTS, "nope", &mut m);
        assert_ne!(m[&1].roughness.0, 0.123);
        apply_named(VARIANTS, "autumn", &mut m);
        assert_eq!(m[&1].roughness.0, 0.123);
    }
}
