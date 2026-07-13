//! Bespoke mood-group kits (#793) — parts crafted for a narrow mood so a
//! theme's vehicles read distinctly, grouped by family below. Each respects its
//! slot's fixed assembler anchor (boat Bow = forward foredeck; Stack = stern;
//! Ornament = low on the deck just forward of amidships; Deck = the sole; skiff
//! Canopy = cabin top; skiff Ornament = bonnet nose; airship Ornament = forward
//! of the gondola), and any translated / rotated root hangs off a hidden origin
//! hub so it can't tumble its children (the #792-review transform-inheritance
//! gotcha). See the [`super`] module docstring for the mood-group / band scheme.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cone, cuboid, cylinder, helix, id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, spine,
    superellipsoid, torus, with_shape,
};
use crate::pds::avatar::parts::defaults::airship::airship_colors;
use crate::pds::avatar::parts::defaults::common::{darken, shade};
use crate::pds::avatar::parts::defaults::skiff::skiff_colors;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{
    AGRARIAN, AIRSHIP, BOAT, CLEAN, COASTAL, FANCY, HISTORIC, MARTIAL, NEON, NORSE_FEY, REGAL,
    SEPULCHRAL, SKIFF, STEAM, WORKING, WORN_PLUS, deck_dims,
};

// --- Boat ------------------------------------------------------------------

fn bow_serpent(ctx: &PartCtx) -> Generator {
    // A longship / fae dragon-prow (Nordic / Fantasy): a Spine-swept serpent neck
    // rising off the stem, curving forward to a horned head with glowing eyes.
    // Authored front-+Z; the neck sweeps up and forward.
    let scale = ctx.materials.metal(ctx.palette.secondary_accent);
    let horn = ctx.materials.trim(ctx.palette.tertiary_accent);
    let eye = ctx.materials.glow(ctx.palette.primary_accent);
    // Catmull-Rom neck: stem base → a head swell → a tapering snout.
    let neck = spine(
        &[
            ([0.0, 0.0, 0.0], 0.055),
            ([0.0, 0.13, 0.05], 0.05),
            ([0.0, 0.24, 0.13], 0.046),
            ([0.0, 0.31, 0.26], 0.055),
            ([0.0, 0.29, 0.4], 0.022),
        ],
        12,
        scale.clone(),
    );
    let mut root = prim(neck, [0.0, 0.0, 0.0], id_quat());
    // Lower jaw under the snout so the head reads as a mouth, not just a taper.
    root.children.push(prim(
        cone(0.03, 0.12, 6, scale),
        [0.0, 0.26, 0.34],
        quat_xyzw(quat_x(1.9)),
    ));
    // Two swept-back horns off the crown.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            cone(0.02, 0.13, 6, horn.clone()),
            [s * 0.045, 0.36, 0.24],
            quat_xyzw(quat_x(-0.7)),
        ));
    }
    // Glowing eyes.
    for s in [-1.0f32, 1.0] {
        root.children.push(prim(
            sphere(0.02, 3, eye.clone()),
            [s * 0.045, 0.32, 0.31],
            id_quat(),
        ));
    }
    root
}

fn bow_rope_coil(ctx: &PartCtx) -> Generator {
    // A working-boat foredeck fitting (Nordic / Medieval / frontier / industrial):
    // a Helix-coiled mooring line flaked around a bollard, with a horn cleat.
    // A tan hemp rope, well clear of the dark iron, so the coil doesn't merge
    // into a single dark donut.
    let rope = ctx.materials.cloth([0.72, 0.6, 0.42]);
    let iron = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    // Hidden hub (the bollard sits above the mount → a translated root would
    // lift the flaked coil off the deck).
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], iron.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Short bollard the line is coiled around, capped with a head.
    root.children.push(prim(
        cylinder(0.045, 0.16, 10, iron.clone()),
        [0.0, 0.08, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.05, 3, iron.clone()),
        [0.0, 0.16, 0.0],
        id_quat(),
    ));
    // Helix rope flaked round the bollard — a higher pitch + thinner wire so the
    // individual turns read as separate wraps, not one solid donut.
    root.children.push(prim(
        helix(0.11, 0.021, 0.075, 3.0, 16, rope),
        [0.0, 0.015, 0.0],
        id_quat(),
    ));
    // A horn cleat set well clear of the bollard so its T-shape is silhouetted.
    root.children.push(prim(
        cuboid([0.12, 0.02, 0.025], iron.clone()),
        [0.2, 0.04, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid([0.024, 0.04, 0.024], iron),
        [0.2, 0.01, 0.0],
        id_quat(),
    ));
    root
}

