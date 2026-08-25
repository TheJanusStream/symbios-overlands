//! Wearable catalogue items — the **Attachments** category (#1086/#1087).
//!
//! Every entry here is a normal catalogue item that *additionally* declares
//! a [`wear_socket`](crate::catalogue::CatalogueEntry::wear_socket): the
//! catalogue's detail panel offers **Copy to inventory & wear** beside the
//! usual drag-to-place (#1096 — the inventory is the wear surface; the
//! copy carries the entry's socket and fit as
//! [`WearMeta`](crate::pds::inventory::WearMeta), and wearing it writes an
//! [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) at that
//! socket with the item's name as provenance). Attachment-ness is
//! overlands-only metadata — the avatar engine stays attachment-agnostic
//! (owner decision, 2026-08-23), and every entry remains placeable in the
//! world like any other inventory item.
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

pub mod banner;
pub mod circlet;
pub mod lantern;
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

/// Aged ship's iron — dark, worn, still metal.
pub(super) fn aged_iron() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.14, 0.14, 0.16]),
        roughness: Fp(0.55),
        metallic: Fp(0.85),
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

#[cfg(test)]
mod budget_tests {
    /// Per-item ceiling on a worn record's serialized size (#1092).
    ///
    /// An [`AttachmentRecord`](crate::pds::avatar::AttachmentRecord) is its own
    /// PDS record and the publish preflight holds each to the 100 KiB record
    /// budget — but the WARDROBE is 16 slots, and the aggregate is what the
    /// resolution fan-out fetches, the resolved rig clones, and every peer
    /// pays. 6,000 bytes a slot keeps a full 16-slot loadout (96,000 B) under
    /// one record budget *in total*, so no future path that carries an outfit
    /// in one place — a bundle, a cache, a gift crate — inherits a payload
    /// problem. Measured when set: satchel 1,100 B, circlet 1,418, sashimono
    /// 1,435, ships_lantern 4,061 (the audio graph is most of it).
    const MAX_WORN_RECORD_BYTES: usize = 6_000;

    /// Per-item triangle ceiling for a wearable's built tree (#1092).
    ///
    /// The avatar's own binding corner is dearest-head × greediest-hair, and
    /// the engine's `tests/budget.rs` ratchets that corner against a 30,000
    /// target. 1,875 a slot means a FULL 16-slot loadout (30,000) can at most
    /// match the dearest body ever built — attachments may double an avatar,
    /// never dominate it. A wearable is a garnish, not a building; raising
    /// this is a deliberate act with the corner arithmetic redone, exactly
    /// like the engine's own ratchet. Measured when set: sashimono 380,
    /// satchel 868, circlet 1,372, ships_lantern 1,600.
    const MAX_WORN_TRIANGLES: u32 = 1_875;

    use crate::catalogue::StructureRole;
    use crate::pds::avatar::wardrobe::AttachmentRecord;
    use crate::pds::{Generator, GeneratorKind};

    /// Triangles the built tree costs a renderer, counted from the REAL
    /// meshers ([`crate::world_builder::build_primitive_mesh`]) rather
    /// than estimated — the resolution knobs the meshers read are exactly
    /// what an author tunes. A particle system is billboard quads at its
    /// worst-case alive count. Any other kind is a hard failure on
    /// purpose: a future blob-built or L-system wearable must teach this
    /// counter its cost model BEFORE it can ship, not slide past at zero.
    fn triangles(node: &Generator) -> u32 {
        let own = match &node.kind {
            GeneratorKind::ParticleSystem(params) => params.max_particles * 2,
            kind if crate::catalogue::items::measure::is_primitive(kind) => {
                crate::world_builder::build_primitive_mesh(kind)
                    .mesh
                    .indices()
                    .map(|i| (i.len() / 3) as u32)
                    .unwrap_or(0)
            }
            other => panic!(
                "wearable carries a kind this budget cannot count: {other:?} — \
                 teach `triangles` its cost before shipping it"
            ),
        };
        own + node.children.iter().map(triangles).sum::<u32>()
    }

    /// The #1092 guard: every registered wearable, worn exactly as the
    /// Wear button records it, fits both per-slot budgets — so a full
    /// 16-slot loadout can breach neither the 100 KiB record budget in
    /// aggregate nor the engine's dearest-head × greediest-hair triangle
    /// corner (16 × the caps = 96,000 B and 30,000 triangles; the
    /// constants' docs carry the arithmetic).
    #[test]
    fn every_wearable_fits_the_sixteen_slot_budgets() {
        let mut checked = 0;
        for entry in crate::catalogue::ENTRIES
            .iter()
            .filter(|e| e.role() == StructureRole::Attachment)
        {
            let socket = entry
                .wear_socket()
                .expect("the registry guard pairs Attachment with wear_socket");
            let mut record = AttachmentRecord::with_fit(
                entry.build("did:budget:guard"),
                socket,
                entry.wear_fit(),
            );
            record.sanitize();

            let bytes = serde_json::to_vec(&record).expect("serializes").len();
            assert!(
                bytes <= MAX_WORN_RECORD_BYTES,
                "{}: worn record is {bytes} bytes, over the {MAX_WORN_RECORD_BYTES} per-slot \
                 budget — 16 such slots would breach the aggregate record budget",
                entry.slug()
            );

            let tris = triangles(&record.item);
            assert!(
                tris <= MAX_WORN_TRIANGLES,
                "{}: builds {tris} triangles, over the {MAX_WORN_TRIANGLES} per-slot budget — \
                 16 such slots would outweigh the dearest body's own 30,000 corner",
                entry.slug()
            );
            checked += 1;
        }
        assert!(checked >= 4, "the guard walked only {checked} wearables");
    }
}
