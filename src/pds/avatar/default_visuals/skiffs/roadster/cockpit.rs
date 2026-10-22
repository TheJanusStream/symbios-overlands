//! The open car's cockpit: the windscreen frame, the seat, the steering column
//! and its wheel - and on a tourer, the second row's bench.
//!
//! A closed car draws none of this: its cabin would hide every part of it
//! ([`super::hardtop`]).

use std::f32::consts::FRAC_PI_2;

use bevy::math::Vec3;

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{bevel, id_quat, prim, quat_x, quat_xyzw, sphere, torus};
use super::super::{STEER_GAIN, STEER_LIMIT_DEG, SkiffColours, dim};
use super::RoadsterPlan;
use super::{TUB_HOLLOW, line};

/// How far ahead of the cockpit's forward lip the screen's feet stand, as a
/// fraction of the length (#1651).
///
/// Far enough that each raked post leaves the scuttle through its CROWN. At
/// 0.004, where the screen stood until #1651, a post broke out through the
/// scuttle's flat after face - the dash - instead, and the sill rail and both
/// feet stood up to 0.013 of the length proud of it: the frame's lower part,
/// in plain view of the chase camera, which looks into the cockpit from
/// behind. Every stance the blueprint can roll buries the frame from 0.0175;
/// this keeps 0.0045 of the length (a centimetre on the hero) to spare and
/// still stands the screen at the cockpit's lip rather than on the cowl.
const SCREEN_SET: f32 = 0.022;

/// The steering column's offset from the centreline and the wheel's outer
/// radius, as shares of the body's half-width at the seat cushion.
///
/// The radius is read off the WIDTH, as the offset is, so the rim's outboard
/// edge stands at 0.99 of the cushion's half-width on every body - inside the
/// flank. Drawn at 0.085 of the length, face-on now, the wheel spanned most
/// of the cockpit and its rim stood out past the flank (#1651).
const COLUMN_X: f32 = 0.44;
const WHEEL_R: f32 = 0.55;
/// How far the column runs on past its foot into the footwell floor, as a
/// fraction of the length (#1723).
const COLUMN_SEAT: f32 = 0.02;
/// The rim's tube radius and how far the rim stands proud of the boss toward
/// the driver - the dish a period wheel has - as fractions of the length.
const RIM_R: f32 = 0.0065;
const WHEEL_DISH: f32 = 0.012;
/// The boss at the wheel's centre and the spokes running out of it into the
/// rim, likewise.
const BOSS_R: f32 = 0.012;
const SPOKE_R: f32 = 0.0035;
/// How many degrees the steering wheel turns for each degree the front
/// wheels steer (#1653): a quarter turn at their full lock, which is as far
/// as the sanitiser lets a steer term turn anything. A real car's ratio is
/// twice this and more, and a wheel turned half as far barely reads from the
/// chase camera.
const STEERING_RATIO: f32 = 6.0;

/// The screen frame, the seat, the column and the wheel, and a bench where the
/// body has a second row.
pub(super) fn build(kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
    kids.push(screen(plan, c));
    let sz = plan.seat_z();
    seat(kids, plan, c, sz);
    kids.extend(steering(plan, c, sz));
    // The tourer's second row: the same seat on the station the body names,
    // and the only part that body adds (#1367).
    if let Some(bench) = plan.bench_z() {
        seat(kids, plan, c, bench);
    }
}

/// The windscreen frame: a CLOSED loop - post, header, post, sill rail -
/// because an open U of thin chrome reads as a roll hoop rather than as a
/// screen frame. NO glass in it: `SovereignMaterialSettings` has no alpha, so
/// a pane renders as a dark crate (#1359 rule 4).
///
/// It stands at the cockpit's FORWARD lip, not at the cowl: on the cowl it
/// leaves half a metre of bare scuttle behind it and reads as a hoop planted
/// in the bonnet. Its feet and its sill rail are buried in the scuttle just
/// ahead of the lip ([`SCREEN_SET`]), read at their own station.
fn screen(plan: &RoadsterPlan, c: &SkiffColours) -> Generator {
    let l = plan.length;
    let fz = plan.cockpit_z().1 + l * SCREEN_SET;
    let foot = plan.crown_at(fz) * 0.55;
    let post = plan.side_at(fz, foot) * 0.86;
    // The header raked aft over the cockpit, its middle swept a little
    // further back and up than its corners.
    let (rake, sweep, top, r) = (l * 0.046, l * 0.0502, plan.screen_top(), l * 0.0105);
    line(
        &[
            ([-post, foot, fz], r),
            ([-post * 0.97, top, fz - rake], r),
            ([0.0, top + l * 0.005, fz - sweep], r),
            ([post * 0.97, top, fz - rake], r),
            ([post, foot, fz], r),
            ([0.0, foot - l * 0.004, fz + l * 0.006], r),
            ([-post, foot, fz], r),
        ],
        6,
        c.brightwork.clone(),
    )
}

