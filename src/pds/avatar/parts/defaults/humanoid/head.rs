//! Humanoid head builder — tier-aware skull + jaw construction, the seeded
//! resting expression, facial hair, and hair (see [`super::hair`]).
//!
//! Construction follows the classical two-mass armature: a **cranial ball**
//! (whose per-tier XZ scale sets the register: wide toy ball → flattened
//! adult skull) and a **jaw mass** whose width/taper recipe is the seeded
//! [`FaceShape`]. All feature positions come from the tier-banded landmark
//! fractions in [`FaceParams`](crate::seeded_defaults::FaceParams) — the head-local frame puts the skull centre
//! at the origin with the crown at `+1.15 r` and the chin at `-1.2 r`, so a
//! landmark fraction `f` (measured from the crown) maps to
//! `y = r · (1.15 − 2.35 f)`.
//!
//! Contrast contract: eyes, brows, mouth, and facial hair resolve their
//! colours *relative to the seeded skin tone* (≥ ~0.18 luminance separation)
//! so features neither wash out on fair skins nor vanish on deep ones.
//! Sub-centimetre details (eye highlights, small mouths) author at the
//! sanitiser's 1 cm floor and shrink via leaf `transform.scale`, which the
//! sanitiser permits down to 0.001.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::default_visuals::common::{
    blob_capsule, blob_ellipsoid, blob_group, blob_sphere, capsule, cone, cuboid, cylinder,
    id_quat, prim, quat_x, quat_xyzw, quat_z, sphere, torus, with_cut, with_shape, with_torture,
};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{FaceShape, FacialHair, NoseKind, StylizationTier};

use super::super::super::PartCtx;
use super::super::common::shade;
use super::hair;

