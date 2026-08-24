//! Wearable catalogue items — the **Attachments** category (#1086/#1087).
//!
//! Every entry here is a normal catalogue item that *additionally* declares
//! a [`wear_socket`](crate::catalogue::CatalogueEntry::wear_socket): the
//! catalogue's detail panel offers **Wear** beside the usual drag-to-place,
//! and wearing writes the built generator into the wardrobe as an
//! [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) at that
//! socket. Attachment-ness is overlands-only metadata — the avatar engine
//! stays attachment-agnostic (owner decision, 2026-08-23), and every entry
//! remains placeable in the world like any other inventory item.
//!
//! # Authoring conventions
//!
//! * **Body scale, attach origin.** Author the tree at real worn size
//!   around the point that should meet the body: the record's offset stays
//!   identity, which is the sentinel for "let the engine seat it against
//!   the measured surface" (`src/player/attachments.rs`). Children inherit
//!   the root transform — assemble around the origin, never around a
//!   world-space stand.
//! * **Small budgets.** These ride inside the avatar record's byte budget
//!   and on a body whose triangle budget already has a known worst corner;
//!   a wearable is a garnish, not a building. Slice #1092 adds the guard
//!   tests; until then keep trees to a handful of prims.
//! * **Themeless is fine.** [`StructureRole::Attachment`] entries never
//!   enter the seeded settlement pools, so `themes()` may stay empty; a
//!   themed wearable is a tag for browsing, nothing more.
//!
//! [`StructureRole::Attachment`]: crate::catalogue::StructureRole::Attachment

use crate::pds::{Fp, Fp3, SovereignMaterialSettings};

pub mod satchel;

/// Worn dark-tan leather — pouch bodies, straps.
pub(super) fn leather(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.78),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

/// Dull brass — buckles and clasps.
pub(super) fn brass() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.62, 0.48, 0.22]),
        roughness: Fp(0.38),
        metallic: Fp(0.85),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}
