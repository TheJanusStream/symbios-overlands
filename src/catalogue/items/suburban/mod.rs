//! Suburban-theme catalogue structures — a quiet residential street.
//!
//! Two prosperity registers share one identity: the established
//! ([`SUB_BAND`]) neighbourhood kit (community center, family house,
//! detached garage, mini-mart, picket fence, mailbox, minivan, swing set)
//! and the destitute ([`SUB_POOR`]) trailer-lot kit (trailer home, metal
//! carport, yard junk).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! lap [`siding`] and [`wood`] plank, asphalt [`shingle`] roofs, [`brick`]
//! and rendered [`render`] walls, [`glass`] windows, smooth painted
//! [`enamel`], and clipped hedges and shrubs. Porch lights and shop signs glow,
//! a backyard sprinkler mists the lawn, and birdsong drifts over the street
//! from [`fx`]. The theme's soft sunny accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod community_center;
pub mod detached_garage;
pub mod mailbox;
pub mod mini_mart;
pub mod minivan;
pub mod picket_fence;
pub mod suburban_house;
pub mod swing_set;
// Poor (trailer-lot) variants — the prosperity-Poor end of the theme.
pub mod carport;
pub mod trailer_home;
pub mod yard_junk;

pub mod fx;

use std::f32::consts::FRAC_PI_2;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_z, solid,
};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignBrickConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignPlankConfig, SovereignShingleConfig, SovereignStuccoConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established neighbourhood kit — tidy
/// houses and lawns read as a Modest-to-Rich suburb. The poor end is the
/// separate trailer-lot kit ([`trailer_home`], …), tagged `Poor`.
pub(super) const SUB_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the trailer-lot kit — the destitute end of the theme,
/// never picked for a modest or affluent room.
pub(super) const SUB_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Vinyl lap siding — the body of a house, garage, or trailer. Fine
/// horizontal courses with little grain so it reads as siding, not raw plank.
pub(super) fn siding(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.08, color[1] * 1.08, color[2] * 1.08]),
            color_wood_dark: Fp3([color[0] * 0.85, color[1] * 0.85, color[2] * 0.85]),
            plank_count: Fp64(10.0),
            knot_density: Fp64(0.0),
            grain_warp: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted timber — fences, swing-set frames, porch posts, mailbox posts.
pub(super) fn wood(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.12, color[1] * 1.12, color[2] * 1.12]),
            color_wood_dark: Fp3([color[0] * 0.78, color[1] * 0.78, color[2] * 0.78]),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.15),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Asphalt-shingle roof — the pitched roofs of houses and the hall.
pub(super) fn shingle(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::Shingle(SovereignShingleConfig {
            color_tile: Fp3(color),
            color_grout: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.62]),
            scale: Fp64(6.0),
            shape_profile: Fp64(0.2),
            moss_level: Fp64(0.05),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Face brick — the community center, mini-mart, and house chimneys.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.80, 0.78, 0.74]),
            scale: Fp64(5.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted render / stucco — civic and shop walls.
pub(super) fn render(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
            color_base: Fp3(color),
            color_shadow: Fp3([color[0] * 0.85, color[1] * 0.85, color[2] * 0.83]),
            scale: Fp64(7.0),
            roughness: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Clean window glass — house and shop windows (`glow` lights them at dusk).
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.2),
        metallic: Fp(0.3),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.1),
            color_frame: Fp3([0.9, 0.9, 0.88]),
            ..Default::default()
        }),
    }
}

/// Smooth painted enamel — cars, bins, garage doors, mailboxes, carports.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.5),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.2, 0.12]),
            seam_count: Fp64(1.0),
            roughness: Fp64(0.3),
            metallic: Fp(0.5),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A little parked car: a chamfered body, a glazed cabin greenhouse, and four
/// round tyres (axle along X, so they read round from the side). `center` is
/// the ground point under the car; `body` is the paint colour. Shared by the
/// driveway car, the kerbside car, and the carport's tired old wreck so none
/// of them is the bare floating box it used to be.
pub(super) fn parked_car(center: [f32; 3], body: [f32; 3]) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let mut out = vec![
        // Lower body.
        prim(
            solid(cuboid_tapered([1.9, 0.7, 4.0], 0.08, enamel(body))),
            [cx, cy + 0.7, cz],
            id_quat(),
        ),
        // Cabin.
        prim(
            solid(cuboid_tapered([1.7, 0.62, 2.3], 0.18, enamel(body))),
            [cx - 0.1, cy + 1.25, cz],
            id_quat(),
        ),
        // Glazed greenhouse.
        prim(
            cuboid_tapered([1.62, 0.5, 2.32], 0.18, glass(GLASS_TINT, 0.0)),
            [cx - 0.1, cy + 1.25, cz],
            id_quat(),
        ),
    ];
    // Four round tyres, axle along X.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        out.push(prim(
            solid(cylinder_tapered(
                0.38,
                0.26,
                12,
                0.0,
                enamel([0.07, 0.07, 0.08]),
            )),
            [cx + sx * 0.95, cy + 0.38, cz + sz * 1.3],
            quat_z(FRAC_PI_2),
        ));
    }
    out
}

// Siding + roof palette.
pub(super) const SIDING_BLUE: [f32; 3] = [0.52, 0.64, 0.70];
pub(super) const SIDING_CREAM: [f32; 3] = [0.84, 0.80, 0.68];
pub(super) const SIDING_SAGE: [f32; 3] = [0.62, 0.68, 0.56];
pub(super) const ROOF_GREY: [f32; 3] = [0.26, 0.26, 0.29];
pub(super) const BRICK_TAN: [f32; 3] = [0.60, 0.44, 0.32];
pub(super) const RENDER_WHITE: [f32; 3] = [0.86, 0.85, 0.80];

// Garden + street palette.
pub(super) const HEDGE_GREEN: [f32; 3] = [0.22, 0.40, 0.18];
pub(super) const WOOD_WHITE: [f32; 3] = [0.88, 0.88, 0.84];
pub(super) const WOOD_BROWN: [f32; 3] = [0.44, 0.30, 0.18];
pub(super) const CAR_SILVER: [f32; 3] = [0.60, 0.62, 0.64];
pub(super) const TRAILER_WHITE: [f32; 3] = [0.80, 0.80, 0.77];
pub(super) const GLASS_TINT: [f32; 3] = [0.40, 0.50, 0.52];

// Emissive trim.
pub(super) const PORCH_WARM: [f32; 3] = [1.0, 0.84, 0.54];
pub(super) const SIGN_GLOW: [f32; 3] = [1.0, 0.74, 0.40];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (trailer-lot) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &trailer_home::TrailerHome,
            &carport::Carport,
            &yard_junk::YardJunk,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The community center is the kit's lit hero — it must keep its emissive
    /// sign so escalation's broken-emissive ruin pass has something to dim.
    #[test]
    fn community_center_keeps_its_sign() {
        assert!(
            crate::catalogue::items::util::has_emissive(
                &community_center::CommunityCenter.build("")
            ),
            "community center lost its emissive sign"
        );
    }
}
