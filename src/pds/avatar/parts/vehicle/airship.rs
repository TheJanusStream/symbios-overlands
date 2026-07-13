//! Styled airship parts — a teardrop envelope, engine pods, and gondola
//! variants. See the [`super`] module docstring for the mood-group / band
//! tagging scheme; the shared envelope / gondola / pod primitives live in
//! [`crate::pds::avatar::parts::defaults::airship`].

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, helix, id_quat, prim, quat_x, quat_xyzw, sphere, torus,
};
use crate::pds::avatar::parts::defaults::airship::{
    GondolaDims, airship_colors, ctx_profile, dress_gondola, env_core, envelope_material,
    lathe_spindle, pod_nacelle, pod_pylon, pod_tail, push_env_gores, push_env_rings,
};
use crate::pds::avatar::parts::defaults::common::shade;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{AIRSHIP, GRUBBY, HISTORIC, NEON, STEAM};

fn teardrop_envelope(ctx: &PartCtx) -> Generator {
    // Steampunk teardrop — a single smooth Lathe spindle whose profile is a
    // SHARP nose over a FULL rounded tail with the waist biased forward (the
    // classic teardrop), no sphere↔cone junction (#791). Built as a child of a
    // hidden unscaled core (the assembler mounts the gondola / fins to the root,
    // which a root scale would fling). Shares the airship gore/ring/registry
    // surface pass so it reads as taut doped fabric, not a glossy blob.
    let c = airship_colors(ctx);
    let skin = envelope_material(c.envelope);
    let frame = ctx.materials.metal(c.frame);
    let stripe = ctx.materials.trim(c.stripe);
    let p = ctx_profile(ctx, "airship_envelope_teardrop");
    let mut env = env_core(&skin);
    env.children.push(lathe_spindle(&p, 0.0, skin));
    push_env_gores(&mut env, &p, 0.0, 7, &frame, Some(&stripe));
    push_env_rings(&mut env, &p, 0.0, 2, &frame);
    // Pointed nose finial just past the sharp nose.
    env.children.push(prim(
        sphere(0.1, 3, stripe),
        [0.0, 0.0, p.nose_z() + 0.04],
        id_quat(),
    ));
    env
}

