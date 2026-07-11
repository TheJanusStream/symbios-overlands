//! Coastal-Resort-theme catalogue structures — a sun-bleached seaside
//! holiday strip on the bright clear-sky coast.
//!
//! Two prosperity registers share one seaside identity: the established
//! ([`RESORT_BAND`]) holiday kit (grand hotel, pier, beach house,
//! boardwalk shops, lifeguard tower, parasols, deck chairs, a dinghy and
//! a channel buoy) and the destitute ([`RESORT_POOR`]) fishing-hamlet kit
//! (a driftwood shack, a bait stand, a stack of crab pots).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! whitewashed [`stucco`] hotel and bungalow walls, sun-greyed [`plank`]
//! decking and clapboard, striped [`canvas`] awnings and parasols, [`glass`]
//! balcony fronts, brushed [`steel`] railings and pilings, board-formed
//! [`concrete`] piers, glossy [`enamel`] buoys, and a [`sand`] apron under
//! the beach furniture. The hotel's lit sign and lobby glow, and the pier
//! breathes a fine sea spray over a slow surf wash from [`fx`]. The theme's
//! bright clear-sky accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod beach_house;
pub mod beach_umbrella;
pub mod boardwalk_shops;
pub mod buoy;
pub mod deck_chair;
pub mod dinghy;
pub mod gateway;
pub mod grand_hotel;
pub mod lifeguard_tower;
pub mod resort_pier;
// Poor (fishing-hamlet) variants — the prosperity-Poor end of the theme.
pub mod bait_stand;
pub mod crab_traps;
pub mod fishing_shack;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignConcreteConfig, SovereignFabricConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignSandConfig, SovereignStuccoConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established holiday kit — whitewashed
/// hotels and varnished piers read as a Modest-to-Rich resort. The poor end
/// of the theme is the separate fishing-hamlet kit ([`fishing_shack`], …),
/// tagged `Poor`, so a destitute coastal room grows the driftwood hamlet.
pub(super) const RESORT_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the fishing-hamlet kit — the destitute end of the
/// theme, never picked for a modest or affluent resort room.
pub(super) const RESORT_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Whitewashed stucco — the rendered walls of the hotel, the bungalows and
/// the shop fronts. Bright Mediterranean plaster, not a flat painted slab.
pub(super) fn stucco(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.78, color[1] * 0.76, color[2] * 0.72]),
            scale: Fp64(7.0),
            roughness: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sun-greyed timber plank — pier decking, boardwalks, clapboard walls,
/// boat hulls. Weathered grain with a little knotting so it reads as wood.
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
            knot_density: Fp64(0.22),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Striped woven canvas — parasols, deck-chair seats, awnings, flags, fishing
/// nets. A two-tone weave that reads as beach fabric.
pub(super) fn canvas(warp: [f32; 3], weft: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(warp),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            color_warp: Fp3(warp),
            color_weft: Fp3(weft),
            thread_count: Fp64(16.0),
            fuzz: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Balcony / shopfront glass — clean lit panes with a faint inner glow
/// (`glow` sets the lit-window bloom) so a facade reads as glowing glass
/// rather than a black hole.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.5),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 3,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.08),
            color_frame: Fp3([0.93, 0.92, 0.88]),
            ..Default::default()
        }),
    }
}

/// Brushed structural steel — railings, pier pilings, tower frames, poles.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.34, 0.22, 0.12]),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Glossy painted enamel — buoy bodies, lifebelt rings, hull trim. Smooth
/// marine paint with no rust.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.4),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.18, 0.1]),
            seam_count: Fp64(1.0),
            roughness: Fp64(0.25),
            metallic: Fp(0.4),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete — pier pilings and the promenade plinth.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rippled beach sand — the apron disc under the parasols and deck chairs.
pub(super) fn sand(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(1.0),
        metallic: Fp(0.0),
        uv_scale: Fp(2.5),
        texture: SovereignTextureConfig::Sand(SovereignSandConfig {
            color_crest: Fp3(color),
            color_trough: Fp3([color[0] * 0.78, color[1] * 0.76, color[2] * 0.68]),
            ripple_count: Fp64(8.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Wet pool water — a glassy turquoise surface with a faint inner glow so the
/// blue reads under the bright sky rather than going matte grey. The civic
/// fountain idiom (low roughness + emission ~0.5); used for the grand hotel's
/// resort pool.
pub(super) fn water(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(0.5),
        roughness: Fp(0.1),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::None,
    }
}

// Stucco + decking palette.
pub(super) const STUCCO_WHITE: [f32; 3] = [0.93, 0.91, 0.86];
pub(super) const STUCCO_SAND: [f32; 3] = [0.90, 0.82, 0.66];
pub(super) const DECK_PALE: [f32; 3] = [0.68, 0.60, 0.44];
pub(super) const DECK_WOOD: [f32; 3] = [0.52, 0.40, 0.26];
pub(super) const DRIFT_GREY: [f32; 3] = [0.56, 0.55, 0.50];
pub(super) const STEEL_GREY: [f32; 3] = [0.56, 0.58, 0.61];
pub(super) const PILING_GREY: [f32; 3] = [0.60, 0.59, 0.57];
pub(super) const SAND_TAN: [f32; 3] = [0.86, 0.76, 0.55];

// Glass + awning colours.
pub(super) const GLASS_AQUA: [f32; 3] = [0.46, 0.66, 0.70];
pub(super) const AWNING_RED: [f32; 3] = [0.76, 0.20, 0.18];
pub(super) const AWNING_WHITE: [f32; 3] = [0.92, 0.90, 0.86];
pub(super) const AWNING_TEAL: [f32; 3] = [0.16, 0.52, 0.54];

// Marine paint colours.
pub(super) const BUOY_RED: [f32; 3] = [0.80, 0.16, 0.13];
pub(super) const HULL_BLUE: [f32; 3] = [0.20, 0.34, 0.52];

/// Bright turquoise of the resort pool — a holiday-postcard aqua.
pub(super) const POOL_AQUA: [f32; 3] = [0.18, 0.58, 0.68];

/// Warm self-lit gold for the hotel's lobby glow and the boardwalk lamps.
pub(super) const SIGN_GOLD: [f32; 3] = [1.0, 0.84, 0.46];
/// Deep-saturated amber for the broad rooftop sign band — a pale gold at high
/// strength blooms to a near-white blank, so the big lit bars hold this richer
/// hue instead. [`SIGN_GOLD`] stays for the small lobby/lamp glows behind glass.
pub(super) const SIGN_AMBER: [f32; 3] = [1.0, 0.58, 0.16];
/// Warm lamp for the lifeguard tower's eave light.
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.88, 0.6];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (fishing-hamlet) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &fishing_shack::FishingShack,
            &bait_stand::BaitStand,
            &crab_traps::CrabTraps,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The grand hotel is the kit's lit hero — it must keep its emissive
    /// sign and lobby so escalation's broken-emissive ruin pass has lights
    /// to snuff.
    #[test]
    fn hotel_keeps_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&grand_hotel::GrandHotel.build("")),
            "grand hotel lost its emissive sign / lobby glow"
        );
    }
}
