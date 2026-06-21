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

use std::f32::consts::PI;

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{PfpFacing, offset, pastel, pfp_panel, quat_xyzw, quat_y};

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
                .push(offset(part.build(&ctx), [0.0, 0.04, 0.52])),
            PartSlot::Stack => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.04, -0.5])),
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.2, 0.15])),
            _ => {}
        }
    }

    // pfp identity worn as a livery decal on the hull flank (normal ±X), since
    // the hover-skiff carries no sail to fly a crest from.
    root.children.push(pfp_panel(
        did,
        0.22,
        [0.32, 0.05, 0.05],
        pastel(ctx.palette.primary_accent),
        PfpFacing::Side,
    ));

    // Travel is toward local -Z; parts are authored front-+Z, so yaw 180°.
    // Drop to a low hover above the hover-boat's suspension ground line (the
    // chassis floats ≈0.97 m; a small gap keeps it reading as a hover-craft).
    root.transform.rotation = quat_xyzw(quat_y(PI));
    root.transform.translation = Fp3([0.0, -0.6, 0.0]);

    root
}
