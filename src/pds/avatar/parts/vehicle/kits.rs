//! Bespoke mood-group kits (#793) - parts crafted for a narrow mood so a
//! theme's vehicles read distinctly, grouped by family below. Each respects its
//! slot's fixed assembler anchor (boat Bow = forward foredeck; Stack = stern;
//! Ornament = low on the deck just forward of amidships; Deck = the sole; skiff
//! Canopy = cabin top; skiff Ornament = bonnet nose; airship Ornament = forward
//! of the gondola), and any translated / rotated root hangs off a hidden origin
//! hub so it can't tumble its children (the #792-review transform-inheritance
//! gotcha). See the [`super`] module docstring for the mood-group / band scheme.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, superellipsoid, with_shape,
};
use crate::pds::avatar::parts::defaults::airship::airship_colors;
use crate::pds::avatar::parts::defaults::common::darken;
use crate::pds::avatar::parts::defaults::skiff::skiff_colors;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{AGRARIAN, AIRSHIP, CLEAN, COASTAL, FANCY, HISTORIC, MARTIAL, NEON, SKIFF, WORN_PLUS};

// --- Skiff -----------------------------------------------------------------

fn canopy_buckboard(ctx: &PartCtx) -> Generator {
    // A wooden buckboard cart canopy (agrarian / roadside): an open plank bench
    // under a peaked canvas awning - a farm runabout, not a glass greenhouse.
    let colors = skiff_colors(ctx);
    let wood = ctx.materials.body(colors.body);
    let dark = ctx.materials.body(colors.lower);
    let canvas = ctx.materials.cloth(colors.trim);
    // Hidden hub (bench + awning sit at different heights around the cabin top).
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], dark.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Plank bench seat + a low backrest.
    root.children.push(prim(
        cuboid([0.34, 0.05, 0.24], wood.clone()),
        [0.0, -0.08, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid([0.34, 0.16, 0.04], wood.clone()),
        [0.0, 0.0, -0.11],
        id_quat(),
    ));
    // Dark plank gaps across the seat.
    for x in [-0.1f32, 0.1] {
        root.children.push(prim(
            cuboid([0.012, 0.055, 0.24], dark.clone()),
            [x, -0.08, 0.0],
            id_quat(),
        ));
    }
    // Two bow ribs on posts holding the awning up off the seat (so the canvas
    // reads as stretched over a frame, not floating), plus a ridge pole.
    for z in [-0.15f32, 0.15] {
        root.children.push(prim(
            cylinder(0.012, 0.28, 6, dark.clone()),
            [0.0, 0.13, z],
            id_quat(),
        ));
    }
    root.children.push(prim(
        cylinder(0.01, 0.44, 6, dark.clone()),
        [0.0, 0.27, -0.02],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Peaked canvas awning: two panels tilted to a ridge (a covered-wagon top).
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.22, 0.014, 0.42], canvas.clone()),
            [s * 0.1, 0.2, -0.02],
            quat_xyzw(quat_z(s * 0.85)),
        ));
    }
    root
}

fn canopy_aero(ctx: &PartCtx) -> Generator {
    // A clean speedster aero canopy (NEON, Pristine only): a LOW long wedge cowl
    // with an integrated fastback and bright shoulder strakes - the clean-tier
    // read (deliberately flat + sleek, the opposite of the boxy greenhouse).
    let colors = skiff_colors(ctx);
    let shell = ctx.materials.metal(colors.body);
    let glass = ctx.materials.glass(colors.glass);
    // A hue that contrasts the shell so the tech strakes actually pop.
    let glow = ctx.materials.glow(ctx.palette.tertiary_accent);
    // Low, long, flat wedge cowl at the origin (lower + flatter than a bubble).
    let mut root = prim(
        superellipsoid([0.26, 0.08, 0.37], 0.38, 0.42, shell.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // A steeply-raked wraparound windscreen standing proud at the cockpit front.
    root.children.push(prim(
        with_shape(
            cuboid([0.3, 0.13, 0.02], glass),
            [0.25, 0.0],
            [0.0, 0.0, -0.04],
            [0.0, 0.0],
        ),
        [0.0, 0.1, 0.12],
        quat_xyzw(quat_x(-0.7)),
    ));
    // A low fastback fairing sloping down to the tail (a tapered wedge, kept low
    // so it reads as one continuous body, not a stacked headrest lump).
    root.children.push(prim(
        with_shape(
            superellipsoid([0.11, 0.09, 0.26], 0.4, 0.45, shell),
            [0.6, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0],
        ),
        [0.0, 0.04, -0.15],
        id_quat(),
    ));
    // Twin bright accent strakes down the shoulders.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cuboid([0.02, 0.02, 0.4], glow.clone()),
            [s * 0.15, 0.06, 0.0],
            id_quat(),
        ));
    }
    root
}

