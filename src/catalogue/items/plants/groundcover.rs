//! Ground-cover props (#911) — the cheap scatter tier below the trees.
//!
//! Every entry here is a handful of primitives, not an L-system: the
//! seeded ground-cover scatter places these by the hundred, so per-instance
//! entity cost is the binding constraint. Two shapes cover the whole tier:
//!
//! * **Crossed cards** — two alpha-masked quads at 90°, each carrying one of
//!   the WS1 vegetation textures. Crossing them means the prop reads from
//!   every yaw instead of vanishing edge-on, which a single card does.
//! * **Cushion mounds** — one squashed sphere centred on the ground plane,
//!   for the encrusting cover (moss, lichen). These carry opaque *surface*
//!   textures rather than alpha cards, so a flat quad would read as a square
//!   stamped on the terrain; a buried-hemisphere dome has no such edge.
//!
//! ## Card geometry
//!
//! A `Plane` is horizontal by default, so a standing card is the root rotated
//! `+90°` about X. Children inherit the root's rotation, so the crossing card
//! cannot simply be given a world-space `Ry(90)` — its *local* rotation must
//! be `Rx(-90)·Ry(90)·Rx(90)`, which reduces to `Rz(-90)`. Getting this wrong
//! splays the second card flat instead of crossing it (see the rotated-root
//! trap in the catalogue notes).
//!
//! ## Entity cost
//!
//! | Prop | Entities |
//! |---|---|
//! | grass tuft, fern, reed, dwarf shrub | 2 |
//! | wildflower | 4 |
//! | moss, lichen | 1 |

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{prim, prim_scaled, quat_x, quat_z, sphere};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::pds::{
    Fp, Fp2, Fp3, Fp4, Generator, GeneratorKind, SovereignBroadleafConfig, SovereignFlowerConfig,
    SovereignFrondConfig, SovereignGrassTuftConfig, SovereignLichenConfig,
    SovereignMaterialSettings, SovereignMossConfig, SovereignReedConfig, SovereignTextureConfig,
    TortureParams,
};

/// A single quad carrying `material`, `size` metres, at `translation` with
/// `rotation`.
fn quad(
    size: [f32; 2],
    translation: [f32; 3],
    rotation: Fp4,
    material: SovereignMaterialSettings,
) -> Generator {
    prim(
        GeneratorKind::Plane {
            size: Fp2(size),
            subdivisions: 0,
            // Ground cover is never collidable: you walk through grass, and a
            // collider per card would cost a physics body per instance on a
            // tier placed by the hundred.
            solid: false,
            material,
            torture: TortureParams::default(),
        },
        translation,
        rotation,
    )
}

/// Two standing quads crossed at 90°, rooted on the ground plane.
///
/// `width` / `height` are the card's metre extents; the pair is centred on the
/// origin in XZ with its base at `y = 0`.
fn crossed_cards(width: f32, height: f32, material: SovereignMaterialSettings) -> Generator {
    // Root: stood upright, lifted so its base meets the ground.
    let mut root = quad(
        [width, height],
        [0.0, height * 0.5, 0.0],
        quat_x(FRAC_PI_2),
        material.clone(),
    );
    // Crossing card: local rotation only, since it inherits the root's.
    root.children.push(quad(
        [width, height],
        [0.0, 0.0, 0.0],
        quat_z(-FRAC_PI_2),
        material,
    ));
    root
}

/// A low cushion mound for the encrusting covers.
///
/// A flat quad would be simpler, but moss and lichen carry *opaque* surface
/// textures rather than alpha cards, so a quad reads as an unmistakable square
/// patch stamped on the terrain. A squashed sphere centred on the ground plane
/// shows only its dome — no silhouette edge to give the trick away, and no
/// coplanar z-fight, since nothing is flush with the surface.
fn cushion(radius: f32, height: f32, material: SovereignMaterialSettings) -> Generator {
    prim_scaled(
        sphere(radius, 5, material),
        // Centred on the ground: the lower hemisphere is buried, leaving a
        // cushion sitting proud of the surface.
        [0.0, 0.0, 0.0],
        Fp4([0.0, 0.0, 0.0, 1.0]),
        [1.0, (height / radius).max(0.05), 1.0],
    )
}

