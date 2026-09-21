//! The seeded land-skiff family: one builder per craft type, over one body
//! plan.
//!
//! Replaces the part-assembled skiff pipeline of #778, which stacked a body
//! out of superellipsoid pillows, hung 8.5 cm fender tubes on a 21 cm wheel,
//! and mounted its canopy at a fixed `y = 0.33` that only one of its four
//! chassis ever reached - so on the other three the cabin hovered in open air.
//! At 1.5 m it was a toy beside a 1.7 m person (#1359 diagnosis, owner
//! decision 1), and its wheels and fenders agreed only because three files
//! repeated the same magic numbers.
//!
//! # A type is a builder, not an arrangement
//!
//! [`SkiffType`] (#1362) is the discrete pick inside the family, and this is
//! where it becomes geometry. One struct per type implements [`SkiffCraft`],
//! one file per type, and [`craft`] is the only match over the enum - the
//! central [`build_for_seed`](super::build_for_seed) just delegates. This is
//! the skiff half of the seam the sloop opened in #1363, and it is written the
//! same way, down to the explicit `None` group for the types with no
//! implementor: [`craft_for`] resolves an unbuilt pick to
//! [`SkiffType::UNIVERSAL`] while the PICK stays a property of the seed.
//!
//! ALL SIX types are built: the roadster, the universal floor; since #1377
//! the horseless [`wagon`], which takes all ten historic themes and is 31 %
//! of the family against the Roadster's 30; since #1374 the dune [`buggy`],
//! on the six leisure and frontier themes, 13 % of it; since #1376 the
//! three-wheeled [`cyclecar`], on the neon themes and the campus - the first
//! type on an unpaired axle; since #1375 the [`armoured`] car, on the three
//! modern martial themes, 8 % of it - the first type drawn in FACETS rather
//! than in sweeps; and since #1378 the six-wheeled [`rover`], on the outpost
//! and alien themes, the other 8 % - the first type on more than four
//! wheels, the first to carry a textured finish and the first whose identity
//! trim is LIT on every seed she has. No skiff seed draws another type's
//! machine any more.
//!
//! **The Option seam is gone** (#1382): [`craft`] returns the builder rather
//! than an `Option` of one, in both families at once. See its own note for
//! what that costs a seventh type.
//!
//! # Where a skiff sits
//!
//! The visual origin is the body's **datum** - the plane the body sweeps are
//! cut on, which is the cockpit coaming line. The ground is
//! [`BodyPlan::datum_height`] below it, and [`travel_drop`] is simply the
//! difference between where the suspension rests the chassis origin and that
//! number, which is what puts the tyres on the suspension's own ground line
//! for the wheels this seed actually rolls on (#1361).

mod armoured;
mod buggy;
mod cyclecar;
mod plan;
mod roadster;
mod rover;
mod shape;
mod wagon;

pub(crate) use plan::BodyPlan;

use crate::pds::avatar::locomotion::CarParams;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ParticleAura, SkiffBlueprint, SkiffType};

/// The skiff family's colours, which live in the fleet's one livery home
/// (#1365) rather than beside its geometry - the roadster and every type after
/// it read them through here.
pub(crate) use crate::pds::avatar::livery::{SkiffColours, skiff_colours};

use super::Propulsion;
use super::assemble::apply_travel_pose;

/// The gate both families drive through - one home, in the module that owns
/// them both (#1382). These used to be a local `const AIR_DRAFT = 2.8` in four
/// of this file's tests and a `const MOUTH = 2.6` in five.
#[cfg(test)]
use super::{AIR_DRAFT_CAP, GATEWAY_MOUTH};

/// Smallest dimension anything on a skiff is built at (m).
///
/// A hair over the sanitiser's own 0.01 m floor. Every radius and every
/// thickness here scales with the machine, so the smallest seeded skiff would
/// otherwise draw a bonnet bead or a track rod *under* that floor - and a part
/// the sanitiser CHANGES fails the round-trip the family owes (#1359 rule 8).
/// Flooring it here instead means the record round-trips untouched at every
/// blueprint extreme.
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// How a craft type drives: the numbers
/// [`skiff_locomotion`](super::skiff_locomotion) scales its preset by.
///
/// Per type rather than per chassis *class*, which is what the four legacy
/// chassis slugs used to carry. Every tuple here was driven through the
/// probe in #1381 and agreed by the owner on 2026-09-20; the roadster's are
/// the old default chassis's exactly, so the drive validated in #1361 is
/// still the drive that ships and is still the yardstick the other five are
/// set against.
///
/// # Why this grew two damping fields (#1381)
///
/// Until the sweep it carried three numbers, and every skiff therefore
/// shared `CarParams`' own 0.8 / 4.0. Measured on the drive probe, that
/// made all six reach 90% of their speed in the same 2.89 s and coast down
/// in the same 2.89 s: with one shared damping a craft can be made SLOW but
/// never PONDEROUS, because `top speed = drive_accel / linear_damping` and
/// `t90 = ln(10) / linear_damping` are then the same knob. Six of the ten
/// agreed tuples need their own.
///
/// It costs nothing on the wire. This struct is `pub(super)` and build-time
/// only; what the two fields WRITE is `CarParams::linear_damping` and
/// `::angular_damping`, which the record has always carried. So there is no
/// lexicon change, no sanitiser range and no editor slice - and the
/// roadster's literals are `CarParams`' own defaults, so her published
/// record comes out bit-identical to the one #1361 signed off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SkiffFeel {
    /// Mass over the family's 900 kg baseline, before the size re-basing.
    ///
    /// It moves nothing the owner drives by - measured as the probe's own
    /// control (#1381): `drive_force = mass x drive_accel`, `turn_torque =
    /// mass x turn_accel` and avian's inertia is `mass x box`, so the mass
    /// cancels out of top speed, acceleration, yaw rate and turning circle
    /// alike. It sets how hard she shoves another body, and how the
    /// suspension and grip are scaled so she keeps her ride height.
    pub(super) mass_factor: f32,
    /// Target acceleration (m/s^2); `drive_force` is this times the mass.
    pub(super) drive_accel: f32,
    /// Target angular acceleration; `turn_torque` is this times the mass.
    /// What it BUYS depends on the collider box, because the body answers
    /// with torque over inertia - so the card was measured, never derived.
    pub(super) turn_accel: f32,
    /// Sets both her top speed (`drive_accel / linear_damping`) and how
    /// long she takes to reach it (`ln(10) / linear_damping`), exactly on
    /// all six.
    pub(super) linear_damping: f32,
    /// How quickly a turn settles, and so - with the box - how tight a
    /// circle she holds.
    pub(super) angular_damping: f32,
}

/// What a craft type does at rest, and how far she leans in a corner
/// (#1381).
///
/// # Stillness is a type, not a number
///
/// `SKIFF_SHIVER_HZ` is documented as "an idling-engine buzz" and was worn
/// by all six, so a horse-drawn wagon, a servo rover and an ELECTRIC
/// cyclecar each trembled at 9 Hz from an engine they do not have. A craft
/// with nothing to idle has `shiver: None` and sits still, and that is
/// enforced by the shape of this struct rather than by a zero somebody has
/// to remember to write.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SkiffIdle {
    /// What she does at rest, or `None` for a craft with no engine.
    pub(super) shiver: Option<Shiver>,
    /// How far she leans in a corner, in degrees - the bank CLAMP, which
    /// rides the record's angular field since #1381 (see
    /// [`SkiffIdle`]'s boat twin, `BoatIdle`, for why that field).
    ///
    /// The DIRECTION is not here, and cannot be: a vehicle's published
    /// record is a generator tree, a locomotion config and a gait section -
    /// no craft type, no propulsion, no seed - so a peer could never key a
    /// table on it. It comes from the record's own MASS instead, through
    /// `player::gait::skiff_bank_sign`.
    pub(super) bank_degrees: f32,
}

/// An idling engine's tremble.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Shiver {
    /// Vertical tremble, as a multiple of what the roadster does on the
    /// same seed.
    pub(super) amplitude: f32,
    /// What the engine idles at (Hz), before the seed's own spread is
    /// carried across it. A flat-four shakes at 4.6, the roadster's
    /// baseline is 3.75, and a big slow diesel lopes at 2.5 - the 11, 9 and
    /// 6 of #1381 scaled together into the owner's sway band (#1400), which
    /// `every_seeded_skiff_idles_and_banks_inside_the_owners_bands` holds
    /// every type to.
    pub(super) hz: f32,
}

/// One buildable kind of land craft.
pub(super) trait SkiffCraft {
    /// This type's body, from the seeded blueprint: its own plan form, section
    /// depth and layout over dimensions everyone shares. It takes the seed
    /// because a type can build more than one body and roll on more than one
    /// wheel, and both are properties of the seed that the plan must carry -
    /// the roadster's boat-tail, bobtail and tourer, and the balloon tyre that
    /// raises its axle line (#1367).
    fn plan(&self, bp: &SkiffBlueprint, seed: u64) -> BodyPlan;

