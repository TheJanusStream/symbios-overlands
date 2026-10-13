//! The seeded helicopter family: one builder per craft type, over one pod
//! plan (#1614, epic #1611).
//!
//! The first of the aircraft families, and the fourth seeded vehicle drawn
//! the way the boats and the skiffs are (#1363, #1364): a craft TYPE is a
//! builder, every line and every mount is read off ONE profile - here the
//! [`PodPlan`], the cabin a light helicopter is built round - and the craft is
//! authored at the size it is drawn at, with no parts, no outfit and no root
//! scale. [`craft`] is the only match over [`HelicopterType`], so a new type
//! is a compile error here until it is built.
//!
//! FIVE types are built. The light utility helicopter the owner agreed on
//! the phase-2 renders (#1613) - a glazed pod on skids, a two-blade main
//! rotor and a tail rotor on a slim boom - at the proportion they chose (B: a
//! 3.2 m fuselage, the cabin 22 % fuller than the first draft) and with all
//! ten of that issue's recommendations taken. And four of the fan-out's
//! (#1618), each on a profile of her own read off the same [`PodPlan`]: the
//! bubble-canopy trainer (#1621) - a glazed bubble over an open engine bay,
//! saddle tanks, an open truss boom and tall skids - with all eight of her
//! recommendations taken; the autogyro (#1624) - a radial's airscrew at her
//! nose, a free rotor on a pylon and three wheels - with all nine of hers;
//! the coaxial (#1628) - two rotors turning opposite ways on one mast, no
//! tail rotor, and a shrouded pusher behind an H-tail - with all eight of
//! hers; and the tandem-rotor transport (#1630) - a box cabin under two
//! rotors turning opposite ways, one on a tall aft pylon, on four wheels in
//! sponsons - with all eight of hers.
//!
//! # Where a helicopter sits
//!
//! On her skids, in a box that holds her fuselage (owner decision 3 of
//! #1611): from the skid bottoms to the top of the rotor head, as wide as the
//! skid track and as long as nose to fin. The main rotor's blades stand
//! outside it - a 3.4 m disc on a 0.9 m track, past the 2.6 m gateway mouth,
//! which decision 3 accepts - and so does the tail rotor's sweep. The
//! Helicopter preset's landed state (#1612) rests her on the box's floor, so
//! the travel pose ([`travel_drop`]) hangs the drawn frame so that the skid
//! bottoms are exactly that floor. The autogyro stands on wheels instead,
//! tail-down, drawn turned in the same level box: her box is measured off
//! her as she stands, and its floor is her tyres (#1624). The tandem stands
//! level on four wheels, and her box's floor is her tyres too (#1630).
//!
//! # What she does at rest
//!
//! Sits still on her skids, rotors turning (#1613 recommendation 3: the
//! rotors are always turning, a Constant spin, until a ground idle earns a
//! Spin term of its own) - but for the autogyro's free rotor, which turns
//! slowly at rest and faster with her speed, on a Speed spin (#1624), and
//! the coaxial's pusher, which does the same (#1628). The
//! airship gait's heave and nose wander are what an envelope does on the
//! air; a machine on skids or wheels does neither, so her seeded gait writes
//! both to zero and leans her into her motion instead ([`HelicopterIdle`]).

mod autogyro;
mod coaxial;
mod plan;
mod tandem;
mod trainer;
mod utility;

/// The coaxial's blade pass (Hz): what her double chop gallops at.
pub(super) use coaxial::BLADE_PASS_HZ as COAXIAL_BLADE_PASS_HZ;
pub(crate) use plan::{PodPlan, PodProfile};
/// The trainer's blade pass (Hz): what her piston voice slaps at.
pub(super) use trainer::BLADE_PASS_HZ as TRAINER_BLADE_PASS_HZ;
/// The utility's blade pass (Hz): what her turbine voice chops at.
pub(super) use utility::BLADE_PASS_HZ as UTILITY_BLADE_PASS_HZ;

use bevy::math::{Affine3A, Quat, Vec3};

use crate::catalogue::items::measure::Bounds;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::{Generator, GeneratorKind};
use crate::seeded_defaults::{
    AvatarCharacter, HelicopterBlueprint, HelicopterType, OrnatenessTier, ParticleAura,
};
use crate::world_builder::primitive_outline;

/// The helicopter family's colours, which live in the fleet's one livery
/// home (#1365) rather than beside its geometry: the utility's, the
/// trainer's (#1621), the autogyro's (#1624), the coaxial's (#1628) and the
/// tandem's (#1630).
pub(crate) use crate::pds::avatar::livery::{
    AutogyroColours, CoaxialColours, HelicopterColours, TandemColours, TrainerColours,
    autogyro_colours, coaxial_colours, helicopter_colours, tandem_colours, trainer_colours,
};

use super::Propulsion;
use super::assemble::apply_travel_pose;

/// The gate every seeded craft drives through - one home, in the module that
/// owns them all (#1382).
#[cfg(test)]
use super::{AIR_DRAFT_CAP, GATEWAY_MOUTH};

