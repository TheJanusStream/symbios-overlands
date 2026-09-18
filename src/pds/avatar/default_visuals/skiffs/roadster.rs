//! The roadster: an open two-seat sports car on a boat-tailed body, and the
//! skiff family's universal floor.
//!
//! Every dimension here is a fraction of the seeded machine, read off
//! [`BodyPlan`], so a 1.9 m seed and a 3.6 m one are the same car at two sizes
//! rather than two different mistakes. The one exception is the one that
//! *cannot* scale: the sanitiser's dimension floor ([`super::MIN_DIM`]).
//!
//! The shape was agreed by render before any of this was written (#1359 rules
//! 1, 12 and 14): the prototype is `target/dump/vehicles2026-09/roadster2.py`,
//! judged at the chase camera's range on `render --play-view` and then on
//! zoomed sheets at two elevations.
//!
//! # Why it is built the way it is
//!
//! Three swept runs of ONE plan share one section scale, so their sections
//! meet flush: a full barrel for the bonnet, a **bored lower half-pipe** for
//! the tub - whose cut rim is the cockpit coaming and whose bore is the
//! footwell - and upper half-pipes for the scuttle and the tail deck, which
//! stop at the cockpit's ends so the opening between them is simply the gap.
//! "Land-skiff" is the right word for it: boat-built torpedo bodies were a
//! real coachbuilding style, and this is one.
//!
//! Nothing here is a BlobGroup (owner decision 4): a blob reads as organic and
//! this is a machine.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{ParticleAura, SkiffBlueprint};

use super::super::common::{
    bevel, cuboid, id_quat, lathe, prim, quat_x, quat_xyzw, quat_z, spine, with_cut,
};
use super::plan::{Axle, BodyPlan, Layout};
use super::{SkiffColours, SkiffCraft, SkiffFeel, dim, skiff_colours};

/// Section depth per unit half-width - the ONE node scale every body sweep
/// shares, and the knob that turns the plan form below into a body. 0.80 makes
/// a section wider than it is deep, which is what a car's is.
const SECTION: f32 = 0.80;

/// The roadster's plan form: `(z fraction of the overall length, half-width
/// fraction)` from tail to nose. Nine stations, well inside the sanitiser's
/// sixteen. Maximum beam just abaft the cockpit, a boat tail running out to a
/// point, and a bonnet that narrows steadily to the radiator.
const STATIONS: &[(f32, f32)] = &[
    (-0.500, 0.09),
    (-0.452, 0.34),
    (-0.352, 0.70),
    (-0.230, 0.94),
    (-0.074, 1.00),
    (0.082, 0.97),
    (0.204, 0.91),
    (0.333, 0.80),
    (0.422, 0.70),
];

/// Where the roadster puts the stations every land craft has.
static LAYOUT: Layout = Layout {
    cowl: 0.082,
    cockpit: (-0.320, -0.040),
    seat: -0.250,
    radiator: 0.437,
    bumper: 0.492,
    axles: &[
        Axle {
            at: 1.0,
            paired: true,
        },
        Axle {
            at: -1.0,
            paired: true,
        },
    ],
};

/// How deep the tub is bored.
///
/// `hollow` leaves a wall of `(1 - hollow)` of the radius, so this is the
/// cockpit floor as a fraction of the section: 0.74 gives a coaming rim a hand
/// wide and a footwell a hand deep. The feasibility probe's 0.14 left the wall
/// 0.86 of the radius - a solid tub with a drain hole through it, and a seat
/// standing on a lid rather than sitting in a car (#1364).
const TUB_HOLLOW: f32 = 0.74;

/// The wings' node scale on x. A flattened tube is what makes a mudguard; 3.4
/// is the brief's, and with no root scale on a craft authored in true metres
/// the scale PRODUCT down this path is 3.4 against the sanitiser's 4.0.
const WING_SCALE_X: f32 = 3.4;
/// Guard tube radius, as a fraction of the length - so the guard is
/// `WING_SCALE_X` times this across and twice it thick.
const WING_R: f32 = 0.0113;
/// The guard's arc radius over the tyre's.
const WING_ARC: f32 = 1.16;