    /// Draw it, at the origin, nose `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator;

    /// How it drives.
    fn feel(&self) -> SkiffFeel;

    /// How it sits at rest, and how far it leans. Required with no
    /// default, like [`propulsion`](Self::propulsion): a craft that lands
    /// without saying whether it has an engine to idle does not compile,
    /// so no wagon inherits a buzz.
    fn idle(&self) -> SkiffIdle;

    /// The widest the type is actually drawn, guards included (m) - what the
    /// gateway-mouth guard measures, and a type's own knowledge rather than
    /// the plan's, because a guard is the type's choice. It takes the seed
    /// because what stands outermost can be the seed's own pick: a wagon's
    /// naves, bigger on an ox-cart (#1377).
    fn overall_width(&self, plan: &BodyPlan, seed: u64) -> f32;

    /// Where a seeded particle aura issues from, read off the body - and off
    /// the seed's own picks, because a flourish that hovers over an open
    /// cockpit would hover inside a closed one (#1367).
    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan, seed: u64) -> [f32; 3];

    /// How it is driven, and so what it sounds like and whether it trails an
    /// exhaust (#1377). Required, with no default - the boat's rule (#1383):
    /// a type that lands without saying whether it has an engine does not
    /// compile, so no wagon inherits a putter.
    fn propulsion(&self) -> Propulsion;
}

/// The builder for a craft type - the twin of the boats' own `craft`.
///
/// The one match over [`SkiffType`], and deliberately not a stub: a match
/// over a non-empty enum needs an arm per variant, so ADDING A TYPE IS A
/// COMPILE ERROR HERE until it is listed. Each of #1374-#1378 added one.
///
/// # It used to return an `Option`, and no longer does (#1382)
///
/// Through the fan-out this returned `None` for a type nothing drew yet, and
/// [`craft_for`] resolved such a pick to [`SkiffType::UNIVERSAL`]. The rover
/// (#1378) emptied that group on this side and the longship (#1369) on the
/// boats'; both halves collapsed together, as they were held open together.
///
/// The price, the same on both sides: a SEVENTH TYPE CANNOT LAND HALF-BUILT.
/// There is no unbuilt state to report and no floor to fall back to, so a new
/// type arrives with its builder in the same commit as its enum variant.
fn craft(t: SkiffType) -> &'static dyn SkiffCraft {
    match t {
        SkiffType::Roadster => &roadster::Roadster,
        SkiffType::Wagon => &wagon::Wagon,
        SkiffType::DuneBuggy => &buggy::Buggy,
        SkiffType::Cyclecar => &cyclecar::Cyclecar,
        SkiffType::ArmouredCar => &armoured::Armoured,
        SkiffType::Rover => &rover::Rover,
    }
}

/// The builder a seed draws with - its own type's, always.
fn craft_for(seed: u64) -> &'static dyn SkiffCraft {
    craft(SkiffType::for_seed(seed))
}

/// The idle of the type a seed actually draws with (#1381) - what
/// `super::seeded_gait` folds into the seeded gait section.
pub(super) fn idle_for(seed: u64) -> SkiffIdle {
    craft_for(seed).idle()
}

/// Every built type's idle, by name - for the per-type idle guard and the
/// owner's idle page.
#[cfg(test)]
pub(super) fn every_idle() -> Vec<(&'static str, SkiffIdle)> {
    SkiffType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).idle()))
        .collect()
}

/// Every built type's feel, by name - the per-type feel guard's table half
/// (#1381, `no_two_craft_types_in_a_family_share_a_feel`).
///
/// Test-only, for the reason in the boats' twin: only `skiff_locomotion`
/// reads a feel in the build, and it reads the seed's own.
#[cfg(test)]
pub(super) fn every_feel() -> Vec<(&'static str, SkiffFeel)> {
    SkiffType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).feel()))
        .collect()
}

/// The seeded body for `seed`, or `None` for a seed that is not a skiff.
fn body_for(seed: u64) -> Option<(&'static dyn SkiffCraft, BodyPlan)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.skiff().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.plan(&bp, seed)))
}

// ---------------------------------------------------------------------------
// Where the machine stands
// ---------------------------------------------------------------------------

/// Half-extents (m) of the chassis collider for this plan.
///
/// Read off the bodywork the craft actually draws, which is what makes it safe
/// at airship-class size. The legacy `0.4 * (body_len / 1.5)` tracked the
/// body's *length*, so a machine scaled to 2.65 m stood in a 1.4 m tall
/// collider - the tall-narrow shape behind the #804 rollovers, on something
/// only 0.7 m high. The width is the DRAWN width rather than the coachwork's,
/// because what a player collides with is the wheels: this era's body is far
/// narrower than its track, and a collider cut to the tub would let the guards
/// pass through a wall.
pub(super) fn chassis_half_extents(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> [f32; 3] {
    [
        craft.overall_width(plan, seed) * 0.5,
        plan.depth(),
        plan.length * 0.5,
    ]
}

/// Height (m) above flat ground the skiff's **chassis origin** rests at.
///
/// The four corner springs carry the weight from `half_y` below the origin,
/// compressing by
/// [`static_suspension_compression`](super::static_suspension_compression), so
/// the origin floats that much less than a full suspension rest length above
/// the ground. Its compression term is seed-invariant and its `half_y` is not,
/// which is why [`travel_drop`] has to be derived per craft rather than
/// written down once.
fn chassis_ride_height(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> f32 {
    let p = CarParams::default();
    chassis_half_extents(craft, plan, seed)[1] + p.suspension_rest_length.0
        - super::static_suspension_compression(super::SKIFF_REF_MASS, p.suspension_stiffness.0)
}

/// Travel-pose drop (m): how far under the chassis origin the assembler hangs
/// the body's datum.
///
/// **Derived, not tuned** (#1361): the datum sits [`BodyPlan::datum_height`]
/// over the ground by construction, so dropping the visual by the difference
/// is the one value that puts the tyre bottoms exactly on the suspension's
/// ground line - for the wheels THIS seed rolls on, not a nominal pair. The
/// hand-set 0.55 this replaced assumed one nominal wheel and floated or sank
/// them by centimetres across the seeded radius band.
pub(super) fn travel_drop(craft: &dyn SkiffCraft, plan: &BodyPlan, seed: u64) -> f32 {
    chassis_ride_height(craft, plan, seed) - plan.datum_height()
}

/// Assemble the seeded skiff for `seed`, posed for travel.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, plan) = body_for(seed).expect("a skiff seed carries a skiff blueprint");
    let mut root = craft.build(&ctx, &plan);
    // No scale: since #1364 a skiff is authored at the size she is drawn at,
    // so the airship-class bridge the legacy pipeline carried has nothing left
    // to convert - and the skiff was the last family holding one.
    apply_travel_pose(&mut root, travel_drop(craft, &plan, seed));
    root
}

/// How the seeded skiff for `seed` drives, and the collider box it drives in.
pub(super) fn feel_and_box(seed: u64) -> (SkiffFeel, [f32; 3]) {
    match body_for(seed) {
        Some((craft, plan)) => (craft.feel(), chassis_half_extents(craft, &plan, seed)),
        // Defensive: a skiff seed always has a blueprint. Falling back to the
        // floor's own feel keeps a locomotion query total rather than
        // panicking in a sanitiser round-trip that exercises the family
        // off-seed.
        None => (craft(SkiffType::UNIVERSAL).feel(), [0.66, 0.29, 1.33]),
    }
}

/// How far the ground is under the visual origin for `seed` (m), or `None` for
/// a seed that is not a skiff - the number [`travel_drop`] places against, and
/// what the family's pose test checks the assembled tree against.
#[cfg(test)]
pub(super) fn datum_height_for_seed(seed: u64) -> Option<f32> {
    body_for(seed).map(|(_, plan)| plan.datum_height())
}

/// How the skiff `seed` DRAWS is driven (#1377) - asked of the DRAWN craft,
/// which since #1378 is her own type's. Any seed answers, as the boats' does:
/// the voice asks before it knows the family is a skiff's.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded skiff's particle aura issues from (root-local, before the
/// travel pose) - read off its own body by the craft type that drew it, so an
/// exhaust wisp leaves the pipe mouth of the machine that is actually there.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan) = body_for(seed)?;
    Some(craft.fx_mount(aura, &plan, seed))
}

#[cfg(test)]
mod tests {
    use super::super::common::first_difference;
    use super::*;
    use crate::seeded_defaults::{ChassisFamily, VehicleStance};

