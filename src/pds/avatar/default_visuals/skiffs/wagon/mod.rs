//! The horseless wagon: the land craft of a world without engines, and a
//! third of the skiff family (#1377).
//!
//! It takes every historic theme, so one body would put a Japanese court and
//! a Victorian undertaker in the same farm cart. Five bodies, each behind the
//! [`Bodywork`] trait and picked by theme ([`WagonBody`]):
//!
//! - [`cart`]: a plank box on four wheels, the front pair smaller, a sprung
//!   bench, and a canvas tilt on swept bows from Adorned up - the floor;
//! - [`buckboard`]: a low flat bed between high wheels, a sprung seat on a
//!   riser, a dash board (WildWest);
//! - [`hearse`]: a tall glazed body on the cart's running gear, lanterns lit
//!   on every tier (GothicHorror);
//! - [`chariot`]: two wheels, a turned breastwork open at the back, a pole
//!   (AncientClassical);
//! - [`oxcart`]: two big wheels under a lacquered cabin with a swept roof and
//!   upswept eaves, two shafts (FeudalJapan's gissha).
//!
//! Shared by all of them: [`running_gear`] (the open spoked wheels and the
//! axles and bolsters the bed stands on), [`bed`] (the plank box, the sprung
//! bench, the toe board, the tilt, the lanterns) and [`dressing`] (the ladder
//! of secondary masses by ornateness and wear).
//!
//! # The plan idiom, unchanged
//!
//! A wagon is drawn on the family's [`BodyPlan`] with a flat plan form: the
//! DATUM is the bed floor and the crown is the top of the sides (the seat's
//! top on a buckboard, whose sides are low), so [`BodyPlan::datum_height`],
//! the travel drop and the collider depth are the roadster's arithmetic. A
//! wagon's own stations - the bed's extent and the bench - live in
//! [`WagonPlan`], as the roadster's cowl and radiator live in its own.
//!
//! The wheels are HIGH (0.30-0.34 of the length across) and the front pair is
//! smaller - the proportion that says wagon rather than car at 12 m - so each
//! axle carries its own radius. A two-wheeler is one axle with the body
//! centred ON it: that is the balance point, and it holds at every wheelbase.
//!
//! # Why a spoked wheel can be open
//!
//! A Lathe whose profile ends at a non-zero radius is capped by a solid disc,
//! which is why the roadster's tyres are drums (#1359). But a Lathe bored with
//! `hollow` gets a proportional inner shell and ANNULAR end caps, so a
//! two-point band bored to the felloe's depth is a ring you can see through:
//! the ground shows between the spokes at play distance.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1377, #1359 rules 1, 12, 14):
//! `target/dump/vehicles2026-09/wagon/wagon.py` is its python twin, and the
//! port was checked node for node against it.

mod bed;
mod buckboard;
mod cart;
mod chariot;
mod dressing;
mod hearse;
mod oxcart;
mod running_gear;

use bevy::math::Quat;

use crate::pds::avatar::livery::{WagonColours, wagon_colours};
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{OrnatenessTier, ParticleAura, SkiffBlueprint, WagonBody, WearTier};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim, spine, with_cut};
use super::plan::{Axle, BodyPlan};
// The family's shape vocabulary (#1374), imported here so every body's
// `use super::{board, line, ..}` still resolves.
use super::shape::{board, line, solid, turned};
use super::{SkiffCraft, SkiffFeel, dim};

/// A wagon wheel is HIGH: 0.30-0.34 of the length across (the brief). The
/// blueprint's wheel fraction is 0.110-0.125 of the length, so this maps that
/// band onto 0.150-0.170 exactly, and a seed's limb thickness still decides
/// where in it she sits.
const HIGH_WHEEL: f32 = 1.36;

/// A four-wheeler's front wheels over its rear: small enough to turn under
/// the bed, which is the one proportion that reads as a wagon at 12 m.
const FRONT_WHEEL: f32 = 0.78;

