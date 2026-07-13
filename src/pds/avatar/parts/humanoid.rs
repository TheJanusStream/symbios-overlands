//! Styled humanoid part kits — crafted hats, ornaments, and a robe torso
//! that fill the optional [`PartSlot::Hat`] / [`PartSlot::Ornament`] slots and
//! add style-specific variants alongside the universal defaults.
//!
//! Each part is tagged with the [`ThemeArchetype`] styles it suits and an
//! ornateness / wear band, so the outfit deriver only mounts a wizard's hat
//! on an ornate fantasy avatar, a neon sigil on a cyberpunk one, and so on.
//! Geometry uses the shared primitive vocabulary + torture shaping; finish
//! comes from the seeded [`MaterialKit`](crate::seeded_defaults::MaterialKit)
//! (so emissive styles' accents glow). Parts build in their slot's local
//! attachment frame (see the module docstring on [`super`]).

use crate::pds::avatar::default_visuals::common::{
    capsule, cone, cuboid, cylinder, id_quat, prim, sphere, torus, with_cut, with_torture,
};
use crate::pds::avatar::parts::defaults::common::darken;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::ThemeArchetype::{
    self, AlienMonolithic, AlienOrganic, AncientClassical, CivicCampus, Cyberpunk, Fantasy,
    GothicHorror, IndustrialPark, Medieval, ModernCity, Nordic, PostApoc, Solarpunk, SpaceOutpost,
    Steampunk, WildWest,
};
use crate::seeded_defaults::{OrnatenessBand, OrnatenessTier, WearBand};

use super::{PartCtx, PartDef, PartSlot};

const HUMANOID: &[ChassisFamily] = &[ChassisFamily::Humanoid];

// Shared style affinity groups.
const ARCANE: &[ThemeArchetype] = &[Fantasy, AlienOrganic];
const FORMAL: &[ThemeArchetype] = &[Steampunk, GothicHorror, CivicCampus, ModernCity];
const MARTIAL: &[ThemeArchetype] = &[Medieval, Nordic, AncientClassical];
const REGAL: &[ThemeArchetype] = &[Fantasy, AncientClassical, CivicCampus, Medieval];
const NEON: &[ThemeArchetype] = &[Cyberpunk, SpaceOutpost, AlienMonolithic, Solarpunk];
const ROBED: &[ThemeArchetype] = &[Fantasy, GothicHorror, Medieval, AncientClassical];
const FRONTIER: &[ThemeArchetype] = &[WildWest, PostApoc, IndustrialPark];

/// Adorned-or-more — the band most ornamental parts advertise.
const fn fancy() -> OrnatenessBand {
    OrnatenessBand::range(OrnatenessTier::Adorned, OrnatenessTier::Ornate)
}

// ---------------------------------------------------------------------------
// Hats  (mounted just above the head crown)
// ---------------------------------------------------------------------------

/// Hat scale: every hat was authored against the old fixed r = 0.13 head;
/// builders multiply *all* dimensions AND translations by this so a hat
/// fits its seed's head. (The assembler's old root-scale trick scaled the
/// children but not the root's own offset, which stranded brow-level hats
/// mid-face.)
fn hat_k(ctx: &PartCtx) -> f32 {
    ctx.blueprint.head_r / 0.13
}

fn wizard_cone(ctx: &PartCtx) -> Generator {
    let k = hat_k(ctx);
    let cloth = ctx.materials.cloth(ctx.palette.tertiary_accent);
    // A tall cone with a slight forward bend, TRUNCATED at 88 % height by a
    // profile-cut: a full point-tip rasterises sub-pixel over its last
    // stretch at contact-sheet scale, so any tip ornament read as floating
    // above the visible taper (the round-5 "star gap" — the star was at
    // the true apex all along). The flat stub always rasterises and the
    // star caps it.
    let mut hat = prim(
        with_cut(
            with_torture(
                cone(0.15 * k, 0.44 * k, 12, cloth),
                0.0,
                0.0,
                [0.05 * k, 0.0, 0.0],
            ),
            [0.0, 1.0],
            [0.0, 0.88],
            0.0,
        ),
        [0.0, 0.20 * k, 0.0],
        id_quat(),
    );
    hat.children.push(prim(
        torus(
            (0.022 * k).max(0.011),
            0.16 * k,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.20 * k, 0.0],
        id_quat(),
    ));
    // Tip star — a CHILD of the cone node, so its offset is cone-LOCAL:
    // the stub top sits at −0.22k + 0.88·0.44k ≈ +0.167k, displaced the
    // full +0.05k bend (t renormalises over the truncated mesh, so t = 1
    // at the stub). The star swallows the ~0.018k-radius flat top.
    hat.children.push(prim(
        sphere(
            (0.035 * k).max(0.011),
            2,
            ctx.materials.accent(ctx.palette.primary_accent),
        ),
        [0.05 * k, 0.167 * k, 0.0],
        id_quat(),
    ));
    hat
}

