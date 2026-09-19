//! The buggy's tube frame: its joints read off the plan, the members drawn
//! through them, the backbone that is her root, and the lamps it carries.
//!
//! Every member runs through joints another member also runs through, so
//! each one ends ON a centreline - the only place the connectedness guard
//! registers a joint between thin tubes (see the module docs one level up).
//! The floor rail is drawn nose to tail through all nine of its mounts, the
//! shoulder rail nose to the main hoop through six.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::BuggyColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::BuggyVariant;

use super::super::super::common::quat_x;
use super::engine::Engine;
use super::{
    AxleLine, BuggyPlan, FRONT_HOOP, MAIN_HOOP, RAIL, RAKE, TUBE, along_z, axle, line, solid,
};

/// The frame's joints, read off the plan - where every member, mount and
/// mass on the buggy starts and ends.
pub(super) struct Frame {
    pub(super) plan: BuggyPlan,
    /// The machine's length (m).
    pub(super) l: f32,
    /// The frame's depth: half its side's height (m).
    pub(super) depth: f32,
    /// The main hoop behind the seats and the front hoop at the dash (m).
    pub(super) main_z: f32,
    pub(super) dash_z: f32,
    /// The engine guard at the tail and the nose hoop (m).
    pub(super) tail_z: f32,
    pub(super) nose_z: f32,
    pub(super) front: AxleLine,
    pub(super) rear: AxleLine,
    /// The cage's top bar over the datum, at the main hoop (m).
    pub(super) top_y: f32,
    /// The cage's top corners' distance from the centreline (m).
    pub(super) top_x: f32,
    /// The front hoop's top, raked back and a little lower (m).
    pub(super) rake_z: f32,
    pub(super) rake_y: f32,
    pub(super) half_track: f32,
    /// The front A-arms' mounts on the rails, aft and forward of the front
    /// axle (m).
    pub(super) arm_aft: f32,
    pub(super) arm_fore: f32,
    /// The rear trailing arms' pivot on the floor rail, ahead of the rear
    /// axle (m).
    pub(super) pivot_z: f32,
    /// The engine guard's top corners: the frame's half-width at the tail,
    /// and a height over the engine (m).
    pub(super) tail_x: f32,
    pub(super) tail_y: f32,
    pub(super) engine: Engine,
}

impl Frame {
    /// The joints, then the engine's numbers off the rear axle, then the
    /// engine guard's height over the engine - which the rear stays and the
    /// tail lamps read.
    pub(super) fn new(plan: &BuggyPlan) -> Self {
        let l = plan.length;
        let (main_z, dash_z) = (plan.at(MAIN_HOOP), plan.at(FRONT_HOOP));
        let (tail_z, nose_z) = (plan.tail_z(), plan.nose_z());
        let (front, rear) = (axle(plan, true), axle(plan, false));
        let top_y = plan.cage_top();
        let engine = Engine::new(l, rear, tail_z);
        let tail_y = (plan.shoulder(tail_z) + 0.050 * l).max(engine.peak() + 0.022 * l);
        Self {
            plan: *plan,
            l,
            depth: plan.depth(),
            main_z,
            dash_z,
            tail_z,
            nose_z,
            front,
            rear,
            top_y,
            top_x: plan.hw(main_z) * 0.80,
            rake_z: dash_z - RAKE * l,
            rake_y: top_y * 0.96,
            half_track: plan.track * 0.5,
            arm_aft: front.z - 0.045 * l,
            arm_fore: front.z + 0.045 * l,
            pivot_z: rear.z + 0.150 * l,
            tail_x: plan.hw(tail_z),
            tail_y,
            engine,
        }
    }

    /// A joint on the floor rail at `z`, on side `s`.
    pub(super) fn lower(&self, z: f32, s: f32) -> [f32; 3] {
        [s * self.plan.hw(z), self.plan.floor(z), z]
    }

    /// A joint on the shoulder rail at `z`, on side `s`.
    pub(super) fn upper(&self, z: f32, s: f32) -> [f32; 3] {
        [s * self.plan.hw(z), self.plan.shoulder(z), z]
    }

    /// A point ON the rear stay over the rear axle, where the rear coilover
    /// hangs from: the stay is drawn through it.
    pub(super) fn stay_mid(&self, s: f32) -> [f32; 3] {
        let z = self.rear.z + 0.020 * self.l;
        let t = (z - self.main_z) / (self.tail_z - self.main_z);
        let x = self.top_x + (self.tail_x * 0.92 - self.top_x) * t;
        let y = self.top_y + (self.tail_y - self.top_y) * t;
        [s * x, y, z]
    }

