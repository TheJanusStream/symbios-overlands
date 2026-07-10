//! Humanoid family assembler — composes the figure from the seeded
//! [`AvatarOutfit`] parts instead of hardcoded geometry.
//!
//! The assembler owns only the *skeleton*: a pelvis root at the entity
//! origin (hips at `y = 0`, which keeps the figure centred in the Humanoid
//! locomotion capsule) and the joint anchors each slot's part mounts to —
//! legs hanging from the hips, torso above, arms at the shoulder line (with
//! a slight outward + forward splay so they don't read as pinned to a tube),
//! head atop the torso, an optional hat above the head. Every anchor comes
//! from the seeded [`HumanoidBlueprint`](crate::seeded_defaults::HumanoidBlueprint) — the same proportion contract the
//! parts and the locomotion capsule read — so canon landmarks (wrist at the
//! crotch line, legs ~half the figure, tier-banded head size) hold by
//! construction. Every slot's geometry, colour, and finish comes from the
//! part catalogue ([`crate::pds::avatar::parts`]); the part is built in its
//! own local attachment frame and the assembler offsets it to the joint
//! anchor.
//!
//! The pfp identity panel is *not* a part (it's identity, not cosmetics):
//! the assembler wears it as a small flush chest badge (front-facing) so a
//! re-roll never disturbs it. The seeded FX aura + voice are attached
//! centrally by [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::AvatarOutfit;

use super::common::{
    PfpFacing, blob_box, blob_carve, blob_ellipsoid, blob_group, blob_sphere, cuboid, id_quat,
    offset, offset_rot, pastel, pfp_panel, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z,
};

