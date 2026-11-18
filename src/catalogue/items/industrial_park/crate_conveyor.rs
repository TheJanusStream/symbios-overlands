//! Crate conveyor - an Industrial-Park prop (#1942), one of the three
//! showpieces of a part's slide (#1940): an endless line of crates rides a
//! roller conveyor out of one machine, under a stamping press and into
//! another.
//!
//! The steel frame on its legs, the two housings - an infeed and an outfeed
//! box, each with a dark mouth behind strip curtains - and the press's
//! frame stand still and are solid. Six crates stand at ONE place inside
//! the infeed, each on a belt slide along +X ([`SPEED`] m/s over [`RUN`]
//! m) whose starts are spaced a sixth of the run apart, so they ride out as
//! one even stream, a crate every [`PERIOD`] seconds. The run reaches from
//! inside the infeed to inside the outfeed, so each crate's jump back to
//! its place happens inside the housings, out of sight. The price of one
//! place: wherever the pose stands still - an editor's selection, Settings >
//! Effects Off, a still preview - the run is empty and all six crates wait
//! inside the infeed.
//!
//! The press's ram rests at the bottom of its stroke, a few centimetres over
//! a crate's top, and hops up along +Y once every [`PERIOD`] seconds - the
//! crate interval. A bounce turns sharply where it rests and smoothly at its
//! top, so the ram eases up, falls and strikes: the sharp turn is the blow,
//! and its phase lands it just as a crate's middle passes beneath. The belt
//! and the hop read one clock and one copy offset (`world_builder::spin`),
//! so the beat holds exactly: the speed, the period and the run are all
//! exact in binary (5/8 m/s, 2 s and 15/2 m), and the run is the speed
//! times the period times the crates, so neither drifts from the other
//! even at the wall clock's 1.8e9 seconds. At Settings > Effects Reduced
//! the belt runs at half speed while the press keeps its beat (a hop is
//! calmed by its height, not its period): the ram then strikes a crate
//! every other beat and the gap between two crates on the others. It never
//! comes down into one at any setting, for its lowest point is its rest.
//!
//! The rollers under the run turn at the belt's own rate, and the press's
//! flywheel once a stroke - details, both. An amber beacon on the press
//! breathes on its glowing lens.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::fx::light::{Lamp, lit};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, nest, prim, quat_x, solid, steady,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Generator, Slide, SlideTerm, SovereignMaterialSettings, Spin};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, cladding, tank_steel, timber};

/// How many crates ride the run.
pub(super) const CRATES: usize = 6;
/// The belt's speed (m/s) and the crate interval (s): exact in binary.
pub(super) const SPEED: f32 = 0.625;
pub(super) const PERIOD: f32 = 2.0;
/// The run a crate rides before it jumps back (m), and the spacing of the
/// crates' starts along it: the speed times the interval, exact too.
pub(super) const RUN: f32 = SPEED * PERIOD * CRATES as f32;
pub(super) const SPACING: f32 = RUN / CRATES as f32;
/// Where every crate stands at rest - the start of the run, inside the
/// infeed - and the end of the run, inside the outfeed (m along X).
pub(super) const START_X: f32 = -RUN * 0.5;
/// A crate's size: length along the run, height, width.
pub(super) const CRATE: [f32; 3] = [0.6, 0.45, 0.5];
/// The height the crates ride on: the rollers' tops.
pub(super) const BELT_TOP: f32 = 0.9;
/// The housings: from the outer end to the mouth (m along X, the infeed's;
/// the outfeed's mirrors it), half their width and their height.
pub(super) const HOUSING_OUTER: f32 = -4.6;
pub(super) const HOUSING_MOUTH: f32 = -3.1;
pub(super) const HOUSING_HZ: f32 = 0.55;
pub(super) const HOUSING_H: f32 = 1.7;
/// The rollers: radius, how many, the first one's place and their pitch.
pub(super) const ROLLER_R: f32 = 0.06;
const ROLLERS: usize = 10;
const ROLLER_FIRST_X: f32 = -2.7;
const ROLLER_PITCH: f32 = 0.6;
/// The side rails: their inner faces' half gap and their thickness.
const RAIL_HZ: f32 = 0.36;
const RAIL_T: f32 = 0.06;
/// Where the press stands along the run.
pub(super) const PRESS_X: f32 = 0.0;
/// How far the ram hops up, and how far over a crate's top it strikes.
pub(super) const RAM_HOP: f32 = 0.42;
pub(super) const RAM_GAP: f32 = 0.03;
/// The ram's underside at rest, at the bottom of its stroke: where it
/// strikes, just over a passing crate.
pub(super) const RAM_REST_BOTTOM: f32 = BELT_TOP + CRATE[1] + RAM_GAP;
/// The press head's underside and top.
const HEAD_BOTTOM: f32 = 2.4;
const HEAD_TOP: f32 = 2.9;
/// Spacing circle round the conveyor: its housings' far corners and the
/// flywheel in front.
pub(super) const CLEARANCE: f32 = 4.8;

