//! Roadside / Highway-theme catalogue structures — a sun-faded strip of
//! Americana along the interstate shoulder.
//!
//! Two prosperity registers share one blacktop identity: the established
//! ([`ROADSIDE_BAND`]) franchise strip (gas station, chrome diner, motel,
//! billboard, fuel pumps, a guide sign, a cone, a vending machine and a
//! guardrail) and the destitute ([`ROADSIDE_POOR`]) busted-shoulder kit (a
//! rickety produce stand, a boarded-up shack, a heap of oil drums).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! glossy [`enamel`] pump and sign panels, polished [`chrome`] diner trim,
//! brushed structural [`steel`] frames, board-formed [`concrete`] footings,
//! cracked [`asphalt`] forecourts, painted [`brick`] walls, lit [`glass`]
//! storefronts, rusting [`corrugated`] roofs and weathered [`plank`]. The
//! gas-station canopy and pylon sign glow over a buzzing-neon and distant-
//! highway bed from [`fx`]. The theme's dusty sodium-amber accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod billboard;
pub mod fuel_pump;
pub mod gas_station;
pub mod guardrail;
pub mod motel;
pub mod road_sign;
pub mod roadside_diner;
pub mod traffic_cone;
pub mod vending_machine;
// Poor (busted-shoulder) variants — the prosperity-Poor end of the theme.
pub mod boarded_shack;
pub mod oil_drums;
pub mod produce_stand;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAsphaltConfig, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPlankConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established strip — a working franchise
/// reads as a Modest-to-Rich stop. The poor end of the theme is the separate
/// busted-shoulder kit ([`produce_stand`], …), tagged `Poor`, so a destitute
/// roadside room grows the broke-down hamlet instead.
pub(super) const ROADSIDE_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the busted-shoulder kit — the destitute end of the
/// theme, never picked for a modest or affluent roadside room.
pub(super) const ROADSIDE_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Glossy painted enamel — pump bodies, sign panels, the diner's coloured
/// skirt, the vending machine, the cone, the drums. Smooth automotive paint.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.5),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.18, 0.1]),
            seam_count: Fp64(1.0),
            roughness: Fp64(0.25),
            metallic: Fp(0.5),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Polished chrome — diner trim, canopy fascia, pump nozzles. Bright,
/// near-mirror metal.
pub(super) fn chrome(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.18),
        metallic: Fp(0.95),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.2, 0.12]),
            roughness: Fp64(0.18),
            metallic: Fp(0.95),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Brushed structural steel — billboard A-frames, sign posts, canopy
/// columns, guardrails. Honest galvanised metal with a little rust.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.45),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.34, 0.22, 0.12]),
            roughness: Fp64(0.45),
            metallic: Fp(0.85),
            rust_level: Fp64(0.12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete — forecourt curbs, footings, motel plinths.
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

/// Cracked, oil-stained asphalt — the forecourt pad and the lot.
pub(super) fn asphalt(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Asphalt(SovereignAsphaltConfig {
            color_base: Fp3(color),
            color_aggregate: Fp3([0.35, 0.33, 0.30]),
            stain_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted brick — diner and motel walls.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.74, 0.72, 0.68]),
            scale: Fp64(5.0),
            cell_variance: Fp64(0.18),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit storefront glass — diner windows, motel rooms, the kiosk. A faint
/// inner glow (`glow`) so the panes read as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.5),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 4,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.12),
            color_frame: Fp3([0.2, 0.21, 0.24]),
            ..Default::default()
        }),
    }
}

/// Rusting corrugated metal — canopy decks, the motel walkway roof, the
/// shack's patched roof.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.22),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sun-greyed plank — the produce stand, the boarded shack, sign backs.
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
            knot_density: Fp64(0.25),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Enamel + structure palette.
pub(super) const ENAMEL_RED: [f32; 3] = [0.74, 0.16, 0.14];
pub(super) const ENAMEL_BLUE: [f32; 3] = [0.16, 0.30, 0.55];
pub(super) const ENAMEL_CREAM: [f32; 3] = [0.90, 0.86, 0.74];
pub(super) const CHROME_BRIGHT: [f32; 3] = [0.78, 0.80, 0.84];
pub(super) const STEEL_GREY: [f32; 3] = [0.50, 0.52, 0.55];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.62, 0.61, 0.59];
pub(super) const ASPHALT_DARK: [f32; 3] = [0.09, 0.09, 0.10];
pub(super) const BRICK_TAN: [f32; 3] = [0.64, 0.46, 0.32];
pub(super) const CORRUGATED_GREY: [f32; 3] = [0.60, 0.62, 0.64];
pub(super) const PLANK_WOOD: [f32; 3] = [0.52, 0.40, 0.26];
pub(super) const DRIFT_GREY: [f32; 3] = [0.55, 0.53, 0.49];
pub(super) const TARP_BLUE: [f32; 3] = [0.20, 0.34, 0.58];
pub(super) const RUST_BROWN: [f32; 3] = [0.48, 0.30, 0.16];

// Sign + glass colours.
pub(super) const GLASS_TINT: [f32; 3] = [0.32, 0.44, 0.50];
pub(super) const SIGN_WHITE: [f32; 3] = [0.92, 0.92, 0.88];
pub(super) const ROAD_GREEN: [f32; 3] = [0.12, 0.42, 0.24];
pub(super) const CONE_ORANGE: [f32; 3] = [0.95, 0.40, 0.08];

// Emissive trim colours.
pub(super) const NEON_RED: [f32; 3] = [1.0, 0.22, 0.26];
pub(super) const NEON_CYAN: [f32; 3] = [0.36, 0.95, 1.0];
pub(super) const PRICE_AMBER: [f32; 3] = [1.0, 0.80, 0.34];
pub(super) const CANOPY_LIT: [f32; 3] = [1.0, 0.96, 0.86];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (busted-shoulder) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &produce_stand::ProduceStand,
            &boarded_shack::BoardedShack,
            &oil_drums::OilDrums,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The gas station is the kit's lit hero — it must keep its emissive
    /// canopy and pylon sign so escalation's broken-emissive ruin pass has
    /// lights to snuff.
    #[test]
    fn gas_station_keeps_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&gas_station::GasStation.build("")),
            "gas station lost its emissive canopy / sign"
        );
    }
}
