//! Cyberpunk-theme catalogue structures — the kit's quality benchmark for
//! every theme that follows.
//!
//! Two prosperity registers share one neon identity: the affluent
//! ([`CYBER_BAND`]) glossy-metal kit (megatower, data spire, arcade block,
//! holo billboard, parking stack, kiosk, drone perch, cable arch) and the
//! destitute ([`CYBER_POOR`]) scrap-shanty undercity (container shanty,
//! container stack, tarp shelter, e-waste pile, busted terminal).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! standing-seam [`metal`], lit [`window_wall`] facades, [`corrugated`]
//! container steel, [`concrete`] decks, [`chain_link`] fencing,
//! [`grille`] vents, brushed-rust [`rust`] scrap and woven [`tarp`].
//! Neon comes from strongly emissive [`super::util::glow`] materials, and
//! signature elements are brought to life with small particle emitters and
//! spatial-audio patches from [`fx`] (steam vents, failing-neon sparks,
//! transformer hum, drone whir, electrical crackle). The theme's magenta
//! fog accent lives in [`crate::seeded_defaults::room::accent`].
//!
//! **Emissive-strength discipline.** With HDR + bloom, a [`super::util::glow`]
//! surface clips to white once `colour × strength` pushes a channel past
//! `1.0`, and a *broad face* (a billboard panel, a screen) reaches that
//! point at a far lower strength than a *thin tube* (a band, an edge strip,
//! a ring). So the two can't share a value: thin neon trim runs hot
//! (`~5–9`) — the white-hot core plus a coloured bloom halo is exactly how
//! a neon tube reads — while broad faces stay moderate (`~1.5–3.5`) so they
//! read as lit *colour*, not a featureless white lightbox. A framed face
//! (panel ringed by a hot tube border) gets the best of both.

pub mod arcade_block;
pub mod cable_arch;
pub mod data_spire;
pub mod drone_perch;
pub mod gateway;
pub mod holo_billboard;
pub mod neon_kiosk;
pub mod neon_megatower;
pub mod parking_stack;
// Poor (undercity) variants — the prosperity-Poor end of the theme.
pub mod busted_terminal;
pub mod container_stack;
pub mod ewaste_pile;
pub mod scrap_shanty;
pub mod tarp_shelter;

pub mod fx;

use super::util::{tile, tiles_per_metre};
use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignChainLinkConfig, SovereignConcreteConfig, SovereignCorrugatedConfig,
    SovereignIronGrilleConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established neon kit — these glossy
/// megastructures read as a Modest-to-Rich settlement. The poor end of the
/// theme is the separate scrap-shanty kit ([`scrap_shanty`], …), tagged
/// `Poor`, so a destitute cyberpunk room grows the undercity instead.
pub(super) const CYBER_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the scrap-shanty undercity kit — the destitute end
/// of the theme, never picked for a modest or affluent cyberpunk room.
pub(super) const CYBER_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Dark, glossy structural metal — the body shared by every cyberpunk
/// build. Standing-seam panel lines + a touch of grime so the neon trim
/// reflects off a *surface*, not a flat slab.
pub(super) fn metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.32),
        metallic: Fp(0.85),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::StandingSeam,
            color_metal: Fp3(color),
            color_rust: Fp3([0.20, 0.12, 0.08]),
            seam_count: Fp64(8.0),
            seam_sharpness: Fp64(2.5),
            roughness: Fp64(0.32),
            metallic: Fp(0.85),
            rust_level: Fp64(0.06),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A lit window-grid facade — frames + grimy glass with a faint inner glow,
/// so a tower reads as a glowing building rather than a black box. `glow`
/// sets how brightly the panes shine (city-light bloom).
pub(super) fn window_wall(glass: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(glass),
        emission_color: Fp3(glass),
        emission_strength: Fp(glow),
        roughness: Fp(0.4),
        metallic: Fp(0.2),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 3,
            panes_y: 4,
            glass_opacity: Fp64(0.5),
            grime_level: Fp64(0.2),
            color_frame: Fp3([0.08, 0.09, 0.12]),
            ..Default::default()
        }),
    }
}

/// Ridged corrugated steel — shipping containers and lean-to roofing. The
/// correct surface for the scrap-shanty undercity, with built-in rust.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        uv_scale: tiles_per_metre(tile::CORRUGATED_PITCH * 10.0),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.22, 0.10]),
            ridges: Fp64(10.0),
            ridge_depth: Fp64(1.0),
            roughness: Fp64(0.5),
            metallic: Fp(0.6),
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Corroded brushed metal with heavy rust — battered scrap panels, drums,
/// dead chassis. The poor counterpoint to the glossy [`metal`].
pub(super) fn rust(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.4),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.30, 0.16, 0.08]),
            seam_count: Fp64(3.0),
            roughness: Fp64(0.85),
            metallic: Fp(0.4),
            rust_level: Fp64(0.55),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusted chain-link / mesh — undercity fencing and cage panels.
pub(super) fn chain_link() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.5, 0.52, 0.54]),
        roughness: Fp(0.7),
        metallic: Fp(0.5),
        texture: SovereignTextureConfig::ChainLink(SovereignChainLinkConfig {
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete — parking decks, stair cores, plinths.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: tiles_per_metre(tile::CONCRETE),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(4.0),
            formwork_depth: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusted iron louvre / grille — wall vents and exhaust louvres.
pub(super) fn grille() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.14, 0.13, 0.13]),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        texture: SovereignTextureConfig::IronGrille(SovereignIronGrilleConfig {
            rust_level: Fp64(0.25),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sagging tarp / plastic sheeting over a makeshift shelter — woven-fabric
/// weave normal so it reads as cloth, not a painted plank.
pub(super) fn tarp(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::FABRIC),
        texture: SovereignTextureConfig::Fabric(crate::pds::SovereignFabricConfig::default()),
        ..Default::default()
    }
}

/// Near-black panelled body colour.
pub(super) const DARK_METAL: [f32; 3] = [0.06, 0.07, 0.10];
pub(super) const NEON_CYAN: [f32; 3] = [0.10, 0.95, 1.00];
pub(super) const NEON_MAGENTA: [f32; 3] = [1.00, 0.12, 0.78];
pub(super) const NEON_LIME: [f32; 3] = [0.55, 1.00, 0.20];

// Scrap-shanty palette — weathered container steel, rust, faded tarp.
pub(super) const CONTAINER_BLUE: [f32; 3] = [0.18, 0.30, 0.38];
pub(super) const CONTAINER_RUST: [f32; 3] = [0.45, 0.28, 0.18];
pub(super) const RUST_BROWN: [f32; 3] = [0.34, 0.22, 0.14];
pub(super) const TARP_BLUE: [f32; 3] = [0.18, 0.26, 0.42];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The five poor (undercity) variants must build clean trees the
    /// sanitiser leaves untouched, and each must still carry its dim neon.
    #[test]
    fn poor_variants_round_trip_and_keep_their_glow() {
        let entries: [&dyn CatalogueEntry; 5] = [
            &scrap_shanty::ScrapShanty,
            &container_stack::ContainerStack,
            &tarp_shelter::TarpShelter,
            &ewaste_pile::EwastePile,
            &busted_terminal::BustedTerminal,
        ];
        for e in entries {
            let built = e.build("");
            assert_sanitize_stable(&built, e.slug());
            assert!(
                crate::catalogue::items::util::has_emissive(&built),
                "{} lost its neon",
                e.slug()
            );
        }
    }
}