/// Smallest dimension anything on a helicopter is built at (m): a hair over
/// the sanitiser's own 0.01 m floor, as the skiffs' is, so a part the
/// sanitiser would change is never drawn (#1359 rule 8).
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// How a helicopter type flies: the numbers
/// [`helicopter_locomotion`](super::helicopter_locomotion) scales the
/// Helicopter preset by - the boats' and skiffs' feel, for a rotorcraft.
///
/// Build-time only, like theirs: what it writes is `HelicopterParams`, which
/// the record has always carried, so it costs no lexicon change, no
/// sanitiser range and no editor slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HelicopterFeel {
    /// Mass over the family's reference, before the size re-basing. It
    /// cancels out of everything she does under her own power - every force
    /// and torque here is per kilogram - and sets how hard she shoves.
    pub(super) mass_factor: f32,
    /// Cyclic acceleration (m/s^2); `cyclic_force` is this times the mass,
    /// and her top speed under it this over `linear_damping`.
    pub(super) cyclic_accel: f32,
    /// Yaw acceleration per kilogram on the preset's default body, scaled by
    /// her own box's inertia (`rotor_torques`), so she turns as a default
    /// body with this number would, whatever her size.
    pub(super) yaw_accel: f32,
    pub(super) linear_damping: f32,
    pub(super) angular_damping: f32,
}

/// What a helicopter type does at rest, and how far she leans into her
/// motion - folded into the seeded gait section (`super::seeded_gait`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HelicopterIdle {
    /// The most she tips into her motion (degrees): nose down flying
    /// forward, banked into a turn, rolled toward a strafe (#1612). The
    /// record's `lean_degrees`.
    pub(super) lean_degrees: f32,
}

/// One buildable kind of rotorcraft.
pub(super) trait HelicopterCraft {
    /// This type's pod at the seeded blueprint's proportions.
    fn plan(&self, bp: &HelicopterBlueprint) -> PodPlan;

    /// Draw her, at the origin, nose `+Z`, in true metres, `y = 0` on the
    /// datum. The caller owns the root pose.
    fn build(&self, ctx: &PartCtx, plan: &PodPlan) -> Generator;

    /// How she flies.
    fn feel(&self) -> HelicopterFeel;

    /// How she sits and leans. Required with no default, as the skiffs'
    /// is: a type that lands without saying how she leans does not compile.
    fn idle(&self) -> HelicopterIdle;

    /// The widest the fuselage is drawn (m) at `ornateness` - the skids,
    /// nearly always, or what a tier hangs past them (the trainer's litter
    /// pods, #1621), or what the autogyro is drawn as, measured (#1624), or
    /// twice the reach of her wider side where a tier hangs something out
    /// on one side alone (the tandem's hoist, #1630) - and what the box,
    /// which is symmetric, is as wide as. NOT the rotor's disc: see the
    /// module header.
    fn overall_width(&self, plan: &PodPlan, ornateness: OrnatenessTier) -> f32;

    /// How far the top of her rotor head stands over what she stands on -
    /// her skid bottoms, or her tyres (m) - what the box is as tall as.
    fn height(&self, plan: &PodPlan) -> f32;

    /// Where a seeded particle aura issues from, read off her body (drawn
    /// frame, before the travel pose).
    fn fx_mount(&self, aura: ParticleAura, plan: &PodPlan) -> [f32; 3];

    /// How she is driven, and so what she sounds like. Required, with no
    /// default - the boats' rule (#1383).
    fn propulsion(&self) -> Propulsion;
}

/// The builder for a helicopter type - the one match over
/// [`HelicopterType`]. A match over a non-empty enum needs an arm per
/// variant, so ADDING A TYPE IS A COMPILE ERROR HERE until it is built, as it
/// is in the boats' and the skiffs' (#1382).
fn craft(t: HelicopterType) -> &'static dyn HelicopterCraft {
    match t {
        HelicopterType::Utility => &utility::Utility,
        HelicopterType::Trainer => &trainer::Trainer,
        HelicopterType::Autogyro => &autogyro::Autogyro,
        HelicopterType::Coaxial => &coaxial::Coaxial,
        HelicopterType::Tandem => &tandem::Tandem,
    }
}

/// The builder a seed draws with - its own type's, always.
fn craft_for(seed: u64) -> &'static dyn HelicopterCraft {
    craft(HelicopterType::for_seed(seed))
}

/// The idle of the type a seed draws with - what `super::seeded_gait` folds
/// into the seeded gait section.
pub(super) fn idle_for(seed: u64) -> HelicopterIdle {
    craft_for(seed).idle()
}

/// Every built type's feel, by name - the per-type feel guard's table half.
#[cfg(test)]
pub(super) fn every_feel() -> Vec<(&'static str, HelicopterFeel)> {
    HelicopterType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).feel()))
        .collect()
}

