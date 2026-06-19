//! Solarpunk-theme catalogue structures — a verdant optimistic eco-quarter of
//! glass domes, green roofs and clean energy.
//!
//! Two prosperity registers share one solar-green identity: the established
//! ([`SOLAR_BAND`]) kit (a biodome, a green-roof pavilion, a wind turbine, a
//! vertical farm, solar panels, a veggie planter, a water channel, a solar
//! lamp and a beehive) and the destitute ([`SOLAR_POOR`]) grassroots kit (a
//! cob roundhouse, a poly-tunnel, a compost heap).
//!
//! Surfaces use the real procedural generators rather than flat colour: clean
//! lit [`glass`] domes and glazing, white brushed [`steel`] frames, warm
//! [`timber`] posts, pale [`concrete`] rings, matte [`foliage`] living roofs
//! and crops, glossy dark [`pv`] panels and translucent [`water`]. The dome
//! glows softly and the vertical farm's grow-lights shine over a clean-air
//! and birdsong bed from [`fx`]. The theme's fresh-green accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod beehive;
pub mod biodome;
pub mod green_pavilion;
pub mod solar_lamp;
pub mod solar_panel;
pub mod veggie_planter;
pub mod vertical_farm;
pub mod water_channel;
pub mod wind_turbine;
// Poor (grassroots) variants — the prosperity-Poor end of the theme.
pub mod cob_roundhouse;
pub mod compost_heap;
pub mod poly_tunnel;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignConcreteConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPlankConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established eco-quarter — glass domes and
/// clean energy read as a Modest-to-Rich community. The poor end of the theme
/// is the separate grassroots kit ([`cob_roundhouse`], …), tagged `Poor`, so
/// a destitute solarpunk room grows the makeshift commune instead.
pub(super) const SOLAR_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the grassroots kit — the destitute end of the theme,
/// never picked for a modest or affluent solarpunk room.
pub(super) const SOLAR_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Clean lit glass — the biodome, the greenhouse glazing, the vertical farm.
/// A faint inner glow (`glow`) so the panes read as lit and alive.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.12),
        metallic: Fp(0.3),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 3,
            panes_y: 3,
            glass_opacity: Fp64(0.35),
            grime_level: Fp64(0.04),
            color_frame: Fp3([0.86, 0.88, 0.84]),
            ..Default::default()
        }),
    }
}

/// White brushed steel — turbine tower and blades, pavilion posts, frames.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.8),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.34, 0.22, 0.12]),
            roughness: Fp64(0.35),
            metallic: Fp(0.8),
            rust_level: Fp64(0.02),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Warm timber — pavilion frames, planter boxes, trellises, channels.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.18]),
            color_wood_dark: Fp3([color[0] * 0.62, color[1] * 0.6, color[2] * 0.56]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.18),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Pale eco-concrete — biodome ring, vertical-farm core, footings.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Matte greenery — living roofs, planted soil, crops, hedges. A soft
/// non-glossy green with no procedural texture.
pub(super) fn foliage(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Glossy dark photovoltaic panel — solar arrays and panel roofs.
pub(super) fn pv(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.12),
        metallic: Fp(0.6),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.1, 0.12, 0.2]),
            seam_count: Fp64(6.0),
            roughness: Fp64(0.12),
            metallic: Fp(0.6),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Translucent water — the rill channels and pond surfaces. A smooth
/// blue-green with no procedural texture.
pub(super) fn water(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.1),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Glass + structure palette.
pub(super) const GLASS_CLEAN: [f32; 3] = [0.56, 0.74, 0.70];
pub(super) const STEEL_WHITE: [f32; 3] = [0.86, 0.87, 0.88];
pub(super) const STEEL_GREY: [f32; 3] = [0.58, 0.60, 0.62];
pub(super) const TIMBER_WARM: [f32; 3] = [0.55, 0.42, 0.26];
pub(super) const CONCRETE_PALE: [f32; 3] = [0.74, 0.73, 0.70];

// Greenery + utility palette.
pub(super) const LEAF_GREEN: [f32; 3] = [0.26, 0.46, 0.22];
pub(super) const MOSS_GREEN: [f32; 3] = [0.32, 0.44, 0.22];
pub(super) const CROP_GREEN: [f32; 3] = [0.40, 0.56, 0.24];
pub(super) const PV_BLUE: [f32; 3] = [0.10, 0.14, 0.30];
pub(super) const WATER_BLUE: [f32; 3] = [0.32, 0.56, 0.62];
pub(super) const COB_EARTH: [f32; 3] = [0.66, 0.52, 0.36];

// Emissive trim colours.
pub(super) const DOME_GLOW: [f32; 3] = [0.72, 1.0, 0.74];
pub(super) const GROW_PINK: [f32; 3] = [1.0, 0.42, 0.82];
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.90, 0.66];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (grassroots) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &cob_roundhouse::CobRoundhouse,
            &poly_tunnel::PolyTunnel,
            &compost_heap::CompostHeap,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The biodome is the kit's lit hero — it must keep its emissive dome and
    /// interior glow so escalation's broken-emissive ruin pass has light to
    /// snuff.
    #[test]
    fn biodome_keeps_its_glow() {
        assert!(
            crate::catalogue::items::util::has_emissive(&biodome::Biodome.build("")),
            "biodome lost its emissive dome / interior glow"
        );
    }
}
