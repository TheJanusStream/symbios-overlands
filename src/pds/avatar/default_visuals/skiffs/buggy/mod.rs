//! The dune buggy: an open tube frame over a small seat pod, an air-cooled
//! flat four hung out behind, and fat tyres standing clear of a narrow
//! frame. The light machine of the leisure and frontier themes (#1374).
//!
//! Three variants, each picked by theme ([`BuggyVariant`]):
//!
//! - the sand **rail**, the bare frame (Roadside, SportsRec);
//! - the **beach** buggy, the rail under a striped canopy on every tier
//!   (CoastalResort, Suburban);
//! - the desert **raider**, the rail with a spare wheel on her roll bar, two
//!   jerrycans on her near nerf bar and a diagonal across her main hoop on
//!   every tier (PostApoc, WildWest).
//!
//! [`frame`] draws the tube frame, the backbone that is her root, and her
//! lamps; [`cockpit`] the pod and the seats; [`engine`] the flat four and its
//! swept stinger; [`running_gear`] the suspension and the wheels; and
//! [`dressing`] the ladder of secondary masses by ornateness and wear.
//!
//! # The plan idiom, for a frame
//!
//! The family's [`BodyPlan`] fits a frame with no new field. The DATUM is
//! the mid-height of the frame's side, so [`BodyPlan::crown_at`] IS the
//! shoulder rail and [`BodyPlan::sill_at`] the floor rail, and both close in
//! toward the nose and the tail by the plan's own coupling - a sand rail's
//! nose really is a wedge. The depth is half the side's height, so the
//! collider is the roadster's shape rather than a tower. The ROOT is the
//! backbone, a Spine along the floor whose points are its own: the travel
//! pose overwrites the root's translation, and a hub on the datum would hang
//! in the open cockpit. The two axles stand at their own radii - big fat
//! rears and smaller fronts - so every tyre meets the ground.
//!
//! # Joints by construction
//!
//! The connectedness guard registers a joint between two thin tubes only
//! where an end ring of one lies inside the other. So no member is fattened
//! at a joint: every one is drawn THROUGH joints another member also runs
//! through - the floor rail through nine - and ends on a centreline, and a
//! mass hung between joints (the light bar, the spare's bracket) is placed
//! ON the tube's own drawn centreline by sampling the Catmull-Rom the mesher
//! draws it with. Shorten a rail to its ends and the machine falls apart.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1374, #1359 rules 1, 12 and 14):
//! `target/dump/vehicles2026-09/buggy/buggy.py` is its python twin, and the
//! port was checked node for node against it.

mod cockpit;
mod dressing;
mod engine;
mod frame;
mod running_gear;

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::buggy_colours;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BuggyVariant, ParticleAura, SkiffBlueprint, WearTier};

use super::super::Propulsion;
use super::super::common::{quat_x, quat_z};
use super::plan::{Axle, BodyPlan};
// The family's shape vocabulary, imported here so each part's
// `use super::{line, ..}` resolves.
use super::shape::{board, line, rim_profile, solid, sweep, tyre_profile};
use super::{Shiver, SkiffCraft, SkiffFeel, SkiffIdle, dim};
use frame::Frame;

/// The frame's depth per unit half-width - see [`BodyPlan::section`]. As
/// deep as the roadster's body, so her collider is the roadster's shape.
const SECTION: f32 = 0.78;

/// The rear tyre's radius over the blueprint's wheel - the plan's own wheel
/// radius, lifting the blueprint's 0.110-0.125 of the length to 0.123-0.140 -
/// and the front tyre's over the rear's.
const REAR: f32 = 1.12;
const FRONT: f32 = 0.80;

/// A tyre's half-width over its radius: a fat paddle-era rear, a narrower
/// front.
const REAR_W: f32 = 0.38;
const FRONT_W: f32 = 0.26;

/// How far the rims stand out from the machine's centre, over the rear
/// tyre's half-width: they stand proud of both of its end caps.
const RIM_REACH: f32 = 1.12;

