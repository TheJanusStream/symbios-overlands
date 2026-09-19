//! The running gear: the axles, the track rod and the torque tube.
//!
//! Without these the wheels stand BESIDE the machine with nothing joining them
//! to it, which is the one thing a swept-and-turned craft cannot get away
//! with: every other part of this body is read off the plan and therefore
//! touches its neighbour, and four wheels in mid-air undo all of it. It could
//! not be caught by eye either, because the chase camera looks down and
//! nothing under a car is ever in frame at play distance - see
//! `common::touch`, the test-only helper that catches it by arithmetic.

use crate::pds::generator::Generator;

use super::super::SkiffColours;
use super::RoadsterPlan;
use super::line;

/// Axle tube radius, as a fraction of the length.
const AXLE_R: f32 = 0.0125;

/// How far under the body's own sill the LIVE rear axle dips at the
/// centreline, as a fraction of the length. The dip is period-correct - a live
/// axle hangs below the frame - and it is also the only reason it can be SEEN:
/// at hub height the elliptical body has already closed in to a third of its
/// beam, so a straight beam is swallowed by the coachwork it passes under.
const AXLE_DROP: f32 = 0.011;

/// How far the dropped FRONT beam dips under the sill, as a fraction of its
/// own mid radius (#1367 defect 2).
///
/// It used to dip [`AXLE_DROP`] like the rear, and at a mid radius of 0.0115
/// of the length that left it overlapping the bonnet's belly by 0.0005 of the
/// length - 1.33 mm on a 2.65 m car - so the beam, the track rod and both
/// front wheels hung on ONE of the connectedness guard's surface samples. Half
/// its own radius is an overlap the beam carries by construction, at every
/// size, and the dip still shows.
const BEAM_DIP_OF_RADIUS: f32 = 0.5;

/// The axles, the track rod and the torque tube.
pub(super) fn build(kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
    let l = plan.length;
    let (axle_y, half_track) = (plan.axle_y(), plan.track * 0.5);
    let r = AXLE_R * l;
    let stations = plan.axle_stations();
    let (front, rear) = (stations[0], stations[stations.len() - 1]);

    // A dropped front beam, hub to hub.
    let dip = r * 0.92 * BEAM_DIP_OF_RADIUS;
    let fy = plan.sill_at(front) - dip;
    let mut beam: Vec<([f32; 3], f32)> = vec![([-half_track, axle_y, front], r)];
    beam.extend(
        [-0.52f32, 0.0, 0.52]
            .iter()
            .map(|&f| ([f * half_track, fy, front], r * 0.92)),
    );
    beam.push(([half_track, axle_y, front], r));
    kids.push(line(&beam, 12, c.machinery.clone()));

    // A live rear axle, with a banjo housing over the differential - the one
    // lump of machinery under a car that reads at 109 px/m.
    let ry = plan.sill_at(rear) - AXLE_DROP * l;
    let mut live: Vec<([f32; 3], f32)> = vec![([-half_track, axle_y, rear], r)];
    live.extend(
        [
            (-0.46f32, 0.95f32),
            (-0.19, 2.10),
            (0.0, 2.45),
            (0.19, 2.10),
            (0.46, 0.95),
        ]
        .iter()
        .map(|&(f, rf)| ([f * half_track, ry, rear], r * rf)),
    );
    live.push(([half_track, axle_y, rear], r));
    kids.push(line(&live, 14, c.machinery.clone()));

    // Track rod ahead of the front beam: what makes the front axle read as
    // STEERED rather than as a bar somebody laid across the car.
    kids.push(line(
        &[
            (
                [-half_track * 0.92, axle_y - r * 0.7, front + l * 0.045],
                r * 0.42,
            ),
            ([0.0, fy + dip * 0.4, front + l * 0.052], r * 0.42),
            (
                [half_track * 0.92, axle_y - r * 0.7, front + l * 0.045],
                r * 0.42,
            ),
        ],
        8,
        c.machinery.clone(),
    ));
    // Torque tube forward off the banjo into the body: a housing with nothing
    // running to it is a bar with a lump on it.
    let mid = rear * 0.45;
    kids.push(line(
        &[
            ([0.0, ry, rear + r * 2.2], r * 1.30),
            ([0.0, (ry + plan.sill_at(mid)) * 0.5, mid], r * 0.85),
        ],
        10,
        c.machinery.clone(),
    ));
}
