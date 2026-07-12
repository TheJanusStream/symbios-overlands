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

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_y, quat_z, sphere,
    superellipsoid, torus, with_cut, with_shape,
};
use crate::pds::avatar::parts::defaults::skiff::{
    push_wheel_fenders, skiff_colors, skiff_dims, skiff_wheel_anchors,
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
const HISTORIC: &[ThemeArchetype] = &[
    Medieval,
    Nordic,
    WildWest,
    PostApoc,
    Fantasy,
    AncientClassical,
];

fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

fn darken(c: [f32; 3]) -> [f32; 3] {
    shade(c, 0.4)
}

/// Blueprint mast height (deck → masthead), or the pre-blueprint nominal (a
/// boat ctx always carries a blueprint; the fallback is defensive, and lets a
/// styled mast still build valid geometry when the round-trip test exercises it
/// against a non-boat seed).
fn mast_height(ctx: &PartCtx) -> f32 {
    ctx.boat().map_or(0.42, |b| b.mast_h)
}

/// Boat deck footprint multipliers `(dw, dl)` — the seeded beam / length over
/// the nominal, so a deck variant scales with its hull like the default deck.
fn deck_dims(ctx: &PartCtx) -> (f32, f32) {
    let (beam, length) = ctx.boat().map_or((0.5, 1.32), |b| (b.beam, b.hull_len));
    (beam / 0.5, length / 1.32)
}

// ---------------------------------------------------------------------------
// Boat
// ---------------------------------------------------------------------------

fn bow_ram(ctx: &PartCtx) -> Generator {
    // A forward-pointing ram cone. quat_x(+90°) sends the cone apex (local +Y)
    // to +Z — the authored bow direction (the assembler yaws the craft 180° so
    // +Z reads as travel-forward), matching the sibling hull prow in
    // `defaults::boat`. (A −90° here aimed the ram astern, base-first — #779.)
    prim(
        cone(
            0.12,
            0.5,
            10,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
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

fn mast_square_rig(ctx: &PartCtx) -> Generator {
    // A square-rigged mast (historic moods): a yard slung athwartships with a
    // broad sail hung below it. The sail fills the crossbar so it reads as a
    // square-rigger, never a bare "crucifix"; set across the boat it shows its
    // full face from ahead, where the fore-and-aft default sail is edge-on.
    let spar = ctx.materials.metal(ctx.palette.secondary_accent);
    let canvas = ctx.materials.cloth(ctx.palette.primary_accent);
    let flag = ctx.materials.accent(ctx.palette.tertiary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.017, h, 8, spar.clone()),
        [0.0, h * 0.5, 0.0],
        quat_xyzw(quat_x(-0.03)),
    );
    // Yard: a crossbar laid athwartships (along X) high on the mast.
    let yard = h * 0.66;
    root.children.push(prim(
        cylinder(0.012, yard, 6, spar.clone()),
        [0.0, h * 0.28, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Square sail hung below the yard (broad in X, thin in Z), leaned forward a
    // touch as if drawing wind. taper narrows the foot slightly.
    let sail_w = yard * 0.9;
    let sail_h = h * 0.52;
    root.children.push(prim(
        with_shape(
            cuboid([sail_w, sail_h, 0.012], canvas),
            [0.08, 0.0],
            [0.0, 0.0, 0.05],
            [0.0, 0.0],
        ),
        [0.0, h * 0.28 - sail_h * 0.5, 0.0],
        id_quat(),
    ));
    // Masthead pennant streaming aft.
    root.children.push(prim(
        cuboid([0.012, 0.05, 0.13], flag),
        [0.0, h * 0.46, -0.06],
        id_quat(),
    ));
    root
}

fn mast_antenna(ctx: &PartCtx) -> Generator {
    // A comms mast for tech moods: a pole bristling with whip antennas, a
    // canted dish, and a beacon — no sail, so it reads as a sensor cluster
    // rather than a crossbar.
    let pole = ctx.materials.metal(ctx.palette.tertiary_accent);
    let whip = ctx.materials.trim(ctx.palette.secondary_accent);
    let dish = ctx.materials.metal(darken(ctx.palette.primary_accent));
    let beacon = ctx.materials.glow(ctx.palette.primary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.02, h, 8, pole.clone()),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    );
    // Cross-spar with two boxy sensor pods.
    root.children.push(prim(
        cylinder(0.01, h * 0.4, 6, pole.clone()),
        [0.0, h * 0.16, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.04, 0.05, 0.05], whip.clone()),
            [s * h * 0.19, h * 0.16, 0.0],
            id_quat(),
        ));
    }
    // Whip antennas of staggered height poking above the masthead.
    for (dx, dz, hh) in [(-0.02, 0.02, 0.34), (0.025, -0.015, 0.26), (0.0, 0.03, 0.2)] {
        root.children.push(prim(
            cylinder(0.01, h * hh, 4, whip.clone()),
            [dx, h * 0.86 + h * hh * 0.5, dz],
            id_quat(),
        ));
    }
    // A canted dish part-way up (a shallow bowl via a profile-cut sphere).
    root.children.push(prim(
        with_cut(sphere(0.09, 3, dish), [0.0, 1.0], [0.0, 0.5], 0.0),
        [0.11, h * 0.36, 0.02],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Beacon at the very top.
    root.children.push(prim(
        cylinder(0.02, 0.03, 8, beacon),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    ));
    root
}

fn mast_derrick(ctx: &PartCtx) -> Generator {
    // A cargo derrick for industrial moods: a stout king-post with a raking jib
    // boom and a hanging block-and-hook — working gear, not a crucifix.
    let steel = ctx.materials.metal(ctx.palette.secondary_accent);
    let tackle = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let hook = ctx.materials.trim(ctx.palette.tertiary_accent);
    let h = mast_height(ctx);

    let mut root = prim(
        cylinder(0.024, h, 8, steel.clone()),
        [0.0, h * 0.5, 0.0],
        id_quat(),
    );
    // Raking jib boom from low on the post up-and-forward (+Z).
    let jib = h * 0.95;
    let ang = 0.8_f32; // rotate +Y toward +Z
    let (sa, ca) = ang.sin_cos();
    let base_y = h * 0.15;
    root.children.push(prim(
        cylinder(0.016, jib, 6, steel.clone()),
        [0.0, base_y + 0.5 * jib * ca, 0.5 * jib * sa],
        quat_xyzw(quat_x(ang)),
    ));
    // Block-and-hook hanging from the jib tip.
    let tip = [0.0, base_y + jib * ca, jib * sa];
    let drop = h * 0.35;
    root.children.push(prim(
        cylinder(0.01, drop, 4, tackle.clone()),
        [tip[0], tip[1] - drop * 0.5, tip[2]],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid([0.05, 0.06, 0.04], hook),
        [tip[0], tip[1] - drop, tip[2]],
        id_quat(),
    ));
    // Winch drum at the foot.
    root.children.push(prim(
        cylinder(0.03, 0.1, 8, tackle),
        [0.0, base_y * 0.4, -0.06],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    root
}

fn deck_cargo(ctx: &PartCtx) -> Generator {
    // An open working deck (industrial / grubby moods): a low sole carrying
    // lashed crates and a raised hatch instead of a cabin trunk.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.7));
    let hatch = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    let crate_a = ctx.materials.body(ctx.palette.secondary_accent);
    let crate_b = ctx.materials.body(shade(ctx.palette.tertiary_accent, 0.8));
    let (dw, dl) = deck_dims(ctx);

    let mut deck = prim(
        cuboid([0.35 * dw, 0.045, 0.64 * dl], sole.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Raised cargo hatch amidships-forward.
    deck.children.push(prim(
        cuboid([0.26 * dw, 0.07, 0.24 * dl], hatch.clone()),
        [0.0, 0.045, 0.2 * dl],
        id_quat(),
    ));
    // A stack of lashed crates of staggered size / colour aft.
    deck.children.push(prim(
        cuboid([0.14 * dw, 0.13, 0.16 * dl], crate_a),
        [-0.07 * dw, 0.088, -0.16 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cuboid([0.12 * dw, 0.1, 0.13 * dl], crate_b),
        [0.09 * dw, 0.072, -0.2 * dl],
        id_quat(),
    ));
    // Low bulwarks down each side of the working deck.
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cuboid([0.02, 0.05, 0.5 * dl], hatch.clone()),
            [s * 0.17 * dw, 0.03, 0.0],
            id_quat(),
        ));
    }
    deck
}

fn deck_bench(ctx: &PartCtx) -> Generator {
    // An open leisure deck (regal / genteel moods): a low sole with a cushioned
    // lounge bench and a small helm console, no cabin trunk.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.72));
    let trimwork = ctx.materials.trim(ctx.palette.tertiary_accent);
    let cushion = ctx.materials.cloth(ctx.palette.secondary_accent);
    let (dw, dl) = deck_dims(ctx);

    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], sole.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Lounge bench aft: a cushioned seat with a low backrest.
    deck.children.push(prim(
        cuboid([0.28 * dw, 0.05, 0.1 * dl], cushion.clone()),
        [0.0, 0.05, -0.22 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cuboid([0.28 * dw, 0.11, 0.03], cushion),
        [0.0, 0.09, -0.28 * dl],
        id_quat(),
    ));
    // Small helm console forward, with a wheel post.
    deck.children.push(prim(
        cuboid([0.18 * dw, 0.09, 0.06], sole),
        [0.0, 0.045, 0.24 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        torus(0.012, 0.05, trimwork.clone()),
        [0.0, 0.13, 0.22 * dl],
        quat_xyzw(quat_x(0.5)),
    ));
    // Bright toe-rail posts down each side (genteel trim).
    for s in [-1.0f32, 1.0] {
        for z in [0.1 * dl, -0.05 * dl] {
            deck.children.push(prim(
                cylinder(0.012, 0.08, 6, trimwork.clone()),
                [s * 0.17 * dw, 0.04, z],
                id_quat(),
            ));
        }
    }
    deck
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

// --- Skiff chassis variants (#788) -----------------------------------------
//
// The Chassis slot shipped exactly one part; the family docstring promises
// "rover / dune-skiff / trike". Each variant is a full structural root sized
// from the shared [`skiff_dims`] contract, draws its own mudguards via
// [`push_wheel_fenders`] (so the guards match the assembler's wheels), and
// wears the value-floored [`skiff_colors`] scheme (#787). The trike collapses
// its front axle to a single centreline wheel — the assembler keys that off the
// `skiff_chassis_trike` slug.

fn skiff_headlamps(c: &mut Generator, ctx: &PartCtx, xs: &[f32], z: f32) {
    // A dark bezel ring + bright lens per position — shared 3D-relief lamp.
    let bezel = ctx.materials.metal([0.09, 0.09, 0.11]);
    let lamp = ctx.materials.glow([1.0, 0.95, 0.8]);
    for &x in xs {
        c.children.push(prim(
            cylinder(0.05, 0.03, 12, bezel.clone()),
            [x, 0.05, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        c.children.push(prim(
            cylinder(0.036, 0.05, 12, lamp.clone()),
            [x, 0.05, z + 0.01],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
}

fn chassis_dune(ctx: &PartCtx) -> Generator {
    // A dune buggy: a low exposed pod on an open tube frame with a roll bar and
    // an exposed rear engine — no full bodywork.
    let colors = skiff_colors(ctx);
    let pod = ctx.materials.body(colors.body);
    let frame = ctx.materials.metal(colors.trim);
    let dark = ctx.materials.metal(colors.lower);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Low exposed pod tub (open-cockpit feel).
    let mut c = prim(
        superellipsoid([body_w * 0.42, 0.085, body_len * 0.44], 0.4, 0.55, pod),
        [0.0, -0.02, 0.04 * dl],
        id_quat(),
    );
    // Exposed rear engine block.
    c.children.push(prim(
        superellipsoid([0.24 * dw, 0.09, 0.2 * dl], 0.5, 0.6, dark.clone()),
        [0.0, 0.06, -0.42 * dl],
        id_quat(),
    ));
    // Roll bar behind the cockpit: two posts + a top tube.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.018, 0.28, 8, frame.clone()),
            [s * 0.22 * dw, 0.12, -0.14 * dl],
            id_quat(),
        ));
    }
    c.children.push(prim(
        cylinder(0.018, 0.46 * dw, 8, frame.clone()),
        [0.0, 0.26, -0.14 * dl],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Nerf-bar side tubes + a front brush-bar carrying the lamps.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.014, 0.72 * dl, 6, frame.clone()),
            [s * 0.4 * dw, -0.05, 0.0],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c.children.push(prim(
        cylinder(0.016, 0.5 * dw, 8, frame),
        [0.0, 0.03, 0.5 * body_len],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, false), wheel_r, &dark);
    skiff_headlamps(&mut c, ctx, &[-0.24 * dw, 0.24 * dw], 0.5 * body_len);
    c
}

fn chassis_trike(ctx: &PartCtx) -> Generator {
    // A three-wheeler: a wide two-wheel rear cabin tapering to a single-wheel
    // nose (the assembler collapses the front axle to centreline for this slug).
    let colors = skiff_colors(ctx);
    let body = ctx.materials.body(colors.body);
    let dark = ctx.materials.metal(colors.lower);
    let trim = ctx.materials.metal(colors.trim);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Wide rear cabin (the canopy seats here).
    let mut c = prim(
        superellipsoid(
            [body_w * 0.48, 0.1, body_len * 0.34],
            0.42,
            0.5,
            body.clone(),
        ),
        [0.0, 0.0, -0.16 * dl],
        id_quat(),
    );
    // Narrow forward spine reaching the single nose wheel.
    c.children.push(prim(
        superellipsoid([0.19 * dw, 0.08, body_len * 0.4], 0.42, 0.55, body.clone()),
        [0.0, -0.015, 0.26 * dl],
        id_quat(),
    ));
    // A pointed nose fairing over the front wheel.
    c.children.push(prim(
        superellipsoid([0.15 * dw, 0.07, 0.14 * dl], 0.45, 0.6, dark.clone()),
        [0.0, 0.0, 0.52 * dl],
        id_quat(),
    ));
    // Side trim spear along the spine.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.02, 0.03, 0.7 * dl], trim.clone()),
            [s * 0.2 * dw, 0.0, 0.1 * dl],
            id_quat(),
        ));
    }
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, true), wheel_r, &dark);
    // A single central headlamp on the nose.
    skiff_headlamps(&mut c, ctx, &[0.0], 0.56 * body_len);
    // Rear tail lamps.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.036, 0.04, 10, taillight.clone()),
            [sx * 0.26 * dw, 0.08, -0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c
}

fn chassis_armored(ctx: &PartCtx) -> Generator {
    // A plated rover (martial): angular armour panels over a boxy hull, a sloped
    // glacis, side skirts and a skid plate — faceted where the civilian body is
    // rounded.
    let colors = skiff_colors(ctx);
    let body = ctx.materials.body(colors.body);
    let plate = ctx.materials.metal(colors.lower);
    let trim = ctx.materials.metal(colors.trim);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);
    let dims = skiff_dims(ctx);
    let (body_w, body_len, _, _, _, wheel_r) = dims;
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Boxy armoured hull (a faceted cuboid, gently tumblehomed).
    let mut c = prim(
        with_shape(
            cuboid([body_w, 0.2, body_len], body.clone()),
            [0.1, 0.06],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Sloped front glacis plate.
    c.children.push(prim(
        cuboid([0.72 * dw, 0.16, 0.03], plate.clone()),
        [0.0, 0.05, 0.5 * dl],
        quat_xyzw(quat_x(-0.5)),
    ));
    // Side skirt plates.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.16, 1.02 * dl], plate.clone()),
            [s * 0.5 * body_w, -0.02, 0.0],
            id_quat(),
        ));
    }
    // Armoured cabin block (the canopy seats on this).
    c.children.push(prim(
        cuboid([0.62 * dw, 0.14, 0.5 * dl], body),
        [0.0, 0.15, -0.14 * dl],
        id_quat(),
    ));
    // Underbody skid plate.
    c.children.push(prim(
        cuboid([0.74 * dw, 0.06, 1.06 * dl], plate.clone()),
        [0.0, -0.13, 0.0],
        id_quat(),
    ));
    // Bolt-strip trim across the glacis.
    c.children.push(prim(
        cuboid([0.5 * dw, 0.03, 0.03], trim),
        [0.0, 0.02, 0.52 * body_len],
        id_quat(),
    ));
    push_wheel_fenders(&mut c, &skiff_wheel_anchors(dims, false), wheel_r, &plate);
    // Slit headlamps set into the glacis (narrow, armoured).
    skiff_headlamps(&mut c, ctx, &[-0.24 * dw, 0.24 * dw], 0.5 * body_len);
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.036, 0.04, 10, taillight.clone()),
            [sx * 0.24 * dw, 0.08, -0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    c
}

// --- Skiff wheel variants (#788) --------------------------------------------
//
// The Wheel slot ships one part repeated to every corner, so a single variant
// changes all of a vehicle's wheels at once. Each keeps the outer radius at the
// blueprint `wheel_r` so it still seats in its guard.

fn wheel_spoked(ctx: &PartCtx) -> Generator {
    // A wagon wheel: a thin tyre with radial spoke bars and a proud hub.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.16;
    let major = wheel_r - minor;
    let mut w = prim(torus(minor, major, tyre), [0.0, 0.0, 0.0], id_quat());
    // Four crossing spoke bars (eight spokes) in the wheel plane.
    for i in 0..4 {
        w.children.push(prim(
            cuboid([major * 1.9, 0.02, 0.03], rim.clone()),
            [0.0, 0.0, 0.0],
            quat_xyzw(quat_y(i as f32 * FRAC_PI_4)),
        ));
    }
    // Proud hub cap.
    w.children.push(prim(
        cylinder(major * 0.26, 0.14, 12, hub),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w
}

fn wheel_knobby(ctx: &PartCtx) -> Generator {
    // A fat off-road tyre studded with tread knobs around the crown.
    let tyre = ctx.materials.metal([0.08, 0.08, 0.09]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.42;
    let major = wheel_r - minor;
    let mut w = prim(
        torus(minor, major, tyre.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    let n = 10;
    for i in 0..n {
        let ang = i as f32 / n as f32 * TAU;
        let (s, cc) = ang.sin_cos();
        let r = major + minor * 0.72;
        w.children.push(prim(
            cuboid([0.045, 0.055, 0.045], tyre.clone()),
            [cc * r, 0.0, s * r],
            quat_xyzw(quat_y(ang)),
        ));
    }
    // Deep rim plate + hub.
    w.children.push(prim(
        cylinder(major * 0.62, 0.16, 12, rim),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w.children.push(prim(
        cylinder(major * 0.24, 0.18, 10, hub),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    w
}

fn wheel_glow(ctx: &PartCtx) -> Generator {
    // A hover-tech wheel: a slim tyre over a glowing hub disc on each face.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let glow = ctx.materials.glow(ctx.palette.primary_accent);
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.24;
    let major = wheel_r - minor;
    let mut w = prim(torus(minor, major, tyre), [0.0, 0.0, 0.0], id_quat());
    // Rim ring + a glowing disc on each face.
    w.children.push(prim(
        cylinder(major * 0.86, 0.1, 20, rim),
        [0.0, 0.0, 0.0],
        id_quat(),
    ));
    for s in [-1.0f32, 1.0] {
        w.children.push(prim(
            cylinder(major * 0.7, 0.02, 20, glow.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
    }
    w
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
static MAST_SQUARE_RIG: PartDef = PartDef {
    slug: "boat_mast_square_rig",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_square_rig,
};
static MAST_ANTENNA: PartDef = PartDef {
    slug: "boat_mast_antenna",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_antenna,
};
static MAST_DERRICK: PartDef = PartDef {
    slug: "boat_mast_derrick",
    slot: PartSlot::Mast,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: mast_derrick,
};
static DECK_CARGO: PartDef = PartDef {
    slug: "boat_deck_cargo",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_cargo,
};
static DECK_BENCH: PartDef = PartDef {
    slug: "boat_deck_bench",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: REGAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_bench,
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
static CHASSIS_DUNE: PartDef = PartDef {
    slug: "skiff_chassis_dune",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_dune,
};
static CHASSIS_TRIKE: PartDef = PartDef {
    slug: "skiff_chassis_trike",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_trike,
};
static CHASSIS_ARMORED: PartDef = PartDef {
    slug: "skiff_chassis_armored",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_armored,
};
static WHEEL_SPOKED: PartDef = PartDef {
    slug: "skiff_wheel_spoked",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_spoked,
};
static WHEEL_KNOBBY: PartDef = PartDef {
    slug: "skiff_wheel_knobby",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_knobby,
};
static WHEEL_GLOW: PartDef = PartDef {
    slug: "skiff_wheel_glow",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_glow,
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
    &MAST_SQUARE_RIG,
    &MAST_ANTENNA,
    &MAST_DERRICK,
    &DECK_CARGO,
    &DECK_BENCH,
    &TEARDROP_ENVELOPE,
    &BUBBLE_CANOPY,
    &TWIN_PIPES,
    &CHASSIS_DUNE,
    &CHASSIS_TRIKE,
    &CHASSIS_ARMORED,
    &WHEEL_SPOKED,
    &WHEEL_KNOBBY,
    &WHEEL_GLOW,
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
