//! Skiff defaults: chassis, the two canopy forms, and wheels. Built in each slot's local attachment frame — see the module
//! docstring on [`super::super`] (`parts`).

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_mul, quat_x, quat_xyzw, quat_z, superellipsoid, torus,
    with_cut, with_shape,
};
use crate::pds::generator::Generator;

use super::super::PartCtx;
use super::common::{ensure_delta, floor_value, luma, to_value};

/// The seeded skiff landmarks — body tub size + the wheel/fender/anchor
/// contract — from the blueprint (nominal fallback if ever built without one).
/// Returned as `(body_w, body_len, track, wheelbase, ride_y, wheel_r)`; the
/// chassis fenders, the wheel part, and the assembler wheel anchors all read
/// these so the three can never disagree (the magic-number coupling the
/// blueprint dissolves, #783).
fn skiff_dims(ctx: &PartCtx) -> (f32, f32, f32, f32, f32, f32) {
    ctx.skiff()
        .map_or((0.76, 1.5, 0.45, 0.55, -0.12, 0.21), |s| {
            (
                s.body_w,
                s.body_len,
                s.track,
                s.wheelbase,
                s.ride_y,
                s.wheel_r,
            )
        })
}

/// The seeded skiff colour scheme, value-floored + value-separated (#787), so a
/// dark seed's body / greenhouse / trim keep readable boundaries. Lamps stay
/// fixed warm/red — a running light reads wrong in accent paint.
struct SkiffColors {
    /// Bodywork (primary accent, value-floored).
    body: [f32; 3],
    /// Lower rocker / skirt / fenders (a distinctly darker body).
    lower: [f32; 3],
    /// Brightwork trim (secondary accent, value-separated from the body).
    trim: [f32; 3],
    /// Greenhouse glazing — value-separated from the body by a wider delta so
    /// the glass never washes into the paint (seed-3 brown-on-brown, #787).
    glass: [f32; 3],
}

fn skiff_colors(ctx: &PartCtx) -> SkiffColors {
    let p = &ctx.palette;
    let body = floor_value(p.primary_accent, 0.24);
    let bl = luma(body);
    SkiffColors {
        body,
        lower: to_value(body, (bl * 0.5).max(0.05)),
        trim: ensure_delta(p.secondary_accent, bl, 0.14),
        glass: ensure_delta(p.secondary_accent, bl, 0.22),
    }
}

