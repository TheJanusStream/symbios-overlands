//! Rural / Farmland-theme catalogue structures — a working farmstead under
//! a golden-hour sky.
//!
//! Two prosperity registers share one identity: the established
//! ([`FARM_BAND`]) farmstead kit (red barn, farmhouse, grain silo, windmill,
//! greenhouse, tractor, hay bales, scarecrow, rail fence) and the destitute
//! ([`FARM_POOR`]) hardscrabble kit (homestead shack, pole barn, farm junk).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! red [`barn_board`] and pale [`clapboard`] plank, weathered grey [`weathered`]
//! timber, ribbed [`metal_roof`] and [`silo_metal`] steel, asphalt
//! [`shingle`], [`glass`] panes, smooth painted [`enamel`], and fieldstone
//! [`stone`]. A lit barn window glows, the farmhouse chimneys smoke, chaff
//! drifts off the hayloft and the windmill creaks over crickets — all from
//! [`fx`]. The theme's golden-hour accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod barn;
pub mod farmhouse;
pub mod grain_silo;
pub mod greenhouse;
pub mod hay_bales;
pub mod rail_fence;
pub mod scarecrow;
pub mod tractor;
pub mod windmill;
// Poor (hardscrabble) variants — the prosperity-Poor end of the theme.
pub mod farm_junk;
pub mod homestead_shack;
pub mod pole_barn;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignCobblestoneConfig, SovereignCorrugatedConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignShingleConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established farmstead kit — a painted barn
/// and tidy fields read as a Modest-to-Rich farm. The poor end is the
/// separate hardscrabble kit ([`homestead_shack`], …), tagged `Poor`.
pub(super) const FARM_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the hardscrabble kit — the destitute end of the
/// theme, never picked for a modest or affluent room.
pub(super) const FARM_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Painted barn-board siding — the deep red of the barn, vertical boards
/// with a little grain so it reads as timber, not a flat slab.
pub(super) fn barn_board(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.15, color[1] * 1.1, color[2] * 1.1]),
            color_wood_dark: Fp3([color[0] * 0.65, color[1] * 0.6, color[2] * 0.6]),
            plank_count: Fp64(8.0),
            knot_density: Fp64(0.1),
            grain_warp: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted clapboard — the pale lap siding of the farmhouse.
pub(super) fn clapboard(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.08, color[1] * 1.08, color[2] * 1.08]),
            color_wood_dark: Fp3([color[0] * 0.82, color[1] * 0.82, color[2] * 0.82]),
            plank_count: Fp64(9.0),
            knot_density: Fp64(0.05),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Weathered grey timber — fences, the shack, the pole barn.
pub(super) fn weathered(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.15, color[1] * 1.15, color[2] * 1.15]),
            color_wood_dark: Fp3([color[0] * 0.7, color[1] * 0.7, color[2] * 0.7]),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.35),
            grain_warp: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Ribbed corrugated roofing steel — barn and shed roofs.
pub(super) fn metal_roof(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.7),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.24, 0.12]),
            ridges: Fp64(14.0),
            ridge_depth: Fp64(0.8),
            roughness: Fp64(0.55),
            metallic: Fp(0.7),
            rust_level: Fp64(0.12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Galvanised ribbed steel — the grain silo body.
pub(super) fn silo_metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.45),
        metallic: Fp(0.85),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.28, 0.16]),
            ridges: Fp64(24.0),
            ridge_depth: Fp64(0.6),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.06),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Asphalt-shingle roof — the farmhouse.
pub(super) fn shingle(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.62]),
            scale: Fp64(6.0),
            shape_profile: Fp64(0.2),
            moss_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Greenhouse / window glass (`glow` lights it from within at dusk).
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.2),
        metallic: Fp(0.2),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 3,
            panes_y: 4,
            glass_opacity: Fp64(0.35),
            grime_level: Fp64(0.15),
            color_frame: Fp3([0.7, 0.72, 0.7]),
            ..Default::default()
        }),
    }
}

/// Smooth painted enamel — the tractor, water troughs, windmill fan, vanes.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.5),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.24, 0.12]),
            seam_count: Fp64(1.0),
            roughness: Fp64(0.35),
            metallic: Fp(0.5),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Fieldstone — barn and house foundations.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.45, color[2] * 0.4]),
            roundness: Fp64(1.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Farmstead palette.
pub(super) const BARN_RED: [f32; 3] = [0.52, 0.13, 0.10];
pub(super) const TRIM_WHITE: [f32; 3] = [0.88, 0.86, 0.80];
pub(super) const ROOF_GREY: [f32; 3] = [0.34, 0.34, 0.36];
pub(super) const SILO_STEEL: [f32; 3] = [0.62, 0.64, 0.66];
pub(super) const CLAPBOARD_CREAM: [f32; 3] = [0.84, 0.80, 0.68];
pub(super) const WOOD_GREY: [f32; 3] = [0.50, 0.48, 0.44];
pub(super) const HAY_GOLD: [f32; 3] = [0.72, 0.58, 0.26];
pub(super) const STONE_GREY: [f32; 3] = [0.52, 0.50, 0.46];
pub(super) const TRACTOR_GREEN: [f32; 3] = [0.16, 0.34, 0.16];
pub(super) const TRACTOR_YELLOW: [f32; 3] = [0.80, 0.66, 0.16];
pub(super) const GLASS_TINT: [f32; 3] = [0.55, 0.66, 0.60];

/// Warm lamplight in the barn window.
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.82, 0.48];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (hardscrabble) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &homestead_shack::HomesteadShack,
            &pole_barn::PoleBarn,
            &farm_junk::FarmJunk,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The barn is the kit's lit hero — it must keep its emissive window so
    /// escalation's broken-emissive ruin pass has something to dim.
    #[test]
    fn barn_keeps_its_lamp() {
        assert!(
            crate::catalogue::items::util::has_emissive(&barn::Barn.build("")),
            "barn lost its emissive window"
        );
    }
}
