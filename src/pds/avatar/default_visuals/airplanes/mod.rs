//! The seeded airplane family: one builder per craft type, over one
//! fuselage plan (#1617, epic #1611).
//!
//! The second of the aircraft families, drawn the way the helicopters are
//! (#1614): a craft TYPE is a builder, every line and every mount is read
//! off ONE profile - here the [`FusePlan`], the fuselage a light airplane
//! is built round - and the craft is authored at the size it is drawn at,
//! with no parts, no outfit and no root scale. [`craft`] is the only match
//! over [`AirplaneType`], so a second type (the fan-out, #1618) is a compile
//! error here until it is built.
//!
//! ONE type is built: the high-wing light monoplane taildragger the owner
//! agreed on the phase-5 renders (#1616) - a round-cowled cabin under a
//! strut-braced wing, on fat tyres and a tail wheel - at the proportion
//! they chose (C: a 3.19 m airframe, the fuselage 18 % fuller than a true
//! one's) and with all eleven of that issue's recommendations taken.
//!
//! # Where an airplane sits
//!
//! On her three wheels, tail-down at ten degrees as a taildragger stands,
//! and level in the air (#1616 recommendation 3). Her box is her fuselage
//! ([`FusePlan`]'s header): the wing, the tail surfaces, the gear and the
//! airscrew stand outside it, the wing's 4.7 m span past the 2.6 m gateway
//! mouth, which owner decision 3 of #1611 accepts. Her wheels are the
//! Airplane preset's raycast gear under the box (#1615), so the record
//! carries where the drawn ones stand ([`AirplaneLayout`]): the mains'
//! track and how far ahead they are, the tail wheel's shorter reach that
//! sits her tail-down, and her centre of mass forward over her wing. Her
//! drawn origin is her box's middle, so the travel pose hangs her with no
//! drop at all.
//!
//! # What she does at rest
//!
//! Sits on her wheels, her airscrew idling (a Speed spin, #1616
//! recommendation 5) and turning up with her speed to the fastest a spin
//! may strobe at, which it reaches at her cruise. The Airplane preset has no
//! gait (`GaitMode::for_locomotion`): she banks and pitches as she flies, so
//! her seeded gait writes the airship's heave and nose wander to zero.

mod high_wing;
mod plan;

/// The hero's blade pass (Hz): what her propeller voice thrums at.
pub(super) use high_wing::BLADE_PASS_HZ;
pub(crate) use plan::{FusePlan, FuseProfile};

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{AirplaneBlueprint, AirplaneType, ParticleAura};

/// The airplane family's colours, which live in the fleet's one livery
/// home (#1365) rather than beside its geometry.
pub(crate) use crate::pds::avatar::livery::{AirplaneColours, airplane_colours};

use super::Propulsion;
use super::assemble::apply_travel_pose;

/// The gate every seeded craft drives through - one home, in the module that
/// owns them all (#1382).
#[cfg(test)]
use super::{AIR_DRAFT_CAP, GATEWAY_MOUTH};

/// Smallest dimension anything on an airplane is built at (m): a hair over
/// the sanitiser's own 0.01 m floor, as the helicopter's is, so a part the
/// sanitiser would change is never drawn (#1359 rule 8).
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// How an airplane type flies: what
/// [`airplane_locomotion`](super::airplane_locomotion) scales the Airplane
/// preset's validated numbers by (#1616 recommendation 9: P4's model, its
/// mass and thrust in proportion to her length).
///
/// Build-time only, like the helicopter's: what it writes is
/// `AirplaneParams`, which the record has always carried.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AirplaneFeel {
    /// Mass over the preset's default body at her length, before the size
    /// re-basing: thrust and drag scale with it, so her speeds are the
    /// preset's whatever she weighs.
    pub(super) mass_factor: f32,
}