/// A small glazed lantern cage centred at `center` (rising +Y): floor plate,
/// four corner posts around a translucent box with a deep glowing core, a peaked
/// cap and a finial. Shared by the stern / deck lanterns (#793). Kept small
/// so the glow reads lit without blooming.
fn push_lantern_cage(parent: &mut Generator, ctx: &PartCtx, center: [f32; 3]) {
    let frame = ctx.materials.metal(ctx.palette.secondary_accent);
    let glass = ctx.materials.glass(ctx.palette.tertiary_accent);
    let flame = ctx.materials.glow(ctx.palette.primary_accent);
    let gold = ctx.materials.trim(ctx.palette.tertiary_accent);
    let [x, y, z] = center;
    // Floor plate.
    parent.children.push(prim(
        cuboid([0.08, 0.02, 0.08], frame.clone()),
        [x, y - 0.06, z],
        id_quat(),
    ));
    // Translucent body + a small deep glowing core.
    parent
        .children
        .push(prim(cuboid([0.06, 0.1, 0.06], glass), [x, y, z], id_quat()));
    parent.children.push(prim(
        cuboid([0.028, 0.06, 0.028], flame),
        [x, y, z],
        id_quat(),
    ));
    // Four corner posts (the cage).
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.0f32, 1.0] {
            parent.children.push(prim(
                cuboid([0.012, 0.11, 0.012], frame.clone()),
                [x + sx * 0.032, y, z + sz * 0.032],
                id_quat(),
            ));
        }
    }
    // Peaked cap + finial.
    parent.children.push(prim(
        cone(0.058, 0.06, 6, gold.clone()),
        [x, y + 0.1, z],
        id_quat(),
    ));
    parent
        .children
        .push(prim(sphere(0.02, 3, gold), [x, y + 0.15, z], id_quat()));
}

fn stack_stern_lantern(ctx: &PartCtx) -> Generator {
    // A funereal / temple stern lantern (GothicHorror / FeudalJapan / Medieval):
    // an ornate glazed lantern hung from a gooseneck bracket at the taffrail.
    let iron = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    // Hidden hub (the post + hung lantern sit at various heights above the mount).
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], iron.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Vertical post + a gooseneck arm reaching aft (-Z).
    root.children.push(prim(
        cylinder(0.02, 0.34, 8, iron.clone()),
        [0.0, 0.17, 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        cylinder(0.015, 0.18, 6, iron),
        [0.0, 0.33, -0.07],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // The lantern hangs from the arm tip.
    push_lantern_cage(&mut root, ctx, [0.0, 0.24, -0.15]);
    root
}

fn deck_veranda(ctx: &PartCtx) -> Generator {
    // A seaside resort deck (COASTAL): a low sole with a pair of reclined sun
    // loungers, a bright parasol, and a transom swim ladder — a leisure cruiser.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.72));
    let canvas = ctx.materials.cloth(ctx.palette.secondary_accent);
    let cushion = ctx.materials.cloth(ctx.palette.tertiary_accent);
    let pole = ctx.materials.metal(ctx.palette.tertiary_accent);
    let (dw, dl) = deck_dims(ctx);
    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], sole),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Two sun loungers: a cushion slab + a TALL reclined backrest at the head
    // (leaf, tilted aft) so the deck-chair read is unmistakable.
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cuboid([0.12 * dw, 0.04, 0.32 * dl], cushion.clone()),
            [s * 0.13 * dw, 0.05, -0.05 * dl],
            id_quat(),
        ));
        deck.children.push(prim(
            cuboid([0.12 * dw, 0.18, 0.025], cushion.clone()),
            [s * 0.13 * dw, 0.12, -0.19 * dl],
            quat_xyzw(quat_x(-0.5)),
        ));
    }
    // Bright parasol amidships-forward: a pole + a wide shallow conical canopy.
    deck.children.push(prim(
        cylinder(0.014, 0.34, 6, pole.clone()),
        [0.0, 0.17, 0.22 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cone(0.2, 0.1, 12, canvas),
        [0.0, 0.36, 0.22 * dl],
        id_quat(),
    ));
    // Transom swim ladder hooked over the stern: two rails rising proud of the
    // deck + two rungs stepped in Y, all in the same Z plane so they connect.
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cuboid([0.016, 0.22, 0.016], pole.clone()),
            [s * 0.05, -0.02, -0.34 * dl],
            id_quat(),
        ));
    }
    for y in [0.03f32, -0.08] {
        deck.children.push(prim(
            cuboid([0.12, 0.016, 0.016], pole.clone()),
            [0.0, y, -0.34 * dl],
            id_quat(),
        ));
    }
    deck
}

