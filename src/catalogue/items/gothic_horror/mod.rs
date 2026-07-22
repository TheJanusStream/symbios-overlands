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
pub mod gateway;
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
        uv_scale: Fp(1.0),
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

// ---------------------------------------------------------------------------
// Gothic geometry vocabulary — the pointed-arch language shared across the kit.
//
// The defining Gothic move is the two-centred *pointed* arch; the round
// (Romanesque) half-torus reads as the wrong era. These helpers build the
// genuine two-centred construction from the cut toolkit so cathedral, tower,
// crypt and ruin all speak the same silhouette.
// ---------------------------------------------------------------------------

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, id_quat, prim, quat_x, quat_z, solid, torus, with_cut,
};
use crate::pds::Generator;

/// The two stone arcs of a Gothic equilateral pointed arch, standing in the wall
/// (XY) plane and meeting at an apex `half_span * √3` above the springline.
///
/// Built by the real two-centred construction: each side is an arc of a circle
/// of radius `2 * half_span` centred on the *opposite* springer, cut from a
/// torus with `path_cut` and stood upright with `quat_x(-FRAC_PI_2)` (the
/// semicircle recipe, but only the 60° apex-ward sixth of each ring). Returns
/// the two arcs ready to drop into an [`assemble`](crate::catalogue::items::util::assemble)
/// list — never as the root, since they carry a rotation. `spring` is the
/// springline-midpoint world position (its Z the wall face), `thick` the rib's
/// round cross-section.
pub(super) fn pointed_arch(
    spring: [f32; 3],
    half_span: f32,
    thick: f32,
    mat: SovereignMaterialSettings,
) -> [Generator; 2] {
    let [cx, cy, zf] = spring;
    let r = 2.0 * half_span;
    let right = prim(
        with_cut(
            torus(thick, r, mat.clone()),
            [0.0, 1.0 / 6.0],
            [0.0, 1.0],
            0.0,
        ),
        [cx - half_span, cy, zf],
        quat_x(-FRAC_PI_2),
    );
    let left = prim(
        with_cut(torus(thick, r, mat), [1.0 / 3.0, 0.5], [0.0, 1.0], 0.0),
        [cx + half_span, cy, zf],
        quat_x(-FRAC_PI_2),
    );
    [right, left]
}

/// A lit Gothic lancet window: a glowing leaded light framed by two jamb ribs
/// and a central mullion, capped by a [`pointed_arch`], on a small sill ledge.
/// `cx`/`sill`/`zf` place the sill-centre on the wall face (`zf` sign picks the
/// proud/recess direction, so it works on either the −Z or +Z wall); `half_w`
/// the half opening width, `body_h` the straight light height below the
/// springline. The glass is emissive (`glow_str`) so the ruin pass has light to
/// snuff. Returns every piece for an `assemble` list.
pub(super) fn lancet(
    cx: f32,
    sill: f32,
    zf: f32,
    half_w: f32,
    body_h: f32,
    glow_str: f32,
) -> Vec<Generator> {
    let n = if zf < 0.0 { -1.0_f32 } else { 1.0 }; // outward normal sign
    let spring_y = sill + body_h;
    let rib_z = zf + 0.04 * n; // frame stands proud of the wall face
    let glass_z = zf - 0.04 * n; // glass set back into the opening
    let glass_h = body_h + half_w * 0.55;
    let mut v = vec![
        // Glowing leaded light, set into the opening.
        prim(
            cuboid_tapered(
                [half_w * 1.7, glass_h, 0.1],
                0.0,
                stained(STAINED_TINT, glow_str),
            ),
            [cx, sill + glass_h * 0.5, glass_z],
            id_quat(),
        ),
        // Sill ledge.
        prim(
            solid(cuboid_tapered(
                [half_w * 2.0 + 0.24, 0.14, 0.3],
                0.0,
                stone(STONE_DARK),
            )),
            [cx, sill - 0.02, rib_z],
            id_quat(),
        ),
        // Central mullion dividing the light into two.
        prim(
            cuboid_tapered([0.09, body_h + half_w * 0.9, 0.2], 0.0, stone(STONE_DARK)),
            [cx, sill + (body_h + half_w * 0.9) * 0.5, rib_z],
            id_quat(),
        ),
    ];
    // Two jamb ribs.
    for s in [-1.0_f32, 1.0] {
        v.push(prim(
            cuboid_tapered([0.11, body_h, 0.22], 0.0, stone(STONE_DARK)),
            [cx + s * half_w, sill + body_h * 0.5, rib_z],
            id_quat(),
        ));
    }
    // Pointed-arch head.
    v.extend(pointed_arch(
        [cx, spring_y, rib_z],
        half_w,
        0.1,
        stone(STONE_DARK),
    ));
    v
}

/// A Gothic broach spire: an octagonal stone needle with a flared base band, two
/// climbing ranks of corner crockets and an apex finial — the bristly soaring
/// silhouette a plain cone never gives. `base` is the spire foot (it rises +Y);
/// `r` the foot radius, `h` the height. Returns every piece.
pub(super) fn spire(
    base: [f32; 3],
    r: f32,
    h: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let [bx, by, bz] = base;
    let mut v = vec![
        // Octagonal needle.
        prim(
            solid(cone(r, h, 8, mat.clone())),
            [bx, by + h * 0.5, bz],
            id_quat(),
        ),
        // Flared base band masking the tower-to-spire join.
        prim(
            solid(torus(r * 0.16, r * 0.95, mat.clone())),
            [bx, by + 0.06, bz],
            quat_x(FRAC_PI_2),
        ),
        // Apex finial.
        prim(
            solid(cone(r * 0.24, h * 0.3, 6, mat.clone())),
            [bx, by + h + h * 0.1, bz],
            id_quat(),
        ),
    ];
    // Crockets climbing two opposite edges.
    for k in 0..3 {
        let t = 0.22 + k as f32 * 0.24;
        let cy = by + h * t;
        let cr = r * (1.0 - t) * 0.92;
        for s in [-1.0_f32, 1.0] {
            v.push(prim(
                solid(cone(r * 0.13, r * 0.34, 5, mat.clone())),
                [bx + s * cr, cy, bz],
                quat_z(-s * 1.15),
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
            crate::catalogue::items::util::has_emissive(&cathedral::Cathedral.build("")),
            "cathedral lost its emissive stained glass"
        );
    }
}