/// The side the chase camera's usual quarter shows: the cask and a worn
/// cart's replacement board hang there.
const NEAR: f32 = 1.0;

/// A wagon's plan form: flat, the bed's full width end to end. Its stations
/// only have to make the shared [`BodyPlan`] arithmetic hold.
const FLAT: &[(f32, f32)] = &[(-0.5, 1.0), (0.5, 1.0)];

/// Where a body's bed lies along the machine.
#[derive(Debug)]
enum Bed {
    /// Its after and forward ends, as z fractions of the length.
    Fixed(f32, f32),
    /// Centred on the (one) axle, this half-length either way as a fraction
    /// of the length - a two-wheeler's balance point, whatever the wheelbase.
    OnAxle(f32),
}

/// What a body arranges along its plan.
#[derive(Debug)]
struct Form {
    /// Side height per unit half-width - see [`BodyPlan::section`].
    section: f32,
    bed: Bed,
    /// The bench's station as a z fraction of the length, where there is one.
    seat: Option<f32>,
    axles: &'static [Axle],
}

const FOUR_WHEELS: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: FRONT_WHEEL,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

/// The wagon's plan: the family's [`BodyPlan`] and the body's own stations.
/// It derefs to the plan, so every shared read looks the same as on a car.
#[derive(Clone, Copy, Debug)]
pub(super) struct WagonPlan {
    plan: BodyPlan,
    body: WagonBody,
    /// The bed's after and forward ends (m).
    bed: (f32, f32),
    /// The bench's station (m), if the body has one.
    seat: Option<f32>,
}

impl std::ops::Deref for WagonPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl WagonPlan {
    /// The bed's after and forward ends (m).
    fn bed_z(&self) -> (f32, f32) {
        self.bed
    }

    /// The bench's station (m). Only the bodies that carry a bench ask.
    fn seat_z(&self) -> f32 {
        self.seat
            .expect("a body with a bench publishes its station")
    }

    /// The bed floor's thickness (m).
    fn floor_t(&self) -> f32 {
        dim(self.length * 0.014)
    }
}

/// The body a [`WagonBody`] is drawn as - the one match over it.
fn bodywork(body: WagonBody) -> &'static dyn Bodywork {
    match body {
        WagonBody::Cart => &cart::Cart,
        WagonBody::Buckboard => &buckboard::Buckboard,
        WagonBody::Hearse => &hearse::Hearse,
        WagonBody::Chariot => &chariot::Chariot,
        WagonBody::OxCart => &oxcart::OxCart,
    }
}

/// One wagon body.
trait Bodywork {
    /// What it arranges along its plan.
    fn form(&self) -> &'static Form;

    /// Draw it onto `kids` for these tiers, ladder included.
    fn build(
        &self,
        kids: &mut Vec<Generator>,
        plan: &WagonPlan,
        c: &WagonColours,
        o: OrnatenessTier,
        w: WearTier,
    );

    /// How far a nave stands proud of its wheel's plane, over the wheel's
    /// radius - what the drawn width is measured to.
    fn nave(&self) -> f32 {
        running_gear::NAVE_HALF_LENGTH
    }

    /// Where a lit lantern hangs on these tiers (m), if one does - where an
    /// ember aura issues from.
    fn lantern(&self, plan: &WagonPlan, o: OrnatenessTier) -> Option<[f32; 3]>;

    /// Over the seat, or the cabin: where a decorative aura hovers, which is
    /// where a person would be.
    fn perch(&self, plan: &WagonPlan) -> [f32; 3];
}

/// The wagon's plan for a blueprint on a named body.
pub(super) fn plan_of(bp: &SkiffBlueprint, body: WagonBody) -> WagonPlan {
    let form = bodywork(body).form();
    let plan = BodyPlan::new(bp, form.section, FLAT, form.axles);
    let plan = plan.on_wheels(plan.wheel_r * HIGH_WHEEL);
    with_body(plan, body)
}

