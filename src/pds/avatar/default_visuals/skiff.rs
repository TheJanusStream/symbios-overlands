//! Land-skiff family assembler — composes the ground vehicle from the
//! seeded [`AvatarOutfit`](crate::seeded_defaults::AvatarOutfit) parts.
//!
//! The chassis (a shaped body with a lower skirt, rear cabin, and front
//! hood) is the structural root (centred at the origin); the canopy seats on
//! the cabin, one wheel part is repeated to the four corners (laid on its
//! axle by the assembler), and the optional exhaust mounts at the stern. All
//! geometry, colour, and finish come from the part catalogue
//! ([`crate::pds::avatar::parts`]); seeded FX are attached centrally by
//! [`super::build_for_seed`].

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::defaults::skiff::{skiff_dims, skiff_wheel_anchors};
use crate::pds::avatar::parts::{PartSlot, by_slug};
use crate::pds::generator::Generator;

use super::assemble::{apply_travel_pose, assemble_root, debug_assert_slots_handled};
use super::common::{offset, offset_rot, quat_xyzw, quat_z};

pub(super) fn build(seed: u64) -> Generator {
    // The chassis is the structural root (centred at the origin).
    let (outfit, ctx, mut root) = assemble_root(seed, PartSlot::Chassis);

    // Wheels are laid on their axle (cylinder Y-axis → X-axis).
    let axle = quat_xyzw(quat_z(FRAC_PI_2));

    // Wheel anchors + the fore/aft mount stations come from the SAME skiff
    // blueprint the chassis fenders and the wheel part read, so the wheels sit
    // exactly in their guards regardless of the seeded body size (#783). A trike
    // chassis collapses the two front anchors to a single centreline wheel — the
    // chassis draws the matching single front guard (#788).
    let dims = skiff_dims(&ctx);
    let dl = dims.1 / 1.5;
    let is_trike = outfit
        .parts
        .iter()
        .any(|p| p.slot == PartSlot::Chassis && p.slug == "skiff_chassis_trike");
    let wheel_anchors = skiff_wheel_anchors(dims, is_trike);

    for choice in &outfit.parts {
        if choice.slot == PartSlot::Chassis {
            continue;
        }
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            // Seat the canopy on the rear cabin (tracks the cabin's station).
            PartSlot::Canopy => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.33, -0.12 * dl])),
            PartSlot::Wheel => {
                // One wheel part repeated to each seeded anchor (four corners,
                // or three for a trike).
                for anchor in &wheel_anchors {
                    root.children
                        .push(offset_rot(part.build(&ctx), *anchor, axle));
                }
            }
            // Exhaust at the stern, seated into the rear bodywork (the tub
            // ends at z≈−0.75·len) so the stacks emerge from the deck rather
            // than hovering behind it (#780). A slug-aware / spine-swept
            // exhaust is the skiff redesign's job (#788).
            PartSlot::Exhaust => root
                .children
                .push(offset(part.build(&ctx), exhaust_station(dims.1))),
            // Ornament as a hood mascot on the bonnet nose (clear of every
            // canopy volume — a canopy-relative mount buried the neon strip
            // inside closed greenhouses and floated it over the open roadster
            // cockpit, #780). Single-mounted: every skiff ornament (mascot /
            // bull bar / neon strip) is a front, directional piece, so unlike
            // the boat's deck finials it doesn't line up in a tiered set (#798).
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, 0.17, 0.68 * dl])),
            _ => {}
        }
    }

    // Drop the whole visual so the wheels rest at the car's suspension ground
    // line — the chassis origin floats ≈0.87 m (half-extent 0.4 + rest 0.6 −
    // static compression ≈0.13) and the wheel bottoms sit ≈0.32 below the
    // visual origin.
    apply_travel_pose(&mut root, 0.55);
    debug_assert_slots_handled(
        &outfit,
        PartSlot::Chassis,
        &[
            PartSlot::Canopy,
            PartSlot::Wheel,
            PartSlot::Exhaust,
            PartSlot::Ornament,
        ],
    );
    root
}

/// Aft exhaust-pipe station (root-local, before the assembler's yaw) from the
/// seeded body length — the single source the assembler seats the Exhaust part
/// at and the FX exhaust-wisp anchor issues from, so the wisp leaves the same
/// pipe the part builds (#798). The tub ends at z ≈ −0.75·len, so the pipe sits
/// just inboard of the stern.
pub(super) fn exhaust_station(body_len: f32) -> [f32; 3] {
    [0.0, 0.05, -0.70 * (body_len / 1.5)]
}
