//! Civic / Campus-theme catalogue structures — a dignified quarter of
//! municipal and university buildings around a green quad.
//!
//! Two prosperity registers share one institutional identity: the
//! established ([`CAMPUS_BAND`]) stone-and-brick campus (town hall, library,
//! lecture hall, dormitory, clock tower, a flagpole, a bike rack, a notice
//! board and a lamp) and the destitute ([`CAMPUS_POOR`]) underfunded kit
//! (a portable classroom, a worn bus shelter, a row of recycling bins).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! veined [`marble`] porticoes and steps, dressed [`stone`] ashlar walls,
//! red [`brick`] halls, board-formed [`concrete`], lit [`glass`] windows,
//! verdigris [`copper`] domes and roofs, brushed [`steel`] railings and
//! [`plank`] boards. The town-hall windows and lamps glow and the clock
//! tower keeps a lit face over a soft quad bed from [`fx`]. The theme's
//! warm sandstone accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod bike_rack;
pub mod campus_lamp;
pub mod clock_tower;
pub mod dormitory;
pub mod flagpole;
pub mod lecture_hall;
pub mod library;
pub mod notice_board;
pub mod town_hall;
// Poor (underfunded) variants — the prosperity-Poor end of the theme.
pub mod bus_shelter;
pub mod portable_classroom;
pub mod recycling_bins;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::pds::{
    Fp, Fp3, Fp64, SovereignAshlarConfig, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignMarbleConfig, SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig,
    SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established campus — stone halls and a
/// brick tower read as a Modest-to-Rich institution. The poor end of the
/// theme is the separate underfunded kit ([`portable_classroom`], …),
/// tagged `Poor`, so a destitute civic room grows the demountable lot.
pub(super) const CAMPUS_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the underfunded kit — the destitute end of the theme,
/// never picked for a modest or affluent civic room.
pub(super) const CAMPUS_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Veined polished marble — porticoes, columns, steps, plinths. The dressed
/// stone of the civic front, not a flat painted slab.
pub(super) fn marble(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.2),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Marble(SovereignMarbleConfig {
            color_base: Fp3(color),
            color_vein: Fp3([color[0] * 0.5, color[1] * 0.48, color[2] * 0.44]),
            vein_frequency: Fp64(3.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dressed ashlar stone — town-hall and library walls.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 1.15, color[1] * 1.15, color[2] * 1.12]),
            rows: 4,
            cols: 4,
            chisel_depth: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Red brick — dormitory, clock tower, lecture-hall base.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            color_mortar: Fp3([0.78, 0.76, 0.70]),
            scale: Fp64(5.0),
            cell_variance: Fp64(0.16),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Board-formed concrete — the modern lecture hall, steps, plinths.
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

/// Lit institutional glass — tall hall windows, dorm rooms, the entrance. A
/// faint inner glow (`glow`) so the panes read as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.4),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 3,
            panes_y: 4,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.08),
            color_frame: Fp3([0.88, 0.86, 0.82]),
            ..Default::default()
        }),
    }
}

/// Verdigris copper — the town-hall dome lantern and the clock-tower roof.
pub(super) fn copper(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.20, 0.42, 0.36]),
            roughness: Fp64(0.5),
            metallic: Fp(0.7),
            rust_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Brushed structural steel — railings, the bike rack, the flagpole, the
/// lamp column.
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
            rust_level: Fp64(0.06),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Sun-greyed plank — the notice board, the portable classroom, the bus
/// shelter bench.
pub(super) fn plank(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.18]),
            color_wood_dark: Fp3([color[0] * 0.62, color[1] * 0.6, color[2] * 0.56]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.2),
            grain_warp: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte paint — flags, sign faces, painted trim. A plain coloured
/// surface with no procedural texture.
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

// Masonry palette.
pub(super) const MARBLE_WHITE: [f32; 3] = [0.90, 0.88, 0.84];
pub(super) const STONE_PALE: [f32; 3] = [0.66, 0.63, 0.57];
pub(super) const BRICK_RED: [f32; 3] = [0.52, 0.26, 0.20];
pub(super) const CONCRETE_GREY: [f32; 3] = [0.62, 0.61, 0.59];
pub(super) const COPPER_VERDIGRIS: [f32; 3] = [0.30, 0.58, 0.50];
pub(super) const STEEL_GREY: [f32; 3] = [0.50, 0.52, 0.55];
pub(super) const PLANK_WOOD: [f32; 3] = [0.52, 0.40, 0.26];

// Accent + glass colours.
pub(super) const GLASS_TINT: [f32; 3] = [0.42, 0.50, 0.54];
pub(super) const FLAG_RED: [f32; 3] = [0.66, 0.16, 0.16];
pub(super) const NOTICE_GREEN: [f32; 3] = [0.18, 0.40, 0.26];

// Emissive trim colours.
pub(super) const WINDOW_WARM: [f32; 3] = [1.0, 0.92, 0.74];
pub(super) const LAMP_WARM: [f32; 3] = [1.0, 0.88, 0.6];
pub(super) const CLOCK_LIT: [f32; 3] = [1.0, 0.97, 0.86];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (underfunded) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &portable_classroom::PortableClassroom,
            &bus_shelter::BusShelter,
            &recycling_bins::RecyclingBins,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The town hall is the kit's lit hero — it must keep its emissive
    /// windows and lamps so escalation's broken-emissive ruin pass has
    /// lights to snuff.
    #[test]
    fn town_hall_keeps_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&town_hall::TownHall.build("")),
            "town hall lost its emissive windows / lamps"
        );
    }
}
