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
    capsule, cone, cuboid, cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z,
    sphere, torus, with_cut, with_torture,
};
use crate::pds::avatar::parts::defaults::common::{darken, ensure_delta, luma, shade};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::ThemeArchetype::{
    self, AlienMonolithic, AlienOrganic, AncientClassical, CivicCampus, Cyberpunk, Fantasy,
    GothicHorror, IndustrialPark, Medieval, ModernCity, Nordic, Pirate, PostApoc, Solarpunk,
    SpaceOutpost, Steampunk, WildWest,
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
/// The buccaneer read: a tricorn, a baldric and a captain's coat.
///
/// A group of exactly one, and deliberately so. Every other group here is a
/// *mood* several themes share, but the three parts below are period dress
/// rather than a mood — a tricorn on a wild-west avatar is a costume error,
/// not a stylistic choice. Pirate is otherwise served by the universal
/// defaults, and it sits in MARTIAL / HISTORIC / WORKING on the vehicle side
/// where the parts really are shared.
const BUCCANEER: &[ThemeArchetype] = &[Pirate];

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

/// A tricorn — a low round crown on a wide brim cocked up on three sides.
///
/// The three cocks are what make the hat, and they are the reason this is not
/// just a wide-brimmed hat with a taper: a tricorn's silhouette is a *triangle
/// seen from above* and three straight edges seen from the side. Each cock is
/// a leaf prim carrying its own yaw-then-tilt, which is the rotated form that
/// displaces nothing (a turn with no offset children under it).
///
/// The corners land at 0 / 120 / 240 degrees so one point faces front (`-Z`,
/// the render and settlement convention), which is how a tricorn is worn.
fn tricorn(ctx: &PartCtx) -> Generator {
    let k = hat_k(ctx);
    let felt = ctx.materials.cloth(darken(ctx.palette.tertiary_accent));
    let lace = ctx.materials.trim(ctx.palette.secondary_accent);

    // Crown, seated low so the hat rests on the hair rather than hovering
    // over a bald head — the top hat's lesson, same mount.
    let mut hat = prim(
        cylinder(0.125 * k, 0.17 * k, 14, felt.clone()),
        [0.0, 0.10 * k, 0.0],
        id_quat(),
    );

    // The brim is a NARROW lip, not a disc.
    //
    // The first build gave it 0.29k and hung the three cocks above it, which
    // rendered as a sombrero: a wide flat plate with flaps floating over it.
    // A tricorn has no flat brim on show — the brim *is* the three cocks,
    // turned up against the crown, and all that peeks out between them are
    // the three points. So the disc shrinks to just past the crown and the
    // cocks do the work.
    hat.children.push(prim(
        cylinder(0.195 * k, (0.022 * k).max(0.011), 18, felt.clone()),
        [0.0, -0.085 * k, 0.0],
        id_quat(),
    ));
    hat.children.push(prim(
        torus((0.016 * k).max(0.011), 0.129 * k, lace.clone()),
        [0.0, -0.058 * k, 0.0],
        id_quat(),
    ));

    // The three cocked sides, standing UP against the crown rather than lying
    // out flat: 0.55 rad off vertical, where the first build used 0.95 and
    // laid them nearly horizontal. Yawed to sit between the corners, so the
    // three points land at 0 / 120 / 240 degrees and one faces front (`-Z`,
    // the render and settlement convention), which is how a tricorn is worn.
    //
    // They are wide enough to overlap at the corners — that overlap IS the
    // point of the hat, in both senses.
    let flap_r = 0.15 * k;
    for i in 0..3 {
        let yaw = std::f32::consts::FRAC_PI_3 + i as f32 * std::f32::consts::TAU / 3.0;
        // `quat_mul(a, b)` applies b first: tilt about X, then swing by yaw.
        let turn = quat_mul(quat_y(yaw), quat_x(-0.55));
        hat.children.push(prim(
            cuboid([0.31 * k, 0.19 * k, (0.024 * k).max(0.011)], felt.clone()),
            [flap_r * yaw.sin(), -0.035 * k, -flap_r * yaw.cos()],
            quat_xyzw(turn),
        ));
    }

    // Cockade on the left front cock — the one asymmetry, and what stops the
    // hat reading as a piece of geometry with threefold symmetry.
    let cockade_yaw = std::f32::consts::FRAC_PI_3;
    hat.children.push(prim(
        cylinder(
            0.055 * k,
            (0.022 * k).max(0.011),
            10,
            ctx.materials.accent(ctx.palette.primary_accent),
        ),
        [
            0.175 * k * cockade_yaw.sin(),
            0.02 * k,
            -0.175 * k * cockade_yaw.cos(),
        ],
        quat_xyzw(quat_mul(quat_y(cockade_yaw), quat_x(-0.55))),
    ));
    hat
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

/// A baldric — the broad shoulder belt, with a brace of pistols tucked in it.
///
/// Distinct from the [`bandolier`] it sits beside: that is a *twisted* strap
/// with pouches, which is a frontier read. This is a wide flat belt worn over
/// one shoulder with a heavy buckle and two pistol butts, which is a
/// buccaneer's, and the pistols are most of why — a strap alone is luggage.
fn baldric(ctx: &PartCtx) -> Generator {
    let leather = ctx.materials.cloth(darken(ctx.palette.tertiary_accent));
    let brass = ctx.materials.metal(ctx.palette.secondary_accent);
    let walnut = ctx
        .materials
        .cloth(shade(ctx.palette.tertiary_accent, 0.28));

    // The BUCKLE is the root, and it is unrotated.
    //
    // The obvious build is a rotated strap carrying everything, and it is
    // wrong twice over: a rotated parent spins its children's offsets, so the
    // pistols placed in chest space would swing out of it; and the first
    // attempt at dodging that used a degenerate 1 mm cuboid as a pivot, which
    // the sanitiser clamps to 10 mm — an invisible node that is not actually
    // invisible. Rooting on a real part that genuinely sits at the ornament's
    // centre solves both: the strap hangs off it at ITS OWN ORIGIN, so the
    // tilt turns the strap and moves nothing.
    let mut b = prim(
        cuboid([0.075, 0.062, 0.022], brass.clone()),
        [0.0, 0.0, -0.028],
        id_quat(),
    );
    // Strap across the chest — at the buckle's origin, so its turn displaces
    // nothing.
    b.children.push(prim(
        cuboid([0.095, 0.62, 0.035], leather),
        [0.0, 0.0, 0.014],
        id_quat(),
    ));
    let strap = b.children.len() - 1;
    b.children[strap].transform.rotation = quat_xyzw(quat_z(0.52));
    // Buckle tongue. The minor radius is at the sanitiser's 0.011 floor —
    // anything finer is silently rewritten, which fails the round-trip guard
    // rather than rendering differently.
    b.children.push(prim(
        torus(0.012, 0.03, brass.clone()),
        [0.0, 0.0, -0.012],
        quat_xyzw(quat_x(std::f32::consts::FRAC_PI_2)),
    ));
    // A brace of pistols, butts out, tucked under the strap at opposite
    // heights so the pair reads as carried rather than as symmetrical trim.
    for (sx, y, tilt) in [(-1.0_f32, 0.075_f32, 0.6_f32), (1.0, -0.055, -0.45)] {
        b.children.push(prim(
            cuboid([0.032, 0.115, 0.03], walnut.clone()),
            [sx * 0.085, y, -0.017],
            quat_xyzw(quat_z(tilt * sx)),
        ));
        b.children.push(prim(
            sphere(0.021, 2, brass.clone()),
            [sx * 0.107, y - 0.058, -0.017],
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

static TRICORN: PartDef = PartDef {
    slug: "hum_hat_tricorn",
    slot: PartSlot::Hat,
    chassis: HUMANOID,
    styles: BUCCANEER,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: tricorn,
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

static BALDRIC: PartDef = PartDef {
    slug: "hum_orn_baldric",
    slot: PartSlot::Ornament,
    chassis: HUMANOID,
    styles: BUCCANEER,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: baldric,
};

static LONGCOAT_TORSO: PartDef = PartDef {
    slug: "hum_torso_longcoat",
    slot: PartSlot::Torso,
    chassis: HUMANOID,
    styles: BUCCANEER,
    // A captain's coat is finery: a plain buccaneer wears the default trunk.
    ornateness: fancy(),
    wear: WearBand::ANY,
    build: longcoat_torso,
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

/// A captain's longcoat — a justaucorps over a sash.
///
/// Built against the blueprint like [`robe_torso`], and deliberately not a
/// recolour of it: a robe's skirt is a cone to the ankle that hides the legs,
/// a coat's is a flared skirt that stops **below the knee** and is meant to be
/// seen over boots. That difference, the turned-back collar and the button
/// run are the whole read, so all three are derived from the skeleton rather
/// than hand-placed.
fn longcoat_torso(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let coat_c = ctx.palette.primary_accent;
    // The lapels are the coat's signature and they are cut from the secondary
    // accent — which on plenty of seeds sits within a few percent of the
    // primary, so the whole garment merges into one mass and the justaucorps
    // reads as a dress. Forcing a value gap is what the vehicle kits already
    // do between a hull and its deck, for exactly this reason.
    let facing_c = ensure_delta(ctx.palette.secondary_accent, luma(coat_c), 0.22);
    let coat = ctx.materials.cloth(coat_c);
    let facing = ctx.materials.cloth(facing_c);
    let brass = ctx.materials.metal(ensure_delta(
        ctx.palette.secondary_accent,
        luma(coat_c),
        0.3,
    ));

    let chest_r = bp.chest_r * 0.96;
    let mut torso = prim(
        capsule(chest_r * 0.97, bp.trunk_len * 0.82, coat.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    torso.transform.scale = Fp3([1.0, 1.0, bp.depth]);

    // Shoulder yoke — without it the arms hang beside the coat with a gap,
    // which is the disconnect the robe was reported for.
    let yoke_y = bp.shoulder_y - bp.torso_y;
    let mut yoke = prim(sphere(1.0, 3, coat.clone()), [0.0, yoke_y, 0.0], id_quat());
    yoke.transform.scale = Fp3([
        bp.shoulder_x + bp.arm_r * 0.72,
        chest_r * 0.46,
        chest_r * 0.94,
    ]);
    torso.children.push(yoke);

    // Skirt: from the waist to just below the knee, flaring as it falls.
    // The hem is derived from the leg the coat is worn over, so it lands in
    // the right place on every stylization tier.
    let waist_y = -bp.trunk_len * 0.14;
    let hem_y = -(bp.torso_y + bp.thigh + bp.shin * 0.22);
    let skirt_h = waist_y - hem_y;
    torso.children.push(prim(
        cone(bp.waist_r * 1.95, skirt_h, 14, coat.clone()),
        [0.0, (waist_y + hem_y) * 0.5, 0.0],
        id_quat(),
    ));

    // Turned-back LAPELS rather than a collar cone.
    //
    // The cone was invisible in the render — swallowed by the yoke it sat on.
    // Two flat facing-coloured boards angled off the neck down the chest are
    // the justaucorps' actual signature and, unlike a collar, they are the
    // biggest contrasting shape on the garment. Leaf prims, so their turns
    // carry nothing.
    for sx in [-1.0_f32, 1.0] {
        torso.children.push(prim(
            cuboid(
                [chest_r * 0.42, bp.trunk_len * 0.5, chest_r * 0.12],
                facing.clone(),
            ),
            [
                sx * chest_r * 0.36,
                yoke_y - bp.trunk_len * 0.22,
                -chest_r * 0.93,
            ],
            quat_xyzw(quat_z(sx * 0.22)),
        ));
    }
    // Collar band across the back of the neck, tying the two lapels together.
    torso.children.push(prim(
        cuboid(
            [chest_r * 1.05, chest_r * 0.3, chest_r * 0.14],
            facing.clone(),
        ),
        [0.0, yoke_y + chest_r * 0.16, -chest_r * 0.6],
        id_quat(),
    ));

    // Sash at the waist, in the facing colour — the band that separates the
    // coat's two masses and stops the silhouette reading as one cone.
    torso.children.push(prim(
        torus(chest_r * 0.17, bp.waist_r * 1.06, facing),
        [0.0, waist_y, 0.0],
        id_quat(),
    ));

    // Button run down the centre front (`-Z`), between the lapels. Sized to
    // be seen: an 18 mm bead on a chest this size is a speck, and a row of
    // specks is nothing at all.
    let buttons = 5;
    let top = yoke_y - bp.trunk_len * 0.1;
    for i in 0..buttons {
        let t = (i as f32 + 0.5) / buttons as f32;
        torso.children.push(prim(
            sphere(0.028, 2, brass.clone()),
            [0.0, top - t * (top - waist_y).abs(), -chest_r * 1.0],
            id_quat(),
        ));
    }
    torso
}

/// Every styled humanoid part.
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &WIZARD_CONE,
    &TOP_HAT,
    &TRICORN,
    &WAR_HELM,
    &CIRCLET,
    &VISOR,
    &MEDALLION,
    &NEON_SIGIL,
    &BANDOLIER,
    &BALDRIC,
    &ROBE_TORSO,
    &LONGCOAT_TORSO,
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

    /// A pirate's dress is period, not a mood — the three buccaneer parts go
    /// to Pirate and to nobody else, and Pirate does not inherit somebody
    /// else's hat.
    ///
    /// The second half is the point. Before these parts existed Pirate drew
    /// the MARTIAL pool on the Hat slot, so a buccaneer turned up in a Norse
    /// war helm — serviceable, and the wrong hat, which is exactly why #1018
    /// was raised.
    #[test]
    fn only_a_pirate_wears_the_buccaneer_kit() {
        use crate::pds::avatar::parts::parts_for;
        for slug in ["hum_hat_tricorn", "hum_orn_baldric", "hum_torso_longcoat"] {
            let part = ENTRIES
                .iter()
                .find(|p| p.slug() == slug)
                .unwrap_or_else(|| panic!("{slug} is registered"));
            assert_eq!(
                part.styles(),
                BUCCANEER,
                "{slug} has leaked out of the buccaneer group"
            );
        }
        // And the wrong hat is gone: no MARTIAL headwear reaches a pirate.
        let hats: Vec<&str> = parts_for(
            ChassisFamily::Humanoid,
            PartSlot::Hat,
            ThemeArchetype::Pirate,
        )
        .map(|p| p.slug())
        .collect();
        assert!(
            hats.contains(&"hum_hat_tricorn"),
            "a pirate must be able to wear a tricorn; got {hats:?}"
        );
        assert!(
            !hats.contains(&"hum_hat_war_helm"),
            "a pirate is drawing the war helm again; got {hats:?}"
        );
    }

    /// The longcoat is a coat and not a robe: its hem clears the knee, where
    /// the robe's falls to the ankle. Stated against the *skeleton* rather
    /// than as two numbers, so it holds at every stylization tier.
    #[test]
    fn the_longcoat_stops_below_the_knee_where_the_robe_reaches_the_floor() {
        use crate::pds::GeneratorKind;
        fn lowest(g: &Generator, at: f32) -> f32 {
            let here = at + g.transform.translation.0[1];
            let own = match &g.kind {
                GeneratorKind::Cone { height, .. } => here - height.0 * 0.5,
                _ => f32::MAX,
            };
            g.children.iter().fold(own, |lo, c| lo.min(lowest(c, here)))
        }
        for seed in [3_u64, 11, 29, 47] {
            let ctx = PartCtx::for_seed(seed);
            let bp = &ctx.blueprint;
            let coat_hem = lowest(&longcoat_torso(&ctx), 0.0);
            let robe_hem = lowest(&robe_torso(&ctx), 0.0);
            let knee = -(bp.torso_y + bp.thigh);
            let ground = -(bp.torso_y + bp.thigh + bp.shin + bp.foot_len);
            assert!(
                coat_hem < knee,
                "seed {seed}: the coat's hem at {coat_hem} is above the knee at \
                 {knee} — that is a jacket, not a longcoat"
            );
            assert!(
                coat_hem > ground + (knee - ground).abs() * 0.4,
                "seed {seed}: the coat's hem at {coat_hem} is most of the way \
                 to the ground at {ground} — it has become a robe"
            );
            assert!(
                robe_hem < coat_hem,
                "seed {seed}: the robe no longer falls further than the coat"
            );
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
