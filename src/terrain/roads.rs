//! Dedicated road-mesh rebuild system.
//!
//! Roads are a `RoadNetwork` child of the terrain generator in the record
//! (authored / seeded / edited as config — see
//! [`crate::pds::generator::RoadConfig`] and [`crate::urban`]). This system
//! owns the road *mesh*: it watches the finished heightmap and the live
//! record's road config and, on any change, re-meshes the draped road ribbon —
//! **reusing the existing heightmap, never regenerating the terrain**. So a
//! road edit (slider, re-roll, enable toggle) costs one road re-mesh, not a
//! full heightmap rebuild.
//!
//! The road is a standalone entity at the terrain's `-half` world offset: the
//! ribbon geometry is authored in the full heightmap's coordinate frame, the
//! same frame the terrain mesh child is spawned in.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;

use crate::seeded_defaults::{SceneCharacter, ThemeArchetype};
use crate::state::{CurrentRoomDid, LiveRoomRecord};

use super::FinishedHeightMap;

/// Marker for the spawned road mesh entity, so a rebuild can replace it.
#[derive(Component)]
pub(super) struct RoadMeshEntity;

/// Per-theme look for the three road surfaces. The road is a client-derived
/// visual layer (not stored), so its palette is keyed off the room's theme here
/// rather than serialized — cyberpunk gets hot neon, a modern city warm
/// streetlight LEDs, an industrial park hazard amber, residential streets a
/// faint painted curb line.
struct RoadPalette {
    /// Drivable deck base colour (wet-asphalt material).
    deck: Color,
    /// Curb + skirt + bottom base colour (concrete/metal material).
    structure: Color,
    /// Curb edge-line emissive, colour already scaled by strength.
    edge: LinearRgba,
    /// Whether the edge-line is unlit (a true neon tube) vs a lit painted line.
    edge_unlit: bool,
}

/// Emissive colour `rgb` scaled by `strength` — a thin tube runs hot (~6: a
/// white-hot core plus a coloured bloom halo), a painted line stays low (~1).
fn glow(rgb: [f32; 3], strength: f32) -> LinearRgba {
    LinearRgba::rgb(rgb[0] * strength, rgb[1] * strength, rgb[2] * strength)
}

/// The road look for `theme`. Road-growing themes each get a distinct identity;
/// any other theme that opts into roads via the editor falls back to a neutral
/// cool trim.
fn road_palette(theme: ThemeArchetype) -> RoadPalette {
    use ThemeArchetype::*;
    match theme {
        Cyberpunk => RoadPalette {
            deck: Color::srgb(0.015, 0.015, 0.02),
            structure: Color::srgb(0.05, 0.05, 0.06),
            edge: glow([0.10, 0.95, 1.00], 6.0), // hot cyan neon
            edge_unlit: true,
        },
        ModernCity => RoadPalette {
            deck: Color::srgb(0.02, 0.02, 0.024),
            structure: Color::srgb(0.10, 0.10, 0.11),
            edge: glow([1.00, 0.92, 0.70], 2.2), // warm LED streetlight
            edge_unlit: true,
        },
        IndustrialPark => RoadPalette {
            deck: Color::srgb(0.025, 0.024, 0.022),
            structure: Color::srgb(0.08, 0.075, 0.07),
            edge: glow([1.00, 0.62, 0.18], 3.2), // hazard amber
            edge_unlit: true,
        },
        Roadside => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.032),
            structure: Color::srgb(0.09, 0.09, 0.09),
            edge: glow([1.00, 0.85, 0.25], 1.4), // painted lane yellow
            edge_unlit: false,
        },
        CivicCampus => RoadPalette {
            deck: Color::srgb(0.10, 0.10, 0.105), // pale plaza paving
            structure: Color::srgb(0.16, 0.16, 0.17),
            edge: glow([0.90, 0.92, 1.00], 1.2), // cool soft trim
            edge_unlit: false,
        },
        Suburban => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.033),
            structure: Color::srgb(0.13, 0.13, 0.13),
            edge: glow([0.85, 0.85, 0.82], 0.8), // faint painted curb line
            edge_unlit: false,
        },
        SportsRec => RoadPalette {
            deck: Color::srgb(0.05, 0.04, 0.04),
            structure: Color::srgb(0.12, 0.12, 0.12),
            edge: glow([1.00, 0.95, 0.90], 1.0), // court / track line
            edge_unlit: false,
        },
        _ => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.035),
            structure: Color::srgb(0.09, 0.09, 0.10),
            edge: glow([0.60, 0.80, 1.00], 2.5),
            edge_unlit: true,
        },
    }
}

