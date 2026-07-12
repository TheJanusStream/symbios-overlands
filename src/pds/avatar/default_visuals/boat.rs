//! Hover-boat family assembler — composes the vessel from the seeded
//! [`AvatarOutfit`] parts.
//!
//! The hull part is the structural root (a swept blob hull with a pointed prow
//! and sheer-following rub-strakes, centred at the waterline origin); the low
//! cabin deck sits just inside it, the rigged mast (a fore-and-aft mainsail, or
//! a styled square/antenna/derrick variant) rises from the deck, and the
//! optional bow ornament / stern stack mount fore and aft. All geometry,
//! colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); the assembler owns only the layout anchors.
//! Seeded FX are attached centrally by [`super::build_for_seed`].

use std::f32::consts::PI;

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{offset, quat_xyzw, quat_y};

pub(super) fn build(seed: u64) -> Generator {
    let outfit = AvatarOutfit::for_seed(seed);
    // Reuse the derived outfit for the ctx's hat flag (#638).
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));

    // The hull is the structural root (at the waterline origin).
    let mut root = base_root(&outfit, &ctx, PartSlot::Hull);

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
            PartSlot::Bow => root
                .children
                .push(offset(part.build(&ctx), [0.0, deck_y * 0.77, bow_z])),
            PartSlot::Stack => root
                .children
                .push(offset(part.build(&ctx), [0.0, deck_y * 0.62, stack_z])),
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, deck_y * 1.38, ornament_z])),
            _ => {}
        }
    }

    // Travel is toward local -Z; parts are authored front-+Z, so yaw 180°.
    // Drop to a low hover above the hover-boat's suspension ground line (the
    // chassis floats ≈0.97 m; a small gap keeps it reading as a hover-craft).
    root.transform.rotation = quat_xyzw(quat_y(PI));
    root.transform.translation = Fp3([0.0, -0.6, 0.0]);

    root
}
