//! Medieval-theme catalogue structures — a fortified market town of
//! dressed stone, timber framing, thatch and slate.
//!
//! Two prosperity registers share one identity: the established
//! ([`MEDIEVAL_BAND`]) town kit (the grammar-built [`medieval_castle`]
//! and [`watchtower`], plus the primitive-built [`chapel`], [`blacksmith`]
//! forge, [`market_hall`], [`well_house`], and the [`handcart`],
//! [`barrel_stack`], [`trade_stall`] and [`banner_pole`] props) and the
//! destitute ([`MEDIEVAL_POOR`]) cottar kit ([`wattle_hovel`],
//! [`lean_to`], [`kindling_pile`]).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! sawn [`timber`] plank framing, dressed-ashlar [`stone`] and rough
//! fieldstone [`rough_stone`], lime-washed wattle-and-daub [`daub`],
//! straw [`thatch`] and tiled [`shingle`] roofs, riveted [`iron`]
//! fittings and woven [`cloth`] banners and awnings. The blacksmith's
//! forge comes alive with the small particle emitters and the fire
//! crackle in [`fx`] (sooty smoke, leaping sparks), and the hovel breathes
//! hearth smoke. The theme's cool steel-blue light accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod medieval_castle;
pub mod watchtower;
// The theme's bespoke social gateway (#760) — the walk-through Town Gate.
pub mod gateway;
// Established (town) secondaries + props — primitive-built.
pub mod banner_pole;
pub mod barrel_stack;
pub mod blacksmith;
pub mod chapel;
pub mod handcart;
pub mod market_hall;
pub mod trade_stall;
pub mod well_house;
// Poor (cottar) variants — the prosperity-Poor end of the theme.
pub mod kindling_pile;
pub mod lean_to;
pub mod wattle_hovel;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim, solid};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignAshlarConfig, SovereignCobblestoneConfig,
    SovereignFabricConfig, SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig,
    SovereignShingleConfig, SovereignStuccoConfig, SovereignTextureConfig, SovereignThatchConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established town kit — dressed stone,
/// a smith and a market read as a Modest-to-Rich burgh. The poor end of
/// the theme is the separate cottar kit ([`wattle_hovel`], …), tagged
/// `Poor`, so a destitute Medieval room grows the daub hovel instead.
pub(super) const MEDIEVAL_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the cottar kit — the destitute end of the theme,
/// never picked for a modest or affluent Medieval room.
pub(super) const MEDIEVAL_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Sawn oak framing — the body of every timber-framed build: posts,
/// braces, jetties, cart beds, market trestles. Warm grain with knots so
/// a wall reads as wood, not a painted slab.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.25, color[1] * 1.25, color[2] * 1.25]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.6]),
            plank_count: Fp64(6.0),
            knot_density: Fp64(0.3),
            grain_warp: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dressed ashlar stone — chapel and castle walls, market-hall pillars,
/// the well kerb. Coursed blocks with a pale mortar line.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 1.28, color[1] * 1.28, color[2] * 1.22]),
            rows: 4,
            cols: 4,
            chisel_depth: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rough fieldstone cobble — undressed footings, the well shaft, lean-to
/// footings, mud-packed rubble walls.
pub(super) fn rough_stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.45, color[1] * 0.4, color[2] * 0.32]),
            roundness: Fp64(1.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lime-washed wattle-and-daub — the pale infill panels between the
/// timber frame, and the whole of the poor hovel's walls.
pub(super) fn daub(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.78, color[1] * 0.78, color[2] * 0.74]),
            roughness: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Golden straw thatch — the steep roof of a cottage, market hall, or
