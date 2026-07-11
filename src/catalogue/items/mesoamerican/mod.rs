//! Mesoamerican-theme catalogue structures — a painted-limestone temple
//! city in the jungle.
//!
//! Two prosperity registers share one identity: the established
//! ([`MESO_BAND`]) monumental kit (step pyramid, ball court, shrine, stela,
//! skull rack, idol, fire bowl, calendar stone) and the destitute
//! ([`MESO_POOR`]) commoner kit (adobe hut, maize granary, clay pots).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! dressed [`limestone`] ashlar, [`painted`] red stucco, rough [`cobble`]
//! stone, green [`jade`] and black [`obsidian`] marble, [`gold`] metal,
//! [`timber`] plank, and golden [`thatch`]. The temple fire, the fire
//! bowls, and ritual incense come alive with flame, ember, copal-smoke and
//! spatial audio from [`fx`] (a fire crackle and a slow ritual drum). The
//! theme's warm jungle-gold accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod ball_court;
pub mod calendar_stone;
pub mod fire_bowl;
pub mod gateway;
pub mod idol;
pub mod shrine;
pub mod skull_rack;
pub mod stela;
pub mod step_pyramid;
// Poor (commoner) variants — the prosperity-Poor end of the theme.
pub mod adobe_hut;
pub mod clay_pots;
pub mod maize_granary;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAshlarConfig, SovereignCobblestoneConfig, SovereignMarbleConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignStuccoConfig,
    SovereignTextureConfig, SovereignThatchConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established monumental kit — painted
/// pyramids and stone gardens read as a Modest-to-Rich city. The poor end
/// is the separate commoner kit ([`adobe_hut`], …), tagged `Poor`.
pub(super) const MESO_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the commoner kit — the destitute end of the theme,
/// never picked for a modest or affluent room.
pub(super) const MESO_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Dressed limestone ashlar — the pale cut-block body of pyramids, courts,
/// and platforms.
pub(super) fn limestone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 0.82, color[1] * 0.8, color[2] * 0.72]),
            rows: 3,
            cols: 4,
            chisel_depth: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted lime stucco — the vivid red and cream rendering over a temple's
/// stonework.
pub(super) fn painted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.75, color[1] * 0.7, color[2] * 0.68]),
            scale: Fp64(6.0),
            roughness: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rough cobble — fieldstone fill, rubble cores, humble footings.
pub(super) fn cobble(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.45, color[2] * 0.38]),
            roundness: Fp64(1.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Polished jade — green marble for idol inlays, masks, and ornaments.
pub(super) fn jade(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.2),
        metallic: Fp(0.1),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Marble(SovereignMarbleConfig {
            color_base: Fp3(color),
            color_vein: Fp3([color[0] * 0.5, color[1] * 0.7, color[2] * 0.5]),
            vein_frequency: Fp64(4.0),
            roughness: Fp64(0.15),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Black obsidian — glassy volcanic stone for sacrificial blades and altar
/// tops.
pub(super) fn obsidian(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.1),
        metallic: Fp(0.3),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Marble(SovereignMarbleConfig {
            color_base: Fp3(color),
            color_vein: Fp3([
                color[0] * 2.0 + 0.05,
                color[1] * 2.0 + 0.05,
                color[2] * 2.0 + 0.08,
            ]),
            vein_frequency: Fp64(2.0),
            roughness: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Beaten gold — sun discs, finials, and ornaments. Polished metal, no rust.
pub(super) fn gold(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.95),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.3, 0.1]),
            roughness: Fp64(0.3),
            metallic: Fp(0.95),
            rust_level: Fp64(0.04),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Oiled timber — lintels, poles, the skull-rack frame.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.25, color[1] * 1.25, color[2] * 1.2]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.55]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Golden palm thatch — the roof of a temple cella, shrine, or adobe hut.
pub(super) fn thatch(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Thatch(SovereignThatchConfig {
            color_straw: Fp3(color),
            color_shadow: Fp3([color[0] * 0.32, color[1] * 0.30, color[2] * 0.18]),
            density: Fp64(15.0),
            layer_count: Fp64(9.0),
            layer_shadow: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Stone + paint palette.
pub(super) const LIMESTONE_PALE: [f32; 3] = [0.74, 0.70, 0.58];
pub(super) const STUCCO_RED: [f32; 3] = [0.62, 0.20, 0.12];
pub(super) const STUCCO_CREAM: [f32; 3] = [0.82, 0.78, 0.66];
pub(super) const STONE_GREY: [f32; 3] = [0.55, 0.53, 0.48];
pub(super) const JADE_GREEN: [f32; 3] = [0.20, 0.52, 0.36];
pub(super) const GOLD_WARM: [f32; 3] = [0.85, 0.68, 0.22];
pub(super) const TIMBER_BROWN: [f32; 3] = [0.36, 0.24, 0.14];
pub(super) const OBSIDIAN_BLACK: [f32; 3] = [0.07, 0.07, 0.09];
pub(super) const BONE_WHITE: [f32; 3] = [0.86, 0.84, 0.74];
pub(super) const ADOBE_TAN: [f32; 3] = [0.66, 0.48, 0.32];
pub(super) const THATCH_STRAW: [f32; 3] = [0.58, 0.48, 0.26];
pub(super) const CLAY_TERRACOTTA: [f32; 3] = [0.62, 0.34, 0.20];

/// Warm sacrificial firelight — the temple fire and the fire bowls.
pub(super) const FIRE_ORANGE: [f32; 3] = [1.0, 0.55, 0.16];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (commoner) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &adobe_hut::AdobeHut,
            &maize_granary::MaizeGranary,
            &clay_pots::ClayPots,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The fire bowl is the kit's lit hero — it must keep its emissive flame
    /// so escalation's broken-emissive ruin pass has something to snuff.
    #[test]
    fn fire_bowl_keeps_its_flame() {
        assert!(
            crate::catalogue::items::util::has_emissive(&fire_bowl::FireBowl.build("")),
            "fire bowl lost its emissive fire"
        );
    }
}
