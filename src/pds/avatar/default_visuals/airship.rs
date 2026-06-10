//! Airship family builder — envelope + gondola lighter-than-air
//! default avatar.
//!
//! Silhouette anatomy: a plank gondola basket at the root (the
//! locomotion centre), a fabric envelope floating above it on brass
//! struts, stern stabiliser fins in a Y- or X-configuration, optional
//! engine pods on the gondola flanks, and the Steam / Solar / Hybrid
//! ornament kit shared with the boat family (funnel on the gondola
//! stern, solar panel on the gondola roof, antenna on the envelope
//! crown). The pfp banner flies from a short jackstaff on the gondola
//! bow.
//!
//! Colour assignments:
//!   envelope        = primary_accent  (largest visible surface, fabric)
//!   gondola         = secondary_accent (plank basket)
//!   fins            = tertiary_accent
//!   nose jewel      = eye_color (glowing mooring lamp)
//!   struts / pods / funnel / panel = hair_color (metallic tone)

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirshipDesign, AvatarBody, AvatarPalette, EnvelopeForm};

use super::common::{
    brass_mat, capsule, cloth_mat, cuboid, cylinder, funnel_mat, glow_mat, id_quat, metal_mat,
    pastel, pfp_banner, plank_mat, prim, quat_x, quat_xyzw, quat_z, sphere, with_torture,
};

