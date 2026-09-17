//! Land-skiff family assembler - composes the ground vehicle from the
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

use crate::pds::avatar::locomotion::CarParams;
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
    // chassis collapses the two front anchors to a single centreline wheel - the
    // chassis draws the matching single front guard (#788).
    let dims = skiff_dims(&ctx);
    let dl = dims.1 / AUTHORED_BODY_LEN;
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
            // canopy volume - a canopy-relative mount buried the neon strip
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

    // Size the whole craft to airship class and drop it so the tyres rest on
    // the car's suspension ground line - for THIS seed's wheels, not a nominal
    // pair (dims.5 is the seeded tyre radius, dims.4 the hub line).
    apply_travel_pose(&mut root, travel_drop(dims.5, dims.4), VISUAL_SCALE);
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

/// Aft exhaust-pipe station (root-local, before the assembler's yaw, drop and
/// scale) from the seeded body length - the single source the assembler seats
/// the Exhaust part at and the FX exhaust-wisp anchor issues from, so the wisp
/// leaves the same pipe the part builds (#798). The tub ends at z ≈ −0.75·len,
/// so the pipe sits just inboard of the stern.
pub(super) fn exhaust_station(body_len: f32) -> [f32; 3] {
    [0.0, 0.05, -0.70 * (body_len / AUTHORED_BODY_LEN)]
}

// ---------------------------------------------------------------------------
// Airship-class scale bridge (#1361)
// ---------------------------------------------------------------------------
//
// Same bridge as the boat's, for the same reason and with the same expiry: the
// legacy skiff parts are authored around a 1.5 m body against a 3.15 m airship
// and a 1.7 m person, and one uniform scale at the assembled root buys owner
// decision 1 of #1359 without disturbing a single mount (see
// [`apply_travel_pose`]). It is THROWAWAY - the whole legacy skiff pipeline
// dies when the roadster lands in #1364. [`NOMINAL_BODY_LEN`] is not: the
// locomotion in [`super`] is re-based on it.

/// Body-tub length (m) the legacy skiff parts are authored around: the
/// `SkiffBlueprint` nominal every part fraction and wheel landmark is taken
/// from, before the seeded stance / body multipliers spread it.
pub(super) const AUTHORED_BODY_LEN: f32 = 1.5;

/// Authored nominal body width (m) - the blueprint's, likewise pre-multipliers.
pub(super) const AUTHORED_BODY_W: f32 = 0.76;

/// Body-tub length (m) a nominal seeded skiff is **drawn** at: airship class,
/// per owner decision 1 of #1359 (skiffs about 2.65 m). Still a scale model of
/// a bigger machine - lit ports, no driver - not a rideable car.
pub(super) const NOMINAL_BODY_LEN: f32 = 2.65;

/// The single uniform scale the assembler puts on the visual root, so that an
/// authored-nominal body is drawn at [`NOMINAL_BODY_LEN`].
pub(super) const VISUAL_SCALE: f32 = NOMINAL_BODY_LEN / AUTHORED_BODY_LEN;

/// Half-height (m) of the legacy body tub in the authoring frame.
///
/// Read off the chassis part rather than guessed: it draws the body slab (half
/// 0.115, centred on the origin) with the rocker skirt under it (half 0.06 at
/// y −0.12, so −0.18) and the cabin bulge over it (half 0.10 at y +0.13, so
/// +0.23) - 0.41 m of bodywork, half of it 0.205. Unlike the tub's *length*,
/// these are fixed constants in the part, so the tub is the same height for
/// every seed.
const AUTHORED_BODY_HALF_HEIGHT: f32 = 0.205;

/// Half-height (m) of the skiff's chassis collider box.
///
/// Derived from the bodywork the craft actually draws, which is what makes it
/// safe at airship-class size. The old `0.4 · (body_len / 1.5)` tracked the
/// tub's *length*, so a body scaled to 2.65 m would have stood in a 1.4 m tall
/// collider - the tall-narrow shape behind the #804 rollovers, on a machine
/// that is only 0.72 m tall. `center_of_mass_drop` is a fraction of this, so
/// the anti-rollover centre-of-mass drop follows it down untouched.
pub(super) fn chassis_half_height() -> f32 {
    AUTHORED_BODY_HALF_HEIGHT * VISUAL_SCALE
}

/// Height (m) above flat ground the skiff's **chassis origin** rests at.
///
/// The four corner springs carry the weight from `half_height` below the
/// origin, compressing by [`super::static_suspension_compression`] - so the
/// origin floats that much less than a full suspension rest length above the
/// ground.
fn chassis_ride_height() -> f32 {
    let p = CarParams::default();
    chassis_half_height() + p.suspension_rest_length.0
        - super::static_suspension_compression(super::SKIFF_REF_MASS, p.suspension_stiffness.0)
}

/// Travel-pose drop (m) for a skiff whose seeded wheels have authoring-frame
/// radius `wheel_r` and hub line `ride_y` (negative - the hubs hang below the
/// body origin).
///
/// **Derived, not tuned**: put the tyre bottoms exactly on the suspension
/// ground line. The old hand-set 0.55 assumed one nominal wheel, so the seeded
/// radius (0.17-0.25) and hub line already floated or sank the wheels by
/// centimetres before anything was scaled; at airship-class size the same
/// constant would have buried them.
fn travel_drop(wheel_r: f32, ride_y: f32) -> f32 {
    chassis_ride_height() - (wheel_r - ride_y) * VISUAL_SCALE
}
