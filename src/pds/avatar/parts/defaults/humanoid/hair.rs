//! Hair archetypes as primitive clump masses — hairline first.
//!
//! Every style composes from the canonical kit: a **helmet dome** (a
//! profile-cut sphere whose *front rim is the drawn hairline*, tilted back
//! so the rim rises at the brow and drops toward the nape), optional
//! **fringe** (its lower edge redraws the hairline), **temple corners**
//! (the cut-back at the temples that separates hair from swim cap),
//! **nape mass**, and one **appendage** (tail / bun / spikes / puffs).
//!
//! Anti-cap discipline: domes sit 1.05–1.12× off the skull, at least one
//! mass exits the skull's convex hull per style, and the buzz cut — the
//! one deliberately skull-tight style — sells itself with a crisp cut edge
//! and a darker shade instead of volume.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::default_visuals::common::{
    blob_ellipsoid, blob_group, capsule, cone, cuboid, id_quat, prim, quat_mul, quat_x, quat_xyzw,
    quat_z, sphere, torus, with_cut, with_shape,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::HairStyle;

use super::super::super::PartCtx;
use super::super::common::shade;
use super::head::{contrast_feature, lum};

/// Build the clump masses for `style` in head-local space (skull centre at
/// the origin, radius `r`, shell scaled `(sx, ·, sz)`, forehead hairline at
/// `hairline_y`).
pub(super) fn masses(
    ctx: &PartCtx,
    style: HairStyle,
    r: f32,
    sx: f32,
    sz: f32,
    hairline_y: f32,
) -> Vec<Generator> {
    // Hair must separate from the skin by value or the style reads as a
    // swim cap regardless of shape.
    let mut hair_c = ctx.palette.hair_color;
    if (lum(hair_c) - lum(ctx.palette.skin_tone)).abs() < 0.15 {
        hair_c = contrast_feature(hair_c, ctx.palette.skin_tone);
    }
    let hair = ctx.materials.cloth(hair_c);
    let mut out = Vec::new();

    // Helmet dome: profile-cut sphere, `cut` = kept-latitude floor (higher
    // cut = shallower cap), tilted back so the front rim lifts to the
    // hairline and the back rim wraps the nape.
    let dome = |k: f32, cut: f32, tilt: f32, up: f32, back: f32| {
        let mut d = prim(
            with_cut(sphere(r * k, 4, hair.clone()), [0.0, 1.0], [cut, 1.0], 0.0),
            [0.0, up, back],
            quat_xyzw(quat_x(tilt)),
        );
        d.transform.scale = Fp3([sx * 1.02, 1.0, sz * 1.04]);
        d
    };
    // Temple corners — soft pads at the front rim sides; the cut-back
    // corner that makes a hairline read as one (#732: the old literal
    // blocks read as boxes glued to the temples).
    let temples = |out: &mut Vec<Generator>| {
        for s in [-1.0f32, 1.0] {
            let mut pad = prim(
                sphere(r * 0.30, 3, hair.clone()),
                [s * r * 0.84 * sx, hairline_y - r * 0.30, -r * 0.26 * sz],
                quat_xyzw(quat_z(-s * 0.18)),
            );
            pad.transform.scale = Fp3([0.55, 1.15, 0.85]);
            out.push(pad);
        }
    };
    // Nape — two blended lobes hugging the back of the skull down past the
    // occiput (#732): the old tapered slab left the classic bathing-cap
    // gap between dome rim and neck; the lower lobe now closes it.
    let nape = |out: &mut Vec<Generator>, depth: f32| {
        out.push(prim(
            blob_group(
                vec![
                    blob_ellipsoid(
                        [0.0, r * 0.14, r * 0.60 * sz],
                        [r * 0.86 * sx, r * 0.55, r * (0.28 + depth * 0.5) * sz],
                        id_quat(),
                        r * 0.18,
                    ),
                    blob_ellipsoid(
                        [0.0, -r * 0.40, r * 0.64 * sz],
                        [r * 0.60 * sx, r * 0.44, r * (0.20 + depth * 0.4) * sz],
                        id_quat(),
                        r * 0.18,
                    ),
                ],
                28,
                hair.clone(),
            ),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
    };
    // Fringe slab whose straight lower edge redraws the forehead hairline.
    let fringe = |out: &mut Vec<Generator>, drop: f32, roll: f32, off_x: f32| {
        out.push(prim(
            cuboid([r * 1.12 * sx, r * 0.34, r * 0.42], hair.clone()),
            [off_x, hairline_y - drop + r * 0.17, -r * 0.72 * sz],
            quat_xyzw(quat_z(roll)),
        ));
    };

    match style {
        HairStyle::Bald => {}
        HairStyle::Buzz => {
            // Skull-tight by design: crisp cut edge + a darker, matte
            // shade carries the read, not volume.
            let buzz = ctx.materials.cloth(shade(hair_c, 0.8));
            let mut d = prim(
                with_cut(sphere(r * 1.035, 4, buzz), [0.0, 1.0], [0.47, 1.0], 0.0),
                [0.0, r * 0.24, r * 0.02],
                quat_xyzw(quat_x(0.40)),
            );
            d.transform.scale = Fp3([sx * 1.02, 1.0, sz * 1.04]);
            out.push(d);
            temples(&mut out);
        }
        HairStyle::Crop => {
            out.push(dome(1.08, 0.45, 0.46, r * 0.28, r * 0.02));
            temples(&mut out);
            nape(&mut out, 0.3);
        }
        HairStyle::Bob => {
            out.push(dome(1.11, 0.40, 0.40, r * 0.24, r * 0.03));
            // Side curtains to the jaw — rounded (#732: the old flat slabs
            // read as boards glued to the head sides).
            for s in [-1.0f32, 1.0] {
                let mut curtain = prim(
                    capsule(r * 0.26, r * 0.62, hair.clone()),
                    [s * r * 0.94 * sx, -r * 0.16, r * 0.10],
                    id_quat(),
                );
                curtain.transform.scale = Fp3([0.55, 1.0, 1.55 * sz]);
                out.push(curtain);
            }
            nape(&mut out, 0.42);
        }
        HairStyle::SidePart => {
            out.push(dome(1.08, 0.44, 0.44, r * 0.27, r * 0.02));
            // The fringe sweeps across the brow — its roll IS the part.
            fringe(&mut out, r * 0.05, 0.16, r * 0.12);
            temples(&mut out);
            nape(&mut out, 0.3);
        }
        HairStyle::SlickBack => {
            // Sheared back, front rim standing high off the forehead.
            let mut d = prim(
                with_shape(
                    with_cut(
                        sphere(r * 1.08, 4, hair.clone()),
                        [0.0, 1.0],
                        [0.4, 1.0],
                        0.0,
                    ),
                    [0.0, 0.0],
                    [0.0, 0.0, 0.0],
                    [0.0, r * 0.5],
                ),
                [0.0, r * 0.30, r * 0.04],
                quat_xyzw(quat_x(0.26)),
            );
            d.transform.scale = Fp3([sx * 1.0, 1.0, sz * 1.05]);
            out.push(d);
            temples(&mut out);
            nape(&mut out, 0.3);
        }
        HairStyle::Ponytail => {
            out.push(dome(1.08, 0.45, 0.46, r * 0.28, r * 0.02));
            temples(&mut out);
            // Tie + tail; the tie is what makes it read.
            out.push(prim(
                torus(
                    0.018,
                    (r * 0.12).max(0.02),
                    ctx.materials.trim(shade(hair_c, 0.6)),
                ),
                [0.0, r * 0.42, r * 0.78 * sz],
                quat_xyzw(quat_x(FRAC_PI_2)),
            ));
            // Tail authored bent, flipped so the pinned end is the tie and
            // the curl swings out behind the nape.
            out.push(prim(
                with_shape(
                    capsule(r * 0.16, r * 1.15, hair.clone()),
                    [0.35, 0.35],
                    [0.0, 0.0, -r * 0.35],
                    [0.0, 0.0],
                ),
                [0.0, r * 0.42 - r * 0.62, r * 0.86 * sz],
                quat_xyzw(quat_x(std::f32::consts::PI)),
            ));
        }
        HairStyle::Bun => {
            out.push(dome(1.08, 0.45, 0.46, r * 0.28, r * 0.02));
            temples(&mut out);
            nape(&mut out, 0.3);
            let mut bun = prim(
                sphere(r * 0.3, 3, hair.clone()),
                [0.0, r * 0.78, r * 0.5 * sz],
                id_quat(),
            );
            bun.transform.scale = Fp3([1.0, 0.9, 1.0]);
            out.push(bun);
        }
        HairStyle::Pigtails => {
            out.push(dome(1.08, 0.44, 0.44, r * 0.27, r * 0.02));
            temples(&mut out);
            for s in [-1.0f32, 1.0] {
                out.push(prim(
                    torus(
                        0.016,
                        (r * 0.1).max(0.018),
                        ctx.materials.trim(shade(hair_c, 0.6)),
                    ),
                    [s * r * 0.95 * sx, r * 0.12, r * 0.28],
                    quat_xyzw(quat_z(FRAC_PI_2)),
                ));
                out.push(prim(
                    with_shape(
                        capsule(r * 0.14, r * 0.75, hair.clone()),
                        [0.3, 0.3],
                        [s * r * 0.2, 0.0, 0.0],
                        [0.0, 0.0],
                    ),
                    [s * r * 1.05 * sx, -r * 0.28, r * 0.28],
                    quat_xyzw(quat_z(-s * 0.35)),
                ));
            }
        }
        HairStyle::Spikes => {
            out.push(dome(1.04, 0.48, 0.30, r * 0.24, r * 0.02));
            temples(&mut out);
            // Odd count, irregular tilts — even rings read as a crown.
            const SPIKES: [(f32, f32, f32, f32, f32); 5] = [
                // (x, y, z, tilt_x, tilt_z)
                (0.0, 1.0, -0.15, -0.25, 0.0),
                (0.42, 0.88, 0.1, -0.1, -0.45),
                (-0.45, 0.9, 0.05, -0.15, 0.4),
                (0.2, 0.85, 0.5, 0.35, -0.2),
                (-0.18, 0.82, 0.55, 0.45, 0.15),
            ];
            for (x, y, z, tx, tz) in SPIKES {
                out.push(prim(
                    cone(r * 0.15, r * 0.5, 6, hair.clone()),
                    [x * r * sx, y * r, z * r * sz],
                    quat_xyzw(quat_mul(quat_x(tx), quat_z(tz))),
                ));
            }
        }
        HairStyle::Afro => {
            // One big sphere, up-back so the face emerges from its lower
            // front; the crisp face arc is the hairline.
            out.push(prim(
                sphere(r * 1.42, 4, hair.clone()),
                [0.0, r * 0.42, r * 0.12 * sz],
                id_quat(),
            ));
        }
        HairStyle::Curls => {
            out.push(dome(1.09, 0.43, 0.42, r * 0.26, r * 0.03));
            // Lumpy rim — the silhouette is the curl read.
            const CURLS: [(f32, f32, f32); 6] = [
                (0.0, 0.55, -0.85),
                (0.72, 0.5, -0.55),
                (-0.72, 0.5, -0.55),
                (0.95, 0.3, 0.15),
                (-0.95, 0.3, 0.15),
                (0.0, 0.42, 0.95),
            ];
            for (x, y, z) in CURLS {
                out.push(prim(
                    sphere(r * 0.26, 3, hair.clone()),
                    [x * r * sx, y * r, z * r * sz],
                    id_quat(),
                ));
            }
        }
        HairStyle::Long => {
            out.push(dome(1.09, 0.42, 0.42, r * 0.26, r * 0.03));
            fringe(&mut out, r * 0.02, -0.06, 0.0);
            // Front locks past the jaw + a broad back sheet, flared at the
            // bottom so it doesn't read as a tube.
            for s in [-1.0f32, 1.0] {
                out.push(prim(
                    with_shape(
                        capsule(r * 0.16, r * 1.3, hair.clone()),
                        [-0.2, -0.2],
                        [0.0, 0.0, 0.0],
                        [0.0, 0.0],
                    ),
                    [s * r * 1.0 * sx, -r * 0.45, -r * 0.42 * sz],
                    id_quat(),
                ));
            }
            // Back sheet as three blended lobes widening toward the tips —
            // an organic falling sheet with a bottom flare instead of the
            // old tapered plank (#732).
            out.push(prim(
                blob_group(
                    vec![
                        blob_ellipsoid(
                            [0.0, -r * 0.05, r * 0.68 * sz],
                            [r * 0.92 * sx, r * 0.55, r * 0.30],
                            id_quat(),
                            r * 0.25,
                        ),
                        blob_ellipsoid(
                            [0.0, -r * 0.70, r * 0.80 * sz],
                            [r * 1.02 * sx, r * 0.60, r * 0.26],
                            id_quat(),
                            r * 0.25,
                        ),
                        blob_ellipsoid(
                            [0.0, -r * 1.15, r * 0.86 * sz],
                            [r * 1.16 * sx, r * 0.50, r * 0.22],
                            id_quat(),
                            r * 0.25,
                        ),
                    ],
                    30,
                    hair.clone(),
                ),
                [0.0, 0.0, 0.0],
                id_quat(),
            ));
        }
        HairStyle::Horseshoe => {
            // Sides + back only; the bare crown wears the actual skin.
            // Keep a front-biased 58 % arc and yaw it half a turn so the
            // opening faces forward — robust against the sweep-start
            // convention (render-verified: the unrotated arc crossed the
            // brow like a headband).
            out.push(prim(
                with_cut(
                    torus(r * 0.17, r * 0.82, hair.clone()),
                    [0.0, 0.58],
                    [0.0, 1.0],
                    0.0,
                ),
                [0.0, -r * 0.34, r * 0.06],
                quat_xyzw(quat_mul(
                    quat_x(0.0),
                    crate::pds::avatar::default_visuals::common::quat_y(std::f32::consts::PI),
                )),
            ));
        }
    }
    out
}
