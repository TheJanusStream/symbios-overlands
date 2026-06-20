//! Airship family assembler — composes the lighter-than-air craft from the
//! seeded [`AvatarOutfit`] parts.
//!
//! The envelope is the structural root (a cigar centred at the origin, built
//! from composed lobes so it carries **no** root scale — a root scale would
//! stretch and fling the children mounted here). The gondola slings beneath
//! it on rigging lines, and the stabiliser fins cluster as a cruciform tail
//! (one fin part placed at four tail positions, each rotated into place). All
//! geometry, colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); seeded FX are attached centrally by
//! [`super::build_for_seed`].

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{
    PfpFacing, cylinder, id_quat, offset, offset_rot, pastel, pfp_panel, prim, quat_x, quat_xyzw,
    quat_z,
};

pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    // The envelope is the structural root (centred at the origin, no scale).
    let mut root = base_root(&outfit, &ctx, PartSlot::Envelope);

    let gondola_y = -1.05;
    for choice in &outfit.parts {
        if choice.slot == PartSlot::Envelope {
            continue;
        }
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Gondola => root
                .children
                .push(offset(part.build(&ctx), [0.0, gondola_y, 0.0])),
            PartSlot::Fin => {
                // One fin part placed as a cruciform tail: dorsal, ventral,
                // and two horizontal stabilisers. The fin is centred on its
                // mount, so each copy is rotated about its own centre and its
                // inner edge buries in the tapering tail.
                let tail_z = -1.0;
                let placements = [
                    ([0.0, 0.55, tail_z], id_quat()),                     // dorsal (up)
                    ([0.0, -0.55, tail_z], quat_xyzw(quat_x(PI))),        // ventral (down)
                    ([-0.55, 0.0, tail_z], quat_xyzw(quat_z(FRAC_PI_2))), // port stabiliser
                    ([0.55, 0.0, tail_z], quat_xyzw(quat_z(-FRAC_PI_2))), // starboard
                ];
                for (anchor, rot) in placements {
                    root.children
                        .push(offset_rot(part.build(&ctx), anchor, rot));
                }
            }
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, gondola_y + 0.25, 0.6])),
            _ => {}
        }
    }

    // Suspension rigging — four near-vertical cables bridging the envelope
    // belly to the gondola so it reads as slung, not floating.
    let cable = ctx.materials.metal(ctx.palette.tertiary_accent);
    for x in [-0.22f32, 0.22] {
        for z in [-0.32f32, 0.32] {
            root.children.push(prim(
                cylinder(0.012, 0.4, 6, cable.clone()),
                [x, gondola_y + 0.35, z],
                id_quat(),
            ));
        }
    }

    // pfp identity worn as a roundel flush on the envelope flank (±X).
    root.children.push(pfp_panel(
        did,
        0.3,
        [0.78, 0.05, 0.1],
        pastel(ctx.palette.primary_accent),
        PfpFacing::Side,
    ));

    root
}