/// Fingerprint of the road config last meshed — `None` when no road mesh is
/// live (no config, disabled, or no terrain). Lets the rebuild skip work when
/// an unrelated record edit fires.
#[derive(Resource, Default)]
pub(super) struct RoadFingerprint(pub(super) Option<String>);

/// Re-mesh the road network when the heightmap or the road config changes,
/// reusing the existing heightmap (no terrain regeneration).
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource/query.
pub(super) fn maybe_rebuild_roads(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    did: Option<Res<CurrentRoomDid>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut fingerprint: ResMut<RoadFingerprint>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<RoadMeshEntity>>,
) {
    // No terrain (not yet generated, or torn down) → no roads. Sweep any
    // straggler once, then idle.
    let Some(heightmap) = heightmap else {
        if fingerprint.0.is_some() {
            for e in &existing {
                commands.entity(e).try_despawn();
            }
            fingerprint.0 = None;
        }
        return;
    };

    // Only consider work on frames where the terrain or the record changed.
    if !heightmap.is_changed() && !record.is_changed() {
        return;
    }

    let config = crate::pds::find_road_config(&record.0)
        .filter(|c| c.enabled)
        .cloned();
    let want = config.as_ref().and_then(|c| serde_json::to_string(c).ok());

    // A record edit that didn't touch the road config, on stable terrain, is a
    // no-op. A fresh heightmap (initial load / terrain regen) always re-meshes,
    // since the draped geometry depends on the new surface.
    if !heightmap.is_changed() && want == fingerprint.0 {
        return;
    }

    for e in &existing {
        commands.entity(e).try_despawn();
    }
    if let Some(config) = &config
        && let Some(parts) = crate::urban::build_road_geometry(&heightmap.0, config)
    {
        // The ribbon lives in the full heightmap frame; the terrain mesh child
        // is offset by -half, so the road shares that offset.
        let world_extent = (heightmap.0.width() - 1) as f32 * heightmap.0.scale();
        let half = world_extent * 0.5;
        let offset = Transform::from_xyz(-half, 0.0, -half);

        // The road look follows the room's theme (client-side; not stored).
        let theme = did.as_ref().map_or(ThemeArchetype::Cyberpunk, |d| {
            SceneCharacter::for_did(&d.0).theme
        });
        let palette = road_palette(theme);

        // Dark wet-asphalt drivable deck — low roughness + reflectance gives the
        // sheen that catches the city light.
        let deck = materials.add(StandardMaterial {
            base_color: palette.deck,
            perceptual_roughness: 0.22,
            metallic: 0.0,
            reflectance: 0.6,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        // Concrete/metal foundation (curb + skirt + bottom) — matte and lighter
        // so the deck reads as a distinct surface sitting on top of it.
        let structure = materials.add(StandardMaterial {
            base_color: palette.structure,
            perceptual_roughness: 0.8,
            metallic: 0.25,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        // Curb edge-line. A neon tube runs hot (white-hot core + bloom halo)
        // and unlit so apply_nightfall can't dim it; a painted line stays low
        // and lit. Either way it's textureless → splat-safe.
        let neon = materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: palette.edge,
            unlit: palette.edge_unlit,
            double_sided: true,
            cull_mode: None,
            ..default()
        });

        // One mesh + material per non-empty surface; the despawn marker on each
        // sweeps them all on the next rebuild. The drivable deck and the
        // curb/skirt structure each carry a static trimesh collider built from
        // their own geometry, so the WHOLE road body is solid — a high road over
        // a dip is a real bridge the player and vehicles stand on. The neon
        // edge-line is a decorative emissive overlay riding proud of the curb, so
        // it stays non-collidable (it would only add thin lips above the curb the
        // structure collider already covers).
        for (geo, material, collide) in [
            (&parts.deck, deck, true),
            (&parts.structure, structure, true),
            (&parts.neon, neon, false),
        ] {
            if geo.is_empty() {
                continue;
            }
            let bevy_mesh = crate::urban::to_bevy_mesh(geo);
            // `trimesh_from_mesh` merges duplicate vertices and returns `None`
            // (never panics) if the buffers can't form a trimesh, so a malformed
            // surface degrades to a visible-but-non-collidable mesh, never a crash.
            let collider = collide
                .then(|| Collider::trimesh_from_mesh(&bevy_mesh))
                .flatten();
            let mesh = meshes.add(bevy_mesh);
            let mut entity = commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                offset,
                Visibility::default(),
                RoadMeshEntity,
            ));
            if let Some(collider) = collider {
                entity.insert((RigidBody::Static, collider));
            }
        }
    }
    fingerprint.0 = want;
}