/// Where a drawn airplane's wheels and centre of mass stand (#1617) - what
/// her record carries so the physics stands her where she is drawn.
///
/// Where each stands along and across her as a share of her box's
/// half-extents, as the record holds them; and how high her box stands over
/// the ground standing as drawn, plumb, as her rays measure it. The reaches
/// that stand her there are the Airplane preset's to work out
/// ([`crate::player::airplane_reach_for_stance`]): its springs' sag, and
/// the share of her weight each wheel carries standing pitched, are its
/// own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AirplaneLayout {
    /// Her collider's half-extents (m).
    pub(super) half_extents: [f32; 3],
    /// The pitch she stands at on her three wheels, nose up (radians).
    pub(super) stance: f32,
    /// How high her box's floor stands over the ground above her mains,
    /// plumb, standing at `stance` (m).
    pub(super) floor_over_mains: f32,
    /// The same above her tail wheel (m).
    pub(super) floor_over_tail: f32,
    /// The mains' half-track over the box's half-width.
    pub(super) gear_track: f32,
    /// How far ahead of the box's middle the mains stand, over its
    /// half-length.
    pub(super) gear_ahead: f32,
    /// How far behind it the tail wheel stands, over its half-length.
    pub(super) tail_wheel_aft: f32,
    /// How far ahead of the box's middle her centre of mass is, over its
    /// half-length.
    pub(super) center_of_mass_ahead: f32,
}

/// One buildable kind of airplane.
pub(super) trait AirplaneCraft {
    /// This type's fuselage at the seeded blueprint's proportions.
    fn plan(&self, bp: &AirplaneBlueprint) -> FusePlan;

    /// Draw her, nose `+Z`, in true metres, her box's middle at the origin.
    /// The caller owns the root pose.
    fn build(&self, ctx: &PartCtx, plan: &FusePlan) -> Generator;

    /// How she flies.
    fn feel(&self) -> AirplaneFeel;

    /// Where her wheels and centre of mass stand - read off what she draws.
    fn layout(&self, plan: &FusePlan) -> AirplaneLayout;

    /// Where a seeded particle aura issues from, read off her body (drawn
    /// frame, before the travel pose).
    fn fx_mount(&self, aura: ParticleAura, plan: &FusePlan) -> [f32; 3];

    /// How she is driven, and so what she sounds like. Required, with no
    /// default - the boats' rule (#1383).
    fn propulsion(&self) -> Propulsion;
}

/// The builder for an airplane type - the one match over [`AirplaneType`].
/// A match over a non-empty enum needs an arm per variant, so ADDING A TYPE
/// IS A COMPILE ERROR HERE until it is built, as it is in every other
/// family (#1382).
fn craft(t: AirplaneType) -> &'static dyn AirplaneCraft {
    match t {
        AirplaneType::HighWing => &high_wing::HighWing,
    }
}

/// The builder a seed draws with - its own type's, always.
fn craft_for(seed: u64) -> &'static dyn AirplaneCraft {
    craft(AirplaneType::for_seed(seed))
}

/// The seeded machine for `seed`, or `None` for a seed that is not an
/// airplane.
fn body_for(seed: u64) -> Option<(&'static dyn AirplaneCraft, FusePlan)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.airplane().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.plan(&bp)))
}

/// Assemble the seeded airplane for `seed`, posed for travel - with no
/// drop, her drawn origin being her box's middle.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, plan) = body_for(seed).expect("an airplane seed carries an airplane blueprint");
    let mut root = craft.build(&ctx, &plan);
    apply_travel_pose(&mut root, 0.0);
    root
}

/// How the seeded airplane for `seed` flies, where her wheels and centre of
/// mass stand, and her airframe's length (m, spinner to rudder - the
/// blueprint's) - what the locomotion scales her mass by.
pub(super) fn feel_and_layout(seed: u64) -> (AirplaneFeel, AirplaneLayout, f32) {
    // Defensive: an airplane seed always has a blueprint. Falling back to
    // the nominal machine's own numbers keeps a locomotion query total, as
    // the helicopter's does.
    let (craft, plan) = body_for(seed).unwrap_or_else(|| {
        let craft = craft(AirplaneType::UNIVERSAL);
        (craft, craft.plan(&AirplaneBlueprint::NOMINAL))
    });
    (craft.feel(), craft.layout(&plan), plan.airframe_len())
}

/// How the airplane `seed` draws is driven - any seed answers, as every
/// family's does: the voice asks before it knows the family.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded airplane's particle aura issues from (root-local, before
/// the travel pose) - read off her own body by the type that drew her.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan) = body_for(seed)?;
    Some(craft.fx_mount(aura, &plan))
}

#[cfg(test)]
mod tests;