fn canopy_targa_rack(ctx: &PartCtx) -> Generator {
    // A sport targa canopy with a surfboard rack (COASTAL): a roll hoop + low
    // screen + a roof rack carrying a board or two - a beach cruiser.
    let colors = skiff_colors(ctx);
    let body = ctx.materials.metal(colors.body);
    let bar = ctx.materials.metal(colors.trim);
    let glass = ctx.materials.glass(colors.glass);
    let board = ctx.materials.body(ctx.palette.primary_accent);
    let board2 = ctx.materials.body(ctx.palette.secondary_accent);
    // Low seat tub at the origin.
    let mut root = prim(
        superellipsoid([0.26, 0.09, 0.32], 0.45, 0.5, body),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Roll hoop (targa bar): two posts + a top bar (a leaf).
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cylinder(0.018, 0.24, 8, bar.clone()),
            [s * 0.2, 0.11, -0.06],
            id_quat(),
        ));
    }
    root.children.push(prim(
        cylinder(0.018, 0.42, 8, bar.clone()),
        [0.0, 0.23, -0.06],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Low raked windscreen (a leaf).
    root.children.push(prim(
        cuboid([0.34, 0.1, 0.015], glass),
        [0.0, 0.07, 0.2],
        quat_xyzw(quat_x(-0.4)),
    ));
    // Roof-rack cross bars over the hoop carrying two boards.
    for x in [-0.09f32, 0.09] {
        root.children.push(prim(
            cuboid([0.06, 0.012, 0.5], bar.clone()),
            [x, 0.26, -0.02],
            id_quat(),
        ));
    }
    root.children.push(prim(
        superellipsoid([0.05, 0.02, 0.28], 0.4, 0.6, board),
        [-0.03, 0.29, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        superellipsoid([0.05, 0.02, 0.26], 0.4, 0.6, board2),
        [0.06, 0.29, 0.03],
        id_quat(),
    ));
    root
}

fn orn_bull_bar(ctx: &PartCtx) -> Generator {
    // A MARTIAL front bull-bar / brush guard for the skiff Ornament slot (which
    // anchors on the bonnet nose): a tubular push frame facing forward (+Z).
    let bar = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let tip = ctx.materials.metal(ctx.palette.tertiary_accent);
    // Hidden hub (the cross-bar is laid along X → a rotated root would tumble the
    // uprights + tips).
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], bar.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Horizontal push bar across the nose (laid along X, a leaf).
    root.children.push(prim(
        cylinder(0.022, 0.5, 10, bar.clone()),
        [0.0, 0.02, 0.05],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Uprights (left / centre / right) from the bumper to the bar.
    for x in [-0.18f32, 0.0, 0.18] {
        root.children.push(prim(
            cylinder(0.018, 0.18, 8, bar.clone()),
            [x, -0.07, 0.05],
            id_quat(),
        ));
    }
    // Bright tips on the bar ends.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            sphere(0.028, 3, tip.clone()),
            [s * 0.25, 0.02, 0.05],
            id_quat(),
        ));
    }
    root
}

// --- Airship ---------------------------------------------------------------

fn orn_lanterns(ctx: &PartCtx) -> Generator {
    // A string of festival paper lanterns hung forward of the gondola (airship
    // Ornament, old-world / festival moods): a swagged line of small glowing
    // lanterns dipping at the centre.
    let c = airship_colors(ctx);
    let line_mat = ctx.materials.metal(c.frame);
    // Hidden hub (the swag line is laid along X → a rotated root would tumble the
    // hanging lanterns).
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], line_mat.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // The swag line (a bar across X, a leaf) - wide, since the airship is large
    // and the ornament floats forward of the gondola where a short string is lost.
    root.children.push(prim(
        cylinder(0.012, 0.76, 6, line_mat.clone()),
        [0.0, 0.0, 0.0],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    // Big paper lanterns at intervals, alternating hue, the centre dipping lower.
    let hues = [
        ctx.palette.primary_accent,
        ctx.palette.tertiary_accent,
        ctx.palette.secondary_accent,
    ];
    for (i, &x) in [-0.3f32, -0.15, 0.0, 0.15, 0.3].iter().enumerate() {
        let dip = -0.08 - 0.05 * (1.0 - x.abs() / 0.3);
        let glow = ctx.materials.glow(hues[i % 3]);
        // Hanger wire spanning from the line (y=0) down to the lantern (y=dip) -
        // its length tracks the dip so it never falls short (0.01 = min dim).
        root.children.push(prim(
            cuboid([0.01, -dip, 0.01], line_mat.clone()),
            [x, dip * 0.5, 0.0],
            id_quat(),
        ));
        // Paper lantern body (a slightly squashed glowing sphere) + a cap boss.
        let mut lantern = prim(sphere(0.07, 3, glow), [x, dip, 0.0], id_quat());
        lantern.transform.scale = Fp3([1.0, 0.82, 1.0]);
        root.children.push(lantern);
        root.children.push(prim(
            cuboid([0.02, 0.015, 0.02], line_mat.clone()),
            [x, dip + 0.06, 0.0],
            id_quat(),
        ));
    }
    root
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub(super) static CANOPY_BUCKBOARD: PartDef = PartDef {
    slug: "skiff_canopy_buckboard",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: AGRARIAN,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: canopy_buckboard,
};
pub(super) static CANOPY_AERO: PartDef = PartDef {
    slug: "skiff_canopy_aero",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    // A polished aero cowl - the clean-tier read (pristine craft only).
    wear: CLEAN,
    build: canopy_aero,
};
pub(super) static CANOPY_TARGA_RACK: PartDef = PartDef {
    slug: "skiff_canopy_targa_rack",
    slot: PartSlot::Canopy,
    chassis: SKIFF,
    styles: COASTAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: canopy_targa_rack,
};
pub(super) static ORN_BULL_BAR: PartDef = PartDef {
    slug: "skiff_orn_bull_bar",
    slot: PartSlot::Ornament,
    chassis: SKIFF,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    // A brush guard reads on a rugged, used craft.
    wear: WORN_PLUS,
    build: orn_bull_bar,
};
pub(super) static ORN_LANTERNS: PartDef = PartDef {
    slug: "airship_orn_lanterns",
    slot: PartSlot::Ornament,
    chassis: AIRSHIP,
    styles: HISTORIC,
    // A festival string is a fancy flourish - adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: orn_lanterns,
};
