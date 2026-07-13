//! Cross-family ornaments — a flown pennant, a neon strip, a finial, and a
//! tattered banner. These serve every vehicle family (the `VEHICLES` chassis
//! list); see the [`super`] module docstring for the mood-group / band scheme.

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_x, quat_xyzw, sphere, torus, with_shape,
};
use crate::pds::avatar::parts::defaults::common::{darken, shade};
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessBand, WearBand};

use super::super::{PartCtx, PartDef, PartSlot};
use super::{BATTERED, FANCY, NEON, REGAL, UNIVERSAL, VEHICLES};

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

fn ornament_finial(ctx: &PartCtx) -> Generator {
    // The style-universal ornament floor (empty styles, every family): a little
    // turned finial — a pedestal topped by a banded orb. Being fully 3D it reads
    // from every angle (unlike a flat badge) so it works as a boat masthead knob,
    // a skiff hood mascot, or an airship nose crest, on any theme — the humble
    // accent that keeps every population's Ornament slot fillable (#792).
    let post = ctx.materials.metal(ctx.palette.secondary_accent);
    let orb = ctx.materials.trim(ctx.palette.tertiary_accent);
    let collar = ctx.materials.accent(ctx.palette.primary_accent);
    // Hidden hub at the mount so the post and orb share one un-translated frame —
    // a translated post-as-root would carry its +0.07 into the orb / collar (the
    // transform-inheritance gotcha), floating the orb off the pedestal top.
    let mut root = prim(
        cuboid([0.03, 0.03, 0.03], post.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Turned pedestal rising from the mount (top at y=0.14).
    root.children.push(prim(
        cylinder(0.025, 0.14, 10, post),
        [0.0, 0.07, 0.0],
        id_quat(),
    ));
    // Banded orb seated on the pedestal top (its lower half overlaps the post).
    root.children
        .push(prim(sphere(0.06, 3, orb), [0.0, 0.17, 0.0], id_quat()));
    // Collar ring at the orb waist (orb's Y axis → default torus).
    root.children.push(prim(
        torus(0.012, 0.062, collar),
        [0.0, 0.17, 0.0],
        id_quat(),
    ));
    root
}

fn ornament_tattered(ctx: &PartCtx) -> Generator {
    // The battered-only ornament counterpart (empty styles, wear = Battered): a
    // bent staff flying a ragged swallowtail banner, so a beaten-up craft flies a
    // tattered colour where a pristine one wouldn't — the top wear tier reads on
    // the ornament roll (#792). Cheap: the pennant staff, canted, with a torn
    // (deeply forked) darker cloth.
    let staff = ctx.materials.metal(darken(ctx.palette.secondary_accent));
    let cloth = ctx.materials.cloth(shade(ctx.palette.primary_accent, 0.55));
    // Hidden hub so the canted staff doesn't tumble the banner's placement.
    let mut root = prim(
        cuboid([0.02, 0.02, 0.02], staff.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    // Bent staff (canted aft a touch, as if weathered).
    root.children.push(prim(
        cylinder(0.01, 0.3, 6, staff),
        [0.0, 0.15, 0.0],
        quat_xyzw(quat_x(-0.14)),
    ));
    // Two ragged banner tongues of unequal length, both hung FLUSH at the staff
    // (hoist edge at x=0, z≈0) so the frayed fly ends read as a torn / forked
    // pennant, not scraps floating in front of the pole. The fork reads through
    // the differing length + height, not a forward-Z gap (the #792-review bug);
    // taper frays the fly to a torn point and only a gentle bend flutters the tip.
    for (w, h, z, y) in [
        (0.14f32, 0.075f32, 0.01f32, 0.19f32),
        (0.1, 0.05, -0.01, 0.11),
    ] {
        root.children.push(prim(
            with_shape(
                cuboid([w, h, 0.012], cloth.clone()),
                [0.5, 0.0],
                [0.03, 0.0, 0.02],
                [0.0, 0.0],
            ),
            [w * 0.5, y, z],
            id_quat(),
        ));
    }
    root
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub(super) static PENNANT: PartDef = PartDef {
    slug: "veh_orn_pennant",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: REGAL,
    // A flown pennant is a fancy flourish — an adorned / ornate craft only.
    ornateness: FANCY,
    wear: WearBand::ANY,
    build: pennant,
};
pub(super) static NEON_STRIP: PartDef = PartDef {
    slug: "veh_orn_neon_strip",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: NEON,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: neon_strip,
};
pub(super) static ORNAMENT_FINIAL: PartDef = PartDef {
    slug: "veh_orn_finial",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    // Style-universal ornament floor for every vehicle family: no population's
    // Ornament slot is ever bare.
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    wear: WearBand::ANY,
    build: ornament_finial,
};
pub(super) static ORNAMENT_TATTERED: PartDef = PartDef {
    slug: "veh_orn_tattered",
    slot: PartSlot::Ornament,
    chassis: VEHICLES,
    styles: UNIVERSAL,
    ornateness: OrnatenessBand::ANY,
    // The beaten-up counterpart to the finial / pennant — battered craft only.
    wear: BATTERED,
    build: ornament_tattered,
};