/// The seeded machine for `seed`, or `None` for a seed that is not a
/// helicopter.
fn body_for(seed: u64) -> Option<(&'static dyn HelicopterCraft, PodPlan)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.helicopter().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.plan(&bp)))
}

// ---------------------------------------------------------------------------
// Where the machine stands
// ---------------------------------------------------------------------------

/// The bounds of `parts` (drawn frame), what a type that measures her box
/// reads it off: the mesher's own vertices ([`primitive_outline`], which
/// reads a sweep's without building its mesh), through every part's
/// transform and through a turning hub's to what it carries - but its
/// blades, each a block it carries that does not turn of itself. Her box
/// holds her airframe, and the blades stand outside it (owner decision 3 of
/// #1611): a rotor's disc, an airscrew's, a pusher's. A hub a turning hub
/// carries - a wheel steering and rolling - is in it. The autogyro's
/// (#1624), the coaxial's (#1628) and the tandem's (#1630).
fn airframe_bounds(parts: &[Generator]) -> Bounds {
    fn reach_into(part: &Generator, parent: Affine3A, out: &mut Option<Bounds>) {
        let t = &part.transform;
        let world = parent
            * Affine3A::from_scale_rotation_translation(
                Vec3::from(t.scale.0),
                Quat::from_array(t.rotation.0).normalize(),
                Vec3::from(t.translation.0),
            );
        for p in primitive_outline(&part.kind) {
            let at = world.transform_point3(p);
            *out = Some(match *out {
                None => Bounds { min: at, max: at },
                Some(b) => Bounds {
                    min: b.min.min(at),
                    max: b.max.max(at),
                },
            });
        }
        let turning = part.spin.is_some();
        for child in &part.children {
            if turning && matches!(child.kind, GeneratorKind::Cuboid { .. }) && child.spin.is_none()
            {
                continue;
            }
            reach_into(child, world, out);
        }
    }
    let mut out: Option<Bounds> = None;
    for part in parts {
        reach_into(part, Affine3A::IDENTITY, &mut out);
    }
    out.expect("she draws something")
}

/// Half-extents (m) of the chassis collider for this plan at `ornateness`:
/// skid bottoms (or tyres) to rotor head, as wide as she is drawn, nose to
/// tail - the fuselage, owner decision 3. The drawn frame is centred fore
/// and aft on the origin ([`PodPlan::at`]), so the box is symmetric.
pub(super) fn chassis_half_extents(
    craft: &dyn HelicopterCraft,
    plan: &PodPlan,
    ornateness: OrnatenessTier,
) -> [f32; 3] {
    let (nose, tail) = plan.nose_and_tail();
    [
        craft.overall_width(plan, ornateness) * 0.5,
        craft.height(plan) * 0.5,
        (nose - tail) * 0.5,
    ]
}

/// Travel-pose drop (m): how far under the chassis origin the assembler
/// hangs the drawn frame's origin (the datum).
///
/// **Derived, not tuned.** The chassis origin is the box's centre, half her
/// height over her skid bottoms or her tyres; the datum is
/// [`PodPlan::datum_height`] over them. Dropping the drawn frame by the
/// difference puts them exactly on the box's floor, which is what a landed
/// body rests on (#1612).
pub(super) fn travel_drop(craft: &dyn HelicopterCraft, plan: &PodPlan) -> f32 {
    craft.height(plan) * 0.5 - plan.datum_height()
}

/// Assemble the seeded helicopter for `seed`, posed for travel.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, plan) = body_for(seed).expect("a helicopter seed carries a helicopter blueprint");
    let mut root = craft.build(&ctx, &plan);
    apply_travel_pose(&mut root, travel_drop(craft, &plan));
    root
}

/// How the seeded helicopter for `seed` flies, the box she flies in and her
/// fuselage length (m, nose to boom end - the blueprint's) - what the
/// locomotion scales her mass by.
pub(super) fn feel_and_box(seed: u64) -> (HelicopterFeel, [f32; 3], f32) {
    // Defensive: a helicopter seed always has a blueprint. Falling back to
    // the nominal machine's own numbers keeps a locomotion query total, as
    // the skiffs' does.
    let (craft, plan) = body_for(seed).unwrap_or_else(|| {
        let craft = craft(HelicopterType::UNIVERSAL);
        (craft, craft.plan(&HelicopterBlueprint::NOMINAL))
    });
    // Her tier, as her build reads it (`PartCtx::for_seed`): what she hangs
    // past her skids is in her box.
    let ornateness = AvatarCharacter::for_seed(seed).ornateness_tier();
    (
        craft.feel(),
        chassis_half_extents(craft, &plan, ornateness),
        plan.fuselage_len(),
    )
}

/// How the helicopter `seed` draws is driven - any seed answers, as the
/// boats' and skiffs' do: the voice asks before it knows the family.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded helicopter's particle aura issues from (root-local, before
/// the travel pose) - read off her own body by the type that drew her.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan) = body_for(seed)?;
    Some(craft.fx_mount(aura, &plan))
}

#[cfg(test)]
mod tests;
