//! Land-skiff family assembler — composes the ground vehicle from the
//! seeded [`AvatarOutfit`] parts.
//!
//! The chassis (a shaped body with a lower skirt, rear cabin, and front
//! hood) is the structural root (centred at the origin); the canopy seats on
//! the cabin, one wheel part is repeated to the four corners (laid on its
//! axle by the assembler), and the optional exhaust mounts at the stern. All
//! geometry, colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); seeded FX are attached centrally by
//! [`super::build_for_seed`].

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{offset, offset_rot, quat_xyzw, quat_y, quat_z};

pub(super) fn build(seed: u64) -> Generator {
    let outfit = AvatarOutfit::for_seed(seed);
    // Reuse the derived outfit for the ctx's hat flag (#638).
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));

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
            // Seat the canopy on the rear cabin.
            PartSlot::Canopy => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.33, -0.12])),
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
            // Exhaust at the stern, seated into the rear bodywork (the tub
            // ends at z≈−0.75) so the stacks emerge from the deck rather than
            // hovering behind it (#780). A slug-aware / spine-swept exhaust is
            // the skiff redesign's job (#788).
            PartSlot::Exhaust => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.05, -0.70])),
            // Ornament as a hood mascot on the bonnet nose (clear of every
            // canopy volume — a canopy-relative mount buried the neon strip
            // inside closed greenhouses and floated it over the open roadster
            // cockpit, #780). A slot-aware, per-canopy mount is #783's job.
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.17, 0.68])),
            _ => {}
        }
    }

    // Vehicles travel toward local -Z (`Transform::forward`), but the parts
    // are authored front-+Z, so yaw the whole visual 180°. Drop it so the
    // wheels rest at the car's suspension ground line — the chassis origin
    // floats ≈0.87 m (half-extent 0.4 + rest 0.6 − static compression ≈0.13)
    // and the wheel bottoms sit ≈0.32 below the visual origin.
    root.transform.rotation = quat_xyzw(quat_y(PI));
    root.transform.translation = Fp3([0.0, -0.55, 0.0]);

    root
}