/// The steering column, and its wheel on a boss at the column's end.
///
/// The column comes out of the SCUTTLE - which is what a column comes
/// through; started from the footwell floor instead, it stands in mid-air -
/// and rises aft toward the seat.
///
/// The wheel is SQUARE TO ITS COLUMN by construction (#1651): it is drawn in
/// its own frame ([`wheel`]) and the boss carrying it is turned so that
/// frame's axis is the column's own direction, read off the column's two
/// points. So its face looks up the column at the seat and its top leans
/// forward, toward the screen, the way a real wheel's does. It was a flat
/// disc under a hand-set turn that tipped it some 75 degrees off square to
/// the column on the hero, its top leaning back over the seat, and the chase
/// camera - which looks down the column from behind - saw it tipped away
/// from it, a flat ellipse.
///
/// And it TURNS WITH THE STEERING (#1653). The boss carries a steer spin
/// about its own +Y - the column - that reads the car's turning about the
/// car's own up, as the front wheels' kingpins do, so it turns
/// [`STEERING_RATIO`] times as far as they do and saturates as they do. Read
/// about the column instead it would follow the car's lean into a bend too,
/// which the gait snaps in within a tenth of a second.
fn steering(plan: &RoadsterPlan, c: &SkiffColours, sz: f32) -> [Generator; 2] {
    let l = plan.length;
    let cushion = plan.half_width_at(sz + 0.062 * l);
    let wx = -cushion * COLUMN_X;
    let foot = Vec3::new(
        wx,
        plan.sill_at(0.0) * TUB_HOLLOW + l * 0.022,
        plan.cockpit_z().1 + l * 0.026,
    );
    let end = Vec3::new(wx, l * 0.052, sz + 0.120 * l);
    // Run on along its own line past the foot, down into the footwell floor
    // under the scuttle, so the column is fixed to the car (#1723): ended at
    // the foot it stood 3.4 to 6.4 mm clear of the bore wall.
    let seated = foot + (foot - end).normalize() * l * COLUMN_SEAT;
    let column = line(
        &[
            (seated.to_array(), l * 0.0048),
            (end.to_array(), l * 0.0048),
        ],
        6,
        c.machinery.clone(),
    );
    // The column stands in one x-plane, so one turn about x carries the
    // wheel's axis - its frame's +Y - up the column.
    let up_the_column = (end - foot).normalize();
    let mut boss = prim(
        sphere(dim(l * BOSS_R), 3, c.brightwork.clone()),
        end.to_array(),
        quat_xyzw(quat_x(up_the_column.z.atan2(up_the_column.y))),
    );
    boss.children = wheel(l, cushion * WHEEL_R, c);
    boss.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Steer {
            gain: Fp(STEER_GAIN * STEERING_RATIO),
            limit: Fp(STEER_LIMIT_DEG * STEERING_RATIO),
            follow_yaw: true,
        },
    ));
    [column, boss]
}

/// The wheel of outer radius `outer` in its boss's frame - its axis +Y,
/// toward the driver, and its top +Z: a hide-bound rim, dished toward the
/// driver, on three spokes, the period's T - across the wheel and down to the
/// bottom of the rim - each run from the boss's centre into the middle of the
/// rim's tube.
///
/// The rim is a torus, which is a true ring: a lathe ending at a non-zero
/// radius is closed with a full disc ([`super::wheels`]). Its hide is the
/// trunk's, held clear of the coachwork it is seen against.
fn wheel(l: f32, outer: f32, c: &SkiffColours) -> Vec<Generator> {
    let (tube, dish) = (dim(l * RIM_R), l * WHEEL_DISH);
    let ring = outer - tube;
    let spoke = l * SPOKE_R;
    vec![
        prim(
            torus(tube, ring, c.trunk.clone()),
            [0.0, dish, 0.0],
            id_quat(),
        ),
        line(
            &[
                ([-ring, dish, 0.0], spoke),
                ([0.0; 3], spoke),
                ([ring, dish, 0.0], spoke),
            ],
            6,
            c.brightwork.clone(),
        ),
        line(
            &[([0.0; 3], spoke), ([0.0, dish, -ring], spoke)],
            6,
            c.brightwork.clone(),
        ),
    ]
}

