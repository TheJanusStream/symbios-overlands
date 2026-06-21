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
    capsule, cone, cuboid, cylinder, id_quat, prim, sphere, torus, with_torture,
};
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

fn darken(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.4, c[1] * 0.4, c[2] * 0.4]
}

// ---------------------------------------------------------------------------
// Hats  (mounted just above the head crown)
// ---------------------------------------------------------------------------

fn wizard_cone(ctx: &PartCtx) -> Generator {
    let cloth = ctx.materials.cloth(ctx.palette.tertiary_accent);
    // A tall cone with a slight forward bend in the tip.
    let mut hat = prim(
        with_torture(cone(0.15, 0.44, 12, cloth), 0.0, 0.0, [0.05, 0.0, 0.0]),
        [0.0, 0.20, 0.0],
        id_quat(),
    );
    hat.children.push(prim(
        torus(
            0.022,
            0.16,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.20, 0.0],
        id_quat(),
    ));
    hat.children.push(prim(
        sphere(0.03, 2, ctx.materials.accent(ctx.palette.primary_accent)),
        [0.10, 0.22, 0.0],
        id_quat(),
    ));
    hat
}

fn top_hat(ctx: &PartCtx) -> Generator {
    let felt = ctx.materials.cloth(darken(ctx.palette.tertiary_accent));
    let mut hat = prim(
        cylinder(0.12, 0.26, 16, felt.clone()),
        [0.0, 0.13, 0.0],
        id_quat(),
    );
    hat.children.push(prim(
        cylinder(0.18, 0.02, 16, felt),
        [0.0, -0.13, 0.0],
        id_quat(),
    ));
    hat.children.push(prim(
        torus(
            0.014,
            0.122,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.07, 0.0],
        id_quat(),
    ));
    hat
}

fn war_helm(ctx: &PartCtx) -> Generator {
    let metal = ctx.materials.metal(ctx.palette.tertiary_accent);
    let mut helm = prim(sphere(0.15, 3, metal.clone()), [0.0, 0.02, 0.0], id_quat());
    helm.transform.scale = Fp3([1.0, 0.9, 1.05]);
    // Nasal guard down the front face (-Z).
    helm.children.push(prim(
        cuboid([0.03, 0.13, 0.02], metal),
        [0.0, -0.06, -0.15],
        id_quat(),
    ));
    // Crest spike.
    helm.children.push(prim(
        cone(
            0.03,
            0.18,
            8,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, 0.18, 0.0],
        id_quat(),
    ));
    helm
}

fn circlet(ctx: &PartCtx) -> Generator {
    // A circlet rings the head at the brow rather than topping it, so it hangs
    // well below the shared Hat mount (which suits crown-toppers) to sit around
    // the hair like a headband. Ring slightly wider than the head+hair.
    let mut c = prim(
        torus(
            0.014,
            0.15,
            ctx.materials.trim(ctx.palette.secondary_accent),
        ),
        [0.0, -0.10, 0.0],
        id_quat(),
    );
    c.children.push(prim(
        sphere(0.028, 2, ctx.materials.accent(ctx.palette.primary_accent)),
        [0.0, -0.07, -0.15],
        id_quat(),
    ));
    c
}

fn visor(ctx: &PartCtx) -> Generator {
    let frame = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Like the circlet, the visor wraps the face at brow level, so it hangs
    // below the crown-topper Hat mount.
    let mut v = prim(
        cuboid([0.30, 0.07, 0.04], frame),
        [0.0, -0.11, -0.1],
        id_quat(),
    );
    // Glowing lens band across the front.
    v.children.push(prim(
        cuboid(
            [0.26, 0.03, 0.02],
            ctx.materials.glow(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, -0.03],
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
    let cloth = ctx.materials.cloth(ctx.palette.primary_accent);
    let mut torso = prim(capsule(0.15, 0.40, cloth), [0.0, 0.0, 0.0], id_quat());
    // Flared skirt cone, wide at the hem.
    torso.children.push(prim(
        cone(
            0.30,
            0.55,
            14,
            ctx.materials.cloth(ctx.palette.secondary_accent),
        ),
        [0.0, -0.42, 0.0],
        id_quat(),
    ));
    // Belt at the waist.
    torso.children.push(prim(
        torus(0.025, 0.16, ctx.materials.trim(ctx.palette.tertiary_accent)),
        [0.0, -0.16, 0.0],
        id_quat(),
    ));
    torso
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static WIZARD_CONE: PartDef = PartDef {
    slug: "hum_hat_wizard_cone",
    name: "Wizard Hat",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: ARCANE,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wizard_cone,
};
static TOP_HAT: PartDef = PartDef {
    slug: "hum_hat_top_hat",
    name: "Top Hat",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: FORMAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: top_hat,
};
static WAR_HELM: PartDef = PartDef {
    slug: "hum_hat_war_helm",
    name: "War Helm",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: war_helm,
};
static CIRCLET: PartDef = PartDef {
    slug: "hum_hat_circlet",
    name: "Circlet",
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
    name: "Visor",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: visor,
};

static MEDALLION: PartDef = PartDef {
    slug: "hum_orn_medallion",
    name: "Medallion",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: REGAL,
    ornateness: fancy(),
    wear: WearBand::ANY,
    build: medallion,
};
static NEON_SIGIL: PartDef = PartDef {
    slug: "hum_orn_neon_sigil",
    name: "Neon Sigil",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_sigil,
};
static BANDOLIER: PartDef = PartDef {
    slug: "hum_orn_bandolier",
    name: "Bandolier",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: FRONTIER,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bandolier,
};

static ROBE_TORSO: PartDef = PartDef {
    slug: "hum_torso_robe",
    name: "Robe",
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
        let ctx = PartCtx::for_seed(7, "did:plc:hum");
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
