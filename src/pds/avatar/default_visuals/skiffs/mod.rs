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
//! **Most skiff seeds draw the roadster floor rather than their own type**, and
//! will until #1377: a horseless wagon takes all ten historic themes, so the
//! Wagon is 31 % of the family against the Roadster's 30. That is expected and
//! the readouts say so.
//!
//! # Where a skiff sits
//!
//! The visual origin is the body's **datum** - the plane the body sweeps are
//! cut on, which is the cockpit coaming line. The ground is
//! [`BodyPlan::datum_height`] below it, and [`travel_drop`] is simply the
//! difference between where the suspension rests the chassis origin and that
//! number, which is what puts the tyres on the suspension's own ground line
//! for the wheels this seed actually rolls on (#1361).

mod plan;
mod roadster;

pub(crate) use plan::BodyPlan;

use crate::pds::avatar::colour::{ensure_delta, floor_value, luma, mix, shade, window_light};
use crate::pds::avatar::locomotion::CarParams;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::seeded_defaults::{ParticleAura, SkiffBlueprint, SkiffType};

use super::assemble::apply_travel_pose;

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
/// chassis slugs used to carry. The roadster's are the old default chassis's
/// exactly, so the drive the owner validated in #1361 is the drive that ships;
/// a real per-type feel sweep is #1381's.
#[derive(Clone, Copy, Debug)]
pub(super) struct SkiffFeel {
    /// Mass over the family's 900 kg baseline, before the size re-basing.
    pub(super) mass_factor: f32,
    pub(super) drive_accel: f32,
    pub(super) turn_accel: f32,
}

/// One buildable kind of land craft.
pub(super) trait SkiffCraft {
    /// This type's body, from the seeded blueprint: its own plan form, section
    /// depth and layout over dimensions everyone shares.
    fn plan(&self, bp: &SkiffBlueprint) -> BodyPlan;

    /// Draw it, at the origin, nose `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, plan: &BodyPlan) -> Generator;

    /// How it drives.
    fn feel(&self) -> SkiffFeel;

    /// The widest the type is actually drawn, guards included (m) - what the
    /// gateway-mouth guard measures, and a type's own knowledge rather than
    /// the plan's, because a guard is the type's choice.
    fn overall_width(&self, plan: &BodyPlan) -> f32;

    /// Where a seeded particle aura issues from, read off the body.
    fn fx_mount(&self, aura: ParticleAura, plan: &BodyPlan) -> [f32; 3];
}

/// The builder for a craft type, or `None` while nothing implements it.
///
/// The seam, and deliberately not a stub (the same reasoning as the boats'):
/// a match over a non-empty enum needs an arm per variant, and an arm that
/// drew *something* for an unimplemented type would be a lie the population
/// census could not see. The unbuilt group is named in full, so adding a type
/// is a compile error here until it is listed - which is what each of
/// #1374-#1378 will do.
fn craft(t: SkiffType) -> Option<&'static dyn SkiffCraft> {
    match t {
        SkiffType::Roadster => Some(&roadster::Roadster),
        SkiffType::DuneBuggy
        | SkiffType::ArmouredCar
        | SkiffType::Cyclecar
        | SkiffType::Wagon
        | SkiffType::Rover => None,
    }
}

/// The builder a seed actually draws with: its own type where that type is
/// built, and the family's universal floor where it is not.
fn craft_for(seed: u64) -> &'static dyn SkiffCraft {
    craft(SkiffType::for_seed(seed)).unwrap_or_else(|| {
        craft(SkiffType::UNIVERSAL).expect("the family's universal floor is always built")
    })
}