/// A Bevel back and a Bevel cushion down in the bored footwell, each sitting on
/// the bore's floor AT ITS OWN STATION. Taking the floor depth amidships
/// instead floats the back a finger clear of it, which is the same class of
/// mistake as a mount on a guessed fraction.
fn seat(kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours, sz: f32) {
    let l = plan.length;
    let cz = sz + 0.062 * l;
    kids.push(prim(
        bevel(
            [
                dim(plan.half_width_at(sz) * 1.34),
                dim(l * 0.036),
                dim(l * 0.115),
            ],
            l * 0.018,
            4,
            c.leather.clone(),
        ),
        [
            0.0,
            plan.sill_at(sz) * TUB_HOLLOW + l * 0.075,
            sz - l * 0.018,
        ],
        quat_xyzw(quat_x(FRAC_PI_2 - 0.20)),
    ));
    kids.push(prim(
        bevel(
            [
                dim(plan.half_width_at(cz) * 1.34),
                dim(l * 0.030),
                dim(l * 0.125),
            ],
            l * 0.016,
            4,
            c.leather.clone(),
        ),
        [0.0, plan.sill_at(cz) * TUB_HOLLOW + l * 0.024, cz],
        id_quat(),
    ));
}

#[cfg(test)]
mod tests {
    use bevy::math::{Affine3A, Vec3};

    use super::super::guard::{placed, plans, vertices};
    use super::*;
    use crate::pds::avatar::livery::skiff_colours;
    use crate::pds::avatar::parts::PartCtx;
    use crate::pds::generator::GeneratorKind;

    /// The steering's rim - the one torus anywhere under `parts` - and the
    /// frame it is drawn in, through every node above it.
    fn rim(parts: &[Generator]) -> (&Generator, Affine3A) {
        fn find(g: &Generator, parent: Affine3A) -> Option<(&Generator, Affine3A)> {
            let at = placed(g, parent);
            if matches!(g.kind, GeneratorKind::Torus { .. }) {
                return Some((g, at));
            }
            g.children.iter().find_map(|c| find(c, at))
        }
        parts
            .iter()
            .find_map(|p| find(p, Affine3A::IDENTITY))
            .expect("the wheel has a rim")
    }

    /// The column's two points - its seated end in the floor, then its end
    /// at the wheel: the steering's one top-level spine.
    fn column(parts: &[Generator]) -> (Vec3, Vec3) {
        let (g, points) = parts
            .iter()
            .find_map(|g| match &g.kind {
                GeneratorKind::Spine { points, .. } => Some((g, points)),
                _ => None,
            })
            .expect("the steering has a column");
        let at = placed(g, Affine3A::IDENTITY);
        let p = |i: usize| at.transform_point3(Vec3::from(points[i].position.0));
        (p(0), p(points.len() - 1))
    }

