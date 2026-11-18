//! Gantry crane - an Industrial-Park yard crane (#1943), one of the three
//! showpieces of a part's slide and swell (#1940): a rail-mounted gantry
//! that travels its rails while its trolley crosses the bridge and its hook
//! rises and falls, so the load it carries traces a slow path through
//! three dimensions. Not the quayside container crane
//! (`container_crane_quay`): no boom over the water, no A-frame - a
//! double-girder bridge on four tubular legs, its trolley riding on top.
//!
//! What stands still, and is solid: the yard's apron (the footing), two
//! long rails on sleepers along Z and a buffer at each of their ends.
//!
//! What moves, every client moving it for itself, so none of it has a
//! collider:
//!
//! - **The gantry** - two girders across the span, joined at their ends,
//!   on four legs standing on two sills with a bogie wheel at each end -
//!   swings [`GANTRY_TRAVEL`] metres either way along the rails, once every
//!   [`GANTRY_PERIOD`] seconds. Its wheels roll with that travel, by a roll
//!   term of their own radius (a roll reads the part's motion, slide and
//!   all). The operator's cab and two amber beacons ride with it.
//! - **The trolley** on the girders swings [`TROLLEY_TRAVEL`] metres either
//!   way across the span, once every [`TROLLEY_PERIOD`] seconds, its work
//!   light hanging under it and shining down.
//! - **The hoist**: from a hidden reel node inside the trolley, whose
//!   origin is where the cables leave it, two cables hang [`CABLE`] metres
//!   to the hook block. The reel swells along its own +Y alone (weights
//!   `[0, 1, 0]`), so the cables stretch downward from the trolley. Beside
//!   the reel the hook block - its hook, spreader and load below it -
//!   swings [`HOOK_DROP`] metres either side of its rest along -Y. The
//!   swell and the swing share their period ([`HOOK_PERIOD`]) and phase,
//!   and the swell's amount is the drop over the cable's length, so the
//!   cables' lower ends and the block's top stay joined every frame: with
//!   `w` the swing's share of its reach at a moment, the cables end
//!   `CABLE x (1 + SWELL x w)` below the reel and the block's top stands
//!   `CABLE + HOOK_DROP x w` below it - the same, since `CABLE x SWELL` is
//!   `HOOK_DROP`. At Settings > Effects Reduced both are calmed alike, so
//!   they stay joined there too.
//!
//! The three periods (30, 17 and 13 seconds) share no factor, so the load's
//! path comes round again only after 6630 seconds - nearly two hours.
//!
//! Built in the yard's ground frame by [`assemble`] and [`nest`]; every part
//! that carries others stands axis-aligned, and the wheels' webs, authored
//! in their turned wheels' own frames, are the only exception.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::fx::light::{Lamp, lit};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest,
    prim, quat_x, quat_z, solid, sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, Slide, SlideTerm, Spin, SpinTerm, Swell, SwellTerm};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, CONTAINER_BLUE, LAMP_AMBER, PIPE_GREY, TANK_WHITE, cladding, glass, tank_steel,
    timber,
};

/// The apron's top - the footing's reveal over the ground.
pub(super) const APRON_TOP: f32 = 0.15;
/// The rails: half the span between them, half their length, their top.
pub(super) const RAIL_X: f32 = 7.0;
pub(super) const RAIL_HALF: f32 = 6.2;
const RAIL_TOP: f32 = 0.43;
/// The bogie wheels: radius, and how far along the sill from its middle.
pub(super) const WHEEL_R: f32 = 0.32;
const WHEEL_Z: f32 = 1.2;
/// The wheels' axles, a centimetre into the rail's top.
const WHEEL_Y: f32 = RAIL_TOP + WHEEL_R - 0.01;
/// The girders' underside and top, and how far each stands off the middle.
const GIRDER_BOTTOM: f32 = 8.0;
const GIRDER_TOP: f32 = 9.0;
const GIRDER_Z: f32 = 0.9;
/// A leg's radius, where it stands on its sill and how far out along Z.
pub(super) const LEG_R: f32 = 0.24;
const LEG_FOOT_Y: f32 = 1.5;
const LEG_FOOT_Z: f32 = 1.45;
/// How far the gantry travels either way along the rails (m), and how long
/// a there and back takes (s).
pub(super) const GANTRY_TRAVEL: f32 = 3.0;
pub(super) const GANTRY_PERIOD: f32 = 30.0;
/// How far the trolley crosses either way (m), and its period (s).
pub(super) const TROLLEY_TRAVEL: f32 = 4.0;
pub(super) const TROLLEY_PERIOD: f32 = 17.0;
/// The hoist: where the cables leave the trolley (the reel's origin), how
/// long they hang at rest, how far the hook rides either side of its rest,
/// and its period - the swell's as well.
pub(super) const REEL_Y: f32 = 9.2;
pub(super) const CABLE: f32 = 4.0;
pub(super) const HOOK_DROP: f32 = 2.0;
pub(super) const HOOK_PERIOD: f32 = 13.0;
/// The swell's amount: the hook's drop over the cable's length, so the
/// cables' lower ends ride with the block's top.
pub(super) const SWELL: f32 = HOOK_DROP / CABLE;
/// The hook block's top at rest, where the cables end.
pub(super) const HOOK_TOP: f32 = REEL_Y - CABLE;
/// The hook block's height, and how far apart the two cables hang.
pub(super) const BLOCK_H: f32 = 0.5;
pub(super) const CABLE_X: f32 = 0.12;
/// Spacing circle round the yard: the apron's far corners.
pub(super) const CLEARANCE: f32 = 10.5;
const APRON: [f32; 2] = [15.4, 13.4];