    /// The main hoop, behind the seats: floor to shoulder to the cage top,
    /// across, and down.
    pub(super) fn main_hoop(&self) -> [[f32; 3]; 8] {
        let (l, d, z) = (self.l, self.depth, self.main_z);
        let (x0, xt) = (self.plan.hw(z), self.top_x);
        let shoulder = self.top_y - 0.030 * l;
        [
            [-x0, -d, z],
            [-x0, d, z],
            [-xt * 1.10, shoulder, z],
            [-xt, self.top_y, z],
            [xt, self.top_y, z],
            [xt * 1.10, shoulder, z],
            [x0, d, z],
            [x0, -d, z],
        ]
    }

    /// The front hoop, at the dash: its legs raked back to a top a little
    /// lower than the main hoop's.
    pub(super) fn front_hoop(&self) -> [[f32; 3]; 8] {
        let (l, d, z) = (self.l, self.depth, self.dash_z);
        let (x1, xt) = (self.plan.hw(z), self.top_x);
        let (shoulder, zs) = (self.rake_y - 0.030 * l, self.rake_z + 0.012 * l);
        [
            [-x1, -d, z],
            [-x1, d, z],
            [-xt * 1.08, shoulder, zs],
            [-xt, self.rake_y, self.rake_z],
            [xt, self.rake_y, self.rake_z],
            [xt * 1.08, shoulder, zs],
            [x1, d, z],
            [x1, -d, z],
        ]
    }
}

/// A frame member through its joints, one radius all along.
fn tube(joints: &[[f32; 3]], radius: f32, resolution: u32, c: &BuggyColours) -> Generator {
    let points: Vec<([f32; 3], f32)> = joints.iter().map(|&p| (p, radius)).collect();
    line(&points, resolution, &c.frame)
}

/// The ROOT: the pan's backbone along the floor, from the front cross tube
/// to the transaxle's nose - its points its own, the boats' keelson idiom.
pub(super) fn backbone(f: &Frame, c: &BuggyColours) -> Generator {
    let (l, d) = (f.l, f.depth);
    line(
        &[
            ([0.0, f.plan.floor(f.front.z), f.front.z], RAIL * l * 1.3),
            ([0.0, -d * 0.93, f.dash_z], RAIL * l * 1.6),
            ([0.0, -d * 0.93, f.main_z], RAIL * l * 1.6),
            ([0.0, f.rear.y, f.rear.z + 0.075 * l], RAIL * l * 1.5),
        ],
        8,
        &c.alloy,
    )
}

/// Rails, hoops, roof bars, rear stays, nerf bars, the engine guard, the
/// nose hoop, the brush bar, the front cross tube - one painted tube frame -
/// and on a raider a diagonal across the main hoop.
pub(super) fn build(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let (l, d) = (f.l, f.depth);
    let (r, rr) = (TUBE * l, RAIL * l);
    for s in [-1.0f32, 1.0] {
        // The floor rail, nose to tail through every mount it carries, and
        // the shoulder rail, nose to the main hoop.
        let floor = [
            f.nose_z, f.arm_fore, f.front.z, f.arm_aft, f.dash_z, f.main_z, f.pivot_z, f.rear.z,
            f.tail_z,
        ];
        kids.push(tube(&floor.map(|z| f.lower(z, s)), rr, 8, c));
        let shoulder = [
            f.nose_z, f.arm_fore, f.front.z, f.arm_aft, f.dash_z, f.main_z,
        ];
        kids.push(tube(&shoulder.map(|z| f.upper(z, s)), rr, 8, c));
    }
    kids.push(tube(&f.main_hoop(), r, 8, c));
    kids.push(tube(&f.front_hoop(), r, 8, c));
    let (x0, x1, xt) = (f.plan.hw(f.main_z), f.plan.hw(f.dash_z), f.top_x);
    let out = 0.028 * l;
    for s in [-1.0f32, 1.0] {
        // The roof bar, front hoop's top corner to the main hoop's.
        kids.push(tube(
            &[[s * xt, f.rake_y, f.rake_z], [s * xt, f.top_y, f.main_z]],
            r,
            6,
            c,
        ));
        // The rear stay, from the main hoop's top corner over the engine to
        // the guard.
        kids.push(tube(
            &[
                [s * xt, f.top_y, f.main_z],
                f.stay_mid(s),
                [s * f.tail_x * 0.92, f.tail_y, f.tail_z],
            ],
            rr,
            6,
            c,
        ));
        // The nerf bar, bowed out between the hoops' feet.
        kids.push(tube(
            &[
                f.lower(f.dash_z, s),
                [s * (x1 + out), -d * 0.92, f.dash_z - 0.06 * l],
                [s * (x0 + out), -d * 0.92, f.main_z + 0.06 * l],
                f.lower(f.main_z, s),
            ],
            rr,
            6,
            c,
        ));
    }
    // The engine guard: up from the tail's floor ends and across over the
    // engine.
    let (xg, low) = (f.tail_x, f.plan.floor(f.tail_z));
    kids.push(tube(
        &[
            [-xg, low, f.tail_z],
            [-xg * 0.92, f.tail_y, f.tail_z],
            [xg * 0.92, f.tail_y, f.tail_z],
            [xg, low, f.tail_z],
        ],
        rr,
        6,
        c,
    ));
    // The nose hoop, from the floor rails' nose ends up to the shoulder
    // rails' and across, and the brush bar out in front of it off its floor
    // corners.
    let xn = f.plan.hw(f.nose_z);
    let (lo, hi) = (f.plan.floor(f.nose_z), f.plan.shoulder(f.nose_z));
    kids.push(tube(
        &[
            [-xn, lo, f.nose_z],
            [-xn, hi, f.nose_z],
            [xn, hi, f.nose_z],
            [xn, lo, f.nose_z],
        ],
        rr,
        6,
        c,
    ));
    let (zb, xb) = (f.nose_z + 0.036 * l, xn * 1.35);
    kids.push(tube(
        &[
            [-xn, lo, f.nose_z],
            [-xb, lo - 0.006 * l, zb],
            [-xb * 0.92, hi + 0.020 * l, zb + 0.004 * l],
            [xb * 0.92, hi + 0.020 * l, zb + 0.004 * l],
            [xb, lo - 0.006 * l, zb],
            [xn, lo, f.nose_z],
        ],
        rr,
        6,
        c,
    ));
    // The front cross tube at the front axle: the backbone's forward end.
    kids.push(tube(
        &[f.lower(f.front.z, -1.0), f.lower(f.front.z, 1.0)],
        rr,
        6,
        c,
    ));
    if f.plan.variant == BuggyVariant::Raider {
        // The raider's diagonal across the main hoop, from its far shoulder
        // to its near top corner.
        kids.push(tube(
            &[[-x0, d, f.main_z], [xt, f.top_y, f.main_z]],
            rr,
            6,
            c,
        ));
    }
}