/// The frame's plan form, `(z fraction of the length, half-width
/// fraction)`, tail to nose: the engine guard, the rear cradle over the
/// transaxle, the cockpit full width between the main hoop and the dash, the
/// front suspension bay, the nose.
const FRAME: &[(f32, f32)] = &[
    (-0.490, 0.86),
    (-0.320, 0.82),
    (-0.200, 1.00),
    (0.120, 1.00),
    (0.300, 0.55),
    (0.440, 0.28),
];

/// The main hoop behind the seats and the front hoop at the dash, as z
/// fractions of the length.
const MAIN_HOOP: f32 = -0.200;
const FRONT_HOOP: f32 = 0.120;

/// The cage's top bar over the datum, over the roadster's screen height: so
/// the buggy's hoop stands clear above any roadster beside her, and the
/// family never reads her cage as a windscreen.
const CAGE: f32 = 1.35;

/// How far back the front hoop's top is raked, as a fraction of the length.
const RAKE: f32 = 0.070;

/// The cage's tube radius, and the rails' and braces', as fractions of the
/// length.
const TUBE: f32 = 0.0110;
const RAIL: f32 = 0.0100;

/// The engine's scale over a stock flat four's proportions: at stock size it
/// was a small black box between two tyres, invisible from behind.
const ENGINE: f32 = 1.30;

/// Two paired axles, the front one on smaller wheels.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: FRONT,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

/// The side the chase camera's usual quarter shows: the jerrycans, the dune
/// whip, the taped seat and the mismatched rim are all on it.
const NEAR: f32 = 1.0;

