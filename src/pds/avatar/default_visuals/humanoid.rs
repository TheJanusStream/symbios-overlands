//! Humanoid family builder — the primitive-built figure that finally
//! consumes the skin / hair / eye colours, `torso_leg_ratio`, and the
//! costume picks from
//! [`HumanoidStyle`].
//!
//! Anatomy: the pelvis block is the root (hips sit at the entity
//! origin, which keeps the figure roughly centred in the Humanoid
//! locomotion capsule), legs hang below, the tapered torso capsule
//! rises above with arms at the shoulder line, and the head sphere
//! carries hair, eyes, and optional headwear. The pfp banner flies as
//! a pennant from a backpack pole (or a belt pole when the figure
//! rolled no backpack).
//!
//! Colour assignments:
//!   head / arms     = skin_tone   (curated table)
//!   hair / hat      = hair_color
//!   eyes            = eye_color   (emissive when `eye_glow`)
//!   torso (shirt)   = primary_accent
//!   legs (trousers) = secondary_accent
//!   pelvis / boots / belt / pauldrons = tertiary_accent

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{AvatarBody, AvatarPalette, HatStyle, HumanoidStyle};

use super::common::{
    brass_mat, capsule, cloth_mat, cone, cuboid, cylinder, glow_mat, id_quat, pastel, pfp_banner,
    prim, quat_xyzw, quat_z, skin_mat, sphere, torus, with_torture,
};

