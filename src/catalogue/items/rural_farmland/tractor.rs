//! Tractor — a Rural/Farmland prop. A classic farm tractor: a green chassis
//! under a rounded hood with a grille, headlights, exhaust stack and
//! pre-cleaner; an operator's platform with a sprung seat, a steering wheel
//! on its column, a roll bar and a step; big dished rear wheels under curved
//! fenders and small steers on a front axle; a drawbar behind and weights
//! in front. Nose toward `+X`; the broadside is what reads.
//!
//! Rebuilt from scratch under #972 after an in-world check ("clumsy and
//! blocky, wrongly rotated steering wheel and seat"). The steering wheel
//! was a torus turned `quat_x(0.5)`, which tilts its ring about the axle
//! line — so it faced the side of the tractor. The seat back was a box
//! wide in `X` and thin in `Z`, a board across the driver's back rather
//! than behind it. The fenders were flat slabs floating over the tyres, the
//! rear tyres sat 30 mm above the ground, and the spokes' outer faces were
//! flush with the hub dish's (a z-fight). Now the wheel is aimed at the
//! driver with [`aim_y`] on the column's own direction — the column is a
//! [`strut`], the wheel's normal is the same vector — the seat back is
//! thin in `X` and stands on the cushion's rear edge, the fenders are cut
//! tube shells concentric with the wheels ([`with_cut`]), the hood is a
//! half-cylinder on the engine box, and every tyre's bottom is at `y = 0`.
//!
//! #972 lesson 39: **a wheel faces along its column.** A steering wheel,
//! a dish, a lamp, a clock — anything that has to face somebody is aimed by
//! ONE direction vector that also places its stalk: build the stalk with
//! [`strut`] from the mount to the hub and aim the head with `aim_y` on the
//! same vector, and the two cannot disagree. Guard by rotating the head's
//! own `+Y` through the built quaternion and checking it is parallel to the
//! built stalk and points at the seat. `quat_x(θ)` on a torus tilts it
//! about `X` — sideways on a machine whose driver sits along `X`.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    aim_y, assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_y, quat_z,
    solid, sphere, strut, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{TRACTOR_GREEN, TRACTOR_YELLOW, enamel};

/// Tyre black, dark fittings, bare steel.
const TIRE: [f32; 3] = [0.08, 0.08, 0.09];
const DARK: [f32; 3] = [0.2, 0.2, 0.22];
const STEEL: [f32; 3] = [0.6, 0.6, 0.62];

/// Chassis rail, the root.
const CHASSIS: [f32; 3] = [3.1, 0.3, 0.6];
const CHASSIS_X: f32 = 0.1;
const CHASSIS_Y: f32 = 0.75;
/// Rear drive wheels and their fenders.
const REAR_X: f32 = -0.9;
const REAR_R: f32 = 0.82;
const REAR_W: f32 = 0.44;
const REAR_Z: f32 = 0.85;
/// The shell's bore clears the tyre by 30 mm; it also has to stay under the
/// sanitiser's 0.95 bore-to-radius cap, or the record is rewritten on the
/// way to the PDS.
const FENDER_IN: f32 = 0.85;
const FENDER_OUT: f32 = 0.9;
const FENDER_W: f32 = 0.5;
/// The fender shell's kept arc, in turns of a cylinder turned
/// `quat_x(+π/2)`: over the top from just above the rear horizontal to
/// just above the front one, where its front end sinks into the platform.
const FENDER_ARC: [f32; 2] = [0.53, 0.98];
/// Front steer wheels on their axle.
const FRONT_X: f32 = 1.3;
const FRONT_R: f32 = 0.4;
const FRONT_W: f32 = 0.26;
const FRONT_Z: f32 = 0.6;
/// Engine box and the half-round hood on it.
const HOOD_BOX: [f32; 3] = [1.5, 0.5, 0.84];
const HOOD_X: f32 = 0.85;
const HOOD_BOX_Y: f32 = 1.05;
const HOOD_R: f32 = HOOD_BOX[2] * 0.5;
const HOOD_FLAT: f32 = HOOD_BOX_Y + HOOD_BOX[1] * 0.5;
/// The half of a cylinder turned `quat_z(−π/2)` that faces up: angles
/// π/2..3π/2.
const HOOD_ARC: [f32; 2] = [0.25, 0.75];
/// Sink the hood's flat and the stacks' feet by this much so no face meets
/// another on one plane and nothing balances on a crown.
const SINK: f32 = 0.03;
/// Operator's platform on the chassis.
const FLOOR: [f32; 3] = [1.15, 0.05, 1.24];
const FLOOR_X: f32 = -0.35;
const FLOOR_Y: f32 = CHASSIS_Y + CHASSIS[1] * 0.5 + FLOOR[1] * 0.5;
const FLOOR_TOP: f32 = FLOOR_Y + FLOOR[1] * 0.5;
/// Seat: a pedestal from the floor, a cushion, a back on its rear edge.
const SEAT_X: f32 = -0.5;
const CUSHION: [f32; 3] = [0.5, 0.12, 0.5];
const CUSHION_Y: f32 = 1.36;
const BACK: [f32; 3] = [0.08, 0.45, 0.48];
/// Steering: the column from inside the cowl to the wheel's hub.
const COLUMN_FOOT: [f32; 3] = [0.0, 1.45, 0.0];
const WHEEL_HUB: [f32; 3] = [-0.25, 1.65, 0.0];
const WHEEL_R: f32 = 0.19;

