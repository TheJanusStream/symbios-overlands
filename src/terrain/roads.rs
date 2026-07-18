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

/// Which road surface an entity renders — lets the appearance re-tint
/// (#891) find each surface's material without re-spawning anything.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum RoadSurfaceKind {
    Deck,
    Structure,
    Neon,
}

/// Which network (child order under the Terrain generator, #895) a road
/// surface belongs to — pairs with [`RoadSurfaceKind`] so per-network
/// appearance overrides re-tint the right district.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) struct RoadNetworkIndex(pub(super) usize);

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
    /// Curb edge-line base colour (before the strength multiplier), kept
    /// separate from the strength so an authored override can replace either
    /// half independently (#891).
    edge_rgb: [f32; 3],
    /// Emissive strength: ~6 = hot neon tube, ~1 = painted line.
    edge_strength: f32,
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
            edge_rgb: [0.10, 0.95, 1.00],
            edge_strength: 6.0, // hot cyan neon
            edge_unlit: true,
        },
        ModernCity => RoadPalette {
            deck: Color::srgb(0.02, 0.02, 0.024),
            structure: Color::srgb(0.10, 0.10, 0.11),
            edge_rgb: [1.00, 0.92, 0.70],
            edge_strength: 2.2, // warm LED streetlight
            edge_unlit: true,
        },
        IndustrialPark => RoadPalette {
            deck: Color::srgb(0.025, 0.024, 0.022),
            structure: Color::srgb(0.08, 0.075, 0.07),
            edge_rgb: [1.00, 0.62, 0.18],
            edge_strength: 3.2, // hazard amber
            edge_unlit: true,
        },
        Roadside => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.032),
            structure: Color::srgb(0.09, 0.09, 0.09),
            edge_rgb: [1.00, 0.85, 0.25],
            edge_strength: 1.4, // painted lane yellow
            edge_unlit: false,
        },
        CivicCampus => RoadPalette {
            deck: Color::srgb(0.10, 0.10, 0.105), // pale plaza paving
            structure: Color::srgb(0.16, 0.16, 0.17),
            edge_rgb: [0.90, 0.92, 1.00],
            edge_strength: 1.2, // cool soft trim
            edge_unlit: false,
        },
        Suburban => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.033),
            structure: Color::srgb(0.13, 0.13, 0.13),
            edge_rgb: [0.85, 0.85, 0.82],
            edge_strength: 0.8, // faint painted curb line
            edge_unlit: false,
        },
        SportsRec => RoadPalette {
            deck: Color::srgb(0.05, 0.04, 0.04),
            structure: Color::srgb(0.12, 0.12, 0.12),
            edge_rgb: [1.00, 0.95, 0.90],
            edge_strength: 1.0, // court / track line
            edge_unlit: false,
        },
        _ => RoadPalette {
            deck: Color::srgb(0.03, 0.03, 0.035),
            structure: Color::srgb(0.09, 0.09, 0.10),
            edge_rgb: [0.60, 0.80, 1.00],
            edge_strength: 2.5,
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
    /// In-flight background extrusion: the fingerprint it builds + its task
    /// (one entry per active network, #895). Replacing the pair drops the
    /// old task, which cancels it.
    building: Option<(String, Task<Vec<Option<crate::urban::RoadParts>>>)>,
}