/// Matte card material — ground cover is never glossy, and `base_color` stays
/// near white so the generator's own palette shows through unmodulated.
fn card_material(texture: SovereignTextureConfig) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.9),
        metallic: Fp(0.0),
        uv_scale: Fp(1.0),
        texture,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// Declare a ground-cover entry: slug, display name, description, and the
/// closure building its generator. Every entry is `StructureRole::Plant`.
macro_rules! ground_cover_entry {
    ($ty:ident, $slug:literal, $name:literal, $desc:literal, $build:expr) => {
        pub struct $ty;

        impl CatalogueEntry for $ty {
            fn slug(&self) -> &'static str {
                $slug
            }
            fn name(&self) -> &'static str {
                $name
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn role(&self) -> StructureRole {
                StructureRole::Plant
            }
            fn build(&self, _local_did: &str) -> Generator {
                let f: fn() -> Generator = $build;
                f()
            }
        }
    };
}

ground_cover_entry!(
    GrassTuft,
    "gc_grass_tuft",
    "Grass Tuft",
    "A crossed-card clump of grass blades — the ground-cover workhorse.",
    || crossed_cards(
        0.55,
        0.45,
        card_material(SovereignTextureConfig::GrassTuft(
            SovereignGrassTuftConfig::default()
        ))
    )
);

ground_cover_entry!(
    DryGrassTuft,
    "gc_dry_grass_tuft",
    "Dry Grass Tuft",
    "Sun-bleached grass clump — savanna, badlands and arid ground cover.",
    || crossed_cards(
        0.6,
        0.4,
        card_material(SovereignTextureConfig::GrassTuft(
            SovereignGrassTuftConfig {
                color_base: Fp3([0.20, 0.17, 0.07]),
                color_tip: Fp3([0.47, 0.40, 0.15]),
                color_dry: Fp3([0.52, 0.44, 0.18]),
                dry_fraction: crate::pds::Fp64(0.7),
                ..Default::default()
            }
        ))
    )
);

ground_cover_entry!(
    Wildflower,
    "gc_wildflower",
    "Wildflower Clump",
    "Grass tuft topped with a blossom — meadow and verge colour.",
    || {
        let mut root = crossed_cards(
            0.45,
            0.4,
            card_material(SovereignTextureConfig::GrassTuft(
                SovereignGrassTuftConfig {
                    color_tip: Fp3([0.34, 0.48, 0.15]),
                    dry_fraction: crate::pds::Fp64(0.1),
                    ..Default::default()
                },
            )),
        );
        // One blossom card seated among the blades. Its local rotation is
        // relative to the already-rotated root, same as the crossing card;
        // local -Z maps to world +Y through the root's Rx(90), so this sits
        // the flower just below the blade tips rather than floating above.
        let blossom = card_material(SovereignTextureConfig::Flower(SovereignFlowerConfig {
            // One blossom per card: the atlas default bakes a 2x2 grid, which
            // on a single quad reads as four floating flowers.
            variant_rows: 1,
            variant_cols: 1,
            ..Default::default()
        }));
        // Crossed like the tuft: a lone blossom card disappears edge-on, and
        // with random per-instance yaw that means half a meadow's flowers
        // wink out as the camera turns.
        root.children.push(quad(
            [0.2, 0.2],
            [0.0, 0.0, -0.12],
            quat_z(-FRAC_PI_2),
            blossom.clone(),
        ));
        root.children.push(quad(
            [0.2, 0.2],
            [0.0, 0.0, -0.12],
            Fp4([0.0, 0.0, 0.0, 1.0]),
            blossom,
        ));
        root
    }
);

