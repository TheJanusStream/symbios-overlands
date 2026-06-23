//! Sports / Recreation-theme catalogue structures — a stadium complex and
//! its training grounds.
//!
//! Two prosperity registers share one sporting identity: the established
//! ([`SPORTS_BAND`]) stadium kit (the bowl, a gym, open bleachers, a ticket
//! booth, a clubhouse, goalposts, floodlight masts, a scoreboard and a team
//! bench) and the destitute ([`SPORTS_POOR`]) rec-ground kit (a cracked
//! court, a chain-link backstop, a stack of training tyres).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! board-formed [`concrete`] stands, brushed [`steel`] masts and frames,
//! glossy [`enamel`] seats and panels, lit [`glass`] gym glazing, rusting
//! [`corrugated`] cladding, woven [`chainlink`] fencing, cracked [`asphalt`]
//! courts, mown [`turf`] pitches and flat [`painted`] line markings. The
//! scoreboard and floodlights glow over a crowd-murmur and tannoy bed from
//! [`fx`]. The theme's bright field-day accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod bleachers;
pub mod clubhouse;
pub mod floodlight_mast;
pub mod goalpost;
pub mod gym;
pub mod players_bench;
pub mod scoreboard;
pub mod stadium;
pub mod ticket_booth;
// Poor (rec-ground) variants — the prosperity-Poor end of the theme.
pub mod backstop;
pub mod rec_court;
pub mod tire_stack;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{cuboid_tapered, glow, id_quat, prim, solid};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignAsphaltConfig, SovereignChainLinkConfig,
    SovereignConcreteConfig, SovereignCorrugatedConfig, SovereignMaterialSettings,
    SovereignMetalConfig, SovereignTextureConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established stadium — a working ground
/// reads as a Modest-to-Rich complex. The poor end of the theme is the
/// separate rec-ground kit ([`rec_court`], …), tagged `Poor`, so a destitute
/// sports room grows the cracked municipal court instead.
pub(super) const SPORTS_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the rec-ground kit — the destitute end of the theme,
/// never picked for a modest or affluent sports room.
pub(super) const SPORTS_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Board-formed concrete — stand structure, plinths, courts, the gym base.
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

/// Brushed structural steel — floodlight masts, goalposts, frames, railings.
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
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Glossy painted enamel — seat banks, scoreboard housings, panels, the
/// hoop backboard. Smooth coloured finish.
pub(super) fn enamel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.4),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.3, 0.18, 0.1]),
            seam_count: Fp64(1.0),
            roughness: Fp64(0.25),
            metallic: Fp(0.4),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Lit gym / clubhouse / booth glass — a faint inner glow (`glow`) so the
/// glazing reads as lit rather than black.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.15),
        metallic: Fp(0.4),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 5,
            panes_y: 2,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.1),
            color_frame: Fp3([0.3, 0.31, 0.34]),
            ..Default::default()
        }),
    }
}

/// Rusting corrugated metal — gym cladding, dugout and stand roofs.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.7),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(10.0),
            rust_level: Fp64(0.18),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Woven chain-link — perimeter fencing and the backstop.
