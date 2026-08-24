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
//! * **Body scale, attach origin, face on `+Z`.** Author the tree at real
//!   worn size around the point that should meet the body, with the side
//!   meant to be seen on `+Z`: the record's offset stays identity, which
//!   is the sentinel for "seat it against the measured surface and yaw
//!   the `+Z` face out of the body" (`src/player/attachments.rs`,
//!   `outward_yaw`). Children inherit the root transform — assemble
//!   around the origin, never around a world-space stand.
//! * **A fitted band circles the origin** (#1089). An entry declaring
//!   [`WearFit::HeadBand`](crate::catalogue::WearFit) authors its ring in
//!   the X–Z plane at `y = 0`: the fitted seat puts the origin on the
//!   head's axis at the measured hat line and scales the subtree to the
//!   wearer, so the authored geometry needs no per-body drop guessing.
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

pub mod circlet;
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

/// Polished gold — bands, filigree, regalia.
pub(super) fn gold() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.85, 0.66, 0.23]),
        roughness: Fp(0.22),
        metallic: Fp(0.95),
        uv_scale: Fp(1.0),
        ..Default::default()
    }
}

/// A cut stone with a faint inner light — enough glow to read as a gem at
/// avatar distance, far under the fx kit's lamp strengths.
pub(super) fn gemstone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        emission_color: Fp3(color),
        emission_strength: Fp(0.35),
        roughness: Fp(0.06),
        metallic: Fp(0.15),
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
