//! Alien-Organic-theme catalogue structures — a living hive-colony of chitin,
//! flesh and biolume.
//!
//! Two prosperity registers share one xenobiological identity: the established
//! ([`ORGANIC_BAND`]) thriving hive (a chitinous hive, a pod cluster, a fleshy
//! spire, a membrane wall, egg sacs, biolume stalks, a tendril, a spore vent
//! and a creep patch) and the destitute ([`ORGANIC_POOR`]) necrotic kit (a
//! withered hive, burst husk pods, a rot patch).
//!
//! Surfaces use the real procedural generators rather than flat colour: hard
//! glossy [`chitin`] shell, soft matte [`flesh`], wet [`membrane`] and glowing
//! biolume carried by [`crate::catalogue::items::util::glow`] emissive trim.
//! The hive pulses and spores drift over an eerie [`fx`] bed. The theme's
//! green-biolume accent lives in [`crate::seeded_defaults::room::accent`].

pub mod biolume_stalk;
pub mod chitinous_hive;
pub mod creep_patch;
pub mod egg_sac;
pub mod fleshy_spire;
pub mod membrane_wall;
pub mod pod_cluster;
pub mod spore_vent;
pub mod tendril;
// Poor (necrotic) variants — the prosperity-Poor end of the theme.
pub mod husk_pods;
pub mod rot_patch;
pub mod withered_hive;

pub mod fx;

use bevy_symbios_texture::metal::MetalStyle;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, prim_scaled, quat_y, quat_z, solid, sphere,
};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier};

/// Shared prosperity band for the thriving hive — a living biolit colony reads
/// as a Modest-to-Rich organism. The poor end of the theme is the separate
/// necrotic kit ([`withered_hive`], …), tagged `Poor`, so a destitute alien
/// room grows the dying colony instead.
pub(super) const ORGANIC_BAND: ProsperityBand =
    ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich);

/// Prosperity band for the necrotic kit — the destitute end of the theme,
/// never picked for a modest or affluent alien room.
pub(super) const ORGANIC_POOR: ProsperityBand = ProsperityBand::only(ProsperityTier::Poor);

/// Hard glossy chitin — the hive's plated shell, ribs and carapace.
pub(super) fn chitin(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.3),
        metallic: Fp(0.5),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([color[0] * 1.4, color[1] * 0.8, color[2] * 1.2]),
            roughness: Fp64(0.3),
            metallic: Fp(0.5),
            rust_level: Fp64(0.1),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Soft matte flesh — pods, spires, tendrils, the hive's living tissue.
pub(super) fn flesh(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.7),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Wet translucent membrane — stretched walls and sac skins, a damp sheen.
pub(super) fn membrane(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.22),
        metallic: Fp(0.1),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

// Chitin + flesh palette.
pub(super) const CHITIN_DARK: [f32; 3] = [0.20, 0.16, 0.26];
pub(super) const CHITIN_GREEN: [f32; 3] = [0.18, 0.24, 0.20];
pub(super) const FLESH_RED: [f32; 3] = [0.52, 0.26, 0.30];
pub(super) const FLESH_PINK: [f32; 3] = [0.66, 0.42, 0.46];
pub(super) const MEMBRANE_TEAL: [f32; 3] = [0.34, 0.54, 0.50];
pub(super) const NECROTIC: [f32; 3] = [0.44, 0.42, 0.38];
pub(super) const HUSK: [f32; 3] = [0.56, 0.50, 0.42];

// Emissive biolume colours. Deep-saturated so they hold their hue under bloom
// at a moderate `glow()` strength (~1.8-2.2): a pale colour driven bright
// washes the brightest texels to a near-white blank (the fantasy/solarpunk
// deep-saturate rule), which on a bioluminescent kit reads as dead plastic.
pub(super) const BIOLUME_CYAN: [f32; 3] = [0.10, 0.92, 0.80];
pub(super) const BIOLUME_GREEN: [f32; 3] = [0.28, 0.95, 0.30];
pub(super) const SAC_GLOW: [f32; 3] = [1.0, 0.20, 0.48];

// ---------------------------------------------------------------------------
// Alien-Organic signature helpers (`pub(super)`, theme-local like steampunk's
// `cog()` / nordic's `gable_roof` / fantasy's `crystal()`). The curved,
// fleshy, bioluminescent vocabulary the kit shares — built once and reused
// across the hive, pods, spires, walls and the necrotic kit.
// ---------------------------------------------------------------------------

