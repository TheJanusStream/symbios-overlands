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
pub mod gateway;
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

use super::util::{tile, tiles_per_metre};
use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{id_quat, prim_scaled, sphere};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignConcreteConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig, SovereignWindowConfig,
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
        uv_scale: Fp(1.0),
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
        uv_scale: tiles_per_metre(tile::METAL),
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
        uv_scale: tiles_per_metre(tile::PLANK_BOARD * 5.0),
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
        uv_scale: tiles_per_metre(tile::CONCRETE),
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
        // No texture, so `uv_scale` is inert — pinned at 1.0 so it does not
        // read as a stale pre-#936 repeat count.
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// A planted bed of leafy crop tufts — rounded foliage clumps in a grid, the
/// green that turns a flat painted slab into rows of growing greens. Each
/// clump is a flattened low-poly dome; sizes and offsets vary by index for an
/// organic, hand-planted look (a deterministic jitter, no RNG — so the
/// sanitiser round-trip stays stable). `center` is the soil-top centre,
/// `span` the bed `[x, z]` extent the clumps fill, `h` the nominal clump
/// height. Returns the clumps for an [`assemble`](crate::catalogue::items::util::assemble)
/// list — the solarpunk green signature, reused on every planter and terrace.
pub(super) fn crop_tufts(
    center: [f32; 3],
    span: [f32; 2],
    cols: u32,
    rows: u32,
    h: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let mut v = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let fc = if cols > 1 {
                c as f32 / (cols - 1) as f32 - 0.5
            } else {
                0.0
            };
            let fr = if rows > 1 {
                r as f32 / (rows - 1) as f32 - 0.5
            } else {
                0.0
            };
            // Deterministic per-clump jitter from the cell index.
            let k = (r * 7 + c * 13) % 5;
            let j = (r * 5 + c * 11) % 7;
            let s = 0.74 + k as f32 * 0.08; // size factor 0.74..1.06
            let clump_r = h * 0.55 * s;
            let wob = (j as f32 / 7.0 - 0.5) * h * 0.35; // small XZ wobble
            let x = center[0] + fc * span[0] + wob;
            let z = center[2] + fr * span[1] - wob;
            v.push(prim_scaled(
                sphere(clump_r, 5, mat.clone()),
                [x, center[1] + clump_r * 0.5, z],
                id_quat(),
                [1.0, 0.82, 1.0],
            ));
        }
    }
    v
}

/// Glossy dark photovoltaic panel — solar arrays and panel roofs.
pub(super) fn pv(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.12),
        metallic: Fp(0.6),
        uv_scale: tiles_per_metre(tile::METAL),
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
        // No texture, so `uv_scale` is inert — pinned at 1.0 so it does not
        // read as a stale pre-#936 repeat count.
        uv_scale: Fp(1.0),
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
/// Dark turned-earth of planting beds and tunnel floors.
pub(super) const SOIL_DARK: [f32; 3] = [0.34, 0.26, 0.18];

// Emissive trim colours — deep-saturated so the brightest facets hold their
// hue under bloom instead of washing to a pale near-white blank. A pale colour
// driven bright clips toward white; a saturated base keeps its off-channels
// low so it stays green / magenta / amber when lit (see the fantasy +
// space-outpost overhaul lesson).
pub(super) const DOME_GLOW: [f32; 3] = [0.34, 0.92, 0.40];
pub(super) const GROW_PINK: [f32; 3] = [1.0, 0.30, 0.74];
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.80, 0.45];

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