/// Axle tube radius, and how far under the body's own sill the beam dips at
/// the centreline, both as fractions of the length.
///
/// The dip is period-correct - a dropped front beam and a live rear axle both
/// hang below the frame - and it is also the only reason either can be SEEN:
/// at hub height the elliptical body has already closed in to a third of its
/// beam, so a straight beam is swallowed by the coachwork it passes under.
const AXLE_R: f32 = 0.0125;
const AXLE_DROP: f32 = 0.011;

pub(super) struct Roadster;

impl SkiffCraft for Roadster {
    fn plan(&self, bp: &SkiffBlueprint) -> BodyPlan {
        BodyPlan::new(bp, SECTION, STATIONS, &LAYOUT)
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        let c = skiff_colours(ctx);
        // Root: a hidden hub on the datum amidships, buried under the scuttle.
        // The body cannot BE the structural root - an elliptical section needs
        // a per-axis node scale and a structural root may not carry one (#798)
        // - and the datum is the one plane where a hub is inside the bodywork
        // whatever the seed, which is where the legacy skiff's exposed core
        // becomes impossible rather than merely fixed.
        let hub = dim(plan.length * 0.008);
        let mut root = prim(
            cuboid([hub; 3], c.machinery.clone()),
            [0.0, 0.0, 0.0],
            id_quat(),
        );
        let kids = &mut root.children;
        body(kids, plan, &c);
        radiator(kids, plan, &c);
        wings(kids, plan, &c);
        running_gear(kids, plan, &c);
        wheels(kids, plan, &c);
        lamps_and_bumper(kids, plan, &c);
        cockpit(kids, plan, &c);
        root
    }

    fn feel(&self) -> SkiffFeel {
        // The retired default chassis's numbers exactly. The chassis *classes*
        // carried the feel before #1364 and the roadster is what the default
        // chassis was, so the drive the owner validated with the scale bridge
        // (#1361) is the drive that ships. Per-type feel is #1381's slice.
        SkiffFeel {
            mass_factor: 1.0,
            drive_accel: 8.9,
            turn_accel: 2.0,
        }
    }

