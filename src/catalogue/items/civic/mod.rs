//! Cross-theme socio-political prop kit — small scatter clutter that
//! belongs to a room's *socio-political tier* rather than its theme.
//!
//! Unlike the per-theme structure kits (`ancient`, `cyberpunk`, …), these
//! props read in any setting: a barricade, a fountain, a barrel fire, a
//! park bench say something about a settlement's prosperity or its state
//! of conflict regardless of whether it's medieval or cyberpunk. They are
//! therefore tagged with **every** theme ([`all_themes`]) but with a
//! non-default socio-political band
//! ([`prosperity_band`](crate::catalogue::CatalogueEntry::prosperity_band) /
//! [`escalation_band`](crate::catalogue::CatalogueEntry::escalation_band)),
//! so the room-aware query
//! ([`entries_for_room`](crate::catalogue::entries_for_room)) only surfaces
//! them when the room's tier matches:
//!
//! | band | props |
//! |------|-------|
//! | prosperity **Poor** | shanty, scrap pile, laundry line, barrel fire |
//! | prosperity **Rich** | fountain, statue, banner, planter |
//! | escalation **Conflict** | barricade, sandbag wall, wreckage, watch post |
//! | escalation **Calm** | market stall, bench, lantern, garden bed |
//!
//! All are primitive-built (see [`super::util`]). They are deliberately
//! small-footprint so the settlement deriver can append several without
//! crowding the themed structures.

// Prosperity Poor.
pub mod barrel_fire;
pub mod laundry_line;
pub mod scrap_pile;
pub mod shanty;
// Prosperity Rich.
pub mod banner;
pub mod fountain;
pub mod planter;
pub mod statue;
// Escalation Conflict.
pub mod barricade;
pub mod sandbag_wall;
pub mod watch_post;
pub mod wreckage;
// Escalation Calm.
pub mod bench;
pub mod garden_bed;
pub mod lantern;
pub mod market_stall;

use crate::pds::{Fp, Fp3, SovereignMaterialSettings, SovereignTextureConfig};
use crate::seeded_defaults::ThemeArchetype;

/// Rebase-and-parent helper shared with the other primitive-built kits —
/// see [`super::util::assemble`]. Re-exported so this module's props keep
/// calling `super::assemble`.
pub(super) use super::util::assemble;

/// Every theme — the cross-theme props belong to a tier, not a theme, so
/// they advertise membership in all of them. Returned as the shared
/// [`CatalogueEntry::themes`](crate::catalogue::CatalogueEntry::themes)
/// value for every entry in this module.
pub(super) fn all_themes() -> &'static [ThemeArchetype] {
    &ThemeArchetype::ALL
}

/// Rotation around Z — leans planks and crossed barricade beams sideways.
/// (`super::util` only ships X and Y rotations.)
pub(super) fn quat_z(angle_rad: f32) -> crate::pds::Fp4 {
    let half = angle_rad * 0.5;
    crate::pds::Fp4([0.0, 0.0, half.sin(), half.cos()])
}

// ---------------------------------------------------------------------------
// Shared materials. Civic props lean on a small palette of unglamorous
// surfaces (weathered wood, canvas, rough stone, corroded metal) plus a few
// richer finishes for the prosperity-Rich set (marble, bronze, gilt). The
// socio-finish pass (crate::pds::material_finish) then grades these by the
// room's tiers, so e.g. a barrel fire in a calmer room reads less scorched.
// ---------------------------------------------------------------------------

/// Weathered structural timber — beams, posts, planks.
pub(super) fn wood(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::None,
        ..Default::default()
    }
}

/// Cloth — awnings, banners, hanging laundry, sandbag burlap.
pub(super) fn cloth(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// Rough cut / cast stone — kerbs, plinths, barricade fill.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        ..Default::default()
    }
}

/// Polished marble — the prosperity-Rich basin / plinth finish.
pub(super) fn marble(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.22),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// Corroded / dull metal — scrap, oil drums, tin sheeting.
pub(super) fn rust_metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.55),
        ..Default::default()
    }
}

/// Burnished metal — bronze statuary, gilt finials.
pub(super) fn bronze(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.9),
        ..Default::default()
    }
}

