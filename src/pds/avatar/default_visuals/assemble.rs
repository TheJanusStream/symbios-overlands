//! Shared assembler helpers that bridge the seeded [`AvatarOutfit`] and the
//! [`crate::pds::avatar::parts`] catalogue.
//!
//! Kept separate from [`super::common`] (which is pure primitive geometry,
//! reused by the parts themselves) because these helpers depend on the
//! outfit + part registry — the assembly layer, not the geometry layer.

use crate::pds::avatar::parts::{PartCtx, PartSlot, by_slug};
use crate::pds::generator::Generator;
use crate::seeded_defaults::AvatarOutfit;

use super::common::{cuboid, id_quat, prim};

/// Build the part filling `base` as the family's structural root node (at the
/// origin). Falls back to a plain cuboid only if the base slot is somehow
/// unfilled — the universal default parts make that unreachable in practice.
pub(super) fn base_root(outfit: &AvatarOutfit, ctx: &PartCtx, base: PartSlot) -> Generator {
    outfit
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
        })
}
