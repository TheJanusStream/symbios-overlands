//! Hover-boat family assembler — composes the vessel from the seeded
//! [`AvatarOutfit`] parts.
//!
//! The hull part is the structural root (a shaped hull with a pointed prow
//! and gunwale rails, centred at the waterline origin); the deck sits just
//! inside it, the masted sail rises from the deck, and the optional bow
//! ornament / stern stack mount fore and aft. All geometry, colour, and
//! finish come from the part catalogue ([`crate::pds::avatar::parts`]); the
//! assembler owns only the layout anchors and the assembler-owned pfp
//! identity crest. Seeded FX are attached centrally by
//! [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{PfpFacing, offset, pastel, pfp_panel};

pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    // The hull is the structural root (at the waterline origin).
    let mut root = base_root(&outfit, &ctx, PartSlot::Hull);

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
                .push(offset(part.build(&ctx), [0.0, 0.21, 0.0])),
            PartSlot::Mast => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.27, 0.0])),
            PartSlot::Bow => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.30, 1.0])),
            PartSlot::Stack => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.35, -0.7])),
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.30, 0.4])),
            _ => {}
        }
    }

    // pfp identity worn as a crest on the sail (the sail's normal is ±X, so a
    // side-facing panel laid just off its face reads as a heraldic crest).
    root.children.push(pfp_panel(
        did,
        0.25,
        [0.02, 1.0, -0.1],
        pastel(ctx.palette.primary_accent),
        PfpFacing::Side,
    ));

    root
}
