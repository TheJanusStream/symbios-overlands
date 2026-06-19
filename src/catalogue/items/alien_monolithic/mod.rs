//! Alien-Monolithic-theme catalogue structures — a precise, geometric site of
//! black obsidian monoliths and levitating, glyph-lit machinery.
//!
//! Two prosperity registers share one inscrutable identity: the established
//! ([`MONOLITH_BAND`]) active site (a black monolith, a levitating platform, a
//! light pylon, a glyph arch, a floating cube, a glyph stone, an energy node, a
//! monolith shard and a light disc) and the destitute ([`MONOLITH_POOR`])
//! dormant kit (a broken monolith, a dead pylon, glyph rubble).
//!
//! Surfaces use the real procedural generators rather than flat colour: a
//! polished [`obsidian`] sheen and dead matte [`stone`], with glyphs and
//! energy carried by [`crate::catalogue::items::util::glow`] emissive trim.
//! Monoliths hum and motes rise over the [`fx`] bed. The theme's blue-glow
//! accent lives in [`crate::seeded_defaults::room::accent`].

pub mod black_monolith;
pub mod energy_node;
pub mod floating_cube;
pub mod glyph_arch;
pub mod glyph_stone;
pub mod levitating_platform;
pub mod light_disc;
pub mod light_pylon;
pub mod monolith_shard;
// Poor (dormant) variants — the prosperity-Poor end of the theme.
pub mod broken_monolith;
pub mod dead_pylon;
pub mod glyph_rubble;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignMaterialSettings, SovereignMetalConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the active site — humming, glyph-lit monoliths
/// read as a Modest-to-Rich working array. The poor end of the theme is the
/// separate dormant kit ([`broken_monolith`], …), tagged `Poor`, so a
/// destitute alien room grows the dead, lightless site instead.
pub(super) const MONOLITH_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the dormant kit — the destitute end of the theme, never
/// picked for a modest or affluent alien room.
pub(super) const MONOLITH_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Polished black obsidian — the monoliths, pylons, platforms and arches. A
/// near-mirror dark sheen.
pub(super) fn obsidian(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.15),
        metallic: Fp(0.7),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.1, 0.1, 0.16]),
            roughness: Fp64(0.15),
            metallic: Fp(0.7),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dead matte stone — cracked, dormant monoliths and rubble, the light gone.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Stone palette.
pub(super) const OBSIDIAN: [f32; 3] = [0.06, 0.06, 0.10];
pub(super) const DEAD_STONE: [f32; 3] = [0.24, 0.24, 0.28];

// Emissive glyph / energy colours.
pub(super) const GLYPH_CYAN: [f32; 3] = [0.40, 0.90, 1.0];
pub(super) const ENERGY_BLUE: [f32; 3] = [0.45, 0.55, 1.0];
pub(super) const GLYPH_VIOLET: [f32; 3] = [0.62, 0.42, 1.0];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (dormant) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &broken_monolith::BrokenMonolith,
            &dead_pylon::DeadPylon,
            &glyph_rubble::GlyphRubble,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The black monolith is the kit's lit hero — it must keep its emissive
    /// glyphs so escalation's broken-emissive ruin pass has light to snuff.
    #[test]
    fn monolith_keeps_its_glyphs() {
        assert!(
            crate::catalogue::items::util::has_emissive(&black_monolith::BlackMonolith.build("")),
            "black monolith lost its emissive glyphs"
        );
    }
}
