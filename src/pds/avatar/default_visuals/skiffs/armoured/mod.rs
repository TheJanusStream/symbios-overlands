//! The armoured car: a stepped plate hull under an octagonal turret, on four
//! wheels behind boxy arches. The machine of the modern martial themes -
//! PostApoc, IndustrialPark and ModernCity (#1375).
//!
//! She is an avatar, not a weapon, and carries **no gun anywhere**: the
//! review complaint the epic opened on was that everything read as a gun. The
//! mass, the hatch and the commander's cupola are what say "armoured car"
//! from the chase camera, which looks DOWN at 22.9 degrees and so sees her
//! roof before anything else.
//!
//! Two variants by theme ([`ArmouredVariant`]): the **works** machine on
//! IndustrialPark and ModernCity, and the PostApoc **raider**, who bolts two
//! applique slabs on her driver's plate, one on her near flank and a pile of
//! scavenged kit on her rear deck. Without it a PostApoc armoured car is a
//! municipal wagon in sand paint; and the applique the prototype first drew
//! on her glacis did not register from the stern quarter at all.
//!
//! [`hull`] draws the plates, the glacis, the vision slits, the lamps and the
//! unit flash; [`turret`] the drum, the hatch, the cupola and the band round
//! its base; [`running_gear`] the arches, the axle beams, the wheels, the
//! spare and the exhaust; and [`dressing`] the ladder of secondary masses by
//! ornateness and wear.
//!
//! # She is plate, and that is a finding rather than a taste
//!
//! **A swept or turned shape cannot be faceted.** A Spine pushes a RADIAL
//! ring normal at every station and a Lathe a radial revolve normal at every
//! profile point, whatever the resolution - so a res-6 hull has a hexagonal
//! silhouette and smooth barrel SHADING, and a res-8 turret is a dome with
//! eight edges. At 12 m the shading is what reads, and it read as a van with
//! a bump on it. So she is the first type in this epic to leave the
//! swept-and-turned vocabulary: her hull, her turret, her arches and her
//! plates are [`plate`]s and one [`ramp`], whose faces are planar and whose
//! edges are hard.
//!
//! A Bevel with ONE bevel segment is an octagonal prism, and `taper` /
//! `taper_bottom` turn it into a frustum. So one node is an armoured hull
//! plate with sloped flanks, and one node is an octagonal turret.
//!
//! # The plan idiom, for a box
//!
//! The family's [`BodyPlan`] fits her with no new field, and her plan form is
//! a box's: ONE half-width from the tail to the nose, because a box is a box
//! and the spare wheel bolted to her stern plate fills the rest of her
//! length. The DATUM is mid-hull, so [`BodyPlan::depth`] is half the hull's
//! height and the collider comes out LOW AND WIDE - the #804 shape by a wide
//! margin.
//!
//! **The section is DERIVED, not typed**: the roof stands at [`ROOF`] of the
//! blueprint's height and the floor at [`BELLY`] of the wheel radius, and the
//! section that puts them both there falls out of the two. That one
//! arithmetic is what holds her ground clearance and her turret's height at
//! every blueprint corner; with a constant section the clearance swings 3.4x
//! across the six.
//!
//! Every plate reproduces the section the plan publishes exactly -
//! `taper_bottom` 0.5 under the datum and `taper` 0.5 over it draw the same
//! hexagon a res-6 sweep of the same plan would - so every mount reads ONE
//! function, [`ArmouredPlan::flank_x`], and is bedded against the drawn flank
//! at its own height. Four arches floated at the first render because they
//! were cut to the widest line, and above the chine the section has already
//! drawn in.
//!
//! The shape was prototyped in generator JSON and agreed by the owner on its
//! renders before any of this was written (#1375, #1359 rules 1, 12 and 14):
//! `target/dump/vehicles2026-09/armoured/armoured.py` is its python twin, and
//! the port was checked node for node against it.

mod dressing;
mod hull;
mod running_gear;
mod turret;

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::livery::armoured_colours;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ArmouredVariant, ParticleAura, SkiffBlueprint, WearTier};

use super::super::Propulsion;
use super::super::common::{cuboid, id_quat, prim, quat_x, quat_y, quat_z};
use super::plan::{Axle, BodyPlan};
// The family's shape vocabulary, imported here so each part's
// `use super::{plate, ..}` resolves.
use super::shape::{board, line, plate, ramp, rim_profile, solid, tapered_plate, tyre_profile};
use super::{SkiffCraft, SkiffFeel, dim};

