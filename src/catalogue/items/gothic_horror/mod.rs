//! Gothic-Horror-theme catalogue structures — a fog-shrouded necropolis of
//! cathedral, crypt and grave.
//!
//! Two prosperity registers share one funereal identity: the established
//! ([`GOTHIC_BAND`]) consecrated kit (a cathedral, a mausoleum, a cemetery, a
//! bell tower, gravestones, a gargoyle, a dead tree, an iron fence and a stone
//! cross) and the destitute ([`GOTHIC_POOR`]) forsaken kit (a ruined chapel, a
//! pauper's graves plot, a bone pile).
//!
//! Surfaces use the real procedural generators rather than flat colour: dark
//! dressed [`stone`] ashlar, [`mossy`] weathered cobble, lit leaded
//! [`stained`] glass, black wrought [`iron`], grey dead [`wood`] and [`matte`]
//! bone. The cathedral's stained windows glow over a cold-wind and
//! ghostly-drone bed from [`fx`]. The theme's desaturating fog accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod bell_tower;
pub mod cathedral;
pub mod cemetery;
pub mod dead_tree;
pub mod gargoyle;
pub mod gravestone;
pub mod iron_fence;
pub mod mausoleum;
pub mod stone_cross;
// Poor (forsaken) variants — the prosperity-Poor end of the theme.
pub mod bone_pile;
pub mod pauper_graves;
pub mod ruined_chapel;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAshlarConfig, SovereignCobblestoneConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignStainedGlassConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the consecrated kit — a cathedral and its
/// necropolis read as a Modest-to-Rich holy seat. The poor end of the theme is
/// the separate forsaken kit ([`ruined_chapel`], …), tagged `Poor`, so a
/// destitute gothic room grows the abandoned graveyard instead.
pub(super) const GOTHIC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the forsaken kit — the destitute end of the theme,
/// never picked for a modest or affluent gothic room.
pub(super) const GOTHIC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Dark dressed ashlar — cathedral, mausoleum and tower masonry.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.62]),
            rows: 5,
            cols: 4,
            chisel_depth: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Mossy weathered cobble — crypt footings, gravestones, old walls.
pub(super) fn mossy(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.6, color[2] * 0.42]),
            roundness: Fp64(1.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit leaded stained glass — the cathedral's windows and rose. A coloured
/// inner glow (`glow`) so the tracery reads as lit from within the nave.
pub(super) fn stained(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.1),
        metallic: Fp(0.1),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::StainedGlass(SovereignStainedGlassConfig {
            cell_count: 16,
            grime_level: Fp64(0.18),
            ..Default::default()
        }),
    }
}

/// Black wrought iron — fences, gates, the bell, finials.
pub(super) fn iron(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.32, 0.20, 0.12]),
            roughness: Fp64(0.55),
            metallic: Fp(0.85),
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Grey dead wood — bare trees, coffins, pauper markers, doors.
pub(super) fn wood(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.2]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.6]),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.4),
            grain_warp: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte colour — bone, plain trim. A plain surface with no procedural
/// texture.
pub(super) fn matte(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Masonry + material palette.
pub(super) const STONE_DARK: [f32; 3] = [0.42, 0.42, 0.45];
pub(super) const STONE_MOSS: [f32; 3] = [0.40, 0.44, 0.38];
pub(super) const IRON_BLACK: [f32; 3] = [0.14, 0.14, 0.16];
pub(super) const DEADWOOD: [f32; 3] = [0.34, 0.32, 0.30];
pub(super) const BONE: [f32; 3] = [0.80, 0.78, 0.70];
pub(super) const STAINED_TINT: [f32; 3] = [0.58, 0.40, 0.52];

// Emissive trim colours.
pub(super) const STAINED_GLOW: [f32; 3] = [0.85, 0.48, 0.66];

/// Walk a built tree and report whether any primitive is strongly emissive
/// — the shared "do the windows still glow?" check for the kit's tests.
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

    /// The three poor (forsaken) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &ruined_chapel::RuinedChapel,
            &pauper_graves::PauperGraves,
            &bone_pile::BonePile,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The cathedral is the kit's lit hero — it must keep its emissive stained
    /// glass so escalation's broken-emissive ruin pass has light to snuff.
    #[test]
    fn cathedral_keeps_its_glow() {
        assert!(
            has_emissive(&cathedral::Cathedral.build("")),
            "cathedral lost its emissive stained glass"
        );
    }
}
