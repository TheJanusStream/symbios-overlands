//! Airship defaults: envelope forms, gondola, and the tail fin. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, torus, with_shape,
    with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::PartCtx;
use super::common::shade;

/// A scaled-ellipsoid gas bag (a unit sphere scaled to `half`-extents) — the
/// building block of every airship envelope form. The envelope root carries no
/// scale (the assembler mounts gondola / fins to it and a root scale would
/// stretch + fling them), so every bag is a scaled child of a hidden core.
pub(super) fn gas_bag(
    material: &SovereignMaterialSettings,
    center: [f32; 3],
    half: [f32; 3],
) -> Generator {
    let mut bag = prim(sphere(1.0, 4, material.clone()), center, id_quat());
    bag.transform.scale = Fp3(half);
    bag
}

/// A structural frame ring (torus in the plane ⟂ Z) at `z`, major radius `r`.
/// `r` should be ≈ the bag radius at `z` so the band straddles the surface
/// (reads as a frame ring, not a hoop floating proud of the silhouette).
pub(super) fn env_ring(material: &SovereignMaterialSettings, z: f32, r: f32) -> Generator {
    prim(
        torus(0.024, r, material.clone()),
        [0.0, 0.0, z],
        quat_xyzw(quat_x(FRAC_PI_2)),
    )
}