/// The gantry's yard yellow, the hoist house's white, the buffers' black.
const CRANE_YELLOW: [f32; 3] = [0.90, 0.62, 0.10];
const BUFFER_BLACK: [f32; 3] = [0.10, 0.10, 0.11];
const CAB_GLASS: [f32; 3] = [0.40, 0.52, 0.56];
const SLEEPER_BROWN: [f32; 3] = [0.36, 0.28, 0.20];

pub struct GantryCrane;

impl CatalogueEntry for GantryCrane {
    fn slug(&self) -> &'static str {
        "gantry_crane"
    }
    fn name(&self) -> &'static str {
        "Gantry crane"
    }
    fn description(&self) -> &'static str {
        "Rail-mounted yard gantry: it travels its rails while the trolley crosses \
         the bridge and the hook rises and falls, cable and hook moving as one."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
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
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn swing(amplitude: f32, period: f32) -> SlideTerm {
    SlideTerm::Swing {
        amplitude: Fp(amplitude),
        period: Fp(period),
        phase: Fp::ZERO,
    }
}

/// One bogie wheel at `at`, its axle across the rails, rolling with the
/// gantry's travel; a web across it, authored in the wheel's own frame (its
/// own +Y the axle), so the roll reads.
fn wheel(at: [f32; 3]) -> Generator {
    let mut wheel = prim(
        cylinder_tapered(WHEEL_R, 0.16, 14, 0.0, tank_steel(PIPE_GREY)),
        at,
        quat_z(FRAC_PI_2),
    );
    wheel.spin = Some(Spin::about(
        [0.0, 1.0, 0.0],
        SpinTerm::Roll {
            radius: Fp(WHEEL_R),
        },
    ));
    wheel.children.push(prim(
        cuboid_tapered([WHEEL_R * 1.7, 0.18, 0.09], 0.0, tank_steel(CRANE_YELLOW)),
        [0.0; 3],
        id_quat(),
    ));
    wheel
}

/// An amber beacon on the girder's top at `x`: its base and its lens, the
/// lens a light that breathes.
fn beacon(x: f32) -> Vec<Generator> {
    vec![
        prim(
            cylinder_tapered(0.1, 0.1, 12, 0.0, tank_steel(BUFFER_BLACK)),
            [x, GIRDER_TOP + 0.04, -GIRDER_Z],
            id_quat(),
        ),
        lit(
            prim(
                cylinder_tapered(0.08, 0.16, 12, 0.0, glow(LAMP_AMBER, 2.4)),
                [x, GIRDER_TOP + 0.165, -GIRDER_Z],
                id_quat(),
            ),
            Lamp::Beacon { over: 1.5 },
        ),
    ]
}

