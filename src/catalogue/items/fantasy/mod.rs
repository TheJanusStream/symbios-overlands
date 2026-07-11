//! High-Fantasy-theme catalogue structures — an arcane quarter of wizardry
//! and fae magic, aglow with mana.
//!
//! Two prosperity registers share one enchanted identity: the established
//! ([`FANTASY_BAND`]) high-magic kit (a wizard tower, an enchanted library, a
//! fae ring, a crystal shrine, a floating runestone, glowing mushrooms, a
//! spell circle, a mana font and a crystal cluster) and the destitute
//! ([`FANTASY_POOR`]) hedge-magic kit (a hedge-witch's hut, a leaning standing
//! stone, a humble toadstool ring).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! dressed [`stone`] ashlar towers, [`mossy`] old cobble, dark [`timber`]
//! beams, golden [`thatch`], arcane lit [`glass`] and [`gold`] trim, with
//! crystals, runes and motes carried by [`crate::catalogue::items::util::glow`]
//! emissive trim and the [`fx`] particle/audio kit. The theme's magic-mote
//! accent lives in [`crate::seeded_defaults::room::accent`].

pub mod crystal_cluster;
pub mod crystal_shrine;
pub mod enchanted_library;
pub mod fae_ring;
pub mod gateway;
pub mod glow_mushroom;
pub mod mana_font;
pub mod runestone;
pub mod spell_circle;
pub mod wizard_tower;
// Poor (hedge-magic) variants — the prosperity-Poor end of the theme.
pub mod hedge_hut;
pub mod standing_stone;
pub mod toadstool_ring;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, id_quat, prim, prim_scaled, quat_z, solid, sphere,
    with_cut,
};
use crate::pds::{
    Fp, Fp3, Fp4, Fp64, Generator, SovereignAshlarConfig, SovereignCobblestoneConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig,
    SovereignThatchConfig, SovereignWindowConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the high-magic kit — wizard towers and crystal
/// shrines read as a Modest-to-Rich arcane seat. The poor end of the theme is
/// the separate hedge-magic kit ([`hedge_hut`], …), tagged `Poor`, so a
/// destitute fantasy room grows the hedge-witch's holding instead.
pub(super) const FANTASY_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the hedge-magic kit — the destitute end of the theme,
/// never picked for a modest or affluent fantasy room.
pub(super) const FANTASY_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Dressed ashlar stone — wizard tower, library and shrine masonry.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 0.7, color[1] * 0.7, color[2] * 0.68]),
            rows: 4,
            cols: 4,
            chisel_depth: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Mossy old cobble — standing stones, shrine footings, weathered bases.
pub(super) fn mossy(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.55, color[1] * 0.65, color[2] * 0.42]),
            roundness: Fp64(1.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dark timber — beams, lintels, the hedge hut's frame.
pub(super) fn timber(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.3, color[1] * 1.3, color[2] * 1.28]),
            color_wood_dark: Fp3([color[0] * 0.6, color[1] * 0.6, color[2] * 0.56]),
            plank_count: Fp64(5.0),
            knot_density: Fp64(0.3),
            grain_warp: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Golden thatch — the wizard-tower cap underlay and the hedge-hut roof.
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
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Arcane lit glass — the glowing windows of the tower and library. A faint
/// inner glow (`glow`) so the panes read as enchanted rather than dark.
pub(super) fn glass(tint: [f32; 3], glow: f32) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(tint),
        emission_color: Fp3(tint),
        emission_strength: Fp(glow),
        roughness: Fp(0.12),
        metallic: Fp(0.2),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Window(SovereignWindowConfig {
            panes_x: 2,
            panes_y: 4,
            glass_opacity: Fp64(0.4),
            grime_level: Fp64(0.06),
            color_frame: Fp3([0.4, 0.34, 0.16]),
            ..Default::default()
        }),
    }
}

/// Polished gold — finials, rune inlays, shrine fittings.
pub(super) fn gold(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.25),
        metallic: Fp(0.95),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.4, 0.3, 0.12]),
            roughness: Fp64(0.25),
            metallic: Fp(0.95),
            rust_level: Fp64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Flat matte colour — daub plaster, toadstool caps, charms. A plain
