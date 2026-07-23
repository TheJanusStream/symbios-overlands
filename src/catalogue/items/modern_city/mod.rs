//! Modern-City-theme catalogue structures — a glass-and-concrete downtown.
//!
//! Two prosperity registers share one identity: the established
//! ([`CITY_BAND`]) corporate kit (glass skyscraper, office block, parking
//! garage, transit stop, street lamp, traffic light, bus stop, parked car,
//! dumpster) and the destitute ([`CITY_POOR`]) inner-city kit (brick
//! tenement, corner store, trash bags).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! [`glass`] curtain-wall facades, board-formed [`concrete`], brushed
//! [`steel`], smooth painted [`enamel`], and red [`brick`]. Lit windows,
//! street lamps, and signal heads glow, and the
//! skyscraper vents rooftop steam over a low traffic hum from [`fx`]. The
//! theme's smoggy grey accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod dumpster;
pub mod gateway;
pub mod glass_skyscraper;
pub mod office_block;
pub mod parked_car;
pub mod parking_garage;
pub mod street_lamp;
pub mod traffic_light;
pub mod transit_stop;
// Poor (inner-city) variants — the prosperity-Poor end of the theme.
pub mod corner_store;
pub mod tenement;
pub mod trash_bags;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use super::util::{tile, tiles_per_metre};
use crate::catalogue::items::util::{cuboid_tapered, id_quat, prim};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established downtown kit — glass towers
/// and clean concrete read as a Modest-to-Rich district. The poor end is
/// the separate inner-city kit ([`tenement`], …), tagged `Poor`.
pub(super) const CITY_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the inner-city kit — the destitute end of the theme,
/// never picked for a modest or affluent room.
pub(super) const CITY_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Curtain-wall glass — the lit facade of a tower or office. Clean panes
/// with a faint inner glow (`glow` sets the lit-window bloom); a building
/// reads as glowing glass rather than a black slab.
/// Glazing. `uv_scale` stays `1.0`: the `Window` generator is an alpha card
/// and must span its quad exactly once (see
/// [`window_card`](super::util::window_card)). It also belongs on a `Plane`
/// with `UvMapping::Fit`, not pinned to the face of a solid.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.6),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 4,
            panes_y: 5,
            glass_opacity: Fp64(0.45),
            grime_level: Fp64(0.08),
            color_frame: Fp3([0.18, 0.19, 0.22]),
            ..Default::default()
        }),
    }
}

/// Board-formed concrete — parking decks, cores, plinths, planters.
pub(super) fn concrete(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: tiles_per_metre(tile::CONCRETE),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            formwork_lines: Fp64(5.0),
            formwork_depth: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Brushed structural steel — mullions, poles, canopies, railings.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.85),
        uv_scale: tiles_per_metre(tile::METAL),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.30, 0.20, 0.12]),
            roughness: Fp64(0.4),
            metallic: Fp(0.85),
            rust_level: Fp64(0.04),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Smooth painted enamel — car bodies, dumpsters, signal housings, shelter
/// frames. Glossy automotive finish.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.5),
        uv_scale: tiles_per_metre(tile::METAL),
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

/// Red brick — the body of tenements, corner stores, office bases.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: tiles_per_metre(tile::BRICK),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.72, 0.70, 0.66]),
            scale: Fp64(5.0),
            cell_variance: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A glazed curtain-wall façade — the crisp downtown glazing signature. A lit
/// glass panel gridded by proud steel mullions (verticals) and spandrel
/// transoms (horizontals) so the face reads as a true window grid rather than
/// a flat lit slab. The glass sits in an XY plane centred on `center`; the grid
/// stands `proud` beyond it along Z, the sign choosing the face (negative =
/// toward the −Z render front, so nothing is left coplanar). Returns the panel
/// plus its grid as a flat list to splice into an [`assemble`] vec.
///
/// `bays` is `(cols, rows)` of glazing; the grid draws `cols + 1` verticals and
/// `rows + 1` transoms. Reusable across the kit's glazed buildings (tower,
/// office, storefront).
///
/// The glass panel is a slab, not a `Window`-card plane — a known limitation
/// shared by every caller (#942). Migrating it is per-item work (each façade
/// needs an interior to reveal and its pane grid re-checked against `bays`),
/// so it is done as those items come up for review rather than in one sweep.
///
/// [`assemble`]: super::util::assemble
pub(super) fn curtain_wall(
    center: [f32; 3],
    size: [f32; 2],
    bays: (u32, u32),
    proud: f32,
    glass_mat: SovereignMaterialSettings,
    mullion_mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let [w, h] = size;
    let (cols, rows) = bays;
    let bar = 0.16_f32; // mullion / transom face width
    let depth = proud.abs().max(0.18); // how far the fins stand off the glass
    let grid_z = cz + proud;
    let mut out = vec![
        // Lit glass infill panel.
        prim(
            cuboid_tapered([w, h, 0.3], 0.0, glass_mat),
            [cx, cy, cz],
            id_quat(),
        ),
    ];
    // Vertical mullions dividing the bays.
    for i in 0..=cols {
        let x = cx - w * 0.5 + w * (i as f32 / cols as f32);
        out.push(prim(
            cuboid_tapered([bar, h + bar, depth], 0.0, mullion_mat.clone()),
            [x, cy, grid_z],
            id_quat(),
        ));
    }
    // Horizontal spandrel transoms.
    for j in 0..=rows {
        let y = cy - h * 0.5 + h * (j as f32 / rows as f32);
        out.push(prim(
            cuboid_tapered([w + bar, bar, depth], 0.0, mullion_mat.clone()),
            [cx, y, grid_z],
            id_quat(),
        ));
    }
    out
}

// Glass + concrete palette.
pub(super) const GLASS_BLUE: [f32; 3] = [0.34, 0.46, 0.58];
pub(super) const GLASS_TEAL: [f32; 3] = [0.30, 0.52, 0.52];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.56, 0.56, 0.57];
pub(super) const STEEL_GREY: [f32; 3] = [0.55, 0.57, 0.60];
pub(super) const BRICK_RED: [f32; 3] = [0.48, 0.25, 0.19];

// Street-furniture colours.
pub(super) const CAR_BODY: [f32; 3] = [0.62, 0.16, 0.14];
pub(super) const CAR_GLASS: [f32; 3] = [0.10, 0.12, 0.14];
pub(super) const DUMPSTER_GREEN: [f32; 3] = [0.16, 0.32, 0.22];
pub(super) const TIRE_BLACK: [f32; 3] = [0.06, 0.06, 0.07];

// Emissive trim colours.
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.86, 0.58];
pub(super) const SIGNAL_GREEN: [f32; 3] = [0.20, 0.95, 0.35];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (inner-city) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &tenement::Tenement,
            &corner_store::CornerStore,
            &trash_bags::TrashBags,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The traffic light is the kit's lit hero — it must keep its emissive
    /// signal so escalation's broken-emissive ruin pass has something to kill.
    #[test]
    fn traffic_light_keeps_its_signal() {
        assert!(
            crate::catalogue::items::util::has_emissive(&traffic_light::TrafficLight.build("")),
            "traffic light lost its emissive signal"
        );
    }
}
