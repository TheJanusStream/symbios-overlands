//! Sports / Recreation-theme catalogue structures — a stadium complex and
//! its training grounds.
//!
//! Two prosperity registers share one sporting identity: the established
//! ([`SPORTS_BAND`]) stadium kit (the bowl, a gym, open bleachers, a ticket
//! booth, a clubhouse, goalposts, floodlight masts, a scoreboard and a team
//! bench) and the destitute ([`SPORTS_POOR`]) rec-ground kit (a cracked
//! court, a chain-link backstop, a stack of training tyres).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! board-formed [`concrete`] stands, brushed [`steel`] masts and frames,
//! glossy [`enamel`] seats and panels, lit [`glass`] gym glazing, rusting
//! [`corrugated`] cladding, woven [`chainlink`] fencing, cracked [`asphalt`]
//! courts, mown [`turf`] pitches and flat [`painted`] line markings. The
//! scoreboard and floodlights glow over a crowd-murmur and tannoy bed from
//! [`fx`]. The theme's bright field-day accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod bleachers;
pub mod clubhouse;
pub mod floodlight_mast;
pub mod goalpost;
pub mod gym;
pub mod players_bench;
pub mod scoreboard;
pub mod stadium;
pub mod ticket_booth;
// Poor (rec-ground) variants — the prosperity-Poor end of the theme.
pub mod backstop;
pub mod rec_court;
pub mod tire_stack;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAsphaltConfig, SovereignChainLinkConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established stadium — a working ground
/// reads as a Modest-to-Rich complex. The poor end of the theme is the
/// separate rec-ground kit ([`rec_court`], …), tagged `Poor`, so a destitute
/// sports room grows the cracked municipal court instead.
pub(super) const SPORTS_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the rec-ground kit — the destitute end of the theme,
/// never picked for a modest or affluent sports room.
pub(super) const SPORTS_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Board-formed concrete — stand structure, plinths, courts, the gym base.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(5.0),
            formwork_depth: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Brushed structural steel — floodlight masts, goalposts, frames, railings.
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

/// Glossy painted enamel — seat banks, scoreboard housings, panels, the
/// hoop backboard. Smooth coloured finish.
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

/// Lit gym / clubhouse / booth glass — a faint inner glow (`glow`) so the
/// glazing reads as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.4),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 5,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.1),
            color_frame: Fp3([0.3, 0.31, 0.34]),
            ..Default::default()
        }),
    }
}

/// Rusting corrugated metal — gym cladding, dugout and stand roofs.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.18),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Woven chain-link — perimeter fencing and the backstop.
pub(super) fn chainlink(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.7),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::ChainLink(SovereignChainLinkConfig {
            color_wire: Fp3(color),
            cell_count: Fp64(8.0),
            rust_level: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cracked asphalt — the poor rec-court surface.
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

/// Flat matte paint — the mown pitch, line markings, court colour, painted
/// trim. A plain coloured surface with no procedural texture.
pub(super) fn painted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Mown grass — the pitch and the field, a soft matte green.
pub(super) fn turf(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(4.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Structure palette.
pub(super) const CONCRETE_GREY: [f32; 3] = [0.62, 0.61, 0.59];
pub(super) const STEEL_GREY: [f32; 3] = [0.52, 0.54, 0.57];
pub(super) const CORRUGATED_GREY: [f32; 3] = [0.60, 0.62, 0.64];
pub(super) const CHAIN_GREY: [f32; 3] = [0.62, 0.64, 0.66];
pub(super) const ASPHALT_DARK: [f32; 3] = [0.10, 0.10, 0.11];
pub(super) const PITCH_GREEN: [f32; 3] = [0.22, 0.42, 0.20];
pub(super) const COURT_BLUE: [f32; 3] = [0.20, 0.36, 0.50];
pub(super) const LINE_WHITE: [f32; 3] = [0.92, 0.92, 0.88];
pub(super) const SEAT_BLUE: [f32; 3] = [0.18, 0.34, 0.58];
pub(super) const SEAT_RED: [f32; 3] = [0.62, 0.18, 0.16];
pub(super) const GLASS_TINT: [f32; 3] = [0.40, 0.50, 0.54];
pub(super) const HOOP_ORANGE: [f32; 3] = [0.92, 0.42, 0.10];

// Emissive trim colours.
pub(super) const FLOOD_LIT: [f32; 3] = [1.0, 0.97, 0.90];
pub(super) const SCORE_AMBER: [f32; 3] = [1.0, 0.78, 0.32];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (rec-ground) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &rec_court::RecCourt,
            &backstop::Backstop,
            &tire_stack::TireStack,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The stadium is the kit's lit hero — it must keep its emissive
    /// floodlights and scoreboard so escalation's broken-emissive ruin pass
    /// has lights to snuff.
    #[test]
    fn stadium_keeps_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&stadium::Stadium.build("")),
            "stadium lost its emissive floodlights / scoreboard"
        );
    }
}