/// surface with no procedural texture.
pub(super) fn matte(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Fantasy-signature geometry helpers
// ---------------------------------------------------------------------------

/// A faceted crystal shard — a hexagonal prism shaft terminated by a
/// pyramidal point, the gem read a smooth cone never gives (a cone is an
/// ice-cream scoop; six flat facets catch the light as a crystal). Returned
/// as one positioned subtree (the shaft is its local root), so the whole
/// shard leans on a single `tilt` quaternion as a *child* of the item — never
/// the assemble root, so the rotation is safe. `foot` is the base-centre
/// world position, `r` the shaft radius, `h` the total height (~⅗ shaft,
/// ~⅖ point). Pass a [`glow`](crate::catalogue::items::util::glow) material
/// for the luminous arcane crystals.
pub(super) fn crystal(
    foot: [f32; 3],
    r: f32,
    h: f32,
    tilt: Fp4,
    mat: SovereignMaterialSettings,
) -> Generator {
    let body_h = h * 0.6;
    let tip_h = h * 0.4;
    // Hexagonal prism shaft — the subtree root, carrying the lean.
    let mut shaft = prim(
        cylinder_tapered(r, body_h, 6, 0.14, mat.clone()),
        [foot[0], foot[1] + body_h * 0.5, foot[2]],
        tilt,
    );
    // Pyramidal terminated point, in the shaft's local frame.
    shaft.children.push(prim(
        cone(r * 0.9, tip_h, 6, mat),
        [0.0, body_h * 0.5 + tip_h * 0.5, 0.0],
        id_quat(),
    ));
    shaft
}

/// A toadstool — a pale stem under a domed parasol cap, the fantasy mushroom
/// signature (a *dome* reads as a mushroom where a cone reads as a fir tree).
/// Returned as one positioned subtree (the stem is its local root). `foot` is
/// the stem-base world position, `scale` sizes the whole stool. `cap_mat`
/// glows for the luminous variety; `spots` studs the cap with pale flecks for
/// the storybook red toadstool. A darker gill ring tucks under the cap rim so
/// the parasol reads round.
pub(super) fn toadstool(
    foot: [f32; 3],
    scale: f32,
    cap_mat: SovereignMaterialSettings,
    spots: bool,
) -> Generator {
    let stem_h = 0.6 * scale;
    let cap_r = 0.42 * scale;
    // Stem — the subtree root; centred so its top sits at +stem_h/2.
    let mut stem = prim(
        solid(cylinder_tapered(
            0.1 * scale,
            stem_h,
            8,
            0.2,
            matte([0.86, 0.85, 0.78]),
        )),
        [foot[0], foot[1] + stem_h * 0.5, foot[2]],
        id_quat(),
    );
    let cap_y = stem_h * 0.5 - 0.03 * scale; // skirt overhangs the stem top
    // Domed parasol cap — a flattened upper hemisphere.
    stem.children.push(prim_scaled(
        with_cut(sphere(cap_r, 6, cap_mat), [0.0, 1.0], [0.5, 1.0], 0.0),
        [0.0, cap_y, 0.0],
        id_quat(),
        [1.0, 0.6, 1.0],
    ));
    // Dark gill ring tucked just under the cap edge.
    stem.children.push(prim(
        cone(cap_r * 0.85, 0.14 * scale, 8, matte([0.34, 0.3, 0.28])),
        [0.0, cap_y - 0.04 * scale, 0.0],
        quat_z(std::f32::consts::PI),
    ));
    if spots {
        for i in 0..4 {
            let a = i as f32 / 4.0 * std::f32::consts::TAU + 0.4;
            stem.children.push(prim(
                sphere(0.07 * scale, 4, matte([0.92, 0.9, 0.84])),
                [
                    a.cos() * cap_r * 0.5,
                    cap_y + 0.16 * scale,
                    a.sin() * cap_r * 0.5,
                ],
                id_quat(),
            ));
        }
    }
    stem
}

/// A cluster of glowing rune strokes standing proud of a stone face — a
/// central stave crossed by a couple of angled branches, the Elder-Futhark
/// look. Thin, saturated strokes on dark stone *read*, where a single flat
/// glowing panel over-brightens and washes to a pale blank. `center` is the
/// face-centre world position (its Z already nudged proud of the wall), `h`
/// the rune height. Returns the strokes for an `assemble` list.
pub(super) fn rune_marks(
    center: [f32; 3],
    h: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let t = h * 0.13; // stroke thickness
    let mut v = vec![
        // Central stave.
        prim(
            cuboid_tapered([t, h, t * 0.7], 0.0, mat.clone()),
            [cx, cy, cz],
            id_quat(),
        ),
    ];
    // Two angled branches off the stave (the ᚠ / ᚱ look).
    for (dy, s) in [(h * 0.26, 1.0_f32), (-h * 0.06, 1.0_f32)] {
        v.push(prim(
            cuboid_tapered([h * 0.42, t, t * 0.7], 0.0, mat.clone()),
            [cx + h * 0.2 * s, cy + dy, cz],
            quat_z(-0.55 * s),
        ));
    }
    v
}

// Masonry + timber palette.
pub(super) const STONE_GREY: [f32; 3] = [0.56, 0.55, 0.52];
pub(super) const STONE_MOSS: [f32; 3] = [0.44, 0.48, 0.38];
pub(super) const TIMBER_DARK: [f32; 3] = [0.32, 0.24, 0.16];
pub(super) const THATCH_STRAW: [f32; 3] = [0.54, 0.45, 0.26];
pub(super) const GOLD: [f32; 3] = [0.80, 0.66, 0.30];
pub(super) const ARCANE_GLASS: [f32; 3] = [0.52, 0.42, 0.80];

// Emissive magic colours. Deeply saturated on purpose: `glow` sets both
// base_color and emission_color to these, and a too-pale colour over-brightens
// and washes to a near-white blank under bloom (the steampunk over-bright-clips
// lesson). Deep base hues hold their colour when driven emissive.
pub(super) const ARCANE_PURPLE: [f32; 3] = [0.70, 0.42, 1.0];
pub(super) const CRYSTAL_CYAN: [f32; 3] = [0.18, 0.80, 1.0];
pub(super) const RUNE_GOLD: [f32; 3] = [1.0, 0.82, 0.42];
pub(super) const MANA_TEAL: [f32; 3] = [0.20, 0.95, 0.78];
pub(super) const MUSH_GLOW: [f32; 3] = [0.28, 0.86, 0.50];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (hedge-magic) variants must build clean trees the
    /// sanitiser leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &hedge_hut::HedgeHut,
            &standing_stone::StandingStone,
            &toadstool_ring::ToadstoolRing,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The wizard tower is the kit's lit hero — it must keep its emissive
    /// windows and crystal orb so escalation's broken-emissive ruin pass has
    /// magic to snuff.
    #[test]
    fn wizard_tower_keeps_its_glow() {
        assert!(
            crate::catalogue::items::util::has_emissive(&wizard_tower::WizardTower.build("")),
            "wizard tower lost its emissive windows / orb"
        );
    }
}