/// A curling multi-segment flesh tendril returned as ONE positioned subtree.
/// The base segment is the local root; each upper segment nests as a child of
/// the one below, so it *inherits* the curl beneath it — the leans compound
/// into a natural coil (the nordic `dragon_head` neck-as-subtree-root trick).
/// `yaw` aims the curl azimuth, `curl` is the lean added at every joint, the
/// radius tapers up the chain. Drop it into an [`assemble`](crate::catalogue::items::util::assemble) list as a
/// NON-first child: the base carries a `quat_y(yaw)` rotation, so it must
/// never be `prims[0]` (the root-rotation gotcha — a rotated assemble root
/// spins every sibling into its frame).
pub(super) fn tendril(
    foot: [f32; 3],
    yaw: f32,
    base_r: f32,
    seg_len: f32,
    segs: usize,
    curl: f32,
    mat: SovereignMaterialSettings,
) -> Generator {
    let n = segs.max(1);
    // Build tip-first so each lower segment wraps the chain already grown
    // above it; the last one built becomes the returned base.
    let mut node: Option<Generator> = None;
    for i in (0..n).rev() {
        let r = (base_r * (1.0 - 0.62 * i as f32 / n as f32)).max(0.04);
        let mut seg = prim(
            solid(cylinder_tapered(r, seg_len, 6, 0.12, mat.clone())),
            [0.0, 0.0, 0.0],
            quat_z(0.0),
        );
        if let Some(child) = node.take() {
            // Seat the inner chain at this segment's tip, tilted by `curl`
            // toward +Z (overlapping ~16% so the bend leaves no gap).
            let arm = seg_len * 0.42;
            let mut c = child;
            c.transform.translation = Fp3([0.0, arm + curl.cos() * arm, curl.sin() * arm]);
            seg.children.push(c);
        }
        node = Some(seg);
    }
    let mut base = node.unwrap();
    base.transform.translation = Fp3([foot[0], foot[1] + seg_len * 0.5, foot[2]]);
    base.transform.rotation = quat_y(yaw);
    base
}

/// A smooth fleshy egg-pod — a res-6 ovoid (taller than wide, the `tall`
/// y-scale) seated on a short tapered collar. Returned as ONE subtree (the
/// collar is the local root, `id_quat`, so it is safe anywhere including as an
/// assemble root). Reused across the brood — `pod_cluster`, `egg_sac`, the
/// hive. Sphere res stays at the `(0,6)` sanitiser clamp.
pub(super) fn egg_pod(
    foot: [f32; 3],
    r: f32,
    tall: f32,
    pod_mat: SovereignMaterialSettings,
    collar_mat: SovereignMaterialSettings,
) -> Generator {
    let collar_h = r * 0.5;
    let mut collar = prim(
        solid(cylinder_tapered(r * 0.6, collar_h, 8, 0.4, collar_mat)),
        [foot[0], foot[1] + collar_h * 0.5, foot[2]],
        id_quat(),
    );
    collar.children.push(prim_scaled(
        solid(sphere(r, 6, pod_mat)),
        [0.0, collar_h * 0.5 + r * tall * 0.72, 0.0],
        id_quat(),
        [1.0, tall, 1.0],
    ));
    collar
}

/// A branching glowing vein network standing proud of a FLAT membrane face
/// (emissive reads on flat faces, not curved ones — the steampunk lesson): a
/// central stem + four angled offshoots, thin saturated strokes. `center` is
/// the panel-face point, `zf` the proud offset along the face normal (sign
/// picks the front side), `h` the stem length. Reused on `membrane_wall`,
/// `egg_sac` and the spire frills.
pub(super) fn glow_veins(
    center: [f32; 3],
    zf: f32,
    h: f32,
    mat: SovereignMaterialSettings,
) -> Vec<Generator> {
    let mut v = vec![prim(
        cuboid_tapered([0.1, h, 0.07], 0.0, mat.clone()),
        [center[0], center[1], center[2] + zf],
        id_quat(),
    )];
    for (dy, sign, bl) in [
        (h * 0.20, 1.0_f32, h * 0.5),
        (h * 0.16, -1.0, h * 0.44),
        (-h * 0.12, 1.0, h * 0.38),
        (-h * 0.16, -1.0, h * 0.32),
    ] {
        v.push(prim(
            cuboid_tapered([0.06, bl, 0.06], 0.0, mat.clone()),
            [center[0] + sign * bl * 0.26, center[1] + dy, center[2] + zf],
            quat_z(sign * 0.7),
        ));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::CatalogueEntry;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// The three poor (necrotic) variants must build clean trees the sanitiser
    /// leaves untouched.
    #[test]
    fn poor_variants_round_trip() {
        let entries: [&dyn CatalogueEntry; 3] = [
            &withered_hive::WitheredHive,
            &husk_pods::HuskPods,
            &rot_patch::RotPatch,
        ];
        for e in entries {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The chitinous hive is the kit's lit hero — it must keep its emissive
    /// biolume so escalation's broken-emissive ruin pass has light to snuff.
    #[test]
    fn hive_keeps_its_biolume() {
        assert!(
            crate::catalogue::items::util::has_emissive(&chitinous_hive::ChitinousHive.build("")),
            "chitinous hive lost its emissive biolume"
        );
    }
}