pub struct Tractor;

impl CatalogueEntry for Tractor {
    fn slug(&self) -> &'static str {
        "tractor"
    }
    fn name(&self) -> &'static str {
        "Tractor"
    }
    fn description(&self) -> &'static str {
        "Classic green farm tractor with big rear wheels and an exhaust stack."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The unit direction from the column's foot to the wheel's hub — the one
/// vector that places the column and aims the wheel.
fn column_dir() -> [f32; 3] {
    let v = [
        WHEEL_HUB[0] - COLUMN_FOOT[0],
        WHEEL_HUB[1] - COLUMN_FOOT[1],
        WHEEL_HUB[2] - COLUMN_FOOT[2],
    ];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

fn build_tree() -> Generator {
    let green = || enamel(TRACTOR_GREEN);
    let mut prims = vec![
        // Chassis — the root.
        prim(
            solid(cuboid_tapered(CHASSIS, 0.0, green())),
            [CHASSIS_X, CHASSIS_Y, 0.0],
            id_quat(),
        ),
        // Engine box and the half-round hood, its flat sunk into the box.
        prim(
            solid(cuboid_tapered(HOOD_BOX, 0.0, green())),
            [HOOD_X, HOOD_BOX_Y, 0.0],
            id_quat(),
        ),
        prim(
            solid(with_cut(
                cylinder_tapered(HOOD_R, HOOD_BOX[0], 16, 0.0, green()),
                HOOD_ARC,
                [0.0, 1.0],
                0.0,
            )),
            [HOOD_X, HOOD_FLAT - SINK * 0.1, 0.0],
            quat_z(-FRAC_PI_2),
        ),
        // Grille proud of the engine box's face — and of the chassis nose,
        // which reaches the same plane 25 mm out.
        prim(
            cuboid_tapered([0.05, 0.42, 0.7], 0.0, enamel(DARK)),
            [HOOD_X + HOOD_BOX[0] * 0.5 + 0.04, HOOD_BOX_Y, 0.0],
            id_quat(),
        ),
        // Front weights on the chassis nose.
        prim(
            solid(cuboid_tapered([0.14, 0.3, 0.5], 0.0, enamel(DARK))),
            [CHASSIS_X + CHASSIS[0] * 0.5 + 0.07, CHASSIS_Y, 0.0],
            id_quat(),
        ),
        // Exhaust stack and its rain cap, standing in the hood's crown.
        prim(
            solid(cylinder_tapered(0.05, 0.9, 10, 0.0, enamel(DARK))),
            [1.15, HOOD_FLAT + HOOD_R - SINK + 0.45, 0.0],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.075, 0.02, 10, 0.0, enamel(DARK)),
            [1.15, HOOD_FLAT + HOOD_R - SINK + 0.9 + 0.01, 0.0],
            id_quat(),
        ),
        // Air pre-cleaner: a thin pipe with a bowl on top.
        prim(
            cylinder_tapered(0.035, 0.35, 8, 0.0, enamel(DARK)),
            [0.55, HOOD_FLAT + HOOD_R - SINK + 0.175, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.09, 4, enamel(STEEL)),
            [0.55, HOOD_FLAT + HOOD_R - SINK + 0.35 + 0.05, 0.0],
            id_quat(),
        ),
        // Operator's platform on the chassis.
        prim(
            solid(cuboid_tapered(FLOOR, 0.0, green())),
            [FLOOR_X, FLOOR_Y, 0.0],
            id_quat(),
        ),
        // Cowl behind the hood, the column's mount.
        prim(
            solid(cuboid_tapered([0.22, 0.6, 0.8], 0.0, green())),
            [0.0, FLOOR_TOP - 0.01 + 0.3, 0.0],
            id_quat(),
        ),
        // Seat: pedestal from the floor, cushion, back on the rear edge.
        prim(
            solid(cylinder_tapered(
                0.05,
                CUSHION_Y - CUSHION[1] * 0.5 - FLOOR_TOP + 0.02,
                8,
                0.0,
                enamel(DARK),
            )),
            [
                SEAT_X,
                (CUSHION_Y - CUSHION[1] * 0.5 + FLOOR_TOP - 0.01) * 0.5,
                0.0,
            ],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(CUSHION, 0.0, enamel(DARK))),
            [SEAT_X, CUSHION_Y, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(BACK, 0.0, enamel(DARK))),
            [
                SEAT_X - CUSHION[0] * 0.5 + BACK[0] * 0.5,
                CUSHION_Y + CUSHION[1] * 0.5 + BACK[1] * 0.5,
                0.0,
            ],
            id_quat(),
        ),
        // Steering column, and the wheel aimed along it.
        strut(COLUMN_FOOT, WHEEL_HUB, 0.018, 8, enamel(DARK)),
        // Drawbar behind, sunk into the chassis.
        prim(
            solid(cuboid_tapered([0.5, 0.05, 0.12], 0.0, enamel(DARK))),
            [CHASSIS_X - CHASSIS[0] * 0.5 - 0.22, CHASSIS_Y - 0.1, 0.0],
            id_quat(),
        ),
        // Front axle beam.
        prim(
            solid(cylinder_tapered(0.05, FRONT_Z * 2.0, 8, 0.0, enamel(DARK))),
            [FRONT_X, FRONT_R, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Steering wheel: a rim with two crossed spokes at its own origin, the
    // whole aimed along the column.
    let mut wheel = prim(
        torus(0.02, WHEEL_R, enamel(DARK)),
        WHEEL_HUB,
        aim_y(column_dir()),
    );
    // Two stocks, not one: crossed bars of equal thickness tie for depth
    // where they cross.
    for (k, t) in [(0.0_f32, 0.015_f32), (1.0, 0.02)] {
        wheel.children.push(prim(
            cuboid_tapered([WHEEL_R * 2.0, t, 0.03], 0.0, enamel(DARK)),
            [0.0, 0.0, 0.0],
            quat_y(k * FRAC_PI_2),
        ));
    }
    prims.push(wheel);

    // Headlights on the hood's face.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            cylinder_tapered(0.09, 0.05, 12, 0.0, glow([1.0, 0.92, 0.6], 1.6)),
            [
                HOOD_X + HOOD_BOX[0] * 0.5 + 0.025,
                HOOD_FLAT + 0.2,
                sz * 0.26,
            ],
            quat_z(FRAC_PI_2),
        ));
    }

    // Rear drive wheels: tyre, yellow dish and hub cap each a little
    // prouder than the last, and a fender shell over each.
    for sz in [-1.0_f32, 1.0] {
        let zc = sz * REAR_Z;
        prims.push(prim(
            solid(cylinder_tapered(REAR_R, REAR_W, 20, 0.0, enamel(TIRE))),
            [REAR_X, REAR_R, zc],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(0.38, REAR_W + 0.02, 16, 0.0, enamel(TRACTOR_YELLOW)),
            [REAR_X, REAR_R, zc],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(0.1, REAR_W + 0.06, 10, 0.0, enamel(DARK)),
            [REAR_X, REAR_R, zc],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            with_cut(
                tube(FENDER_OUT, FENDER_IN, FENDER_W, 24, green()),
                FENDER_ARC,
                [0.0, 1.0],
                0.0,
            ),
            [REAR_X, REAR_R, zc],
            quat_x(FRAC_PI_2),
        ));
    }
    // Front steer wheels with steel hubs.
    for sz in [-1.0_f32, 1.0] {
        let zc = sz * FRONT_Z;
        prims.push(prim(
            solid(cylinder_tapered(FRONT_R, FRONT_W, 16, 0.0, enamel(TIRE))),
            [FRONT_X, FRONT_R, zc],
            quat_x(FRAC_PI_2),
        ));
        prims.push(prim(
            cylinder_tapered(0.15, FRONT_W + 0.02, 12, 0.0, enamel(STEEL)),
            [FRONT_X, FRONT_R, zc],
            quat_x(FRAC_PI_2),
        ));
    }

    // Roll bar from the platform, and a step on the near side.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 1.3, 0.06], 0.0, green())),
            [-0.85, FLOOR_TOP - 0.02 + 0.65, sz * 0.55],
            id_quat(),
        ));
    }
    // The crossbar is a heavier stock than the uprights and stops 5 mm
    // short of their tops, so no face of it lies on a face of theirs.
    prims.push(prim(
        solid(cuboid_tapered([0.07, 0.07, 1.2], 0.0, green())),
        [-0.85, FLOOR_TOP - 0.02 + 1.3 - 0.035 - 0.005, 0.0],
        id_quat(),
    ));
    // The bracket's head is sunk into the floor and its foot passes through
    // the step, so neither end shares a plane with either.
    prims.push(prim(
        cuboid_tapered([0.04, 0.415, 0.04], 0.0, enamel(DARK)),
        [-0.15, FLOOR_TOP + 0.01 - 0.2075, -(FLOOR[2] * 0.5 - 0.03)],
        id_quat(),
    ));
    // The plate's inboard edge stops inside the bracket, a centimetre short
    // of its outer face.
    prims.push(prim(
        cuboid_tapered([0.3, 0.03, 0.26], 0.0, enamel(DARK)),
        [-0.15, FLOOR_TOP - 0.4, -(FLOOR[2] * 0.5 - 0.03) - 0.12],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
    };
    use crate::pds::{GeneratorKind, PrimCommon};

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Tractor.build(""), "tractor");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Tractor.build(""), "tractor");
    }

    /// Against the shipped build this names the spokes' outer faces flush
    /// with the hub dish.
    #[test]
    fn no_two_faces_tie_for_depth() {
        assert_no_coplanar_faces(&Tractor.build(""), "tractor");
    }

    /// **The steering wheel faces the driver, along its own column.** The
    /// rim's normal is its local `+Y` rotated by the built quaternion; it
    /// must be parallel to the built column and point back and up toward
    /// the seat. Against the shipped `quat_x(0.5)` the normal is
    /// `[0, 0.88, 0.48]` — sideways.
    #[test]
    fn the_steering_wheel_faces_the_driver_along_its_column() {
        let root = Tractor.build("");
        let mut rim: Option<([f32; 3], [f32; 3])> = None;
        let mut column: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Torus { major_radius, .. } if major_radius.0 > 0.1 => {
                rim = Some((at, rotate_by(g.transform.rotation.0, [0.0, 1.0, 0.0])));
            }
            GeneratorKind::Cylinder { radius, height, .. }
                if radius.0 < 0.03 && height.0 > 0.2 && height.0 < 0.5 && at[1] > 1.3 =>
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                column = Some(([at[0] + tip[0], at[1] + tip[1], at[2] + tip[2]], tip));
            }
            _ => {}
        });
        let (hub, normal) = rim.expect("a steering wheel rim");
        assert!(
            normal[0] < -0.5 && normal[1] > 0.3 && normal[2].abs() < 0.05,
            "tractor: the steering wheel's normal is {normal:?} — it does not face the seat"
        );
        let (top, tip) = column.expect("a steering column");
        let len = (tip[0] * tip[0] + tip[1] * tip[1] + tip[2] * tip[2]).sqrt();
        let dot = (tip[0] * normal[0] + tip[1] * normal[1] + tip[2] * normal[2]) / len;
        assert!(dot.abs() > 0.99, "the wheel is not square to its column");
        assert!(
            (0..3).all(|i| (top[i] - hub[i]).abs() < 0.01),
            "the column's head {top:?} is not at the hub {hub:?}"
        );
    }

    /// **The seat back stands behind the cushion, thin in `X`**, and rises
    /// from the cushion's top. Against the shipped build the back is a
    /// 0.5 m board thin in `Z`.
    #[test]
    fn the_seat_back_stands_behind_the_cushion() {
        let root = Tractor.build("");
        let mut cushion: Option<([f32; 3], [f32; 3])> = None;
        let mut boxes: Vec<([f32; 3], [f32; 3])> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, .. } = &g.kind {
                let s = size.0;
                if (s[0] - 0.5).abs() < 1e-4 && (s[2] - 0.5).abs() < 1e-4 && s[1] < 0.2 {
                    cushion = Some((at, s));
                } else {
                    boxes.push((at, s));
                }
            }
        });
        let (c, cs) = cushion.expect("a seat cushion");
        let (b, bs) = boxes
            .iter()
            .find(|(at, s)| {
                at[0] < c[0]
                    && at[1] > c[1]
                    && s[1] > 0.3
                    && at[2].abs() < 0.1
                    && (at[0] - c[0]).abs() < 0.5
            })
            .copied()
            .expect("a seat back behind and above the cushion");
        assert!(
            bs[0] < bs[2] * 0.5,
            "tractor: the seat back is {:?} — thin in Z, a board across the driver's back \
             instead of behind it",
            bs
        );
        assert!(
            (b[1] - bs[1] * 0.5 - (c[1] + cs[1] * 0.5)).abs() < 1e-3,
            "the back does not rise from the cushion's top"
        );
        assert!(
            b[0] - bs[0] * 0.5 >= c[0] - cs[0] * 0.5 - 1e-4,
            "the back hangs off the cushion's rear edge"
        );
    }

    /// **All four tyres touch the ground.** Against the shipped build the
    /// rear pair float 30 mm.
    #[test]
    fn all_four_tyres_touch_the_ground() {
        let mut tyres = 0;
        walk(&Tractor.build(""), [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder { radius, common, .. } = &g.kind
                && radius.0 >= 0.35
                && common.material.base_color.0 == TIRE
            {
                tyres += 1;
                assert!(
                    (at[1] - radius.0).abs() < 0.005,
                    "tractor: a tyre at {at:?} of radius {} floats at {}",
                    radius.0,
                    at[1] - radius.0
                );
            }
        });
        assert_eq!(tyres, 4);
    }

    /// **Each rear fender is a shell concentric with its wheel**, wider than
    /// the tyre, with its crown up — the kept arc's midpoint rotated by the
    /// built quaternion lands on `+Y`.
    #[test]
    fn the_fenders_are_concentric_shells_over_the_rear_wheels() {
        let root = Tractor.build("");
        let mut fenders = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Tube {
                radius,
                inner_radius,
                height,
                common: PrimCommon { torture, .. },
                ..
            } = &g.kind
            else {
                return;
            };
            fenders += 1;
            assert!(
                (at[0] - REAR_X).abs() < 1e-4 && (at[1] - REAR_R).abs() < 1e-4,
                "a fender at {at:?} is not centred on the rear wheel"
            );
            assert!(
                inner_radius.0 > REAR_R && height.0 > REAR_W,
                "a fender does not clear the tyre"
            );
            let cut = torture.path_cut.0;
            let mid = (cut[0] + cut[1]) * 0.5 * std::f32::consts::TAU;
            let crown = rotate_by(g.transform.rotation.0, [mid.cos(), 0.0, mid.sin()]);
            assert!(crown[1] > 0.99, "a fender's crown points {crown:?}");
            assert!(radius.0 > inner_radius.0);
        });
        assert_eq!(fenders, 2);
    }

    /// The hood's crown is up, and the stack and pre-cleaner stand IN it
    /// (their feet below the crown, above its flat) rather than balancing on
    /// it (#972 lesson 33b).
    #[test]
    fn the_stacks_stand_in_the_hood_crown() {
        let root = Tractor.build("");
        let mut hood: Option<(f32, f32, f32)> = None;
        let mut stacks: Vec<(f32, f32)> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cylinder {
                radius,
                height,
                common: PrimCommon { torture, .. },
                ..
            } = &g.kind
            {
                let cut = torture.path_cut.0;
                if cut != [0.0, 1.0] {
                    let mid = (cut[0] + cut[1]) * 0.5 * std::f32::consts::TAU;
                    let crown = rotate_by(g.transform.rotation.0, [mid.cos(), 0.0, mid.sin()]);
                    assert!(crown[1] > 0.99, "the hood's crown points {crown:?}");
                    hood = Some((at[1], radius.0, at[0]));
                } else if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0]
                    && at[1] > 1.8
                    && height.0 > 0.3
                {
                    stacks.push((at[1] - height.0 * 0.5, at[0]));
                }
            }
        });
        let (flat, r, hx) = hood.expect("a half-round hood");
        assert_eq!(stacks.len(), 2, "an exhaust stack and a pre-cleaner");
        for (foot, x) in stacks {
            let crown_there = flat + r;
            assert!((x - hx).abs() < r, "a stack at x {x} is off the hood");
            assert!(
                foot < crown_there && foot > flat,
                "tractor: a stack's foot at {foot} is not in the hood (flat {flat}, crown {crown_there})"
            );
        }
    }
}