fn pod_ducted(ctx: &PartCtx) -> Generator {
    // NEON engine pod: a ducted fan — a short fat nacelle behind a cowl ring
    // that shrouds a glowing fan disc, so the tech airships read as fan-driven.
    // Wears the ship's `accent` metal + a normalized glow so it sits in the
    // two-hue scheme (#789) rather than drawing a fresh colour.
    let c = airship_colors(ctx);
    let body = ctx.materials.metal(c.accent);
    let ring = ctx.materials.metal(c.frame);
    let glow = ctx.materials.glow(c.window);

    // Short fat nacelle.
    let mut p = pod_nacelle(0.15, 0.36, 14, body);
    // A fat shroud ring (the duct cowl) proud of the intake — a closed hoop, so
    // the pod reads as ducted, not an open airscrew like the default (#790
    // review: it looked identical to the default open-prop).
    p.children.push(prim(
        torus(0.05, 0.19, ring.clone()),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // A bright face-on fan disc filling the duct (a flat glowing cylinder facing
    // +Z), so the ducted read carries head-on where the pods are most visible.
    p.children.push(prim(
        cylinder(0.16, 0.02, 18, glow.clone()),
        [0.0, 0.0, 0.2],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // A dark hub + spokes over the disc so it reads as a spinning fan, not a lamp.
    p.children.push(prim(
        sphere(0.045, 3, ring.clone()),
        [0.0, 0.0, 0.23],
        id_quat(),
    ));
    // (Spoke thickness stays ≥ the sanitiser's 0.01 min cuboid dim.)
    for size in [[0.28, 0.02, 0.012], [0.02, 0.28, 0.012]] {
        p.children.push(prim(
            cuboid(size, ring.clone()),
            [0.0, 0.0, 0.225],
            id_quat(),
        ));
    }
    p.children.push(pod_tail(-0.22, ring.clone()));
    pod_pylon(&mut p, &ring);
    p
}

fn pod_screw(ctx: &PartCtx) -> Generator {
    // STEAM engine pod: a riveted nacelle driving a brass Archimedes screw
    // (Helix prim, #527) — the steampunk airscrew. The screw + spinner wear the
    // registry `stripe` (brass) pop; the boiler bands the `frame` metal.
    let c = airship_colors(ctx);
    let body = ctx.materials.metal(c.accent);
    let dark = ctx.materials.metal(c.frame);
    let brass = ctx.materials.trim(c.stripe);

    let mut p = pod_nacelle(0.13, 0.5, 12, body.clone());
    // Brass screw at the front (Helix laid along Z via quat_x(90°)).
    p.children.push(prim(
        helix(0.11, 0.02, 0.11, 2.5, 16, brass.clone()),
        [0.0, 0.0, 0.18],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Spinner cone capping the screw shaft.
    p.children.push(prim(
        cone(0.07, 0.14, 10, brass),
        [0.0, 0.0, 0.34],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Two boiler bands round the barrel.
    for z in [-0.14f32, 0.06] {
        p.children.push(prim(
            torus(0.018, 0.135, dark.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    p.children.push(pod_tail(-0.3, body));
    pod_pylon(&mut p, &dark);
    p
}

fn gondola_basket(ctx: &PartCtx) -> Generator {
    // Open wicker basket (a balloon-style car): a floor + four low woven walls
    // around an OPEN top, ringed by a bright rim — the HISTORIC alternative to
    // the enclosed cabin. Built as explicit walls rather than a hollowed
    // superellipsoid, which read as a solid shortened box from the side/front
    // (#790 review); the open-topped box reads unmistakably as a tub. Hidden hub
    // root at origin so the shared dressing seats correctly (the env_core
    // pattern). No glazing (it's open); the dressing hangs lanterns + a view
    // dome; the shallow floor is its underside.
    let c = airship_colors(ctx);
    let wicker = ctx.materials.cloth(c.accent);
    let floor = ctx.materials.cloth(shade(c.accent, 0.8));
    let rim = ctx.materials.trim(c.stripe);
    let dims = GondolaDims {
        hw: 0.24,
        hh: 0.15,
        hl: 0.42,
        keel_y: -0.15,
    };
    let mut g = prim(
        cuboid([0.03, 0.03, 0.03], wicker.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Floor pan.
    g.children.push(prim(
        cuboid([dims.hw * 2.0, 0.05, dims.hl * 2.0], floor),
        [0.0, -dims.hh, 0.0],
        id_quat(),
    ));
    // Four low woven walls, standing from the floor to a low open rim (~0.7 of
    // full height), leaving the top open.
    let wall_h = dims.hh * 1.5;
    let wall_cy = -dims.hh + wall_h * 0.5;
    for sx in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.03, wall_h, dims.hl * 2.0], wicker.clone()),
            [sx * dims.hw, wall_cy, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.0, wall_h, 0.03], wicker.clone()),
            [0.0, wall_cy, sz * dims.hl],
            id_quat(),
        ));
    }
    // Bright rim rails capping the open top.
    let top = -dims.hh + wall_h;
    for sx in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.028, 0.028, dims.hl * 2.05], rim.clone()),
            [sx * dims.hw, top, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.05, 0.028, 0.028], rim.clone()),
            [0.0, top, sz * dims.hl],
            id_quat(),
        ));
    }
    dress_gondola(&mut g, ctx, dims);
    g
}

fn gondola_cargo(ctx: &PartCtx) -> Generator {
    // Girder cargo frame: an open box frame of girders over a floor plate,
    // holding a couple of lashed crates — the GRUBBY freight hauler. Built on a
    // hidden hub at the car centre (origin) so the shared dressing seats
    // correctly — the visible frame hangs off it (the env_core pattern; a
    // floor-plate root would shift every dressing child down by its offset).
    let c = airship_colors(ctx);
    let girder = ctx.materials.metal(c.frame);
    let plate = ctx.materials.body(shade(c.accent, 0.8));
    let crate_mat = ctx.materials.body(c.accent);
    let dims = GondolaDims {
        hw: 0.24,
        hh: 0.16,
        hl: 0.46,
        // The deck pan is the underside; seat the view port just below it.
        keel_y: -0.18,
    };
    let mut g = prim(
        cuboid([0.03, 0.03, 0.03], girder.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Floor plate.
    g.children.push(prim(
        cuboid([dims.hw * 2.0, 0.04, dims.hl * 2.0], plate),
        [0.0, -dims.hh, 0.0],
        id_quat(),
    ));
    // Four corner posts (spanning floor→roof) + a top perimeter frame.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            g.children.push(prim(
                cuboid([0.03, dims.hh * 2.0, 0.03], girder.clone()),
                [sx * dims.hw * 0.94, 0.0, sz * dims.hl * 0.94],
                id_quat(),
            ));
        }
        g.children.push(prim(
            cuboid([0.03, 0.03, dims.hl * 2.0], girder.clone()),
            [sx * dims.hw * 0.94, dims.hh, 0.0],
            id_quat(),
        ));
    }
    for sz in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([dims.hw * 2.0, 0.03, 0.03], girder.clone()),
            [0.0, dims.hh, sz * dims.hl * 0.94],
            id_quat(),
        ));
    }
    // A couple of lashed crates sitting on the deck.
    g.children.push(prim(
        cuboid([0.17, 0.15, 0.17], crate_mat.clone()),
        [-0.06, -dims.hh + 0.095, 0.13],
        id_quat(),
    ));
    g.children.push(prim(
        cuboid([0.13, 0.12, 0.13], crate_mat),
        [0.08, -dims.hh + 0.08, -0.15],
        id_quat(),
    ));
    dress_gondola(&mut g, ctx, dims);
    g
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub(super) static TEARDROP_ENVELOPE: PartDef = PartDef {
    slug: "airship_envelope_teardrop",
    slot: PartSlot::Envelope,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: teardrop_envelope,
};
pub(super) static POD_DUCTED: PartDef = PartDef {
    slug: "airship_pod_ducted",
    slot: PartSlot::Pod,
    chassis: AIRSHIP,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pod_ducted,
};
pub(super) static POD_SCREW: PartDef = PartDef {
    slug: "airship_pod_screw",
    slot: PartSlot::Pod,
    chassis: AIRSHIP,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: pod_screw,
};
pub(super) static GONDOLA_BASKET: PartDef = PartDef {
    slug: "airship_gondola_basket",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    styles: HISTORIC,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: gondola_basket,
};
pub(super) static GONDOLA_CARGO: PartDef = PartDef {
    slug: "airship_gondola_cargo",
    slot: PartSlot::Gondola,
    chassis: AIRSHIP,
    styles: GRUBBY,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: gondola_cargo,
};