/// A plan the family built, with its body's stations laid back along it -
/// the [`SkiffCraft`] seam carries only the shared [`BodyPlan`].
fn with_body(plan: BodyPlan, body: WagonBody) -> WagonPlan {
    let form = bodywork(body).form();
    let l = plan.length;
    let bed = match form.bed {
        Bed::Fixed(aft, fwd) => (aft * l, fwd * l),
        Bed::OnAxle(half) => {
            let z = plan.axle_stations()[0];
            (z - half * l, z + half * l)
        }
    };
    WagonPlan {
        plan,
        body,
        bed,
        seat: form.seat.map(|s| s * l),
    }
}

pub(super) struct Wagon;

impl SkiffCraft for Wagon {
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan {
        plan_of(bp, WagonBody::for_seed(seed)).plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(ctx, &with_body(*plan, WagonBody::for_seed(ctx.seed)))
    }

    fn feel(&self) -> SkiffFeel {
        // Heavier and slower than the roadster's 1.0 / 8.9 / 2.0 - a
        // placeholder the owner agreed, for #1381's per-type sweep to tune.
        SkiffFeel {
            mass_factor: 1.15,
            drive_accel: 7.0,
            turn_accel: 1.7,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, seed: u64) -> f32 {
        // Wheels outboard of a narrow bed, and their naves proud of them: the
        // widest thing on a wagon is a rear nave.
        let biggest = plan.wheels().iter().map(|&(_, r)| r).fold(0.0f32, f32::max);
        plan.track + 2.0 * biggest * bodywork(WagonBody::for_seed(seed)).nave()
    }

    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3] {
        let body = WagonBody::for_seed(seed);
        let plan = &with_body(*plan, body);
        let work = bodywork(body);
        match aura {
            // Sparks from a lantern where one is lit, else over the seat.
            ParticleAura::Embers => work
                .lantern(plan, PartCtx::for_seed(seed).ornateness)
                .unwrap_or_else(|| work.perch(plan)),
            // Exhaust and steam never reach a wagon - the assembler drops
            // them from any craft that is not engine-driven - and every other
            // flourish hovers where a person would be.
            _ => work.perch(plan),
        }
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Rolling
    }
}

/// The wagon on a named body, dressed for the tiers `ctx` carries - what
/// [`Wagon::build`] draws with the seed's own body, and what the guards sweep
/// every body through.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &WagonPlan) -> Generator {
    let c = wagon_colours(ctx, plan.body);
    // Root: a hidden hub on the datum amidships, inside the bed floor (or
    // the pole, on a chariot) whatever the seed - the roadster's reason.
    let hub = plan.floor_t() * 0.7;
    let mut root = prim(cuboid([hub; 3], c.iron.clone()), [0.0; 3], id_quat());
    bodywork(plan.body).build(&mut root.children, plan, &c, ctx.ornateness, ctx.wear);
    root
}

// ---------------------------------------------------------------------------
// The helpers only a wagon builds with - the board, the line and the turned
// parts are the family's, in `super::shape`
// ---------------------------------------------------------------------------

/// A swept half-pipe shaped by its own node scale - the tilt and the ox-cart's
/// roof. The path is PRE-DIVIDED by that scale (the roadster's `sweep`, and
/// the trap it closes: a node scale moves the path as well as the profile).
fn half_pipe(
    points: &[([f32; 3], f32)],
    scale_y: f32,
    hollow: f32,
    m: &SovereignMaterialSettings,
) -> Generator {
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&([x, y, z], r)| ([x, y / scale_y, z], dim(r)))
        .collect();
    let mut node = prim(
        with_cut(spine(&path, 24, m.clone()), [0.0, 0.5], [0.0, 1.0], hollow),
        [0.0; 3],
        id_quat(),
    );
    node.transform.scale = Fp3([1.0, scale_y, 1.0]);
    node
}

/// `v` turned by `q`.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    Quat::from_array(q).mul_vec3(v.into()).into()
}

/// `a` then `b` composed: the rotation that applies `b` first.
fn compose(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    (Quat::from_array(a) * Quat::from_array(b)).to_array()
}
