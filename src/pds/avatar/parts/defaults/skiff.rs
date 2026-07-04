//! Skiff defaults: chassis, the two canopy forms, and wheels. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_z, torus, with_cut,
    with_shape,
};
use crate::pds::generator::Generator;

use super::super::PartCtx;
use super::common::shade;

pub(super) fn chassis(ctx: &PartCtx) -> Generator {
    let body = ctx.materials.body(ctx.palette.primary_accent);
    let lower = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    let trim = ctx.materials.metal(ctx.palette.secondary_accent);
    let headlight = ctx.materials.glow([1.0, 0.95, 0.8]);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);

    // Body tub (structural root — no root *scale*; the shape comes from
    // baked-in torture, which deforms only this mesh, not the mounted
    // children). A gentle tumblehome (sides draw in toward the top) plus a
    // slight forward top-shear de-blocks the slab into a leaning body.
    let mut c = prim(
        with_shape(
            cuboid([0.76, 0.2, 1.5], body.clone()),
            [0.16, 0.06],
            [0.0, 0.0, 0.0],
            [0.0, 0.04],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Dark lower rocker / skirt — tucks under (top flares out past the narrower
    // base) so the body doesn't read as one flat-sided box down to the sills.
    c.children.push(prim(
        with_shape(
            cuboid([0.8, 0.12, 1.42], lower.clone()),
            [-0.1, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, -0.13, 0.0],
        id_quat(),
    ));
    // Hood at the front (+Z), lower than the cabin. Tapers in toward the top
    // (more fore-aft than across) and shears forward for a sloped bonnet.
    c.children.push(prim(
        with_shape(
            cuboid([0.68, 0.1, 0.5], body.clone()),
            [0.16, 0.24],
            [0.0, 0.0, 0.0],
            [0.0, 0.05],
        ),
        [0.0, 0.11, 0.5],
        id_quat(),
    ));
    // Cabin block toward the rear (the canopy seats on this). A clear roof
    // taper narrows it toward the greenhouse base — the most car-like de-block.
    c.children.push(prim(
        with_shape(
            cuboid([0.64, 0.18, 0.66], body.clone()),
            [0.26, 0.2],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, 0.15, -0.18],
        id_quat(),
    ));
    // Mudguard arching over each wheel — a hollow Torus channel laid on the
    // axle (X), placed **concentric with its wheel** (same x/z anchor, hub-line
    // y) so it actually wraps the tyre instead of floating beside it. The cuts:
    // `path_cut [0.0, 0.5]` keeps the top 180° arch (back → over → front);
    // `profile_cut [0.5, 1.0]` keeps only the **outer-radius** half of the tube
    // (the flat-pole cut convention — see `world_builder::prim`), so the open
    // channel faces inward over the tyre and the guard never dips into it;
    // `hollow` thins it to a shell. Major radius (0.215) hugs just outside the
    // tyre's outer tread (≈0.21) so the guard caps the crown closely; the minor
    // radius is kept substantial (0.085) so the mudguard reads as solid mass
    // from head-on, not a thin sliver. The roll
    // `quat_x(-FRAC_PI_2)·quat_z(FRAC_PI_2)` lays the ring on the axle with its
    // kept arch centred over the top.
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            let fender = with_cut(
                torus(0.085, 0.215, lower.clone()),
                [0.0, 0.5],
                [0.5, 1.0],
                0.5,
            );
            c.children.push(prim(
                fender,
                [sx * 0.45, -0.12, sz * 0.55],
                quat_xyzw(quat_mul(quat_x(-FRAC_PI_2), quat_z(FRAC_PI_2))),
            ));
        }
    }
    // Front grille bar + headlights.
    c.children.push(prim(
        cuboid([0.5, 0.07, 0.04], trim.clone()),
        [0.0, 0.04, 0.76],
        id_quat(),
    ));
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.12, 0.07, 0.04], headlight.clone()),
            [sx * 0.26, 0.08, 0.75],
            id_quat(),
        ));
    }
    // Rear taillights.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.1, 0.06, 0.04], taillight.clone()),
            [sx * 0.26, 0.08, -0.74],
            id_quat(),
        ));
    }
    // Side trim strake along each flank.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.02, 0.04, 1.1], trim.clone()),
            [s * 0.385, 0.0, 0.0],
            id_quat(),
        ));
    }
    // Running board: a flat step bridging each body sill out to the wheel line
    // between the front and rear fenders. Closes the gap that, head-on, made
    // the outboard wheels read as floating off the sides, and is iconic vintage
    // styling in its own right.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.18, 0.035, 0.62], lower.clone()),
            [s * 0.41, -0.1, 0.0],
            id_quat(),
        ));
    }
    c
}

