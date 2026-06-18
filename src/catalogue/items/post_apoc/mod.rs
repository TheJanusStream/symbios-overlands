//! Post-apocalyptic-theme catalogue structures — a scavenged survivor
//! settlement of fortified ruins and welded scrap.
//!
//! Two prosperity registers share one wasteland identity: the established
//! ([`POSTAPOC_BAND`]) holdout (a fortified ruin, a salvage shack, a radio
//! mast, a fuel depot, a wrecked car, a scrap wall, fuel barrels, a tyre wall
//! and a signal fire) and the destitute ([`POSTAPOC_POOR`]) drifter kit (a
//! survivor lean-to, a rubble barricade, an ash pit).
//!
//! Surfaces use the real procedural generators rather than flat colour: heavy
//! [`rusted`] scrap, cracked [`concrete`], corrugated [`sheet`] metal, grey
//! [`plank`] and matte [`tarp`]. The ruin's barrel fire and worklight glow
//! over a desolate-wind and fire-crackle bed from [`fx`]. The theme's
//! dust-haze accent lives in [`crate::seeded_defaults::room::accent`].

pub mod fortified_ruin;
pub mod fuel_barrels;
pub mod fuel_depot;
pub mod radio_mast;
pub mod salvage_shack;
pub mod scrap_wall;
pub mod signal_fire;
pub mod tire_wall;
pub mod wrecked_car;
// Poor (drifter) variants — the prosperity-Poor end of the theme.
pub mod ash_pit;
pub mod rubble_barricade;
pub mod survivor_lean_to;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignConcreteConfig, SovereignCorrugatedConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the holdout — a fortified, lit, defended camp
/// reads as a Modest-to-Rich survivor settlement. The poor end of the theme is
/// the separate drifter kit ([`survivor_lean_to`], …), tagged `Poor`, so a
/// destitute wasteland room grows the lone hovel instead.
pub(super) const POSTAPOC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the drifter kit — the destitute end of the theme, never
/// picked for a modest or affluent wasteland room.
pub(super) const POSTAPOC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Heavily-rusted scrap metal — welded walls, drums, car bodies, the mast.
pub(super) fn rusted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.6),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.42, 0.24, 0.12]),
            roughness: Fp64(0.7),
            metallic: Fp(0.6),
            rust_level: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cracked, stained concrete — the ruin's surviving walls and slabs.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(3.0),
            pit_density: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rusting corrugated sheet — shanty walls, fences, lean-to roofs.
pub(super) fn sheet(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.6),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.45),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Grey weathered plank — salvaged timber framing and boards.
pub(super) fn plank(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.2]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.6]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.4),
            grain_warp: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Matte cloth / rubber / dirt — tarps, tyres, ash, sandbags. A plain surface
/// with no procedural texture.
pub(super) fn tarp(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Scrap + structure palette.
pub(super) const RUST_BROWN: [f32; 3] = [0.46, 0.30, 0.18];
pub(super) const STEEL_GREY: [f32; 3] = [0.40, 0.40, 0.42];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.50, 0.49, 0.46];
pub(super) const CORRUGATED_RUST: [f32; 3] = [0.50, 0.38, 0.26];
pub(super) const PLANK_GREY: [f32; 3] = [0.42, 0.40, 0.36];
pub(super) const TARP_FADED: [f32; 3] = [0.40, 0.46, 0.40];
pub(super) const TIRE_BLACK: [f32; 3] = [0.10, 0.10, 0.11];
pub(super) const CAR_RUST: [f32; 3] = [0.46, 0.33, 0.27];
pub(super) const ASH_GREY: [f32; 3] = [0.26, 0.25, 0.24];

// Emissive trim colours.
pub(super) const FIRE_ORANGE: [f32; 3] = [1.0, 0.50, 0.16];
pub(super) const WORKLIGHT: [f32; 3] = [1.0, 0.95, 0.82];
pub(super) const SIGNAL_RED: [f32; 3] = [1.0, 0.22, 0.20];

/// Walk a built tree and report whether any primitive is strongly emissive
/// — the shared "are the fires still burning?" check for the kit's tests.
#[cfg(test)]
pub(super) fn has_emissive(g: &crate::pds::Generator) -> bool {
    use crate::pds::GeneratorKind::*;
    let own = match &g.kind {
        Cuboid { material, .. }
        | Cylinder { material, .. }
        | Sphere { material, .. }
        | Cone { material, .. }
        | Torus { material, .. }
        | Capsule { material, .. } => material.emission_strength.0 > 1.0,
        _ => false,
    };
    own || g.children.iter().any(has_emissive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (drifter) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &survivor_lean_to::SurvivorLeanTo,
            &rubble_barricade::RubbleBarricade,
            &ash_pit::AshPit,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The fortified ruin is the kit's lit hero — it must keep its emissive
    /// barrel fire and worklight so escalation's broken-emissive ruin pass has
    /// fire to snuff.
    #[test]
    fn ruin_keeps_its_fire() {
        assert!(
            has_emissive(&fortified_ruin::FortifiedRuin.build("")),
            "fortified ruin lost its emissive fire / worklight"
        );
    }
}