pub(super) fn chassis(ctx: &PartCtx) -> Generator {
    let colors = skiff_colors(ctx);
    let body = ctx.materials.body(colors.body);
    let lower = ctx.materials.metal(colors.lower);
    let trim = ctx.materials.metal(colors.trim);
    let chrome = ctx.materials.trim(colors.trim);
    let bezel = ctx.materials.metal([0.09, 0.09, 0.11]);
    let headlight = ctx.materials.glow([1.0, 0.95, 0.8]);
    let taillight = ctx.materials.glow([0.85, 0.12, 0.1]);
    // Everything scales off the seeded tub: `dw` widths (X), `dl` lengths (Z).
    let (body_w, body_len, track, wheelbase, ride_y, wheel_r) = skiff_dims(ctx);
    let (dw, dl) = (body_w / 0.76, body_len / 1.5);

    // Body — a rounded Superellipsoid slab (a soft auto-body panel, not a
    // sheared box: the biggest step toward the humanoid's blob-era softness,
    // #787). It's the structural root, so it carries no root *scale* (which
    // would displace the mounted canopy / wheels); the roundness is intrinsic
    // to the superellipsoid, not a transform. Low exponents give near-flat
    // faces with softly rounded edges.
    let mut c = prim(
        superellipsoid(
            [body_w * 0.5, 0.115, body_len * 0.5],
            0.38,
            0.5,
            body.clone(),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Dark lower rocker / skirt — a slimmer rounded superellipsoid tucked under
    // so the body doesn't read as one slab down to the sills.
    c.children.push(prim(
        superellipsoid(
            [body_w * 0.44, 0.06, body_len * 0.5],
            0.34,
            0.5,
            lower.clone(),
        ),
        [0.0, -0.12, 0.0],
        id_quat(),
    ));
    // Rounded hood at the front (+Z), lower than the cabin.
    c.children.push(prim(
        superellipsoid([0.34 * dw, 0.06, 0.27 * dl], 0.4, 0.5, body.clone()),
        [0.0, 0.09, 0.48 * dl],
        id_quat(),
    ));
    // Cabin bulge toward the rear (the canopy seats on this).
    c.children.push(prim(
        superellipsoid([0.33 * dw, 0.1, 0.34 * dl], 0.42, 0.5, body.clone()),
        [0.0, 0.13, -0.16 * dl],
        id_quat(),
    ));
    // Mudguard arching over each wheel — a hollow Torus channel laid on the
    // axle (X), placed **concentric with its wheel** (same x/z anchor, hub-line
    // y) so it actually wraps the tyre instead of floating beside it. The cuts:
    // `path_cut [0.0, 0.5]` keeps the top 180° arch (back → over → front);
    // `profile_cut [0.5, 1.0]` keeps only the **outer-radius** half of the tube
    // (the flat-pole cut convention — see `world_builder::prim`), so the open
    // channel faces inward over the tyre and the guard never dips into it;
    // `hollow` thins it to a shell. The major radius hugs just outside the
    // tyre's outer tread (`wheel_r` + a hair) so the guard caps the crown
    // closely — deriving it from the blueprint's wheel radius means a bigger
    // wheel always gets a bigger guard. The minor radius stays substantial so
    // the mudguard reads as solid mass head-on. The roll
    // `quat_x(-FRAC_PI_2)·quat_z(FRAC_PI_2)` lays the ring on the axle with its
    // kept arch centred over the top. (Kept as-is — the fenders read well, #787.)
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            let fender = with_cut(
                torus(0.085, wheel_r + 0.005, lower.clone()),
                [0.0, 0.5],
                [0.5, 1.0],
                0.5,
            );
            c.children.push(prim(
                fender,
                [sx * track, ride_y, sz * wheelbase],
                quat_xyzw(quat_mul(quat_x(-FRAC_PI_2), quat_z(FRAC_PI_2))),
            ));
        }
    }
    // Front bumper / grille bar — a rounded 3D chrome bar across the nose
    // (a cylinder laid along X), not a flat slab.
    c.children.push(prim(
        cylinder(0.028, 0.56 * dw, 12, chrome.clone()),
        [0.0, 0.0, 0.53 * body_len],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Headlights: a dark bezel ring around a bright lens, both shallow cylinders
    // facing forward — 3D relief instead of a flat painted patch.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.055, 0.03, 12, bezel.clone()),
            [sx * 0.26 * dw, 0.1, 0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
        c.children.push(prim(
            cylinder(0.04, 0.05, 12, headlight.clone()),
            [sx * 0.26 * dw, 0.1, 0.51 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Tail lamps: shallow red lenses.
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cylinder(0.038, 0.04, 10, taillight.clone()),
            [sx * 0.24 * dw, 0.1, -0.5 * body_len],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Flank vent — three louvre slats on each hood side (mid-scale detail).
    for sx in [-1.0f32, 1.0] {
        for i in 0..3 {
            c.children.push(prim(
                cuboid([0.02, 0.035, 0.09], bezel.clone()),
                [sx * 0.32 * dw, 0.09, (0.4 - i as f32 * 0.07) * dl],
                id_quat(),
            ));
        }
    }
    // Side trim strake along each flank.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.02, 0.04, 1.02 * dl], trim.clone()),
            [s * 0.5 * body_w, 0.0, 0.0],
            id_quat(),
        ));
    }
    // Running board: a flat step bridging each body sill out to the wheel line
    // between the front and rear fenders. Closes the gap that, head-on, made
    // the outboard wheels read as floating off the sides, and is iconic vintage
    // styling in its own right. Its outboard reach tracks the wheel line.
    for s in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.18, 0.035, 0.62 * dl], lower.clone()),
            [s * (track - 0.04), -0.1, 0.0],
            id_quat(),
        ));
    }
    // Rear-deck spare wheel — a torus + hub standing on the tail, rescuing the
    // blank BACK tile (#787). The spare's own radius echoes the road wheels.
    let spare_r = wheel_r * 0.62;
    c.children.push(prim(
        torus(spare_r * 0.3, spare_r, lower.clone()),
        [0.0, 0.12, -0.53 * body_len],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    c.children.push(prim(
        cylinder(spare_r * 0.5, 0.05, 12, chrome),
        [0.0, 0.12, -0.53 * body_len],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    c
}

pub(super) fn canopy(ctx: &PartCtx) -> Generator {
    let colors = skiff_colors(ctx);
    let glass = ctx.materials.glass(colors.glass);
    let frame = ctx.materials.metal(colors.lower);
    let roof_mat = ctx.materials.body(colors.body);
    // A real greenhouse: inset glass panels held in a proud pillar/rail cage,
    // capped by a flush body-coloured roof — no crate-lid overhang (#787). The
    // glazing is value-separated from the body (skiff_colors::glass) so the
    // windows never wash into the paint.
    //
    // Inset glass box (smaller than the cage, so the frame stands proud of it).
    let mut c = prim(
        cuboid([0.44, 0.19, 0.52], glass),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Flush roof panel (matches the cage footprint — does not overhang).
    c.children.push(prim(
        cuboid([0.48, 0.045, 0.5], roof_mat),
        [0.0, 0.11, -0.02],
        id_quat(),
    ));
    // Corner pillars (A + C posts) standing proud of the glass on all four
    // corners, so the windows read as framed panes.
    for sx in [-1.0f32, 1.0] {
        for sz in [0.25f32, -0.25] {
            c.children.push(prim(
                cuboid([0.035, 0.2, 0.035], frame.clone()),
                [sx * 0.235, 0.0, sz],
                id_quat(),
            ));
        }
    }
    // Waist rail (belt line) wrapping the base of the glazing.
    for sz in [0.26f32, -0.26] {
        c.children.push(prim(
            cuboid([0.5, 0.03, 0.03], frame.clone()),
            [0.0, -0.085, sz],
            id_quat(),
        ));
    }
    for sx in [-1.0f32, 1.0] {
        c.children.push(prim(
            cuboid([0.03, 0.03, 0.55], frame.clone()),
            [sx * 0.235, -0.085, 0.0],
            id_quat(),
        ));
    }
    c
}

pub(super) fn canopy_roadster(ctx: &PartCtx) -> Generator {
    let colors = skiff_colors(ctx);
    let glass = ctx.materials.glass(colors.glass);
    let frame = ctx.materials.metal(colors.lower);
    let body = ctx.materials.body(colors.body);
    let seat = ctx.materials.cloth(colors.trim);
    let column = ctx.materials.metal([0.12, 0.12, 0.14]);
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
    // Seat back — a rounded bucket back rising in the open cockpit, so the
    // interior reads occupiable (#787). A shallow superellipsoid, cushion-toned.
    c.children.push(prim(
        superellipsoid([0.15, 0.1, 0.04], 0.5, 0.6, seat),
        [0.0, 0.04, -0.08],
        id_quat(),
    ));
    // Steering column + wheel raked up from the cowl ahead of the seat.
    c.children.push(prim(
        cylinder(0.012, 0.16, 8, column.clone()),
        [0.0, 0.02, 0.12],
        quat_xyzw(quat_x(0.5)),
    ));
    c.children.push(prim(
        torus(0.014, 0.05, column),
        [0.0, 0.1, 0.17],
        quat_xyzw(quat_x(0.9)),
    ));
    // Low faired headrest hump behind the cockpit (a rear tonneau cowl), domed
    // via a roof taper. Kept well below the windscreen top so the cockpit reads
    // clearly OPEN (not an enclosed cabin) between the two.
    c.children.push(prim(
        with_shape(
            cuboid([0.34, 0.11, 0.3], body),
            [0.4, 0.55],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, -0.015, -0.29],
        id_quat(),
    ));
    c
}

pub(super) fn canopy_coupe(ctx: &PartCtx) -> Generator {
    let colors = skiff_colors(ctx);
    let glass = ctx.materials.glass(colors.glass);
    let frame = ctx.materials.metal(colors.lower);
    // Closed fastback hardtop — the glazed cabin tapers in and shears rearward
    // into a sloping roofline, distinct from the upright greenhouse box. Glazing
    // is value-separated from the body (#787).
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
    // Tyre outer radius = blueprint `wheel_r`; split into a tread minor radius
    // (~0.29 of it) and the hub major radius so the same wheel scales with the
    // seed and always matches its fender (both read `wheel_r`).
    let (_, _, _, _, _, wheel_r) = skiff_dims(ctx);
    let minor = wheel_r * 0.286;
    let major = wheel_r - minor;
    // Tyre: a torus gives a rounded tread cross-section — a real tyre, not a
    // flat-sided disc (outer radius ≈ major + minor).
    let mut w = prim(torus(minor, major, tyre), [0.0, 0.0, 0.0], id_quat());
    // Rim plate filling the hub (shares the torus axis; the assembler lays the
    // whole wheel onto its axle).
    let mut rim_disc = prim(
        cylinder(major * 0.73, 0.12, 16, rim.clone()),
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