/// The hook block, swinging along -Y, with its hook, slings, spreader and
/// load below it.
fn hook_block() -> Generator {
    let mut block = prim(
        cuboid_tapered([0.5, BLOCK_H, 0.32], 0.0, tank_steel(CRANE_YELLOW)),
        [0.0, HOOK_TOP - BLOCK_H * 0.5, 0.0],
        id_quat(),
    );
    block.slide = Some(Slide::along(
        [0.0, -1.0, 0.0],
        swing(HOOK_DROP, HOOK_PERIOD),
    ));
    let below = HOOK_TOP - BLOCK_H;
    let hook_y = below - 0.28;
    let spreader_y = hook_y - 0.49;
    let load_top = spreader_y - 0.06 + 0.01;
    nest(
        block,
        vec![
            // The shank, and the hook stood on edge below it.
            prim(
                cylinder_tapered(0.05, 0.18, 8, 0.0, tank_steel(PIPE_GREY)),
                [0.0, below - 0.07, 0.0],
                id_quat(),
            ),
            prim(
                torus(0.045, 0.13, tank_steel(PIPE_GREY)),
                [0.0, hook_y, 0.0],
                quat_x(FRAC_PI_2),
            ),
            // Two slings from the hook to the spreader's ends.
            strut(
                [0.0, hook_y - 0.12, 0.0],
                [-1.0, spreader_y + 0.04, 0.0],
                0.02,
                6,
                tank_steel(BUFFER_BLACK),
            ),
            strut(
                [0.0, hook_y - 0.12, 0.0],
                [1.0, spreader_y + 0.04, 0.0],
                0.02,
                6,
                tank_steel(BUFFER_BLACK),
            ),
            prim(
                cuboid_tapered([2.2, 0.12, 0.16], 0.0, tank_steel(CRANE_YELLOW)),
                [0.0, spreader_y, 0.0],
                id_quat(),
            ),
            // The load: a small blue container, locked under the spreader.
            prim(
                cuboid_tapered([2.4, 1.1, 1.15], 0.0, cladding(CONTAINER_BLUE)),
                [0.0, load_top - 0.55, 0.0],
                id_quat(),
            ),
        ],
    )
}

/// The trolley: its frame on the girders, its hoist house, the reel and its
/// cables, the hook block and the work light under it.
fn trolley() -> Generator {
    let frame_bottom = GIRDER_TOP - 0.02;
    let frame_top = frame_bottom + 0.45;
    let mut frame = prim(
        cuboid_tapered([2.0, 0.45, 2.5], 0.0, tank_steel(CRANE_YELLOW)),
        [0.0, (frame_bottom + frame_top) * 0.5, 0.0],
        id_quat(),
    );
    frame.slide = Some(Slide::along(
        [1.0, 0.0, 0.0],
        swing(TROLLEY_TRAVEL, TROLLEY_PERIOD),
    ));

    // The reel: hidden in the frame, its origin where the cables leave it,
    // swelling along +Y alone so the cables stretch down from it.
    let mut reel = prim(
        sphere(0.03, 2, tank_steel(PIPE_GREY)),
        [0.0, REEL_Y, 0.0],
        id_quat(),
    );
    reel.swell = Some(Swell {
        weights: Fp3([0.0, 1.0, 0.0]),
        keep_volume: false,
        terms: vec![SwellTerm::Swing {
            amount: Fp(SWELL),
            period: Fp(HOOK_PERIOD),
            phase: Fp::ZERO,
        }],
    });
    let cable = |x: f32| {
        prim(
            cylinder_tapered(0.025, CABLE, 6, 0.0, tank_steel(BUFFER_BLACK)),
            [x, REEL_Y - CABLE * 0.5, 0.0],
            id_quat(),
        )
    };
    let reel = nest(reel, vec![cable(-CABLE_X), cable(CABLE_X)]);

    // The work light, on a rod hung under the frame beside the cables,
    // below the girders so it is seen and its pool is clear of them.
    let lens_y = GIRDER_BOTTOM - 0.15;
    let lens = lit(
        prim(
            cylinder_tapered(0.1, 0.03, 12, 0.0, glow([1.0, 0.96, 0.85], 2.6)),
            [0.6, lens_y, 0.0],
            id_quat(),
        ),
        Lamp::Flood {
            over: lens_y,
            aim: [0.0, -1.0, 0.0],
        },
    );
    let rod_top = frame_bottom + 0.1;
    let rod_bottom = lens_y + 0.06;
    nest(
        frame,
        vec![
            // The hoist house, and its roof pitched along the span.
            prim(
                cuboid_tapered([1.6, 0.9, 1.8], 0.0, cladding(TANK_WHITE)),
                [0.0, frame_top - 0.01 + 0.45, 0.0],
                id_quat(),
            ),
            prim(
                cuboid_tapered_xz([1.7, 0.25, 1.9], [0.0, 0.7], tank_steel(CONCRETE_GREY)),
                [0.0, frame_top - 0.02 + 0.9 + 0.125, 0.0],
                id_quat(),
            ),
            reel,
            hook_block(),
            prim(
                cylinder_tapered(0.03, rod_top - rod_bottom, 6, 0.0, tank_steel(PIPE_GREY)),
                [0.6, (rod_top + rod_bottom) * 0.5, 0.0],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.26, 0.12, 0.26], 0.0, tank_steel(BUFFER_BLACK)),
                [0.6, lens_y + 0.07, 0.0],
                id_quat(),
            ),
            lens,
        ],
    )
}

