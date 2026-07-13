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

use crate::pds::avatar::parts::defaults::airship::{EnvProfile, airship_colors, airship_profile};
use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::{Fp3, Fp4};
use crate::seeded_defaults::AvatarOutfit;

use super::assemble::base_root;
use super::common::{cylinder, id_quat, offset, offset_rot, prim, quat_xyzw, quat_y, quat_z};

pub(super) fn build(seed: u64) -> Generator {
    let outfit = AvatarOutfit::for_seed(seed);
    // Reuse the derived outfit for the ctx's hat flag (#638).
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));

    // The envelope is the structural root (centred at the origin, no scale).
    let mut root = base_root(&outfit, &ctx, PartSlot::Envelope);

    // Mount landmarks come from the *chosen envelope form*, not a one-size
    // constant: the belly line the gondola slings from, the tail station and
    // fin ring the cruciform fins seat on. This is what makes the rigging
    // actually reach the hull (the twin's belly sits far higher than the
    // zeppelin's) and keeps the fins from being swallowed by a fat blimp tail.
    // Landmarks derive from the *same* seeded Lathe profile the envelope part
    // laths, so the gondola / fins / pods stay seated as the blueprint stretches
    // the envelope's length + girth (#791).
    let mounts = resolve_mounts(&outfit, &ctx);
    let gondola_scale = ctx.airship().map_or(1.0, |a| a.gondola_scale);

    // The gondola hangs a short gap below the belly; its roof is a cabin
    // half-height under its own centre.
    let gondola_y = gondola_hang_y(&mounts);
    let gondola_roof = gondola_y + 0.15 * gondola_scale;

    for choice in &outfit.parts {
        if choice.slot == PartSlot::Envelope {
            continue;
        }
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Gondola => {
                let mut g = part.build(&ctx);
                // The gondola is a leaf part, so a uniform root scale is safe
                // (nothing is mounted onto it).
                g.transform.scale = Fp3([gondola_scale, gondola_scale, gondola_scale]);
                root.children.push(offset(g, [0.0, gondola_y, 0.0]));
            }
            PartSlot::Fin => {
                // One fin part placed as a cruciform tail: dorsal, ventral,
                // and two horizontal stabilisers. The fin is centred on its
                // mount, so each copy is rotated about its own centre and its
                // inner edge buries in the tail body.
                for (anchor, rot) in fin_placements(mounts.tail_z, mounts.fin_ring_r) {
                    root.children
                        .push(offset_rot(part.build(&ctx), anchor, rot));
                }
            }
            PartSlot::Pod => {
                // Underslung engine nacelles — one mirrored pair amidships. The
                // pod is authored X-symmetric (a vertical pylon reaches up into
                // the flank), so each side is the same part, no mirror flip.
                for sx in [-1.0f32, 1.0] {
                    root.children.push(offset(
                        part.build(&ctx),
                        [sx * mounts.pod_x, mounts.pod_y, mounts.pod_z],
                    ));
                }
            }
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, gondola_y + 0.25, 0.6])),
            _ => {}
        }
    }

    // Suspension rigging — four cables that actually bridge the envelope belly
    // to the gondola roof: the length is computed from the belly→roof gap, so
    // they never hang short of the hull (the twin bug) or bury into it. The
    // cables wear the ship's structural `frame` colour so the rigging reads as
    // one system with the fins / keel, not a stray tertiary third draw (#789).
    let cable = ctx.materials.metal(airship_colors(&ctx).frame);
    let cable_len = (mounts.belly_y - gondola_roof).max(0.05);
    let cable_mid = (mounts.belly_y + gondola_roof) * 0.5;
    for x in [-0.2f32, 0.2] {
        for z in [-0.3f32, 0.3] {
            root.children.push(prim(
                cylinder(0.012, cable_len, 6, cable.clone()),
                [x, cable_mid, z],
                id_quat(),
            ));
        }
    }

    // Travel is toward local -Z; the envelope nose is authored at +Z, so yaw
    // the craft 180° to fly nose-first. No vertical drop — a helicopter hovers.
    root.transform.rotation = quat_xyzw(quat_y(PI));

    root
}

/// Per-envelope-form mount landmarks (metres, envelope centred at the origin).
/// Each form's belly line, tail station and fin ring radius are hand-fit to
/// that form's geometry so the slung gondola and cruciform fins seat on *its*
/// body — the fix for the envelope-invariant anchors that floated the twin's
/// rigging and swallowed the blimp's fins (#783, absorbing #781's fin item).
struct AirshipMounts {
    /// Envelope lowest point at the centreline — cable top + gondola hang line.
    belly_y: f32,
    /// Fin cluster station (−Z).
    tail_z: f32,
    /// Radial offset of the cruciform fins from the tail axis.
    fin_ring_r: f32,
    /// Lateral offset of the underslung engine pods (the assembler mirrors a
    /// pair to ±`pod_x`); their pylons reach up into the envelope's lower flank.
    pod_x: f32,
    /// Vertical station of the pod nacelles (below the flank).
    pod_y: f32,
    /// Fore-aft station of the pods (amidships, biased a touch aft).
    pod_z: f32,
}

