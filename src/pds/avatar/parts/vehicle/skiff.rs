//! Styled skiff parts — canopies, exhausts, chassis variants, and wheels (#788).
//! See the [`super`] module docstring for the mood-group / band tagging scheme;
//! the shared skiff dims / colours / wheel anchors live in
//! [`crate::pds::avatar::parts::defaults::skiff`].

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_y, quat_z, sphere, superellipsoid,
    torus, with_shape,
};
use crate::pds::avatar::parts::defaults::common::darken;
use crate::pds::avatar::parts::defaults::skiff::{
    push_wheel_fenders, skiff_colors, skiff_dims, skiff_wheel_anchors,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{COASTAL, GRUBBY, HISTORIC, MARTIAL, NEON, SKIFF, UNIVERSAL, WORN_PLUS};

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

fn exhaust_tailpipe(ctx: &PartCtx) -> Generator {
    // The style-universal exhaust floor (empty styles): a single chromed
    // tailpipe running aft with a flared tip, so every skiff carries an exhaust
    // regardless of theme (the twin pipes are grimy / worn; this is the plain
    // fitting under every craft).
    let chrome = ctx.materials.metal([0.6, 0.6, 0.64]);
    // Pipe laid along Z (its local +Y → +Z), centred so most of it emerges aft
    // (-Z) with the forward stub buried in the tail bodywork.
    let mut root = prim(
        cylinder(0.035, 0.34, 12, chrome.clone()),
        [0.0, 0.06, -0.12],
        quat_xyzw(quat_x(FRAC_PI_2)),
    );
    // Flared tip ring at the aft mouth: in the pipe-local frame the barrel axis
    // is +Y, and pipe-local -Y is the aft end, so the default torus wraps it.
    root.children.push(prim(
        torus(0.016, 0.045, chrome),
        [0.0, -0.17, 0.0],
        id_quat(),
    ));
    root
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
// Registry
// ---------------------------------------------------------------------------

pub(super) static BUBBLE_CANOPY: PartDef = PartDef {
    slug: "skiff_canopy_bubble",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    // The sleek teardrop bubble is the sporty / seaside cockpit read (it homes
    // the COASTAL group's skiffs; the tech craft keep the default greenhouse).
    styles: COASTAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bubble_canopy,
};
pub(super) static TWIN_PIPES: PartDef = PartDef {
    slug: "skiff_exhaust_twin_pipes",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    // Sooted twin stacks read on a used / beaten skiff, not a fresh one.
    wear: WORN_PLUS,
    build: twin_pipes,
};
pub(super) static EXHAUST_TAILPIPE: PartDef = PartDef {
    slug: "skiff_exhaust_tailpipe",
    slot: PartSlot::Exhaust,
    chassis: SKIFF,
    // Style-universal exhaust floor: every skiff can carry a tailpipe.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: exhaust_tailpipe,
};
pub(super) static CHASSIS_DUNE: PartDef = PartDef {
    slug: "skiff_chassis_dune",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    // An open buggy is the workaday / off-road read — grimy industrial /
    // frontier craft plus the agrarian / roadside / suburban beaters GRUBBY now
    // folds in.
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_dune,
};
pub(super) static CHASSIS_TRIKE: PartDef = PartDef {
    slug: "skiff_chassis_trike",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_trike,
};
pub(super) static CHASSIS_ARMORED: PartDef = PartDef {
    slug: "skiff_chassis_armored",
    slot: PartSlot::Chassis,
    chassis: SKIFF,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: chassis_armored,
};
pub(super) static WHEEL_SPOKED: PartDef = PartDef {
    slug: "skiff_wheel_spoked",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_spoked,
};
pub(super) static WHEEL_KNOBBY: PartDef = PartDef {
    slug: "skiff_wheel_knobby",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    // Fat off-road tyres are the grimy / frontier / farm / roadside read.
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_knobby,
};
pub(super) static WHEEL_GLOW: PartDef = PartDef {
    slug: "skiff_wheel_glow",
    slot: PartSlot::Wheel,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: wheel_glow,
};
