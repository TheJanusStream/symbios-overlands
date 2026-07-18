//! Dedicated road-mesh rebuild system.
//!
//! Roads are a `RoadNetwork` child of the terrain generator in the record
//! (authored / edited as config — see
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
use bevy::tasks::Task;
use bevy_symbios_ground::HeightMap;

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

/// Trailing debounce (s) between a road-config edit and the re-mesh (#884).
/// Long enough to collapse a slider drag into one rebuild, short enough that
/// the world answers promptly on release. Shared with the lot-population
/// system so buildings re-derive on the same cadence as the streets.
pub(super) const ROAD_EDIT_DEBOUNCE_SECS: f64 = 0.3;

/// Road-rebuild pipeline state (#884). Replaces the old synchronous
/// fingerprint: edits arm a trailing debounce, the deadline kicks the CPU
/// extrusion onto a background task, and completion swaps the meshes — the
/// previous road stays visible in the meantime, so a drag never shows a
/// road-less gap.
#[derive(Resource, Default)]
pub(super) struct RoadRebuild {
    /// serde-JSON of the config whose mesh is currently live — `None` when no
    /// road mesh exists (no config, disabled, or no terrain).
    live: Option<String>,
    /// Deadline for the pending re-mesh; every further edit pushes it out.
    due: Option<f64>,
    /// In-flight background extrusion: the fingerprint it builds + its task.
    /// Replacing the pair drops the old task, which cancels it.
    building: Option<(String, Task<Option<crate::urban::RoadParts>>)>,
}

/// The active road config + its fingerprint. `enabled: false` reads as no
/// config — the mesh sweep path.
fn current_config(
    record: &crate::pds::RoomRecord,
) -> (Option<crate::pds::generator::RoadConfig>, Option<String>) {
    let config = crate::pds::find_road_config(record)
        .filter(|c| c.enabled)
        .cloned();
    // `to_string` on this plain struct cannot realistically fail; an empty
    // fingerprint (rather than a panic or a silently-dropped mesh) is the
    // degenerate fallback.
    let want = config
        .as_ref()
        .map(|c| serde_json::to_string(c).unwrap_or_default());
    (config, want)
}

/// Data-copy of the heightmap for the background task — `HeightMap` is not
/// `Clone`, but the road builder only samples heights (the normal cache and
/// lake table rebuild lazily / are unused by the road window copy).
fn copy_heightmap(hm: &HeightMap) -> HeightMap {
    let mut copy = HeightMap::new(hm.width(), hm.height(), hm.scale());
    copy.data_mut().copy_from_slice(hm.data());
    copy
}

/// Re-mesh the road network when the heightmap or the road config changes,
/// reusing the existing heightmap (no terrain regeneration). Debounced +
/// task-offloaded (#884): on native the extrusion runs on the
/// `AsyncComputeTaskPool`; on wasm the pool is the main thread, so the win
/// there is the debounce (one build per edit gesture instead of per tick).
#[allow(clippy::too_many_arguments)] // Bevy system: each arg is a distinct resource/query.
pub(super) fn maybe_rebuild_roads(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    did: Option<Res<CurrentRoomDid>>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut state: ResMut<RoadRebuild>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<Entity, With<RoadMeshEntity>>,
) {
    // No terrain (not yet generated, or torn down) → no roads. Sweep any
    // straggler once (dropping an in-flight build cancels it), then idle.
    let Some(heightmap) = heightmap else {
        if state.live.is_some() || state.due.is_some() || state.building.is_some() {
            for e in &existing {
                commands.entity(e).try_despawn();
            }
            *state = RoadRebuild::default();
        }
        return;
    };
    let now = time.elapsed_secs_f64();

    // 1 — change detection arms (or cancels) the trailing debounce. A fresh
    // heightmap (initial load / terrain regen) always re-meshes, since the
    // draped geometry depends on the new surface.
    if heightmap.is_changed() || record.is_changed() {
        let (_, want) = current_config(&record.0);
        if heightmap.is_changed() || want != state.live {
            state.due = Some(now + ROAD_EDIT_DEBOUNCE_SECS);
        } else if state.building.is_none() {
            // The config slid back to exactly the live mesh (an undo mid-
            // debounce): nothing left to rebuild.
            state.due = None;
        }
    }

    // 2 — deadline reached: kick the extrusion for the CURRENT config (not a
    // snapshot from arm time — later edits inside the debounce window are
    // folded in), or sweep synchronously when the network went away.
    if state.due.is_some_and(|d| now >= d) {
        state.due = None;
        let (config, want) = current_config(&record.0);
        if want != state.live || heightmap.is_changed() {
            match config {
                Some(config) => {
                    let hm = copy_heightmap(&heightmap.0);
                    let task = bevy::tasks::AsyncComputeTaskPool::get()
                        .spawn(async move { crate::urban::build_road_geometry(&hm, &config) });
                    state.building = Some((want.unwrap_or_default(), task));
                }
                None => {
                    for e in &existing {
                        commands.entity(e).try_despawn();
                    }
                    state.building = None;
                    state.live = None;
                }
            }
        }
    }

    // 3 — a finished build swaps the meshes. The result may already be a step
    // behind a still-armed debounce; applying it keeps the display fresh and
    // the pending deadline rebuilds to the latest config right after.
    let finished = if let Some((_, task)) = &mut state.building {
        futures_lite::future::block_on(futures_lite::future::poll_once(task))
    } else {
        None
    };
    if let Some(parts) = finished {
        let (built, _) = state.building.take().expect("building checked above");
        for e in &existing {
            commands.entity(e).try_despawn();
        }
        if let Some(parts) = &parts {
            spawn_road_meshes(
                &mut commands,
                &mut meshes,
                &mut materials,
                did.as_deref(),
                &heightmap.0,
                parts,
            );
        }
        state.live = Some(built);
    }
}

/// Spawn the three road surface entities for `parts` — split from the
/// rebuild system so the async completion path stays readable.
fn spawn_road_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    did: Option<&CurrentRoomDid>,
    heightmap: &HeightMap,
    parts: &crate::urban::RoadParts,
) {
    // The ribbon lives in the full heightmap frame; the terrain mesh child
    // is offset by -half, so the road shares that offset.
    {
        let world_extent = (heightmap.width() - 1) as f32 * heightmap.scale();
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
}
