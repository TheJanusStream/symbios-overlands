//! Hover-boat family assembler - composes the vessel from the seeded
//! [`AvatarOutfit`](crate::seeded_defaults::AvatarOutfit) parts.
//!
//! The hull part is the structural root (a swept blob hull with a pointed prow
//! and sheer-following rub-strakes, centred at the waterline origin); the low
//! cabin deck sits just inside it, the rigged mast (a fore-and-aft mainsail, or
//! a styled square/antenna/derrick variant) rises from the deck, and the
//! optional bow ornament / stern stack mount fore and aft. All geometry,
//! colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); the assembler owns only the layout anchors.
//! Seeded FX are attached centrally by [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartSlot, by_slug};
use crate::pds::generator::Generator;

use super::assemble::{
    apply_travel_pose, assemble_root, debug_assert_slots_handled, ornament_count,
};
use super::common::offset;

pub(super) fn build(seed: u64) -> Generator {
    // The hull is the structural root (at the waterline origin).
    let (outfit, ctx, mut root) = assemble_root(seed, PartSlot::Hull);

    // Mount landmarks come from the shared boat blueprint, so the deck / mast /
    // bow / stack anchors track the seeded hull instead of re-encoding its
    // default length + freeboard as constants (the coupling that floated the
    // stack and bow off mis-sized hulls). The fore/aft stations are hull-length
    // fractions on the blueprint; the small mount heights ride the deck line.
    let bp = ctx.boat();
    let deck_y = bp.map_or(0.13, |b| b.deck_y);
    let bow_z = bp.map_or(0.78, |b| b.bow_z);
    let stack_z = bp.map_or(-0.56, |b| b.stack_z);
    let ornament_z = bp.map_or(0.1, |b| b.ornament_z);

    for choice in &outfit.parts {
        if choice.slot == PartSlot::Hull {
            continue;
        }
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Deck => root
                .children
                .push(offset(part.build(&ctx), [0.0, deck_y, 0.0])),
            PartSlot::Mast => root
                .children
                .push(offset(part.build(&ctx), [0.0, deck_y, -0.05])),
            PartSlot::Bow => root.children.push(offset(
                part.build(&ctx),
                [0.0, deck_y * 0.77, bow_z * BOW_HULL_EMBED],
            )),
            PartSlot::Stack => root
                .children
                .push(offset(part.build(&ctx), stack_station(deck_y, stack_z))),
            PartSlot::Ornament => {
                // An ornate boat lines the deck with trinkets: amidships, then
                // a pair fore + aft on either side of it (#798).
                let stations = [
                    [0.0, deck_y * 1.38, ornament_z],
                    [0.0, deck_y * 1.28, stack_z * 0.5],
                    [0.0, deck_y * 1.28, bow_z * 0.5],
                ];
                for &station in stations.iter().take(ornament_count(&ctx)) {
                    root.children.push(offset(part.build(&ctx), station));
                }
            }
            _ => {}
        }
    }

    // Size the whole craft to airship class and set it on its hover line.
    apply_travel_pose(&mut root, TRAVEL_DROP, VISUAL_SCALE);
    debug_assert_slots_handled(
        &outfit,
        PartSlot::Hull,
        &[
            PartSlot::Deck,
            PartSlot::Mast,
            PartSlot::Bow,
            PartSlot::Stack,
            PartSlot::Ornament,
        ],
    );
    root
}

// ---------------------------------------------------------------------------
// Airship-class scale bridge (#1361)
// ---------------------------------------------------------------------------
//
// The legacy boat parts below are authored around a 1.32 m hull - a third of
// the airship's 3.15 m, and a toy beside a 1.7 m person. At the chase camera's
// 12 m orbit that hull is about 144 px long, so every fitting the last two
// passes added is sub-pixel in play and the sanitiser's 0.01 m floor forces the
// small ones fat. Owner decision 1 of the redesign (#1359) is airship-class
// scale, and this slice buys it before any art is spent on it, by scaling the
// assembled tree ONCE at its root: uniform, and applied last, so every mount
// the parts and the [`BoatBlueprint`](crate::seeded_defaults) agree on is
// untouched (see [`apply_travel_pose`]).
//
// The scale itself is THROWAWAY - the legacy boat pipeline dies when the sloop
// lands in #1363, authored at true metres with no bridge to apply. What is not
// throwaway is [`NOMINAL_HULL_LEN`]: the locomotion in [`super`] is re-based on
// it, so a boat's mass, collider and ride height all come from the size it is
// actually drawn at rather than from the size its parts happen to be authored
// at.

/// Overall hull length (m) the legacy boat parts are authored around: the
/// `BoatBlueprint` nominal every part fraction and mount station is taken from,
/// before the seeded stance / body multipliers spread it.
pub(super) const AUTHORED_HULL_LEN: f32 = 1.32;

/// Authored nominal beam (m) - the blueprint's, likewise pre-multipliers.
pub(super) const AUTHORED_BEAM: f32 = 0.5;

/// Authored nominal freeboard (m) - the blueprint's, likewise pre-multipliers.
pub(super) const AUTHORED_FREEBOARD: f32 = 0.26;

