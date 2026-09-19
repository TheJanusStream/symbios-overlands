//! The cyclecar: a three-wheeled teardrop pod with its cabin in its own
//! shell, the light machine of the neon themes and the campus (#1376).
//!
//! A TADPOLE: two wheels in front on swept stub axles, and the pod tapering
//! aft onto ONE wheel on the centreline under a spat, carried by a trailing
//! fork - the Morgan's layout, and the one that makes her wheel count
//! legible from the chase camera, which looks at the single spatted wheel
//! with the pair out wide at the far end. She is one machine on all four of
//! her themes: the finish speaks for the theme, lighting her rims and her
//! accent strip on a luminous kit.
//!
//! [`pod`] draws the pod, the window band in its shell, the accent strip and
//! the lamps; [`running_gear`] the stub axles, the fork, the spat and the
//! wheels; and [`dressing`] the ladder of secondary masses by ornateness and
//! wear.
//!
//! # The plan idiom, for a pod
//!
//! The family's [`BodyPlan`] fits her with no new field. She is ONE full
//! barrel swept over the plan's own stations, so crown and sill are the
//! pod's own top and bottom, and by the plan's coupling of depth to width
//! the pod narrows AND drops aft - the teardrop for free. The pod IS the
//! cabin, so its crown stands at the blueprint's HEIGHT (the roadster's
//! screen top), not its beltline: the blueprint is copied with the beltline
//! raised to the height, and the DATUM - the pod's widest section, where its
//! flank is upright - falls out of the plan as `height - depth` over the
//! ground. The root is a hidden hub on the datum, buried in the barrel (the
//! roadster's idiom). Two [`Axle`] rows, the rear one `paired: false`: the
//! first built type with a single wheel, which [`BodyPlan::wheels`] has
//! always emitted on the centreline.
//!
//! # Windows in a full barrel
//!
//! The window band is not a separate glass volume: it is SECTORS of a second
//! sweep over the pod's own stations, standing 2.5 % proud of it, on the
//! upper flank where a full barrel still stands near upright - with a
//! windscreen sector across the top forward of it and a rear-light sector
//! aft. **The proud is not decoration**: at 0.6 % the sectors came out
//! ragged, because a sub-run's Catmull-Rom leaves the whole pod's between
//! stations and a sector a hair proud dips under it. The primer patch is the
//! same trap, at 3 %.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1376, #1359 rules 1, 12 and 14):
//! `target/dump/vehicles2026-09/cyclecar/cyclecar.py` is its python twin,
//! and the port was checked node for node against it.

mod dressing;
mod pod;
mod running_gear;

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::cyclecar_colours;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ParticleAura, SkiffBlueprint, WearTier};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim, quat_x, quat_z};
use super::plan::{Axle, BodyPlan};
// The family's shape vocabulary, imported here so each part's
// `use super::{line, ..}` resolves.
use super::shape::{
    FAIRING_CLEAR, FAIRING_DROP, fairing, line, rim_profile, solid, sweep, tyre_profile,
};
use super::{SkiffCraft, SkiffFeel, dim};

/// The pod's depth per unit half-width - see [`BodyPlan::section`]. A little
/// taller than it is wide: the pod carries the cabin.
const SECTION: f32 = 1.12;

/// The pod's half-width over the blueprint's body half-width: a cyclecar is
/// narrow, but her pod is her cabin too.
const POD_W: f32 = 1.06;

/// The wheels' radius over the blueprint's, and the front pair's track over
/// the blueprint's. At the blueprint's own they were small wheels under a big
/// pod and the far front wheel hid behind it from the chase quarter.
const WHEEL: f32 = 1.15;
const TRACK: f32 = 1.12;

/// A tyre's half-width over its radius, on every wheel.
const TYRE_W: f32 = 0.26;

/// How far the rims stand out from the hub, over the tyre's half-width: proud
/// of both of its end caps.
const RIM_REACH: f32 = 1.10;

