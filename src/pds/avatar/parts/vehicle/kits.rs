//! Bespoke mood-group kits (#793) - parts crafted for a narrow mood so a
//! theme's vehicles read distinctly. Only the airship's are left: the boat's
//! went in #1363 and the skiff's in #1364, with the catalogues they dressed.
//! Each respects its slot's fixed assembler anchor (airship Ornament = forward
//! of the gondola), and any translated / rotated root hangs off a hidden
//! origin hub so it can't tumble its children (the #792-review
//! transform-inheritance gotcha). See the [`super`] module docstring for the
//! mood-group / band scheme.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    cuboid, cylinder, id_quat, prim, quat_xyzw, quat_z, sphere,
};
use crate::pds::avatar::parts::defaults::airship::airship_colors;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::WearBand;

use super::super::{PartCtx, PartDef, PartSlot};
use super::{AIRSHIP, FANCY, HISTORIC};

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