ground_cover_entry!(
    FernClump,
    "gc_fern_clump",
    "Fern Clump",
    "Low frond rosette — forest-floor and jungle understory cover.",
    || crossed_cards(
        0.8,
        0.6,
        card_material(SovereignTextureConfig::Frond(SovereignFrondConfig {
            width: crate::pds::Fp64(0.16),
            lobe_count: crate::pds::Fp64(5.0),
            lobe_depth: crate::pds::Fp64(0.4),
            ..Default::default()
        }))
    )
);

ground_cover_entry!(
    ReedClump,
    "gc_reed_clump",
    "Reed Clump",
    "Tall shoreline reeds with cattail heads — wetland and pond margins.",
    || crossed_cards(
        0.7,
        1.5,
        card_material(SovereignTextureConfig::Reed(SovereignReedConfig::default()))
    )
);

ground_cover_entry!(
    DwarfShrub,
    "gc_dwarf_shrub",
    "Dwarf Shrub",
    "Low woody cushion — tundra and alpine ground cover.",
    || crossed_cards(
        0.5,
        0.35,
        card_material(SovereignTextureConfig::Broadleaf(
            SovereignBroadleafConfig {
                color_base: Fp3([0.12, 0.18, 0.07]),
                color_edge: Fp3([0.28, 0.24, 0.10]),
                lobe_count: crate::pds::Fp64(3.0),
                radius: crate::pds::Fp64(0.8),
                ..Default::default()
            }
        ))
    )
);

ground_cover_entry!(
    MossPatch,
    "gc_moss_patch",
    "Moss Patch",
    "Velvet moss cushion — damp forest and boreal floors.",
    || cushion(
        0.7,
        0.16,
        card_material(SovereignTextureConfig::Moss(SovereignMossConfig::default()))
    )
);

ground_cover_entry!(
    LichenPatch,
    "gc_lichen_patch",
    "Lichen Patch",
    "Crustose lichen crust over stone — tundra ground cover.",
    || cushion(
        0.55,
        0.09,
        card_material(SovereignTextureConfig::Lichen(
            SovereignLichenConfig::default()
        ))
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    /// Every ground-cover entry, for the shared invariants below.
    fn all() -> Vec<&'static dyn CatalogueEntry> {
        vec![
            &GrassTuft,
            &DryGrassTuft,
            &Wildflower,
            &FernClump,
            &ReedClump,
            &DwarfShrub,
            &MossPatch,
            &LichenPatch,
        ]
    }

    #[test]
    fn entries_round_trip_through_sanitize() {
        for e in all() {
            assert_sanitize_stable(&e.build(""), e.slug());
        }
    }

    /// The whole point of this tier is that it is cheap. A ground-cover prop
    /// that grew a deep tree would blow the scatter's entity budget.
    #[test]
    fn entity_cost_stays_tiny() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        for e in all() {
            let n = count(&e.build(""));
            assert!(
                n <= 4,
                "{} costs {n} entities; ground cover must stay <= 4",
                e.slug()
            );
        }
    }

    /// Cards must stand on the ground, not float or sink: the root's base sits
    /// at y = 0 (cards) or just above it (decals).
    #[test]
    fn props_sit_on_the_ground() {
        for e in all() {
            let g = e.build("");
            let y = g.transform.translation.0[1];
            assert!(
                y >= 0.0 && y < 1.0,
                "{} root y = {y}, expected to rest on the ground plane",
                e.slug()
            );
        }
    }

    /// The encrusting covers are squashed domes centred on the ground plane,
    /// not flat quads — a flat quad carrying an opaque surface texture reads
    /// as a square stamped on the terrain, and would z-fight if laid flush.
    #[test]
    fn cushions_are_squashed_domes_on_the_ground_plane() {
        for e in [&MossPatch as &dyn CatalogueEntry, &LichenPatch] {
            let g = e.build("");
            assert_eq!(
                g.transform.translation.0[1],
                0.0,
                "{} should be centred on the ground plane",
                e.slug()
            );
            let s = g.transform.scale.0;
            assert!(
                s[1] < s[0] && s[1] < s[2],
                "{} should be squashed flat (scale {s:?})",
                e.slug()
            );
        }
    }
}