/// Overall hull length (m) a nominal seeded boat is **drawn** at: airship
/// class, per owner decision 1 of #1359 (boats about 2.8 m against the
/// airship's 3.15 m and a 1.7 m person). Still a scale model of a bigger craft
/// - lit ports, no pilot - like the airship's 0.9 m gondola.
pub(super) const NOMINAL_HULL_LEN: f32 = 2.8;

/// The single uniform scale the assembler puts on the visual root, so that an
/// authored-nominal hull is drawn at [`NOMINAL_HULL_LEN`].
pub(super) const VISUAL_SCALE: f32 = NOMINAL_HULL_LEN / AUTHORED_HULL_LEN;

/// Depth (m) of the hull below its design waterline, per metre of freeboard.
///
/// Read off the hull the parts actually build: `boat_hull_body` centres the
/// amidships mass at `-0.26 · freeboard` with a `0.74 · freeboard` semi-axis, so
/// the analytic keel sits exactly one freeboard under the waterline origin (the
/// blob iso-surface pulls in a little from there, in the craft's favour).
const DRAFT_PER_FREEBOARD: f32 = 1.0;

/// Keel clearance over the suspension ground line, as a fraction of the hull's
/// draft - what makes a hover-boat read as *hovering* rather than beached.
///
/// A fraction rather than a constant so the hover reads the same at any size;
/// set to the clearance the pre-scale build happened to have (0.067 m under a
/// 0.26 m draft), so the airship-class boat hovers exactly as the old one did,
/// scaled.
const KEEL_CLEARANCE_FRAC: f32 = 0.25;

/// Travel-pose drop (m): how far under the chassis origin the assembler hangs
/// the hull's design waterline (the visual origin, which is where
/// `boat_hull_body` seams its topsides to its belly).
///
/// **Derived, not tuned.** Buoyancy rests a floating chassis origin exactly
/// `water_rest_length` above the surface (`player::hover_boat`), so dropping
/// the visual by that same distance is the one value that lands the design
/// waterline *on* the water - the boat floats on its own painted waterline
/// instead of wading 0.1 m deep, which is what the old hand-set 0.6 did.
pub(super) const TRAVEL_DROP: f32 = crate::config::rover::WATER_REST_LENGTH;

/// Height (m) above flat ground this hull wants its **chassis origin** at, for
/// a hull of true (drawn) freeboard `freeboard`.
///
/// The other half of [`TRAVEL_DROP`]: on land there is no buoyancy plane to
/// meet, so the hull is placed by its keel instead. Under the chassis origin go
/// the drop, then the hull's draft, then the clearance that keeps the keel off
/// the ground. [`super::boat_locomotion`] turns this into the suspension rest
/// length that actually holds the craft there - the two are a pair, and a boat
/// twice the size needs both or it reads as beached.
pub(super) fn land_ride_height(freeboard: f32) -> f32 {
    TRAVEL_DROP + freeboard * (DRAFT_PER_FREEBOARD + KEEL_CLEARANCE_FRAC)
}

/// Rise (m) from the Stack mount up to the funnel mouth the FX steam issues
/// from - the smokestack part's mouth sits ≈ this far above its base. In the
/// parts' **authoring frame**, like every station below: it rides the root's
/// [`VISUAL_SCALE`] along with the funnel it leaves.
pub(super) const FUNNEL_MOUTH_RISE: f32 = 0.5;

/// Inboard-embed fractions for the Bow / Stack part bases (#806).
///
/// `bow_z` / `stack_z` (from [`BoatBlueprint`](crate::seeded_defaults)) are the
/// *analytic* stem / stern stations, but the hull is a swept-blob iso-surface
/// that pulls inboard of those analytic tips by a seed/torture-dependent margin -
/// most at the fine prow, where a part seated on the tip floats ahead of the
/// mesh (the reported detached bowsprit). Seating each base at this fraction of
/// its analytic station pulls it *into* the hull, so it always embeds rather
/// than undershooting into open air. Embedding is invisible - the hull is
/// opaque and a bowsprit / funnel still projects clear via its own forward /
/// upward offset - and per the overshoot-beats-undershoot rule an embedded base
/// reads better than a floating one across every seed. The prow needs the
/// stronger pull (its cone tapers to a fine point the iso-surface eats most).
const BOW_HULL_EMBED: f32 = 0.80;
const STACK_HULL_EMBED: f32 = 0.86;

/// Stack (funnel) mount station (root-local, before the assembler's yaw, drop
/// and scale) from the deck line + the blueprint's aft stack station, pulled
/// inboard by
/// [`STACK_HULL_EMBED`] so the funnel base embeds in the hull. The single
/// source the assembler seats the Stack part on and the FX steam anchor rises
/// from by [`FUNNEL_MOUTH_RISE`], so the steam leaves the same funnel the part
/// builds and stays anchored to the same seated base (#798, #806).
pub(super) fn stack_station(deck_y: f32, stack_z: f32) -> [f32; 3] {
    [0.0, deck_y * 0.62, stack_z * STACK_HULL_EMBED]
}