/// The machines' green paint, the frame's safety yellow, the mouths'
/// rubber and the curtains' strips.
const MACHINE_GREEN: [f32; 3] = [0.22, 0.40, 0.30];
const SAFETY_YELLOW: [f32; 3] = [0.86, 0.68, 0.12];
const MOUTH_BLACK: [f32; 3] = [0.04, 0.04, 0.05];
const STRIP_GREY: [f32; 3] = [0.20, 0.21, 0.22];
/// The crates' looks, in turn: pine, cardboard, oak, a blue tote, pine,
/// cardboard - wood on the even ones, painted on the odd.
const CRATE_LOOKS: [[f32; 3]; CRATES] = [
    [0.66, 0.50, 0.30],
    [0.70, 0.55, 0.36],
    [0.50, 0.36, 0.22],
    [0.18, 0.36, 0.62],
    [0.62, 0.46, 0.26],
    [0.74, 0.60, 0.40],
];

pub struct CrateConveyor;

impl CatalogueEntry for CrateConveyor {
    fn slug(&self) -> &'static str {
        "crate_conveyor"
    }
    fn name(&self) -> &'static str {
        "Crate conveyor"
    }
    fn description(&self) -> &'static str {
        "Roller conveyor between two machines: an endless line of crates rides \
         under a stamping press that comes down on each one's beat."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    /// Placed from the Catalogue alone (#1940): a showpiece of motion, it
    /// changes no seeded world.
    fn seeds_worlds(&self) -> bool {
        false
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The ram's hop phase (degrees, 180 a whole hop) that lands its strike -
/// the hop's sharp turn at its rest, the bottom of the stroke - just as a
/// crate's middle passes under it. A crate's middle is under the ram once
/// it has run `PRESS_X - START_X` along the belt; the starts are an
/// interval apart, so one passes every [`PERIOD`] seconds, the first that
/// run over the speed in. A hop is at its rest a whole period in, less its
/// phase.
pub(super) fn ram_phase_deg() -> f32 {
    let pass = (PRESS_X - START_X) / SPEED;
    let lag = (pass / PERIOD).rem_euclid(1.0);
    Fp((-180.0 * lag).rem_euclid(180.0)).on_wire().0
}

/// The rollers' turn (degrees a second): their surface runs at the belt's
/// speed, on the wire's grid.
pub(super) fn roller_rate_deg() -> f32 {
    Fp((SPEED / ROLLER_R).to_degrees()).on_wire().0
}

/// One housing, its mouth at `mouth` along X and its outer end at `outer`:
/// a green box with a dark mouth in the face toward the run and a curtain
/// of rubber strips across it, standing on the ground. The crates ride
/// into it through the strips, and do their jumping inside.
fn housing(outer: f32, mouth: f32) -> Generator {
    let toward = (mouth - outer).signum();
    let mid = (outer + mouth) * 0.5;
    let body = prim(
        solid(cuboid_tapered(
            [(mouth - outer).abs(), HOUSING_H, HOUSING_HZ * 2.0],
            0.0,
            cladding(MACHINE_GREEN),
        )),
        [mid, HOUSING_H * 0.5, 0.0],
        id_quat(),
    );
    let (mouth_lo, mouth_hi) = (BELT_TOP - 0.04, BELT_TOP + CRATE[1] + 0.13);
    let mut parts = vec![
        // The dark mouth, half sunk in the face.
        prim(
            cuboid_tapered(
                [0.02, mouth_hi - mouth_lo, 0.72],
                0.0,
                tank_steel(MOUTH_BLACK),
            ),
            [mouth, (mouth_lo + mouth_hi) * 0.5, 0.0],
            id_quat(),
        ),
        // A hood over it, standing proud of the face.
        prim(
            solid(cuboid_tapered(
                [0.3, 0.08, 0.9],
                0.0,
                tank_steel(MACHINE_GREEN),
            )),
            [mouth + toward * 0.13, mouth_hi + 0.06, 0.0],
            id_quat(),
        ),
    ];
    // Four strips hung from the hood across the mouth, a hair apart and
    // proud of it, alternate ones a few millimetres prouder so no two
    // neighbours share a face.
    let strip_top = mouth_hi + 0.04;
    for k in 0..4 {
        let z = -0.27 + 0.18 * k as f32;
        parts.push(prim(
            cuboid_tapered(
                [0.012, strip_top - BELT_TOP, 0.17],
                0.0,
                tank_steel(STRIP_GREY),
            ),
            [
                mouth + toward * (0.025 + 0.004 * (k % 2) as f32),
                (strip_top + BELT_TOP) * 0.5,
                z,
            ],
            id_quat(),
        ));
    }
    nest(body, parts)
}

/// Crate `k`, standing at the start of the run inside the infeed and riding
/// the belt from there, its start `k` spacings along.
fn crate_box(k: usize) -> Generator {
    let look = CRATE_LOOKS[k];
    let material = if k.is_multiple_of(2) {
        timber(look)
    } else {
        SovereignMaterialSettings {
            base_color: crate::pds::Fp3(look),
            roughness: Fp(0.95),
            ..Default::default()
        }
    };
    let mut crate_node = prim(
        cuboid_tapered(CRATE, 0.0, material),
        [START_X, BELT_TOP + CRATE[1] * 0.5, 0.0],
        id_quat(),
    );
    crate_node.slide = Some(Slide::along(
        [1.0, 0.0, 0.0],
        SlideTerm::Belt {
            speed: Fp(SPEED),
            length: Fp(RUN),
            start: Fp(SPACING * k as f32),
        },
    ));
    crate_node
}

fn build_tree() -> Generator {
    let frame = tank_steel(SAFETY_YELLOW);
    let steel = tank_steel(PIPE_GREY);

    // ---- the housings, the crates waiting inside the infeed.
    let mut infeed = housing(HOUSING_OUTER, HOUSING_MOUTH);
    for k in 0..CRATES {
        crate::catalogue::items::util::attach(&mut infeed, crate_box(k));
    }
    let outfeed = housing(-HOUSING_OUTER, -HOUSING_MOUTH);

    // ---- the frame: two side rails reaching into both housings, the
    // rollers between them, and legs under them.
    let rail_y = BELT_TOP - ROLLER_R;
    let rail_len = 2.0 * (-HOUSING_MOUTH + 0.1);
    let rail = |z: f32| {
        prim(
            solid(cuboid_tapered([rail_len, 0.16, RAIL_T], 0.0, frame.clone())),
            [0.0, rail_y, z],
            id_quat(),
        )
    };
    let mut rail_parts = Vec::new();
    // The rollers, sunk at their ends into the rails, turning so their
    // tops run with the crates: about -Z, which carries a top along +X.
    for k in 0..ROLLERS {
        let x = ROLLER_FIRST_X + ROLLER_PITCH * k as f32;
        let mut roller = prim(
            cylinder_tapered(
                ROLLER_R,
                2.0 * (RAIL_HZ + RAIL_T * 0.5),
                10,
                0.0,
                steel.clone(),
            ),
            [x, rail_y, 0.0],
            quat_x(FRAC_PI_2),
        );
        // The roller's own +Y is the item's +Z, so -Y is -Z.
        roller.spin = Some(Spin::about(
            [0.0, -1.0, 0.0],
            crate::pds::SpinTerm::Constant {
                rate: Fp(roller_rate_deg()),
            },
        ));
        rail_parts.push(roller);
    }
    let leg_top = rail_y - 0.08 + 0.02;
    for x in [-2.7, -1.5, 1.5, 2.7] {
        for z in [-1.0, 1.0] {
            // A centimetre narrower than the rail across, so the leg's sides
            // stand inside the rail's rather than in their plane.
            rail_parts.push(prim(
                solid(cuboid_tapered(
                    [0.06, leg_top, RAIL_T - 0.01],
                    0.0,
                    frame.clone(),
                )),
                [x, leg_top * 0.5, z * (RAIL_HZ + RAIL_T * 0.5)],
                id_quat(),
            ));
        }
        // A brace across each pair of legs.
        rail_parts.push(prim(
            cuboid_tapered([0.04, 0.04, 2.0 * RAIL_HZ + 0.02], 0.0, frame.clone()),
            [x, 0.28, 0.0],
            id_quat(),
        ));
    }
    let near_rail = nest(rail(-(RAIL_HZ + RAIL_T * 0.5)), rail_parts);
    let far_rail = rail(RAIL_HZ + RAIL_T * 0.5);

    // ---- the press: two columns, a head across them, the ram hanging from
    // it, a bolster under the run, a flywheel and the beacon.
    let ram_h = 0.32;
    let mut ram = prim(
        cuboid_tapered([0.5, ram_h, 0.46], 0.0, tank_steel(STEEL_BLUE)),
        [PRESS_X, RAM_REST_BOTTOM + 0.04 + ram_h * 0.5, 0.0],
        id_quat(),
    );
    ram.slide = Some(Slide::along(
        [0.0, 1.0, 0.0],
        SlideTerm::Bounce {
            height: Fp(RAM_HOP),
            period: Fp(PERIOD),
            phase: Fp(ram_phase_deg()),
        },
    ));
    let rod_bottom = RAM_REST_BOTTOM + 0.04 + ram_h - 0.02;
    let rod_top = HEAD_BOTTOM + 0.06;
    let ram = nest(
        ram,
        vec![
            // The die, whose face is the ram's underside.
            prim(
                cuboid_tapered([0.44, 0.06, 0.4], 0.0, tank_steel(CONCRETE_GREY)),
                [PRESS_X, RAM_REST_BOTTOM + 0.03, 0.0],
                id_quat(),
            ),
            // The piston rod: inside the head at rest, and still under its
            // top at the top of the hop.
            prim(
                cylinder_tapered(0.07, rod_top - rod_bottom, 10, 0.0, steel.clone()),
                [PRESS_X, (rod_top + rod_bottom) * 0.5, 0.0],
                id_quat(),
            ),
        ],
    );
    let flywheel_z = -(1.84 * 0.5) - 0.12;
    let mut flywheel = prim(
        cylinder_tapered(0.42, 0.1, 18, 0.0, tank_steel(STEEL_BLUE)),
        [PRESS_X, 2.65, flywheel_z],
        quat_x(FRAC_PI_2),
    );
    // A turn a stroke; the wheel's own +Y is the item's +Z.
    flywheel.spin = Some(Spin::about([0.0, 1.0, 0.0], steady(360.0 / PERIOD)));
    // A yellow spoke across its face, so the turn reads: authored in the
    // wheel's own frame, whose -Y is the item's -Z (the front) and whose
    // +Z is the item's -Y.
    flywheel.children.push(prim(
        cuboid_tapered([0.72, 0.03, 0.08], 0.0, tank_steel(SAFETY_YELLOW)),
        [0.0, -0.06, 0.0],
        id_quat(),
    ));
    let lens = lit(
        prim(
            cylinder_tapered(0.07, 0.14, 12, 0.0, glow(LAMP_AMBER, 2.4)),
            [PRESS_X + 0.15, HEAD_TOP + 0.14, 0.6],
            id_quat(),
        ),
        Lamp::Beacon { over: 3.0 },
    );
    let head = nest(
        prim(
            solid(cuboid_tapered(
                [0.6, HEAD_TOP - HEAD_BOTTOM, 1.84],
                0.0,
                tank_steel(STEEL_BLUE),
            )),
            [PRESS_X, (HEAD_TOP + HEAD_BOTTOM) * 0.5, 0.0],
            id_quat(),
        ),
        vec![
            ram,
            // The flywheel's axle, out of the head's front.
            prim(
                cylinder_tapered(0.06, 0.24, 10, 0.0, steel.clone()),
                [PRESS_X, 2.65, flywheel_z + 0.04],
                quat_x(FRAC_PI_2),
            ),
            flywheel,
            // The beacon's base, its lens and its cap.
            prim(
                cylinder_tapered(0.09, 0.08, 12, 0.0, tank_steel(MOUTH_BLACK)),
                [PRESS_X + 0.15, HEAD_TOP + 0.03, 0.6],
                id_quat(),
            ),
            lens,
            prim(
                cylinder_tapered(0.075, 0.03, 12, 0.0, tank_steel(MOUTH_BLACK)),
                [PRESS_X + 0.15, HEAD_TOP + 0.22, 0.6],
                id_quat(),
            ),
        ],
    );
    let column = |z: f32| {
        prim(
            solid(cuboid_tapered(
                [0.2, HEAD_TOP - 0.1, 0.2],
                0.0,
                tank_steel(STEEL_BLUE),
            )),
            [PRESS_X, (HEAD_TOP - 0.1) * 0.5, z],
            id_quat(),
        )
    };
    let press = nest(
        column(-0.72),
        vec![
            column(0.72),
            head,
            // The bolster the dies meet over, under the run's rollers: its
            // ends inside the columns' outer faces, its underside a hair off
            // the ground they stand on.
            prim(
                solid(cuboid_tapered(
                    [0.5, 0.34, 1.6],
                    0.0,
                    tank_steel(STEEL_BLUE),
                )),
                [PRESS_X, 0.19, 0.0],
                id_quat(),
            ),
        ],
    );

    assemble(vec![infeed, outfeed, near_rail, far_rail, press])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure::Bounds;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, motion_reach, placed, placed_at};
    use crate::pds::{SpinTerm, sanitize_generator};

    fn built() -> Generator {
        CrateConveyor.build("")
    }

    /// The tree as a peer draws it: sanitised and through the wire.
    fn on_the_wire() -> Generator {
        let mut tree = built();
        sanitize_generator(&mut tree);
        serde_json::from_str(&serde_json::to_string(&tree).expect("writes")).expect("reads")
    }

    /// Each crate's node path, by its belt's start.
    fn crate_paths(tree: &Generator) -> Vec<(f32, Vec<usize>)> {
        let mut found: Vec<_> = placed(tree)
            .into_iter()
            .filter_map(|p| match p.node.slide.as_ref()?.terms[0] {
                SlideTerm::Belt { start, .. } => Some((start.0, p.path)),
                _ => None,
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found
    }

    /// The path of the ram, the one part that hops.
    fn ram_path(tree: &Generator) -> Vec<usize> {
        placed(tree)
            .into_iter()
            .find(|p| {
                p.node
                    .slide
                    .as_ref()
                    .is_some_and(|s| matches!(s.terms[0], SlideTerm::Bounce { .. }))
            })
            .expect("a ram")
            .path
    }

    fn drawn_at(tree: &Generator, path: &[usize], now: f64, calm: f32) -> Bounds {
        placed_at(tree, now, calm)
            .into_iter()
            .find(|p| p.path == path)
            .and_then(|p| p.drawn)
            .expect("drawn")
    }

    /// The lowest the ram reaches under `path` (the ram and its die) at
    /// `now`.
    fn ram_bottom(tree: &Generator, ram: &[usize], now: f64, calm: f32) -> f32 {
        placed_at(tree, now, calm)
            .into_iter()
            .filter(|p| p.path.starts_with(ram))
            .filter_map(|p| p.drawn)
            .map(|b| b.min.y)
            .fold(f32::INFINITY, f32::min)
    }

    /// The infeed's and the outfeed's bodies, as drawn.
    fn housings(tree: &Generator) -> (Bounds, Bounds) {
        let at = |x: f32| {
            placed(tree)
                .into_iter()
                .filter(|p| p.node.children.len() >= 6)
                .filter_map(|p| p.drawn)
                .find(|b| b.contains(bevy::prelude::Vec3::new(x, 1.0, 0.0), 0.0))
                .expect("a housing")
        };
        (at(-4.0), at(4.0))
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "crate_conveyor");
    }

    #[test]
    fn nothing_floats_and_nothing_z_fights() {
        let tree = built();
        assert_nothing_floats(&tree, "crate_conveyor");
        assert_no_z_fighting(&tree, "crate_conveyor");
    }

    /// The beat's numbers are exact in binary and fit one another: the run
    /// is the speed times the interval times the crates, so the crates and
    /// the press never drift apart.
    #[test]
    fn the_beat_is_exact() {
        assert_eq!(SPEED, 5.0 / 8.0);
        assert_eq!(PERIOD, 2.0);
        assert_eq!(RUN, 15.0 / 2.0);
        assert_eq!(
            f64::from(RUN),
            f64::from(SPEED) * f64::from(PERIOD) * CRATES as f64
        );
        // On the wire's grid as well, so a peer reads back the same beat.
        for v in [SPEED, PERIOD, RUN, SPACING, START_X, ram_phase_deg()] {
            assert_eq!(Fp(v).on_wire().0, v);
        }
    }

    /// At any moment the six crates stand a spacing apart round the run,
    /// as one even stream: their offsets, sorted, step by exactly a
    /// spacing, the last back round to the first.
    #[test]
    fn the_crates_ride_evenly_spaced() {
        let tree = on_the_wire();
        let crates = crate_paths(&tree);
        assert_eq!(crates.len(), CRATES);
        for (k, (start, _)) in crates.iter().enumerate() {
            assert_eq!(*start, SPACING * k as f32);
        }
        for now in [0.0, 0.73, 3.1, 11.9, 1.8e9 + 0.41] {
            let mut along: Vec<f32> = crates
                .iter()
                .map(|(_, path)| drawn_at(&tree, path, now, 1.0).center().x - START_X)
                .collect();
            along.sort_by(f32::total_cmp);
            for pair in along.windows(2) {
                assert!(
                    (pair[1] - pair[0] - SPACING).abs() < 1e-3,
                    "at {now} s: {along:?}"
                );
            }
            let round = along[0] + RUN - along[CRATES - 1];
            assert!((round - SPACING).abs() < 1e-3, "at {now} s: {along:?}");
        }
    }

    /// Each crate jumps back from inside the outfeed to inside the
    /// infeed: just before its jump it is wholly inside the outfeed's body,
    /// behind its mouth, and just after it wholly inside the infeed's.
    #[test]
    fn every_crate_jumps_back_inside_the_housings() {
        let tree = on_the_wire();
        let (infeed, outfeed) = housings(&tree);
        let inside = |b: Bounds, h: Bounds| {
            b.min.x > h.min.x
                && b.max.x < h.max.x
                && b.min.z > h.min.z
                && b.max.z < h.max.z
                && b.max.y < h.max.y
        };
        for (start, path) in crate_paths(&tree) {
            // The belt wraps when speed x t + start reaches a whole run.
            for lap in [1.0, 7.0, 1.44e8] {
                let jump = (lap * f64::from(RUN) - f64::from(start)) / f64::from(SPEED);
                let before = drawn_at(&tree, &path, jump - 0.01, 1.0);
                let after = drawn_at(&tree, &path, jump + 0.01, 1.0);
                assert!(
                    inside(before, outfeed),
                    "the crate starting {start} m along leaves from {before:?}, outside the outfeed"
                );
                assert!(
                    inside(after, infeed),
                    "the crate starting {start} m along comes back at {after:?}, outside the infeed"
                );
                // Inside the mouth by the crate's own length and more, so no
                // eye looking in through the curtain sees it go.
                assert!(before.min.x > -HOUSING_MOUTH + 0.3, "{before:?}");
                assert!(after.max.x < HOUSING_MOUTH - 0.3, "{after:?}");
            }
        }
    }

    /// The ram strikes - reaches its lowest, at its rest - just as a crate's
    /// middle passes under the press, every beat and at the wall clock's
    /// seconds too, a few centimetres over that crate's top; it rises from
    /// there, and over the whole run it never comes down into any crate, at
    /// either Effects setting that moves it.
    #[test]
    fn the_press_comes_down_on_each_crate_and_clear_of_it() {
        let tree = on_the_wire();
        let crates = crate_paths(&tree);
        let ram = ram_path(&tree);
        let phase = f64::from(ram_phase_deg()) / 180.0;
        let crate_top = BELT_TOP + CRATE[1];
        for beat in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 7.2e8] {
            // A hop is at its rest, the bottom of the stroke, a whole period
            // in, less its phase.
            let lowest = f64::from(PERIOD) * (beat + 1.0 - phase);
            let bottom = ram_bottom(&tree, &ram, lowest, 1.0);
            let under: Vec<Bounds> = crates
                .iter()
                .map(|(_, path)| drawn_at(&tree, path, lowest, 1.0))
                .filter(|b| (b.center().x - PRESS_X).abs() < 1e-3)
                .collect();
            assert_eq!(under.len(), 1, "beat {beat}: a crate under the press");
            let gap = bottom - under[0].max.y;
            assert!(
                (gap - RAM_GAP).abs() < 1e-3,
                "beat {beat}: the ram stops {gap} m over the crate"
            );
            // And it is the lowest: a hair either side it is higher.
            for d in [-0.05, 0.05] {
                assert!(ram_bottom(&tree, &ram, lowest + d, 1.0) > bottom);
            }
            // The strike is the hop's sharp turn: a twentieth of a second
            // after it the ram has already climbed some 3 cm, where a ram
            // easing round a smooth bottom would have risen a millimetre.
            let climbed = ram_bottom(&tree, &ram, lowest + 0.05, 1.0) - bottom;
            assert!(
                climbed > 0.02,
                "beat {beat}: 0.05 s after its strike the ram has climbed {climbed} m"
            );
        }
        for calm in [1.0, crate::world_builder::spin::REDUCED_MOTION_SHARE] {
            for k in 0..400 {
                let now = f64::from(k) * f64::from(PERIOD) * CRATES as f64 / 400.0;
                let bottom = ram_bottom(&tree, &ram, now, calm);
                for (_, path) in &crates {
                    let b = drawn_at(&tree, path, now, calm);
                    if (b.center().x - PRESS_X).abs() < 0.6 {
                        assert!(
                            bottom > b.max.y + 0.02,
                            "at {now} s (calm {calm}) the ram comes down to {bottom} over a crate topped at {}",
                            b.max.y
                        );
                    }
                }
                assert!(bottom >= crate_top + RAM_GAP - 1e-4);
            }
        }
    }

    /// The rollers' surfaces run at the belt's speed, the way the crates
    /// ride, and the flywheel turns once a stroke.
    #[test]
    fn the_rollers_run_with_the_belt() {
        let tree = on_the_wire();
        let mut rollers = 0;
        for p in placed(&tree) {
            let Some(spin) = &p.node.spin else { continue };
            let SpinTerm::Constant { rate } = spin.terms[0] else {
                panic!("a steady turn");
            };
            let axis = p.toward(spin.axis.0);
            if (rate.0 - 360.0 / PERIOD).abs() < 1e-3 {
                assert!(
                    (axis.z.abs() - 1.0).abs() < 1e-4,
                    "the flywheel faces the front"
                );
                continue;
            }
            rollers += 1;
            // A turn about -Z carries a roller's top along +X.
            assert!((axis.z + 1.0).abs() < 1e-4, "a roller turns about {axis}");
            let surface = rate.0.to_radians() * ROLLER_R;
            assert!(
                (surface - SPEED).abs() < 1e-5,
                "a roller's surface runs {surface} m/s"
            );
        }
        assert_eq!(rollers, ROLLERS);
    }

    /// Everything the conveyor draws, over a whole cycle of its crates,
    /// stays inside its footprint.
    #[test]
    fn the_footprint_covers_the_line_as_it_runs() {
        let times: Vec<f64> = (0..50).map(|k| f64::from(k) * 0.25).collect();
        let reach = motion_reach(&on_the_wire(), &times);
        assert!(reach <= CLEARANCE, "the line reaches {reach} m");
    }
}