pub(super) fn canopy(ctx: &PartCtx) -> Generator {
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    let frame = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    // A glazed cabin greenhouse — a glass box with a roof panel and A-pillar
    // framing — rather than a gumball bubble.
    let mut c = prim(cuboid([0.5, 0.2, 0.6], glass), [0.0, 0.0, 0.0], id_quat());
    // Roof panel.
    c.children.push(prim(
        cuboid([0.52, 0.04, 0.5], frame.clone()),
        [0.0, 0.1, -0.02],
        id_quat(),
    ));
    // Front A-pillars framing the windscreen.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.2, 0.03], frame.clone()),
            [s * 0.24, 0.0, 0.28],
            id_quat(),
        ));
    }
    c
}

pub(super) fn canopy_roadster(ctx: &PartCtx) -> Generator {
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    let frame = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    let body = ctx.materials.body(ctx.palette.primary_accent);
    // Open-top speedster: a low raked windscreen at the cockpit's front lip and
    // a faired headrest behind — no roof, so the cabin reads open. The root is a
    // flat cowl deck (identity rotation) so the raked windscreen *child* tilts
    // alone and can't spin the whole part (the rotated-root trap).
    let mut c = prim(
        cuboid([0.5, 0.05, 0.62], body.clone()),
        [0.0, -0.07, 0.0],
        id_quat(),
    );
    // Raked windscreen glass standing off the cowl's front lip.
    let rake = quat_xyzw(quat_x(0.24));
    c.children.push(prim(
        cuboid([0.42, 0.16, 0.02], glass),
        [0.0, 0.06, 0.27],
        rake,
    ));
    // Windscreen frame: two side posts (raked to match the screen).
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.17, 0.03], frame.clone()),
            [s * 0.2, 0.06, 0.27],
            rake,
        ));
    }
    // Low faired headrest hump behind the cockpit (a rear tonneau cowl), domed
    // via a roof taper. Kept well below the windscreen top so the cockpit reads
    // clearly OPEN (not an enclosed cabin) between the two.
    c.children.push(prim(
        with_shape(
            cuboid([0.34, 0.11, 0.32], body),
            [0.4, 0.55],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, -0.015, -0.28],
        id_quat(),
    ));
    c
}

pub(super) fn canopy_coupe(ctx: &PartCtx) -> Generator {
    let glass = ctx.materials.glass(ctx.palette.secondary_accent);
    let frame = ctx.materials.metal(shade(ctx.palette.primary_accent, 0.45));
    // Closed fastback hardtop — the glazed cabin tapers in and shears rearward
    // into a sloping roofline, distinct from the upright greenhouse box.
    let mut c = prim(
        with_shape(
            cuboid([0.5, 0.22, 0.62], glass),
            [0.16, 0.4],
            [0.0, 0.0, 0.0],
            [0.0, -0.2],
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Opaque roof cap riding the sloped top, tilted to drop toward the tail so
    // the fastback slope reads (not a flat-roof greenhouse).
    c.children.push(prim(
        cuboid([0.42, 0.04, 0.46], frame.clone()),
        [0.0, 0.095, -0.1],
        quat_xyzw(quat_x(-0.32)),
    ));
    // Front A-pillars framing the windscreen.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.2, 0.03], frame.clone()),
            [s * 0.23, 0.0, 0.26],
            id_quat(),
        ));
    }
    c
}

pub(super) fn wheel(ctx: &PartCtx) -> Generator {
    // Dark rubber regardless of palette — a wheel reads wrong in accent paint.
    let tyre = ctx.materials.metal([0.07, 0.07, 0.08]);
    let rim = ctx.materials.metal(ctx.palette.secondary_accent);
    let hub = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Tyre: a torus gives a rounded tread cross-section — a real tyre, not a
    // flat-sided disc (outer radius ≈ major + minor).
    let mut w = prim(torus(0.06, 0.15, tyre), [0.0, 0.0, 0.0], id_quat());
    // Rim plate filling the hub (shares the torus axis; the assembler lays the
    // whole wheel onto its axle).
    let mut rim_disc = prim(
        cylinder(0.11, 0.12, 16, rim.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    for s in [-1.0f32, 1.0] {
        // Cross spokes + hub cap on each rim face.
        rim_disc.children.push(prim(
            cuboid([0.2, 0.02, 0.04], rim.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
        rim_disc.children.push(prim(
            cuboid([0.04, 0.02, 0.2], rim.clone()),
            [0.0, s * 0.06, 0.0],
            id_quat(),
        ));
        rim_disc.children.push(prim(
            cylinder(0.045, 0.04, 8, hub.clone()),
            [0.0, s * 0.07, 0.0],
            id_quat(),
        ));
    }
    w.children.push(rim_disc);
    w
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------
