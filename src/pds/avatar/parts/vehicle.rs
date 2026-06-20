//! Styled vehicle part kits — crafted variants and ornaments for the boat /
//! airship / skiff families.
//!
//! Fills the previously-empty optional vehicle slots ([`PartSlot::Bow`] /
//! [`PartSlot::Stack`] / [`PartSlot::Exhaust`]) and adds style-specific
//! variants for the body slots, plus cross-family ornaments. Tagged by style
//! and by ornateness / wear bands, so a steam funnel only appears on a
//! steampunk / industrial craft, a neon strip on a cyberpunk one, and so on.
//! Geometry uses the shared primitive vocabulary with torture shaping; finish
//! comes from the seeded [`MaterialKit`](crate::seeded_defaults::MaterialKit).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, torus, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::ChassisFamily;
use crate::seeded_defaults::ThemeArchetype::{
    self, AlienMonolithic, AncientClassical, CivicCampus, Cyberpunk, Fantasy, IndustrialPark,
    Medieval, ModernCity, Nordic, PostApoc, Solarpunk, SpaceOutpost, Steampunk, WildWest,
};
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::{PartCtx, PartDef, PartSlot};

const BOAT: &[ChassisFamily] = &[ChassisFamily::Boat];
const AIRSHIP: &[ChassisFamily] = &[ChassisFamily::Airship];
const SKIFF: &[ChassisFamily] = &[ChassisFamily::Skiff];
const VEHICLES: &[ChassisFamily] = &[
    ChassisFamily::Boat,
    ChassisFamily::Airship,
    ChassisFamily::Skiff,
];

const NEON: &[ThemeArchetype] = &[Cyberpunk, SpaceOutpost, AlienMonolithic, Solarpunk];
const STEAM: &[ThemeArchetype] = &[Steampunk, IndustrialPark, ModernCity];
const MARTIAL: &[ThemeArchetype] = &[Medieval, Nordic, WildWest, PostApoc];
const REGAL: &[ThemeArchetype] = &[Fantasy, AncientClassical, CivicCampus];
const GRUBBY: &[ThemeArchetype] = &[Steampunk, IndustrialPark, WildWest, PostApoc, Cyberpunk];

fn darken(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.4, c[1] * 0.4, c[2] * 0.4]
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

