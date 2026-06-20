//! Humanoid family assembler — composes the figure from the seeded
//! [`AvatarOutfit`] parts instead of hardcoded geometry.
//!
//! The assembler owns only the *skeleton*: a pelvis root at the entity
//! origin (hips at `y = 0`, which keeps the figure centred in the Humanoid
//! locomotion capsule) and the joint anchors each slot's part mounts to —
//! legs hanging from the hips, torso above, arms at the shoulder line (with
//! a slight outward + forward splay so they don't read as pinned to a tube),
//! head atop the torso, an optional hat above the head. Every slot's
//! geometry, colour, and finish comes from the part catalogue
//! ([`crate::pds::avatar::parts`]); the part is built in its own local
//! attachment frame and the assembler offsets it to the joint anchor.
//!
//! The pfp identity panel is *not* a part (it's identity, not cosmetics):
//! the assembler wears it as a small flush chest badge (front-facing) so a
//! re-roll never disturbs it. The seeded FX aura + voice are attached
//! centrally by [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::common::{
    PfpFacing, cuboid, id_quat, offset, offset_rot, pastel, pfp_panel, prim, quat_mul, quat_x,
    quat_xyzw, quat_z,
};

/// `seed` drives the derived look (re-roll re-seeds this); `did` is kept
/// only for identity references the seed must not touch — the pfp badge.
pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    let w = ctx.body.shoulder_width_scale;
    let limb = ctx.body.limb_thickness_scale;
    let primary = ctx.palette.primary_accent;
    // Trousers / pelvis share a darker shade of the primary so the lower body
    // reads as one outfit with the shirt (matches the leg part's trousers).
    let trousers = [primary[0] * 0.6, primary[1] * 0.6, primary[2] * 0.6];

    // ---- Skeleton anchors (hips at the origin) -----------------------------
    // The default parts carry fixed segment lengths; these nominal anchors
    // place them into a coherent figure (x scaled by build width). Styled
    // kits author their parts to the same anchors.
    let torso_r = 0.155 * w;
    let arm_r = 0.055 * limb;
    let torso_y = 0.32;
    let head_y = 0.85;
    let shoulder_y = 0.55;
    let shoulder_x = torso_r + arm_r + 0.02;
    let hip_x = torso_r * 0.55;
    // Slight outward (Z-roll) + forward (X-tilt) arm splay. The forward tilt
    // is gentle now that the arm part bends at the elbow on its own.
    let arm_splay = 0.14;
    let arm_forward = 0.05;

    // ---- Pelvis (root) -----------------------------------------------------
    let mut root = prim(
        cuboid(
            [torso_r * 1.9, 0.14, torso_r * 1.35],
            ctx.materials.body(trousers),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    root.transform = Default::default();

    // ---- Mount each filled slot at its anchor ------------------------------
    for choice in &outfit.parts {
        let Some(part) = by_slug(choice.slug) else {
            continue;
        };
        match choice.slot {
            PartSlot::Torso => root
                .children
                .push(offset(part.build(&ctx), [0.0, torso_y, 0.0])),
            PartSlot::Head => root
                .children
                .push(offset(part.build(&ctx), [0.0, head_y, 0.0])),
            PartSlot::Arm => {
                // One part, mirrored to both shoulders with an outward splay.
                for side in [-1.0f32, 1.0] {
                    let rot = quat_xyzw(quat_mul(quat_z(side * arm_splay), quat_x(arm_forward)));
                    root.children.push(offset_rot(
                        part.build(&ctx),
                        [side * shoulder_x, shoulder_y, 0.0],
                        rot,
                    ));
                }
            }
            PartSlot::Leg => {
                for side in [-1.0f32, 1.0] {
                    root.children
                        .push(offset(part.build(&ctx), [side * hip_x, 0.0, 0.0]));
                }
            }
            PartSlot::Hat => root
                .children
                .push(offset(part.build(&ctx), [0.0, head_y + 0.14, 0.0])),
            PartSlot::Ornament => root
                .children
                .push(offset(part.build(&ctx), [0.0, torso_y, -torso_r])),
            // Slots that don't belong to a humanoid are never produced for
            // this chassis by the outfit deriver; ignore defensively.
            _ => {}
        }
    }

    // ---- pfp identity worn as a flush chest badge --------------------------
    root.children.push(pfp_panel(
        did,
        0.16,
        [0.0, torso_y + 0.12, -(torso_r + 0.015)],
        pastel(primary),
        PfpFacing::Front,
    ));

    root
}
