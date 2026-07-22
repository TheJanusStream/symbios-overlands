//! Wild-West-theme catalogue structures — a dusty frontier boomtown of
//! clapboard false-fronts and weathered timber.
//!
//! Two prosperity registers share one frontier identity: the established
//! ([`FRONTIER_BAND`]) boomtown (a saloon, a water tower, a church, a jail, a
//! general store, a hitching post, a wagon, a split-rail fence and a wind
//! pump) and the destitute ([`FRONTIER_POOR`]) bust kit (a prospector's shack,
//! a boot-hill graves plot, a tumbleweed).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! painted [`clapboard`], fieldstone [`stone`], lit amber [`glass`], dark
//! [`iron`] hardware, rusting [`tin`] roofs and matte [`canvas`]. The saloon's
//! lamps glow over a prairie-wind and windmill-creak bed from [`fx`]. The
//! theme's sun-bleached dust accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod church;
pub mod frontier_fence;
pub mod gateway;
pub mod general_store;
pub mod hitching_post;
pub mod jail;
pub mod saloon;
pub mod wagon;
pub mod water_tower;
pub mod wind_pump;
// Poor (bust) variants — the prosperity-Poor end of the theme.
pub mod boot_hill;
pub mod prospector_shack;
pub mod tumbleweed;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignCobblestoneConfig, SovereignCorrugatedConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig,
    SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the boomtown — a thriving frontier strip reads
/// as a Modest-to-Rich town. The poor end of the theme is the separate bust
/// kit ([`prospector_shack`], …), tagged `Poor`, so a destitute frontier room
/// grows the dried-up claim instead.
pub(super) const FRONTIER_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the bust kit — the destitute end of the theme, never
/// picked for a modest or affluent frontier room.
pub(super) const FRONTIER_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Painted / weathered clapboard — saloon, store and church walls, false
/// fronts, porches, wagons.
pub(super) fn clapboard(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.18]),
            color_wood_dark: Fp3([color[0] * 0.62, color[1] * 0.6, color[2] * 0.56]),
            plank_count: Fp64(7.0),
            knot_density: Fp64(0.25),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Fieldstone — the jail walls and footings.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.46, color[2] * 0.4]),
            roundness: Fp64(1.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit amber glass — the saloon and store windows after dark. A warm inner
/// glow (`glow`) so the panes read as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.2),
        metallic: Fp(0.2),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 4,
            panes_y: 3,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.15),
            color_frame: Fp3([0.36, 0.26, 0.16]),
            ..Default::default()
        }),
    }
}

/// Dark iron — jail bars, hinges, wagon tyres, the wind-pump head.
pub(super) fn iron(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.8),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.34, 0.20, 0.10]),
            roughness: Fp64(0.55),
            metallic: Fp(0.8),
            rust_level: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusting tin — the water-tank bands, roofs, the wind-pump vane.
pub(super) fn tin(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.6),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Matte canvas / dirt — wagon covers, tumbleweed, dust, dry ground. A plain
/// surface with no procedural texture.
pub(super) fn canvas(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Clapboard + structure palette.
pub(super) const CLAP_RED: [f32; 3] = [0.55, 0.22, 0.18];
pub(super) const CLAP_WHITE: [f32; 3] = [0.84, 0.82, 0.76];
pub(super) const CLAP_TAN: [f32; 3] = [0.60, 0.48, 0.32];
pub(super) const WOOD_RAW: [f32; 3] = [0.48, 0.36, 0.22];
pub(super) const STONE_TAN: [f32; 3] = [0.60, 0.54, 0.44];
pub(super) const TIN_GREY: [f32; 3] = [0.55, 0.54, 0.52];
pub(super) const IRON_DARK: [f32; 3] = [0.18, 0.18, 0.20];
pub(super) const CANVAS_TAN: [f32; 3] = [0.78, 0.72, 0.58];
pub(super) const DUST_TAN: [f32; 3] = [0.66, 0.56, 0.40];

// Glass colour.
pub(super) const GLASS_WARM: [f32; 3] = [0.62, 0.50, 0.30];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (bust) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &prospector_shack::ProspectorShack,
            &boot_hill::BootHill,
            &tumbleweed::Tumbleweed,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The saloon is the kit's lit hero — it must keep its emissive windows
    /// so escalation's broken-emissive ruin pass has lamps to snuff.
    #[test]
    fn saloon_keeps_its_lamps() {
        assert!(
            crate::catalogue::items::util::has_emissive(&saloon::Saloon.build("")),
            "saloon lost its emissive windows"
        );
    }
}