/// The pod's plan form, `(z fraction of the length, half-width fraction)`,
/// tail to nose: a teardrop widest ahead of the middle, its nose rounded and
/// its tail drawn out to a point over the single wheel.
const POD: &[(f32, f32)] = &[
    (-0.500, 0.08),
    (-0.450, 0.30),
    (-0.350, 0.56),
    (-0.210, 0.80),
    (-0.050, 0.95),
    (0.110, 1.00),
    (0.250, 0.95),
    (0.360, 0.82),
    (0.440, 0.60),
    (0.480, 0.38),
    (0.500, 0.12),
];

/// The tadpole: a pair in front, one wheel on the centreline behind, both at
/// the plan's own radius.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: 1.0,
    },
    Axle {
        at: -1.0,
        paired: false,
        radius: 1.0,
    },
];

/// The window band's run along the pod (fractions of the length) and how
/// far every sector over the pod stands proud of it.
const BAND_RUN: (f32, f32) = (-0.200, 0.200);
const PROUD: f32 = 1.025;

/// The side the chase camera's usual quarter shows: a worn cyclecar's odd
/// rim is on it.
const NEAR: f32 = 1.0;

/// Which side of the centreline `x` is on - `+0.0`, the single wheel's, the
/// right, as the twin has it.
fn side(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// A turned part laid with its axis outboard along x, on either side - and
/// on the centreline, the single wheel, laid as the right-hand side's.
fn outboard(x: f32) -> [f32; 4] {
    quat_z(-side(x) * FRAC_PI_2)
}

/// A turned part laid with its axis along +z, facing ahead.
fn along_z() -> [f32; 4] {
    quat_x(FRAC_PI_2)
}

/// One axle of the cyclecar: its station, its wheels' centre over the datum,
/// their radius and their tyres' half-width (m).
#[derive(Clone, Copy, Debug)]
struct AxleLine {
    z: f32,
    y: f32,
    r: f32,
    w: f32,
}

/// The cyclecar's plan: the family's [`BodyPlan`] over her own blueprint
/// copy. It derefs to the plan, so every shared read looks the same as on a
/// car, and it adds the reads her pod and her dressing share.
#[derive(Clone, Copy, Debug)]
pub(super) struct CyclecarPlan {
    plan: BodyPlan,
}

impl std::ops::Deref for CyclecarPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl CyclecarPlan {
    /// A station along the machine (m), from a fraction of the length.
    fn at(&self, fraction: f32) -> f32 {
        fraction * self.length
    }

    /// A point ON the pod's section at station `z`, `ang` radians round from
    /// the `+x` flank over the top - the sweep's own angle, so a strip, a lamp
    /// or a light bar is seated on the skin the pod draws.
    fn surface(&self, z: f32, ang: f32) -> [f32; 3] {
        let hw = self.half_width_at(z);
        [hw * ang.cos(), hw * self.section * ang.sin(), z]
    }

    /// The pod's own stations between two `z`, each radius `proud` times the
    /// pod's - a sweep path over the pod, or a sector standing proud of it.
    /// Never negative before the floor: that is a bug, not a small part (the
    /// tug's hidden one, #1370).
    fn over_pod(&self, from: f32, to: f32, proud: f32) -> Vec<([f32; 3], f32)> {
        self.run(from, to)
            .into_iter()
            .map(|(p, r)| {
                debug_assert!(r > 0.0, "the pod's half-width at {} is {r}", p[2]);
                (p, dim(r * proud))
            })
            .collect()
    }

    /// The pod's own section scale, which every sweep over it shares so their
    /// sections meet flush.
    fn scale(&self) -> [f32; 3] {
        [1.0, self.section, 1.0]
    }

    /// The front pair's axle, or the single rear wheel's.
    fn axle(&self, paired: bool) -> AxleLine {
        let (at, r) = self
            .wheels()
            .into_iter()
            .find(|(at, _)| (at[2] > 0.0) == paired)
            .expect("a cyclecar has a front pair and a rear wheel");
        AxleLine {
            z: at[2],
            y: at[1],
            r,
            w: r * TYRE_W,
        }
    }
}

/// The cyclecar's plan for a blueprint: the blueprint's pod half-width,
/// wheels and track scaled to her, and its beltline raised to its height -
/// the pod's crown is her roof.
pub(super) fn plan_of(bp: &SkiffBlueprint) -> CyclecarPlan {
    let bp = SkiffBlueprint {
        body_w: bp.body_w * POD_W,
        wheel_r: bp.wheel_r * WHEEL,
        track: bp.track * TRACK,
        beltline: bp.height,
        ..*bp
    };
    CyclecarPlan {
        plan: BodyPlan::new(&bp, SECTION, POD, AXLES),
    }
}

/// A plan the family built, as a cyclecar's - the [`SkiffCraft`] seam
/// carries only the shared [`BodyPlan`].
fn with_plan(plan: BodyPlan) -> CyclecarPlan {
    CyclecarPlan { plan }
}

pub(super) struct Cyclecar;

impl SkiffCraft for Cyclecar {
    fn plan(&self, bp: &SkiffBlueprint, _seed: u64) -> BodyPlan {
        plan_of(bp).plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(ctx, &with_plan(*plan))
    }

    fn feel(&self) -> SkiffFeel {
        // The legacy trike's numbers: the nimblest skiff, against the buggy's
        // 0.62 / 11.0 / 2.6 - a placeholder the owner agreed, for #1381's
        // per-type sweep to tune.
        SkiffFeel {
            mass_factor: 0.60,
            drive_accel: 11.5,
            turn_accel: 2.8,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, _seed: u64) -> f32 {
        // The front pair's outboard faces, and the rims standing proud of
        // them; the pod is far narrower than her track.
        plan.track + 2.0 * with_plan(*plan).axle(true).w * RIM_REACH
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, _seed: u64) -> [f32; 3] {
        let plan = with_plan(*plan);
        match aura {
            // She is electric, so `fx::drawn_aura` drops both of these and
            // neither is ever drawn; answered anyway, at the tail tip.
            ParticleAura::Exhaust | ParticleAura::Steam => {
                let tail = plan.tail_z();
                [
                    0.0,
                    plan.crown_at(tail) + plan.at(0.010),
                    tail - plan.at(0.010),
                ]
            }
            // A flourish hovers just over the roof at the window band's
            // middle - over the cabin, never inside it (the roadster
            // hardtop's lesson, #1367).
            _ => {
                let z = plan.at((BAND_RUN.0 + BAND_RUN.1) * 0.5);
                [0.0, plan.crown_at(z) + plan.at(0.040), z]
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Electric
    }
}

/// The cyclecar dressed for the tiers `ctx` carries - what [`Cyclecar::build`]
/// draws, and what the guards sweep every tier through.
///
/// The order is the agreed prototype's, part for part, which is what lets a
/// dump of this tree be checked node by node against it.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &CyclecarPlan) -> Generator {
    let c = cyclecar_colours(ctx);
    // Root: a hidden hub on the datum, buried in the barrel - the roadster's
    // idiom.
    let hub = dim(plan.length * 0.008);
    let mut root = prim(cuboid([hub; 3], c.arm.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    pod::pod(kids, plan, &c);
    pod::band(kids, plan, &c);
    pod::strip(kids, plan, &c);
    running_gear::stub_axles(kids, plan, &c);
    running_gear::fork(kids, plan, &c);
    running_gear::spat(kids, plan, &c);
    running_gear::wheels(kids, plan, &c, ctx.wear != WearTier::Pristine);
    pod::lamps(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness, ctx.wear);
    root
}

/// Where the spat's cut plane stands over the datum (m): [`FAIRING_DROP`] of
/// the wheel's radius under the single wheel's axle - so the tyre's lower
/// part stands out under it on the ground.
pub(super) fn spat_cut_y(plan: &CyclecarPlan) -> f32 {
    let a = plan.axle(false);
    a.y - FAIRING_DROP * a.r
}
