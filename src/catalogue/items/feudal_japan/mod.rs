//! Feudal-Japan-theme catalogue structures — a temple-and-garden
//! settlement under drifting cherry blossom.
//!
//! Two prosperity registers share one identity: the established
//! ([`FEUDAL_BAND`]) lacquered-timber kit (pagoda, torii gate, tea house,
//! dojo, stone lantern, koi pond, bamboo fence, bonsai) and the destitute
//! ([`FEUDAL_POOR`]) farmstead kit (minka farmhouse, raised rice shed,
//! straw bales).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! vermilion [`lacquer`] and plain [`timber`] plank, dark [`roof_tile`]
//! shingle, white [`plaster`] stucco, [`paper`] shoji cloth, dressed
//! [`stone`] ashlar and rough [`rough_stone`] cobble, still [`water`],
//! golden [`bronze`] metal, and golden [`thatch`].
//! The stone lantern, the temple bell, and a garden basin come alive with
//! petal-fall, incense, and spatial audio from [`fx`]. The theme's blossom
//! accent (a warm rose haze with drifting petals) lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod bamboo_fence;
pub mod bonsai;
pub mod dojo;
pub mod koi_pond;
pub mod pagoda;
pub mod stone_lantern;
pub mod tea_house;
pub mod torii_gate;
// Poor (farmstead) variants — the prosperity-Poor end of the theme.
pub mod minka;
pub mod rice_shed;
pub mod straw_bales;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAshlarConfig, SovereignCobblestoneConfig, SovereignFabricConfig,
    SovereignIceConfig, SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig,
    SovereignShingleConfig, SovereignStuccoConfig, SovereignTextureConfig, SovereignThatchConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established temple kit — lacquered halls
/// and stone gardens read as a Modest-to-Rich settlement. The poor end is
/// the separate farmstead kit ([`minka`], …), tagged `Poor`.
pub(super) const FEUDAL_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the farmstead kit — the destitute end of the theme,
/// never picked for a modest or affluent room.
pub(super) const FEUDAL_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Vermilion lacquered timber — the glossy red of pagoda columns and torii
/// gates. Plank grain under a low-roughness sheen so it reads as lacquer,
/// not flat paint.
pub(super) fn lacquer(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.15),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.15, color[1] * 1.1, color[2] * 1.1]),
            color_wood_dark: Fp3([color[0] * 0.7, color[1] * 0.5, color[2] * 0.5]),
            plank_count: Fp64(3.0),
            knot_density: Fp64(0.05),
            grain_warp: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Plain oiled timber — posts, beams, verandas, frames.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.25, color[1] * 1.25, color[2] * 1.2]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.55]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dark ceramic roof tile — the curved kawara of a pagoda or hall roof.
pub(super) fn roof_tile(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.1),
        uv_scale: Fp(2.5),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([color[0] * 0.5, color[1] * 0.5, color[2] * 0.55]),
            scale: Fp64(7.0),
            shape_profile: Fp64(0.75),
            overlap: Fp64(0.5),
            moss_level: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// White lime plaster — the daub walls between a hall's timber frame.
pub(super) fn plaster(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.82, color[1] * 0.82, color[2] * 0.8]),
            scale: Fp64(7.0),
            roughness: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Shoji paper — the translucent screen walls and lantern panes. Woven
/// fabric weave at a fine thread count so it reads as paper, not plank.
pub(super) fn paper(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            color_warp: Fp3(color),
            color_weft: Fp3([color[0] * 0.92, color[1] * 0.92, color[2] * 0.9]),
            thread_count: Fp64(40.0),
            thread_width: Fp64(0.95),
            fuzz: Fp64(0.15),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dressed ashlar stone — lantern stacks, pagoda plinths, pond rims.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.15]),
            rows: 2,
            cols: 2,
            chisel_depth: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rough fieldstone cobble — the boulder rim of a koi pond, garden edging.
pub(super) fn rough_stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.45, color[2] * 0.4]),
            roundness: Fp64(1.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Still dark pond water — a smooth blue sheet with faint reflective veins.
pub(super) fn water(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.08),
        metallic: Fp(0.1),
        emission_color: Fp3(color),
        emission_strength: Fp(0.2),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ice(SovereignIceConfig {
            color_ice: Fp3(color),
            color_crack: Fp3([color[0] * 1.5, color[1] * 1.5, color[2] * 1.4]),
            crack_density: Fp64(2.0),
            frost_level: Fp64(0.0),
            ..Default::default()
        }),
    }
}

/// Golden bronze — the temple bell, lantern caps, finial rings. Polished
/// metal with no rust.
pub(super) fn bronze(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.9),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.30, 0.34, 0.18]),
            roughness: Fp64(0.35),
            metallic: Fp(0.9),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Golden straw thatch — the steep roof of a minka farmhouse and rice shed.
pub(super) fn thatch(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Thatch(SovereignThatchConfig {
            color_straw: Fp3(color),
            color_shadow: Fp3([color[0] * 0.32, color[1] * 0.30, color[2] * 0.18]),
            density: Fp64(14.0),
            layer_count: Fp64(9.0),
            layer_shadow: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Lacquer + timber palette.
pub(super) const LACQUER_RED: [f32; 3] = [0.60, 0.10, 0.09];
pub(super) const TIMBER_BROWN: [f32; 3] = [0.34, 0.22, 0.13];
pub(super) const TIMBER_DARK: [f32; 3] = [0.22, 0.15, 0.10];
pub(super) const TILE_SLATE: [f32; 3] = [0.20, 0.23, 0.27];
pub(super) const PLASTER_WHITE: [f32; 3] = [0.86, 0.84, 0.78];
pub(super) const PAPER_CREAM: [f32; 3] = [0.84, 0.82, 0.72];
pub(super) const STONE_GREY: [f32; 3] = [0.52, 0.51, 0.48];
pub(super) const GOLD: [f32; 3] = [0.80, 0.66, 0.22];
pub(super) const WATER_BLUE: [f32; 3] = [0.14, 0.28, 0.32];
pub(super) const KOI_ORANGE: [f32; 3] = [0.92, 0.45, 0.14];
pub(super) const BAMBOO_TAN: [f32; 3] = [0.66, 0.66, 0.38];
pub(super) const THATCH_STRAW: [f32; 3] = [0.60, 0.50, 0.26];

/// Warm paper-lantern light — the glow inside a stone lantern's light box.
pub(super) const LANTERN_GLOW: [f32; 3] = [1.0, 0.82, 0.46];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (farmstead) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &minka::Minka,
            &rice_shed::RiceShed,
            &straw_bales::StrawBales,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The stone lantern is the kit's lit hero — it must keep its emissive
    /// light box so escalation's broken-emissive ruin pass has something to
    /// snuff.
    #[test]
    fn lantern_keeps_its_light() {
        assert!(
            crate::catalogue::items::util::has_emissive(&stone_lantern::StoneLantern.build("")),
            "stone lantern lost its emissive light box"
        );
    }
}