/// Her hull's half-width over the blueprint's body half-width: an armoured
/// car is wide for her length, and the arches stand outboard of that again.
const HULL_W: f32 = 1.36;

/// The hull's roof over the blueprint's HEIGHT, and its floor over the wheel
/// radius - the two numbers the section is derived from (see the module
/// docs). The turret and the cupola on it reach the blueprint's height.
const ROOF: f32 = 0.66;
const BELLY: f32 = 0.72;

/// The wheels' radius over the blueprint's, and her track over the
/// blueprint's.
///
/// 0.82 rather than the blueprint's own is the whole proportion of her: at
/// 1.10 the wheels were 67 % of the machine's height and she read as a
/// go-kart under panels. At 0.82 a wheel is 0.19 of her length in diameter,
/// which is the real proportion, and the hull, the arches and the turret fall
/// into place around them.
const WHEEL: f32 = 0.82;
const TRACK: f32 = 1.00;

/// A tyre's half-width over its radius, on every wheel.
const TYRE_W: f32 = 0.30;

/// How far an arch reaches outboard, over the tyre's half-width: the boxy
/// arches stand outboard of the tyres, and they are what she is widest at.
const ARCH_OUT: f32 = 1.30;

/// The hull's plan form, `(z fraction of the length, half-width fraction)`,
/// tail to nose.
///
/// Parallel-sided, because a box is a box: every plate holds one half-width
/// over its whole run, and so every mount reads the same number the plates
/// are cut to. The tail stops short of the machine's own length - the SPARE
/// WHEEL bolted to the stern plate fills the last 0.06 of it, so she is
/// exactly her blueprint's length including it.
const BOX: &[(f32, f32)] = &[(-0.440, 1.00), (0.500, 1.00)];

/// Four wheels on two paired axles. Six is the ROVER's one identity (#1378),
/// and two types that differed only in their wheel count would be the
/// arrangement problem this epic replaced.
const AXLES: &[Axle] = &[
    Axle {
        at: 1.0,
        paired: true,
        radius: 1.0,
    },
    Axle {
        at: -1.0,
        paired: true,
        radius: 1.0,
    },
];

/// The glacis wedge's run along the machine (of the length), how much of that
/// run is buried in the hull's nose, and its height over the upper hull's.
///
/// **The glacis is the LOWER nose only.** A wedge as tall as the hull can
/// only meet its front face at mid-height, which leaves the ramp's lower half
/// hanging out in front as a wide thin tongue at shin height: a snowplough.
/// It stands on the belly tub's roof - the datum - with the driver's plate
/// above it and the tub's own vertical bow plate under it.
const GLACIS_RUN: f32 = 0.100;
const GLACIS_BED: f32 = 0.25;
const GLACIS_H: f32 = 0.62;

/// The turret's station (of the length), its base radius over the hull's
/// half-width, how far its base is sunk into the roof (of the length), its
/// top radius over its base, and the share of its height the cupola stands
/// above its roof.
const TURRET_Z: f32 = -0.070;
const TURRET_R: f32 = 0.62;
const TURRET_BED: f32 = 0.045;
const TURRET_TAPER: f32 = 0.80;
const CUPOLA_SHARE: f32 = 0.24;

/// The side the chase camera's usual quarter shows: her bedroll, her
/// jerrycans, her exhaust, the raider's flank slab and a worn machine's odd
/// rim are all on it.
const NEAR: f32 = 1.0;

