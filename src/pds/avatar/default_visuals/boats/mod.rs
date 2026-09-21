//! The seeded boat family: one builder per craft type, over one hull profile.
//!
//! Replaces the part-assembled hover-boat pipeline of #778, which drew a hull
//! as a blob iso-surface and bolted a deck, a mast and a funnel onto guessed
//! fractions of it. Nothing could be predicted off that hull, so every trim
//! line floated and every mount needed an embed fudge factor; and at 1.32 m it
//! was a toy beside a 1.7 m person (#1359 diagnosis, owner decision 1).
//!
//! # A type is a builder, not an arrangement
//!
//! [`BoatType`] (#1362) is the discrete pick inside the family, and this is
//! where it becomes geometry. One struct per type implements [`BoatCraft`],
//! one file per type, and [`craft`] is the only match over the enum - the
//! central [`build_for_seed`](super::build_for_seed) just delegates.
//!
//! The sloop, the runabout (#1372), the scow (#1373), the steam tug (#1370),
//! the junk (#1371) and the longship (#1369) are built - which since the
//! longship is ALL SIX, so no boat seed anywhere is drawn as somebody else's
//! craft. [`craft`] is the one match that says so. The PICK itself has always
//! been a property of the seed, so `render --outfit` and `render
//! --family-seeds --craft` answered for every type before anyone had drawn
//! them.
//!
//! # Where a boat sits
//!
//! The visual origin is the hull's **design waterline**. On water, buoyancy
//! rests a floating chassis origin [`TRAVEL_DROP`] above the surface, so the
//! assembler hangs the visual that far under the chassis and the boat floats
//! on her own painted waterline. On land there is no buoyancy plane to meet,
//! so the suspension holds her at [`land_ride_height`] instead - the drop,
//! plus her draft, plus a clearance that keeps the keel off the ground. The
//! two are a pair, and both are derived (#1361).

mod junk;
mod longship;
mod profile;
mod runabout;
mod scow;
mod shape;
mod sloop;
mod tug;

pub(crate) use profile::HullProfile;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{BoatBlueprint, BoatType, ParticleAura};

/// The boat family's colours, which live in the fleet's one livery home
/// (#1365) rather than beside its geometry - the sloop and every type after
/// her read them through here.
pub(crate) use crate::pds::avatar::livery::{
    BoatColours, JunkColours, LongshipColours, RunaboutColours, ScowColours, TugColours,
    boat_colours, junk_colours, longship_colours, runabout_colours, scow_colours, tug_colours,
};

use super::Propulsion;
use super::assemble::apply_travel_pose;

/// Travel-pose drop (m): how far under the chassis origin the assembler hangs
/// the hull's design waterline.
///
/// **Derived, not tuned** (#1361). Buoyancy rests a floating chassis origin
/// exactly `water_rest_length` above the surface (`player::hover_boat`), so
/// dropping the visual by that same distance is the one value that lands the
/// design waterline *on* the water - the boat floats on her own painted
/// waterline instead of wading 0.1 m deep.
pub(super) const TRAVEL_DROP: f32 = crate::config::rover::WATER_REST_LENGTH;

/// Keel clearance over the suspension ground line, as a fraction of the
/// hull's draft - what makes a hover-boat read as *hovering* rather than
/// beached. A fraction rather than a constant so the hover reads the same at
/// any size.
const KEEL_CLEARANCE_FRAC: f32 = 0.25;

#[cfg(test)]
pub(crate) use super::GATEWAY_MOUTH;
/// The gate both families drive through - one home, in the module that owns
/// them both (#1382). Re-exported here rather than restated so the rigs that
/// resolve a mast against the cap keep reaching it as `super::super::`.
pub(crate) use super::{AIR_DRAFT_CAP, AIR_DRAFT_MARGIN};

/// Smallest dimension anything on a boat is built at (m).
///
/// A hair over the sanitiser's own 0.01 m floor. Every radius and every
/// thickness here scales with the hull, so the smallest seeded boat would
/// otherwise draw a masthead or a shroud *under* that floor - and a part the
/// sanitiser CHANGES fails the round-trip the family owes (#1359 rule 8).
/// Flooring it here instead means the record round-trips untouched at every
/// blueprint extreme.
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// Height (m) above flat ground a hull of this `draft` wants its **chassis
/// origin** at.
///
/// The other half of [`TRAVEL_DROP`]: on land there is no buoyancy plane to
/// meet, so the hull is placed by her keel instead. Under the chassis origin
/// go the drop, then the draft, then the clearance.
/// [`boat_locomotion`](super::boat_locomotion) turns this into the suspension
/// rest length that actually holds her there - the two are a pair, and a boat
/// twice the size needs both or she reads as beached.
pub(super) fn land_ride_height(draft: f32) -> f32 {
    TRAVEL_DROP + draft * (1.0 + KEEL_CLEARANCE_FRAC)
}

/// How far the design waterline floats above flat ground on land (m) - the
/// ride height less the drop, and so the height every rig is measured against
/// when it is checked against [`AIR_DRAFT_CAP`].
pub(crate) fn hover(draft: f32) -> f32 {
    land_ride_height(draft) - TRAVEL_DROP
}

/// How a craft type drives: the numbers
/// [`boat_locomotion`](super::boat_locomotion) scales its preset by.
///
/// Per type rather than per hull arrangement, which is what the arrangements
/// used to carry. The sloop's are the old monohull's exactly, so the feel the
/// owner validated in #1361 is unchanged; a real per-type feel sweep is
/// #1381's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoatFeel {
    /// Mass over the family's 50 kg baseline, before the size re-basing.
    pub(super) mass_factor: f32,
    pub(super) drive_accel: f32,
    pub(super) turn_accel: f32,
    pub(super) linear_damping: f32,
    pub(super) angular_damping: f32,
}

/// What a craft type does at rest: how she rides a swell, and how far she
/// lists doing it (#1381).
///
/// Both are MULTIPLIERS on what the seed already gives her, not absolutes.
/// The seeded `idle_sway_amplitude` (0.005-0.025 m) is her individuality
/// and the sweep had no quarrel with it - the finding was that one swell
/// rocked a longship and a laden scow alike. So a type says how much of
/// her own swell she takes, and `super::seeded_gait` folds it in.
///
/// # Why heave and list are two numbers and one field
///
/// `advance_boat` takes BOTH from `idle_sway_amplitude` - heave is
/// `amp x 3.0` m and list is `amp x 3.5` rad - so a scow that heaves x1.1
/// and lists x0.4 cannot be written through that one field at all. Since
/// #1381 the list rides the record's ANGULAR field instead (the humanoid's
/// head turn, the airship's nose wander, and now the boat's list), which
/// is a meaning per profile rather than a new field on the wire. The
/// seeded sloop writes hers so her list comes out exactly `amp x 3.5` rad
/// as before, and a boat record published before the port keeps a list
/// inside the band she had. See `player::gait::boat_list`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoatIdle {
    /// Vertical bob, as a multiple of what the sloop does on the same seed.
    pub(super) heave: f32,
    /// Roll about the fore-aft axis, likewise.
    pub(super) list: f32,
}

/// One buildable kind of boat.
pub(super) trait BoatCraft {
    /// This type's hull, from the seeded blueprint: her own plan form and
    /// section depth over dimensions everyone shares. The seed is here
    /// because a type may carry more than one plan form (the sloop's four,
    /// #1366), and which one a boat is built on is a property of the seed.
    fn profile(&self, bp: &BoatBlueprint, seed: u64) -> HullProfile;

    /// Draw her, at the origin, bow `+Z`, in true metres. The caller owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator;

    /// How she drives.
    fn feel(&self) -> BoatFeel;

