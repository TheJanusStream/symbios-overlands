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

use bevy::prelude::*;

use crate::state::LiveRoomRecord;

use super::FinishedHeightMap;

/// Marker for the spawned road mesh entity, so a rebuild can replace it.
#[derive(Component)]
pub(super) struct RoadMeshEntity;

/// Fingerprint of the road config last meshed — `None` when no road mesh is
/// live (no config, disabled, or no terrain). Lets the rebuild skip work when
/// an unrelated record edit fires.
#[derive(Resource, Default)]
pub(super) struct RoadFingerprint(pub(super) Option<String>);

/// Re-mesh the road network when the heightmap or the road config changes,
/// reusing the existing heightmap (no terrain regeneration).
pub(super) fn maybe_rebuild_roads(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
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
        && let Some(geo) = crate::urban::build_road_geometry(&heightmap.0, config)
    {
        // The ribbon lives in the full heightmap frame; the terrain mesh child
        // is offset by -half, so the road shares that offset.
        let world_extent = (heightmap.0.width() - 1) as f32 * heightmap.0.scale();
        let half = world_extent * 0.5;
        let mesh = meshes.add(crate::urban::to_bevy_mesh(&geo));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.04, 0.05),
            perceptual_roughness: 0.45,
            metallic: 0.2,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_xyz(-half, 0.0, -half),
            Visibility::default(),
            RoadMeshEntity,
        ));
    }
    fingerprint.0 = want;
}
