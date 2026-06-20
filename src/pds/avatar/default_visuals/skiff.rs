//! Land-skiff family assembler — composes the ground vehicle from the
//! seeded [`AvatarOutfit`] parts.
//!
//! The chassis slab is the structural root (centred at the origin); the
//! canopy sits atop it, one wheel part is repeated to the four corners
//! (laid on its axle by the assembler), and the optional exhaust mounts at
//! the stern. All geometry, colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); seeded FX are attached centrally by
//! [`super::build_for_seed`].

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{
    cylinder, id_quat, offset, offset_rot, pastel, pfp_banner, prim, quat_xyzw, quat_z,
};

pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    // The chassis is the structural root (centred at the origin).
    let mut root = base_root(&outfit, &ctx, PartSlot::Chassis);

    // Wheels are laid on their axle (cylinder Y-axis → X-axis).
    let axle = quat_xyzw(quat_z(FRAC_PI_2));

    for choice in &outfit.parts {
        if choice.slot == PartSlot::Chassis {
            continue;
        }
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Canopy => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.25, 0.15])),
            PartSlot::Wheel => {
                // One wheel part repeated to the four corners.
                for anchor in [
                    [-0.45, -0.12, 0.55],
                    [0.45, -0.12, 0.55],
                    [-0.45, -0.12, -0.55],
                    [0.45, -0.12, -0.55],
                ] {
                    root.children
                        .push(offset_rot(part.build(&ctx), anchor, axle));
                }
            }
            PartSlot::Exhaust => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.15, -0.85])),
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.30, 0.0])),
            _ => {}
        }
    }

    // pfp banner on a short pole off the stern deck.
    let pole_h = 0.45;
    let mut pole = prim(
        cylinder(
            0.012,
            pole_h,
            8,
            ctx.materials.metal(ctx.palette.tertiary_accent),
        ),
        [0.0, 0.15 + pole_h * 0.5, -0.4],
        id_quat(),
    );
    pole.children.push(pfp_banner(
        did,
        0.28,
        [0.0, pole_h * 0.25, 0.16],
        pastel(ctx.palette.primary_accent),
    ));
    root.children.push(pole);

    root
}