const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Which side of the centreline `x` is on - `+0.0` the right, as the twin
/// has it.
fn side(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// A turned part laid with its axis outboard along x, on either side.
fn outboard(x: f32) -> [f32; 4] {
    quat_z(-side(x) * FRAC_PI_2)
}

/// A turned part laid with its axis along +z, facing ahead.
fn along_z() -> [f32; 4] {
    quat_x(FRAC_PI_2)
}

/// A tyre's half-width (m) at radius `r` on the axle at station `z`: fat at
/// the back, narrower in front.
fn tyre_half_width(z: f32, r: f32) -> f32 {
    r * if z > 0.0 { FRONT_W } else { REAR_W }
}

/// One axle of the buggy: its station, its wheels' centre over the datum,
/// their radius and their tyres' half-width (m).
#[derive(Clone, Copy, Debug)]
struct AxleLine {
    z: f32,
    y: f32,
    r: f32,
    w: f32,
}

/// The buggy's plan: the family's [`BodyPlan`] and the variant drawn on it.
/// It derefs to the plan, so every shared read looks the same as on a car.
#[derive(Clone, Copy, Debug)]
pub(super) struct BuggyPlan {
    plan: BodyPlan,
    variant: BuggyVariant,
}

impl std::ops::Deref for BuggyPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl BuggyPlan {
    /// A station along the machine (m), from a fraction of the length.
    fn at(&self, fraction: f32) -> f32 {
        fraction * self.length
    }

    /// The frame rails' half-width at `z` (m) - floored at the family's
    /// minimum, and never negative before the floor: a negative there is a
    /// bug, not a small part (the tug's hidden one, #1370).
    fn hw(&self, z: f32) -> f32 {
        let w = self.half_width_at(z);
        debug_assert!(w > 0.0, "the frame's half-width at {z} is {w}");
        dim(w)
    }

    /// The shoulder rail's and the floor rail's height at `z` (m over the
    /// datum) - the plan's crown and sill.
    fn shoulder(&self, z: f32) -> f32 {
        self.crown_at(z)
    }
    fn floor(&self, z: f32) -> f32 {
        self.sill_at(z)
    }

    /// The cage's top bar over the datum (m).
    fn cage_top(&self) -> f32 {
        self.screen_top() * CAGE
    }
}

/// The front or the rear axle of a buggy's plan.
fn axle(plan: &BodyPlan, front: bool) -> AxleLine {
    let (at, r) = plan
        .wheels()
        .into_iter()
        .find(|(at, _)| (at[2] > 0.0) == front)
        .expect("a buggy has a front and a rear axle");
    AxleLine {
        z: at[2],
        y: at[1],
        r,
        w: tyre_half_width(at[2], r),
    }
}

/// The buggy's plan for a blueprint, as `variant`.
pub(super) fn plan_of(bp: &SkiffBlueprint, variant: BuggyVariant) -> BuggyPlan {
    let plan = BodyPlan::new(bp, SECTION, FRAME, AXLES);
    with_variant(plan.on_wheels(plan.wheel_r * REAR), variant)
}

/// A plan the family built, with its variant laid back on it - the
/// [`SkiffCraft`] seam carries only the shared [`BodyPlan`].
pub(super) fn with_variant(plan: BodyPlan, variant: BuggyVariant) -> BuggyPlan {
    BuggyPlan { plan, variant }
}

/// Where the stinger ends (root-local, m): the mouth every aura but a
/// flourish issues from.
pub(super) fn stinger_mouth(plan: &BuggyPlan) -> [f32; 3] {
    Frame::new(plan).engine.mouth()
}

pub(super) struct Buggy;

impl SkiffCraft for Buggy {
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan {
        plan_of(bp, BuggyVariant::for_seed(seed)).plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(ctx, &with_variant(*plan, BuggyVariant::for_seed(ctx.seed)))
    }

    fn feel(&self) -> SkiffFeel {
        // #1381's sweep, agreed by the owner on 2026-09-20. Her straight
        // line was right and is untouched: 49.6 km/h. What moved is the
        // turn - light, short-wheelbase and with enormous grip, she is the
        // darty one, and at 48.7 deg/s she was cornering like the roadster.
        // turn_accel 2.6 -> 3.6 and angular_damping 4.0 -> 3.5 give
        // 76.9 deg/s and a circle of 19.0 m = 7.0 of her own lengths,
        // against the roadster's 11.8.
        SkiffFeel {
            mass_factor: 0.62,
            drive_accel: 11.0,
            turn_accel: 3.6,
            linear_damping: 0.8,
            angular_damping: 3.5,
        }
    }

    fn idle(&self) -> SkiffIdle {
        // `Propulsion::AirCooled`: a flat-four shakes, and shakes fast. The
        // tremble is half again the roadster's and the pace is 11 Hz.
        // 14 degrees of lean - she leans IN, but a tall-sprung buggy on
        // balloon tyres does not lean as far as a low sports car.
        SkiffIdle {
            shiver: Some(Shiver {
                amplitude: 1.4,
                hz: 11.0,
            }),
            bank_degrees: 14.0,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, _seed: u64) -> f32 {
        // The widest thing she draws: the rear tyres, and the rims standing
        // proud of their outboard faces.
        plan.track + 2.0 * axle(plan, false).w * RIM_REACH
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3] {
        let plan = &with_variant(*plan, BuggyVariant::for_seed(seed));
        match aura {
            // Exhaust, a folded steam and a frontier theme's sparks all leave
            // the stinger's mouth: the pipe that is drawn, facing the chase
            // camera.
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers => {
                stinger_mouth(plan)
            }
            // A flourish - which none of her themes draws - hovers over the
            // seats inside the cage, where a person would be.
            _ => {
                let f = Frame::new(plan);
                [0.0, f.depth * 1.5, (f.main_z + f.dash_z) * 0.5]
            }
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::AirCooled
    }
}

/// The buggy as `plan`'s variant, dressed for the tiers `ctx` carries - what
/// [`Buggy::build`] draws with the seed's own variant, and what the guards
/// sweep every variant through.
///
/// The order is the agreed prototype's, part for part, which is what lets a
/// dump of this tree be checked node by node against it.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &BuggyPlan) -> Generator {
    let c = buggy_colours(ctx, plan.variant);
    let f = Frame::new(plan);
    let battered = ctx.wear == WearTier::Battered;
    let mut root = frame::backbone(&f, &c);
    let kids = &mut root.children;
    frame::build(kids, &f, &c);
    cockpit::build(kids, &f, &c, battered);
    engine::build(kids, &f, &c, battered);
    running_gear::suspension(kids, &f, &c);
    running_gear::wheels(kids, &f, &c, ctx.wear != WearTier::Pristine);
    frame::lamps(kids, &f, &c);
    dressing::dress(kids, &f, &c, ctx.ornateness);
    root
}