/// No rotation at all - the authoring frame's own. A plate cut square to the
/// machine, and a Wedge placed nose-forward, which IS a glacis untouched.
const NO_TURN: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Which side of the centreline `x` is on.
fn side(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// A turned part laid with its axis outboard along x, on either side.
fn outboard(x: f32) -> [f32; 4] {
    quat_z(-side(x) * FRAC_PI_2)
}

/// A turned part laid with its axis along `+z`, facing ahead.
fn along_z() -> [f32; 4] {
    quat_x(FRAC_PI_2)
}

/// One axle of the armoured car: its station, its wheels' centre over the
/// datum, their radius and their tyres' half-width (m).
#[derive(Clone, Copy, Debug)]
struct AxleLine {
    z: f32,
    y: f32,
    r: f32,
    w: f32,
}

/// The armoured car's plan: the family's [`BodyPlan`] over her own blueprint
/// copy, and the variant drawn on it. It derefs to the plan, so every shared
/// read looks the same as on a car, and it adds the reads her plates and her
/// dressing share - so the dressing is bedded against the numbers the plates
/// are cut to.
#[derive(Clone, Copy, Debug)]
pub(super) struct ArmouredPlan {
    plan: BodyPlan,
    variant: ArmouredVariant,
}

impl std::ops::Deref for ArmouredPlan {
    type Target = BodyPlan;
    fn deref(&self) -> &BodyPlan {
        &self.plan
    }
}

impl ArmouredPlan {
    /// A station along the machine (m), from a fraction of the length.
    fn at(&self, fraction: f32) -> f32 {
        fraction * self.length
    }

    /// A point on the UPPER flank facet at `z`, `t` from the chine (0) to the
    /// roof's edge (1): `(x, y)` with x positive.
    ///
    /// The hexagon's top run is half its widest, so the facet runs from
    /// `(hw, 0)` to `(hw/2, crown)` - and anything laid on the flank is
    /// placed here and turned to [`Self::flank_tilt`].
    fn flank(&self, z: f32, t: f32) -> (f32, f32) {
        (
            self.half_width_at(z) * (1.0 - 0.5 * t),
            self.crown_at(z) * t,
        )
    }

    /// The hexagon's own x at station `z` and height `y` (m) - where the
    /// DRAWN flank stands.
    ///
    /// Inside the round tube the connectedness guard models a sweep as, so
    /// anything bedded against this is bedded against both. Bed against it at
    /// the part's OWN height: above the chine the section has already drawn
    /// in, and an arch or a bin cut to the widest line hangs in the air.
    fn flank_x(&self, z: f32, y: f32) -> f32 {
        let crown = self.crown_at(z);
        let t = (y.abs() / crown.max(1e-6)).min(1.0);
        self.half_width_at(z) * (1.0 - 0.5 * t)
    }

    /// The upper flank facet's outward normal, as an angle above the
    /// horizontal (rad).
    fn flank_tilt(&self) -> f32 {
        0.5f32.atan2(self.section)
    }

    /// A part lying ON the upper flank facet is turned to its tilt, on
    /// whichever side it sits.
    fn on_flank(&self, s: f32) -> [f32; 4] {
        let tilt = FRAC_PI_2 - self.flank_tilt();
        if s > 0.0 {
            quat_z(-tilt)
        } else {
            quat_z(-tilt + std::f32::consts::PI)
        }
    }

    /// The DRAWN top at station `z` (m over the datum).
    ///
    /// Abaft the fighting compartment the deck is the REAR DECK's, not the
    /// crown - and a primer patch or a kit pile written against `crown_at`
    /// there floats over open air.
    fn top_y(&self, z: f32) -> f32 {
        if z < self.at(hull::CASE_Z0) {
            self.crown_at(z) * hull::CASE_BASE
        } else {
            self.crown_at(z)
        }
    }

    /// Half the width of that drawn top (m).
    fn top_half(&self, z: f32) -> f32 {
        if z < self.at(hull::CASE_Z0) {
            self.half_width_at(z) * (1.0 - hull::DECK_TAPER)
        } else {
            self.half_width_at(z) * hull::CASE_W * (1.0 - hull::CASE_TAPER[0])
        }
    }

    /// The hull's own front face - the DRIVER'S PLATE. The glacis stands
    /// ahead of it and carries the machine out to [`BodyPlan::nose_z`].
    fn nose(&self) -> f32 {
        self.nose_z() - self.at(GLACIS_RUN) * (1.0 - GLACIS_BED)
    }

    /// The front axle, or the rear one.
    fn axle(&self, front: bool) -> AxleLine {
        let (at, r) = self
            .wheels()
            .into_iter()
            .find(|(at, _)| (at[2] > 0.0) == front)
            .expect("an armoured car has a front and a rear axle");
        AxleLine {
            z: at[2],
            y: at[1],
            r,
            w: r * TYRE_W,
        }
    }
}

/// The armoured car's plan for a blueprint, as `variant`: her hull
/// half-width, her wheels and her track over the blueprint's, with the
/// section derived from the roof and the floor she is to stand between.
pub(super) fn plan_of(bp: &SkiffBlueprint, variant: ArmouredVariant) -> ArmouredPlan {
    let wheel_r = bp.wheel_r * WHEEL;
    let roof = bp.height * ROOF;
    let floor = wheel_r * BELLY;
    let half_w = bp.body_w * HULL_W * 0.5;
    let bp = SkiffBlueprint {
        body_w: bp.body_w * HULL_W,
        wheel_r,
        track: bp.track * TRACK,
        // The datum is mid-hull, so the beltline IS the roof.
        beltline: roof,
        ..*bp
    };
    with_variant(
        BodyPlan::new(&bp, (roof - floor) * 0.5 / half_w, BOX, AXLES),
        variant,
    )
}

/// A plan the family built, with its variant laid back on it - the
/// [`SkiffCraft`] seam carries only the shared [`BodyPlan`].
pub(super) fn with_variant(plan: BodyPlan, variant: ArmouredVariant) -> ArmouredPlan {
    ArmouredPlan { plan, variant }
}

/// Where the exhaust pipe's mouth is (root-local, m): what every aura she
/// trails leaves from.
pub(super) fn pipe_mouth(plan: &ArmouredPlan) -> [f32; 3] {
    running_gear::pipe_mouth(plan)
}

pub(super) struct Armoured;

impl SkiffCraft for Armoured {
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan {
        plan_of(bp, ArmouredVariant::for_seed(seed)).plan
    }

    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator {
        build_dressed(
            ctx,
            &with_variant(*plan, ArmouredVariant::for_seed(ctx.seed)),
        )
    }

    fn feel(&self) -> SkiffFeel {
        // The brief's own armoured numbers: the heaviest and least nimble
        // skiff, against the roadster's 1.0 / 8.9 / 2.0 - a placeholder the
        // owner agreed, for #1381's per-type sweep to tune. Note for that
        // sweep: she is the first type to reach the family's 1500 kg mass
        // ceiling, and twelve of the 65 seeds under 3000 clamp at it, so a
        // mass factor much over this stops moving the long half of her
        // population at all.
        SkiffFeel {
            mass_factor: 1.55,
            drive_accel: 6.5,
            turn_accel: 1.7,
        }
    }

    fn overall_width(&self, plan: &BodyPlan, seed: u64) -> f32 {
        // The boxy arches, which stand outboard of the tyres.
        let plan = with_variant(*plan, ArmouredVariant::for_seed(seed));
        plan.track + 2.0 * plan.axle(true).w * ARCH_OUT
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3] {
        // Every aura she can pick leaves the pipe's mouth, and there is no
        // fold in `fx::drawn_aura` for her: a diesel has a radiator, so
        // unlike the buggy's air-cooled flat four there is nothing to justify
        // folding an IndustrialPark seed's steam away - a white plume off a
        // heavy diesel's pipe is what that theme's steam already looks like -
        // and a raider's straight pipe throwing a PostApoc theme's embers is
        // a gift.
        pipe_mouth(&with_variant(*plan, ArmouredVariant::for_seed(seed)))
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Diesel
    }
}

/// The armoured car as `plan`'s variant, dressed for the tiers `ctx`
/// carries: what [`Armoured::build`] draws with the seed's own variant, and
/// what the guards sweep every variant through.
///
/// The order is the agreed prototype's, part for part, which is what lets a
/// dump of this tree be checked node by node against it.
pub(super) fn build_dressed(ctx: &PartCtx, plan: &ArmouredPlan) -> Generator {
    let c = armoured_colours(ctx);
    // Root: a hidden hub on the datum, buried in the plate - the roadster's
    // idiom.
    let hub = dim(plan.length * 0.008);
    let mut root = prim(cuboid([hub; 3], c.arm.clone()), [0.0; 3], id_quat());
    let kids = &mut root.children;
    hull::plates(kids, plan, &c);
    hull::glacis(kids, plan, &c);
    turret::turret(kids, plan, &c, ctx.ornateness);
    hull::slits(kids, plan, &c);
    running_gear::arches(kids, plan, &c);
    running_gear::axle_beams(kids, plan, &c);
    running_gear::wheels(kids, plan, &c, ctx.wear != WearTier::Pristine);
    running_gear::spare(kids, plan, &c);
    hull::stern_rack(kids, plan, &c);
    running_gear::exhaust(kids, plan, &c);
    hull::lamps(kids, plan, &c);
    hull::unit_flash(kids, plan, &c);
    turret::band(kids, plan, &c);
    dressing::dress(kids, plan, &c, ctx.ornateness, ctx.wear);
    root
}
