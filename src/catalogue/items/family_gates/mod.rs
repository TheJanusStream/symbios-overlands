//! Family gates (#1796) - cross-theme social gateways that a few kindred
//! themes share, where `civic_gateway` is the one every theme may fall back
//! on.
//!
//! A seeded world stands its own theme's gate two times in three and a
//! kindred one the rest (`seeded_defaults::room::gateway`, `gateway_pool`).
//! Each gate here reads right in a small family of themes - a stone quay
//! arch on a pirate or a resort coast, a timber palisade in a Norse or a
//! medieval world, a ring of light at an outpost, a monolith field or a
//! neon city - and joins those themes' pools through `kindred_gates`.
//!
//! So every gate here leaves `themes()` empty, as `civic_gateway` does: a
//! gate tagged with a theme would become that theme's own gate (the first
//! `entries_for(theme, Gateway)` returns) and displace the bespoke one. Each
//! is built the way the themed gates are - a frame of jambs, a lintel and a
//! threshold around one [`GeneratorKind::Gateway`](crate::pds::GeneratorKind::Gateway)
//! veil fitted to its opening (`catalogue::items::gateway_fit`), the hero
//! face on `-Z` - and borrows the palette of the themes it serves.

pub mod bone_arch;
pub mod corbel_arch;
pub mod garden_arch;
pub mod harbour;
pub mod light_ring;
pub mod palisade;
pub mod works;
pub mod yakuimon;

use crate::catalogue::Footprint;

/// The footprint every family gate keeps, the themed gates' own: the
/// forecourt a settlement holds clear round it, and its standoff from the
/// spawn point.
pub(super) fn gate_footprint() -> Footprint {
    Footprint {
        clearance: 3.5,
        min_spawn_dist: 8.0,
    }
}

/// The checks every family gate passes: it is a themeless gate carrying one
/// fitted veil, it survives the sanitiser, nothing of it floats or draws a
/// face where another does, and its record stays small beside the
/// settlement it stands in front of.
#[cfg(test)]
pub(super) fn assert_family_gate(entry: &dyn crate::catalogue::CatalogueEntry) {
    use crate::catalogue::StructureRole;
    use crate::pds::{Generator, GeneratorKind};

    let slug = entry.slug();
    let built = entry.build("did:plc:familygate");
    assert_eq!(entry.role(), StructureRole::Gateway, "{slug}");
    assert!(
        entry.themes().is_empty(),
        "{slug}: a family gate is themeless, or it displaces a theme's own"
    );
    super::util::assert_sanitize_stable(&built, slug);
    fn zones(node: &Generator) -> usize {
        usize::from(matches!(node.kind, GeneratorKind::Gateway { .. }))
            + node.children.iter().map(zones).sum::<usize>()
    }
    assert_eq!(zones(&built), 1, "{slug}: one walk-in zone");
    let geo = super::gateway_fit::measure(&built).expect("a veil");
    let faults = super::gateway_fit::fit_faults(&geo);
    assert!(faults.is_empty(), "{slug}: {faults:#?}");
    super::overhaul::assert_no_z_fighting(&built, slug);
    super::overhaul::assert_nothing_floats(&built, slug);
    let bytes = serde_json::to_vec(&built).expect("serialises").len();
    assert!(bytes < 40_000, "{slug} serialises to {bytes} B");
}