fn deck_barrels(ctx: &PartCtx) -> Generator {
    // A frontier / privateer working deck (MARTIAL): lashed barrels, a crate, and
    // a swivel gun at the forward rail — a raiding craft, read on used hulls.
    let sole = ctx.materials.body(shade(ctx.palette.primary_accent, 0.6));
    let wood = ctx.materials.body(shade(ctx.palette.secondary_accent, 0.7));
    let iron = ctx.materials.metal(darken(ctx.palette.tertiary_accent));
    let (dw, dl) = deck_dims(ctx);
    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], sole),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Barrels on their sides (cylinders laid along X) with an iron hoop each.
    for (x, z) in [(-0.08 * dw, -0.12 * dl), (0.1 * dw, -0.22 * dl)] {
        deck.children.push(prim(
            cylinder(0.09, 0.2, 10, wood.clone()),
            [x, 0.09, z],
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
        deck.children.push(prim(
            torus(0.012, 0.092, iron.clone()),
            [x, 0.09, z],
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
    }
    // An upright crate.
    deck.children.push(prim(
        cuboid([0.13, 0.13, 0.13], wood),
        [-0.06 * dw, 0.09, 0.16 * dl],
        id_quat(),
    ));
    // A swivel gun on a post at the forward rail: a short barrel angled up-+Z.
    deck.children.push(prim(
        cylinder(0.02, 0.12, 8, iron.clone()),
        [0.0, 0.1, 0.3 * dl],
        id_quat(),
    ));
    deck.children.push(prim(
        cylinder(0.022, 0.16, 8, iron),
        [0.0, 0.19, 0.35 * dl],
        quat_xyzw(quat_x(0.9)),
    ));
    deck
}

fn deck_engineworks(ctx: &PartCtx) -> Generator {
    // A steam engine deck (STEAM): a riveted boiler casing, standpipes, a
    // pressure gauge and an exposed flywheel — the machinery on show.
    let base = ctx.materials.body(shade(ctx.palette.primary_accent, 0.5));
    let casing = ctx.materials.metal(ctx.palette.secondary_accent);
    let dark = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let brass = ctx.materials.trim(ctx.palette.tertiary_accent);
    let gauge = ctx.materials.glow(ctx.palette.primary_accent);
    let (dw, dl) = deck_dims(ctx);
    let mut deck = prim(
        cuboid([0.34 * dw, 0.045, 0.62 * dl], base),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Boiler casing (a rounded fore-aft drum) amidships with brass hoops.
    deck.children.push(prim(
        superellipsoid([0.15 * dw, 0.13, 0.24 * dl], 0.5, 0.6, casing),
        [0.0, 0.11, -0.05 * dl],
        id_quat(),
    ));
    // Two hoops encircling the Z-axis drum: quat_x(90°) turns the default (hole
    // +Y) torus so its hole lies along Z, wrapping the barrel rather than lying
    // flat through it. Minor radius sits just proud of the casing's XY section.
    for z in [-0.16 * dl, 0.06 * dl] {
        deck.children.push(prim(
            torus(0.014, 0.15, brass.clone()),
            [0.0, 0.11, z],
            quat_xyzw(quat_x(FRAC_PI_2)),
        ));
    }
    // Two standpipes rising aft.
    for s in [-1.0f32, 1.0] {
        deck.children.push(prim(
            cylinder(0.02, 0.24, 8, dark.clone()),
            [s * 0.12 * dw, 0.24, -0.06 * dl],
            id_quat(),
        ));
    }
    // Pressure gauge on a stalk (a bright dial facing +Z).
    deck.children.push(prim(
        cylinder(0.042, 0.02, 12, brass.clone()),
        [0.11 * dw, 0.17, 0.16 * dl],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    deck.children.push(prim(
        cylinder(0.03, 0.024, 12, gauge),
        [0.11 * dw, 0.17, 0.17 * dl],
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
    // Exposed flywheel on the port side: a rim (facing ±X) + hub + a spoke cross.
    let wheel_x = -0.2 * dw;
    deck.children.push(prim(
        torus(0.018, 0.12, dark.clone()),
        [wheel_x, 0.13, 0.14 * dl],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    deck.children.push(prim(
        cylinder(0.03, 0.06, 10, brass),
        [wheel_x, 0.13, 0.14 * dl],
        quat_xyzw(quat_z(FRAC_PI_2)),
    ));
    for a in [0.0f32, FRAC_PI_2] {
        deck.children.push(prim(
            cuboid([0.012, 0.22, 0.012], dark.clone()),
            [wheel_x, 0.13, 0.14 * dl],
            quat_xyzw(quat_x(a)),
        ));
    }
    deck
}

fn orn_deck_lantern(ctx: &PartCtx) -> Generator {
    // A REGAL deck lantern for the boat Ornament slot. That slot anchors LOW, on
    // the deck just forward of amidships (≈0.05 above the sole — the same anchor
    // the pennant / finial use, NOT the masthead), so this is an ornate binnacle-
    // style lamp on a short turned pedestal, not a mast-top light.
    let frame = ctx.materials.metal(ctx.palette.secondary_accent);
    let gold = ctx.materials.trim(ctx.palette.tertiary_accent);
    // Hidden hub at the deck mount; a short pedestal carries the lantern so its
    // floor seats on the pedestal top with no stray ring dangling below.
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], frame.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Turned pedestal: a decorative base ring at the deck + a stem to the lantern.
    root.children
        .push(prim(torus(0.012, 0.045, gold), [0.0, 0.01, 0.0], id_quat()));
    root.children.push(prim(
        cuboid([0.05, 0.06, 0.05], frame),
        [0.0, 0.04, 0.0],
        id_quat(),
    ));
    // Lantern seated on the pedestal top (floor at 0.07 = pedestal top).
    push_lantern_cage(&mut root, ctx, [0.0, 0.13, 0.0]);
    root
}

// --- Skiff -----------------------------------------------------------------

fn canopy_buckboard(ctx: &PartCtx) -> Generator {
    // A wooden buckboard cart canopy (agrarian / roadside): an open plank bench
    // under a peaked canvas awning — a farm runabout, not a glass greenhouse.
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
    // with an integrated fastback and bright shoulder strakes — the clean-tier
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
    // screen + a roof rack carrying a board or two — a beach cruiser.
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
    // The swag line (a bar across X, a leaf) — wide, since the airship is large
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
        // Hanger wire spanning from the line (y=0) down to the lantern (y=dip) —
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

pub(super) static BOW_SERPENT: PartDef = PartDef {
    slug: "boat_bow_serpent",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: NORSE_FEY,
    // A dragon-prow is a fancy carving — an adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: bow_serpent,
};
pub(super) static BOW_ROPE_COIL: PartDef = PartDef {
    slug: "boat_bow_rope_coil",
    slot: PartSlot::Bow,
    chassis: BOAT,
    styles: WORKING,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: bow_rope_coil,
};
pub(super) static STACK_STERN_LANTERN: PartDef = PartDef {
    slug: "boat_stack_stern_lantern",
    slot: PartSlot::Stack,
    chassis: BOAT,
    styles: SEPULCHRAL,
    // An ornate hung lantern — adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: stack_stern_lantern,
};
pub(super) static DECK_VERANDA: PartDef = PartDef {
    slug: "boat_deck_veranda",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: COASTAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_veranda,
};
pub(super) static DECK_BARRELS: PartDef = PartDef {
    slug: "boat_deck_barrels",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: MARTIAL,
    ornateness: OrnatenessBand::ANY,
    // A lashed working / raiding deck reads on a used or beaten hull.
    wear: WORN_PLUS,
    build: deck_barrels,
};
pub(super) static DECK_ENGINEWORKS: PartDef = PartDef {
    slug: "boat_deck_engineworks",
    slot: PartSlot::Deck,
    chassis: BOAT,
    styles: STEAM,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: deck_engineworks,
};
pub(super) static ORN_DECK_LANTERN: PartDef = PartDef {
    slug: "boat_orn_deck_lantern",
    slot: PartSlot::Ornament,
    chassis: BOAT,
    styles: REGAL,
    // An ornate deck lantern — adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: orn_deck_lantern,
};
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
    // A polished aero cowl — the clean-tier read (pristine craft only).
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
    // A festival string is a fancy flourish — adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: orn_lanterns,
};
