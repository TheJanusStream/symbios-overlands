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
    blob_box, blob_carve, blob_ellipsoid, blob_group, blob_sphere, cuboid, id_quat, offset,
    offset_rot, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z,
};

/// `seed` drives the derived look (re-roll re-seeds this).
pub(super) fn build(seed: u64) -> Generator {
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
    // NOTE (#729): four render-driven rounds of PELVIS-side seat reshaping
    // (paired-glute trims, a single wide seat mass, a sub-gluteal fill,
    // crotch-carve and thigh-root retunes) each regressed the buttock→thigh
    // read below this baseline — a pelvis L/R mass pair is separated by the
    // midline gap, so softening the side undercut always exposed a back-view
    // double-lobe + central cleft. A 5th, cleft-immune approach (a fill in
    // the mirrored LEG group, not the pelvis) was then render-tested and
    // also reverted (see the leg builder's #729 note): the seat overhangs a
    // void ≈2·leg_r deep behind the thigh while the thigh's own back surface
    // sits at ≈0.83·leg_r, so a leg-group fill either projects a saddlebag
    // proud of the thigh or is invisible — there is no sweet spot, and the
    // glute is laterally offset from the leg mount (oppositely per tier) so
    // an X-symmetric leg can't align with it anyway. The mild sev2 undercut
    // is therefore accepted as within-tolerance for a stylized figure; a
    // real fix needs the pelvis+thigh unified into ONE blob group (a larger
    // restructure of the leg mounting, with its own cleft risks). The #728
    // waist-shelf improvement is independent and retained.
    for s in [-1.0f32, 1.0] {
        // Glute pair — seat projection behind the back plane, blended
        // generously into the iliac block so it never reads as a bolted-on
        // ball.
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
            PartSlot::Head => root.children.push(offset(
                part.build(&ctx),
                // Set back a touch onto the trunk axis so the head reads
                // centred over the (now flatter) chest rather than thrust
                // forward — the #728-D forward-head read. Gentle (0.03·d):
                // 0.05 over-cranked the neck on one seed.
                [0.0, bp.head_y, bp.chest_r * 0.03 * bp.depth],
            )),
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
                // Lowered 1.08→1.04·head_r so the brim rests on the crown
                // instead of hovering above a bare/short-hair scalp (#730-H6).
                [0.0, bp.head_y + bp.head_r * 1.04, 0.0],
            )),
            PartSlot::Ornament => {
                // Seated on the trunk's ACTUAL sampled front surface via
                // `trunk_front_z` (the pectoral-aware envelope), not the
                // linear `trunk_radius_at` cylinder that floated ornaments
                // on a stalk when it overshot the real surface — the #727
                // pattern-A failure on 5/12 seeds. The ornament's origin
                // sits at the surface so its core embeds and its face reads
                // proud. `chest_r` (shirt baseline) is close enough for the
                // coat's 1.04× trunk given the embed margin.
                let orn_y = bp.torso_y - bp.trunk_len * 0.04;
                let y_local = orn_y - bp.torso_y;
                // Seat the ornament's ORIGIN just proud of the raw envelope.
                // The margin is small (0.03) because an ornament carries its
                // own forward depth — the round-2 button-sized 0.12 margin
                // floated the whole ornament off the chest in profile (#727).
                let surf = bp.trunk_front_z(bp.chest_r, y_local) - bp.chest_r * 0.03 * bp.depth;
                let mut orn = offset(part.build(&ctx), [0.0, orn_y, surf]);
                orn.transform.scale = Fp3([chest_k, chest_k, chest_k]);
                root.children.push(orn);
            }
            // Slots that don't belong to a humanoid are never produced for
            // this chassis by the outfit deriver; ignore defensively.
            _ => {}
        }
    }

    root
}