/// `seed` drives the derived look (re-roll re-seeds this); `did` is kept
/// only for identity references the seed must not touch — the pfp badge.
pub(super) fn build(seed: u64, did: &str) -> Generator {
    let outfit = AvatarOutfit::for_seed(seed);
    // Reuse the outfit we just derived for the ctx's hat flag instead of letting
    // `PartCtx::for_seed` derive a second one (#638).
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));
    let bp = ctx.blueprint;

    let primary = ctx.palette.primary_accent;
    // Trousers / pelvis share a darker shade of the primary so the lower body
    // reads as one outfit with the shirt (matches the leg part's trousers).
    let trousers = [primary[0] * 0.6, primary[1] * 0.6, primary[2] * 0.6];

    // ---- Skeleton anchors (hips at the origin) -----------------------------
    // All from the blueprint; the styled kits author their parts to the same
    // anchors via `ctx.blueprint`.
    //
    // Slight outward (Z-roll) + forward (X-tilt) arm splay: enough negative
    // space that the arms read in silhouette instead of merging into the
    // trunk. The forward tilt is gentle — the arm part bends at the elbow on
    // its own.
    let arm_splay = 0.14;
    let arm_forward = 0.05;
    // Ornaments are authored against the old fixed chest (r = 0.155);
    // scale them to this seed's chest. Hats self-scale in their builders.
    let chest_k = (bp.chest_r / 0.155).clamp(0.6, 1.15);

    // ---- Pelvis (root) -----------------------------------------------------
    // A small hidden structural core at the origin: the assembler mounts every
    // other slot onto it, so the root must keep an identity transform (a root
    // scale would stretch + fling the mounted slots). The *visible* pelvis is
    // a BlobGroup child (#726): a forward-pitched iliac block (the pelvis
    // half of the trunk's opposing-tilt pair), a glute pair projecting past
    // the back plane, hip flares out to the trouser line, and a crotch
    // relief carve so the leg split starts below the pelvis equator instead
    // of at the belt — the fix for the "diaper" / "straws into the hem"
    // read. Never mirrored, so left/right asymmetric masses are safe here.
    let mut root = prim(
        cuboid(
            [bp.waist_r * 0.9, bp.waist_r * 0.8, bp.waist_r * 0.7],
            ctx.materials.body(trousers),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    root.transform = Default::default();
    let wr = bp.waist_r;
    let d = bp.depth;
    let mut pelvis_elements = vec![blob_box(
        [0.0, -wr * 0.10, 0.0],
        [wr * 0.84, wr * 0.46, wr * 0.60 * d],
        quat_xyzw(quat_x(-0.16)),
        wr * 0.5,
    )];
    for s in [-1.0f32, 1.0] {
        // Glute pair — seat projection behind the back plane, blended
        // generously into the iliac block so it never reads as a bolted-on
        // ball (round 2 on two seeds).
        pelvis_elements.push(blob_sphere(
            [s * wr * 0.40, -wr * 0.40, wr * 0.42 * d],
            wr * 0.46,
            wr * 0.50,
        ));
        // Hip flare out to where the thighs socket in.
        pelvis_elements.push(blob_ellipsoid(
            [s * wr * 0.64, -wr * 0.18, 0.0],
            [wr * 0.40, wr * 0.52, wr * 0.58 * d],
            id_quat(),
            wr * 0.38,
        ));
    }
    pelvis_elements.push(blob_carve(blob_ellipsoid(
        [0.0, -wr * 0.80, -wr * 0.06 * d],
        [wr * 0.30, wr * 0.36, wr * 0.44 * d],
        id_quat(),
        wr * 0.22,
    )));
    root.children.push(prim(
        blob_group(pelvis_elements, 36, ctx.materials.body(trousers)),
        [0.0, -0.02, 0.0],
        id_quat(),
    ));

    // ---- Mount each filled slot at its anchor ------------------------------
    for choice in &outfit.parts {
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Torso => root
                .children
                .push(offset(part.build(&ctx), [0.0, bp.torso_y, 0.0])),
            PartSlot::Head => root
                .children
                .push(offset(part.build(&ctx), [0.0, bp.head_y, 0.0])),
            PartSlot::Arm => {
                // One part, mirrored to both shoulders with an outward splay.
                for side in [-1.0f32, 1.0] {
                    let rot = quat_xyzw(quat_mul(quat_z(side * arm_splay), quat_x(arm_forward)));
                    root.children.push(offset_rot(
                        part.build(&ctx),
                        [side * bp.shoulder_x, bp.shoulder_y, 0.0],
                        rot,
                    ));
                }
            }
            PartSlot::Leg => {
                // Mirrored to both hips with a slight toe-out yaw so the
                // stance reads planted instead of ruler-parallel. The leg
                // part is X-symmetric, so the ∓ yaw mirrors correctly.
                for side in [-1.0f32, 1.0] {
                    root.children.push(offset_rot(
                        part.build(&ctx),
                        [side * bp.hip_x, 0.0, 0.0],
                        quat_xyzw(quat_y(-side * 0.07)),
                    ));
                }
            }
            PartSlot::Hat => root.children.push(offset(
                part.build(&ctx),
                [0.0, bp.head_y + bp.head_r * 1.08, 0.0],
            )),
            PartSlot::Ornament => {
                // Seated proud of the trunk's *flattened* front surface at
                // its own height (the trunk V-tapers — the old top-radius
                // seat floated in profile).
                let orn_y = bp.torso_y - bp.trunk_len * 0.04;
                let surf = bp.trunk_radius_at(orn_y).max(bp.chest_r * 0.92);
                let mut orn = offset(part.build(&ctx), [0.0, orn_y, -(surf * bp.depth + 0.006)]);
                orn.transform.scale = Fp3([chest_k, chest_k, chest_k]);
                root.children.push(orn);
            }
            // Slots that don't belong to a humanoid are never produced for
            // this chassis by the outfit deriver; ignore defensively.
            _ => {}
        }
    }

    // ---- pfp identity worn as a flush chest badge --------------------------
    // Recessed INTO the chest mass (round 2: any proud flat plate shows
    // edge-on slivers past the silhouette in profile): the panel's normal
    // offset sits just inside the pectoral surface so the chest curvature
    // swallows its edges, with a slight downward tilt for the lower edge.
    // A scalar recess can't perfectly fit every chest convexity (a truly
    // conformal badge is follow-up work): 1.01 sits between the old proud
    // 1.02 + 0.012 (edge slivers in profile) and the round-3 0.99 (one
    // bulgy-chested seed poked mesh through the plate).
    let badge_y = bp.torso_y + bp.trunk_len * 0.24;
    let mut badge = pfp_panel(
        did,
        0.14 * chest_k,
        [0.0, badge_y, -(bp.chest_r * 1.01 * bp.depth + 0.008)],
        pastel(primary),
        PfpFacing::Front,
    );
    badge.transform.rotation = quat_xyzw(quat_mul(
        quat_x(-0.10),
        [
            badge.transform.rotation.0[0],
            badge.transform.rotation.0[1],
            badge.transform.rotation.0[2],
            badge.transform.rotation.0[3],
        ],
    ));
    root.children.push(badge);

    root
}