/// well canopy.
pub(super) fn thatch(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Thatch(SovereignThatchConfig {
            color_straw: Fp3(color),
            color_shadow: Fp3([color[0] * 0.32, color[1] * 0.30, color[2] * 0.18]),
            density: Fp64(14.0),
            layer_count: Fp64(9.0),
            layer_shadow: Fp64(0.6),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Tiled / slate shingle — the high-status roof of the chapel and the
/// castle, mossed with age.
pub(super) fn shingle(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([color[0] * 0.5, color[1] * 0.5, color[2] * 0.55]),
            moss_level: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Riveted dark iron — door bands, anvil, well windlass, cart tyres,
/// banner finials. Brushed with a little rust.
pub(super) fn iron(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.8),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.34, 0.20, 0.10]),
            roughness: Fp64(0.5),
            metallic: Fp(0.8),
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Woven wool / linen cloth — heraldic banners and the striped awning of
/// the market stall.
pub(super) fn cloth(warp: [f32; 3], weft: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(warp),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            color_warp: Fp3(warp),
            color_weft: Fp3(weft),
            thread_count: Fp64(20.0),
            fuzz: Fp64(0.45),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Stone + timber palette.
pub(super) const STONE_GREY: [f32; 3] = [0.54, 0.52, 0.49];
pub(super) const STONE_PALE: [f32; 3] = [0.62, 0.60, 0.55];
pub(super) const WOOD_OAK: [f32; 3] = [0.40, 0.27, 0.15];
pub(super) const WOOD_DARK: [f32; 3] = [0.26, 0.17, 0.09];
pub(super) const DAUB_CREAM: [f32; 3] = [0.80, 0.76, 0.66];
pub(super) const THATCH_STRAW: [f32; 3] = [0.58, 0.48, 0.26];
pub(super) const SLATE_GREY: [f32; 3] = [0.32, 0.33, 0.37];
pub(super) const IRON_DARK: [f32; 3] = [0.20, 0.21, 0.23];

// Heraldic banner / awning colours.
pub(super) const HERALD_RED: [f32; 3] = [0.60, 0.13, 0.12];
pub(super) const HERALD_BLUE: [f32; 3] = [0.15, 0.24, 0.46];
pub(super) const HERALD_GOLD: [f32; 3] = [0.76, 0.58, 0.18];
pub(super) const CLOTH_CREAM: [f32; 3] = [0.80, 0.74, 0.60];

/// Warm forge-fire light for the blacksmith's hearth glow.
pub(super) const FORGE_ORANGE: [f32; 3] = [1.0, 0.52, 0.16];

/// A ring of merlons (the solid teeth) around a square parapet rim, with
/// equal-width crenel gaps between them — the defining medieval battlement
/// silhouette. `top` is the rim-top centre (merlons rise from it); `hw`/`hz`
/// the half-extents of the wall top in X/Z; `mh` the merlon height; `mw` the
/// merlon width; `t` its radial thickness. The merlons sit *on* the rim, so
/// they share no vertical plane with the wall below (no coplanar z-fight),
/// and straddle the wall face so they read proud from outside. Corners are
/// placed once (on the ±Z runs) and skipped on the ±X runs. Returns every
/// merlon for an [`assemble`](crate::catalogue::items::util::assemble) list —
/// never the root (drop them in after a base piece).
pub(super) fn crenellations(
    top: [f32; 3],
    hw: f32,
    hz: f32,
    mh: f32,
    mw: f32,
    t: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [cx, cy, cz] = top;
    let merlon_y = cy + mh * 0.5;
    let mut v = Vec::new();
    // ±Z runs (merlons march along X), corners included.
    let nx = ((2.0 * hw) / (2.0 * mw)).round().max(1.0) as i32;
    for sz in [-1.0_f32, 1.0] {
        for i in 0..=nx {
            let x = -hw + 2.0 * hw * (i as f32 / nx as f32);
            v.push(prim(
                solid(cuboid_tapered([mw, mh, t], 0.0, mat.clone())),
                [cx + x, merlon_y, cz + sz * hz],
                id_quat(),
            ));
        }
    }
    // ±X runs (merlons march along Z), corners skipped (already placed above).
    let nz = ((2.0 * hz) / (2.0 * mw)).round().max(1.0) as i32;
    for sx in [-1.0_f32, 1.0] {
        for i in 1..nz {
            let z = -hz + 2.0 * hz * (i as f32 / nz as f32);
            v.push(prim(
                solid(cuboid_tapered([t, mh, mw], 0.0, mat.clone())),
                [cx + sx * hw, merlon_y, cz + z],
                id_quat(),
            ));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (cottar) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &wattle_hovel::WattleHovel,
            &lean_to::LeanTo,
            &kindling_pile::KindlingPile,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The blacksmith is the kit's firelit hero — it must keep its emissive
    /// forge fire so escalation's broken-emissive ruin pass has something to
    /// snuff.
    #[test]
    fn blacksmith_keeps_its_forge_fire() {
        assert!(
            crate::catalogue::items::util::has_emissive(&blacksmith::Blacksmith.build("")),
            "blacksmith lost its emissive forge fire"
        );
    }
}
