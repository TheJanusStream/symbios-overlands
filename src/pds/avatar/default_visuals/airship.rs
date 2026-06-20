//! Airship family assembler — composes the lighter-than-air craft from the
//! seeded [`AvatarOutfit`] parts.
//!
//! The envelope is the structural root (centred at the origin); the gondola
//! slings beneath it and the stabiliser fins cluster at the stern (one fin
//! part placed at three stern positions). All geometry, colour, and finish
//! come from the part catalogue ([`crate::pds::avatar::parts`]); seeded FX
//! are attached centrally by [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{cylinder, id_quat, offset, pastel, pfp_banner, prim};

pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    // The envelope is the structural root (centred at the origin).
    let mut root = base_root(&outfit, &ctx, PartSlot::Envelope);

    let gondola_y = -0.95;
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
                // One fin part placed as a stern cluster: a top tail fin and
                // two side stabilisers.
                for anchor in [[0.0, 0.45, -1.3], [-0.5, 0.0, -1.3], [0.5, 0.0, -1.3]] {
                    root.children.push(offset(part.build(&ctx), anchor));
                }
            }
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, gondola_y + 0.2, 0.6])),
            _ => {}
        }
    }

    // pfp banner on a short pole off the gondola's starboard side.
    let pole_h = 0.4;
    let mut pole = prim(
        cylinder(
            0.012,
            pole_h,
            8,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, gondola_y, 0.55],
        id_quat(),
    );
    pole.children.push(pfp_banner(
        did,
        0.28,
        [0.0, pole_h * 0.2, 0.16],
        pastel(ctx.palette.primary_accent),
    ));
    root.children.push(pole);

    root
}
