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
    PfpFacing, cuboid, id_quat, offset, offset_rot, pastel, pfp_panel, prim, quat_mul, quat_x,
    quat_xyzw, quat_z, sphere,
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
    // Legacy part scale: hats and ornaments are authored against the old
    // fixed head (r = 0.13) / chest (r = 0.155); scale them uniformly to
    // this seed's head and chest so a Toy figure doesn't drown under an
    // adult-sized top hat. (Uniform root scale is safe — it propagates to
    // the part's children by design here.)
    let hat_k = bp.head_r / 0.13;
    let chest_k = (bp.chest_r / 0.155).clamp(0.6, 1.15);

    // ---- Pelvis (root) -----------------------------------------------------
    // A small hidden structural core at the origin: the assembler mounts every
    // other slot onto it, so the root must keep an identity transform (a root
    // scale would stretch + fling the mounted slots). The *visible* hip is a
    // rounded ellipsoid child (which may scale), seated low and kept flush
    // with the trunk's waist so the trousered legs emerge from a rounded
    // pelvis rather than a wider "diaper" bulge.
    let mut root = prim(
        cuboid(
            [bp.waist_r * 0.9, bp.waist_r * 0.8, bp.waist_r * 0.7],
            ctx.materials.body(trousers),
        ),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    root.transform = Default::default();
    let mut pelvis = prim(
        sphere(1.0, 3, ctx.materials.body(trousers)),
        [0.0, -0.02, 0.0],
        id_quat(),
    );
    pelvis.transform.scale = Fp3([
        bp.waist_r * 1.02,
        bp.waist_r * 0.75,
        bp.waist_r * 0.90 * bp.depth,
    ]);
    root.children.push(pelvis);

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
                for side in [-1.0f32, 1.0] {
                    root.children
                        .push(offset(part.build(&ctx), [side * bp.hip_x, 0.0, 0.0]));
                }
            }
            PartSlot::Hat => {
                let mut hat = offset(part.build(&ctx), [0.0, bp.head_y + bp.head_r * 1.08, 0.0]);
                hat.transform.scale = Fp3([hat_k, hat_k, hat_k]);
                root.children.push(hat);
            }
            PartSlot::Ornament => {
                // Seated proud of the trunk's *flattened* front face at chest
                // height (the trunk V-tapers, so the surface there is wider
                // than the waist).
                let surf = bp.waist_r + (bp.chest_r - bp.waist_r) * 0.5;
                let mut orn = offset(
                    part.build(&ctx),
                    [
                        0.0,
                        bp.torso_y - bp.trunk_len * 0.04,
                        -(surf * bp.depth + 0.012),
                    ],
                );
                orn.transform.scale = Fp3([chest_k, chest_k, chest_k]);
                root.children.push(orn);
            }
            // Slots that don't belong to a humanoid are never produced for
            // this chassis by the outfit deriver; ignore defensively.
            _ => {}
        }
    }

    // ---- pfp identity worn as a flush chest badge --------------------------
    let badge_y = bp.torso_y + bp.trunk_len * 0.24;
    root.children.push(pfp_panel(
        did,
        0.16 * chest_k,
        [0.0, badge_y, -(bp.chest_r * bp.depth + 0.02)],
        pastel(primary),
        PfpFacing::Front,
    ));

    root
}