/// The seeded body for `seed`, or `None` for a seed that is not a skiff.
fn body_for(seed: u64) -> Option<(&'static dyn SkiffCraft, BodyPlan)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.skiff().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.plan(&bp)))
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
pub(super) fn chassis_half_extents(craft: &dyn SkiffCraft, plan: &BodyPlan) -> [f32; 3] {
    [
        craft.overall_width(plan) * 0.5,
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
fn chassis_ride_height(craft: &dyn SkiffCraft, plan: &BodyPlan) -> f32 {
    let p = CarParams::default();
    chassis_half_extents(craft, plan)[1] + p.suspension_rest_length.0
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
pub(super) fn travel_drop(craft: &dyn SkiffCraft, plan: &BodyPlan) -> f32 {
    chassis_ride_height(craft, plan) - plan.datum_height()
}

/// Assemble the seeded skiff for `seed`, posed for travel.
pub(super) fn build(seed: u64) -> Generator {
    let ctx = PartCtx::for_seed(seed);
    let (craft, plan) = body_for(seed).expect("a skiff seed carries a skiff blueprint");
    let mut root = craft.build(&ctx, &plan);
    // No scale: since #1364 a skiff is authored at the size she is drawn at,
    // so the airship-class bridge the legacy pipeline carried has nothing left
    // to convert - and the skiff was the last family holding one.
    apply_travel_pose(&mut root, travel_drop(craft, &plan));
    root
}

/// How the seeded skiff for `seed` drives, and the collider box it drives in.
pub(super) fn feel_and_box(seed: u64) -> (SkiffFeel, [f32; 3]) {
    match body_for(seed) {
        Some((craft, plan)) => (craft.feel(), chassis_half_extents(craft, &plan)),
        // Defensive: a skiff seed always has a blueprint. Falling back to the
        // floor's own feel keeps a locomotion query total rather than
        // panicking in a sanitiser round-trip that exercises the family
        // off-seed.
        None => {
            let craft = craft(SkiffType::UNIVERSAL).expect("the floor is always built");
            (craft.feel(), [0.66, 0.29, 1.33])
        }
    }
}

/// How far the ground is under the visual origin for `seed` (m), or `None` for
/// a seed that is not a skiff - the number [`travel_drop`] places against, and
/// what the family's pose test checks the assembled tree against.
#[cfg(test)]
pub(super) fn datum_height_for_seed(seed: u64) -> Option<f32> {
    body_for(seed).map(|(_, plan)| plan.datum_height())
}

/// Where a seeded skiff's particle aura issues from (root-local, before the
/// travel pose) - read off its own body by the craft type that drew it, so an
/// exhaust wisp leaves the pipe mouth of the machine that is actually there.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan) = body_for(seed)?;
    Some(craft.fx_mount(aura, &plan))
}

// ---------------------------------------------------------------------------
// Finishes
// ---------------------------------------------------------------------------

/// The surfaces a skiff is finished in. One plain livery: the coachwork takes
/// the seeded primary accent, the guards take the same paint darkened, and
/// everything else is the colour that thing actually is. Heritage liveries
/// with the seeded accent on identity trim are their own slice (#1365).
pub(crate) struct SkiffColours {
    pub(crate) paint: SovereignMaterialSettings,
    /// Wings and running boards - coachwork, not rubber. See [`skiff_colours`].
    pub(crate) guard: SovereignMaterialSettings,
    pub(crate) rubber: SovereignMaterialSettings,
    pub(crate) brightwork: SovereignMaterialSettings,
    pub(crate) leather: SovereignMaterialSettings,
    /// Wheel discs and hub caps.
    pub(crate) disc: SovereignMaterialSettings,
    /// Axles, louvres, the radiator matrix - the dark machinery.
    pub(crate) machinery: SovereignMaterialSettings,
    pub(crate) lamp: SovereignMaterialSettings,
    pub(crate) tail_lamp: SovereignMaterialSettings,
}

/// The colours those things are, fixed rather than seeded, for the same reason
/// the sloop's timber and canvas are: a machine whose guards are the same hue
/// as its tyres has no guards at play distance, and the palette cannot promise
/// a contrast it does not know about.
const TYRE: [f32; 3] = [0.045, 0.045, 0.050];
const CHROME: [f32; 3] = [0.80, 0.80, 0.82];
const HIDE: [f32; 3] = [0.42, 0.24, 0.13];
const CREAM: [f32; 3] = [0.84, 0.80, 0.68];
const MACHINERY: [f32; 3] = [0.10, 0.10, 0.11];
const TAIL_LAMP: [f32; 3] = [0.90, 0.10, 0.08];

/// The value a guard is floored at.
///
/// The one colour rule on this family, and it was found by render (#1364). A
/// guard drawn near-black - which is what a period photograph suggests and
/// what the prototype first did - carries the TYRE's own value, so the eye
/// merges the two and the four wheels read as detached blobs with nothing over
/// them. Coachwork has to stay clear of rubber.
const GUARD_FLOOR: f32 = 0.11;

pub(crate) fn skiff_colours(ctx: &PartCtx) -> SkiffColours {
    let p = &ctx.palette;
    let m = &ctx.materials;
    // Coachwork can be genuinely dark - a racing green or a maroon is the
    // point of this machine - so the accent is only floored off black rather
    // than lifted the way a boat's topsides are.
    let body = floor_value(p.primary_accent, 0.20);
    SkiffColours {
        paint: m.paint(body),
        guard: m.paint(floor_value(shade(body, 0.62), GUARD_FLOOR)),
        rubber: m.rubber(TYRE),
        brightwork: m.brightwork(CHROME),
        // A little of the seed's own secondary through the hide and the discs,
        // so two machines' interiors are not identical, but not enough to lose
        // what they are.
        leather: m.leather(mix(HIDE, shade(p.secondary_accent, 0.7), 0.20)),
        disc: m.paint(ensure_delta(
            mix(CREAM, p.secondary_accent, 0.18),
            luma(body),
            0.22,
        )),
        machinery: m.paint(MACHINERY),
        lamp: crate::pds::avatar::colour::window_material(window_light(p.tertiary_accent)),
        tail_lamp: m.glow(TAIL_LAMP),
    }
}

#[cfg(test)]
mod tests {
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

    /// Where two trees first disagree, as a path plus what moved - so a
    /// sanitiser rewrite names the part it touched instead of printing two
    /// whole machines.
    fn first_difference(a: &Generator, b: &Generator, path: &str) -> Option<String> {
        if a.kind != b.kind {
            return Some(format!(
                "{path} ({}): kind\n  {:?}\n  {:?}",
                a.kind.kind_tag(),
                a.kind,
                b.kind
            ));
        }
        // Rotations are compared with an epsilon: the sanitiser renormalises
        // every quaternion, which moves the last ulp of an already-normalised
        // one. `quat_x(FRAC_PI_2)` is exactly that case - sin and cos of a
        // quarter turn are both 0.70710677 and the pair's norm is a hair under
        // one - and the sloop never met it because she authors no rotated
        // node, where a car is nothing but rotated nodes.
        let turned =
            (0..4).any(|i| (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() > 1e-5);
        if turned
            || a.transform.translation != b.transform.translation
            || a.transform.scale != b.transform.scale
        {
            return Some(format!(
                "{path} ({}): transform {:?} -> {:?}",
                a.kind.kind_tag(),
                a.transform,
                b.transform
            ));
        }
        if a.children.len() != b.children.len() {
            return Some(format!("{path}: child count"));
        }
        a.children
            .iter()
            .zip(b.children.iter())
            .enumerate()
            .find_map(|(i, (ca, cb))| first_difference(ca, cb, &format!("{path}/{i}")))
    }

    fn a_skiff_seed() -> u64 {
        (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff)
            .expect("some seed is a skiff")
    }

    /// The seam (#1362 → #1364). [`SkiffType::implemented`] is what the
    /// readouts and the fan-out slices ask; [`craft`] is what actually draws.
    /// They are two matches over one enum, so pin them together - the failure
    /// they prevent is a type that says it is built and silently draws a
    /// roadster.
    #[test]
    fn a_skiff_type_is_implemented_exactly_when_something_builds_it() {
        for t in SkiffType::ALL {
            assert_eq!(
                craft(t).is_some(),
                t.implemented(),
                "{t:?}: `implemented()` and the builder table disagree"
            );
        }
        assert!(
            SkiffType::UNIVERSAL.implemented(),
            "the universal floor must be built - every unbuilt pick resolves to it"
        );
    }

    /// Every skiff seed draws a skiff, whatever type it picked. This is what
    /// "go live for every skiff seed" means, and the unbuilt types are the
    /// reason it needs saying - most of the family is unbuilt, because the
    /// Wagon alone outweighs the hero.
    #[test]
    fn every_skiff_seed_resolves_to_a_built_craft() {
        let (mut unbuilt, mut total) = (0, 0);
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            if !SkiffType::for_seed(s).implemented() {
                unbuilt += 1;
            }
            total += 1;
            let (_, plan) = body_for(s).expect("a skiff seed has a body");
            assert!(plan.length > 1.0, "seed {s}: degenerate body");
        }
        assert!(total > 50, "too few skiffs sampled: {total}");
        assert!(
            unbuilt * 2 > total,
            "only {unbuilt} of {total} skiff seeds picked an unbuilt type - the \
             floor fallback is what most of this family draws, so if that has \
             stopped being true the population has moved"
        );
    }

    /// Every part of a built skiff meets another, and the whole machine is one
    /// connected component (#1364, the owner's complaint on the prototype).
    ///
    /// Swept over the blueprint EXTREMES rather than one seed, because the
    /// failure mode is size-dependent: a bead or a track rod floored at
    /// [`MIN_DIM`] stops shrinking with the body, so a part that touches at
    /// the nominal size can come adrift at the small end - or push through at
    /// the large one. See [`super::super::common::touch`] for why this cannot
    /// be judged by eye: the chase camera looks down, so nothing under a craft
    /// is ever in frame at play distance.
    #[test]
    fn a_roadster_is_one_machine_at_every_blueprint_extreme() {
        use super::super::common::touch;
        let ctx = PartCtx::for_seed(a_skiff_seed());
        let craft = craft(SkiffType::UNIVERSAL).expect("the floor is built");
        for bp in corners() {
            let built = craft.build(&ctx, &craft.plan(&bp));
            touch::assert_one_machine(&built, &format!("a {} m roadster", bp.length));
        }
    }

    /// Every skiff survives the record sanitiser UNCHANGED at the extremes of
    /// her own blueprint, not only at the seeds the population happens to
    /// contain (#1359 rule 8).
    #[test]
    fn a_skiff_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let ctx = PartCtx::for_seed(a_skiff_seed());
        let craft = craft(SkiffType::UNIVERSAL).expect("the floor is built");
        for bp in corners() {
            let built = craft.build(&ctx, &craft.plan(&bp));
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!(
                    "a {} m machine at body {} was rewritten by the sanitiser at {where_}",
                    bp.length, bp.body_w
                );
            }
        }
    }

    /// Building the same seed twice gives the same tree, bit for bit.
    #[test]
    fn a_seeded_skiff_builds_deterministically() {
        for s in (0u64..200).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            assert_eq!(build(s), build(s), "seed {s} is not deterministic");
        }
    }

    /// A seeded skiff's saved record stays well under the soft budget
    /// (#1359 rule 9).
    #[test]
    fn a_seeded_skiffs_record_stays_well_inside_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        let mut worst = 0usize;
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            worst =
                worst.max(serialized_record_bytes(&build(s)).expect("a built skiff serializes"));
        }
        assert!(worst > 0, "no skiff seed was measured");
        assert!(
            worst * 3 < SOFT_RECORD_BUDGET_BYTES,
            "the heaviest seeded skiff is {worst} bytes, past a third of the \
             {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
             spending nodes where it should be spending shape"
        );
    }

    /// No seeded skiff is wider than the narrowest gateway mouth it has to
    /// drive through.
    ///
    /// The skiff's counterpart of the boat's air-draft cap, and the reason the
    /// brief asked for it: visuals carry no colliders, and wheels and guards
    /// stand a long way outboard of coachwork this narrow, so the width is not
    /// the blueprint's `body_w` but the guards' own. Measured rather than
    /// assumed - the legacy fleet reached 2.39 m against this same 2.6 m.
    #[test]
    fn no_seeded_skiff_is_wider_than_the_narrowest_gateway_mouth() {
        /// The narrowest mouth on a seeded gateway (m) - see #1359 rule 6.
        const MOUTH: f32 = 2.6;
        let mut worst: f32 = 0.0;
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let (craft, plan) = body_for(s).expect("a skiff seed has a body");
            let w = craft.overall_width(&plan);
            assert!(
                w < MOUTH,
                "seed {s} is {w} m wide, past the {MOUTH} m mouth"
            );
            worst = worst.max(w);
            checked += 1;
        }
        assert!(checked > 100, "too few skiffs sampled: {checked}");
    }

    /// The datum, the ground and the axle line are one derivation.
    #[test]
    fn the_axle_line_is_one_wheel_radius_over_the_ground() {
        for bp in corners() {
            let plan = roadster::Roadster.plan(&bp);
            assert!(
                (plan.axle_y() + plan.datum_height() - plan.wheel_r).abs() < 1e-5,
                "a {} m machine's axle line is not its wheel radius over the ground",
                bp.length
            );
            // And the body really does stand where the beltline says.
            assert!((plan.datum_height() + plan.depth() - plan.beltline).abs() < 1e-5);
        }
    }
}