    fn overall_width(&self, plan: &BodyPlan) -> f32 {
        plan.drawn_width(WING_R * WING_SCALE_X * plan.length)
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan) -> [f32; 3] {
        match aura {
            // Exhaust and steam leave the pipe mouth, read off the body's own
            // flank - so the wisp leaves the pipe that is actually drawn
            // rather than a fraction of a nominal machine (#1364 item 8).
            ParticleAura::Exhaust | ParticleAura::Steam => {
                let p = exhaust_path(plan);
                p[p.len() - 1].0
            }
            // Anything else is a flourish, and it belongs over the COCKPIT -
            // the seat the body publishes for a hood or a hardtop (#1364 item
            // 1) is exactly the station a decorative aura should hover on,
            // because it is where a person would be.
            _ => {
                let [x, _, z] = plan.canopy_seat();
                [x, plan.crown_at(z) * 1.5, z]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The one sweep helper
// ---------------------------------------------------------------------------

/// A swept shape whose section is shaped by its own node scale - a body run, a
/// flattened guard.
///
/// **The path is pre-divided by that scale here, and that is the trap this
/// helper exists to close.** A node scale moves its path as well as its
/// profile, so a station written straight into the path is drawn displaced by
/// exactly the factor that makes the shape the shape. It cost the boat two
/// silent defects before a `debug_assert` caught it (#1363), and the guards
/// here would be the next: they carry a 3.4x scale on x and their arcs are
/// written in true metres off the wheel landmarks.
///
/// `cut` is the swept profile's kept fraction: `[0.5, 1.0]` is the lower half
/// (and its flat cut face is the coaming), `[0.0, 0.5]` the upper half (a
/// deck), `[0.0, 1.0]` the whole barrel. `hollow` bores it.
fn sweep(
    points: &[([f32; 3], f32)],
    resolution: u32,
    scale: [f32; 3],
    cut: [f32; 2],
    hollow: f32,
    material: SovereignMaterialSettings,
) -> Generator {
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&([x, y, z], r)| ([x / scale[0], y / scale[1], z / scale[2]], dim(r)))
        .collect();
    let mut node = prim(
        with_cut(spine(&path, resolution, material), cut, [0.0, 1.0], hollow),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    node.transform.scale = Fp3(scale);
    node
}

/// A turned part laid on an axis: the family's only other idiom.
fn turned(
    profile: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    material: SovereignMaterialSettings,
    at: [f32; 3],
    rotation: [f32; 4],
) -> Generator {
    prim(
        lathe(profile, resolution, smooth, material),
        at,
        quat_xyzw(rotation),
    )
}

/// A thin swept line - a bead, a rubbing strip, a rod.
fn line(points: &[([f32; 3], f32)], resolution: u32, m: SovereignMaterialSettings) -> Generator {
    let pts: Vec<([f32; 3], f32)> = points.iter().map(|&(p, r)| (p, dim(r))).collect();
    prim(spine(&pts, resolution, m), [0.0; 3], id_quat())
}

// ---------------------------------------------------------------------------
// Bodywork
// ---------------------------------------------------------------------------

/// Bonnet, tub, scuttle, tail deck - and the three lines that run along them.
fn body(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let s = [1.0, SECTION, 1.0];
    let (cockpit_aft, cockpit_fwd) = plan.cockpit_z();
    // Bonnet: a full swept barrel from the cowl forward, tapering to the
    // radiator. No path cut - a bonnet is closed all round.
    kids.push(sweep(
        &plan.run(plan.cowl_z(), plan.nose_z()),
        26,
        s,
        [0.0, 1.0],
        0.0,
        c.paint.clone(),
    ));
    // Tub: the LOWER half-pipe, bored. The cut rim IS the coaming and the bore
    // IS the footwell, so the cockpit is a real hollow rather than a seat
    // standing on a lid - and it costs nothing, where the sloop had to plug
    // her well with a solid because a sweep cannot be holed.
    kids.push(sweep(
        &plan.run(plan.tail_z(), plan.cowl_z()),
        26,
        s,
        [0.5, 1.0],
        TUB_HOLLOW,
        c.paint.clone(),
    ));
    // Scuttle ahead of the cockpit and tail deck behind it: UPPER half-pipes
    // over the SAME stations, so the decks meet the tub's rim flush.
    kids.push(sweep(
        &plan.run(cockpit_fwd, plan.cowl_z()),
        26,
        s,
        [0.0, 0.5],
        0.0,
        c.paint.clone(),
    ));
    kids.push(sweep(
        &plan.run(plan.tail_z(), cockpit_aft),
        26,
        s,
        [0.0, 0.5],
        0.0,
        c.paint.clone(),
    ));
    // Bonnet hinge bead along the bonnet's own crown, from the cowl to the
    // radiator - read off the plan, so it cannot leave the panel it is a seam
    // in.
    let bead: Vec<([f32; 3], f32)> = [0.082f32, 0.200, 0.320, 0.422]
        .iter()
        .map(|&zf| {
            let z = zf * l;
            ([0.0, plan.crown_at(z) + l * 0.0015, z], l * 0.0042)
        })
        .collect();
    kids.push(line(&bead, 6, c.brightwork.clone()));
    // Louvres: five pressed slots down each bonnet flank, seated ON the skin.
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let z = (0.160 + k as f32 * 0.058) * l;
            let y = plan.crown_at(z) * 0.18;
            kids.push(prim(
                cuboid(
                    [dim(l * 0.0045), dim(l * 0.040), dim(l * 0.013)],
                    c.machinery.clone(),
                ),
                [side * plan.side_at(z, y), y, z],
                id_quat(),
            ));
        }
    }
    // The COACHLINE along each flank at the coaming line - the machine's one
    // trim line and the first place the seed's own colour lands (#1365), with
    // its centre ON the skin the way a boot stripe is, because a tube tangent
    // to what it lies on stipples against it. On a period car this line is
    // painted by hand rather than plated, which is why it carries a colour at
    // all instead of being another length of brightwork.
    for side in [-1.0f32, 1.0] {
        let strip: Vec<([f32; 3], f32)> = [-0.400f32, -0.200, 0.0, 0.200, 0.400]
            .iter()
            .map(|&zf| {
                let z = zf * l;
                ([side * plan.side_at(z, 0.0), 0.0, z], l * 0.0080)
            })
            .collect();
        kids.push(line(&strip, 6, c.coachline.clone()));
    }
}

/// A bevelled pressed shell, a dark matrix and a turned filler cap.
///
/// A [`Bevel`](crate::pds::generator::GeneratorKind::Bevel)'s rounded edges run
/// parallel to its Y extrusion axis, so the size is `[width, depth, height]`
/// and the shell takes a quarter turn about x; authored the other way it lies
/// flat across the nose like a tray (found by render, #1359).
fn radiator(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let (nose, z) = (plan.nose_z(), plan.radiator_z());
    let (w, h) = (plan.half_width_at(nose) * 1.95, plan.crown_at(nose) * 2.30);
    // Its centre sits low enough that the shell stands proud UNDER the bonnet
    // line, which is where a radiator of this era is.
    let y = plan.crown_at(nose) - h * 0.5 + l * 0.0075;
    let turn = quat_x(FRAC_PI_2);
    kids.push(prim(
        bevel(
            [dim(w), dim(l * 0.034), dim(h)],
            l * 0.038,
            4,
            c.brightwork.clone(),
        ),
        [0.0, y, z],
        quat_xyzw(turn),
    ));
    kids.push(prim(
        bevel(
            [dim(w * 0.68), dim(l * 0.038), dim(h * 0.66)],
            l * 0.022,
            4,
            c.machinery.clone(),
        ),
        [0.0, y, z + l * 0.0015],
        quat_xyzw(turn),
    ));
    let cap: Vec<(f32, f32)> = [
        (0.0f32, 0.0f32),
        (0.0111, 0.0),
        (0.0111, 0.0074),
        (0.0067, 0.0111),
        (0.0067, 0.0185),
        (0.0, 0.0204),
    ]
    .iter()
    .map(|&(r, hh)| (r * l, hh * l))
    .collect();
    kids.push(turned(
        &cap,
        12,
        false,
        c.brightwork.clone(),
        [0.0, y + h * 0.5, z],
        [0.0, 0.0, 0.0, 1.0],
    ));
}

/// One flattened swept guard a side, and the board that ties it to the body.
///
/// Front arch, running board, rear arch, swept from the WHEEL LANDMARKS, so a
/// guard is over its own wheel by construction - and a type with three or six
/// wheels gets three or six arches out of this same code rather than out of a
/// slug-string check on its chassis.
fn wings(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let arc = plan.wheel_r * WING_ARC;
    let (axle_y, half_track) = (plan.axle_y(), plan.track * 0.5);
    let board_y = axle_y - l * 0.0113;
    let deck_y = board_y + WING_R * l + l * 0.0023;
    for side in [-1.0f32, 1.0] {
        let x = side * half_track;
        // One guard per AXLE, with a board between consecutive ones. Written
        // over the axle list rather than over "front and rear", so the three-,
        // two- and six-wheeled types coming in #1374-#1378 get their guards
        // from this same sweep instead of from a slug-string check.
        let stations = plan.axle_stations();
        let mut pts: Vec<([f32; 3], f32)> = Vec::with_capacity(stations.len() * 7);
        for (i, &centre) in stations.iter().enumerate() {
            if i > 0 {
                let previous = stations[i - 1];
                pts.push(([x, board_y, previous - arc - 0.10 * l], WING_R * l));
                pts.push(([x, board_y, centre + arc + 0.10 * l], WING_R * l));
            }
            // A leading half-arc on the first guard and a trailing one on the
            // last, so the ends run out past their wheels the way a wing does;
            // the ones between are the arch alone.
            let lead = if i == 0 { 196.0f32 } else { 136.0 };
            let tail = if i + 1 == stations.len() {
                -14.0f32
            } else {
                44.0
            };
            for k in 0..5 {
                let d = lead + (tail - lead) * k as f32 / 4.0;
                let a = d.to_radians();
                pts.push((
                    [x, axle_y + arc * a.sin(), centre - arc * a.cos()],
                    WING_R * l,
                ));
            }
        }
        kids.push(sweep(
            &pts,
            10,
            [WING_SCALE_X, 1.0, 1.0],
            [0.0, 1.0],
            0.0,
            c.guard.clone(),
        ));
        // The running board BRIDGES: its inboard edge is inside the body's own
        // flank at board height and its outboard edge is the guard's. Perched
        // on the guard's waist instead - which is where the prototype first
        // put it - it leaves a quarter of a metre of daylight between guard
        // and coachwork, and that gap was most of what read as "the parts are
        // not connected" (#1364).
        let inner = plan.side_at(0.0, deck_y) * 0.78;
        let outer = half_track + WING_R * WING_SCALE_X * l;
        let span = stations[0] - stations[stations.len() - 1];
        kids.push(prim(
            bevel(
                [
                    dim(outer - inner),
                    dim(l * 0.014),
                    dim(span - 2.0 * arc - 0.18 * l),
                ],
                l * 0.007,
                4,
                c.machinery.clone(),
            ),
            [
                side * (inner + outer) * 0.5,
                deck_y,
                (stations[0] + stations[stations.len() - 1]) * 0.5,
            ],
            id_quat(),
        ));
    }
}

/// The axles, the track rod and the torque tube.
///
/// Without these the wheels stand BESIDE the machine with nothing joining them
/// to it, which is the one thing a swept-and-turned craft cannot get away
/// with: every other part of this body is read off the plan and therefore
/// touches its neighbour, and four wheels in mid-air undo all of it. It could
/// not be caught by eye either, because the chase camera looks down and
/// nothing under a car is ever in frame at play distance - see
/// `common::touch`, the test-only helper that catches it by arithmetic.
fn running_gear(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let (axle_y, half_track) = (plan.axle_y(), plan.track * 0.5);
    let r = AXLE_R * l;
    let stations = plan.axle_stations();
    let (front, rear) = (stations[0], stations[stations.len() - 1]);

    // A dropped front beam, hub to hub.
    let fy = plan.sill_at(front) - AXLE_DROP * l;
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
            ([0.0, fy + AXLE_DROP * l * 0.4, front + l * 0.052], r * 0.42),
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

/// Wheels, TURNED.
///
/// A tyre is a smooth Lathe ring and the disc and its proud cap a second one.
/// **A Lathe whose profile ends at a non-zero radius is closed with a full
/// disc**, so the tyre is really a drum: the wheel disc has to stand PROUD of
/// the tyre's end plane or it is swallowed, and that is arranged here rather
/// than left to luck - the disc's outer radius is the tyre's own lip times
/// 1.06. The INBOARD end is the same trap mirrored: untreated, the far wheels
/// present a matte black plate that at play distance reads as a hole punched
/// through the car, so each carries a brake drum too.
fn wheels(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let w = wheel_profiles(plan.wheel_r);
    for anchor in plan.wheel_anchors() {
        // Lay the wheel on its axle with the cap facing outboard on BOTH sides.
        let lay = quat_z(-anchor[0].signum() * FRAC_PI_2);
        kids.push(turned(&w.tyre, 28, true, c.rubber.clone(), anchor, lay));
        kids.push(turned(&w.disc, 28, false, c.disc.clone(), anchor, lay));
        kids.push(turned(&w.drum, 20, false, c.machinery.clone(), anchor, lay));
    }
}

/// The three turned silhouettes a wheel is made of.
struct WheelProfiles {
    tyre: Vec<(f32, f32)>,
    disc: Vec<(f32, f32)>,
    /// The inboard face. See [`wheels`].
    drum: Vec<(f32, f32)>,
}

/// The silhouettes for a wheel of this radius.
fn wheel_profiles(wheel_r: f32) -> WheelProfiles {
    let lip = wheel_r * 0.683;
    let w = wheel_r * 0.193;
    let tyre = vec![
        (lip, -w * 0.86),
        (wheel_r * 0.873, -w),
        (wheel_r * 0.977, -w * 0.62),
        (wheel_r, 0.0),
        (wheel_r * 0.977, w * 0.62),
        (wheel_r * 0.873, w),
        (lip, w * 0.86),
    ];
    let disc = vec![
        (0.0, -w * 0.78),
        (lip * 1.034, -w * 0.78),
        (lip * 1.063, -w * 0.52),
        (lip * 1.063, w * 0.52),
        (lip * 1.000, w * 0.72),
        (lip * 0.732, w * 0.86),
        (lip * 0.366, w * 1.07),
        (lip * 0.293, w * 1.55),
        (0.0, w * 1.69),
    ];
    // Bottom to top, so it faces the other way: the drum is the inboard face.
    let drum = vec![
        (0.0, -w * 0.16),
        (lip * 0.52, -w * 0.12),
        (lip * 0.80, w * 0.02),
        (lip * 0.78, w * 0.30),
        (0.0, w * 0.30),
    ];
    WheelProfiles { tyre, disc, drum }
}

/// Turned bullet lamp shells with a lit lens, tail lamps, and a bumper.
fn lamps_and_bumper(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let k = l / 2.7;
    let shell: Vec<(f32, f32)> = [
        (0.0f32, -0.130f32),
        (0.045, -0.100),
        (0.085, -0.040),
        (0.098, 0.020),
        (0.098, 0.050),
        (0.088, 0.058),
    ]
    .iter()
    .map(|&(r, h)| (r * k, h * k))
    .collect();
    let turn = quat_x(FRAC_PI_2);
    let mounts: Vec<[f32; 3]> = [-1.0f32, 1.0]
        .iter()
        .map(|&s| lamp_mount(plan, s))
        .collect();
    // The crossbar runs lamp to lamp THROUGH the bonnet, so each shell is
    // carried rather than stuck on the side of the panel.
    kids.push(line(
        &[(mounts[0], l * 0.0048), (mounts[1], l * 0.0048)],
        6,
        c.brightwork.clone(),
    ));
    for (i, &m) in mounts.iter().enumerate() {
        let side = if i == 0 { -1.0f32 } else { 1.0 };
        kids.push(turned(&shell, 20, true, c.brightwork.clone(), m, turn));
        kids.push(turned(
            &[(0.0, 0.0), (0.084 * k, 0.0), (0.084 * k, 0.010 * k)],
            18,
            false,
            c.lamp.clone(),
            [m[0], m[1], m[2] + 0.058 * k],
            turn,
        ));
        // Tail lamp on the quarter, seated on the body's own flank.
        let tz = -0.448 * l;
        let ty = plan.crown_at(tz) * 0.10;
        kids.push(turned(
            &[(0.0, 0.0), (0.034 * k, 0.0), (0.034 * k, 0.012 * k)],
            12,
            false,
            c.tail_lamp.clone(),
            [side * plan.side_at(tz, ty) * 1.02, ty, tz - l * 0.004],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Bumper: a bowed bar on two short irons, ahead of the radiator and INSIDE
    // the guard line - a bar as wide as the track reads as a chrome moustache
    // at 109 px/m, whatever it is in life.
    let (bz, by) = (plan.bumper_z(), plan.sill_at(0.0) * 0.50);
    let bx = plan.half_width_at(plan.nose_z()) * 1.30;
    kids.push(line(
        &[
            ([-bx, by, bz - l * 0.022], l * 0.0090),
            ([-bx * 0.45, by, bz], l * 0.0090),
            ([bx * 0.45, by, bz], l * 0.0090),
            ([bx, by, bz - l * 0.022], l * 0.0090),
        ],
        8,
        c.brightwork.clone(),
    ));
    for side in [-1.0f32, 1.0] {
        kids.push(line(
            &[
                ([side * bx * 0.42, by, bz - l * 0.0015], l * 0.0055),
                (
                    [
                        side * bx * 0.42,
                        by + l * 0.030,
                        plan.radiator_z() - l * 0.010,
                    ],
                    l * 0.0055,
                ),
            ],
            6,
            c.brightwork.clone(),
        ));
    }
    // Side exhaust, out of the bonnet flank and along the sill.
    let pipe: Vec<([f32; 3], f32)> = exhaust_path(plan)
        .iter()
        .map(|&(p, _)| (p, l * 0.0105))
        .collect();
    kids.push(line(&pipe, 10, c.brightwork.clone()));
}

/// The headlamp station on `side` - standing off the bonnet's shoulder, read
/// off the flank so the shell is always against the panel.
fn lamp_mount(plan: &BodyPlan, side: f32) -> [f32; 3] {
    let z = 0.400 * plan.length;
    let y = plan.crown_at(z) * 0.30;
    [side * plan.side_at(z, y) * 1.12, y + plan.length * 0.017, z]
}

/// The side exhaust's stations, tail last, read off the body's OWN flank at
/// every one.
///
/// A fixed height would leave the pipe hanging in air aft, where the section
/// is a third of what it is amidships - the same mistake as a mount on a
/// guessed fraction, one part further out.
fn exhaust_path(plan: &BodyPlan) -> Vec<([f32; 3], f32)> {
    let l = plan.length;
    [0.300f32, 0.180, 0.0, -0.250, -0.392]
        .iter()
        .enumerate()
        .map(|(i, &zf)| {
            let z = zf * l;
            let y = plan.sill_at(z) * if i == 0 { 0.30 } else { 0.66 };
            ([plan.side_at(z, y) + l * 0.006, y, z], l * 0.0105)
        })
        .collect()
}

/// The windscreen frame, the seat, the wheel and the spare.
fn cockpit(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    // The screen stands at the cockpit's FORWARD lip, not at the cowl: on the
    // cowl it leaves half a metre of bare scuttle behind it and reads as a
    // hoop planted in the bonnet.
    let fz = plan.cockpit_z().1;
    let foot = plan.crown_at(fz) * 0.55;
    let post = plan.side_at(fz, foot) * 0.86;
    let (rake, top, r) = (l * 0.042, plan.screen_top(), l * 0.0105);
    // A CLOSED loop - post, header, post, sill rail - because an open U of
    // thin chrome reads as a roll hoop rather than as a screen frame. NO glass
    // in it: `SovereignMaterialSettings` has no alpha, so a pane renders as a
    // dark crate (#1359 rule 4).
    kids.push(line(
        &[
            ([-post, foot, fz + l * 0.004], r),
            ([-post * 0.97, top, fz - rake], r),
            ([0.0, top + l * 0.005, fz - rake * 1.10], r),
            ([post * 0.97, top, fz - rake], r),
            ([post, foot, fz + l * 0.004], r),
            ([0.0, foot - l * 0.004, fz + l * 0.010], r),
            ([-post, foot, fz + l * 0.004], r),
        ],
        6,
        c.brightwork.clone(),
    ));
    // Seat: a Bevel back and a Bevel cushion down in the bored footwell, each
    // sitting on the bore's floor AT ITS OWN STATION. Taking the floor depth
    // amidships instead floats the back a finger clear of it, which is the
    // same class of mistake as a mount on a guessed fraction.
    let sz = plan.seat_z();
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
    // Steering column, out of the SCUTTLE - which is what a column comes
    // through. Started from the footwell floor instead, it stands in mid-air.
    let wx = -plan.half_width_at(cz) * 0.44;
    kids.push(line(
        &[
            (
                [
                    wx,
                    plan.sill_at(0.0) * TUB_HOLLOW + l * 0.022,
                    fz + l * 0.026,
                ],
                l * 0.0048,
            ),
            ([wx, l * 0.052, sz + 0.120 * l], l * 0.0048),
        ],
        6,
        c.machinery.clone(),
    ));
    kids.push(turned(
        &[
            (0.0, 0.0),
            (l * 0.085, 0.0),
            (l * 0.085, l * 0.006),
            (0.0, l * 0.006),
        ],
        20,
        false,
        c.machinery.clone(),
        [wx, l * 0.058, sz + 0.126 * l],
        quat_x(FRAC_PI_2 - 0.55),
    ));
    // The spare on the tail deck, leaning back with its disc facing aft - the
    // rear of a boat tail is otherwise a blank at play distance.
    let spz = -0.462 * l;
    let spare = wheel_profiles(plan.wheel_r * 0.52);
    let lay = quat_x(-(FRAC_PI_2 - 0.35));
    let at = [0.0, plan.crown_at(spz) + l * 0.0445, spz];
    kids.push(turned(&spare.tyre, 24, true, c.rubber.clone(), at, lay));
    kids.push(turned(&spare.disc, 24, false, c.disc.clone(), at, lay));
}