pub(super) fn build(did: &str) -> Generator {
    let palette = AvatarPalette::for_did(did);
    let body = AvatarBody::for_did(did);
    let ship = AirshipDesign::for_did(did);

    let envelope_color = palette.primary_accent;
    let gondola_color = palette.secondary_accent;
    let fin_color = palette.tertiary_accent;
    let jewel_color = palette.eye_color;
    let metal_color = palette.hair_color;

    let h = body.height_scale;
    let w = body.shoulder_width_scale;
    let limb = body.limb_thickness_scale;

    // ---- Gondola (root) ----------------------------------------------------
    // Basket sides flare outward toward the rim (negative taper).
    let gondola_x = 0.85 * w * ship.gondola_width_scale;
    let gondola_y = 0.45;
    let gondola_z = 1.7 * h * ship.gondola_length_scale;
    let gondola_kind = with_torture(
        cuboid([gondola_x, gondola_y, gondola_z], plank_mat(gondola_color)),
        0.0,
        -0.15,
        [0.0, 0.0, 0.0],
    );

    // ---- Envelope ----------------------------------------------------------
    let env_r = 0.55 * w * ship.envelope_radius_scale;
    let env_len = 2.6 * h * ship.envelope_length_scale;
    // Capsule laid stern-first (local +Y → world +Z) so the teardrop
    // taper narrows the *stern*; the fat end faces forward.
    let lay_stern_first = quat_xyzw(quat_x(FRAC_PI_2));
    let env_y = gondola_y * 0.5 + 0.9 * ship.envelope_lift_scale + env_r;

    let make_envelope = |x: f32, r: f32| {
        prim(
            with_torture(
                capsule(r, env_len, cloth_mat(envelope_color)),
                0.0,
                ship.envelope_taper,
                [0.0, 0.0, 0.0],
            ),
            [x, env_y, 0.0],
            lay_stern_first,
        )
    };

    let mut envelopes: Vec<Generator> = Vec::new();
    let twin = matches!(ship.envelope_form, EnvelopeForm::TwinHull);
    if twin {
        let r = env_r * 0.68;
        let spread = r + 0.06;
        envelopes.push(make_envelope(-spread, r));
        envelopes.push(make_envelope(spread, r));
    } else {
        envelopes.push(make_envelope(0.0, env_r));
    }

    // Glowing mooring lamp on the envelope nose.
    let nose_z = -(env_len * 0.5 + env_r * 0.6);
    let nose_lamp = prim(
        sphere(0.09 * w, 3, glow_mat(jewel_color)),
        [0.0, env_y, nose_z],
        id_quat(),
    );

    // ---- Struts ------------------------------------------------------------
    // Vertical brass rods from the gondola rim up into the envelope
    // belly, in pairs along the hull length.
    let strut_h = env_y - env_r * 0.6;
    let mut struts: Vec<Generator> = Vec::new();
    let pair_span = gondola_z * 0.36;
    for i in 0..ship.strut_pairs {
        // Spread pairs evenly across [-pair_span, pair_span].
        let t = if ship.strut_pairs == 1 {
            0.0
        } else {
            (i as f32 / (ship.strut_pairs - 1) as f32) * 2.0 - 1.0
        };
        let z = t * pair_span;
        for x in [-gondola_x * 0.38, gondola_x * 0.38] {
            struts.push(prim(
                cylinder(0.028 * limb, strut_h, 8, brass_mat(metal_color)),
                [x, gondola_y * 0.5 + strut_h * 0.5, z],
                id_quat(),
            ));
        }
    }

    // ---- Stern fins ----------------------------------------------------------
    // Radial tapered blades around the envelope stern. 3 fins = Y-tail
    // (one up, two down-angled); 4 fins = X-config.
    let fin_span = 0.55 * ship.fin_scale;
    let fin_ring = env_r * 0.75 + fin_span * 0.5;
    let fin_z = env_len * 0.42;
    // 3 fins: 0°/120°/240° (Y-tail). 4 fins: 45° + 90° steps (X-config).
    let angles: Vec<f32> = if ship.fin_count == 3 {
        (0..3)
            .map(|i| i as f32 * std::f32::consts::TAU / 3.0)
            .collect()
    } else {
        (0..4)
            .map(|i| std::f32::consts::FRAC_PI_4 + i as f32 * FRAC_PI_2)
            .collect()
    };
    let mut fins: Vec<Generator> = Vec::new();
    for theta in angles {
        let fin_kind = with_torture(
            cuboid([0.05, fin_span, 0.45], metal_mat(fin_color)),
            0.0,
            0.35,
            [0.0, 0.0, 0.0],
        );
        fins.push(prim(
            fin_kind,
            [
                theta.sin() * fin_ring,
                env_y + theta.cos() * fin_ring,
                fin_z,
            ],
            quat_xyzw(quat_z(-theta)),
        ));
    }

    // ---- Engine pods ---------------------------------------------------------
    // Capsule nacelles on the gondola flanks, nose jewel forward.
    let mut pods: Vec<Generator> = Vec::new();
    let pod_x = gondola_x * 0.5 + 0.14;
    for i in 0..ship.engine_pods_per_side {
        let z = -gondola_z * 0.18 + i as f32 * gondola_z * 0.36;
        for x in [-pod_x, pod_x] {
            let mut pod = prim(
                capsule(0.09, 0.30, brass_mat(metal_color)),
                [x, 0.05, z],
                quat_xyzw(quat_x(FRAC_PI_2)),
            );
            // Child of the rotated pod: local -Y is world -Z (forward).
            pod.children.push(prim(
                sphere(0.05, 2, glow_mat(jewel_color)),
                [0.0, -0.24, 0.0],
                id_quat(),
            ));
            pods.push(pod);
        }
    }

    // ---- Archetype ornaments ---------------------------------------------
    let mut ornaments: Vec<Generator> = Vec::new();
    if ship.archetype.has_smokestacks() {
        // Flared funnel on the gondola stern roof — fat and sooty so
        // it reads from a distance (matches the boat family's stacks).
        ornaments.push(prim(
            with_torture(
                cylinder(0.09 * limb, 0.45, 12, funnel_mat(metal_color)),
                0.0,
                -0.25,
                [0.0, 0.0, 0.0],
            ),
            [0.0, gondola_y * 0.5 + 0.22, gondola_z * 0.35],
            id_quat(),
        ));
    }
    if ship.archetype.has_solar_panel() {
        ornaments.push(prim(
            cuboid([0.55 * w, 0.03, 0.55 * h], brass_mat(metal_color)),
            [0.0, gondola_y * 0.5 + 0.10, gondola_z * 0.05],
            quat_xyzw(quat_x(0.15)),
        ));
    }
    if ship.archetype.has_antenna() {
        // Spire on the envelope crown.
        let antenna_h = 0.5;
        ornaments.push(prim(
            cylinder(0.015 * limb, antenna_h, 8, brass_mat(metal_color)),
            [0.0, env_y + env_r * 0.9 + antenna_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // ---- Pfp banner on a bow jackstaff -------------------------------------
    let staff_h = 0.6;
    let mut jackstaff = prim(
        cylinder(0.015, staff_h, 8, brass_mat(metal_color)),
        [0.0, gondola_y * 0.5 + staff_h * 0.5, -gondola_z * 0.42],
        id_quat(),
    );
    jackstaff.children.push(pfp_banner(
        did,
        0.40,
        0.30,
        [0.0, staff_h * 0.18, 0.30 * 0.5 + 0.04],
        quat_xyzw(quat_z(FRAC_PI_2)),
        pastel(envelope_color),
    ));

    // ---- Assemble -----------------------------------------------------------
    let mut gondola = prim(gondola_kind, [0.0, 0.0, 0.0], id_quat());
    gondola.transform = Default::default();
    gondola.children.extend(envelopes);
    gondola.children.push(nose_lamp);
    gondola.children.extend(struts);
    gondola.children.extend(fins);
    gondola.children.extend(pods);
    gondola.children.extend(ornaments);
    gondola.children.push(jackstaff);
    gondola
}