/// Every active road config (#895, child order, capped) + the combined
/// GEOMETRY fingerprint — `None` when no enabled network exists (the mesh
/// sweep path). Appearance is deliberately excluded (#891): a re-tint
/// updates the live materials in place ([`sync_road_appearance`]) and must
/// never trigger a re-extrusion.
fn current_configs(
    record: &crate::pds::RoomRecord,
) -> (Vec<crate::pds::generator::RoadConfig>, Option<String>) {
    let configs: Vec<_> = crate::pds::find_road_configs(record)
        .into_iter()
        .filter(|c| c.enabled)
        .cloned()
        .collect();
    if configs.is_empty() {
        return (configs, None);
    }
    // `to_string` on these plain structs cannot realistically fail; an empty
    // fingerprint (rather than a panic or a silently-dropped mesh) is the
    // degenerate fallback.
    let want = configs
        .iter()
        .map(|c| {
            let mut geometry_only = c.clone();
            geometry_only.appearance = Default::default();
            serde_json::to_string(&geometry_only).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("\u{1f}");
    (configs, Some(want))
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
    mut stats: ResMut<super::RoadPanelStats>,
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
            let (buildings, props) = (stats.buildings, stats.props);
            *stats = super::RoadPanelStats {
                buildings,
                props,
                ..default()
            };
        }
        return;
    };
    let now = time.elapsed_secs_f64();

    // 1 — change detection arms (or cancels) the trailing debounce. A fresh
    // heightmap (initial load / terrain regen) always re-meshes, since the
    // draped geometry depends on the new surface.
    if heightmap.is_changed() || record.is_changed() {
        let (_, want) = current_configs(&record.0);
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
        let (configs, want) = current_configs(&record.0);
        if want != state.live || heightmap.is_changed() {
            if configs.is_empty() {
                for e in &existing {
                    commands.entity(e).try_despawn();
                }
                state.building = None;
                state.live = None;
                let (buildings, props) = (stats.buildings, stats.props);
                *stats = super::RoadPanelStats {
                    buildings,
                    props,
                    ..default()
                };
            } else {
                // One task builds every network (#895) — the heightmap copy
                // is shared and the swap stays atomic across districts.
                let hm = copy_heightmap(&heightmap.0);
                let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                    configs
                        .iter()
                        .map(|c| crate::urban::build_road_geometry(&hm, c))
                        .collect::<Vec<_>>()
                });
                state.building = Some((want.unwrap_or_default(), task));
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
    if let Some(all_parts) = finished {
        let (built, _) = state.building.take().expect("building checked above");
        for e in &existing {
            commands.entity(e).try_despawn();
        }
        // Per-network appearances re-read at swap time (#891/#895).
        let appearances: Vec<_> = crate::pds::find_road_configs(&record.0)
            .into_iter()
            .filter(|c| c.enabled)
            .map(|c| c.appearance)
            .collect();
        let (mut streets, mut junctions, mut vertices) = (0, 0, 0);
        for (i, parts) in all_parts.iter().enumerate() {
            let Some(parts) = parts else { continue };
            spawn_road_meshes(
                &mut commands,
                &mut meshes,
                &mut materials,
                did.as_deref(),
                &heightmap.0,
                parts,
                &appearances.get(i).copied().unwrap_or_default(),
                RoadNetworkIndex(i),
            );
            streets += parts.chains;
            junctions += parts.junctions;
            vertices += parts.vertex_count();
        }
        // Editor readout (#888) — buildings belong to the lot layer, which
        // updates its own field on its own cadence.
        stats.built = true;
        stats.streets = streets;
        stats.junctions = junctions;
        stats.vertices = vertices;
        state.live = Some(built);
    }
}

/// The three surface materials for `theme` with the record's overrides
/// (#891) layered on: every `None` falls back to the theme palette. Shared
/// by the spawn path and the live re-tint so they can never disagree.
fn resolved_road_materials(
    theme: ThemeArchetype,
    ap: &crate::pds::generator::RoadAppearance,
) -> [StandardMaterial; 3] {
    let palette = road_palette(theme);
    let color3 = |c: crate::pds::Fp3| Color::srgb(c.0[0], c.0[1], c.0[2]);
    // Dark wet-asphalt drivable deck — low roughness + reflectance gives the
    // sheen that catches the city light.
    let deck = StandardMaterial {
        base_color: ap.deck_color.map(color3).unwrap_or(palette.deck),
        perceptual_roughness: ap.deck_roughness.map_or(0.22, |r| r.0),
        metallic: 0.0,
        reflectance: 0.6,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    // Concrete/metal foundation (curb + skirt + bottom) — matte and lighter
    // so the deck reads as a distinct surface sitting on top of it.
    let structure = StandardMaterial {
        base_color: ap.structure_color.map(color3).unwrap_or(palette.structure),
        perceptual_roughness: 0.8,
        metallic: 0.25,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    // Curb edge-line. A neon tube runs hot (white-hot core + bloom halo)
    // and unlit so apply_nightfall can't dim it; a painted line stays low
    // and lit. Either way it's textureless → splat-safe.
    let rgb = ap.neon_color.map_or(palette.edge_rgb, |c| c.0);
    let strength = ap.neon_strength.map_or(palette.edge_strength, |s| s.0);
    let neon = StandardMaterial {
        base_color: Color::BLACK,
        emissive: glow(rgb, strength),
        unlit: palette.edge_unlit,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    [deck, structure, neon]
}

/// Re-tint the live road materials when the record's appearance overrides
/// change (#891) — in place, without touching geometry, so colour edits are
/// instant (no debounce, no re-extrusion). Also covers reverting to the
/// theme look when an override clears.
pub(super) fn sync_road_appearance(
    record: Res<LiveRoomRecord>,
    did: Option<Res<CurrentRoomDid>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    surfaces: Query<(
        &RoadNetworkIndex,
        &RoadSurfaceKind,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut last: Local<Option<String>>,
) {
    if surfaces.is_empty() {
        *last = None;
        return;
    }
    if !record.is_changed() && last.is_some() {
        return;
    }
    // One appearance per active network (#895), keyed together.
    let appearances: Vec<_> = crate::pds::find_road_configs(&record.0)
        .into_iter()
        .filter(|c| c.enabled)
        .map(|c| c.appearance)
        .collect();
    let key = format!("{appearances:?}");
    if last.as_deref() == Some(key.as_str()) {
        return;
    }
    *last = Some(key);
    let theme = did.as_ref().map_or(ThemeArchetype::Cyberpunk, |d| {
        SceneCharacter::for_did(&d.0).theme
    });
    let resolved: Vec<[StandardMaterial; 3]> = appearances
        .iter()
        .map(|ap| resolved_road_materials(theme, ap))
        .collect();
    for (net, kind, handle) in &surfaces {
        let Some(set) = resolved.get(net.0) else {
            continue;
        };
        if let Some(mat) = materials.get_mut(&handle.0) {
            *mat = match kind {
                RoadSurfaceKind::Deck => set[0].clone(),
                RoadSurfaceKind::Structure => set[1].clone(),
                RoadSurfaceKind::Neon => set[2].clone(),
            };
        }
    }
}

/// Spawn the three road surface entities for `parts` — split from the
/// rebuild system so the async completion path stays readable.
#[allow(clippy::too_many_arguments)] // one spawn site; each arg is a distinct sink.
fn spawn_road_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    did: Option<&CurrentRoomDid>,
    heightmap: &HeightMap,
    parts: &crate::urban::RoadParts,
    appearance: &crate::pds::generator::RoadAppearance,
    network: RoadNetworkIndex,
) {
    // The ribbon lives in the full heightmap frame; the terrain mesh child
    // is offset by -half, so the road shares that offset.
    {
        let world_extent = (heightmap.width() - 1) as f32 * heightmap.scale();
        let half = world_extent * 0.5;
        let offset = Transform::from_xyz(-half, 0.0, -half);

        // The road look follows the room's theme (client-side; not stored),
        // with the record's authored overrides layered on top (#891).
        let theme = did.as_ref().map_or(ThemeArchetype::Cyberpunk, |d| {
            SceneCharacter::for_did(&d.0).theme
        });
        let [deck_mat, structure_mat, neon_mat] = resolved_road_materials(theme, appearance);
        let deck = materials.add(deck_mat);
        let structure = materials.add(structure_mat);
        let neon = materials.add(neon_mat);

        // One mesh + material per non-empty surface; the despawn marker on each
        // sweeps them all on the next rebuild. The drivable deck and the
        // curb/skirt structure each carry a static trimesh collider built from
        // their own geometry, so the WHOLE road body is solid — a high road over
        // a dip is a real bridge the player and vehicles stand on. The neon
        // edge-line is a decorative emissive overlay riding proud of the curb, so
        // it stays non-collidable (it would only add thin lips above the curb the
        // structure collider already covers).
        for (geo, material, kind, collide) in [
            (&parts.deck, deck, RoadSurfaceKind::Deck, true),
            (
                &parts.structure,
                structure,
                RoadSurfaceKind::Structure,
                true,
            ),
            (&parts.neon, neon, RoadSurfaceKind::Neon, false),
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
                kind,
                network,
            ));
            if let Some(collider) = collider {
                entity.insert((RigidBody::Static, collider));
            }
        }
    }
}
