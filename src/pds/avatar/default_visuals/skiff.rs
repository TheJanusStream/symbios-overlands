//! Land-skiff family builder — the Car-locomotion ground vehicle.
//!
//! Anatomy: a low chassis slab at the root with a tapered wedge nose,
//! running gear per [`SkiffForm`]
//! (corner wheels, hover-skids, or a trike stance), a cockpit canopy
//! per [`CanopyStyle`], a stern
//! engine block with the Steam / Solar / Hybrid ornament kit, an
//! optional spoiler, and the pfp banner on a raked whip antenna.
//!
//! Colour assignments:
//!   chassis / nose  = primary_accent
//!   canopy glass / skids = secondary_accent
//!   hubs / spoiler / belt-line = tertiary_accent
//!   headlamp jewel  = eye_color
//!   tires / engine / exhausts = hair_color (dark metallic tone)

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::locomotion::CarParams;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AvatarBody, AvatarPalette, CanopyStyle, SkiffDesign, SkiffForm};

use super::common::{
    brass_mat, capsule, cuboid, cylinder, funnel_mat, glass_mat, glow_mat, id_quat, metal_mat,
    pastel, pfp_banner, prim, quat_x, quat_xyzw, quat_z, rubber_mat, sphere, torus, with_torture,
};

/// `seed` drives the derived look (re-roll re-seeds this); `did` is kept
/// only for identity references the seed must not touch — the pfp banner.
pub(super) fn build(seed: u64, did: &str) -> Generator {
    let palette = AvatarPalette::for_seed(seed);
    let body = AvatarBody::for_seed(seed);
    let skiff = SkiffDesign::for_seed(seed);

    let chassis_color = palette.primary_accent;
    let canopy_color = palette.secondary_accent;
    let trim_color = palette.tertiary_accent;
    let jewel_color = palette.eye_color;
    let metal_color = palette.hair_color;

    let h = body.height_scale;
    let w = body.shoulder_width_scale;
    let limb = body.limb_thickness_scale;

    // ---- Chassis (root) ------------------------------------------------------
    let chassis_x = 1.30 * w * skiff.chassis_width_scale;
    let chassis_y = 0.26;
    let chassis_z = 2.6 * h * skiff.chassis_length_scale;
    let half_z = chassis_z * 0.5;
    let chassis_kind = cuboid([chassis_x, chassis_y, chassis_z], metal_mat(chassis_color));

    // Wedge nose: a tapered cuboid laid nose-first so the taper
    // narrows toward the front (local +Y → world -Z).
    let nose_len = 0.55;
    let nose = prim(
        with_torture(
            cuboid(
                [chassis_x * 0.9, nose_len, chassis_y * 0.9],
                metal_mat(chassis_color),
            ),
            0.0,
            0.5,
            [0.0, 0.0, 0.0],
        ),
        [0.0, 0.0, -(half_z + nose_len * 0.5 - 0.05)],
        quat_xyzw(quat_x(-FRAC_PI_2)),
    );

    // Headlamp jewel on the nose tip.
    let headlamp = prim(
        sphere(0.07, 2, glow_mat(jewel_color)),
        [0.0, 0.02, -(half_z + nose_len - 0.02)],
        id_quat(),
    );

    // ---- Running gear -----------------------------------------------------------
    // The Car locomotion holds the entity origin one suspension-
    // equilibrium above the terrain: rays start at chassis-bottom and
    // settle at `rest_length - mg/(4k)`. Place the running gear so the
    // tire / skid undersides sit exactly on that contact plane —
    // otherwise the whole vehicle reads as floating (the wheels used
    // to stop ~0.35 m short of the ground).
    let car = CarParams::default();
    let equilibrium =
        car.suspension_rest_length.0 - (car.mass.0 * 9.81) / (4.0 * car.suspension_stiffness.0);
    let ground_y = -(car.chassis_half_extents.0[1] + equilibrium);

    let wheel_r = 0.28 * skiff.wheel_radius_scale;
    let wheel_x = chassis_x * 0.5 + 0.10;
    // Torus tire rolled onto its rim (axis along X) + brass hub, hung
    // from the chassis on a visible suspension strut.
    let chassis_bottom = -chassis_y * 0.5;
    let make_wheel = |x: f32, z: f32, r: f32| {
        let hub_y = ground_y + r;
        let mut wheel = prim(
            torus(r * 0.30, r, rubber_mat(metal_color)),
            [x, hub_y, z],
            quat_xyzw(quat_z(FRAC_PI_2)),
        );
        wheel.children.push(prim(
            sphere(r * 0.32, 2, brass_mat(trim_color)),
            [0.0, 0.0, 0.0],
            id_quat(),
        ));
        // Strut from the hub up to the chassis underside. Child of
        // the wheel (rotated +90° around Z), whose local +X maps to
        // world +Y — so "up" is a positive local-X offset.
        let strut_len = (chassis_bottom - hub_y).max(0.12);
        wheel.children.push(prim(
            cylinder(0.035, strut_len, 8, brass_mat(metal_color)),
            [strut_len * 0.5, 0.0, 0.0],
            quat_xyzw(quat_z(FRAC_PI_2)),
        ));
        wheel
    };

    let mut gear: Vec<Generator> = Vec::new();
    match skiff.form {
        SkiffForm::Rover => {
            for z in [-(half_z - 0.45), half_z - 0.45] {
                gear.push(make_wheel(-wheel_x, z, wheel_r));
                gear.push(make_wheel(wheel_x, z, wheel_r));
            }
        }
        SkiffForm::Trike => {
            gear.push(make_wheel(0.0, -(half_z - 0.35), wheel_r * 1.3));
            gear.push(make_wheel(-wheel_x, half_z - 0.45, wheel_r));
            gear.push(make_wheel(wheel_x, half_z - 0.45, wheel_r));
        }
        SkiffForm::DuneSkiff => {
            // Two long hover-skids with up-swept tips (same prow-rake
            // trick as the boat hulls: bow-first lay, local +Z → +Y).
            // Skid undersides ride the suspension contact plane.
            let skid_r = 0.10 * skiff.wheel_radius_scale + 0.04;
            let skid_len = chassis_z * 0.85;
            for x in [-chassis_x * 0.42, chassis_x * 0.42] {
                let mut skid = prim(
                    with_torture(
                        capsule(skid_r, skid_len, metal_mat(canopy_color)),
                        0.0,
                        0.0,
                        [0.0, 0.0, 0.30],
                    ),
                    [x, ground_y + skid_r, 0.0],
                    quat_xyzw(quat_x(-FRAC_PI_2)),
                );
                // Two pylons tie the skid to the chassis underside.
                // Child of the laid capsule: local +Y is world -Z, and
                // world +Y is local +Z, so pylons extend along local Z.
                let pylon_len = (chassis_bottom - (ground_y + skid_r)).max(0.12);
                for along in [-skid_len * 0.28, skid_len * 0.28] {
                    skid.children.push(prim(
                        cylinder(0.045, pylon_len, 8, brass_mat(metal_color)),
                        [0.0, along, pylon_len * 0.5],
                        quat_xyzw(quat_x(FRAC_PI_2)),
                    ));
                }
                gear.push(skid);
            }
        }
    }

    // ---- Canopy -----------------------------------------------------------------
    let canopy_z = -chassis_z * 0.12;
    let canopy: Option<Generator> = match skiff.canopy {
        CanopyStyle::Bubble => Some(prim(
            sphere(0.34 * w, 3, glass_mat(canopy_color)),
            [0.0, chassis_y * 0.5 + 0.12, canopy_z],
            id_quat(),
        )),
        CanopyStyle::Shell => Some(prim(
            with_torture(
                cuboid([0.58 * w, 0.36, 0.75], glass_mat(canopy_color)),
                0.0,
                0.45,
                [0.0, 0.0, 0.0],
            ),
            [0.0, chassis_y * 0.5 + 0.18, canopy_z],
            id_quat(),
        )),
        CanopyStyle::Open => Some(prim(
            // Low raked windscreen plate.
            cuboid([0.52 * w, 0.26, 0.03], glass_mat(canopy_color)),
            [0.0, chassis_y * 0.5 + 0.13, canopy_z - 0.30],
            quat_xyzw(quat_x(-0.35)),
        )),
    };

    // ---- Engine block + archetype ornaments ----------------------------------
    let engine_y = chassis_y * 0.5 + 0.15;
    let engine_z = half_z - 0.40;
    // Sooty engine block: raw hair-colour brass washes out to white on
    // platinum rolls, same as the funnels did.
    let mut engine = prim(
        cuboid([0.55 * w, 0.30, 0.50], funnel_mat(metal_color)),
        [0.0, engine_y, engine_z],
        id_quat(),
    );
    if skiff.exhaust_count > 0 {
        let xs: &[f32] = if skiff.exhaust_count == 1 {
            &[0.0]
        } else {
            &[-0.12, 0.12]
        };
        for x in xs {
            // Flared funnel raked toward the stern — fat and sooty,
            // matching the boat/airship stacks.
            engine.children.push(prim(
                with_torture(
                    cylinder(0.07 * limb, 0.35, 10, funnel_mat(metal_color)),
                    0.0,
                    -0.30,
                    [0.0, 0.0, 0.0],
                ),
                [*x, 0.28, 0.10],
                quat_xyzw(quat_x(0.45)),
            ));
        }
    }
    if skiff.archetype.has_solar_panel() {
        engine.children.push(prim(
            cuboid([0.50 * w, 0.025, 0.45], brass_mat(metal_color)),
            [0.0, 0.22, -0.05],
            quat_xyzw(quat_x(0.12)),
        ));
    }

    // ---- Spoiler -------------------------------------------------------------
    let spoiler: Option<Generator> = skiff.spoiler.then(|| {
        let mut wing = prim(
            cuboid([0.95 * chassis_x, 0.03, 0.26], metal_mat(trim_color)),
            [0.0, chassis_y * 0.5 + 0.40, half_z - 0.12],
            quat_xyzw(quat_x(-0.15)),
        );
        for x in [-chassis_x * 0.35, chassis_x * 0.35] {
            wing.children.push(prim(
                cuboid([0.04, 0.28, 0.04], brass_mat(metal_color)),
                [x, -0.16, 0.0],
                id_quat(),
            ));
        }
        wing
    });

    // ---- Whip antenna + pfp banner --------------------------------------------
    let whip_h = 0.85;
    let mut whip = prim(
        cylinder(0.012, whip_h, 8, brass_mat(metal_color)),
        [
            chassis_x * 0.38,
            chassis_y * 0.5 + whip_h * 0.45,
            half_z - 0.15,
        ],
        quat_xyzw(quat_x(skiff.antenna_rake_rad)),
    );
    let banner_size = 0.32;
    whip.children.push(pfp_banner(
        did,
        banner_size,
        [0.0, whip_h * 0.32, banner_size * 0.5 + 0.03],
        pastel(chassis_color),
    ));

    // ---- Assemble ---------------------------------------------------------------
    let mut chassis = prim(chassis_kind, [0.0, 0.0, 0.0], id_quat());
    chassis.transform = Default::default();
    chassis.children.push(nose);
    chassis.children.push(headlamp);
    chassis.children.extend(gear);
    if let Some(c) = canopy {
        chassis.children.push(c);
    }
    chassis.children.push(engine);
    if let Some(s) = spoiler {
        chassis.children.push(s);
    }
    chassis.children.push(whip);
    chassis
}
