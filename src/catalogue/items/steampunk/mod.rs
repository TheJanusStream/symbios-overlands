//! Steampunk-theme catalogue structures — a brass-and-iron works wreathed in
//! amber steam.
//!
//! Two prosperity registers share one industrial-Victorian identity: the
//! established ([`STEAM_BAND`]) works (a cog tower, an airship dock, a
//! foundry, a pump house, pipework, a pressure tank, a gear pile, a gas lamp
//! and a coal hopper) and the destitute ([`STEAM_POOR`]) soot-yard kit (a
//! tinkerer's shack, a scrap boiler, a heap of cog scrap).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! polished [`brass`] fittings, riveted dark [`iron`] plate, orange [`copper`]
//! pipework, sooty [`brick`] halls, rusting [`corrugated`] roofs, lit amber
//! [`glass`] gauges and weathered [`plank`]. The cog tower's clock and the
//! foundry's furnace glow, venting steam and soot over an engine-chug and
//! boiler-hiss bed from [`fx`]. The theme's amber-smog accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod airship_dock;
pub mod coal_hopper;
pub mod cog_tower;
pub mod foundry;
pub mod gas_lamp;
pub mod gear_pile;
pub mod pipework;
pub mod pressure_tank;
pub mod pump_house;
// Poor (soot-yard) variants — the prosperity-Poor end of the theme.
pub mod cog_scrap;
pub mod scrap_boiler;
pub mod tinkerers_shack;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignBrickConfig, SovereignCorrugatedConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established works — a running foundry reads
/// as a Modest-to-Rich concern. The poor end of the theme is the separate
/// soot-yard kit ([`tinkerers_shack`], …), tagged `Poor`, so a destitute
/// steampunk room grows the scrap yard instead.
pub(super) const STEAM_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the soot-yard kit — the destitute end of the theme,
/// never picked for a modest or affluent steampunk room.
pub(super) const STEAM_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Polished brass — gauges, bands, cog teeth, lamp fittings, valve wheels.
pub(super) fn brass(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.9),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.3, 0.12]),
            roughness: Fp64(0.3),
            metallic: Fp(0.9),
            rust_level: Fp64(0.05),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Riveted dark iron — structural plate, frames, beams, pipes.
pub(super) fn iron(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.8),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.36, 0.20, 0.10]),
            roughness: Fp64(0.55),
            metallic: Fp(0.8),
            rust_level: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Orange copper — pipe runs, boiler vats, still coils.
pub(super) fn copper(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.30, 0.45, 0.34]),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.18),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sooty brick — foundry, pump house and tower base walls.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.34, 0.30, 0.28]),
            scale: Fp64(5.0),
            cell_variance: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusting corrugated iron — shack walls, lean-to roofs, ducting.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.35),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit amber gauge / window glass — a faint inner glow (`glow`) so the dial
/// reads as lit rather than dark.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.2),
        metallic: Fp(0.3),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.2),
            color_frame: Fp3([0.4, 0.32, 0.16]),
            ..Default::default()
        }),
    }
}

/// Weathered plank — gangways, crates, the shack's patched cladding.
pub(super) fn plank(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.18]),
            color_wood_dark: Fp3([color[0] * 0.62, color[1] * 0.6, color[2] * 0.56]),
            plank_count: Fp64(6.0),
            knot_density: Fp64(0.28),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Metal + masonry palette.
pub(super) const BRASS: [f32; 3] = [0.78, 0.62, 0.28];
pub(super) const IRON_DARK: [f32; 3] = [0.22, 0.21, 0.20];
pub(super) const COPPER_ORANGE: [f32; 3] = [0.66, 0.42, 0.24];
pub(super) const BRICK_SOOT: [f32; 3] = [0.42, 0.28, 0.24];
pub(super) const CORRUGATED_RUST: [f32; 3] = [0.50, 0.38, 0.26];
pub(super) const WOOD_BROWN: [f32; 3] = [0.42, 0.30, 0.18];
pub(super) const GLASS_AMBER: [f32; 3] = [0.70, 0.52, 0.26];

// Emissive trim colours.
pub(super) const FURNACE_ORANGE: [f32; 3] = [1.0, 0.50, 0.16];
pub(super) const GAUGE_AMBER: [f32; 3] = [1.0, 0.80, 0.42];
pub(super) const LAMP_GAS: [f32; 3] = [1.0, 0.86, 0.55];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (soot-yard) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &tinkerers_shack::TinkerersShack,
            &scrap_boiler::ScrapBoiler,
            &cog_scrap::CogScrap,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The cog tower is the kit's lit hero — it must keep its emissive clock
    /// and furnace glow so escalation's broken-emissive ruin pass has lights
    /// to snuff.
    #[test]
    fn cog_tower_keeps_its_glow() {
        assert!(
            crate::catalogue::items::util::has_emissive(&cog_tower::CogTower.build("")),
            "cog tower lost its emissive clock / furnace glow"
        );
    }
}