/// `seed` drives the derived look (re-roll re-seeds this); `did` is kept
/// only for identity references the seed must not touch — the pfp banner.
pub(super) fn build(seed: u64, did: &str) -> Generator {
    let palette = AvatarPalette::for_seed(seed);
    let body = AvatarBody::for_seed(seed);
    let style = HumanoidStyle::for_seed(seed);

    let skin = palette.skin_tone;
    let hair = palette.hair_color;
    let eye = palette.eye_color;
    let shirt = palette.primary_accent;
    let trousers = palette.secondary_accent;
    let trim = palette.tertiary_accent;

    let h = body.height_scale;
    let w = body.shoulder_width_scale;
    let limb = body.limb_thickness_scale;
    let head_s = body.head_scale;

    // ---- Proportions --------------------------------------------------------
    // Total standing height splits into head + (torso : legs) per the
    // seeded ratio. All Y coordinates below are relative to the hips
    // (the root), which sit at the entity origin.
    let total_h = 1.70 * h;
    let head_d = 0.26 * head_s;
    let trunk_h = total_h - head_d - 0.02;
    let torso_h = trunk_h * body.torso_leg_ratio;
    let leg_h = trunk_h - torso_h;

    let torso_r = 0.155 * w;
    let leg_r = 0.055 * limb;
    let arm_r = 0.058 * limb;
    let head_r = head_d * 0.5;

    // ---- Pelvis (root) -------------------------------------------------------
    // Matches the torso diameter — anything wider reads as a shelf
    // poking out under the shirt.
    let pelvis = cuboid([torso_r * 1.9, 0.14, torso_r * 1.35], cloth_mat(trim));

    // ---- Legs ---------------------------------------------------------------
    let hip_x = torso_r * 0.55;
    let leg_len = (leg_h - 2.0 * leg_r).max(0.1);
    let make_leg = |x: f32| {
        let mut leg = prim(
            capsule(leg_r, leg_len, cloth_mat(trousers)),
            [x, -leg_h * 0.5, 0.0],
            id_quat(),
        );
        // Boot: small forward-offset block at the foot.
        leg.children.push(prim(
            cuboid([0.09, 0.07, 0.17], cloth_mat(trim)),
            [0.0, -(leg_len * 0.5 + leg_r * 0.5), -0.03],
            id_quat(),
        ));
        leg
    };

    // ---- Torso ----------------------------------------------------------------
    // Negative taper widens the top: shoulders broader than the waist.
    let torso_len = (torso_h - 2.0 * torso_r).max(0.1);
    let torso_center_y = 0.06 + torso_h * 0.5;
    let mut torso = prim(
        with_torture(
            capsule(torso_r, torso_len, cloth_mat(shirt)),
            0.0,
            -0.18,
            [0.0, 0.0, 0.0],
        ),
        [0.0, torso_center_y, 0.0],
        id_quat(),
    );

    // Belt at the torso base.
    torso.children.push(prim(
        torus(0.022, torso_r * 1.02, brass_mat(trim)),
        [0.0, -torso_h * 0.42, 0.0],
        id_quat(),
    ));

    // ---- Arms (children of the torso, hung from the shoulder line) -----------
    // The shoulder pivot sits a little below the torso crown so the
    // arm caps don't ride up beside the head.
    let shoulder_x = torso_r + arm_r + 0.015;
    let shoulder_y = torso_h * 0.34;
    let arm_len = (torso_h * 0.80 - 2.0 * arm_r).max(0.1);
    for side in [-1.0f32, 1.0] {
        let mut arm = prim(
            capsule(arm_r, arm_len, skin_mat(skin)),
            [side * shoulder_x, shoulder_y - arm_len * 0.5, 0.0],
            // Slight outward lean so the arms don't shave the torso.
            quat_xyzw(quat_z(-side * 0.10)),
        );
        if style.pauldrons {
            arm.children.push(prim(
                sphere(arm_r * 1.9, 2, brass_mat(trim)),
                [0.0, arm_len * 0.5 + arm_r * 0.4, 0.0],
                id_quat(),
            ));
        }
        torso.children.push(arm);
    }

    // ---- Head (child of the torso) -------------------------------------------
    let head_y = torso_h * 0.5 + 0.02 + head_r;
    let mut head = prim(
        sphere(head_r, 3, skin_mat(skin)),
        [0.0, head_y, 0.0],
        id_quat(),
    );

    // Hair cap: a flattened sphere sitting on the crown. Vertical
    // squash comes from the node's transform scale.
    let mut hair_cap = prim(
        sphere(head_r * 1.06 * style.hair_volume_scale, 3, cloth_mat(hair)),
        [0.0, head_r * 0.25, head_r * 0.10],
        id_quat(),
    );
    hair_cap.transform.scale = Fp3([1.0, 0.78, 1.0]);
    head.children.push(hair_cap);

    // Eyes on the forward (-Z) face.
    let eye_mat = if style.eye_glow {
        glow_mat(eye)
    } else {
        cloth_mat(eye)
    };
    for side in [-1.0f32, 1.0] {
        head.children.push(prim(
            sphere(head_r * 0.16, 2, eye_mat.clone()),
            [side * head_r * 0.34, head_r * 0.10, -head_r * 0.88],
            id_quat(),
        ));
    }

    // Headwear.
    match style.hat {
        HatStyle::None => {}
        HatStyle::Cone => {
            head.children.push(prim(
                cone(head_r, head_r * 2.4, 12, cloth_mat(trim)),
                [0.0, head_r * 1.15, 0.0],
                id_quat(),
            ));
        }
        HatStyle::TopHat => {
            // Crown + brim.
            head.children.push(prim(
                cylinder(
                    head_r * 0.82,
                    head_r * 1.3,
                    16,
                    cloth_mat(hair_contrast(hair)),
                ),
                [0.0, head_r * 1.30, 0.0],
                id_quat(),
            ));
            head.children.push(prim(
                cylinder(head_r * 1.35, 0.02, 16, cloth_mat(hair_contrast(hair))),
                [0.0, head_r * 0.72, 0.0],
                id_quat(),
            ));
        }
        HatStyle::Band => {
            head.children.push(prim(
                torus(0.018, head_r * 0.96, brass_mat(trim)),
                [0.0, head_r * 0.35, 0.0],
                id_quat(),
            ));
        }
    }
    torso.children.push(head);

    // ---- Backpack + pfp pennant -----------------------------------------------
    // The pole rises from the backpack (or straight from the belt
    // line when the figure rolled no pack).
    let banner_h = 0.30;
    let banner_w = 0.22;
    let pole_h = 0.55;
    let pole_base_y = if style.backpack {
        0.12
    } else {
        -torso_h * 0.40
    };
    let pole_z = torso_r + if style.backpack { 0.13 } else { 0.05 };
    let mut pole = prim(
        cylinder(0.012, pole_h, 8, brass_mat(trim)),
        [0.0, pole_base_y + pole_h * 0.5, pole_z],
        id_quat(),
    );
    pole.children.push(pfp_banner(
        did,
        banner_h,
        banner_w,
        [0.0, pole_h * 0.30, banner_w * 0.5 + 0.03],
        quat_xyzw(quat_z(FRAC_PI_2)),
        pastel(shirt),
    ));
    if style.backpack {
        torso.children.push(prim(
            cuboid([torso_r * 1.5, 0.30, 0.13], cloth_mat(trousers)),
            [0.0, 0.05, torso_r + 0.08],
            id_quat(),
        ));
    }
    torso.children.push(pole);

    // ---- Assemble ---------------------------------------------------------------
    let mut root = prim(pelvis, [0.0, 0.0, 0.0], id_quat());
    root.transform = Default::default();
    root.children.push(make_leg(-hip_x));
    root.children.push(make_leg(hip_x));
    root.children.push(torso);
    root
}

/// A hat shouldn't vanish into same-coloured hair: darken light hair,
/// lighten dark hair.
fn hair_contrast(hair: [f32; 3]) -> [f32; 3] {
    let luma = 0.299 * hair[0] + 0.587 * hair[1] + 0.114 * hair[2];
    if luma > 0.45 {
        [hair[0] * 0.35, hair[1] * 0.35, hair[2] * 0.35]
    } else {
        [
            (hair[0] + 0.35).min(1.0),
            (hair[1] + 0.35).min(1.0),
            (hair[2] + 0.35).min(1.0),
        ]
    }
}
