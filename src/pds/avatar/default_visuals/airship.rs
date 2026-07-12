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

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::Fp4;
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{cylinder, id_quat, offset, offset_rot, prim, quat_xyzw, quat_y, quat_z};

pub(super) fn build(seed: u64) -> Generator {
    let outfit = AvatarOutfit::for_seed(seed);
    // Reuse the derived outfit for the ctx's hat flag (#638).
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));

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
                for (anchor, rot) in fin_placements(-1.0) {
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

    // Travel is toward local -Z; the envelope nose is authored at +Z, so yaw
    // the craft 180° to fly nose-first. No vertical drop — a helicopter hovers.
    root.transform.rotation = quat_xyzw(quat_y(PI));

    root
}

/// The four cruciform-tail fin placements (anchor + rotation) at tail station
/// `tail_z`. The fin part is authored upright with its aft edge at local −Z;
/// each copy must keep that aft sweep pointing aft, so every rotation here
/// preserves the local −Z axis (dorsal keeps it identity; the stabilisers spin
/// about Z; the ventral mirrors about **Z**, not X — a `quat_x(PI)` would flip
/// the sweep and glow edge to +Z, the forward-swept-ventral-fin bug, #779).
fn fin_placements(tail_z: f32) -> [([f32; 3], Fp4); 4] {
    [
        ([0.0, 0.55, tail_z], id_quat()),                     // dorsal (up)
        ([0.0, -0.55, tail_z], quat_xyzw(quat_z(PI))),        // ventral (down)
        ([-0.55, 0.0, tail_z], quat_xyzw(quat_z(FRAC_PI_2))), // port stabiliser
        ([0.55, 0.0, tail_z], quat_xyzw(quat_z(-FRAC_PI_2))), // starboard
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::default_visuals::common::quat_mul;

    /// Rotate `v` by the `[x,y,z,w]` quaternion `q` (v' = q·v·q⁻¹).
    fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        let [qx, qy, qz, qw] = q;
        let vq = [v[0], v[1], v[2], 0.0];
        let qi = [-qx, -qy, -qz, qw];
        let r = quat_mul(quat_mul(q, vq), qi);
        [r[0], r[1], r[2]]
    }

    #[test]
    fn every_fin_keeps_its_sweep_aft() {
        // The fin's aft-swept edge is local −Z; each cruciform placement must
        // keep it pointing aft (world −Z) so dorsal and ventral fins sweep the
        // same way. Regression guard for the quat_x(PI) ventral flip (#779).
        for (i, (_, rot)) in fin_placements(-1.0).into_iter().enumerate() {
            let aft = rotate(rot.0, [0.0, 0.0, -1.0]);
            assert!(
                aft[2] < -0.999,
                "fin {i} sweep points z={} (must stay aft, ≈−1)",
                aft[2]
            );
        }
    }
}