/// The gantry: two girders joined at their ends, four legs on two sills,
/// a bogie wheel at each sill's end, the cab, the beacons and the trolley -
/// the near girder its root, carrying its travel.
fn gantry() -> Generator {
    let paint = tank_steel(CRANE_YELLOW);
    let girder = |z: f32| {
        prim(
            cuboid_tapered(
                [2.0 * RAIL_X + 1.0, GIRDER_TOP - GIRDER_BOTTOM, 0.5],
                0.0,
                paint.clone(),
            ),
            [0.0, (GIRDER_TOP + GIRDER_BOTTOM) * 0.5, z],
            id_quat(),
        )
    };
    let mut near = girder(-GIRDER_Z);
    near.slide = Some(Slide::along(
        [0.0, 0.0, 1.0],
        swing(GANTRY_TRAVEL, GANTRY_PERIOD),
    ));
    let mut parts = vec![girder(GIRDER_Z)];
    let sill_bottom = WHEEL_Y + WHEEL_R - 0.02;
    for side in [-1.0f32, 1.0] {
        let x = side * RAIL_X;
        // The end tie across the girders' ends, inside their outer faces.
        parts.push(prim(
            cuboid_tapered([0.6, 0.6, 2.0 * GIRDER_Z + 0.4], 0.0, paint.clone()),
            [x, (GIRDER_TOP + GIRDER_BOTTOM) * 0.5, 0.0],
            id_quat(),
        ));
        // The sill, the two legs on it and a tie between them halfway up.
        parts.push(prim(
            cuboid_tapered([0.5, 0.5, 2.0 * (LEG_FOOT_Z + 0.25)], 0.0, paint.clone()),
            [x, sill_bottom + 0.25, 0.0],
            id_quat(),
        ));
        for z in [-1.0f32, 1.0] {
            parts.push(strut(
                [x, LEG_FOOT_Y, z * LEG_FOOT_Z],
                [x, GIRDER_BOTTOM + 0.05, z * GIRDER_Z],
                LEG_R,
                10,
                paint.clone(),
            ));
            parts.push(wheel([x, WHEEL_Y, z * WHEEL_Z]));
        }
        let tie_y = 4.5;
        let tie_z = LEG_FOOT_Z
            - (LEG_FOOT_Z - GIRDER_Z) * (tie_y - LEG_FOOT_Y) / (GIRDER_BOTTOM + 0.05 - LEG_FOOT_Y);
        parts.push(strut(
            [x, tie_y, -tie_z],
            [x, tie_y, tie_z],
            0.12,
            8,
            paint.clone(),
        ));
    }
    // The operator's cab under the near girder by the right-hand legs,
    // glazed to the front.
    parts.push(prim(
        cuboid_tapered([1.1, 1.52, 1.1], 0.0, cladding(TANK_WHITE)),
        [6.05, GIRDER_BOTTOM + 0.02 - 0.76, -1.3],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.9, 0.7, 0.02], 0.0, glass(CAB_GLASS, 0.0)),
        [6.05, GIRDER_BOTTOM - 0.55, -1.855],
        id_quat(),
    ));
    parts.extend(beacon(-6.95));
    parts.extend(beacon(6.95));
    parts.push(trolley());
    nest(near, parts)
}