fn top_hat(ctx: &PartCtx) -> Generator {
    let k = hat_k(ctx);
    let felt = ctx.materials.cloth(darken(ctx.palette.tertiary_accent));
    // Seated a touch lower than the shared crown mount so the brim rests
    // on the hair instead of hovering on bald crowns.
    let mut hat = prim(
        cylinder(0.12 * k, 0.26 * k, 16, felt.clone()),
        [0.0, 0.11 * k, 0.0],
        id_quat(),
    );
    hat.children.push(prim(
        cylinder(0.18 * k, (0.02 * k).max(0.011), 16, felt),
        [0.0, -0.13 * k, 0.0],
        id_quat(),
    ));
    hat.children.push(prim(
        torus(
            (0.014 * k).max(0.011),
            0.122 * k,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.07 * k, 0.0],
        id_quat(),
    ));
    hat
}

fn war_helm(ctx: &PartCtx) -> Generator {
    let k = hat_k(ctx);
    let metal = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Root stays scale-free (children would inherit it); the dome's slight
    // squash rides on a leaf child instead.
    let mut helm = prim(
        sphere(0.145 * k, 3, metal.clone()),
        [0.0, 0.02 * k, 0.0],
        id_quat(),
    );
    let mut dome = prim(
        sphere(0.15 * k, 3, metal.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    dome.transform.scale = Fp3([1.0, 0.9, 1.05]);
    helm.children.push(dome);
    // Nasal guard down the front face (-Z).
    helm.children.push(prim(
        cuboid(
            [(0.03 * k).max(0.011), 0.13 * k, (0.02 * k).max(0.011)],
            metal,
        ),
        [0.0, -0.06 * k, -0.15 * k],
        id_quat(),
    ));
    // Crest spike.
    helm.children.push(prim(
        cone(
            (0.03 * k).max(0.011),
            0.18 * k,
            8,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, 0.18 * k, 0.0],
        id_quat(),
    ));
    helm
}

fn circlet(ctx: &PartCtx) -> Generator {
    // A circlet rings the head at the brow rather than topping it, so it hangs
    // well below the shared Hat mount (which suits crown-toppers) to sit around
    // the hair like a headband. Ring slightly wider than the head+hair.
    let k = hat_k(ctx);
    let mut c = prim(
        torus(
            (0.014 * k).max(0.011),
            0.15 * k,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.10 * k, 0.0],
        id_quat(),
    );
    // Brow gem, tucked against the ring (it used to hang scaled while the
    // ring hung unscaled, stranding it at nose height).
    c.children.push(prim(
        sphere(
            (0.028 * k).max(0.011),
            2,
            ctx.materials.accent(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, -0.15 * k],
        id_quat(),
    ));
    c
}

fn visor(ctx: &PartCtx) -> Generator {
    let k = hat_k(ctx);
    let frame = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Like the circlet, the visor wraps the face at brow level, so it hangs
    // below the crown-topper Hat mount.
    let mut v = prim(
        cuboid([0.30 * k, 0.07 * k, (0.04 * k).max(0.011)], frame),
        [0.0, -0.11 * k, -0.1 * k],
        id_quat(),
    );
    // Glowing lens band across the front. Kept narrower than the frame's
    // 0.30 half-width so the metal frame caps the glow at the temples
    // instead of the emissive wrapping onto the ear (#738-3: an isolated
    // over-bright lens fleck read at the ear on the magenta NEON palette).
    v.children.push(prim(
        cuboid(
            [0.21 * k, (0.03 * k).max(0.011), (0.02 * k).max(0.011)],
            ctx.materials.glow(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, -0.03 * k],
        id_quat(),
    ));
    v
}

// ---------------------------------------------------------------------------
// Ornaments  (mounted on the chest front)
// ---------------------------------------------------------------------------

fn medallion(ctx: &PartCtx) -> Generator {
    let mut m = prim(
        sphere(0.05, 2, ctx.materials.accent(ctx.palette.primary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    m.children.push(prim(
        torus(
            0.016,
            0.06,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    m
}

fn neon_sigil(ctx: &PartCtx) -> Generator {
    // A glowing chest emblem — always emits regardless of style.
    let mut s = prim(
        cuboid(
            [0.07, 0.12, 0.02],
            ctx.materials.glow(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    s.children.push(prim(
        cuboid(
            [0.12, 0.03, 0.02],
            ctx.materials.glow(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    s
}

fn bandolier(ctx: &PartCtx) -> Generator {
    // A diagonal strap across the chest with a couple of pouches.
    let strap = ctx.materials.cloth(darken(ctx.palette.tertiary_accent));
    let mut b = prim(
        with_torture(
            cuboid([0.06, 0.5, 0.03], strap.clone()),
            0.6,
            0.0,
            [0.0, 0.0, 0.0],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for y in [-0.12f32, 0.06] {
        b.children.push(prim(
            cuboid([0.07, 0.06, 0.04], strap.clone()),
            [y * 0.6, y, -0.01],
            id_quat(),
        ));
    }
    b
}

// ---------------------------------------------------------------------------
// Torso variant  (centred at the origin, like the default torso)
// ---------------------------------------------------------------------------

fn robe_torso(ctx: &PartCtx) -> Generator {
    // Blueprint-sized so the robe fits every stylization tier: the trunk
    // matches the default torso's chest, and the skirt cone falls from the
    // waist to just short of the ground (it hides the legs by design).
    let bp = &ctx.blueprint;
    let cloth = ctx.materials.cloth(ctx.palette.primary_accent);
    let chest_r = bp.chest_r * 0.94;
    let mut torso = prim(
        capsule(chest_r * 0.96, bp.trunk_len * 0.8, cloth.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    torso.transform.scale = Fp3([1.0, 1.0, bp.depth]);
    // Shoulder yoke, as the shirt/coat trunks — without it the arms hang
    // beside the robe with a visible gap (the reported disconnect).
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let mut yoke = prim(sphere(1.0, 3, cloth.clone()), [0.0, yoke_y, 0.0], id_quat());
    yoke.transform.scale = Fp3([
        bp.shoulder_x + bp.arm_r * 0.7,
        chest_r * 0.45,
        chest_r * 0.92,
    ]);
    torso.children.push(yoke);
    // Flared skirt cone, wide at the hem. Torso-local: its top starts at the
    // belt line and its base lands a touch above the ground plane.
    let hem_y = -(bp.torso_y + bp.leg_total() * 0.92);
    let top_y = -bp.trunk_len * 0.12;
    let skirt_h = top_y - hem_y;
    torso.children.push(prim(
        cone(
            bp.waist_r * 2.2,
            skirt_h,
            14,
            ctx.materials.cloth(ctx.palette.secondary_accent),
        ),
        [0.0, (top_y + hem_y) * 0.5, 0.0],
        id_quat(),
    ));
    // Belt at the waist.
    torso.children.push(prim(
        torus(
            0.025,
            bp.waist_r * 1.08,
            ctx.materials.trim(ctx.palette.tertiary_accent),
        ),
        [0.0, top_y, 0.0],
        id_quat(),
    ));
    torso
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static WIZARD_CONE: PartDef = PartDef {
    slug: "hum_hat_wizard_cone",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: ARCANE,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wizard_cone,
};
static TOP_HAT: PartDef = PartDef {
    slug: "hum_hat_top_hat",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: FORMAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: top_hat,
};
static WAR_HELM: PartDef = PartDef {
    slug: "hum_hat_war_helm",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: war_helm,
};
static CIRCLET: PartDef = PartDef {
    slug: "hum_hat_circlet",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: REGAL,
    // A jewelled circlet reads as finery — ornate avatars only.
    ornateness: OrnatenessBand::only(OrnatenessTier::Ornate),
    wear: WearBand::ANY,
    build: circlet,
};
static VISOR: PartDef = PartDef {
    slug: "hum_hat_visor",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: visor,
};

static MEDALLION: PartDef = PartDef {
    slug: "hum_orn_medallion",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: REGAL,
    ornateness: fancy(),
    wear: WearBand::ANY,
    build: medallion,
};
static NEON_SIGIL: PartDef = PartDef {
    slug: "hum_orn_neon_sigil",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_sigil,
};
static BANDOLIER: PartDef = PartDef {
    slug: "hum_orn_bandolier",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: FRONTIER,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bandolier,
};

static ROBE_TORSO: PartDef = PartDef {
    slug: "hum_torso_robe",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    styles: ROBED,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: robe_torso,
};

/// Every styled humanoid part.
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &WIZARD_CONE,
    &TOP_HAT,
    &WAR_HELM,
    &CIRCLET,
    &VISOR,
    &MEDALLION,
    &NEON_SIGIL,
    &BANDOLIER,
    &ROBE_TORSO,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;

    #[test]
    fn every_styled_part_builds_and_is_tagged() {
        let ctx = PartCtx::for_seed(7);
        for part in ENTRIES {
            assert!(!part.styles().is_empty(), "{} is untagged", part.slug());
            assert_eq!(part.chassis(), HUMANOID, "{} wrong chassis", part.slug());
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} non-deterministic", part.slug());
        }
    }

    #[test]
    fn a_fantasy_ornate_avatar_can_wear_a_wizard_hat() {
        // The Hat pool for an ornate Fantasy avatar includes the wizard hat.
        use crate::pds::avatar::parts::parts_for_avatar;
        let hats: Vec<&str> = parts_for_avatar(
            ChassisFamily::Humanoid,
            PartSlot::Hat,
            ThemeArchetype::Fantasy,
            OrnatenessTier::Ornate,
            crate::seeded_defaults::WearTier::Pristine,
        )
        .map(|p| p.slug())
        .collect();
        assert!(hats.contains(&"hum_hat_wizard_cone"), "got {hats:?}");
    }
}