/// Relative luminance (Rec. 601 approximation) — good enough to enforce
/// the feature-vs-skin contrast contract.
pub(super) fn lum(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Resolve a feature colour (brow / facial hair) against the skin: keep it
/// when it already separates, otherwise darken on light skins or lift
/// toward ivory on deep skins.
pub(super) fn contrast_feature(c: [f32; 3], skin: [f32; 3]) -> [f32; 3] {
    if (lum(c) - lum(skin)).abs() >= 0.18 {
        return c;
    }
    // Candidate shifts in both directions; keep whichever separates more
    // from the skin (a mid-value hair colour on a mid-value skin can fail
    // in one direction but not the other).
    let dark = shade(c, 0.3);
    let light = [
        (c[0] * 0.35 + 0.60).min(1.0),
        (c[1] * 0.35 + 0.57).min(1.0),
        (c[2] * 0.35 + 0.52).min(1.0),
    ];
    if (lum(dark) - lum(skin)).abs() >= (lum(light) - lum(skin)).abs() {
        dark
    } else {
        light
    }
}

/// Mouth-line colour: a darker skin shade on light skins, a warm lifted
/// tone on deep skins (a plain darkening would vanish there).
fn mouth_color(skin: [f32; 3]) -> [f32; 3] {
    if lum(skin) > 0.35 {
        shade(skin, 0.42)
    } else {
        [
            (skin[0] * 1.5 + 0.15).min(1.0),
            (skin[1] * 1.25 + 0.07).min(1.0),
            (skin[2] * 1.2 + 0.05).min(1.0),
        ]
    }
}

/// Blush tint — a hue shift of the *actual* skin toward red (a fixed pink
/// reads as paint on anything but fair skin).
fn blush_color(skin: [f32; 3]) -> [f32; 3] {
    [
        (skin[0] * 1.12 + 0.08).min(1.0),
        skin[1] * 0.72,
        skin[2] * 0.72,
    ]
}

pub(in super::super) fn head(ctx: &PartCtx) -> Generator {
    let bp = &ctx.blueprint;
    let face = &ctx.face;
    let tier = bp.tier;
    let r = bp.head_r;
    let skin_c = ctx.palette.skin_tone;
    let skin = ctx.materials.skin(skin_c);
    let hair_mat_c = ctx.palette.hair_color;

    // Landmark helper: fraction-from-crown → head-local y.
    let land = |f: f32| r * (1.15 - 2.35 * f);

    // ---- Cranial mass (#690) -------------------------------------------
    // The whole skull is one BlobGroup: the cranium ellipsoid plus the
    // face-shape jaw / cheek masses smoothly blended into a single skin
    // where the old scaled-sphere shell + tapered-cuboid jaw showed
    // intersection seams. The cranium element's semi-axes are exactly the
    // old shell's (r·sx, r·sy, r·sz), so the `z_surf` feature-seating curve
    // is unchanged; blends are kept small so the blend bulge never pushes
    // the surface past the features seated on it. The root carries NO scale
    // (node scale propagates to children — the arm-squash lesson).
    let (mut sx, mut sy, sz) = match tier {
        StylizationTier::Toy => (1.0, 1.0, 0.94),
        StylizationTier::Stylized => (0.94, 1.0, 0.97),
        StylizationTier::Realistic => (0.88, 1.0, 0.96),
        StylizationTier::Heroic => (0.86, 1.0, 0.97),
    };
    match face.shape {
        FaceShape::Oblong => sy *= 1.08,
        FaceShape::Diamond => sx *= 0.94,
        FaceShape::Round => sx *= 1.04,
        _ => {}
    }
    let blend = r * 0.12;
    let mut elements = vec![blob_ellipsoid(
        [0.0, 0.0, 0.0],
        [r * sx, r * sy, r * sz],
        id_quat(),
        blend,
    )];
    // Front surface of the cranium at a given height — features must seat ON
    // this curve (a fixed z buries eyes in the brow and floats mouths off
    // the receding chin).
    let z_surf =
        move |y: f32| -> f32 { -(r * sz * (1.0 - (y / (r * sy)).powi(2)).max(0.10).sqrt()) };

    // ---- Jaw mass per face shape --------------------------------------
    // Toy keeps the single ball (its "jaw" is at most a cheek overlay);
    // other tiers blend in a tapered jaw whose recipe is the face shape.
    let jaw_k: f32 = match tier {
        StylizationTier::Toy => 0.0,
        StylizationTier::Stylized => 0.8,
        StylizationTier::Realistic => 1.0,
        StylizationTier::Heroic => 1.15,
    };
    let has_cheeks = face.shape == FaceShape::Round || tier == StylizationTier::Toy;
    if has_cheeks {
        // Full-cheek mass widening the lower face; chin stays soft.
        elements.push(blob_ellipsoid(
            [0.0, -r * 0.48, -r * 0.22],
            [r * 0.55 * 1.5 * sx, r * 0.55 * 0.85, r * 0.55 * 1.1],
            id_quat(),
            blend,
        ));
    }
    if jaw_k > 0.0 && face.shape != FaceShape::Round {
        let (w_top, chin_frac): (f32, f32) = match face.shape {
            FaceShape::Oval => (1.30, 0.60),
            FaceShape::Square => (1.42, 0.85),
            FaceShape::Oblong => (1.28, 0.64),
            FaceShape::Heart => (1.36, 0.42),
            FaceShape::Diamond => (1.36, 0.46),
            FaceShape::Round => unreachable!(),
        };
        // The old tapered jaw block, as two blended masses: a wide hinge
        // ellipsoid up at the ears and a chin ellipsoid narrowed by the
        // face shape's `chin_frac` — the blend produces the taper the
        // cuboid needed upside-down torture for.
        let jaw_w = r * w_top * jaw_k.min(1.0) * sx * 0.5;
        elements.push(blob_ellipsoid(
            [0.0, -r * 0.37, -r * 0.16],
            [jaw_w * 0.95, r * 0.35, r * 0.47 * sz],
            id_quat(),
            blend,
        ));
        elements.push(blob_ellipsoid(
            [0.0, -r * 0.95, -r * 0.22],
            [jaw_w * chin_frac.max(0.3), r * 0.24, r * 0.32 * sz],
            id_quat(),
            r * 0.18,
        ));
        if face.shape == FaceShape::Square || tier == StylizationTier::Heroic {
            // A widened chin bar for the square/heroic jaw.
            elements.push(blob_capsule(
                [0.0, -r * 1.0, -r * 0.34],
                r * 0.13,
                r * 0.275 * jaw_k,
                quat_xyzw(quat_z(FRAC_PI_2)),
                blend,
            ));
        }
        if face.shape == FaceShape::Diamond {
            // Cheekbone accents — the widest point of the diamond.
            for s in [-1.0f32, 1.0] {
                elements.push(blob_sphere(
                    [s * r * 0.78 * sx, land(face.eye_line) - r * 0.3, -r * 0.35],
                    r * 0.2,
                    r * 0.08,
                ));
            }
        }
    }
    let mut head = prim(
        blob_group(elements, 40, skin.clone()),
        [0.0, 0.0, 0.0],
        id_quat(),
    );

    // ---- Neck: short, thick, trapezius flare — tilted a few degrees
    // forward (#726) so the throat meets the jaw underside and the nape
    // rises higher up the skull, instead of a vertical peg under the chin.
    // The base sinks into the torso's neck-root column.
    let neck_l = bp.neck_len + 0.10;
    head.children.push(prim(
        with_torture(
            cylinder(bp.neck_r * 1.25, neck_l, 10, skin.clone()),
            0.0,
            0.35,
            [0.0, 0.0, 0.0],
        ),
        [0.0, -(r * 1.12 - 0.05 + neck_l * 0.5), 0.0],
        quat_xyzw(quat_x(-0.12)),
    ));

    // The lower-face seat plane: features must sit proud of whichever mass
    // is frontmost at their height — the shell, the jaw block, or the
    // round-face cheek overlay (which buried mouths until it joined this
    // min). More negative = further forward.
    let has_jaw = jaw_k > 0.0 && face.shape != FaceShape::Round;
    let jaw_front = -r * (0.16 + 0.55 * sz);
    let cheek_front = -r * 0.84;
    let lower_face = move |y: f32| -> f32 {
        let mut z = z_surf(y);
        if has_jaw {
            z = z.min(jaw_front);
        }
        if has_cheeks {
            z = z.min(cheek_front);
        }
        z
    };

    // ---- Eyes ----------------------------------------------------------
    let er = face.eye_size * r * sx; // eye half-width
    let ex = er * (1.0 + face.eye_gap);
    let y_eye = land(face.eye_line);
    let z_eye = z_surf(y_eye) - er * 0.40;
    let open = face.eye_open;
    let iris_c = ctx.materials.cloth(ctx.palette.eye_color);
    let dot_eyes = face.iris_frac >= 0.9;
    let sclera = ctx.materials.cloth([0.93, 0.92, 0.88]);
    for s in [-1.0f32, 1.0] {
        let cx = s * ex;
        if dot_eyes {
            // Toy bead eye: one dark oblate dot, always with a highlight.
            let mut bead = prim(
                sphere(
                    er.max(0.011),
                    2,
                    ctx.materials.cloth(shade(ctx.palette.eye_color, 0.5)),
                ),
                [cx, y_eye, z_eye],
                id_quat(),
            );
            bead.transform.scale = Fp3([0.85, 0.85 * open, 0.5]);
            head.children.push(bead);
        } else {
            let mut white = prim(
                sphere(er.max(0.011), 2, sclera.clone()),
                [cx, y_eye, z_eye],
                id_quat(),
            );
            white.transform.scale = Fp3([1.0, 0.82 * open, 0.5]);
            head.children.push(white);
            let mut iris = prim(
                sphere((er * face.iris_frac).max(0.01), 2, iris_c.clone()),
                [cx, y_eye, z_eye - er * 0.42],
                id_quat(),
            );
            iris.transform.scale = Fp3([1.0, (0.82 * open).min(1.0), 0.45]);
            head.children.push(iris);
        }
        // Specular highlight — the "alive" dot, same world corner on both
        // eyes so the implied light source is consistent.
        let mut hl = prim(
            sphere(0.011, 2, ctx.materials.cloth([0.97, 0.97, 0.95])),
            [cx - er * 0.26, y_eye + er * 0.28 * open, z_eye - er * 0.62],
            id_quat(),
        );
        let hk = (er * 0.30 / 0.011).clamp(0.4, 1.2);
        hl.transform.scale = Fp3([hk, hk, hk * 0.7]);
        head.children.push(hl);
        // Upper lid line — the calm/adult read; its depth also encodes
        // eye openness (a dreamy face wears a heavier lid).
        if face.lidded {
            head.children.push(prim(
                cuboid(
                    [
                        er * 2.15,
                        (er * 0.55 * (1.15 - open)).max(0.01),
                        (er * 0.9).max(0.01),
                    ],
                    ctx.materials.skin(shade(skin_c, 0.78)),
                ),
                [cx, y_eye + er * 0.7 * open, z_eye + er * 0.05],
                id_quat(),
            ));
        }
    }

    // ---- Brows ----------------------------------------------------------
    // One prim each: angle carries the disposition, the asym bit cocks the
    // +X brow. Thickness bumps 1.5× on deep skins (contrast contract).
    let brow_c = ctx.materials.cloth(contrast_feature(hair_mat_c, skin_c));
    let brow_th = if lum(skin_c) < 0.35 { 1.5 } else { 1.0 };
    for s in [-1.0f32, 1.0] {
        let angle = face.brow_angle + if s > 0.0 { face.brow_asym } else { 0.0 };
        head.children.push(prim(
            cuboid(
                [
                    er * 2.1,
                    (er * 0.34 * brow_th).max(0.01),
                    (er * 0.5).max(0.01),
                ],
                brow_c.clone(),
            ),
            [
                s * ex * 1.02,
                y_eye + er * (1.2 + face.brow_height * 1.5),
                z_surf(y_eye + er * (1.2 + face.brow_height * 1.5)) - er * 0.25,
            ],
            quat_xyzw(quat_z(-s * angle)),
        ));
    }

    // ---- Nose -----------------------------------------------------------
    let y_nose = land(face.nose_line);
    match face.nose {
        NoseKind::NoNose => {}
        NoseKind::Dot => head.children.push(prim(
            sphere((r * 0.06).max(0.011), 2, skin.clone()),
            [0.0, y_nose, lower_face(y_nose) - r * 0.02],
            id_quat(),
        )),
        NoseKind::Nub => {
            let nose_z = lower_face(y_nose) - r * 0.02;
            let mut nub = prim(
                sphere(r * 0.11, 2, skin.clone()),
                [0.0, y_nose, nose_z - r * 0.03],
                id_quat(),
            );
            nub.transform.scale = Fp3([0.85, 0.75, 0.8]);
            head.children.push(nub);
        }
        NoseKind::Ball => head.children.push(prim(
            sphere(r * 0.16, 3, ctx.materials.skin(shade(skin_c, 1.06))),
            [0.0, y_nose, lower_face(y_nose) - r * 0.05],
            id_quat(),
        )),
        NoseKind::Wedge | NoseKind::StrongWedge => {
            let strong = face.nose == NoseKind::StrongWedge;
            let (w, d, tp) = if strong {
                (0.19, 0.30, [0.35, 0.15])
            } else {
                (0.16, 0.24, [0.45, 0.20])
            };
            head.children.push(prim(
                with_shape(
                    cuboid([r * w, (y_eye - y_nose) * 0.9, r * d], skin.clone()),
                    tp,
                    [0.0, 0.0, 0.0],
                    [0.0, if strong { 0.0 } else { -r * 0.05 }],
                ),
                [
                    0.0,
                    (y_eye + y_nose) * 0.5 - r * 0.03,
                    lower_face((y_eye + y_nose) * 0.5) - r * 0.06,
                ],
                id_quat(),
            ));
        }
    }
    // Heroic brow ridge — the plane break above the eyes.
    if tier == StylizationTier::Heroic {
        head.children.push(prim(
            cuboid([r * 1.05 * sx, r * 0.13, r * 0.2], skin.clone()),
            [
                0.0,
                y_eye + er * (1.2 + face.brow_height * 1.5) + er * 0.55,
                z_surf(y_eye + er * (1.2 + face.brow_height * 1.5)) + r * 0.05,
            ],
            id_quat(),
        ));
    }

    // ---- Mouth ----------------------------------------------------------
    let mc = ctx.materials.cloth(mouth_color(skin_c));
    let mw = face.mouth_width * 2.0 * r * sx;
    let y_m = land(face.mouth_line);
    let z_m = lower_face(y_m) - r * 0.04;
    let mx = face.mouth_off * 2.0 * r;
    let curve = face.mouth_curve;
    if curve.abs() < 0.12 {
        // Neutral dash.
        head.children.push(prim(
            capsule((r * 0.045).max(0.01), mw * 0.85, mc),
            [mx, y_m, z_m],
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
    } else {
        // Smile / frown arc: a path-cut torus ring facing the viewer.
        // Sweep convention (render-verified on seed 35, which wore its
        // smile on its forehead the other way round): the kept fraction
        // centred on 0.25 turns lands at the ring's BOTTOM after the
        // quat_x(+90°) stand-up (a smile); 0.75 lands at the top (a
        // frown). Sub-centimetre tube thickness comes from a uniform leaf
        // down-scale (authored larger to clear the 1 cm sanitise floor).
        let span = 0.13 + 0.11 * curve.abs();
        let centre = if curve > 0.0 { 0.25 } else { 0.75 };
        let tube_d = (r * 0.05).max(0.004);
        let major_d = mw / (2.0 * (PI * span).sin());
        let k = (tube_d / 0.014).min(1.0);
        let mut arc = prim(
            with_cut(
                torus(0.014, major_d / k, mc.clone()),
                [centre - span * 0.5, centre + span * 0.5],
                [0.0, 1.0],
                0.0,
            ),
            [mx, y_m + if curve > 0.0 { major_d } else { -major_d }, z_m],
            quat_xyzw(quat_x(FRAC_PI_2)),
        );
        arc.transform.scale = Fp3([k, k, k]);
        head.children.push(arc);
        // Open smile: a dark fill + tooth band under a big warm curve
        // (toy / stylized registers only).
        if curve > 0.62 && matches!(tier, StylizationTier::Toy | StylizationTier::Stylized) {
            let mut fill = prim(
                sphere(
                    (mw * 0.28).max(0.01),
                    2,
                    ctx.materials.cloth(shade(skin_c, 0.3)),
                ),
                [mx, y_m - r * 0.04, z_m + r * 0.01],
                id_quat(),
            );
            fill.transform.scale = Fp3([1.5, 0.75, 0.3]);
            head.children.push(fill);
            head.children.push(prim(
                cuboid(
                    [mw * 0.5, (r * 0.035).max(0.01), 0.012],
                    ctx.materials.cloth([0.93, 0.92, 0.88]),
                ),
                [mx, y_m + r * 0.005, z_m - r * 0.015],
                id_quat(),
            ));
        }
    }

    // ---- Warmth accents --------------------------------------------------
    if face.blush {
        let bc = ctx.materials.cloth(blush_color(skin_c));
        for s in [-1.0f32, 1.0] {
            let mut b = prim(
                sphere((r * 0.10).max(0.011), 2, bc.clone()),
                [
                    s * r * 0.56 * sx,
                    y_eye - r * 0.36,
                    z_surf(y_eye - r * 0.36) - r * 0.01,
                ],
                id_quat(),
            );
            b.transform.scale = Fp3([1.0, 0.6, 0.35]);
            head.children.push(b);
        }
    }
    if face.freckles {
        let fc = ctx.materials.cloth(shade(skin_c, 0.72));
        for s in [-1.0f32, 1.0] {
            for (dx, dy) in [(0.34, 0.10), (0.48, 0.16), (0.28, 0.20)] {
                let mut f = prim(
                    sphere(0.01, 1, fc.clone()),
                    [
                        s * r * dx * sx,
                        y_nose + r * dy,
                        z_surf(y_nose + r * dy) - r * 0.008,
                    ],
                    id_quat(),
                );
                f.transform.scale = Fp3([0.6, 0.6, 0.4]);
                head.children.push(f);
            }
        }
    }

    // ---- Ears -------------------------------------------------------------
    for s in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere((r * 0.16).max(0.011), 2, skin.clone()),
            [s * r * 0.97 * sx, (y_eye + y_nose) * 0.5, r * 0.02],
            id_quat(),
        ));
    }

    // ---- Facial hair -------------------------------------------------------
    let fh_c = ctx.materials.cloth(contrast_feature(hair_mat_c, skin_c));
    let moustache = |head: &mut Generator| {
        head.children.push(prim(
            capsule((r * 0.05).max(0.011), mw * 0.95, fh_c.clone()),
            [0.0, (y_nose + y_m) * 0.5 + r * 0.06, z_m - r * 0.005],
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
    };
    let chin_mass = |head: &mut Generator| {
        let mut m = prim(
            sphere(r * 0.38, 3, fh_c.clone()),
            [0.0, -r * 0.95, -r * 0.38 * sz],
            id_quat(),
        );
        m.transform.scale = Fp3([1.3 * sx, 0.8, 0.9]);
        head.children.push(m);
    };
    let jaw_slabs = |head: &mut Generator, th: f32| {
        for s in [-1.0f32, 1.0] {
            head.children.push(prim(
                cuboid([r * th, r * 0.62, r * 0.6], fh_c.clone()),
                [s * r * 0.82 * sx, -r * 0.5, -r * 0.1],
                id_quat(),
            ));
        }
    };
    match face.facial_hair {
        FacialHair::NoFacialHair => {}
        FacialHair::Moustache => moustache(&mut head),
        FacialHair::Goatee => head.children.push(prim(
            cone(r * 0.13, r * 0.3, 8, fh_c.clone()),
            [0.0, -r * 1.05, -r * 0.5 * sz],
            quat_xyzw(quat_x(PI)),
        )),
        FacialHair::MoustacheGoatee => {
            moustache(&mut head);
            head.children.push(prim(
                cone(r * 0.13, r * 0.3, 8, fh_c.clone()),
                [0.0, -r * 1.05, -r * 0.5 * sz],
                quat_xyzw(quat_x(PI)),
            ));
        }
        FacialHair::FullBeard => {
            moustache(&mut head);
            chin_mass(&mut head);
            jaw_slabs(&mut head, 0.24);
        }
        FacialHair::MuttonChops => jaw_slabs(&mut head, 0.32),
    }

    // ---- Hair ----------------------------------------------------------------
    let style = if ctx.has_hat {
        face.hair.under_hat()
    } else {
        face.hair
    };
    let hairline_y = land(face.hairline);
    for mass in hair::masses(ctx, style, r, sx, sz, hairline_y) {
        head.children.push(mass);
    }

    head
}
