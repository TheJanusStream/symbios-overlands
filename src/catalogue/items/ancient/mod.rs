//! AncientClassical-theme catalogue structures — Greco-Roman /
//! bronze-age architecture, and the **settlement fallback theme**: an
//! un-built theme borrows this kit, so it carries a deep roster and a high
//! marble/sandstone material bar.
//!
//! The grand landmarks are the legacy entries ([`ruined_temple`],
//! [`lighthouse`], [`stone_circle`], [`ziggurat`]); the established
//! ([`ANCIENT_BAND`]) town fills in around them with the [`villa`] and
//! [`observatory`] plus the primitive-built [`colonnade`], [`amphitheatre`],
//! [`bathhouse`], and the [`column_drum`], [`urn`], [`statue_plinth`] and
//! firelit [`brazier`] props. The destitute ([`ANCIENT_POOR`]) end grows
//! the [`mudbrick_hut`] and [`ruined_wall`].
//!
//! Surfaces use the real procedural generators: veined [`marble`], coursed
//! [`sandstone`], sun-baked [`adobe`], smooth [`terracotta`], and patinated
//! [`bronze`]. The brazier comes alive with the small particle emitters and
//! the fire crackle in [`fx`] (ember sparks, low flame). The theme's warm
//! sandstone-gold light accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod lighthouse;
pub mod observatory;
pub mod ruined_temple;
pub mod stone_circle;
pub mod villa;
pub mod ziggurat;
// Established (town) secondaries + props — primitive-built.
pub mod amphitheatre;
pub mod bathhouse;
pub mod brazier;
pub mod colonnade;
pub mod column_drum;
pub mod statue_plinth;
pub mod urn;
// Poor variants — the prosperity-Poor end of the theme.
pub mod mudbrick_hut;
pub mod ruined_wall;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAshlarConfig, SovereignMarbleConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignStuccoConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established town kit — marble colonnades,
/// baths and statuary read as a Modest-to-Rich classical settlement. The
/// poor end is the separate mudbrick kit ([`mudbrick_hut`], …), tagged
/// `Poor`.
pub(super) const ANCIENT_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the mudbrick kit — the destitute end of the theme.
pub(super) const ANCIENT_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Veined white marble — columns, statuary, dressed temple facing. The
/// high-status surface that holds the fallback theme's material bar up.
pub(super) fn marble(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Marble(SovereignMarbleConfig {
            color_base: Fp3(color),
            color_vein: Fp3([color[0] * 0.55, color[1] * 0.52, color[2] * 0.5]),
            vein_frequency: Fp64(3.0),
            scale: Fp64(2.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Coursed sandstone ashlar — colonnade stylobates, bathhouse walls,
/// weathered ruins. Warm dressed blocks with a pale mortar line.
pub(super) fn sandstone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 0.78, color[1] * 0.76, color[2] * 0.7]),
            rows: 4,
            cols: 3,
            chisel_depth: Fp64(0.35),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sun-baked adobe / mudbrick — the poor hut's walls, rendered rough and
/// warm.
pub(super) fn adobe(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.98),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.7, color[1] * 0.66, color[2] * 0.6]),
            roughness: Fp64(0.55),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Fired terracotta — amphorae and urns. A smooth warm ceramic.
pub(super) fn terracotta(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

/// Patinated bronze — the brazier basket, statue cores, fittings. Polished
/// metal gone green with age.
pub(super) fn bronze(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.24, 0.42, 0.34]),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.35),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Marble + sandstone + bronze palette.
pub(super) const MARBLE_WHITE: [f32; 3] = [0.90, 0.88, 0.83];
pub(super) const SANDSTONE_GOLD: [f32; 3] = [0.74, 0.62, 0.42];
pub(super) const SANDSTONE_WEATHERED: [f32; 3] = [0.60, 0.51, 0.36];
pub(super) const ADOBE_TAN: [f32; 3] = [0.72, 0.56, 0.38];
pub(super) const TERRACOTTA: [f32; 3] = [0.62, 0.32, 0.20];
pub(super) const BRONZE_GREEN: [f32; 3] = [0.42, 0.40, 0.22];
pub(super) const STONE_VOID: [f32; 3] = [0.05, 0.05, 0.06];

/// Warm ember light for the brazier coals.
pub(super) const EMBER_ORANGE: [f32; 3] = [1.0, 0.5, 0.16];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The two poor (mudbrick) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 2] =
            [&mudbrick_hut::MudbrickHut, &ruined_wall::RuinedWall];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The brazier is the kit's firelit element — it must keep its emissive
    /// coals so escalation's broken-emissive ruin pass has something to snuff.
    #[test]
    fn brazier_keeps_its_embers() {
        assert!(
            crate::catalogue::items::util::has_emissive(&brazier::Brazier.build("")),
            "brazier lost its emissive embers"
        );
    }
}