/// Resolve the seeded envelope's mount landmarks — the single source both the
/// assembler and the FX vent anchor read, so a gondola / vent that tracks the
/// belly can't drift apart from the hull it hangs under.
fn resolve_mounts(outfit: &AvatarOutfit, ctx: &PartCtx) -> AirshipMounts {
    let env_slug = outfit
        .parts
        .iter()
        .find(|p| p.slot == PartSlot::Envelope)
        .map_or("default_envelope", |p| p.slug);
    let (len_mult, radius_mult) = ctx
        .airship()
        .map_or((1.0, 1.0), |a| (a.len_mult, a.radius_mult));
    airship_mounts(env_slug, &airship_profile(env_slug, len_mult, radius_mult))
}

/// The gondola hang line: a short gap below the envelope belly. Shared so the
/// FX vent puff sits under the same floor the gondola actually hangs at.
fn gondola_hang_y(mounts: &AirshipMounts) -> f32 {
    mounts.belly_y - 0.34
}

/// The diegetic vent-puff FX station beneath the gondola (root-local frame,
/// *before* the assembler's yaw), tracking the seeded envelope so the vapour
/// issues from under the actual gondola rather than a fixed point.
pub(super) fn fx_belly_anchor(seed: u64) -> [f32; 3] {
    let outfit = AvatarOutfit::for_seed(seed);
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));
    let mounts = resolve_mounts(&outfit, &ctx);
    // Just below the gondola floor.
    [0.0, gondola_hang_y(&mounts) - 0.15, 0.0]
}

fn airship_mounts(slug: &str, p: &EnvProfile) -> AirshipMounts {
    // Landmarks are read off the seeded profile `p` so they track the
    // blueprint's length + girth stretch (#791) — bar the twin's fin ring, which
    // is fixed to its centreline empennage (see below). The twin is the one
    // bespoke case: its two hulls straddle the centreline, so the gondola / pods
    // hang from the tunnel, not a single belly.
    if slug == "default_envelope_twin" {
        let hull_x = p.max_r + 0.02;
        return AirshipMounts {
            belly_y: -p.max_r,
            tail_z: -0.4 * p.length,
            // Deliberately fixed: the twin's cruciform fins cluster on the
            // fixed-size *central empennage* at the centreline (the vertical
            // stabiliser is ±0.55 tall), not on either hull surface — so this
            // ring tracks the empennage, not the hull girth like the others.
            fin_ring_r: 0.5,
            pod_x: hull_x,
            pod_y: -p.max_r * 0.9,
            pod_z: -0.06 * p.length,
        };
    }
    // Single-hull forms: `fin_t` is the tail-inboard station where the profile
    // still has girth for the cruciform fins to grip (matching the −0.4·length
    // the twin empennage uses).
    let fin_t = 0.1;
    AirshipMounts {
        belly_y: -p.max_r * 0.94,
        tail_z: p.height(fin_t),
        fin_ring_r: p.radius(fin_t) + 0.08,
        pod_x: p.max_r * 0.8,
        pod_y: -p.max_r * 0.95,
        pod_z: -0.05 * p.length,
    }
}

/// The four cruciform-tail fin placements (anchor + rotation) at tail station
/// `tail_z`, offset `r` from the axis so each fin's inner edge buries in the
/// envelope while its blade clears the hull. The fin part is authored upright
/// with its aft edge at local −Z; each copy must keep that aft sweep pointing
/// aft, so every rotation here preserves the local −Z axis (dorsal keeps it
/// identity; the stabilisers spin about Z; the ventral mirrors about **Z**,
/// not X — a `quat_x(PI)` would flip the sweep and glow edge to +Z, the
/// forward-swept-ventral-fin bug, #779).
fn fin_placements(tail_z: f32, r: f32) -> [([f32; 3], Fp4); 4] {
    [
        ([0.0, r, tail_z], id_quat()),                     // dorsal (up)
        ([0.0, -r, tail_z], quat_xyzw(quat_z(PI))),        // ventral (down)
        ([-r, 0.0, tail_z], quat_xyzw(quat_z(FRAC_PI_2))), // port stabiliser
        ([r, 0.0, tail_z], quat_xyzw(quat_z(-FRAC_PI_2))), // starboard
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
        for (i, (_, rot)) in fin_placements(-1.0, 0.55).into_iter().enumerate() {
            let aft = rotate(rot.0, [0.0, 0.0, -1.0]);
            assert!(
                aft[2] < -0.999,
                "fin {i} sweep points z={} (must stay aft, ≈−1)",
                aft[2]
            );
        }
    }
}