    /// No part of the screen frame stands against the dash (#1651): every
    /// vertex the mesher draws aft of the cockpit's forward lip stands clear
    /// above the scuttle's section there. So from the cockpit - the side the
    /// chase camera looks from - the frame rises out of the scuttle's crown,
    /// and its sill rail and its feet are buried in it.
    ///
    /// Before #1651 the feet stood 0.004 L ahead of the lip, each post broke
    /// out through the dash itself, and the frame stood up to 0.013 L proud of
    /// it inside the scuttle's section, on every body and blueprint.
    #[test]
    fn no_part_of_the_screen_frame_stands_against_the_dash() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let lip = plan.cockpit_z().1;
            let (hw, crown) = (plan.half_width_at(lip), plan.crown_at(lip));
            let frame = screen(&plan, &c);
            for v in vertices(&frame, placed(&frame, Affine3A::IDENTITY)) {
                let section = (v.x / hw).powi(2) + (v.y / crown).powi(2);
                assert!(
                    v.z >= lip || v.y <= 0.0 || section >= 1.0,
                    "{what}: the screen frame draws {v}, {} m behind the dash at z {lip} \
                     and inside the scuttle's section there ({section:.3} of it)",
                    lip - v.z
                );
            }
        }
    }

    /// The column's seated end stays inside the car (#1723): run on past its
    /// foot into the footwell floor so the column is fixed to the body, every
    /// vertex the mesher draws for it below the datum lies within the body's
    /// outer section at its station - in the floor's wall, or the footwell
    /// over it - never out through the tub's underside.
    #[test]
    fn the_column_is_seated_in_the_floor_and_buried_there() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let parts = steering(&plan, &c, plan.seat_z());
            let column = &parts[0];
            for v in vertices(column, placed(column, Affine3A::IDENTITY)) {
                let (hw, crown) = (plan.half_width_at(v.z), plan.crown_at(v.z));
                let section = (v.x / hw).powi(2) + (v.y / crown).powi(2);
                assert!(
                    v.y >= 0.0 || section < 1.0,
                    "{what}: the column draws {v} outside the tub's skin at z {} \
                     ({section:.3} of its section)",
                    v.z
                );
            }
        }
    }

    /// The wheel is square to its column, looks up it at the seat and leans
    /// its top forward toward the screen (#1651) - read off the drawn tree:
    /// the rim's ring axis through every node above it, against the column's
    /// own two points.
    ///
    /// Before #1651 the wheel was a disc whose axis stood some 75 degrees off
    /// the column's on the hero, its top leaning back over the seat.
    #[test]
    fn the_wheel_is_square_to_its_column_and_faces_the_seat() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let parts = steering(&plan, &c, plan.seat_z());
            let (foot, end) = column(&parts);
            let along = (end - foot).normalize();
            assert!(
                along.y > 0.0 && along.z < 0.0,
                "{what}: the column does not rise aft toward the seat"
            );
            let (_, frame) = rim(&parts);
            let axis = frame.transform_vector3(Vec3::Y).normalize();
            assert!(
                axis.dot(along) > 0.1f32.to_radians().cos(),
                "{what}: the rim's axis {axis} is {:.2} degrees off the column's {along}",
                axis.angle_between(along).to_degrees()
            );
            // It stands on the column's line, past its end toward the seat.
            let centre = Vec3::from(frame.translation) - end;
            assert!(
                centre.dot(along) > 0.0 && centre.reject_from(along).length() < 1e-4,
                "{what}: the rim's centre is {centre} off the column's end, not up its line"
            );
            // And its top - the ring's highest point - leans toward the screen.
            let up = (Vec3::Y - axis * axis.dot(Vec3::Y)).normalize();
            assert!(
                up.z > 0.0,
                "{what}: the wheel's top leans back, toward the seat ({up})"
            );
        }
    }

    /// The wheel turns with the steering (#1653): the part that carries the
    /// rim spins about the column - the axis the rim rings - reading the
    /// car's turning about the car's up, as the front wheels do, the same way
    /// round as they do and [`STEERING_RATIO`] times as far, saturating at the
    /// same rate of turn. Before #1653 it stood still.
    #[test]
    fn the_wheel_turns_with_the_steering() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let parts = steering(&plan, &c, plan.seat_z());
            let (_, rim) = rim(&parts);
            let ring = rim.transform_vector3(Vec3::Y).normalize();
            let hub = parts
                .iter()
                .find(|g| g.spin.is_some())
                .expect("a part of the steering turns");
            let spin = hub.spin.as_ref().expect("checked");
            let turns = placed(hub, Affine3A::IDENTITY)
                .transform_vector3(Vec3::from(spin.axis.0))
                .normalize();
            assert!(
                turns.dot(ring) > 0.1f32.to_radians().cos(),
                "{what}: it turns about {turns}, not the axis the rim rings, {ring}"
            );
            let [
                SpinTerm::Steer {
                    gain,
                    limit,
                    follow_yaw,
                },
            ] = spin.terms.as_slice()
            else {
                panic!(
                    "{what}: the wheel's spin is one steer term: {:?}",
                    spin.terms
                );
            };
            assert!(*follow_yaw, "{what}: a raked wheel must read the yaw");
            assert!(
                (gain.0 - STEERING_RATIO * STEER_GAIN).abs() < 1e-6 && gain.0 > 0.0,
                "{what}: gain {}",
                gain.0
            );
            assert!(
                (limit.0 / gain.0 - STEER_LIMIT_DEG / STEER_GAIN).abs() < 1e-4,
                "{what}: it saturates at {} deg/s, the front wheels at {}",
                limit.0 / gain.0,
                STEER_LIMIT_DEG / STEER_GAIN
            );
        }
    }

    /// The wheel stands inside the body's flank: in plan, no vertex of its
    /// rim is further out than the body's half-width at that vertex's own
    /// station (#1651). At 0.085 of the length - the disc's radius before
    /// #1651 - the wheel stood past the flank on every body, by as much as
    /// 0.015 of the length on the slimmest, which face-on is plain to see.
    #[test]
    fn the_wheel_stands_inside_the_bodys_flank() {
        let c = skiff_colours(&PartCtx::for_seed(0));
        for (plan, what) in plans() {
            let parts = steering(&plan, &c, plan.seat_z());
            let (g, frame) = rim(&parts);
            for v in vertices(g, frame) {
                let flank = plan.half_width_at(v.z);
                assert!(
                    v.x.abs() < flank,
                    "{what}: the rim draws {v}, {} m outside the flank at z {}",
                    v.x.abs() - flank,
                    v.z
                );
            }
        }
    }
}
