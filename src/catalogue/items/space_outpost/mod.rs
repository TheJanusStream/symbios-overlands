//! Space-Outpost-theme catalogue structures — a pressurised off-world colony
//! of habitat domes and support modules under a thin atmosphere.
//!
//! Two prosperity registers share one frontier-colony identity: the
//! established ([`OUTPOST_BAND`]) base (a habitat dome, a solar array, a comms
//! dish, a landing pad, a hydroponics module, a rover, cargo crates, a beacon
//! and an airlock) and the destitute ([`OUTPOST_POOR`]) wreck kit (a crash
//! shelter, a collapsed solar wreck, a scrap canister).
//!
//! Surfaces use the real procedural generators rather than flat colour: white
//! brushed [`hull`] plating, dark structural [`steel`], lit [`glass`]
//! viewports, glossy dark [`pv`] arrays, ceramic [`concrete`] pads and matte
//! [`painted`] hazard markings. The dome's viewports and interior glow, the
//! beacons and the grow-lights shine over a reactor-hum and comms-static bed
//! from [`fx`]. The theme's thin-atmosphere accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod airlock;
pub mod beacon;
pub mod cargo_crate;
pub mod comms_dish;
pub mod habitat_dome;
pub mod hydroponics;
pub mod landing_pad;
pub mod rover;
pub mod solar_array;
// Poor (wreck) variants — the prosperity-Poor end of the theme.
pub mod crash_shelter;
pub mod scrap_canister;
pub mod solar_wreck;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignConcreteConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established base — a crewed outpost reads as
/// a Modest-to-Rich colony. The poor end of the theme is the separate wreck
/// kit ([`crash_shelter`], …), tagged `Poor`, so a destitute space room grows
/// the derelict crash site instead.
pub(super) const OUTPOST_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the wreck kit — the destitute end of the theme, never
/// picked for a modest or affluent space room.
pub(super) const OUTPOST_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// White brushed hull plating — habitat shells, modules, the rover body.
pub(super) fn hull(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.7),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.36, 0.32]),
            seam_count: Fp64(4.0),
            roughness: Fp64(0.35),
            metallic: Fp(0.7),
            rust_level: Fp64(0.02),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dark structural steel — frames, masts, legs, dish mounts, wheels.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.45),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.22, 0.16]),
            roughness: Fp64(0.45),
            metallic: Fp(0.85),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit viewport glass — habitat windows, hydroponics glazing, hatches. A
/// faint inner glow (`glow`) so the ports read as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.12),
        metallic: Fp(0.4),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 1,
            glass_opacity: Fp64(0.35),
            grime_level: Fp64(0.05),
            color_frame: Fp3([0.6, 0.62, 0.66]),
            ..Default::default()
        }),
    }
}

/// Glossy dark photovoltaic — the solar arrays.
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
            seam_count: Fp64(8.0),
            roughness: Fp64(0.12),
            metallic: Fp(0.6),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Ceramic concrete — the landing pad and footings.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(3.0),
            formwork_depth: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte paint — hazard markings, pad chevrons, crate stencils. A plain
/// coloured surface with no procedural texture.
pub(super) fn painted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Hull + structure palette.
pub(super) const HULL_WHITE: [f32; 3] = [0.84, 0.85, 0.87];
pub(super) const HULL_PANEL: [f32; 3] = [0.70, 0.72, 0.76];
pub(super) const STEEL_DARK: [f32; 3] = [0.34, 0.36, 0.40];
pub(super) const PV_BLUE: [f32; 3] = [0.10, 0.14, 0.30];
pub(super) const PAD_GREY: [f32; 3] = [0.40, 0.40, 0.42];
pub(super) const HAZARD_YELLOW: [f32; 3] = [0.86, 0.72, 0.10];
pub(super) const SCORCH: [f32; 3] = [0.32, 0.28, 0.26];

// Glass + emissive palette.
pub(super) const GLASS_CYAN: [f32; 3] = [0.42, 0.66, 0.74];
pub(super) const VIEWPORT_LIT: [f32; 3] = [0.6, 0.95, 1.0];
pub(super) const INTERIOR_WARM: [f32; 3] = [1.0, 0.92, 0.78];
pub(super) const BEACON_RED: [f32; 3] = [1.0, 0.22, 0.22];
pub(super) const GROW_PINK: [f32; 3] = [1.0, 0.42, 0.82];

/// Walk a built tree and report whether any primitive is strongly emissive
/// — the shared "is the outpost still powered?" check for the kit's tests.
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

    /// The three poor (wreck) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &crash_shelter::CrashShelter,
            &solar_wreck::SolarWreck,
            &scrap_canister::ScrapCanister,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The habitat dome is the kit's lit hero — it must keep its emissive
    /// viewports and interior glow so escalation's broken-emissive ruin pass
    /// has light to snuff.
    #[test]
    fn habitat_dome_keeps_its_glow() {
        assert!(
            has_emissive(&habitat_dome::HabitatDome.build("")),
            "habitat dome lost its emissive viewports / interior glow"
        );
    }
}
