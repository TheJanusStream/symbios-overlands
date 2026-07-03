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
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, torus,
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
    // Twin hover-thruster pods at the stern (reinterpreted from a smokestack for
    // the hover-skiff): a housing with two aft-facing glowing exhaust bells.
    let housing = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let glow = ctx.materials.glow(ctx.palette.tertiary_accent);
    let mut root = prim(
        cuboid([0.32, 0.16, 0.2], housing.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for s in [-1.0f32, 1.0] {
        // Nozzle barrel poking aft (-Z).
        root.children.push(prim(
            cylinder(0.07, 0.14, 12, housing.clone()),
            [s * 0.09, 0.0, -0.13],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        // Glowing exhaust core at the bell mouth.
        root.children.push(prim(
            cylinder(0.05, 0.04, 12, glow.clone()),
            [s * 0.09, 0.0, -0.19],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    root
}

// ---------------------------------------------------------------------------
// Airship
// ---------------------------------------------------------------------------

fn teardrop_envelope(ctx: &PartCtx) -> Generator {
    // A *smooth* teardrop gas-bag: a single scaled-ellipsoid child of a hidden
    // core (the root carries **no** scale — the assembler mounts the gondola /
    // fins to it, which a root scale would stretch and fling), with a long
    // pointed nose cone making the teardrop. Replaces the old lumpy lobes.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);
    let ring = ctx.materials.metal(ctx.palette.secondary_accent);
    let mut env = prim(
        cuboid([0.3, 0.3, 1.5], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let mut bag = prim(sphere(0.8, 4, body.clone()), [0.0, 0.0, -0.15], id_quat());
    bag.transform.scale = Fp3([0.92, 0.92, 1.5]);
    env.children.push(bag);
    // Long pointed teardrop nose at the bow (+Z), apex forward, blending out of
    // the bag.
    env.children.push(prim(
        cone(0.55, 0.95, 12, body.clone()),
        [0.0, 0.0, 0.7],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    env.children
        .push(prim(sphere(0.1, 3, nose), [0.0, 0.0, 1.18], id_quat()));
    // Frame rings encircling the bag.
    for z in [-0.5f32, 0.05] {
        env.children.push(prim(
            torus(0.018, 0.76, ring.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    env
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

fn bubble_canopy(ctx: &PartCtx) -> Generator {
    // A sleek, elongated teardrop cockpit bubble — the sporty alternative to the
    // default boxy cabin greenhouse.
    let mut c = prim(
        sphere(0.3, 4, ctx.materials.glass(ctx.palette.secondary_accent)),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    c.transform.scale = Fp3([0.82, 0.6, 1.08]);
    // Glowing rim around the base.
    c.children.push(prim(
        torus(0.02, 0.3, ctx.materials.glow(ctx.palette.primary_accent)),
        [0.0, -0.2, 0.0],
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

static BOW_RAM: PartDef = PartDef {
    slug: "boat_bow_ram",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_ram,
};
static BOW_FIGUREHEAD: PartDef = PartDef {
    slug: "boat_bow_figurehead",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_figurehead,
};
static FUNNEL: PartDef = PartDef {
    slug: "boat_stack_funnel",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: funnel,
};
static TEARDROP_ENVELOPE: PartDef = PartDef {
    slug: "airship_envelope_teardrop",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: teardrop_envelope,
};
static BUBBLE_CANOPY: PartDef = PartDef {
    slug: "skiff_canopy_bubble",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bubble_canopy,
};
static TWIN_PIPES: PartDef = PartDef {
    slug: "skiff_exhaust_twin_pipes",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: twin_pipes,
};
static PENNANT: PartDef = PartDef {
    slug: "veh_orn_pennant",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pennant,
};
static NEON_STRIP: PartDef = PartDef {
    slug: "veh_orn_neon_strip",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_strip,
};

/// Every styled vehicle part.
pub(super) static ENTRIES: &[&dyn super::BodyPart] = &[
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
        let ctx = PartCtx::for_seed(13);
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