fn sleek_hull(ctx: &PartCtx) -> Generator {
    // A tapered, prow-swept hull — narrower at the top and bent forward.
    prim(
        with_torture(
            cuboid(
                [0.6, 0.28, 2.4],
                ctx.materials.body(ctx.palette.secondary_accent),
            ),
            0.0,
            0.3,
            [0.0, 0.0, 0.4],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

fn bow_ram(ctx: &PartCtx) -> Generator {
    // A forward-pointing ram cone (apex along +Z).
    prim(
        cone(
            0.12,
            0.5,
            10,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    )
}

fn bow_figurehead(ctx: &PartCtx) -> Generator {
    let mut f = prim(
        sphere(0.11, 3, ctx.materials.trim(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    f.children.push(prim(
        cone(
            0.05,
            0.18,
            8,
            ctx.materials.accent(ctx.palette.primary_accent),
        ),
        [0.0, 0.12, 0.0],
        id_quat(),
    ));
    f
}

fn funnel(ctx: &PartCtx) -> Generator {
    // A flared (negative-taper) sooty funnel.
    let soot = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    prim(
        with_torture(cylinder(0.1, 0.5, 12, soot), 0.0, -0.25, [0.0, 0.0, 0.0]),
        [0.0, 0.25, 0.0],
        id_quat(),
    )
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------

fn teardrop_envelope(ctx: &PartCtx) -> Generator {
    // A pointed cigar built from composed lobes — like the default envelope
    // but tapering to a sharper bow. Crucially it sets **no** root scale: it
    // is a structural root, and the assembler mounts the gondola / fins as
    // children, which a root scale would stretch and fling (see
    // `super::defaults::envelope`).
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let mut env = prim(sphere(0.82, 4, body.clone()), [0.0, 0.0, 0.0], id_quat());
    env.children.push(prim(
        sphere(0.5, 4, body.clone()),
        [0.0, 0.0, 0.9],
        id_quat(),
    ));
    env.children
        .push(prim(sphere(0.66, 4, body), [0.0, 0.0, -0.6], id_quat()));
    // A pointed bow finial (+Z).
    env.children.push(prim(
        cone(
            0.16,
            0.5,
            10,
            ctx.materials.trim(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 1.3],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    env
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn bubble_canopy(ctx: &PartCtx) -> Generator {
    // A flattened windshield bubble (matches the default canopy's footprint
    // on the cabin) rather than a full gumball sphere.
    let mut c = prim(
        sphere(0.3, 3, ctx.materials.glass(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    c.transform.scale = Fp3([1.0, 0.62, 1.15]);
    // Glowing rim around the base.
    c.children.push(prim(
        torus(0.02, 0.3, ctx.materials.glow(ctx.palette.primary_accent)),
        [0.0, -0.1, 0.0],
        id_quat(),
    ));
    c
}

fn twin_pipes(ctx: &PartCtx) -> Generator {
    // Two stern exhaust stacks.
    let pipe = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let mut e = prim(
        cylinder(0.04, 0.35, 10, pipe.clone()),
        [-0.08, 0.15, 0.0],
        id_quat(),
    );
    e.children.push(prim(
        cylinder(0.04, 0.35, 10, pipe),
        [0.16, 0.0, 0.0],
        id_quat(),
    ));
    e
}

// ---------------------------------------------------------------------------
// Cross-family ornaments
// ---------------------------------------------------------------------------

fn pennant(ctx: &PartCtx) -> Generator {
    let mut p = prim(
        cylinder(
            0.01,
            0.32,
            6,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.16, 0.0],
        id_quat(),
    );
    p.children.push(prim(
        // 0.01 is the sanitiser's minimum cuboid dimension — a thinner flag
        // would be clamped and diverge from what peers render.
        cuboid(
            [0.18, 0.10, 0.01],
            ctx.materials.cloth(ctx.palette.primary_accent),
        ),
        [0.10, 0.10, 0.0],
        id_quat(),
    ));
    p
}

fn neon_strip(ctx: &PartCtx) -> Generator {
    prim(
        cuboid(
            [0.4, 0.02, 0.02],
            ctx.materials.glow(ctx.palette.primary_accent),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

static SLEEK_HULL: PartDef = PartDef {
    slug: "boat_hull_sleek",
    name: "Sleek Hull",
    slot: PartSlot::Hull,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: sleek_hull,
};
static BOW_RAM: PartDef = PartDef {
    slug: "boat_bow_ram",
    name: "Ram Prow",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_ram,
};
static BOW_FIGUREHEAD: PartDef = PartDef {
    slug: "boat_bow_figurehead",
    name: "Figurehead",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_figurehead,
};
static FUNNEL: PartDef = PartDef {
    slug: "boat_stack_funnel",
    name: "Funnel Stack",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: funnel,
};
static TEARDROP_ENVELOPE: PartDef = PartDef {
    slug: "airship_envelope_teardrop",
    name: "Teardrop Envelope",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: teardrop_envelope,
};
static BUBBLE_CANOPY: PartDef = PartDef {
    slug: "skiff_canopy_bubble",
    name: "Bubble Canopy",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bubble_canopy,
};
static TWIN_PIPES: PartDef = PartDef {
    slug: "skiff_exhaust_twin_pipes",
    name: "Twin Exhaust",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: twin_pipes,
};
static PENNANT: PartDef = PartDef {
    slug: "veh_orn_pennant",
    name: "Pennant",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pennant,
};
static NEON_STRIP: PartDef = PartDef {
    slug: "veh_orn_neon_strip",
    name: "Neon Strip",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_strip,
};

/// Every styled vehicle part.
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
    &SLEEK_HULL,
    &BOW_RAM,
    &BOW_FIGUREHEAD,
    &FUNNEL,
    &TEARDROP_ENVELOPE,
    &BUBBLE_CANOPY,
    &TWIN_PIPES,
    &PENNANT,
    &NEON_STRIP,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::parts::parts_for_avatar;
    use crate::seeded_defaults::{OrnatenessTier, WearTier};

    #[test]
    fn every_styled_part_builds_and_is_tagged() {
        let ctx = PartCtx::for_seed(13, "did:plc:veh");
        for part in ENTRIES {
            assert!(!part.styles().is_empty(), "{} is untagged", part.slug());
            assert!(!part.chassis().is_empty(), "{} no chassis", part.slug());
            let a = part.build(&ctx);
            let b = part.build(&ctx);
            assert_eq!(a, b, "{} non-deterministic", part.slug());
        }
    }

    #[test]
    fn steam_boat_can_fit_a_funnel_stack() {
        let stacks: Vec<&str> = parts_for_avatar(
            ChassisFamily::Boat,
            PartSlot::Stack,
            ThemeArchetype::Steampunk,
            OrnatenessTier::Adorned,
            WearTier::Worn,
        )
        .map(|p| p.slug())
        .collect();
        assert!(stacks.contains(&"boat_stack_funnel"), "got {stacks:?}");
    }
}
