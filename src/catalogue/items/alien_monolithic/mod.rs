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
pub mod gateway;
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

use super::util::{tile, tiles_per_metre};
use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::fantasy::rune_marks;
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig,
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
        uv_scale: tiles_per_metre(tile::METAL),
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

// Emissive glyph / energy colours. Deeply saturated on purpose: `glow` sets
// both base_color and emission_color to these, and a too-pale colour
// over-brightens and washes to a near-white blank under bloom (the fantasy /
// steampunk over-bright-clips lesson — the original pale cyan/blue/violet all
// washed). Deep base hues with one channel near zero hold their colour driven
// emissive: cyan keeps red low, electric-blue keeps green low, violet keeps
// green near zero.
pub(super) const GLYPH_CYAN: [f32; 3] = [0.10, 0.82, 1.0];
pub(super) const ENERGY_BLUE: [f32; 3] = [0.18, 0.32, 1.0];
pub(super) const GLYPH_VIOLET: [f32; 3] = [0.46, 0.12, 1.0];

/// A vertical inscription of alien glyphs climbing a flat face at `z = zf`.
/// Reuses fantasy's [`rune_marks`] for the asymmetric stave-and-branch stroke
/// (an inscribed glyph that reads as script, not the blank "+++ ladder" the
/// old uniform light-bars gave), stacking one glyph per entry in `sizes`
/// evenly between `base_y` and `top_y`. Varied stroke heights plus an
/// alternating x-nudge keep the column from reading as one stamp repeated.
/// Strokes stand proud of the face — pass `zf` just past a slab's −Z front so
/// the inscription reads on the hero side.
pub(super) fn glyph_column(
    cx: f32,
    base_y: f32,
    top_y: f32,
    zf: f32,
    sizes: &[f32],
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let n = sizes.len();
    let mut v = Vec::new();
    for (k, &gh) in sizes.iter().enumerate() {
        let frac = if n <= 1 {
            0.5
        } else {
            k as f32 / (n - 1) as f32
        };
        let y = base_y + frac * (top_y - base_y);
        let nudge = if k % 2 == 0 { -0.14 } else { 0.10 };
        v.extend(rune_marks([cx + nudge, y, zf], gh, mat.clone()));
    }
    v
}

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