/// Two turned headlamps on the nose hoop's top corners, facing ahead, and two
/// tail lamps on the engine guard's top corners, facing the chase camera.
pub(super) fn lamps(kids: &mut Vec<Generator>, f: &Frame, c: &BuggyColours) {
    let l = f.l;
    let k = l / 2.7 * 0.80;
    let shell: Vec<(f32, f32)> = [
        (0.0, -0.110),
        (0.045, -0.090),
        (0.080, -0.035),
        (0.090, 0.020),
        (0.090, 0.045),
        (0.080, 0.052),
    ]
    .iter()
    .map(|&(r, h)| (r * k, h * k))
    .collect();
    let lens = [(0.0, 0.0), (0.077 * k, 0.0), (0.077 * k, 0.010 * k)];
    let tail = [
        (0.0, 0.0),
        (0.016 * l, 0.0),
        (0.016 * l, 0.010 * l),
        (0.0, 0.012 * l),
    ];
    let (xn, hi) = (f.plan.hw(f.nose_z), f.plan.shoulder(f.nose_z));
    for s in [-1.0f32, 1.0] {
        let at = [s * xn, hi + 0.012 * l, f.nose_z + 0.020 * l];
        kids.push(solid(&shell, 18, true, &c.bright, at, along_z()));
        kids.push(solid(
            &lens,
            16,
            false,
            &c.lamp,
            [at[0], at[1], at[2] + 0.052 * k],
            along_z(),
        ));
        kids.push(solid(
            &tail,
            12,
            false,
            &c.tail_lamp,
            [s * f.tail_x * 0.92, f.tail_y, f.tail_z - 0.004 * l],
            quat_x(-FRAC_PI_2),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::plan_of;
    use super::*;
    use crate::seeded_defaults::{SkiffBlueprint, VehicleStance};

    /// The frame IS the plan: both hoops' feet are floor-rail joints,
    /// because the datum is the frame's mid-height and the cockpit is the
    /// plan's full width from the main hoop to the dash. A plan form that
    /// narrowed there would leave the hoops standing beside the rails.
    #[test]
    fn the_hoops_stand_on_the_rails() {
        for length in [1.90f32, 2.65, 3.60] {
            let bp = SkiffBlueprint {
                stance: VehicleStance::Sleek,
                length,
                body_w: length * 0.272,
                wheelbase: length * 0.64,
                track: length * 0.42,
                wheel_r: length * 0.115,
                beltline: length * 0.300,
                height: length * 0.372,
            };
            let f = Frame::new(&plan_of(&bp, BuggyVariant::Rail));
            for (hoop, z) in [(f.main_hoop(), f.main_z), (f.front_hoop(), f.dash_z)] {
                for s in [0usize, 7] {
                    let floor = f.lower(z, hoop[s][0].signum());
                    assert!(
                        (hoop[s][1] - floor[1]).abs() < 1e-5
                            && (hoop[s][0] - floor[0]).abs() < 1e-5,
                        "a {length} m buggy's hoop at {z} stands off its floor rail"
                    );
                }
            }
        }
    }
}
