//! Humanoid family assembler — composes the figure from the seeded
//! [`AvatarOutfit`] parts instead of hardcoded geometry.
//!
//! The assembler owns only the *skeleton*: a pelvis root at the entity
//! origin (hips at `y = 0`, which keeps the figure centred in the Humanoid
//! locomotion capsule) and the joint anchors each slot's part mounts to —
//! legs hanging from the hips, torso above, arms at the shoulder line, head
//! atop the torso, an optional hat above the head. Every slot's geometry,
//! colour, and finish comes from the part catalogue
//! ([`crate::pds::avatar::parts`]); the part is built in its own local
//! attachment frame and the assembler offsets it to the joint anchor.
//!
//! The pfp identity banner is *not* a part (it's identity, not cosmetics):
//! the assembler flies it from a back pole so a re-roll never disturbs it.
//! The seeded FX aura + voice are attached centrally by
//! [`super::build_for_seed`].

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::common::{cuboid, cylinder, id_quat, offset, pastel, pfp_banner, prim};

/// `seed` drives the derived look (re-roll re-seeds this); `did` is kept
/// only for identity references the seed must not touch — the pfp banner.
pub(super) fn build(seed: u64, did: &str) -> Generator {
    let ctx = PartCtx::for_seed(seed, did);
    let outfit = AvatarOutfit::for_seed(seed);

    let w = ctx.body.shoulder_width_scale;
    let limb = ctx.body.limb_thickness_scale;
    let trim = ctx.palette.tertiary_accent;

    // ---- Skeleton anchors (hips at the origin) -----------------------------
    // The default parts carry fixed segment lengths; these nominal anchors
    // place them into a coherent figure (x scaled by build width). Styled
    // kits author their parts to the same anchors.
    let torso_r = 0.155 * w;
    let arm_r = 0.058 * limb;
    let torso_y = 0.32;
    let head_y = 0.85;
    let shoulder_y = 0.55;
    let shoulder_x = torso_r + arm_r + 0.02;
    let hip_x = torso_r * 0.55;

    // ---- Pelvis (root) -----------------------------------------------------
    let mut root = prim(
        cuboid(
            [torso_r * 1.9, 0.14, torso_r * 1.35],
            ctx.materials.body(trim),
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
                // One part, mirrored to both shoulders.
                root.children
                    .push(offset(part.build(&ctx), [-shoulder_x, shoulder_y, 0.0]));
                root.children
                    .push(offset(part.build(&ctx), [shoulder_x, shoulder_y, 0.0]));
            }
            PartSlot::Leg => {
                root.children
                    .push(offset(part.build(&ctx), [-hip_x, 0.0, 0.0]));
                root.children
                    .push(offset(part.build(&ctx), [hip_x, 0.0, 0.0]));
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

    // ---- pfp identity banner on a back pole --------------------------------
    let banner_size = 0.30;
    let pole_h = 0.55;
    let pole_z = torso_r + 0.10;
    let mut pole = prim(
        cylinder(0.012, pole_h, 8, ctx.materials.metal(trim)),
        [0.0, torso_y + pole_h * 0.5, pole_z],
        id_quat(),
    );
    pole.children.push(pfp_banner(
        did,
        banner_size,
        [0.0, pole_h * 0.30, banner_size * 0.5 + 0.03],
        pastel(ctx.palette.primary_accent),
    ));
    root.children.push(pole);

    root
}