    /// How she lies at rest. Required with no default, like
    /// [`propulsion`](Self::propulsion): a hull that lands without saying
    /// what a swell does to her does not compile, so no scow inherits a
    /// yacht's roll.
    fn idle(&self) -> BoatIdle;

    /// Where a seeded particle aura issues from, read off the hull. The seed
    /// is here for the same reason it is on [`overall_beam`](Self::overall_beam):
    /// a mount may sit on a part the seed's tiers move - a battered scow's
    /// stovepipe leans, and her steam leaves it where it leans (#1373).
    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile, seed: u64) -> [f32; 3];

    /// How she is driven, and so what she sounds like (#1383). Required, with
    /// no default: a type that lands without saying whether she has an engine
    /// does not compile, so no craft inherits a voice she never had.
    fn propulsion(&self) -> Propulsion;

    /// Her overall beam (m), which her collider is as wide as. Required, like
    /// [`propulsion`](Self::propulsion): a catamaran is half as wide again as
    /// her blueprint's beam (#1372), and a collider that defaulted to the
    /// blueprint would be narrower than the boat drawn round it.
    fn overall_beam(&self, hull: &HullProfile, seed: u64) -> f32;
}

/// The builder for a craft type.
///
/// The one match over [`BoatType`], and deliberately not a stub (#1362's own
/// note): a match over a non-empty enum needs an arm per variant, so ADDING A
/// TYPE IS A COMPILE ERROR HERE until it is listed.
///
/// # It used to return an `Option`, and no longer does (#1382)
///
/// Through the fan-out this returned `None` for a type nothing drew yet, and
/// [`craft_for`] resolved such a pick to [`BoatType::UNIVERSAL`]. That seam is
/// what let #1363 go live for every boat seed rather than half of them, and it
/// closed when the longship (#1369) made every arm `Some` - the skiffs' twin
/// with the rover (#1378).
///
/// The price of collapsing it, said here because this is where the next
/// person will look: a SEVENTH TYPE CANNOT LAND HALF-BUILT. There is no
/// unbuilt state to report and no floor to fall back to, so a new type has to
/// arrive with its builder in the same commit as its enum variant. That is a
/// one-commit slice now that the pattern is ten types old, and it is the
/// trade the seam was carrying.
fn craft(t: BoatType) -> &'static dyn BoatCraft {
    match t {
        BoatType::Sloop => &sloop::Sloop,
        BoatType::Runabout => &runabout::Runabout,
        BoatType::Scow => &scow::Scow,
        BoatType::SteamTug => &tug::Tug,
        BoatType::Junk => &junk::Junk,
        BoatType::Longship => &longship::Longship,
    }
}

/// The builder a seed draws with - its own type's, always.
fn craft_for(seed: u64) -> &'static dyn BoatCraft {
    craft(BoatType::for_seed(seed))
}

/// The idle of the type a seed actually draws with (#1381) - what
/// `super::seeded_gait` folds into the seeded gait section.
pub(super) fn idle_for(seed: u64) -> BoatIdle {
    craft_for(seed).idle()
}

/// Every built type's idle, by name - for the per-type idle guard and the
/// owner's idle page.
#[cfg(test)]
pub(super) fn every_idle() -> Vec<(&'static str, BoatIdle)> {
    BoatType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).idle()))
        .collect()
}

/// Every built type's feel, by name - the per-type feel guard's table half
/// (#1381, `no_two_craft_types_in_a_family_share_a_feel`).
///
/// Test-only, because nothing in the build wants a type's feel except
/// `boat_locomotion`, and that asks the seed's OWN craft for it. A guard
/// about the whole family is the one reader that needs them all at once.
#[cfg(test)]
pub(super) fn every_feel() -> Vec<(&'static str, BoatFeel)> {
    BoatType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).feel()))
        .collect()
}

/// The seeded hull for `seed`, or `None` for a seed that is not a boat.
fn hull_for(seed: u64) -> Option<(&'static dyn BoatCraft, HullProfile)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.boat().copied())?;
    let craft = craft_for(seed);
    Some((craft, craft.profile(&bp, seed)))
}

/// Assemble the seeded boat for `seed`, posed for travel.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let (craft, hull) = hull_for(seed).expect("a boat seed carries a boat blueprint");
    let mut root = craft.build(&ctx, &hull);
    // No scale: since #1363 a boat is authored at the size she is drawn at, so
    // the airship-class bridge the legacy pipeline carried has nothing left to
    // convert.
    apply_travel_pose(&mut root, TRAVEL_DROP);
    root
}

/// How the seeded boat for `seed` drives, how deep she floats, and how wide
/// she is drawn (m) - `None` for the blueprint's own beam.
pub(super) fn feel_and_draft(seed: u64) -> (BoatFeel, f32, Option<f32>) {
    match hull_for(seed) {
        Some((craft, hull)) => (
            craft.feel(),
            hull.draft,
            Some(craft.overall_beam(&hull, seed)),
        ),
        // Defensive: a boat seed always has a blueprint. Falling back to the
        // floor's own feel keeps a locomotion query total rather than panicking
        // in a sanitiser round-trip that exercises the family off-seed.
        None => (craft(BoatType::UNIVERSAL).feel(), 0.28, None),
    }
}

/// How the boat drawn for `seed` is driven - her voice's answer (#1383).
///
/// Asked of the DRAWN craft, never of the picked type. Since #1369 the two
/// agree on every seed, because every type is built; the distinction still
/// matters, because keying a voice to the PICK would have got every unbuilt
/// pick wrong while the fan-out ran, and keying it to the FAMILY would get
/// every type but the sloop wrong now. Total over every seed, as
/// [`craft_for`] is.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded boat's particle aura issues from (root-local, before the
/// travel pose) - read off her own hull rather than from a constant, so a
/// wake leaves the transom of the boat that is actually drawn.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, hull) = hull_for(seed)?;
    Some(craft.fx_mount(aura, &hull, seed))
}

#[cfg(test)]
mod tests {
    use super::super::common::first_difference;
    use super::*;
    use crate::seeded_defaults::{ChassisFamily, RunaboutVariant, ScowLoad, TugVariant};

