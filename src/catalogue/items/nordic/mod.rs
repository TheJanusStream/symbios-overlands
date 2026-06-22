//! Nordic / Viking-theme catalogue structures — a timber-and-thatch
//! mead-hall settlement on the cold-blue coast.
//!
//! Two prosperity registers share one Norse identity: the established
//! ([`NORDIC_BAND`]) carved-timber kit (mead hall, boathouse, signal
//! beacon, rune stones, longship, shield rack, drying rack, totem pole)
//! and the destitute ([`NORDIC_POOR`]) turf-and-sod croft (turf house,
//! sod shelter, wood pile).
//!
//! Surfaces use the real procedural generators rather than flat colour:
//! sawn [`timber`] plank walls, golden [`thatch`] and green [`turf`]
//! roofs, dressed [`stone`] ashlar footings and rune stones, glacial-
//! boulder [`rough_stone`], woven [`cloth`] shields and sails, riveted
//! [`iron`] fittings, and cut [`log_end`] firewood. The hearth and the
//! signal beacon come alive with small particle emitters and spatial
//! audio from [`fx`] (woodsmoke, leaping flame, drifting embers, fire
//! crackle, a low wind moan). The theme's cold-blue light accent lives in
//! [`crate::seeded_defaults::room::accent`].

pub mod boathouse;
pub mod drying_rack;
pub mod longship;
pub mod mead_hall;
pub mod rune_stones;
pub mod shield_rack;
pub mod signal_beacon;
pub mod totem_pole;
// Poor (croft) variants — the prosperity-Poor end of the theme.
pub mod sod_shelter;
pub mod turf_house;
pub mod wood_pile;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, prim, quat_y, quat_z,
    solid, sphere, torus,
};
use crate::pds::{
    Fp, Fp3, Fp4, Fp64, Generator, SovereignAshlarConfig, SovereignCobblestoneConfig,
    SovereignFabricConfig, SovereignLogEndConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPlankConfig, SovereignTextureConfig, SovereignThatchConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the established timber kit — carved halls
/// and longships read as a Modest-to-Rich steading. The poor end of the
/// theme is the separate turf-croft kit ([`turf_house`], …), tagged
/// `Poor`, so a destitute Nordic room grows the sod croft instead.
pub(super) const NORDIC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the turf-croft kit — the destitute end of the
/// theme, never picked for a modest or affluent Nordic room.
pub(super) const NORDIC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Sawn timber plank — the body of every Norse build: hall staves, posts,
/// gunwales, drying frames. Warm grain with knots so a wall reads as wood,
/// not a painted slab.
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

/// Golden straw thatch — the steep roof of a mead hall or boathouse.
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

/// Green sod / turf roof — overgrown straw read as living grass. The
/// roof and walls of the poor croft kit.
pub(super) fn turf(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(1.0),
        metallic: Fp(0.0),
        uv_scale: Fp(2.5),
        texture: SovereignTextureConfig::Thatch(SovereignThatchConfig {
            color_straw: Fp3(color),
            color_shadow: Fp3([color[0] * 0.4, color[1] * 0.45, color[2] * 0.3]),
            density: Fp64(18.0),
            anisotropy: Fp64(4.0),
            layer_count: Fp64(10.0),
            layer_shadow: Fp64(0.7),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Dressed ashlar stone — hall footings, rune stones, hearth surrounds.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig {
            color_stone: Fp3(color),
            color_mortar: Fp3([color[0] * 1.3, color[1] * 1.3, color[2] * 1.25]),
            rows: 3,
            cols: 3,
            chisel_depth: Fp64(0.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Glacial-boulder cobble — rough fieldstone for the beacon base and croft
/// footings, mud-packed.
pub(super) fn rough_stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.45, color[1] * 0.4, color[2] * 0.32]),
            roundness: Fp64(1.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Woven wool / linen cloth — painted round shields and the longship's
/// striped sail.
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

/// Riveted dark iron — shield bosses, brazier basket, weather-vane,
/// boat nails. Brushed with a little rust.
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

/// Cut log end-grain — the sawn faces of stacked firewood and post tops.
pub(super) fn log_end(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::LogEnd(SovereignLogEndConfig {
            color_early: Fp3([color[0] * 1.2, color[1] * 1.2, color[2] * 1.15]),
            color_late: Fp3(color),
            ..Default::default()
        }),
        ..Default::default()
    }
}

// Timber + thatch palette.
pub(super) const WOOD_WARM: [f32; 3] = [0.40, 0.27, 0.15];
pub(super) const WOOD_DARK: [f32; 3] = [0.28, 0.18, 0.10];
pub(super) const THATCH_STRAW: [f32; 3] = [0.60, 0.50, 0.26];
pub(super) const TURF_GREEN: [f32; 3] = [0.26, 0.36, 0.16];
pub(super) const STONE_GREY: [f32; 3] = [0.50, 0.50, 0.48];
pub(super) const STONE_COLD: [f32; 3] = [0.46, 0.49, 0.52];
pub(super) const IRON_DARK: [f32; 3] = [0.20, 0.21, 0.23];

// Painted-shield / sail colours.
pub(super) const SHIELD_RED: [f32; 3] = [0.58, 0.13, 0.11];
pub(super) const SHIELD_BLUE: [f32; 3] = [0.15, 0.26, 0.46];
pub(super) const SHIELD_GOLD: [f32; 3] = [0.74, 0.56, 0.18];
pub(super) const SHIELD_CREAM: [f32; 3] = [0.78, 0.72, 0.58];

/// Warm firelight for the beacon brazier and hall hearth glow.
pub(super) const FIRE_ORANGE: [f32; 3] = [1.0, 0.55, 0.18];

/// Cold rune-blue glint worked into carved dragon eyes (mead-hall finials,
/// longship prow/stern).
pub(super) const DRAGON_EYE: [f32; 3] = [0.5, 0.7, 0.95];

/// A round Norse shield — a painted woven disc with a rim ring and a
/// proud iron boss — placed at `center` with rotation `tilt` (a single
/// [`quat_x`](crate::catalogue::items::util::quat_x) of ±π/2 stands it
/// upright facing ±Z). The boss is authored in the disc's local frame, so
/// it follows the disc's tilt and always sits proud of the painted face.
/// Returns one [`Generator`] for an [`assemble`](crate::catalogue::items::util::assemble)
/// list.
pub(super) fn round_shield(
    center: [f32; 3],
    tilt: Fp4,
    face: [f32; 3],
    boss: [f32; 3],
) -> Generator {
    let weft = [face[0] * 0.65, face[1] * 0.65, face[2] * 0.65];
    let mut disc = prim(
        solid(cylinder_tapered(0.55, 0.12, 16, 0.0, cloth(face, weft))),
        center,
        tilt,
    );
    // Rim band around the edge, in the disc's local face plane.
    disc.children.push(prim(
        torus(0.06, 0.55, iron(boss)),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    // Iron boss, proud of the face along the disc's local +Y.
    disc.children.push(prim(
        solid(cylinder_tapered(0.13, 0.16, 10, 0.3, iron(boss))),
        [0.0, 0.1, 0.0],
        id_quat(),
    ));
    disc
}

/// A steep pitched gable roof — a triangular-prism ridge running the
/// building's length (X), the Z span pinched to a thin ridge cap. `size` is
/// `[length, rise, span]` measured at the eaves; place `center` at
/// `[0, wall_top + rise * 0.5, 0]`. The single tapered block the Nordic
/// halls used before pinched all four sides equally and read as a
/// flat-topped *mound*; pinching only Z gives a real A-frame the gable
/// triangles face `±X` and the long thatch slopes face `±Z`. Drop the node
/// into an [`assemble`](crate::catalogue::items::util::assemble) list; add a
/// ridge beam, bargeboards, and finials separately.
pub(super) fn gable_roof(
    size: [f32; 3],
    center: [f32; 3],
    mat: SovereignMaterialSettings,
) -> Generator {
    prim(
        solid(cuboid_tapered_xz(size, [0.0, 0.94], mat)),
        center,
        id_quat(),
    )
}

/// A rearing carved dragon / serpent head — the Norse signature finial.
/// Built facing `+X` (snout forward) on a neck rising from `foot`, returned
/// as one positioned subtree (the neck is its local root) so the whole head
/// rides a single [`quat_y`](crate::catalogue::items::util::quat_y)`(yaw)`:
/// `yaw = 0` faces `+X`, `yaw = PI` faces `-X`. `s` scales the head. Open
/// jaws, ridged crest, and glinting eyes carry the beast read a plain block
/// never did. Used on the longship's prow and stern and on the mead hall's
/// crossed gable bargeboards. It is always a *child* subtree of an item
/// (never the assemble root), so its rotation is safe.
pub(super) fn dragon_head(
    foot: [f32; 3],
    s: f32,
    yaw: f32,
    body: [f32; 3],
    eye: [f32; 3],
) -> Generator {
    // Neck — the subtree root, upright, carrying the yaw so head/jaw/crest/
    // eyes all turn with it. Its centre sits half its height above the foot.
    let mut neck = prim(
        solid(cuboid_tapered(
            [0.34 * s, 1.3 * s, 0.34 * s],
            0.25,
            timber(body),
        )),
        [foot[0], foot[1] + 0.65 * s, foot[2]],
        quat_y(yaw),
    );
    // Everything below is in the neck's local frame (origin at the neck
    // centre; the neck rises ±0.65·s in Y).
    // Skull block, set forward of the neck top.
    neck.children.push(prim(
        solid(cuboid_tapered(
            [0.5 * s, 0.46 * s, 0.42 * s],
            0.12,
            timber(body),
        )),
        [0.2 * s, 0.7 * s, 0.0],
        id_quat(),
    ));
    // Upper snout — a tapered muzzle jutting forward.
    neck.children.push(prim(
        solid(cuboid_tapered(
            [0.52 * s, 0.26 * s, 0.3 * s],
            0.5,
            timber(body),
        )),
        [0.55 * s, 0.74 * s, 0.0],
        id_quat(),
    ));
    // Lower jaw, dropped open for a gaping mouth.
    neck.children.push(prim(
        solid(cuboid_tapered(
            [0.42 * s, 0.15 * s, 0.26 * s],
            0.4,
            timber(body),
        )),
        [0.5 * s, 0.52 * s, 0.0],
        quat_z(0.24),
    ));
    // Glinting deep-set eyes, one to each side of the skull.
    for sz in [-1.0_f32, 1.0] {
        neck.children.push(prim(
            sphere(0.075 * s, 6, glow(eye, 2.0)),
            [0.22 * s, 0.84 * s, sz * 0.19 * s],
            id_quat(),
        ));
    }
    // Two horns swept up-and-forward off the brow.
    for sz in [-1.0_f32, 1.0] {
        neck.children.push(prim(
            cone(0.07 * s, 0.34 * s, 6, timber(body)),
            [0.16 * s, 0.96 * s, sz * 0.13 * s],
            quat_z(-0.3),
        ));
    }
    // Ridged crest / mane spikes leaning back down the neck.
    for (k, &cy) in [0.1_f32, 0.45, 0.78].iter().enumerate() {
        let _ = k;
        neck.children.push(prim(
            cone(0.085 * s, 0.32 * s, 6, timber(body)),
            [-0.16 * s, cy * s, 0.0],
            quat_z(0.55),
        ));
    }
    neck
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (croft) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &turf_house::TurfHouse,
            &sod_shelter::SodShelter,
            &wood_pile::WoodPile,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The signal beacon is the kit's firelit hero — it must keep its
    /// emissive flame trim so escalation's broken-emissive ruin pass has
    /// something to snuff.
    #[test]
    fn beacon_keeps_its_firelight() {
        assert!(
            crate::catalogue::items::util::has_emissive(&signal_beacon::SignalBeacon.build("")),
            "signal beacon lost its emissive firelight"
        );
    }
}
