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

// Shared emitter recipes (fire, smoke) the props hang their FX on.
mod fx;

// Cross-theme fallback gateway (a Gateway, not a socio-political prop).
pub mod gateway;
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

use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignBrickConfig, SovereignCobblestoneConfig,
    SovereignCorrugatedConfig, SovereignFabricConfig, SovereignMarbleConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignPlankConfig, SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;
use bevy_symbios_texture::metal::MetalStyle;

use super::util::{cuboid_tapered, cylinder_tapered, id_quat, prim, solid, sphere};

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

/// Weathered structural timber — beams, posts, planks. A sawn-plank grain
/// with a few knots so a post reads as wood, not a flat-painted dowel.
pub(super) fn wood(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.88),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
            color_wood_light: Fp3([color[0] * 1.22, color[1] * 1.22, color[2] * 1.22]),
            color_wood_dark: Fp3([color[0] * 0.58, color[1] * 0.58, color[2] * 0.58]),
            plank_count: Fp64(4.0),
            knot_density: Fp64(0.25),
            grain_warp: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Cloth — awnings, banners, hanging laundry, sandbag burlap. A woven weave
/// with a darker weft so a banner reads as fabric, not a coloured slab.
pub(super) fn cloth(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.95),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            color_warp: Fp3(color),
            color_weft: Fp3([color[0] * 0.72, color[1] * 0.72, color[2] * 0.72]),
            thread_count: Fp64(18.0),
            fuzz: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Rough cut / cast stone — kerbs, plinths, barricade fill. Mud-set
/// fieldstone cobble.
pub(super) fn stone(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(2.0),
        texture: SovereignTextureConfig::Cobblestone(SovereignCobblestoneConfig {
            color_stone: Fp3(color),
            color_mud: Fp3([color[0] * 0.5, color[1] * 0.48, color[2] * 0.42]),
            roundness: Fp64(1.2),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Fired brick — the collapsed wall fragments of the conflict set.
pub(super) fn brick(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.92),
        metallic: Fp(0.0),
        uv_scale: Fp(1.5),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            color_brick: Fp3(color),
            aspect_ratio: Fp64(3.0),
            scale: Fp64(9.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Corrugated sheet — shanty roofs, leaning scrap panels. Ridged + rusted.
pub(super) fn corrugated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.45),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            ridges: Fp64(9.0),
            rust_level: Fp64(0.4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Polished marble — the prosperity-Rich basin / plinth finish, veined.
pub(super) fn marble(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.22),
        metallic: Fp(0.0),
        texture: SovereignTextureConfig::Marble(SovereignMarbleConfig {
            color_base: Fp3(color),
            color_vein: Fp3([color[0] * 0.6, color[1] * 0.58, color[2] * 0.56]),
            vein_frequency: Fp64(3.0),
            scale: Fp64(2.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Corroded / dull metal — scrap, oil drums, tin sheeting. Brushed + rust.
pub(super) fn rust_metal(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.85),
        metallic: Fp(0.55),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([0.32, 0.18, 0.10]),
            roughness: Fp64(0.85),
            metallic: Fp(0.55),
            rust_level: Fp64(0.45),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Burnished metal — bronze statuary, gilt finials, cast-iron frames.
/// Brushed and barely tarnished so it reads as worked metal.
pub(super) fn bronze(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.4),
        metallic: Fp(0.9),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3([color[0] * 0.5, color[1] * 0.45, color[2] * 0.3]),
            roughness: Fp64(0.4),
            metallic: Fp(0.9),
            rust_level: Fp64(0.08),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Matte foliage — hedges, flower-bed leaves, planter greenery. Left
/// flat-colour: a tiling surface texture only muddies a small leaf clump.
pub(super) fn foliage(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// A standing commemorative figure — a robed body with a yoke of shoulders,
/// a head and two arms (one raised in oratory) — built in the prop's world
/// frame from `base_y` (the top of its plinth) upward in `bronze(color)`.
/// Returned as a list of pieces for an [`assemble`] caller to append *after*
/// a flat root, so the figure's tilted arms never become the rotated assemble
/// root. `fz` is the sign of the forward axis (`-1.0` puts the gaze and the
/// raised arm toward the render front, `-Z`).
pub(super) fn figure_parts(base_y: f32, fz: f32, color: [f32; 3]) -> Vec<Generator> {
    vec![
        // Flared robe / lower body.
        prim(
            solid(cylinder_tapered(0.30, 0.82, 12, 0.22, bronze(color))),
            [0.0, base_y + 0.41, 0.0],
            id_quat(),
        ),
        // Torso narrowing toward the shoulders.
        prim(
            solid(cylinder_tapered(0.23, 0.5, 12, 0.18, bronze(color))),
            [0.0, base_y + 0.95, 0.0],
            id_quat(),
        ),
        // Shoulder yoke.
        prim(
            cuboid_tapered([0.5, 0.17, 0.24], 0.0, bronze(color)),
            [0.0, base_y + 1.22, 0.0],
            id_quat(),
        ),
        // Neck.
        prim(
            cylinder_tapered(0.07, 0.12, 8, 0.0, bronze(color)),
            [0.0, base_y + 1.33, 0.0],
            id_quat(),
        ),
        // Head, gaze toward the front.
        prim(
            sphere(0.15, 3, bronze(color)),
            [0.0, base_y + 1.5, fz * 0.03],
            id_quat(),
        ),
        // Raised arm (oratory) — reaches up and toward the front.
        prim(
            cylinder_tapered(0.06, 0.62, 8, 0.0, bronze(color)),
            [0.28, base_y + 1.42, fz * 0.12],
            quat_z(-1.05),
        ),
        // Resting arm at the side.
        prim(
            cylinder_tapered(0.06, 0.56, 8, 0.0, bronze(color)),
            [-0.26, base_y + 1.02, fz * 0.05],
            quat_z(0.22),
        ),
    ]
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
/// Deep-saturated firelight — a broad cone at high strength blooms near-white,
/// so the colour carries the heat and the strength stays moderate.
pub(super) const FIRE: [f32; 3] = [1.0, 0.42, 0.08];
/// Hot ember core, deeper still than [`FIRE`].
pub(super) const EMBER: [f32; 3] = [1.0, 0.24, 0.05];
/// Warm lamplight — deep amber so a lit housing reads incandescent rather
/// than washing to a pale near-white box.
pub(super) const LANTERN_WARM: [f32; 3] = [1.0, 0.74, 0.36];

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