/// Hidden structural core for an airship envelope at the origin.
pub(super) fn env_core(body: &SovereignMaterialSettings) -> Generator {
    prim(
        cuboid([0.3, 0.3, 1.3], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    )
}

pub(super) fn envelope(ctx: &PartCtx) -> Generator {
    // Zeppelin — a long, rigid dirigible: a sleek gas bag with tapered nose +
    // tail cones and prominent segment rings.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let ring = ctx.materials.metal(ctx.palette.secondary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);

    let mut env = env_core(&body);
    // Slim + long (clearly slimmer than the fat blimp).
    env.children
        .push(gas_bag(&body, [0.0, 0.0, 0.0], [0.66, 0.68, 1.55]));
    // Tapered nose cone (apex +Z) and tail cone (apex -Z) for the rigid points.
    env.children.push(prim(
        cone(0.4, 0.5, 12, body.clone()),
        [0.0, 0.0, 1.42],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    env.children.push(prim(
        cone(0.44, 0.55, 12, body.clone()),
        [0.0, 0.0, -1.4],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    // Segment rings (rigid frame) seated at the bag radius so the band
    // straddles the surface — flush, not a hoop floating proud.
    for (z, r) in [
        (-0.92f32, 0.53),
        (-0.46, 0.63),
        (0.0, 0.66),
        (0.46, 0.63),
        (0.92, 0.53),
    ] {
        env.children.push(env_ring(&ring, z, r));
    }
    // Pointed nose finial.
    env.children
        .push(prim(sphere(0.1, 3, nose), [0.0, 0.0, 1.7], id_quat()));
    env
}

pub(super) fn envelope_blimp(ctx: &PartCtx) -> Generator {
    // Blimp — a short, fat, soft non-rigid envelope: rounded ends, only a
    // couple of soft bands, a stubbier silhouette than the zeppelin.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let band = ctx.materials.metal(ctx.palette.secondary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);

    let mut env = env_core(&body);
    env.children
        .push(gas_bag(&body, [0.0, 0.0, 0.0], [0.92, 0.88, 1.24]));
    // A short rounded tail bulb so the fins at z=-1.0 have a body to grip.
    env.children
        .push(gas_bag(&body, [0.0, 0.0, -1.0], [0.5, 0.5, 0.5]));
    // Two soft bands. The bag's cross-section at z=±0.45 runs ≈0.82 (Y) to
    // ≈0.86 (X); seat the circular band near the larger radius so it straddles
    // the surface instead of sinking to a top-crescent (#781).
    for z in [-0.45f32, 0.45] {
        env.children.push(env_ring(&band, z, 0.85));
    }
    // Rounded nose finial.
    env.children
        .push(prim(sphere(0.14, 3, nose), [0.0, 0.0, 1.2], id_quat()));
    env
}

pub(super) fn envelope_lobed(ctx: &PartCtx) -> Generator {
    // Lobed — a multi-cell caterpillar of three gas bags decreasing toward the
    // tail, jointed by rings; a deliberately segmented, knobbly silhouette.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let ring = ctx.materials.metal(ctx.palette.secondary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);

    // Three distinct round beads (decreasing toward the tail) set far enough
    // apart that the silhouette PINCHES to a narrow waist between them, joined
    // by thin neck cylinders — a true string-of-beads caterpillar rather than a
    // smooth ovoid with wrap-bands. The lobing reads from the profile outline.
    let mut env = env_core(&body);
    env.children
        .push(gas_bag(&body, [0.0, 0.0, 0.92], [0.5, 0.52, 0.46]));
    env.children
        .push(gas_bag(&body, [0.0, 0.0, 0.0], [0.62, 0.64, 0.5]));
    env.children
        .push(gas_bag(&body, [0.0, 0.0, -0.92], [0.48, 0.5, 0.46]));
    // Thin necks bridging the pinched waists (laid along Z).
    for z in [0.46f32, -0.46] {
        env.children.push(prim(
            cylinder(0.32, 0.5, 10, body.clone()),
            [0.0, 0.0, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        // A ring cinching each neck.
        env.children.push(env_ring(&ring, z, 0.33));
    }
    // Tail cone (apex -Z) past the tail bead so the cruciform fins at z=-1.0
    // sit on a pointed tail.
    env.children.push(prim(
        cone(0.4, 0.5, 12, body.clone()),
        [0.0, 0.0, -1.32],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    ));
    // Pointed nose finial.
    env.children
        .push(prim(sphere(0.12, 3, nose), [0.0, 0.0, 1.32], id_quat()));
    env
}

pub(super) fn envelope_twin(ctx: &PartCtx) -> Generator {
    // Twin — a catamaran dirigible: two parallel gas bags joined by a central
    // empennage spine that carries the cruciform tail. Its defining feature is
    // the pair of side-by-side hulls seen head-on.
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let nose = ctx.materials.trim(ctx.palette.tertiary_accent);
    let frame = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.5));

    let mut env = env_core(&body);
    for s in [-1.0f32, 1.0] {
        env.children
            .push(gas_bag(&body, [s * 0.46, 0.04, 0.0], [0.4, 0.46, 1.22]));
        // Per-bag nose + tail cones.
        env.children.push(prim(
            cone(0.26, 0.4, 10, body.clone()),
            [s * 0.46, 0.04, 1.1],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        env.children.push(prim(
            cone(0.28, 0.42, 10, body.clone()),
            [s * 0.46, 0.04, -1.08],
            quat_xyzw(quat_x(-FRAC_PI_2)),
        ));
        env.children.push(prim(
            sphere(0.07, 3, nose.clone()),
            [s * 0.46, 0.04, 1.32],
            id_quat(),
        ));
    }
    // Connecting struts between the two bags fore, amidships, and aft.
    for z in [0.7f32, 0.0, -0.7] {
        env.children.push(prim(
            cuboid([0.96, 0.07, 0.12], frame.clone()),
            [0.0, 0.04, z],
            id_quat(),
        ));
    }
    // Central empennage at the cruciform-fin mount (z = -1.0), so the dorsal /
    // ventral fins have a body to grip at the centreline between the two hulls.
    // The vertical stabiliser is tapered + raked aft (not a flat slab) so the
    // tail reads as a shaped fin rather than the bare rectangle a plain cuboid
    // showed broadside (#781); the horizontal spar stays a thin plate (it never
    // read as a slab) but is trimmed shallower so the swept fins overhang it.
    env.children.push(prim(
        with_shape(
            cuboid([0.12, 1.1, 0.46], body.clone()),
            [0.3, 0.7], // draw the top in — full chord at the root, thin aloft
            [0.0, 0.0, 0.0],
            [0.0, -0.12], // rake the tip aft
        ),
        [0.0, 0.0, -1.0],
        id_quat(),
    ));
    env.children.push(prim(
        cuboid([1.0, 0.12, 0.34], body),
        [0.0, 0.0, -1.0],
        id_quat(),
    ));
    env
}

pub(super) fn gondola(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.secondary_accent);
    let keel = ctx
        .materials
        .body(shade(ctx.palette.secondary_accent, 0.65));
    let frame = ctx
        .materials
        .metal(shade(ctx.palette.secondary_accent, 0.5));
    let window = ctx.materials.glow(ctx.palette.tertiary_accent);
    // Main cabin hull.
    let mut g = prim(
        cuboid([0.44, 0.28, 0.92], body.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Rounded nose + tail end caps.
    for sz in [-1.0f32, 1.0] {
        let mut cap = prim(
            sphere(0.22, 3, body.clone()),
            [0.0, -0.02, sz * 0.46],
            id_quat(),
        );
        cap.transform.scale = Fp3([0.95, 0.62, 0.55]);
        g.children.push(cap);
    }
    // A continuous lit window band along each flank, broken into panes by
    // mullions, instead of a sparse row of portholes.
    for s in [-1.0f32, 1.0] {
        g.children.push(prim(
            cuboid([0.02, 0.09, 0.74], window.clone()),
            [s * 0.225, 0.04, 0.0],
            id_quat(),
        ));
        for z in [-0.24f32, 0.0, 0.24] {
            g.children.push(prim(
                cuboid([0.03, 0.11, 0.03], frame.clone()),
                [s * 0.23, 0.04, z],
                id_quat(),
            ));
        }
    }
    // Rounded keel underneath.
    g.children.push(prim(
        cuboid([0.38, 0.12, 0.84], keel),
        [0.0, -0.18, 0.0],
        id_quat(),
    ));
    // Bridge cockpit bump at the bow (+Z).
    g.children.push(prim(
        cuboid([0.3, 0.14, 0.18], frame),
        [0.0, 0.14, 0.4],
        id_quat(),
    ));
    g
}

// ---------------------------------------------------------------------------
// Skiff
// ---------------------------------------------------------------------------

pub(super) fn fin(ctx: &PartCtx) -> Generator {
    // A thin tapered, aft-swept fin centred on its mount; the assembler rotates
    // each copy into a cruciform tail. Centred at the origin (not pre-raised) so
    // the assembler's rotation spins it about its own centre cleanly. Tapered +
    // swept so it reads as a stabiliser, with a glowing trailing edge.
    let mut f = prim(
        with_torture(
            cuboid(
                [0.05, 0.62, 0.62],
                ctx.materials.body(ctx.palette.tertiary_accent),
            ),
            0.0,
            0.5,
            [0.0, 0.0, -0.28],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Glowing trailing edge along the aft side (-Z).
    f.children.push(prim(
        cuboid(
            [0.06, 0.5, 0.04],
            ctx.materials.glow(ctx.palette.secondary_accent),
        ),
        [0.0, 0.0, -0.28],
        id_quat(),
    ));
    f
}