    /// The blueprint corners a seeded skiff can actually reach - the smallest
    /// machine draws the thinnest bead and the finest track rod, the largest
    /// the widest guard, and a clamp corner is still a car.
    fn corners() -> Vec<SkiffBlueprint> {
        let mut out = Vec::new();
        for &length in &[1.90f32, 2.65, 3.60] {
            for &(body, track, wheel, belt, height) in &[
                (0.250f32, 0.390f32, 0.110f32, 0.285f32, 0.360f32),
                (0.300, 0.455, 0.125, 0.315, 0.420),
            ] {
                out.push(SkiffBlueprint {
                    stance: VehicleStance::Sleek,
                    length,
                    body_w: length * body,
                    wheelbase: length * 0.64,
                    track: length * track,
                    wheel_r: length * wheel,
                    beltline: length * belt,
                    height: length * height,
                });
            }
        }
        out
    }

    fn a_skiff_seed() -> u64 {
        (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff)
            .expect("some seed is a skiff")
    }

    /// Every skiff seed draws a skiff, and draws ITS OWN - the boats' twin.
    ///
    /// It used to count the seeds whose pick nothing drew, and assert that
    /// count was zero; since #1382 collapsed [`craft`] there is no unbuilt
    /// state left to count - a pick that had no builder would not compile.
    /// What survives is the half that is still a real claim about the
    /// POPULATION: every one of the six [`SkiffType`]s is PICKED by some seed
    /// under 600, so the fleet the owner actually meets contains all six, and
    /// each one's body is non-degenerate on every seed that picks it.
    #[test]
    fn every_skiff_seed_resolves_to_a_built_craft() {
        let mut total = 0;
        let mut picked: Vec<SkiffType> = Vec::new();
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let t = SkiffType::for_seed(s);
            if !picked.contains(&t) {
                picked.push(t);
            }
            total += 1;
            let (_, plan) = body_for(s).expect("a skiff seed has a body");
            assert!(plan.length > 1.0, "seed {s}: degenerate body");
        }
        assert!(total > 50, "too few skiffs sampled: {total}");
        for t in SkiffType::ALL {
            assert!(
                picked.contains(&t),
                "no skiff seed under 600 picked {t:?} - the census sampled \
                 {} of the {} types",
                picked.len(),
                SkiffType::ALL.len()
            );
        }
    }

    /// Every drawn sweep END in `root`'s own frame, with the node's path so a
    /// failure can name it (#1382).
    ///
    /// This exists so a mount guard can be read against the GEOMETRY rather
    /// than against the function that placed it. `Roadster::fx_mount` returns
    /// the last point of `coachwork::exhaust_path`, and `coachwork` sweeps the
    /// pipe along that same path - so asserting the emitter equals
    /// `fx_mount(..)` compares one call with itself and cannot fail. It was
    /// written that way first and a 10 mm perturbation of the mount did not
    /// turn it red, which is exactly the compensating-reading trap. Reading
    /// the tube out of the tree is the independent half.
    fn sweep_ends(root: &Generator) -> Vec<([f32; 3], String)> {
        use crate::pds::generator::GeneratorKind;
        use bevy::math::{Quat, Vec3};
        fn walk(
            g: &Generator,
            t: Vec3,
            r: Quat,
            s: Vec3,
            path: String,
            out: &mut Vec<([f32; 3], String)>,
        ) {
            let lt = Vec3::from(g.transform.translation.0);
            let lr = Quat::from_array(g.transform.rotation.0).normalize();
            let ls = Vec3::from(g.transform.scale.0);
            let (wt, wr, ws) = (t + r * (s * lt), r * lr, s * ls);
            if let GeneratorKind::Spine { points, .. } = &g.kind {
                for (which, p) in [("head", points.first()), ("tail", points.last())] {
                    if let Some(p) = p {
                        let v = wt + wr * (ws * Vec3::from(p.position.0));
                        out.push((v.to_array(), format!("{path} {which}")));
                    }
                }
            }
            for (i, c) in g.children.iter().enumerate() {
                walk(c, wt, wr, ws, format!("{path}/{i}"), out);
            }
        }
        let mut out = Vec::new();
        // The ROOT's own transform is excluded: an FX mount is expressed in
        // the root's local frame, before the travel pose the assembler puts on
        // the root, and `fx::attach` hangs the emitter as a child of that root.
        for (i, c) in root.children.iter().enumerate() {
            walk(
                c,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::ONE,
                format!("{i}"),
                &mut out,
            );
        }
        out
    }

    /// The box every drawn node's ORIGIN falls inside, in `root`'s own frame -
    /// [`sweep_ends`]'s blunter companion (#1382).
    ///
    /// Node origins rather than sampled surfaces, so this needs no shape
    /// vocabulary and cannot drift with `common::touch` - which is also why it
    /// is used here rather than `touch::highest`: touch PANICS on a prim it has
    /// no solid for, and the tree this is asked about is the RECORD's, with the
    /// FX emitter already hung on it. It is a loose envelope, and it is enough
    /// for the claim it serves: an FX mount that has come adrift from the
    /// machine - the fixed `y = 0.33` canopy of the legacy skiff, which hovered
    /// over every chassis the default did not reach (#1364) - lands outside it.
    ///
    /// Only DRAWN prims count. A `ParticleSystem` is not a primitive, so the
    /// emitter whose position is being judged is not in its own bound.
    fn origin_bounds(root: &Generator) -> ([f32; 3], [f32; 3]) {
        use bevy::math::{Quat, Vec3};
        fn walk(g: &Generator, t: Vec3, r: Quat, s: Vec3, lo: &mut Vec3, hi: &mut Vec3) {
            let lt = Vec3::from(g.transform.translation.0);
            let lr = Quat::from_array(g.transform.rotation.0).normalize();
            let ls = Vec3::from(g.transform.scale.0);
            let (wt, wr, ws) = (t + r * (s * lt), r * lr, s * ls);
            if g.is_primitive() {
                *lo = lo.min(wt);
                *hi = hi.max(wt);
            }
            for c in &g.children {
                walk(c, wt, wr, ws, lo, hi);
            }
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for c in &root.children {
            walk(c, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, &mut lo, &mut hi);
        }
        (lo.to_array(), hi.to_array())
    }

    /// `root` with every non-primitive node pruned - the DRAWN machine, which
    /// is what `common::touch` can read (#1382).
    ///
    /// touch panics on a prim it has no solid for, and a record's tree carries
    /// the FX emitter, which is a `ParticleSystem`. Pruning it is what lets a
    /// guard ask touch how tall the machine actually is while judging where the
    /// emitter sits on it. The emitter is a leaf child of the root, so nothing
    /// drawn is orphaned by the prune.
    fn drawn_only(root: &Generator) -> Generator {
        let mut out = root.clone();
        out.children = root
            .children
            .iter()
            .filter(|c| c.is_primitive())
            .map(drawn_only)
            .collect();
        out
    }

    /// Every roadster the family can draw at the blueprint corners: every
    /// body, top and wheel crossed with every ornateness-by-wear pair a seed
    /// can roll - which is all of them, since the axes are drawn
    /// independently. Returns the built tree, its plan and a label for the
    /// failure - the plan because the air-draft guard has to resolve a drawn
    /// height against the ground this machine stands on (#1382).
    fn every_roadster() -> Vec<(Generator, roadster::RoadsterPlan, String)> {
        use crate::seeded_defaults::{
            OrnatenessTier, RoadsterBody, RoadsterTop, RoadsterWheels, WearTier,
        };
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    for top in RoadsterTop::ALL {
                        for o in OrnatenessTier::ALL {
                            for w in WearTier::ALL {
                                (ctx.ornateness, ctx.wear) = (o, w);
                                out.push((
                                    roadster::build_dressed(&ctx, &plan, top, rolls),
                                    plan,
                                    format!(
                                        "a {} m {} {} roadster on {} wheels, {} / {}",
                                        bp.length,
                                        top.label(),
                                        body.label(),
                                        rolls.label(),
                                        o.label(),
                                        w.label()
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Every wagon the family can draw at the blueprint corners: every body
    /// crossed with every ornateness-by-wear pair. Returns the built tree, its
    /// plan and a label for the failure.
    fn every_wagon() -> Vec<(Generator, wagon::WagonPlan, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WagonBody, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for body in WagonBody::ALL {
                let plan = wagon::plan_of(&bp, body);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            wagon::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                body.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 5 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built wagon meets another and the whole machine is one
    /// component, on every body and tier at every blueprint corner (#1377) -
    /// on the tree AS SAVED, through the record's 0.1 mm wire, as the
    /// roadster's is.
    ///
    /// What it cannot see: the guard ignores `hollow`, so it judges a wheel's
    /// bored felloe as a solid disc and a spoke meets it whatever. A spoke's
    /// contact is by construction - each is seated half the felloe's depth
    /// into it (`running_gear::spoked_wheel`).
    #[test]
    fn a_wagon_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_wagon() {
            let json = serde_json::to_string(&built).expect("a wagon serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every wagon survives the record sanitiser UNCHANGED at the extremes of
    /// its own blueprint, on every body and tier (#1359 rule 8) - the bored
    /// wheel bands included, whose `hollow` is held under the sanitiser's
    /// 0.95 cap by construction.
    #[test]
    fn a_wagon_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_wagon() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// The wagon stands on her wheels under the air draft and inside the
    /// gateway mouth, on every body and tier at every blueprint corner: each
    /// axle's centre is its OWN wheel's radius over the ground - the small
    /// front pair of a four-wheeler as much as the big rear one, and the
    /// two-wheelers' single pair (#1377).
    ///
    /// The wheel half is #1377's. The air-draft and mouth halves are #1382's,
    /// folded into the same sweep rather than added beside it: she was the
    /// first fan-out type and landed before the dune buggy set the per-type
    /// pattern, so she is one of the two machines it skipped (see the
    /// roadster's). Nothing is FIXED here either - measured first, her tallest
    /// is 2.438 m, the Adorned Cart's canvas tilt at the 3.6 m corner, against
    /// a 2.8 m cap, and her widest is 2.020 m against a 2.6 m mouth. She is
    /// the tallest machine in the family and the one that would find a
    /// lowered lintel first.
    #[test]
    fn a_wagon_stands_on_her_wheels_under_the_air_draft() {
        use super::super::common::touch;
        let (mut tallest, mut widest): (f32, f32) = (0.0, 0.0);
        for (built, plan, what) in every_wagon() {
            for (at, r) in plan.wheels() {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the \
                 {AIR_DRAFT_CAP} m air draft"
            );
            // Her width is asked of the WAGON rather than of the seed's own
            // body, because `overall_width` reads `WagonBody::for_seed` and
            // this sweep is over bodies rather than over seeds: the widest
            // nave is what the mouth has to clear whichever body wears it.
            let wide = wagon::Wagon.overall_width(&plan, a_skiff_seed());
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
            (tallest, widest) = (tallest.max(top), widest.max(wide));
        }
        // The numbers the guard was written against (#1382).
        assert!(
            (2.2..2.7).contains(&tallest) && widest > 1.5,
            "the tallest wagon is {tallest} m and the widest {widest} m, not \
             the 2.438 m and 2.020 m this was measured at - the shape moved, \
             so re-read the cap margin"
        );
    }

    /// Every dune buggy the family can draw at the blueprint corners: every
    /// variant crossed with every ornateness-by-wear pair (#1374). Returns the
    /// built tree, its plan and a label for the failure.
    fn every_buggy() -> Vec<(Generator, buggy::BuggyPlan, String)> {
        use crate::seeded_defaults::{BuggyVariant, OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for variant in BuggyVariant::ALL {
                let plan = buggy::plan_of(&bp, variant);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            buggy::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                variant.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 3 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built dune buggy meets another and the whole machine
    /// is one component, on every variant and tier at every blueprint corner
    /// (#1374) - on the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// Nothing on her is fattened to meet: every frame member is drawn
    /// through joints another member also runs through, and a mass hung
    /// between joints sits on the tube's drawn centreline (see the buggy's
    /// module docs). What the guard cannot see: it ignores `hollow` and
    /// `path_cut`, so the pod's bore and the canopy's stripes are judged as
    /// whole tubes, and it samples a turned tyre only at its profile rings
    /// (#1393).
    #[test]
    fn a_buggy_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_buggy() {
            let json = serde_json::to_string(&built).expect("a buggy serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every buggy survives the record sanitiser UNCHANGED at the extremes of
    /// her own blueprint, on every variant and tier (#1359 rule 8) - her
    /// rotated tyres, lamps, cylinder banks, seat backs and spare compared
    /// through the quaternion epsilon the sanitiser's renormalising needs.
    #[test]
    fn a_buggy_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_buggy() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every buggy stands on her wheels, under the air draft and inside the
    /// gateway, at every blueprint corner (#1374): each axle's centre is its
    /// OWN wheel's radius over the ground - the small fronts as much as the
    /// big rears - nothing she draws stands higher than rule 6's air-draft cap
    /// over it, and she is narrower than the gateway mouth.
    ///
    /// The first skiff air-draft guard (owner decision 11): the dune whip
    /// reaches 2.57 m at the largest corner, held there by construction, and
    /// this reads the DRAWN tree rather than the arithmetic that held it.
    #[test]
    fn a_buggy_stands_on_her_wheels_under_the_air_draft() {
        use super::super::common::touch;
        for (built, plan, what) in every_buggy() {
            let mut radii = Vec::new();
            for (at, r) in plan.wheels() {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
                if !radii.contains(&r) {
                    radii.push(r);
                }
            }
            assert_eq!(radii.len(), 2, "{what}: her two axles share a radius");
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT_CAP} m air draft"
            );
            let wide = buggy::Buggy.overall_width(&plan, 0);
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
        }
    }

    /// Every dune buggy seed is drawn as a buggy, on the variant her theme
    /// picks, air-cooled (#1374) - and no other skiff seed is: a skiff is
    /// air-cooled exactly when it is drawn as a buggy. And the aura her
    /// record carries - her exhaust, a Roadside buggy's folded steam, or a
    /// frontier theme's embers - leaves her stinger's mouth.
    #[test]
    fn a_buggy_seed_draws_a_buggy() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::BuggyVariant;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let buggy = SkiffType::for_seed(s) == SkiffType::DuneBuggy;
            assert_eq!(
                propulsion(s) == Propulsion::AirCooled,
                buggy,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !buggy {
                continue;
            }
            let v = BuggyVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (_, plan) = body_for(s).expect("a buggy seed has a body");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("a buggy trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                buggy::stinger_mouth(&buggy::with_variant(plan, v)),
                "seed {s}: her aura does not leave her stinger's mouth"
            );
        }
        assert_eq!(
            seen.len(),
            BuggyVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// Every cyclecar the family can draw at the blueprint corners: every
    /// ornateness-by-wear pair (#1376). Returns the built tree, its plan and
    /// a label for the failure.
    fn every_cyclecar() -> Vec<(Generator, cyclecar::CyclecarPlan, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            let plan = cyclecar::plan_of(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push((
                        cyclecar::build_dressed(&ctx, &plan),
                        plan,
                        format!("a {} m cyclecar, {} / {}", bp.length, o.label(), w.label()),
                    ));
                }
            }
        }
        assert_eq!(out.len(), 6 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built cyclecar meets another and the whole machine is
    /// one component, on every tier at every blueprint corner (#1376) - on
    /// the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// What the guard cannot see: it ignores `path_cut`, so the spat's box
    /// reaches under the ground and the window band's sectors are judged as
    /// whole tubes, which lie inside the pod (#1393).
    #[test]
    fn a_cyclecar_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_cyclecar() {
            let json = serde_json::to_string(&built).expect("a cyclecar serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every cyclecar survives the record sanitiser UNCHANGED at the extremes
    /// of her own blueprint, on every tier (#1359 rule 8) - her rotated tyres
    /// and lamps compared through the quaternion epsilon the sanitiser's
    /// renormalising needs.
    #[test]
    fn a_cyclecar_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_cyclecar() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every cyclecar stands on her three wheels, under the air draft and
    /// inside the gateway, at every blueprint corner (#1376): each wheel's
    /// centre is its radius over the ground - the single rear one on the
    /// centreline as much as the front pair - the spat's cut plane stands
    /// over the ground, nothing she draws stands higher than rule 6's air-draft
    /// cap, and she is narrower than the gateway mouth.
    #[test]
    fn a_cyclecar_stands_on_her_three_wheels_under_the_air_draft() {
        use super::super::common::touch;
        for (built, plan, what) in every_cyclecar() {
            let wheels = plan.wheels();
            assert_eq!(
                wheels.len(),
                3,
                "{what}: she rolls on {} wheels",
                wheels.len()
            );
            assert_eq!(
                wheels.iter().filter(|(at, _)| at[0] == 0.0).count(),
                1,
                "{what}: she has no single wheel on the centreline"
            );
            for (at, r) in wheels {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let cut = cyclecar::spat_cut_y(&plan) + plan.datum_height();
            assert!(
                cut > 0.0,
                "{what}: the spat's cut plane is {cut} m over the ground"
            );
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT_CAP} m air draft"
            );
            let wide = cyclecar::Cyclecar.overall_width(&plan, 0);
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
        }
    }

    /// Every cyclecar seed is drawn as a cyclecar, electric (#1376) - and no
    /// other skiff seed is: a skiff is electric exactly when it is drawn as
    /// one. Her neon themes' haze is the one aura she carries, on the record,
    /// over her roof; a Solarpunk or CivicCampus cyclecar's exhaust floor is
    /// dropped, so she carries no emitter at all.
    #[test]
    fn a_cyclecar_seed_draws_a_cyclecar() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{AvatarCharacter, ThemeArchetype};
        let (mut hazy, mut clear) = (0, 0);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let cyclecar = SkiffType::for_seed(s) == SkiffType::Cyclecar;
            assert_eq!(
                propulsion(s) == Propulsion::Electric,
                cyclecar,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !cyclecar {
                continue;
            }
            let (record, _) = super::super::build_for_seed(s);
            let emitters: Vec<_> = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .filter(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .collect();
            match AvatarCharacter::for_seed(s).style {
                ThemeArchetype::Cyberpunk | ThemeArchetype::AlienMonolithic => {
                    assert_eq!(emitters.len(), 1, "seed {s}: no neon haze");
                    assert_eq!(
                        Some(emitters[0].transform.translation.0),
                        fx_mount(s, ParticleAura::NeonHaze),
                        "seed {s}: her haze is not over her roof"
                    );
                    hazy += 1;
                }
                ThemeArchetype::Solarpunk | ThemeArchetype::CivicCampus => {
                    assert!(
                        emitters.is_empty(),
                        "seed {s}: an electric pod trails an aura"
                    );
                    clear += 1;
                }
                style => panic!("seed {s}: a cyclecar on {style:?}"),
            }
        }
        assert!(
            hazy > 10 && clear > 10,
            "{hazy} hazy cyclecars, {clear} clear"
        );
    }

    /// Every armoured car the family can draw at the blueprint corners: every
    /// variant crossed with every ornateness-by-wear pair (#1375). Returns
    /// the built tree, its plan and a label for the failure.
    fn every_armoured() -> Vec<(Generator, armoured::ArmouredPlan, String)> {
        use crate::seeded_defaults::{ArmouredVariant, OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for variant in ArmouredVariant::ALL {
                let plan = armoured::plan_of(&bp, variant);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            armoured::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                variant.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 2 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built armoured car meets another and the whole machine
    /// is one component, on every variant and tier at every blueprint corner
    /// (#1375) - on the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// Her contact is by CONSTRUCTION rather than by fit, and it has to be:
    /// the guard reads a Wedge and a Bevel as their BOX, so her `taper`,
    /// `taper_bottom` and chamfers are all invisible to it and it is generous
    /// about every plate she carries. Each mount is bedded against the DRAWN
    /// flank at its own height ([`armoured::ArmouredPlan::flank_x`]), which
    /// lies inside that box; and the spare, the stern rails and the tail
    /// lamps reach inside the stern PLANE, which a swept form's ball would
    /// have met on its own.
    #[test]
    fn an_armoured_car_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_armoured() {
            let json = serde_json::to_string(&built).expect("an armoured car serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every armoured car survives the record sanitiser UNCHANGED at the
    /// extremes of her own blueprint, on every variant and tier (#1359 rule
    /// 8).
    ///
    /// She carries no node scale at all - the first type that does not - so
    /// what is at risk here is her TAPERS and her chamfers, which the
    /// sanitiser clamps, and the rotations she authors: her slits, her flash,
    /// her bins, her cans and an Ornate machine's thrown-open hatch, compared
    /// through the quaternion epsilon the sanitiser's renormalising needs.
    #[test]
    fn an_armoured_car_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_armoured() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every armoured car stands on her four wheels with her hull clear of the
    /// ground, under the air draft and inside the gateway, at every blueprint
    /// corner (#1375).
    ///
    /// The ground clearance is the point of it: her section is DERIVED from
    /// the roof she stands under and the floor she stands over, and with a
    /// constant section instead that clearance swings 3.4x across these six
    /// corners. So this reads the DRAWN tree's lowest plate rather than the
    /// arithmetic that held it - the tyres excepted, which are the only thing
    /// of hers that touches the ground.
    #[test]
    fn an_armoured_car_stands_clear_of_the_ground_under_the_air_draft() {
        use super::super::common::touch;
        for (built, plan, what) in every_armoured() {
            let wheels = plan.wheels();
            assert_eq!(
                wheels.len(),
                4,
                "{what}: she rolls on {} wheels",
                wheels.len()
            );
            for (at, r) in wheels {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let floor = plan.sill_at(0.0) + plan.datum_height();
            assert!(
                floor > 0.10,
                "{what}: the hull's floor is {floor} m over the ground"
            );
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT_CAP} m air draft"
            );
            let wide = armoured::Armoured.overall_width(&plan, 0);
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
        }
    }

    /// Every armoured-car seed is drawn as an armoured car, on the variant her
    /// theme picks, a diesel (#1375) - and no other skiff seed is: a skiff is
    /// a diesel exactly when she is drawn as one. And the aura her record
    /// carries - her theme's steam, embers or exhaust, none of them folded -
    /// leaves her own drawn pipe's mouth.
    #[test]
    fn an_armoured_car_seed_draws_an_armoured_car() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::ArmouredVariant;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let car = SkiffType::for_seed(s) == SkiffType::ArmouredCar;
            assert_eq!(
                propulsion(s) == Propulsion::Diesel,
                car,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !car {
                continue;
            }
            let v = ArmouredVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (_, plan) = body_for(s).expect("an armoured-car seed has a body");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("an armoured car trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                armoured::pipe_mouth(&armoured::with_variant(plan, v)),
                "seed {s}: her aura does not leave her pipe's mouth"
            );
        }
        assert_eq!(
            seen.len(),
            ArmouredVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// Every rover the family can draw at the blueprint corners: every
    /// variant crossed with every ornateness-by-wear pair (#1378). Returns
    /// the built tree, its plan and a label for the failure.
    fn every_rover() -> Vec<(Generator, rover::RoverPlan, String)> {
        use crate::seeded_defaults::{OrnatenessTier, RoverVariant, WearTier};
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        let mut out = Vec::new();
        for bp in corners() {
            for variant in RoverVariant::ALL {
                let plan = rover::plan_of(&bp, variant);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            rover::build_dressed(&ctx, &plan),
                            plan,
                            format!(
                                "a {} m {}, {} / {}",
                                bp.length,
                                variant.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        assert_eq!(out.len(), 6 * 3 * 9, "the sweep lost a combination");
        out
    }

    /// Every part of a built rover meets another and the whole machine is
    /// one component, on every variant and tier at every blueprint corner
    /// (#1378) - on the tree AS SAVED, through the record's 0.1 mm wire.
    ///
    /// Her linkage is what this is really for. Her front and rear axles lie
    /// BEYOND the deck's own ends, so the arms that reach them start over
    /// open air: drawn as straight stubs off the deck's flank she comes
    /// apart into three components, and the rocker-bogie through shared
    /// joints is what makes her one machine. Nothing of hers is fattened to
    /// meet - every member ends ON a centreline another member runs along
    /// (the buggy's law, #1374) - and the differential bar athwart both
    /// rocker pivots is what carries the whole linkage back to the deck.
    ///
    /// What the guard cannot see: it reads a Bevel and a Superellipsoid as
    /// their BOX, so her deck's taper and chamfer and her shell's curvature
    /// are invisible to it and it is generous about every plate she carries.
    /// Her contact is by CONSTRUCTION instead: every stem, pedestal, mast
    /// and box reaches INSIDE what it stands on, and the dorsal ridge stands
    /// off the shell's own drawn CROWN rather than off its box's flat top
    /// (#1393).
    #[test]
    fn a_rover_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_rover() {
            let json = serde_json::to_string(&built).expect("a rover serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// Every rover survives the record sanitiser UNCHANGED at the extremes
    /// of her own blueprint, on every variant and tier (#1359 rule 8).
    ///
    /// She carries no node scale at all - the second type that does not -
    /// so what is at risk here is her ROTATED nodes (the solar panel's frame
    /// and its cell plates, the wing panel, the dish, and all six wheels),
    /// compared through the quaternion epsilon the sanitiser's renormalising
    /// needs; her deck's taper and chamfers, which the sanitiser clamps; and
    /// the antenna whip's TIP radius, which is under [`MIN_DIM`] on every
    /// rover shorter than 2.44 m and is floored by `line` before the
    /// sanitiser can rewrite it.
    #[test]
    fn a_rover_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        for (built, _, what) in every_rover() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
        }
    }

    /// Every rover stands on all SIX wheels with her deck clear of the
    /// ground, under the air draft and inside the gateway, at every
    /// blueprint corner (#1378).
    ///
    /// The six are the point of the first half: three paired axles evenly
    /// spaced, and the MIDDLE pair stand on the ground by the plan's own
    /// arithmetic exactly as the outer four do - nothing special-cases
    /// them, and nothing in locomotion reads them at all.
    ///
    /// The air draft is the point of the second. The tallest thing she
    /// carries is an Ornate machine's antenna BEACON, and it is held under
    /// rule 6 by construction: the whip's tip is clamped a cap's half-height
    /// under the line so the drawn top lands at 2.740 m at the 3.60 m
    /// corner, 6 cm to spare. This reads the DRAWN tree rather than the
    /// arithmetic that held it.
    ///
    /// And her ground clearance is the point of the third: her section is
    /// DERIVED from the top her deck stands at and the floor it stands over,
    /// so the clearance holds at every corner - 0.375 m at the worst of
    /// them, on a machine whose wheels are only 0.78 of the blueprint's.
    #[test]
    fn a_rover_stands_on_six_wheels_under_the_air_draft() {
        use super::super::common::touch;
        for (built, plan, what) in every_rover() {
            let wheels = plan.wheels();
            assert_eq!(
                wheels.len(),
                6,
                "{what}: she rolls on {} wheels",
                wheels.len()
            );
            let mut stations: Vec<f32> = Vec::new();
            for (at, r) in wheels {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
                assert_ne!(at[0], 0.0, "{what}: a rover has no wheel on her centreline");
                if !stations.contains(&at[2]) {
                    stations.push(at[2]);
                }
            }
            assert_eq!(
                stations.len(),
                3,
                "{what}: her six wheels are not three pairs"
            );
            let floor = plan.sill_at(0.0) + plan.datum_height();
            assert!(
                floor > 0.30,
                "{what}: the deck's floor is {floor} m over the ground"
            );
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the {AIR_DRAFT_CAP} m air draft"
            );
            let wide = rover::Rover.overall_width(&plan, 0);
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
        }
    }

    /// Every rover seed is drawn as a rover, on the variant her theme picks,
    /// under a servo (#1378) - and no other skiff seed is: a skiff drives a
    /// servo exactly when she is drawn as one.
    ///
    /// And the aura her record carries is her theme's own flourish, over
    /// whatever she stands on her deck - the carapace's crown, the
    /// monolith's slab - or NOTHING AT ALL on a SpaceOutpost seed, whose
    /// Thruster floors to an exhaust on a skiff and whose servo then drops
    /// it. She has no pipe, and a dust plume off her rear tyres would be a
    /// particle system nobody can see: at 22.9 degrees of look-down nothing
    /// under the machine is ever in frame.
    #[test]
    fn a_rover_seed_draws_a_rover() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{AvatarCharacter, RoverVariant, ThemeArchetype};
        let mut seen = Vec::new();
        let (mut flourish, mut clear) = (0, 0);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let rover = SkiffType::for_seed(s) == SkiffType::Rover;
            assert_eq!(
                propulsion(s) == Propulsion::Servo,
                rover,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !rover {
                continue;
            }
            let v = RoverVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (record, _) = super::super::build_for_seed(s);
            let emitters: Vec<_> = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .filter(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .collect();
            let aura = match AvatarCharacter::for_seed(s).style {
                ThemeArchetype::SpaceOutpost => {
                    assert!(
                        emitters.is_empty(),
                        "seed {s}: a rover with no pipe trails an aura"
                    );
                    clear += 1;
                    continue;
                }
                ThemeArchetype::AlienOrganic => ParticleAura::ArcaneMotes,
                ThemeArchetype::AlienMonolithic => ParticleAura::NeonHaze,
                style => panic!("seed {s}: a rover on {style:?}"),
            };
            assert_eq!(emitters.len(), 1, "seed {s}: no flourish over her deck");
            assert_eq!(
                Some(emitters[0].transform.translation.0),
                fx_mount(s, aura),
                "seed {s}: her flourish is not over her deck"
            );
            flourish += 1;
        }
        assert_eq!(
            seen.len(),
            RoverVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
        assert!(
            flourish > 10 && clear > 10,
            "{flourish} rovers with a flourish, {clear} with none"
        );
    }

    /// Every part of a built roadster meets another, and the whole machine is
    /// one connected component (#1364, the owner's complaint on the
    /// prototype) - on every body, top, wheel and tier it can roll (#1367).
    ///
    /// Swept over the blueprint EXTREMES rather than one seed, because the
    /// failure mode is size-dependent: a bead or a track rod floored at
    /// [`MIN_DIM`] stops shrinking with the body, so a part that touches at
    /// the nominal size can come adrift at the small end - or push through at
    /// the large one. See [`super::super::common::touch`] for why this cannot
    /// be judged by eye: the chase camera looks down, so nothing under a craft
    /// is ever in frame at play distance.
    ///
    /// Checked on the tree AS SAVED - through the record's own 0.1 mm wire -
    /// rather than on the f32 tree in memory, because a part authored exactly
    /// flush with what it stands on touches in f32 and not after the rounding:
    /// the radiator's filler cap did exactly that on seed 134 (#1367 defect 1).
    ///
    /// What it cannot see, and the reason a binary test sits beside it: the
    /// guard models a sweep as a chain of capsules, which bulge past a blunt
    /// end, so a wheel standing clear of a bobtail's back still reads as
    /// touching - see the roadster's tail-mount test.
    #[test]
    fn a_roadster_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        for (built, _, what) in every_roadster() {
            let json = serde_json::to_string(&built).expect("a roadster serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// The roadster stands on her wheels under the air draft and inside the
    /// gateway mouth, on every body, top, wheel and tier at every blueprint
    /// corner (#1382).
    ///
    /// SHE AND THE WAGON ARE THE LAST TWO TYPES TO GET THIS. The two heroes
    /// and the first fan-out type predate the per-type guard set every later
    /// slice landed with - the dune buggy's
    /// [`a_buggy_stands_on_her_wheels_under_the_air_draft`] was the first of
    /// them (#1374, owner decision 11) - so the pattern reached four of the
    /// six machines and skipped the two it started from. Nothing is being
    /// FIXED here: measured before it was written, the tallest roadster the
    /// family can draw stands 1.591 m over the ground at the 3.6 m corner,
    /// against a 2.8 m cap. It is a guard against the next change, not
    /// against this one.
    ///
    /// Read on the DRAWN tree plus the datum, as the other four are, rather
    /// than on the arithmetic that placed the top: a hardtop resolved to a
    /// height and then crowned over it would pass a derivation and fail this.
    #[test]
    fn a_roadster_stands_on_her_wheels_under_the_air_draft() {
        use super::super::common::touch;
        let mut tallest: f32 = 0.0;
        for (built, plan, what) in every_roadster() {
            for (at, r) in plan.wheels() {
                assert!(
                    (at[1] + plan.datum_height() - r).abs() < 1e-5,
                    "{what}: a wheel of radius {r} has its centre {} over the ground",
                    at[1] + plan.datum_height()
                );
            }
            let top = touch::highest(&built) + plan.datum_height();
            assert!(
                top <= AIR_DRAFT_CAP,
                "{what}: she stands {top} m over the ground, past the \
                 {AIR_DRAFT_CAP} m air draft"
            );
            let wide = roadster::Roadster.overall_width(&plan, 0);
            assert!(
                wide < GATEWAY_MOUTH,
                "{what}: {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
            tallest = tallest.max(top);
        }
        // The number the guard was written against, so a change that halves
        // her or doubles her is caught here rather than at the cap.
        assert!(
            (1.4..1.8).contains(&tallest),
            "the tallest roadster is {tallest} m, not the 1.591 m this was \
             measured at - the shape moved, so re-read the cap margin"
        );
    }

    /// Every roadster seed is drawn as a roadster, on the body, top and
    /// wheels her seed picks, under an engine (#1364) - and no other skiff
    /// seed is: a skiff has a plain engine exactly when she is drawn as a
    /// roadster. And the aura her record carries LEAVES HER PIPE MOUTH.
    ///
    /// THIS IS #1382's GAP 2, and it is the reason the test exists. The skiff
    /// exhaust mount was right only BY CONSTRUCTION - `Roadster::fx_mount`
    /// reads `coachwork::exhaust_path`, which is also what sweeps the pipe, so
    /// the two agree because they are the same call. Nothing said so. The boat
    /// has `a_boats_aura_leaves_her_own_hull`; the other four skiffs each got
    /// this as their slice landed (the buggy's stinger, the rover's deck); the
    /// hero was the one machine with no such pin, so an `fx_mount` rewritten
    /// to a fraction of a nominal body - the very defect #1364 item 8 removed
    /// - would have gone unnoticed on her.
    ///
    /// Asked through the RECORD rather than of the mount function, so what is
    /// checked is where the emitter actually ends up.
    #[test]
    fn a_roadster_seed_draws_a_roadster() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{ParticleAura, RoadsterBody, RoadsterTop, RoadsterWheels};
        let (mut seen_body, mut seen_top, mut seen_wheels) = (Vec::new(), Vec::new(), Vec::new());
        let (mut piped, mut flourish, mut bare) = (0, 0, 0);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let roadster = SkiffType::for_seed(s) == SkiffType::Roadster;
            assert_eq!(
                propulsion(s) == Propulsion::Engine,
                roadster,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !roadster {
                continue;
            }
            for (v, seen) in [
                (format!("{:?}", RoadsterBody::for_seed(s)), &mut seen_body),
                (format!("{:?}", RoadsterTop::for_seed(s)), &mut seen_top),
                (
                    format!("{:?}", RoadsterWheels::for_seed(s)),
                    &mut seen_wheels,
                ),
            ] {
                if !seen.contains(&v) {
                    seen.push(v);
                }
            }
            let (record, _) = super::super::build_for_seed(s);
            let emitters: Vec<_> = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .filter(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .collect();
            // A roadster has an engine, so her picked aura survives
            // `drawn_aura` whatever it is - except `ParticleAura::None`,
            // which no theme has to roll.
            let aura = super::super::fx::drawn_aura(
                crate::seeded_defaults::AvatarFx::for_seed(s).aura,
                Propulsion::Engine,
            );
            let Some(want) = fx_mount(s, aura) else {
                bare += 1;
                assert!(emitters.is_empty(), "seed {s}: an aura with no mount");
                continue;
            };
            assert_eq!(emitters.len(), 1, "seed {s}: not exactly one aura");
            let at = emitters[0].transform.translation.0;
            // (a) THE WIRING: the assembler hangs the emitter at the mount the
            // craft published, rather than at a constant of its own.
            assert_eq!(
                at, want,
                "seed {s}: her aura does not leave the mount her body publishes"
            );
            // (b) THE GEOMETRY, which is the half that can actually fail
            // (#1382 gap 2): an exhaust or a steam wisp leaves the mouth of a
            // pipe THAT IS DRAWN. Read off the tree's own sweeps, so a mount
            // recomputed from a fraction of a nominal body - the defect #1364
            // item 8 removed - comes out red here even though (a) still holds.
            if matches!(aura, ParticleAura::Exhaust | ParticleAura::Steam) {
                let tree = record.visuals().expect("a skiff is a tree");
                let ends = sweep_ends(tree);
                let near = ends
                    .iter()
                    .map(|(p, what)| {
                        let d = (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f32>().sqrt();
                        (d, what)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .expect("a roadster draws sweeps");
                assert!(
                    near.0 < 1e-3,
                    "seed {s}: her {aura:?} issues from {at:?}, {} m from the \
                     nearest drawn sweep end ({}) - it is not leaving a pipe \
                     she draws",
                    near.0,
                    near.1
                );
                piped += 1;
            } else {
                flourish += 1;
            }
        }
        assert!(
            piped > 10 && flourish > 10,
            "{piped} roadsters trailed a pipe and {flourish} a flourish - both \
             arms of `fx_mount` have to be reached or half of it is untested"
        );
        assert_eq!(bare, 0, "{bare} roadsters rolled an aura with no mount");
        assert_eq!(
            (seen_body.len(), seen_top.len(), seen_wheels.len()),
            (
                RoadsterBody::ALL.len(),
                RoadsterTop::ALL.len(),
                RoadsterWheels::ALL.len()
            ),
            "the seeds under 3000 miss a pick: {seen_body:?} / {seen_top:?} / {seen_wheels:?}"
        );
    }

    /// Every wagon seed is drawn as a wagon, on the body her theme picks,
    /// ROLLING (#1377) - and no other skiff seed is - and the aura her record
    /// carries sits where her body publishes it: sparks at a lit lantern, any
    /// other flourish over the seat.
    ///
    /// She is the second of the two types that predate the per-type pattern
    /// (see her air-draft guard). Her exhaust half is the interesting one: a
    /// horse-drawn wagon has no pipe, so `drawn_aura` drops a picked Exhaust
    /// or Steam to nothing, and a seed that rolled one carries NO emitter at
    /// all - which this counts rather than skips, because "no aura" is the
    /// answer being guarded.
    #[test]
    fn a_wagon_seed_draws_a_wagon() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{AvatarFx, ParticleAura, WagonBody};
        let mut seen = Vec::new();
        let (mut flourish, mut dropped) = (0, 0);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let wagon = SkiffType::for_seed(s) == SkiffType::Wagon;
            assert_eq!(
                propulsion(s) == Propulsion::Rolling,
                wagon,
                "seed {s}: {:?} drives {:?}",
                SkiffType::for_seed(s),
                propulsion(s)
            );
            if !wagon {
                continue;
            }
            let body = WagonBody::for_seed(s);
            if !seen.contains(&body) {
                seen.push(body);
            }
            let (record, _) = super::super::build_for_seed(s);
            let emitters: Vec<_> = record
                .visuals()
                .expect("a skiff is an assembled tree")
                .children
                .iter()
                .filter(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .collect();
            let aura =
                super::super::fx::drawn_aura(AvatarFx::for_seed(s).aura, Propulsion::Rolling);
            if aura == ParticleAura::None {
                dropped += 1;
                assert!(
                    emitters.is_empty(),
                    "seed {s}: a horse-drawn wagon trails an exhaust"
                );
                continue;
            }
            assert_eq!(emitters.len(), 1, "seed {s}: not exactly one aura");
            let at = emitters[0].transform.translation.0;
            // (a) THE WIRING: the assembler uses the mount the craft
            // published, not a constant.
            assert_eq!(
                Some(at),
                fx_mount(s, aura),
                "seed {s}: her {aura:?} is not where her body publishes it"
            );
            // (b) THE GEOMETRY. Weaker than the roadster's pipe pin, because a
            // lantern's flame and a seat's sparks are not a sweep MOUTH and
            // there is no single drawn feature to name: what is claimed is
            // that the flourish is ON THE MACHINE - inside the drawn envelope
            // across and along, and between the ground and her drawn top. That
            // is the #1364 failure (a mount at a fixed height, hovering over
            // every body the default did not reach), and it is what a mount
            // recomputed off a nominal rather than off this plan would break.
            // A tight per-slot pin for her lantern is not built here.
            let tree = record.visuals().expect("a skiff is a tree");
            let (lo, hi) = origin_bounds(tree);
            let (_, plan) = body_for(s).expect("a wagon seed has a body");
            // ACROSS and ALONG against the drawn node origins: a machine's
            // parts are spread over her whole plan, so origins bound these two
            // axes closely enough to catch a mount that has left her.
            for (ax, name) in [(0usize, "across"), (2, "along")] {
                assert!(
                    at[ax] >= lo[ax] - 0.05 && at[ax] <= hi[ax] + 0.05,
                    "seed {s}: her {aura:?} sits at {} {name}, outside the \
                     machine she is drawn on ({}..{})",
                    at[ax],
                    lo[ax],
                    hi[ax]
                );
            }
            // UP is a WEAKER claim than across and along, and deliberately
            // so. A flourish HOVERS - `bodywork::perch` puts it where a person
            // would be, which is over the seat and in the air - so neither the
            // topmost node origin nor the drawn top is an upper bound on it.
            // Both were tried and both were red on real seeds: origins put
            // seed 240's sparks 0.334 m over the topmost origin (a lantern's
            // node is at the foot of its post), and the drawn surface put seed
            // 96's motes 0.211 m over her drawn top. What is asserted is that
            // the flourish is over the GROUND and inside a machine's own
            // length of her top - which is the #1364 failure (a mount at a
            // fixed height, which on the small end of the blueprint range
            // leaves the machine entirely) without pretending to pin a height
            // this test has no independent derivation for.
            let top = super::super::common::touch::highest(&drawn_only(tree));
            let up = at[1] + plan.datum_height();
            assert!(
                up > 0.0 && at[1] <= top + plan.length,
                "seed {s}: her {aura:?} sits {up} m over the ground against a \
                 drawn top of {} m - under the ground, or adrift over a \
                 {} m machine",
                top + plan.datum_height(),
                plan.length
            );
            flourish += 1;
        }
        assert!(
            flourish > 10 && dropped > 10,
            "{flourish} wagons with a flourish, {dropped} with a dropped exhaust"
        );
        assert_eq!(
            seen.len(),
            WagonBody::ALL.len(),
            "the seeds under 3000 miss a body: {seen:?}"
        );
    }

    /// Every roadster survives the record sanitiser UNCHANGED at the extremes
    /// of its own blueprint, on every body, top, wheel and tier, not only at
    /// the seeds the population happens to contain (#1359 rule 8).
    #[test]
    fn a_skiff_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let mut n = 0;
        for (built, _, what) in every_roadster() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 6 * 3 * 3 * 2 * 9, "the sweep lost a combination");
    }

    /// Building the same seed twice gives the same tree, bit for bit.
    #[test]
    fn a_seeded_skiff_builds_deterministically() {
        for s in (0u64..200).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            assert_eq!(
                build(s, None),
                build(s, None),
                "seed {s} is not deterministic"
            );
        }
    }

    /// A seeded skiff's saved record stays well under the soft budget
    /// (#1359 rule 9), at a THIRD of it.
    ///
    /// Two sweeps, in the sloop's form (#1366). The live seeds, as saved - FX
    /// emitter and engine voice included - and the heaviest thing the family
    /// can draw: every roadster body, top and wheel, every wagon body, every
    /// buggy variant, every armoured-car variant and the cyclecar at every
    /// blueprint corner on the fullest ladder,
    /// Ornate and Battered, carrying the heaviest FX overhead
    /// any live seed carries. Measured rather than assumed, so a new aura that
    /// grows the emitter moves this too. The phase-1 prototype put the worst
    /// of it - an open tourer on wire wheels at 3.6 m - at nine per cent under
    /// the guard (#1367), and it is the wire wheels that spend the margin.
    ///
    /// # It sizes what a SAVE writes (#1382 gap 1)
    ///
    /// The boats' twin, and for the same reason - see
    /// `boats::tests::a_seeded_boats_record_stays_well_inside_the_budget`. It
    /// used to bind `build_for_seed`'s locomotion half to `_` and size the
    /// visual body alone; a publish writes the whole
    /// [`AvatarRecord`](crate::pds::avatar::AvatarRecord), about 770 B more.
    /// The heaviest skiff in the population (seed 1731) is the heaviest saved
    /// record in the fleet, so this is the guard that had the least to spare
    /// and still has over 2 KB of it.
    #[test]
    fn a_seeded_skiffs_record_stays_well_inside_the_budget() {
        use crate::pds::avatar::AvatarRecord;
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        use crate::seeded_defaults::{
            OrnatenessTier, RoadsterBody, RoadsterTop, RoadsterWheels, WearTier,
        };
        let bytes = |t: &Generator| serialized_record_bytes(t).expect("a skiff serializes");
        let (mut worst_seed, mut overhead) = (0usize, 0usize);
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let saved = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
                .expect("a record serializes");
            worst_seed = worst_seed.max(saved);
            overhead = overhead.max(saved.saturating_sub(bytes(&build(s, None))));
        }
        assert!(worst_seed > 0 && overhead > 0, "nothing was measured");
        let mut ctx = PartCtx::for_seed(a_skiff_seed());
        ctx.ornateness = OrnatenessTier::Ornate;
        ctx.wear = WearTier::Battered;
        let mut worst_corner = 0usize;
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    for top in RoadsterTop::ALL {
                        let mut built = roadster::build_dressed(&ctx, &plan, top, rolls);
                        apply_travel_pose(&mut built, travel_drop(&roadster::Roadster, &plan, 0));
                        worst_corner = worst_corner.max(bytes(&built) + overhead);
                    }
                }
            }
        }
        // And the wagon's, every body at every corner, on its fullest ladder
        // (#1377): the heaviest is a 3.6 m Ornate / Battered cart at about
        // 27 KB, twelve spokes a wheel spending most of it.
        let mut worst_wagon = 0usize;
        for bp in corners() {
            for body in crate::seeded_defaults::WagonBody::ALL {
                let plan = wagon::plan_of(&bp, body);
                let mut built = wagon::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&wagon::Wagon, &plan, 0));
                worst_wagon = worst_wagon.max(bytes(&built) + overhead);
            }
        }
        // And the dune buggy's, every variant at every corner on her fullest
        // ladder (#1374): the heaviest is a 3.6 m Ornate / Battered beach
        // buggy at about 28 KB, her canopy's six stripes and her frame's
        // members spending most of it.
        let mut worst_buggy = 0usize;
        for bp in corners() {
            for variant in crate::seeded_defaults::BuggyVariant::ALL {
                let plan = buggy::plan_of(&bp, variant);
                let mut built = buggy::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&buggy::Buggy, &plan, 0));
                worst_buggy = worst_buggy.max(bytes(&built) + overhead);
            }
        }
        // And the cyclecar's, at every corner on her fullest ladder and in
        // the heavier of her one-colour and split pods (#1376): the lightest
        // skiff, about 14 KB, one pod sweep most of her.
        let mut worst_cyclecar = 0usize;
        for bp in corners() {
            let plan = cyclecar::plan_of(&bp);
            let split = crate::pds::avatar::livery::CYCLECAR_LIVERIES
                .iter()
                .position(|l| l.name == "Pearl over obsidian")
                .expect("her two-tone is in her list");
            for livery in [0, split] {
                let mut ctx = ctx;
                ctx.livery = Some(livery);
                let mut built = cyclecar::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&cyclecar::Cyclecar, &plan, 0));
                worst_cyclecar = worst_cyclecar.max(bytes(&built) + overhead);
            }
        }
        // And the armoured car's, every variant at every corner on her
        // fullest ladder (#1375): the heaviest is a 3.6 m Ornate / Battered
        // raider at about 18 KB, her plates and her four wheels most of it.
        // She spends the least of any type on thin tubes, which is what keeps
        // her the second lightest.
        let mut worst_armoured = 0usize;
        for bp in corners() {
            for variant in crate::seeded_defaults::ArmouredVariant::ALL {
                let plan = armoured::plan_of(&bp, variant);
                let mut built = armoured::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&armoured::Armoured, &plan, 0));
                worst_armoured = worst_armoured.max(bytes(&built) + overhead);
            }
        }
        // And the rover's, every variant at every corner on her fullest
        // ladder (#1378): the heaviest is a 3.6 m Ornate / Battered
        // SURVEYOR at about 15 KB, her solar panel and her six wheels most
        // of it. The carapace's chitin shell is the one textured material
        // the family draws and costs 422 B of that.
        let mut worst_rover = 0usize;
        for bp in corners() {
            for variant in crate::seeded_defaults::RoverVariant::ALL {
                let plan = rover::plan_of(&bp, variant);
                let mut built = rover::build_dressed(&ctx, &plan);
                apply_travel_pose(&mut built, travel_drop(&rover::Rover, &plan, 0));
                worst_rover = worst_rover.max(bytes(&built) + overhead);
            }
        }
        for (what, worst) in [
            ("seeded skiff", worst_seed),
            ("fully dressed corner", worst_corner),
            ("fully dressed wagon", worst_wagon),
            ("fully dressed buggy", worst_buggy),
            ("fully dressed cyclecar", worst_cyclecar),
            ("fully dressed armoured car", worst_armoured),
            ("fully dressed rover", worst_rover),
        ] {
            assert!(
                worst * 3 < SOFT_RECORD_BUDGET_BYTES,
                "the heaviest {what} is {worst} bytes, past a third of the \
                 {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
                 spending nodes where it should be spending shape"
            );
        }
    }

    /// No seeded skiff is wider than the narrowest gateway mouth it has to
    /// drive through.
    ///
    /// The skiff's counterpart of the boat's air-draft cap, and the reason the
    /// brief asked for it: visuals carry no colliders, and wheels and guards
    /// stand a long way outboard of coachwork this narrow, so the width is not
    /// the blueprint's `body_w` but the guards' own. Measured rather than
    /// assumed - the legacy fleet reached 2.39 m against this same mouth.
    #[test]
    fn no_seeded_skiff_is_wider_than_the_narrowest_gateway_mouth() {
        let mut worst: f32 = 0.0;
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let (craft, plan) = body_for(s).expect("a skiff seed has a body");
            let w = craft.overall_width(&plan, s);
            assert!(
                w < GATEWAY_MOUTH,
                "seed {s} is {w} m wide, past the {GATEWAY_MOUTH} m mouth"
            );
            worst = worst.max(w);
            checked += 1;
        }
        assert!(checked > 100, "too few skiffs sampled: {checked}");
    }

    /// The datum, the ground and the axle line are one derivation - on every
    /// body and every wheel, balloons included: a balloon tyre is the PLAN's
    /// wheel radius, so the axle rises with it and the tyre still stands on
    /// the ground (#1367).
    #[test]
    fn the_axle_line_is_one_wheel_radius_over_the_ground() {
        use crate::seeded_defaults::{RoadsterBody, RoadsterWheels};
        for bp in corners() {
            for body in RoadsterBody::ALL {
                for rolls in RoadsterWheels::ALL {
                    let plan = roadster::plan_of(&bp, body, rolls);
                    assert!(
                        (plan.axle_y() + plan.datum_height() - plan.wheel_r).abs() < 1e-5,
                        "a {} m {body:?} on {rolls:?}: the axle line is not its wheel \
                         radius over the ground",
                        bp.length
                    );
                    // And the body really does stand where the beltline says.
                    assert!((plan.datum_height() + plan.depth() - plan.beltline).abs() < 1e-5);
                }
            }
        }
    }
}
