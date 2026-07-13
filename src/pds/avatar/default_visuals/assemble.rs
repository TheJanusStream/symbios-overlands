//! Shared assembler helpers that bridge the seeded [`AvatarOutfit`] and the
//! [`crate::pds::avatar::parts`] catalogue.
//!
//! Kept separate from [`super::common`] (which is pure primitive geometry,
//! reused by the parts themselves) because these helpers depend on the
//! outfit + part registry — the assembly layer, not the geometry layer.

use std::f32::consts::PI;

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug, outfit_has_hat};
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{AvatarOutfit, OrnatenessTier};

use super::common::{cuboid, id_quat, prim, quat_xyzw, quat_y};

/// How many ornament instances to mount for the avatar's ornateness tier — a
/// Plain craft carries one, an Ornate one three — so "Ornate" reads as the
/// promised decorative density instead of a single trinket at every tier
/// (#798). An assembler mounts its ornament part at the first `count` stations
/// of its per-family station list.
pub(super) fn ornament_count(ctx: &PartCtx) -> usize {
    match ctx.ornateness {
        OrnatenessTier::Plain => 1,
        OrnatenessTier::Adorned => 2,
        OrnatenessTier::Ornate => 3,
    }
}

/// The seeded outfit + part context an assembler (or a per-family FX anchor)
/// works from — bundles the two-step derivation every one repeats.
pub(super) fn outfit_ctx(seed: u64) -> (AvatarOutfit, PartCtx) {
    let outfit = AvatarOutfit::for_seed(seed);
    let ctx = PartCtx::for_seed_with_hat(seed, outfit_has_hat(&outfit));
    (outfit, ctx)
}

/// The seeded outfit + ctx + the family's structural root, ready for the
/// assembler to mount the remaining slots on — the boilerplate every family
/// `build()` opens with.
pub(super) fn assemble_root(seed: u64, base: PartSlot) -> (AvatarOutfit, PartCtx, Generator) {
    let (outfit, ctx) = outfit_ctx(seed);
    let root = base_root(&outfit, &ctx, base);
    (outfit, ctx, root)
}

/// Set the family's travel pose on the visual root: parts are authored
/// front-`+Z`, so yaw 180° to travel nose-first, then drop by `drop_y` onto the
/// craft's ground / hover line (`0.0` for the always-hovering airship). The
/// assembler *owns* the root's placement — this overwrites whatever transform
/// the structural root part set, which is exactly why that part builds at the
/// identity (see [`base_root`]).
pub(super) fn apply_travel_pose(root: &mut Generator, drop_y: f32) {
    root.transform.rotation = quat_xyzw(quat_y(PI));
    root.transform.translation = Fp3([0.0, -drop_y, 0.0]);
}

/// Debug-assert every non-root slot the outfit rolled is one the assembler
/// actually mounts — a guard so a newly-added optional slot can't be silently
/// swallowed by an assembler's `_ => {}` arm. `handled` lists the mounted slots.
pub(super) fn debug_assert_slots_handled(
    outfit: &AvatarOutfit,
    base: PartSlot,
    handled: &[PartSlot],
) {
    for choice in &outfit.parts {
        debug_assert!(
            choice.slot == base || handled.contains(&choice.slot),
            "the {base:?}-rooted assembler rolled a {:?} part it does not mount — \
             add a match arm and list the slot in `handled`",
            choice.slot
        );
    }
}

/// Build the part filling `base` as the family's structural root node (at the
/// origin). Falls back to a plain cuboid only if the base slot is somehow
/// unfilled — the universal default parts make that unreachable in practice.
///
/// The family assembler owns the visual root's *placement* — it applies the
/// 180° travel yaw + the hover/ground drop to the root's translation/rotation
/// *after* this. But the root's **scale** it does not touch, and a structural
/// root must not set one: every other slot (deck, canopy, wheels, gondola,
/// fins) mounts as a child of this root and would inherit a root scale,
/// stretching + flinging the whole avatar (the root-scale discipline — see the
/// defaults module docstring). This debug-asserts the discipline so a future
/// author who reaches for a root scale is told to shape a child instead,
/// rather than shipping a warped avatar silently (#798).
pub(super) fn base_root(outfit: &AvatarOutfit, ctx: &PartCtx, base: PartSlot) -> Generator {
    let root = outfit
        .parts
        .iter()
        .find(|c| c.slot == base)
        .and_then(|c| by_slug(c.slug))
        .map(|p| p.build(ctx))
        .unwrap_or_else(|| {
            prim(
                cuboid(
                    [0.6, 0.3, 1.6],
                    ctx.materials.body(ctx.palette.secondary_accent),
                ),
                [0.0, 0.0, 0.0],
                id_quat(),
            )
        });
    let scale = root.transform.scale.0;
    debug_assert!(
        scale == [1.0; 3],
        "structural root for {base:?} built a root scale {scale:?} that its \
         mounted children would inherit — shape a child leaf instead (root-scale \
         discipline; see the defaults module docstring)"
    );
    root
}
