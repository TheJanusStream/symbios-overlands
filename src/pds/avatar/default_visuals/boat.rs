//! Hover-boat family assembler — composes the vessel from the seeded
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

    // Drop to a low hover above the hover-boat's suspension ground line (the
    // chassis floats ≈0.97 m; a small gap keeps it reading as a hover-craft).
    apply_travel_pose(&mut root, 0.6);
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

/// Rise (m) from the Stack mount up to the funnel mouth the FX steam issues
/// from — the smokestack part's mouth sits ≈ this far above its base.
pub(super) const FUNNEL_MOUTH_RISE: f32 = 0.5;

/// Inboard-embed fractions for the Bow / Stack part bases (#806).
///
/// `bow_z` / `stack_z` (from [`BoatBlueprint`](crate::seeded_defaults)) are the
/// *analytic* stem / stern stations, but the hull is a swept-blob iso-surface
/// that pulls inboard of those analytic tips by a seed/torture-dependent margin
/// — most at the fine prow, where a part seated on the tip floats ahead of the
/// mesh (the reported detached bowsprit). Seating each base at this fraction of
/// its analytic station pulls it *into* the hull, so it always embeds rather
/// than undershooting into open air. Embedding is invisible — the hull is
/// opaque and a bowsprit / funnel still projects clear via its own forward /
/// upward offset — and per the overshoot-beats-undershoot rule an embedded base
/// reads better than a floating one across every seed. The prow needs the
/// stronger pull (its cone tapers to a fine point the iso-surface eats most).
const BOW_HULL_EMBED: f32 = 0.80;
const STACK_HULL_EMBED: f32 = 0.86;

/// Stack (funnel) mount station (root-local, before the assembler's yaw) from
/// the deck line + the blueprint's aft stack station, pulled inboard by
/// [`STACK_HULL_EMBED`] so the funnel base embeds in the hull. The single
/// source the assembler seats the Stack part on and the FX steam anchor rises
/// from by [`FUNNEL_MOUTH_RISE`], so the steam leaves the same funnel the part
/// builds and stays anchored to the same seated base (#798, #806).
pub(super) fn stack_station(deck_y: f32, stack_z: f32) -> [f32; 3] {
    [0.0, deck_y * 0.62, stack_z * STACK_HULL_EMBED]
}
