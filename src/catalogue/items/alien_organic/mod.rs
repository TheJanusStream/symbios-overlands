//! Alien-Organic-theme catalogue structures — a living hive-colony of chitin,
//! flesh and biolume.
//!
//! Two prosperity registers share one xenobiological identity: the established
//! ([`ORGANIC_BAND`]) thriving hive (a chitinous hive, a pod cluster, a fleshy
//! spire, a membrane wall, egg sacs, biolume stalks, a tendril, a spore vent
//! and a creep patch) and the destitute ([`ORGANIC_POOR`]) necrotic kit (a
//! withered hive, burst husk pods, a rot patch).
//!
//! Surfaces use the real procedural generators rather than flat colour: hard
//! glossy [`chitin`] shell, soft matte [`flesh`], wet [`membrane`] and glowing
//! biolume carried by [`crate::catalogue::items::util::glow`] emissive trim.
//! The hive pulses and spores drift over an eerie [`fx`] bed. The theme's
//! green-biolume accent lives in [`crate::seeded_defaults::room::accent`].

pub mod biolume_stalk;
pub mod chitinous_hive;
pub mod creep_patch;
pub mod egg_sac;
pub mod fleshy_spire;
pub mod membrane_wall;
pub mod pod_cluster;
pub mod spore_vent;
pub mod tendril;
// Poor (necrotic) variants — the prosperity-Poor end of the theme.
pub mod husk_pods;
pub mod rot_patch;
pub mod withered_hive;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignMaterialSettings, SovereignMetalConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the thriving hive — a living biolit colony reads
/// as a Modest-to-Rich organism. The poor end of the theme is the separate
/// necrotic kit ([`withered_hive`], …), tagged `Poor`, so a destitute alien
/// room grows the dying colony instead.
pub(super) const ORGANIC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the necrotic kit — the destitute end of the theme,
/// never picked for a modest or affluent alien room.
pub(super) const ORGANIC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Hard glossy chitin — the hive's plated shell, ribs and carapace.
pub(super) fn chitin(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.5),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([color[0] * 1.4, color[1] * 0.8, color[2] * 1.2]),
            roughness: Fp64(0.3),
            metallic: Fp(0.5),
            rust_level: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Soft matte flesh — pods, spires, tendrils, the hive's living tissue.
pub(super) fn flesh(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Wet translucent membrane — stretched walls and sac skins, a damp sheen.
pub(super) fn membrane(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.22),
        metallic: Fp(0.1),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Chitin + flesh palette.
pub(super) const CHITIN_DARK: [f32; 3] = [0.20, 0.16, 0.26];
pub(super) const CHITIN_GREEN: [f32; 3] = [0.18, 0.24, 0.20];
pub(super) const FLESH_RED: [f32; 3] = [0.52, 0.26, 0.30];
pub(super) const FLESH_PINK: [f32; 3] = [0.66, 0.42, 0.46];
pub(super) const MEMBRANE_TEAL: [f32; 3] = [0.34, 0.54, 0.50];
pub(super) const NECROTIC: [f32; 3] = [0.44, 0.42, 0.38];
pub(super) const HUSK: [f32; 3] = [0.56, 0.50, 0.42];

// Emissive biolume colours.
pub(super) const BIOLUME_CYAN: [f32; 3] = [0.40, 1.0, 0.86];
pub(super) const BIOLUME_GREEN: [f32; 3] = [0.55, 1.0, 0.45];
pub(super) const SAC_GLOW: [f32; 3] = [1.0, 0.55, 0.72];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (necrotic) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &withered_hive::WitheredHive,
            &husk_pods::HuskPods,
            &rot_patch::RotPatch,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The chitinous hive is the kit's lit hero — it must keep its emissive
    /// biolume so escalation's broken-emissive ruin pass has light to snuff.
    #[test]
    fn hive_keeps_its_biolume() {
        assert!(
            crate::catalogue::items::util::has_emissive(&chitinous_hive::ChitinousHive.build("")),
            "chitinous hive lost its emissive biolume"
        );
    }
}