/// Matte foliage — hedges, flower-bed leaves, planter greenery.
pub(super) fn foliage(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

// Shared colours.
pub(super) const WOOD: [f32; 3] = [0.42, 0.27, 0.14];
pub(super) const WOOD_GREY: [f32; 3] = [0.39, 0.36, 0.31];
pub(super) const RUST: [f32; 3] = [0.43, 0.22, 0.12];
pub(super) const SCRAP: [f32; 3] = [0.34, 0.35, 0.37];
pub(super) const TIN: [f32; 3] = [0.55, 0.56, 0.58];
pub(super) const SANDBAG: [f32; 3] = [0.62, 0.55, 0.38];
pub(super) const CANVAS_RED: [f32; 3] = [0.62, 0.18, 0.16];
pub(super) const CANVAS_CREAM: [f32; 3] = [0.85, 0.80, 0.68];
pub(super) const MARBLE: [f32; 3] = [0.88, 0.87, 0.84];
pub(super) const STONE: [f32; 3] = [0.6, 0.58, 0.53];
pub(super) const BRONZE: [f32; 3] = [0.46, 0.32, 0.16];
pub(super) const GOLD: [f32; 3] = [0.83, 0.66, 0.22];
pub(super) const FOLIAGE_GREEN: [f32; 3] = [0.18, 0.42, 0.16];
pub(super) const WATER_BLUE: [f32; 3] = [0.30, 0.62, 0.82];
pub(super) const FIRE: [f32; 3] = [1.0, 0.5, 0.12];
pub(super) const LANTERN_WARM: [f32; 3] = [1.0, 0.85, 0.55];

#[cfg(test)]
mod tests {
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::catalogue::{CatalogueEntry, ENTRIES, StructureRole, entries_for_room};
    use crate::seeded_defaults::{
        EscalationBand, EscalationTier, ProsperityBand, ProsperityTier, ThemeArchetype,
    };

    /// The civic props are the cross-theme entries — the only ones tagged
    /// with *every* theme (the per-theme kits list one to a few). Collected
    /// from [`ENTRIES`] so this stays in sync without a hand-maintained
    /// list, and without catching theme kits that also tag a socio band.
    fn civic_entries() -> Vec<&'static dyn CatalogueEntry> {
        ENTRIES
            .iter()
            .copied()
            .filter(|e| e.themes().len() == ThemeArchetype::ALL.len())
            .collect()
    }

    #[test]
    fn there_are_sixteen_civic_props() {
        assert_eq!(
            civic_entries().len(),
            16,
            "expected the 4×4 cross-theme prop set"
        );
    }

    #[test]
    fn every_civic_prop_is_a_cross_theme_prop() {
        for e in civic_entries() {
            assert_eq!(
                e.role(),
                StructureRole::Prop,
                "{} should be a Prop",
                e.slug()
            );
            assert_eq!(
                e.themes().len(),
                ThemeArchetype::ALL.len(),
                "{} should be tagged with every theme (cross-theme)",
                e.slug()
            );
        }
    }

    #[test]
    fn each_band_set_has_four_props() {
        let count = |pred: &dyn Fn(&&'static dyn CatalogueEntry) -> bool| {
            civic_entries().iter().filter(|e| pred(e)).count()
        };
        assert_eq!(
            count(&|e| e.prosperity_band() == ProsperityBand::only(ProsperityTier::Poor)),
            4
        );
        assert_eq!(
            count(&|e| e.prosperity_band() == ProsperityBand::only(ProsperityTier::Rich)),
            4
        );
        assert_eq!(
            count(&|e| e.escalation_band() == EscalationBand::only(EscalationTier::Conflict)),
            4
        );
        assert_eq!(
            count(&|e| e.escalation_band() == EscalationBand::only(EscalationTier::Calm)),
            4
        );
    }

    #[test]
    fn poor_room_surfaces_poor_props_and_hides_rich() {
        // Cross-theme props are tagged with every theme, so any theme's
        // Prop query in a Poor/Calm room must include the poor set and
        // exclude the rich set.
        let slugs: Vec<&str> = entries_for_room(
            ThemeArchetype::AncientClassical,
            StructureRole::Prop,
            ProsperityTier::Poor,
            EscalationTier::Calm,
        )
        .map(|e| e.slug())
        .collect();
        for poor in ["shanty", "scrap_pile", "laundry_line", "barrel_fire"] {
            assert!(slugs.contains(&poor), "poor room missing {poor}");
        }
        for rich in ["fountain", "statue", "banner", "planter"] {
            assert!(!slugs.contains(&rich), "poor room wrongly offers {rich}");
        }
    }

    #[test]
    fn conflict_room_surfaces_conflict_props() {
        let slugs: Vec<&str> = entries_for_room(
            ThemeArchetype::Medieval,
            StructureRole::Prop,
            ProsperityTier::Modest,
            EscalationTier::Conflict,
        )
        .map(|e| e.slug())
        .collect();
        for c in ["barricade", "sandbag_wall", "wreckage", "watch_post"] {
            assert!(slugs.contains(&c), "conflict room missing {c}");
        }
        // Calm-only props must not appear in a conflict room.
        for calm in ["market_stall", "bench", "lantern", "garden_bed"] {
            assert!(
                !slugs.contains(&calm),
                "conflict room wrongly offers {calm}"
            );
        }
    }

    #[test]
    fn civic_props_round_trip_through_sanitize() {
        for e in civic_entries() {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }
}