    /// Every boat seed draws a boat, and draws ITS OWN.
    ///
    /// It used to count the seeds whose pick nothing drew, and assert that
    /// count was zero; since #1382 collapsed [`craft`] there is no unbuilt
    /// state left to count - a pick that had no builder would not compile.
    /// What survives is the half that is still a real claim about the
    /// POPULATION rather than about the match: every one of the six
    /// [`BoatType`]s is PICKED by some seed under 600, so the fleet the owner
    /// actually meets contains all six, and each one's hull is non-degenerate
    /// on every seed that picks it.
    #[test]
    fn every_boat_seed_resolves_to_a_built_craft() {
        let mut total = 0;
        let mut picked: Vec<BoatType> = Vec::new();
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let t = BoatType::for_seed(s);
            if !picked.contains(&t) {
                picked.push(t);
            }
            total += 1;
            let (_, hull) = hull_for(s).expect("a boat seed has a hull");
            assert!(hull.loa > 0.5, "seed {s}: degenerate hull");
        }
        assert!(total > 50, "too few boats sampled: {total}");
        for t in BoatType::ALL {
            assert!(
                picked.contains(&t),
                "no boat seed under 600 picked {t:?} - the census sampled \
                 {} of the {} types",
                picked.len(),
                BoatType::ALL.len()
            );
        }
    }

    /// Every boat-family builder's inputs that the guards below sweep: the
    /// five blueprint corners, crossed with every rig, every hull form and
    /// every ornateness-by-wear pair a seed can roll - which is all of them,
    /// since the two axes are drawn independently. Returns the built tree and
    /// a label for the failure message.
    /// A sloop whose owner is not a Pirate - what every sweep that is not
    /// ABOUT the kit builds, so the kit cannot quietly change what they see.
    const UNKITTED: sloop::SloopKit = sloop::SloopKit { pirate: false };

    fn every_sloop() -> Vec<(Generator, String)> {
        use crate::seeded_defaults::{OrnatenessTier, SloopHull, SloopRig, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            for form in SloopHull::ALL {
                let hull = sloop::profile_of(&bp, form);
                for rig in SloopRig::ALL {
                    for o in OrnatenessTier::ALL {
                        for w in WearTier::ALL {
                            (ctx.ornateness, ctx.wear) = (o, w);
                            out.push((
                                sloop::build_rigged(&ctx, &hull, rig, UNKITTED),
                                format!(
                                    "a {} m {} sloop, {}, {} / {}",
                                    hull.loa,
                                    form.label(),
                                    rig.label(),
                                    o.label(),
                                    w.label()
                                ),
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    /// Every KITTED sloop at the corners that matter - the Pirate kit's own
    /// sweep, and deliberately a hundredth the size of [`every_sloop`].
    ///
    /// THE SWEEP-COST TRAP (#1379): `every_sloop` is 5 corners x 4 hulls x 5
    /// rigs x 9 tier pairs = 900 builds and the dearest test in the family.
    /// Crossing it with the kit would have made it 1800 for one theme. The
    /// kit's ladder tops out at Ornate / Battered - gunports from Adorned and
    /// a tattered fly at Battered - so its FULLEST draw is that one tier pair,
    /// and sweeping only that pair across every corner, hull and rig is 100
    /// builds that reach everything the kit can put on a boat.
    fn every_kitted_sloop() -> Vec<(
        Generator,
        HullProfile,
        crate::seeded_defaults::SloopRig,
        String,
    )> {
        use crate::seeded_defaults::{OrnatenessTier, SloopHull, SloopRig, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        ctx.ornateness = OrnatenessTier::Ornate;
        ctx.wear = WearTier::Battered;
        let kit = sloop::SloopKit { pirate: true };
        let mut out = Vec::new();
        for bp in corners() {
            for form in SloopHull::ALL {
                let hull = sloop::profile_of(&bp, form);
                for rig in SloopRig::ALL {
                    out.push((
                        sloop::build_rigged(&ctx, &hull, rig, kit),
                        hull,
                        rig,
                        format!(
                            "a {} m {} pirate sloop on a {}",
                            hull.loa,
                            form.label(),
                            rig.label()
                        ),
                    ));
                }
            }
        }
        out
    }

    /// The Pirate kit is PIRATE-ONLY - the successor to the retired catalogue
    /// contract test `the_buccaneer_boat_kit_is_pirate_only`, which went with
    /// the boat slugs in #1363 and had nothing to pin since (#1379).
    ///
    /// Over EVERY theme, not over the seeds that happen to roll: the gate is
    /// [`mood::BUCCANEER`], a group of one, and the failure this prevents is
    /// a second theme drifting into that group and quietly putting a jolly
    /// roger on a Nordic longship's owner's sloop.
    ///
    /// The other half of the bargain is in the pick layer:
    /// `seeded_defaults::avatar::craft::tests::
    /// every_theme_reaches_at_least_two_types_per_family` lets Pirate reach
    /// ONE boat type where every other theme must reach two, and this kit is
    /// the reason it may. If the kit ever stopped being Pirate-only, that
    /// exception would have lost its reason - so the two tests are a pair and
    /// each names the other (#1382).
    #[test]
    fn the_pirate_kit_is_pirate_only() {
        use crate::seeded_defaults::ThemeArchetype;
        let mut pirates = 0;
        for style in ThemeArchetype::ALL {
            let kit = sloop::SloopKit::for_style(style);
            let want = style == ThemeArchetype::Pirate;
            assert_eq!(
                kit.pirate,
                want,
                "{style:?} draws {} - the kit is the Pirate's alone",
                kit.label()
            );
            pirates += usize::from(kit.pirate);
        }
        assert_eq!(pirates, 1, "exactly one theme wears the kit");
    }

    /// A seed's kit follows her theme, and a Pirate seed's boat really does
    /// come out heavier than the same seed's would without one - the gate is
    /// wired to the builder and not only to the value.
    #[test]
    fn a_pirate_seed_draws_the_kit_and_no_one_else_does() {
        use crate::seeded_defaults::{AvatarCharacter, BoatType, SloopRig, ThemeArchetype};
        let (mut pirates, mut others) = (0, 0);
        for s in (0u64..1200).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            if BoatType::for_seed(s) != BoatType::Sloop {
                continue;
            }
            let ctx = PartCtx::for_seed(s);
            let hull = sloop_hull_for(s);
            let rig = SloopRig::for_seed(s);
            let kit = sloop::SloopKit::for_seed(s);
            let is_pirate = AvatarCharacter::for_seed(s).style == ThemeArchetype::Pirate;
            assert_eq!(kit.pirate, is_pirate, "seed {s}: the kit ignored her theme");
            let bare = sloop::build_rigged(&ctx, &hull, rig, UNKITTED);
            let drawn = sloop::build_rigged(&ctx, &hull, rig, kit);
            if is_pirate {
                pirates += 1;
                assert!(
                    count_nodes(&drawn) > count_nodes(&bare),
                    "seed {s} is a Pirate and drew nothing extra"
                );
            } else {
                others += 1;
                assert!(drawn == bare, "seed {s} is not a Pirate and her tree moved");
            }
        }
        assert!(
            pirates > 5 && others > 50,
            "only {pirates} pirates and {others} others were reached"
        );
    }

    /// Every node in a tree, so a kit can be shown to have drawn something.
    fn count_nodes(g: &Generator) -> usize {
        1 + g.children.iter().map(count_nodes).sum::<usize>()
    }

    /// The kit is deterministic and survives the sanitiser untouched at every
    /// blueprint extreme, and nothing it adds floats or stands over the cap
    /// (#1379). The binary defect tests that land with the slice.
    #[test]
    fn the_pirate_kit_is_one_machine_inside_the_cap_at_every_corner() {
        use super::super::common::touch;
        use crate::pds::sanitize_avatar_visuals;
        let mut n = 0;
        for (built, hull, rig, what) in every_kitted_sloop() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            // The kit is the SLOOP'S FIRST ROTATED NODE - her gunports lie on
            // the skin's own normal and the roger's bones are laid over - so
            // hers is now a round trip with the rotation epsilon the car
            // families have always needed (`first_difference`), and exact
            // everywhere else. Unkitted she still round-trips bit for bit,
            // which `a_boat_survives_sanitize_unchanged_at_her_blueprint_
            // extremes` still asserts.
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            // Nothing floats: the ensign has to touch the spar or the staff
            // it flies from, and every port its own hull.
            touch::assert_one_machine(&built, &what);
            // The DRAWN top, not the mount - a flag hangs below the head it
            // flies from, so the kit must not move the air draft at all.
            let drawn = touch::highest(&built) + hover(hull.draft);
            assert!(
                drawn <= AIR_DRAFT_CAP,
                "{what}: she draws to {drawn} m over the ground, past the \
                 {AIR_DRAFT_CAP} m cap"
            );
            let bare = touch::highest(&sloop::build_rigged(
                &PartCtx::for_seed(
                    (0u64..600)
                        .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                        .expect("some seed is a boat"),
                ),
                &hull,
                rig,
                UNKITTED,
            ));
            assert!(
                touch::highest(&built) <= bare + 1e-4,
                "{what}: the kit raised her highest point from {bare} to {}",
                touch::highest(&built)
            );
            n += 1;
        }
        assert_eq!(n, 5 * 4 * 5, "the kit sweep lost a combination");
    }

    /// The kit is deterministic: the same seed draws the same bytes twice.
    #[test]
    fn a_pirate_sloop_is_deterministic() {
        use crate::seeded_defaults::{BoatType, SloopRig};
        let mut n = 0;
        for s in (0u64..1200).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            if BoatType::for_seed(s) != BoatType::Sloop || !sloop::SloopKit::for_seed(s).pirate {
                continue;
            }
            let ctx = PartCtx::for_seed(s);
            let hull = sloop_hull_for(s);
            let kit = sloop::SloopKit::for_seed(s);
            let rig = SloopRig::for_seed(s);
            let once = sloop::build_rigged(&ctx, &hull, rig, kit);
            let twice = sloop::build_rigged(&ctx, &hull, rig, kit);
            assert!(once == twice, "seed {s} drew two different pirates");
            n += 1;
        }
        assert!(n > 5, "only {n} pirate sloops were reached");
    }

    /// Nothing a seeded boat carries stands over the air-draft cap (#1359
    /// rule 6) - the hard clash constraint of the whole redesign, since
    /// visuals carry no colliders and a spar over it sails straight through a
    /// gateway lintel.
    ///
    /// EVERY RIG is checked on every boat seed, not only the rig the seed
    /// drew, and each is checked twice: on its own derivation (the height the
    /// cap was resolved against) and on the tree it actually DRAWS. The
    /// second is the one that matters for a rig whose highest point is not
    /// its masthead - a gunter's yard, a square topsail's topmast - because a
    /// rig that resolved its mast against the cap and then crossed a spar
    /// over it would pass the first and sail through a lintel (#1366). Every
    /// blueprint corner a seed can reach is in the population sweep, including
    /// the small end, where a floor after a cap is how a cap gets lost.
    ///
    /// # And the MOUTH (#1382)
    ///
    /// The other half of rule 6, which the sloop alone was missing: the five
    /// later boat types each check their overall beam against
    /// [`GATEWAY_MOUTH`] in their own `fits_the_gateway` guard, and the hero
    /// predates that pattern. Hers is checked twice over, in the same sweep
    /// and for the same reason the air draft is: on the beam she DECLARES
    /// (what her collider is cut to) and on the width she DRAWS, which is
    /// what actually meets a jamb. They are not the same claim - a shroud, a
    /// chainplate or a Pirate's port lid stands outboard of the topsides -
    /// and `touch::widest` reads a tortured prim wide, so the drawn figure is
    /// a pessimistic bound.
    ///
    /// **THE MOUTH IS SLACK ON HER, AND THAT IS THE ANSWER, NOT AN OVERSIGHT.**
    /// The first version of this asserted the mouth *binds* somewhere in the
    /// population, the way the cap is asserted to bind on every rig below, and
    /// it failed: the widest sloop in the population draws **0.967 m** against
    /// a 2.600 m mouth, 1.6 m of slack. A monohull at a realistic L:B of
    /// 3.2-4.5 simply cannot approach a gate this wide at 2.8 m LOA - which is
    /// why the boat family's binding constraint is the air draft and the
    /// SKIFF family's is the mouth (a wagon reaches 2.020 m on her naves).
    /// So her width is guarded as a BAND rather than as a bound: it catches a
    /// change that doubles her, which is the failure that could actually
    /// happen here, instead of pretending a gate constrains her.
    #[test]
    fn no_seeded_boat_stands_over_the_air_draft_cap() {
        use super::super::common::touch;
        use crate::seeded_defaults::SloopRig;
        let mut worst = [0.0f32; SloopRig::ALL.len()];
        let mut widest: f32 = 0.0;
        let mut checked = 0;
        for s in (0u64..900).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            // The SLOOP's hull on every boat seed, whatever it draws: every
            // rig is checked everywhere a sloop could stand, which is every
            // blueprint the family rolls.
            let hull = sloop_hull_for(s);
            let ctx = PartCtx::for_seed(s);
            // The kit is in the sweep for the width, and out of it for the
            // height: her ports and lids are on the TOPSIDES, so they can
            // widen her and cannot raise her - which is exactly what
            // `the_pirate_kit_is_one_machine_inside_the_cap_at_every_corner`
            // pins on the height side.
            let kit = sloop::SloopKit::for_style(crate::seeded_defaults::ThemeArchetype::Pirate);
            for (i, rig) in SloopRig::ALL.into_iter().enumerate() {
                let derived = sloop::top_of_rig(&hull, rig) + hover(hull.draft);
                let bare = sloop::build_rigged(&ctx, &hull, rig, UNKITTED);
                let drawn = touch::highest(&bare) + hover(hull.draft);
                assert!(
                    derived.max(drawn) <= AIR_DRAFT_CAP,
                    "seed {s}, {}: the rig was resolved to {derived} m over the \
                     ground and draws to {drawn} m, past the {AIR_DRAFT_CAP} m cap",
                    rig.label()
                );
                let beam = sloop::Sloop.overall_beam(&hull, s);
                let wide = touch::widest(&bare)
                    .max(touch::widest(&sloop::build_rigged(&ctx, &hull, rig, kit)));
                assert!(
                    beam.max(wide) < GATEWAY_MOUTH,
                    "seed {s}, {}: she declares a {beam} m beam and draws \
                     {wide} m wide, past the {GATEWAY_MOUTH} m gateway mouth",
                    rig.label()
                );
                widest = widest.max(wide);
                worst[i] = worst[i].max(derived);
            }
            checked += 1;
        }
        assert!(checked > 100, "too few boats sampled: {checked}");
        // Her width against the number this was measured at - see the doc.
        assert!(
            (0.8..1.2).contains(&widest),
            "the widest sloop draws {widest} m, not the 0.967 m this was \
             measured at - her proportions moved, so re-read the mouth margin"
        );
        // And it is a real bound on EVERY rig rather than a vacuous one: the
        // cap actually binds each of them somewhere in the population.
        for (rig, worst) in SloopRig::ALL.into_iter().zip(worst) {
            assert!(
                worst > AIR_DRAFT_CAP - AIR_DRAFT_MARGIN - 0.01,
                "the tallest {} only reaches {worst} m - the cap binds on no \
                 boat carrying it, so this test proves nothing about it",
                rig.label()
            );
        }
    }

    /// The sloop's own hull on seed `s`'s blueprint and hull form - what the
    /// sloop guards measure on every boat seed, drawn as a sloop or not.
    fn sloop_hull_for(s: u64) -> HullProfile {
        use crate::seeded_defaults::SloopHull;
        let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(s)
            .and_then(|b| b.boat().copied())
            .expect("a boat seed has a boat blueprint");
        sloop::profile_of(&bp, SloopHull::for_seed(s))
    }

    /// Every runabout variant at every blueprint corner on every
    /// ornateness-by-wear pair, with its hull and a label (#1372).
    fn every_runabout() -> Vec<(Generator, HullProfile, RunaboutVariant, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            for v in RunaboutVariant::ALL {
                let hull = runabout::profile_of(&bp, v);
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            runabout::build_variant(&ctx, &hull, v),
                            hull,
                            v,
                            format!(
                                "a {} m {}, {} / {}",
                                hull.loa,
                                v.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    /// Every scow load at every blueprint corner on every ornateness-by-wear
    /// pair, with its hull and a label (#1373).
    fn every_scow() -> Vec<(Generator, HullProfile, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            let hull = scow::profile_of(&bp);
            for load in ScowLoad::ALL {
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            scow::build_load(&ctx, &hull, load),
                            hull,
                            format!(
                                "a {} m {}, {} / {}",
                                hull.loa,
                                load.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    /// Every tug variant at every blueprint corner on every ornateness-by-wear
    /// pair, with its hull and a label (#1370).
    fn every_tug() -> Vec<(Generator, HullProfile, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            let hull = tug::profile_of(&bp);
            for v in TugVariant::ALL {
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            tug::build_variant(&ctx, &hull, v),
                            hull,
                            format!(
                                "a {} m {}, {} / {}",
                                hull.loa,
                                v.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    /// Every junk at every blueprint corner on every ornateness-by-wear
    /// pair, with her hull and a label (#1371): one rig, no variant.
    fn every_junk() -> Vec<(Generator, HullProfile, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            let hull = junk::profile_of(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push((
                        junk::build_tiered(&ctx, &hull),
                        hull,
                        format!("a {} m junk, {} / {}", hull.loa, o.label(), w.label()),
                    ));
                }
            }
        }
        out
    }

    /// Every longship at every blueprint corner, on every combination of the
    /// two things her SEED decides - the variant her theme draws and whether
    /// she carries the serpent - crossed with every ornateness-by-wear pair
    /// (#1369). Returns the built tree, her hull, her kind and a label.
    fn every_longship() -> Vec<(Generator, HullProfile, longship::LongshipKind, String)> {
        use crate::seeded_defaults::{OrnatenessTier, WearTier};
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        let mut out = Vec::new();
        for bp in corners() {
            let hull = longship::profile_of(&bp);
            for kind in longship::LongshipKind::ALL {
                for o in OrnatenessTier::ALL {
                    for w in WearTier::ALL {
                        (ctx.ornateness, ctx.wear) = (o, w);
                        out.push((
                            longship::build_tiered(&ctx, &hull, kind),
                            hull,
                            kind,
                            format!(
                                "a {} m {}, {} / {}",
                                hull.loa,
                                kind.label(),
                                o.label(),
                                w.label()
                            ),
                        ));
                    }
                }
            }
        }
        out
    }

    /// The blueprint corners a seeded hull can actually reach - where a
    /// dimension floors or a clamp bites. The smallest hull draws the thinnest
    /// shroud and the finest stem; the largest draws the deepest keel.
    fn corners() -> Vec<BoatBlueprint> {
        [
            (1.60f32, 3.2f32, 0.10f32, 0.92f32),
            (1.60, 3.8, 0.12, 1.08),
            (2.80, 3.5, 0.11, 1.00),
            (4.40, 3.2, 0.12, 1.08),
            (4.40, 3.8, 0.10, 0.92),
        ]
        .iter()
        .map(|&(loa, ratio, fb, sheer)| BoatBlueprint {
            stance: crate::seeded_defaults::VehicleStance::Sleek,
            hull_len: loa,
            beam: loa / ratio,
            freeboard: loa * fb,
            sheer_bow: loa * 0.060 * sheer,
            sheer_stern: loa * 0.025 * sheer,
            draft: loa * 0.100 * sheer,
        })
        .collect()
    }

    /// Every boat survives the record sanitiser UNCHANGED at the extremes of
    /// her own blueprint, on every rig, hull form and tier (#1359 rule 8).
    ///
    /// Exact equality still holds, and that is because no node here carries
    /// a rotation except the root's 180 degree travel yaw, which is applied
    /// after this: the sanitiser renormalises quaternions, which moves the
    /// last ulp of almost any other rotation (#1364). A rig that authors one
    /// has to bring the skiffs' epsilon compare with it.
    #[test]
    fn a_boat_survives_sanitize_unchanged_at_her_blueprint_extremes() {
        use crate::pds::sanitize_avatar_visuals;
        let mut n = 0;
        for (built, what) in every_sloop() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert!(built == sanitized, "{what} was rewritten by the sanitiser");
            n += 1;
        }
        assert_eq!(n, 5 * 4 * 5 * 9, "the sweep lost a combination");
        // The runabout authors ROTATED nodes - her wheel, her seat backs, her
        // pods - so her round trip takes the skiffs' epsilon on rotations
        // (#1364) and is exact everywhere else.
        let mut n = 0;
        for (built, _, _, what) in every_runabout() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 5 * 3 * 9, "the runabout sweep lost a combination");
        // The scow authors rotated nodes too - her blade, her paddles and
        // rims, a battered stovepipe, the propped scrap - so hers takes the
        // same epsilon (#1373).
        let mut n = 0;
        for (built, _, what) in every_scow() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 5 * 4 * 9, "the scow sweep lost a combination");
        // The tug authors rotated nodes too - her raked funnel, the two
        // wedges of her forefoot and stem, her tyres, cowls and winch - so
        // hers takes the same epsilon (#1370).
        let mut n = 0;
        for (built, _, what) in every_tug() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 5 * 2 * 9, "the tug sweep lost a combination");
        // The junk authors rotated nodes too - her eyes, her quarter
        // windows, her rudder's slots and her roundel - so hers takes the
        // same epsilon (#1371). Her sails' hundredth node scales and their
        // bands' profile cuts pass as built.
        let mut n = 0;
        for (built, _, what) in every_junk() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 5 * 9, "the junk sweep lost a combination");
        // The longship authors rotated nodes too - every shield in her row,
        // and the serpent's horns and eyes - so hers takes the same epsilon
        // (#1369). Her sail's hundredth node scales and her stripes'
        // profile cuts pass as built, as the junk's do.
        let mut n = 0;
        for (built, _, _, what) in every_longship() {
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            if let Some(where_) = first_difference(&built, &sanitized, "0") {
                panic!("{what} was rewritten by the sanitiser at {where_}");
            }
            n += 1;
        }
        assert_eq!(n, 5 * 4 * 9, "the longship sweep lost a combination");
    }

    /// Every part of a built boat meets another, and the whole craft is one
    /// connected component - the roadster's guard (#1364), and the sloop's on
    /// every rig, hull form and tier she can roll.
    ///
    /// Swept over the blueprint extremes rather than one seed, because the
    /// failure is size-dependent - anything floored at [`MIN_DIM`] stops
    /// shrinking with the hull, so a part that meets at the nominal size can
    /// come adrift at the small end. See [`super::common::touch`] for why this
    /// cannot be judged by eye, and for its one blind spot: it resolves a
    /// tortured cuboid as its UNDEFORMED box, so for the SLOOP's sails it is
    /// green partly for the wrong reason - measured at #1382, hers are the
    /// only tortured sails in the fleet. Teaching it the deform is #1393's,
    /// and the blind spot is why the sail patch the phase-1 prototype drew is
    /// not in the ladder (#1366).
    #[test]
    fn a_boat_is_one_machine_at_her_blueprint_extremes() {
        use super::super::common::touch;
        for (built, what) in every_sloop() {
            touch::assert_one_machine(&built, &what);
        }
        for (built, _, _, what) in every_runabout() {
            touch::assert_one_machine(&built, &what);
        }
        for (built, _, what) in every_scow() {
            touch::assert_one_machine(&built, &what);
        }
        for (built, _, what) in every_tug() {
            touch::assert_one_machine(&built, &what);
        }
        // The junk on the tree AS SAVED, through the record's 0.1 mm wire
        // (#1371): the skiffs' form. The helper honours node scale, so it
        // reads each flattened sail as the lens it is - batten-to-cloth
        // contact holds by construction and by this.
        for (built, _, what) in every_junk() {
            let json = serde_json::to_string(&built).expect("a junk serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
        // The longship on the tree AS SAVED too, and hers is the sweep that
        // needs it most: her strakes, her shield row and her sail's bands
        // are all thin parts held against the shell by construction, and the
        // record's 0.1 mm wire is what they have to survive.
        for (built, _, _, what) in every_longship() {
            let json = serde_json::to_string(&built).expect("a longship serializes");
            let saved: Generator = serde_json::from_str(&json).expect("and reads back");
            touch::assert_one_machine(&saved, &what);
        }
    }

    /// A runabout floats IN her water and fits the gateway, at every corner
    /// on every variant and tier (#1372): her keel is under her own design
    /// waterline by at least a hundredth of her length - the first sweep of
    /// the prototype found the small narrow catamaran floating dry above it -
    /// she is drawn under the air-draft cap hover included, and her overall
    /// beam clears the narrowest gateway mouth.
    #[test]
    fn a_runabout_floats_in_her_water_and_fits_the_gateway() {
        use super::super::common::touch;
        for (built, hull, v, what) in every_runabout() {
            let keel = hull
                .stations()
                .iter()
                .map(|s| s.keel)
                .fold(f32::INFINITY, f32::min);
            assert!(
                keel <= -0.01 * hull.loa,
                "{what}: her keel is {keel} m, not under her waterline"
            );
            let air = touch::highest(&built) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "{what}: drawn to {air} m over the ground"
            );
            let beam = runabout::overall_beam_of(&hull, v);
            assert!(
                beam < GATEWAY_MOUTH,
                "{what}: {beam} m wide, past the {GATEWAY_MOUTH} m gateway mouth"
            );
        }
    }

    /// A scow floats IN her water and fits the gateway, at every corner on
    /// every load and tier (#1373) - the runabout's guard: her flat bottom is
    /// under her own design waterline by at least a hundredth of her length
    /// even though her swept-up ends are clear of it, she is drawn under the
    /// air-draft cap hover included, and her overall beam clears the narrowest
    /// gateway mouth.
    #[test]
    fn a_scow_floats_in_her_water_and_fits_the_gateway() {
        use super::super::common::touch;
        for (built, hull, what) in every_scow() {
            let keel = hull
                .stations()
                .iter()
                .map(|s| s.keel)
                .fold(f32::INFINITY, f32::min);
            assert!(
                keel <= -0.01 * hull.loa,
                "{what}: her bottom is at {keel} m, not under her waterline"
            );
            let air = touch::highest(&built) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "{what}: drawn to {air} m over the ground"
            );
            let beam = scow::Scow.overall_beam(&hull, 0);
            assert!(
                beam < GATEWAY_MOUTH,
                "{what}: {beam} m wide, past the {GATEWAY_MOUTH} m gateway mouth"
            );
        }
    }

    /// A tug floats IN her water and fits the gateway, at every corner on
    /// both variants and every tier (#1370) - the runabout's guard: her
    /// canoe body is under her own design waterline by at least a hundredth
    /// of her length, she is drawn under the air-draft cap hover included -
    /// signal mast, funnel and derrick alike - and her overall beam, tyres
    /// and all, clears the narrowest gateway mouth.
    #[test]
    fn a_tug_floats_in_her_water_and_fits_the_gateway() {
        use super::super::common::touch;
        for (built, hull, what) in every_tug() {
            let keel = hull
                .stations()
                .iter()
                .map(|s| s.keel)
                .fold(f32::INFINITY, f32::min);
            assert!(
                keel <= -0.01 * hull.loa,
                "{what}: her keel is {keel} m, not under her waterline"
            );
            let air = touch::highest(&built) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "{what}: drawn to {air} m over the ground"
            );
            let beam = tug::Tug.overall_beam(&hull, 0);
            assert!(
                beam < GATEWAY_MOUTH,
                "{what}: {beam} m wide, past the {GATEWAY_MOUTH} m gateway mouth"
            );
        }
    }

    /// A junk floats IN her water and fits the gateway, at every corner on
    /// every tier (#1371) - the runabout's guard: her flat bottom is under
    /// her own design waterline by at least a hundredth of her length, she
    /// is drawn under the air-draft cap hover included - the mizzen's yard
    /// and the lantern alike - and her overall beam clears the narrowest mouth.
    ///
    /// And her lowest point is her rudder's foot, at exactly her derived
    /// draft - the allowance IS the rudder - so her hover, a quarter of a
    /// draft, clears it. Read off the drawn tree less the hull's own res-3
    /// sweeps, which the connectivity helper reads too deep as round tubes
    /// (#1393); for those the profile answers, and her flat bottom lies an
    /// allowance over the foot.
    #[test]
    fn a_junk_floats_in_her_water_and_fits_the_gateway() {
        use super::super::common::touch;
        use crate::pds::generator::GeneratorKind;
        for (built, hull, what) in every_junk() {
            let keel = hull
                .stations()
                .iter()
                .map(|s| s.keel)
                .fold(f32::INFINITY, f32::min);
            assert!(
                keel <= -0.01 * hull.loa,
                "{what}: her bottom is at {keel} m, not under her waterline"
            );
            let air = touch::highest(&built) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "{what}: drawn to {air} m over the ground"
            );
            let beam = junk::Junk.overall_beam(&hull, 0);
            assert!(
                beam < GATEWAY_MOUTH,
                "{what}: {beam} m wide, past the {GATEWAY_MOUTH} m gateway mouth"
            );
            let foot = junk::rudder_foot(&hull);
            assert_eq!(foot, -hull.draft, "{what}: her rudder is not her draft");
            let mut rest = built.clone();
            rest.children
                .retain(|g| !matches!(g.kind, GeneratorKind::Spine { resolution: 3, .. }));
            let low = touch::lowest(&rest);
            assert!(
                (low - foot).abs() < 1e-4,
                "{what}: her lowest point is {low} m, not her rudder's foot at {foot} m"
            );
            assert!(
                keel > foot && foot > -hover(hull.draft),
                "{what}: her bottom {keel} m, her rudder's foot {foot} m, the ground {} m",
                -hover(hull.draft)
            );
        }
    }

    /// A longship floats IN her water and fits the gateway, at every corner
    /// on every kind and tier (#1369) - the runabout's guard: her canoe body
    /// is under her own design waterline by at least a hundredth of her
    /// length, she is drawn under the air-draft cap hover included, and her
    /// overall beam - shields, and a galley's oars - clears the narrowest
    /// gateway mouth.
    ///
    /// And she is a DOUBLE-ENDER, which is the whole reason
    /// [`SheerLaw::Crescent`](super::profile::SheerLaw::Crescent) exists:
    /// her two ends stand at EXACTLY the same height, on every corner. That
    /// is asserted bit for bit rather than within a tolerance, because the
    /// law computes one rise and lays it twice - if it ever came to differ
    /// by a rounding, the law would not be the law any more.
    ///
    /// And her lowest point is the STEERING OAR'S foot, at exactly her
    /// derived draft - the allowance IS the oar - so her hover, a quarter of
    /// a draft, clears it. Read off the drawn tree less the hull's own
    /// sweeps, which the connectivity helper reads too deep as round tubes
    /// (#1393); for those the profile answers, and her canoe body lies an
    /// allowance over the foot.
    #[test]
    fn a_longship_floats_in_her_water_and_fits_the_gateway() {
        use super::super::common::touch;
        use crate::pds::generator::GeneratorKind;
        let mut capped = 0;
        for (built, hull, kind, what) in every_longship() {
            capped += usize::from(longship::mast_is_capped(&hull));
            let keel = hull
                .stations()
                .iter()
                .map(|s| s.keel)
                .fold(f32::INFINITY, f32::min);
            assert!(
                keel <= -0.01 * hull.loa,
                "{what}: her keel is {keel} m, not under her waterline"
            );
            let air = touch::highest(&built) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "{what}: drawn to {air} m over the ground"
            );
            // THE CAP COUNTS THE DRAWN TOP, NOT THE MASTHEAD (#1378's
            // finding 7, here again in a rig). Her mast is what the cap is
            // resolved against, and her vane - and at Ornate her banner -
            // stand OVER it, so the rig leaves them room by construction.
            // Both halves are asserted: that something really does stand
            // over the masthead, so the allowance is not guarding thin air,
            // and that the masthead itself is under the cap.
            let mast = longship::top_of_rig(&hull) + hover(hull.draft);
            assert!(
                air > mast,
                "{what}: her drawn top is {air} m and her masthead {mast} m - \
                 nothing stands over the mast, so the fitting allowance the \
                 cap is resolved with guards nothing"
            );
            assert!(
                mast <= AIR_DRAFT_CAP,
                "{what}: her masthead is at {mast} m over the ground"
            );
            let beam = longship::overall_beam_of(&hull, kind.variant);
            assert!(
                beam < GATEWAY_MOUTH,
                "{what}: {beam} m wide, past the {GATEWAY_MOUTH} m gateway mouth"
            );
            // The double-ender's own guard.
            assert_eq!(
                hull.sheer_at(-0.5),
                hull.sheer_at(0.5),
                "{what}: her stern post and her stem do not stand level - \
                 the Crescent law is what makes her a double-ender"
            );
            // Her steering oar IS her draft's allowance, so her blade's
            // foot is her draft by construction, her canoe body hangs an
            // allowance over it, and her hover - a quarter of a draft -
            // clears it: the junk's rudder rule, and the twin's corner
            // check.
            let foot = longship::blade_foot(&hull);
            assert_eq!(
                foot, -hull.draft,
                "{what}: her steering oar is not her draft"
            );
            assert!(
                keel > foot && foot > -hover(hull.draft),
                "{what}: her bottom {keel} m, her oar's foot {foot} m, the ground {} m",
                -hover(hull.draft)
            );
            // And NOTHING she draws hangs under that foot. Read off the
            // drawn tree less the hull's own sweeps, which the connectivity
            // helper reads too deep as round tubes (#1393); for those the
            // profile answers above.
            //
            // The blade does not reach the foot exactly, and that is a fact
            // about a Spine rather than a slack bound: its radius is
            // perpendicular to its PATH, and her blade's path rakes down and
            // outboard, so the deepest station's section is tilted and its
            // lowest drawn point sits a little inside the foot. The bound
            // below is what keeps the oar the deepest thing she carries.
            let mut rest = built.clone();
            rest.children.retain(|g| {
                !matches!(
                    g.kind,
                    GeneratorKind::Spine {
                        resolution: longship::HULL_RES,
                        ..
                    }
                )
            });
            let low = touch::lowest(&rest);
            assert!(
                (foot - 1e-4..=foot + 0.02 * hull.loa).contains(&low),
                "{what}: her lowest drawn point is {low} m, against a steering \
                 oar's foot at {foot} m"
            );
        }
        // The cap is a REAL bound on her rig, not a vacuous one: it clamps
        // the mast on her long hulls. Her live seeds never reach it - the
        // tallest longship under 3000 stands 2.04 m of the 2.8 m cap - so
        // this sweep, which runs out to the 4.40 m corner, is the only place
        // the clamp is exercised at all.
        assert!(
            capped > 0,
            "the cap clamped no longship's mast in the corner sweep - the \
             clamp in Rig::new is never exercised"
        );
    }

    /// Every longship seed is drawn as a longship, on the variant her theme
    /// picks, under sail (#1369) - the seam's other half, and the last type
    /// in the family to be able to say it.
    ///
    /// Unlike the tug, the scow, the junk and the runabout this canNOT say
    /// "and no other boat seed is": a SLOOP sails too, and so the drive is
    /// one-way here. What replaces the other direction is the variant census
    /// and the air-draft sweep below.
    ///
    /// EVERY live longship seed is walked through the cap as DRAWN, at her
    /// own tiers, because [`no_seeded_boat_stands_over_the_air_draft_cap`]
    /// builds the SLOOP's rigs on every seed's sloop hull and never the
    /// drawn craft - so this and
    /// [`a_longship_floats_in_her_water_and_fits_the_gateway`] are the only
    /// guards on her rig.
    #[test]
    fn a_longship_seed_draws_a_longship() {
        use super::super::common::touch;
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::LongshipVariant;
        let (mut ships, mut worst) = (0, 0.0f32);
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            if BoatType::for_seed(s) != BoatType::Longship {
                continue;
            }
            assert_eq!(propulsion(s), Propulsion::Sail, "seed {s}");
            let v = LongshipVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (craft, hull) = hull_for(s).expect("a longship seed has a hull");
            let aura = crate::seeded_defaults::AvatarFx::for_seed(s).aura;
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a boat is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("a longship trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                craft.fx_mount(aura, &hull, s),
                "seed {s}: her aura does not leave her own mount"
            );
            let air =
                touch::highest(&craft.build(&PartCtx::for_seed(s), &hull)) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "seed {s}: drawn to {air} m over the ground"
            );
            worst = worst.max(air);
            ships += 1;
        }
        assert!(ships > 50, "only {ships} longship seeds under 3000");
        assert_eq!(
            seen.len(),
            LongshipVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
        // Unlike the junk's, this is NOT where the cap is shown to bind. A
        // longship carries one mast at 0.56 L and nothing over it but a
        // vane, so the tallest of her live seeds stands about 2.04 m of the
        // 2.8 m cap and the clamp never fires in the population. Where it
        // does fire is the 4.40 m blueprint corner, and
        // `a_longship_floats_in_her_water_and_fits_the_gateway` is what
        // asserts it there. What this bound says is only that the sweep
        // measured something.
        assert!(
            worst > 1.0,
            "the tallest longship under 3000 stands {worst} m - nothing was measured"
        );
    }

    /// Every steam tug seed is drawn as a tug, on the variant her theme
    /// picks, under steam (#1370) - and no other boat seed is: a boat is
    /// under steam exactly when she is drawn as a tug. And the aura her
    /// record carries - steam, or a wood-fired stack's embers - leaves her
    /// funnel's mouth.
    #[test]
    fn a_tug_seed_draws_a_tug_under_steam() {
        use crate::pds::generator::GeneratorKind;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let tug = BoatType::for_seed(s) == BoatType::SteamTug;
            assert_eq!(
                propulsion(s) == Propulsion::Steam,
                tug,
                "seed {s}: {:?} drives {:?}",
                BoatType::for_seed(s),
                propulsion(s)
            );
            if !tug {
                continue;
            }
            let v = TugVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
            let (_, hull) = hull_for(s).expect("a tug seed has a hull");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a boat is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("a tug trails an aura");
            assert_eq!(
                emitter.transform.translation.0,
                tug::funnel_mouth(&hull),
                "seed {s}: her aura does not leave her funnel's mouth"
            );
        }
        assert_eq!(
            seen.len(),
            TugVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// Every junk seed is drawn as a junk under battened sail (#1371) - and
    /// no other boat seed is: a boat is under battened sail exactly when she
    /// is drawn as a junk. The aura her record carries - every junk seed's
    /// is the family's wake floor, since none of her themes lights one -
    /// leaves her own wake mount.
    ///
    /// And EVERY live junk seed stands under the air-draft cap as drawn, at
    /// her own tiers: [`no_seeded_boat_stands_over_the_air_draft_cap`] builds
    /// the sloop's rigs on every seed's sloop hull and never the drawn craft,
    /// so this and [`a_junk_floats_in_her_water_and_fits_the_gateway`] are
    /// the only guards on her rig - and her cap binds on the long seeds.
    #[test]
    fn a_junk_seed_draws_a_junk_under_battened_sail() {
        use super::super::common::touch;
        use crate::pds::generator::GeneratorKind;
        let (mut junks, mut worst) = (0, 0.0f32);
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let junk = BoatType::for_seed(s) == BoatType::Junk;
            assert_eq!(
                propulsion(s) == Propulsion::Battened,
                junk,
                "seed {s}: {:?} drives {:?}",
                BoatType::for_seed(s),
                propulsion(s)
            );
            if !junk {
                continue;
            }
            let (craft, hull) = hull_for(s).expect("a junk seed has a hull");
            let aura = crate::seeded_defaults::AvatarFx::for_seed(s).aura;
            assert_eq!(aura, ParticleAura::Wake, "seed {s}: a junk picked {aura:?}");
            let (record, _) = super::super::build_for_seed(s);
            let emitter = record
                .visuals()
                .expect("a boat is an assembled tree")
                .children
                .iter()
                .find(|g| matches!(g.kind, GeneratorKind::ParticleSystem(..)))
                .expect("a junk trails her wake");
            assert_eq!(
                emitter.transform.translation.0,
                craft.fx_mount(ParticleAura::Wake, &hull, s),
                "seed {s}: her wake does not leave her own mount"
            );
            let air =
                touch::highest(&craft.build(&PartCtx::for_seed(s), &hull)) + hover(hull.draft);
            assert!(
                air <= AIR_DRAFT_CAP,
                "seed {s}: drawn to {air} m over the ground"
            );
            worst = worst.max(air);
            junks += 1;
        }
        assert!(junks > 50, "only {junks} junk seeds under 3000");
        // A real bound, not a vacuous one: the cap binds on her longest seeds.
        assert!(
            worst > AIR_DRAFT_CAP - AIR_DRAFT_MARGIN,
            "the tallest junk under 3000 stands only {worst} m - the cap binds on none"
        );
    }

    /// Every scow seed is drawn as a scow, carrying the load its theme picks,
    /// and is poled (#1373) - and no other boat seed is: a boat is poled
    /// exactly when she is drawn as a scow.
    #[test]
    fn a_scow_seed_draws_a_scow_poled() {
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let scow = BoatType::for_seed(s) == BoatType::Scow;
            assert_eq!(
                propulsion(s) == Propulsion::Poled,
                scow,
                "seed {s}: {:?} drives {:?}",
                BoatType::for_seed(s),
                propulsion(s)
            );
            if scow {
                let load = ScowLoad::for_seed(s);
                if !seen.contains(&load) {
                    seen.push(load);
                }
            }
        }
        assert_eq!(
            seen.len(),
            ScowLoad::ALL.len(),
            "the seeds under 3000 miss a load: {seen:?}"
        );
    }

    /// Every runabout seed is drawn as a runabout, on the variant its theme
    /// picks, and drives under power (#1372, #1383) - the seam's other half:
    /// a built type is drawn, and draws with its own voice.
    #[test]
    fn a_runabout_seed_draws_a_runabout_under_power() {
        use crate::seeded_defaults::BoatType;
        let mut seen = Vec::new();
        for s in (0u64..3000).filter(|&s| BoatType::for_seed(s) == BoatType::Runabout) {
            assert_eq!(propulsion(s), Propulsion::Engine, "seed {s}");
            let v = RunaboutVariant::for_seed(s);
            if !seen.contains(&v) {
                seen.push(v);
            }
        }
        assert_eq!(
            seen.len(),
            RunaboutVariant::ALL.len(),
            "the seeds under 3000 miss a variant: {seen:?}"
        );
    }

    /// A seeded boat's saved record stays well under the soft budget
    /// (#1359 rule 9), at a THIRD of it - the owner raised the sloop's guard
    /// from a quarter to the roadster's fraction for #1366's ladder.
    ///
    /// Two sweeps. The live seeds, as saved - FX emitter and voice included -
    /// and the heaviest thing the family can draw: every rig and hull form at
    /// every blueprint corner on the fullest ladder, Ornate and Battered,
    /// carrying the heaviest record overhead any live seed carries. Measured
    /// rather than assumed, so a new aura that grows the emitter moves this
    /// too.
    ///
    /// # It sizes what a SAVE writes (#1382 gap 1)
    ///
    /// It used to size the [`super::super::RecordBody`] alone - the visual
    /// tree plus the FX hung on it - because `build_for_seed` returns the body
    /// and a [`LocomotionConfig`](crate::pds::LocomotionConfig) and this bound
    /// the second to `_`. A publish writes the whole
    /// [`AvatarRecord`](crate::pds::avatar::AvatarRecord): the `$type`, the
    /// body, the locomotion preset and the seeded gait section, about 770 B
    /// more. So the record itself is what is measured now, and the "overhead"
    /// added to each corner is everything the record carries besides the tree.
    /// The margin was never in danger - the heaviest craft has over 2 KB of it
    /// - and this is honesty rather than a rescue.
    #[test]
    fn a_seeded_boats_record_stays_well_inside_the_budget() {
        use crate::pds::avatar::AvatarRecord;
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        use crate::seeded_defaults::{OrnatenessTier, SloopHull, SloopRig, WearTier};
        let bytes = |t: &Generator| serialized_record_bytes(t).expect("a boat serializes");
        let (mut worst_seed, mut overhead) = (0usize, 0usize);
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let saved = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
                .expect("a record serializes");
            worst_seed = worst_seed.max(saved);
            overhead = overhead.max(saved.saturating_sub(bytes(&super::build(s, None))));
        }
        assert!(worst_seed > 0 && overhead > 0, "nothing was measured");
        let mut ctx = PartCtx::for_seed(
            (0u64..600)
                .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
                .expect("some seed is a boat"),
        );
        ctx.ornateness = OrnatenessTier::Ornate;
        ctx.wear = WearTier::Battered;
        let mut worst_corner = 0usize;
        for bp in corners() {
            for form in SloopHull::ALL {
                let hull = sloop::profile_of(&bp, form);
                for rig in SloopRig::ALL {
                    let mut built = sloop::build_rigged(&ctx, &hull, rig, UNKITTED);
                    apply_travel_pose(&mut built, TRAVEL_DROP);
                    worst_corner = worst_corner.max(bytes(&built) + overhead);
                }
            }
        }
        for (built, ..) in every_runabout() {
            let mut built = built;
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        for (built, ..) in every_scow() {
            let mut built = built;
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        for (built, ..) in every_tug() {
            let mut built = built;
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        // The junk at her fullest - the mizzen, the lantern, the shelter and
        // both of a battered mainsail's bands - on every corner (#1371).
        for bp in corners() {
            let mut built = junk::build_tiered(&ctx, &junk::profile_of(&bp));
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        // And the longship at HERS, which is the heaviest tree in the family:
        // a galley's bank of eighteen oars and her beak, over twelve shields,
        // twelve strakes, a striped and patched sail, the tent and the
        // serpent. `every_longship` crosses both variants with the serpent
        // either way, so the worst of it is in here.
        // And a PIRATE sloop at her fullest, which is the only thing the
        // sloop can draw that `every_sloop` above does not (#1379).
        for (built, ..) in every_kitted_sloop() {
            let mut built = built;
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        for (built, ..) in every_longship() {
            let mut built = built;
            apply_travel_pose(&mut built, TRAVEL_DROP);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
        for (what, worst) in [
            ("seeded boat", worst_seed),
            ("fully dressed corner", worst_corner),
        ] {
            assert!(
                worst * 3 < SOFT_RECORD_BUDGET_BYTES,
                "the heaviest {what} is {worst} bytes, past a third of the \
                 {SOFT_RECORD_BUDGET_BYTES}-byte soft budget - a craft type is \
                 spending nodes where it should be spending shape"
            );
        }
    }

    /// The hover and the ride height are one derivation, and the waterline is
    /// where the two meet.
    #[test]
    fn the_waterline_floats_a_quarter_of_a_draft_clear_of_the_ground() {
        for draft in [0.12f32, 0.28, 0.45] {
            let ride = land_ride_height(draft);
            assert!((ride - TRAVEL_DROP - hover(draft)).abs() < 1e-6);
            // The keel clears the ground by a quarter of the draft, which is
            // what makes her read as hovering.
            assert!((hover(draft) - draft - 0.25 * draft).abs() < 1e-6);
        }
    }
}