pub(super) fn chainlink(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.55),
        metallic: Fp(0.7),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::ChainLink(SovereignChainLinkConfig {
            color_wire: Fp3(color),
            cell_count: Fp64(8.0),
            rust_level: Fp64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cracked asphalt — the poor rec-court surface.
pub(super) fn asphalt(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Asphalt(SovereignAsphaltConfig {
            color_base: Fp3(color),
            color_aggregate: Fp3([0.35, 0.33, 0.30]),
            stain_level: Fp64(0.3),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte paint — the mown pitch, line markings, court colour, painted
/// trim. A plain coloured surface with no procedural texture.
pub(super) fn painted(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Mown grass — the pitch and the field, a soft matte green.
pub(super) fn turf(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        uv_scale: Fp(4.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Structure palette.
pub(super) const CONCRETE_GREY: [f32; 3] = [0.62, 0.61, 0.59];
pub(super) const STEEL_GREY: [f32; 3] = [0.52, 0.54, 0.57];
pub(super) const CORRUGATED_GREY: [f32; 3] = [0.60, 0.62, 0.64];
pub(super) const CHAIN_GREY: [f32; 3] = [0.62, 0.64, 0.66];
pub(super) const ASPHALT_DARK: [f32; 3] = [0.10, 0.10, 0.11];
pub(super) const PITCH_GREEN: [f32; 3] = [0.22, 0.42, 0.20];
pub(super) const COURT_BLUE: [f32; 3] = [0.20, 0.36, 0.50];
pub(super) const LINE_WHITE: [f32; 3] = [0.92, 0.92, 0.88];
pub(super) const SEAT_BLUE: [f32; 3] = [0.18, 0.34, 0.58];
pub(super) const SEAT_RED: [f32; 3] = [0.62, 0.18, 0.16];
pub(super) const GLASS_TINT: [f32; 3] = [0.40, 0.50, 0.54];
pub(super) const HOOP_ORANGE: [f32; 3] = [0.92, 0.42, 0.10];

// Emissive trim colours.
pub(super) const FLOOD_LIT: [f32; 3] = [1.0, 0.97, 0.90];
/// Deep-saturated amber for the segmented lit display cells — a single broad
/// flat lit panel at a brighter amber blooms to a near-white slab, so the
/// score/clock cells use this deeper amber at a lower strength and let the dark
/// gaps between them carry the "segmented board" read.
pub(super) const SCORE_LIT: [f32; 3] = [1.0, 0.48, 0.10];
/// Saturated red for indicator lamps / period marks on a display.
pub(super) const SCORE_RED: [f32; 3] = [1.0, 0.22, 0.12];

/// A segmented, lit scoreboard display face built proud of a dark bezel: a
/// narrow clock strip across the top, two deep-saturated amber score panels
/// split by a dark central gap, and a row of small indicator lamps below. The
/// dark gaps between the lit cells keep the board reading as a *segmented
/// display* instead of the solid lit slab that blooms to a flat white
/// rectangle. `cx,cy` centre the face; `face_z` is the world Z of the lit
/// cells (the caller's dark housing sits just behind); `w,h` are the display
/// extents.
pub(super) fn score_display(cx: f32, cy: f32, face_z: f32, w: f32, h: f32) -> Vec<Generator> {
    let mut out = vec![
        // Clock / time strip across the top.
        prim(
            cuboid_tapered([w * 0.74, h * 0.18, 0.08], 0.0, glow(SCORE_LIT, 2.2)),
            [cx, cy + h * 0.30, face_z],
            id_quat(),
        ),
    ];
    // Two big score panels split by a dark central gap.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([w * 0.40, h * 0.42, 0.08], 0.0, glow(SCORE_LIT, 2.2)),
            [cx + sx * w * 0.22, cy - h * 0.06, face_z],
            id_quat(),
        ));
    }
    // Row of small indicator lamps along the bottom.
    for i in 0..5 {
        let fx = (i as f32 - 2.0) * 0.16;
        let col = if i % 2 == 0 { SCORE_RED } else { SCORE_LIT };
        out.push(prim(
            cuboid_tapered([w * 0.07, h * 0.10, 0.08], 0.0, glow(col, 2.0)),
            [cx + fx * w, cy - h * 0.38, face_z],
            id_quat(),
        ));
    }
    out
}

/// A floodlight lamp bank: a dark backing frame carrying a grid of small
/// warm-white lit cells. The gridded cells (with dark gaps between them) read
/// as an array of lamps and keep the bank from blooming into one flat white
/// rectangle the way a single lit panel does. `center` is the lit-cell face;
/// `w,h` the bank size; `cols,rows` the lamp grid; `face` is the look
/// direction (`+1` = cells face `+Z`, backing toward `-Z`).
pub(super) fn lamp_bank(
    center: [f32; 3],
    w: f32,
    h: f32,
    cols: u32,
    rows: u32,
    face: f32,
) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let mut out = vec![
        // Dark backing frame, set behind the lamps.
        prim(
            solid(cuboid_tapered(
                [w, h, 0.16],
                0.0,
                enamel([0.10, 0.10, 0.12]),
            )),
            [cx, cy, cz - 0.14 * face],
            id_quat(),
        ),
    ];
    let cw = w / cols as f32 * 0.72;
    let ch = h / rows as f32 * 0.72;
    for r in 0..rows {
        for c in 0..cols {
            let fx = (c as f32 + 0.5) / cols as f32 - 0.5;
            let fy = (r as f32 + 0.5) / rows as f32 - 0.5;
            out.push(prim(
                cuboid_tapered([cw, ch, 0.06], 0.0, glow(FLOOD_LIT, 3.0)),
                [cx + fx * w, cy + fy * h, cz],
                id_quat(),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (rec-ground) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &rec_court::RecCourt,
            &backstop::Backstop,
            &tire_stack::TireStack,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The stadium is the kit's lit hero — it must keep its emissive
    /// floodlights and scoreboard so escalation's broken-emissive ruin pass
    /// has lights to snuff.
    #[test]
    fn stadium_keeps_its_lights() {
        assert!(
            crate::catalogue::items::util::has_emissive(&stadium::Stadium.build("")),
            "stadium lost its emissive floodlights / scoreboard"
        );
    }
}