fn build_tree() -> Generator {
    let mut prims = vec![footing(APRON[0], APRON[1], [0.0, 0.0], CLEARANCE)];
    for side in [-1.0f32, 1.0] {
        let x = side * RAIL_X;
        // Sleepers, a centimetre into the apron, and the rail a centimetre
        // into them.
        for k in 0..7 {
            prims.push(prim(
                solid(cuboid_tapered(
                    [1.0, 0.15, 0.24],
                    0.0,
                    timber(SLEEPER_BROWN),
                )),
                [x, APRON_TOP - 0.01 + 0.075, -6.0 + 2.0 * k as f32],
                id_quat(),
            ));
        }
        prims.push(prim(
            solid(cuboid_tapered(
                [0.12, 0.15, 2.0 * RAIL_HALF],
                0.0,
                tank_steel(PIPE_GREY),
            )),
            [x, RAIL_TOP - 0.075, 0.0],
            id_quat(),
        ));
        // A buffer at each end, on the apron.
        for z in [-1.0f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.5, 0.45, 0.3],
                    0.0,
                    tank_steel(BUFFER_BLACK),
                )),
                [x, APRON_TOP - 0.01 + 0.225, z * (RAIL_HALF + 0.15)],
                id_quat(),
            ));
        }
    }
    prims.push(gantry());
    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure::Bounds;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{
        Placed, assert_sanitize_stable, motion_reach, placed, placed_at,
    };
    use crate::pds::sanitize_generator;
    use crate::world_builder::spin::REDUCED_MOTION_SHARE;

    fn built() -> Generator {
        GantryCrane.build("")
    }

    /// The tree as a peer draws it: sanitised and through the wire.
    fn on_the_wire() -> Generator {
        let mut tree = built();
        sanitize_generator(&mut tree);
        serde_json::from_str(&serde_json::to_string(&tree).expect("writes")).expect("reads")
    }

    /// The path of the part whose slide's term says `which` of the three
    /// travels it is: the gantry's along Z, the trolley's along X, the
    /// hook's down.
    fn slider(tree: &Generator, axis: [f32; 3]) -> Vec<usize> {
        placed(tree)
            .into_iter()
            .find(|p| p.node.slide.as_ref().is_some_and(|s| s.axis.0 == axis))
            .expect("a sliding part")
            .path
    }

    fn reel_path(tree: &Generator) -> Vec<usize> {
        placed(tree)
            .into_iter()
            .find(|p| p.node.swell.is_some())
            .expect("a reel")
            .path
    }

    fn at<'a>(drawn: &'a [Placed<'a>], path: &[usize]) -> &'a Placed<'a> {
        drawn.iter().find(|p| p.path == path).expect("drawn")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "gantry_crane");
    }

    #[test]
    fn nothing_floats_and_nothing_z_fights() {
        let tree = built();
        assert_nothing_floats(&tree, "gantry_crane");
        assert_no_z_fighting(&tree, "gantry_crane");
    }

    /// The hoist's numbers fit: the swell's amount is the drop over the
    /// cable, on the wire's grid, and the three periods share no factor, so
    /// the load's path takes 6630 s to come round.
    #[test]
    fn the_hoist_and_the_travels_are_set_to_their_beats() {
        assert_eq!(SWELL, 0.5);
        assert_eq!(Fp(SWELL).on_wire().0, SWELL);
        let gcd = |mut a: u32, mut b: u32| {
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a
        };
        let periods = [GANTRY_PERIOD, TROLLEY_PERIOD, HOOK_PERIOD].map(|p| p as u32);
        for (i, a) in periods.iter().enumerate() {
            for b in &periods[i + 1..] {
                assert_eq!(gcd(*a, *b), 1, "{a} s and {b} s share a factor");
            }
        }
        assert_eq!(periods.iter().product::<u32>(), 6630);
    }

    /// Over a whole hook cycle, at Effects as authored and Reduced and at
    /// the wall clock's seconds, each cable's lower end stays on the hook
    /// block's top, within a millimetre, and over it.
    #[test]
    fn the_cables_stay_joined_to_the_hook() {
        let tree = on_the_wire();
        let reel = reel_path(&tree);
        let hook = slider(&tree, [0.0, -1.0, 0.0]);
        let mut seen = 0;
        for start in [0.0, 1.8e9 + 0.37] {
            for calm in [1.0, REDUCED_MOTION_SHARE] {
                for k in 0..200 {
                    let now = start + f64::from(HOOK_PERIOD) * f64::from(k) / 200.0;
                    let drawn = placed_at(&tree, now, calm);
                    let block = at(&drawn, &hook);
                    let top = block.at([0.0, BLOCK_H * 0.5, 0.0]);
                    let cables: Vec<&Placed> = drawn
                        .iter()
                        .filter(|p| p.path.len() == reel.len() + 1 && p.path.starts_with(&reel))
                        .collect();
                    assert_eq!(cables.len(), 2);
                    for cable in cables {
                        let end = cable.at([0.0, -CABLE * 0.5, 0.0]);
                        assert!(
                            (end.y - top.y).abs() < 1e-3,
                            "at {now} s (calm {calm}) a cable ends at {}, the hook's top is at {}",
                            end.y,
                            top.y
                        );
                        assert!((end.z - top.z).abs() < 1e-3);
                        assert!((end.x - top.x).abs() < 0.25, "over the block");
                        seen += 1;
                    }
                }
            }
        }
        assert_eq!(seen, 2 * 2 * 200 * 2);
    }

    /// Over the load's long path - and at the bottom of every hook swing
    /// with the trolley at either end of the span - the load stays clear of
    /// the apron and of the legs, and the gantry's wheels stay on their
    /// rails.
    #[test]
    fn the_load_keeps_clear_and_the_gantry_keeps_its_rails() {
        let tree = on_the_wire();
        let hook = slider(&tree, [0.0, -1.0, 0.0]);
        let wheels: Vec<Vec<usize>> = placed(&tree)
            .into_iter()
            .filter(|p| p.node.spin.is_some())
            .map(|p| p.path)
            .collect();
        assert_eq!(wheels.len(), 4);
        let bottom_of_swing = f64::from(HOOK_PERIOD) * 0.25;
        let mut times: Vec<f64> = (0..1500).map(|k| f64::from(k) * 4.42).collect();
        times.extend((0..40).map(|k| bottom_of_swing + f64::from(HOOK_PERIOD) * f64::from(k)));
        let leg_inside = RAIL_X - LEG_R;
        let (mut lowest, mut widest) = (f32::INFINITY, 0.0_f32);
        for now in times {
            let drawn = placed_at(&tree, now, 1.0);
            let load = drawn
                .iter()
                .filter(|p| p.path.starts_with(&hook))
                .filter_map(|p| p.drawn)
                .reduce(|a, b| Bounds {
                    min: a.min.min(b.min),
                    max: a.max.max(b.max),
                })
                .expect("the hook draws");
            lowest = lowest.min(load.min.y);
            widest = widest.max(load.max.x.abs().max(load.min.x.abs()));
            for wheel in &wheels {
                let b = at(&drawn, wheel).drawn.expect("a wheel draws");
                assert!(
                    b.max.z < RAIL_HALF && b.min.z > -RAIL_HALF,
                    "at {now} s a wheel runs off its rail: {b:?}"
                );
            }
        }
        assert!(lowest > APRON_TOP + 0.5, "the load comes down to {lowest}");
        assert!(
            lowest < APRON_TOP + 0.8,
            "the reading saw the swing's bottom: {lowest}"
        );
        assert!(
            widest < leg_inside - 1.0,
            "the load reaches {widest} across the span"
        );
    }

    /// The bogie wheels roll with the gantry's travel by their own radius,
    /// about an axle across the rails.
    #[test]
    fn the_wheels_roll_with_the_travel() {
        let tree = on_the_wire();
        let mut wheels = 0;
        for p in placed(&tree) {
            let Some(spin) = &p.node.spin else { continue };
            assert_eq!(
                spin.terms,
                vec![SpinTerm::Roll {
                    radius: Fp(WHEEL_R)
                }]
            );
            let axle = p.toward(spin.axis.0);
            assert!((axle.x.abs() - 1.0).abs() < 1e-5, "an axle along {axle}");
            wheels += 1;
        }
        assert_eq!(wheels, 4);
    }

    /// Everything the crane draws, the gantry at either end of its travel
    /// and the trolley at either end of the span, stays inside its
    /// footprint.
    #[test]
    fn the_footprint_covers_the_crane_wherever_it_travels() {
        let gantry_end = f64::from(GANTRY_PERIOD) * 0.25;
        let trolley_end = f64::from(TROLLEY_PERIOD) * 0.25;
        let times = [
            0.0,
            gantry_end,
            gantry_end + f64::from(GANTRY_PERIOD) * 0.5,
            trolley_end,
            trolley_end + f64::from(TROLLEY_PERIOD) * 0.5,
        ];
        let reach = motion_reach(&on_the_wire(), &times);
        assert!(reach <= CLEARANCE, "the crane reaches {reach} m");
    }
}
