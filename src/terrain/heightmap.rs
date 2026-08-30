//! Async heightmap generation + the terrain mesh / collider spawner.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_symbios_ground::{
    HeightMap, HeightMapMeshBuilder, NormalMethod, build_heightfield_collider,
};

use crate::config::terrain as tcfg;
use crate::offload::{GenJob, GenResult};
use crate::pds::SovereignTerrainConfig;
use crate::splat::{SplatExtension, SplatTerrainMaterial, SplatUniforms};
use crate::state::LiveRoomRecord;

use super::{FinishedHeightMap, OutgoingTerrain, SplatMaterialHandle, TerrainMesh, TerrainTask};

pub(super) fn start_terrain_generation(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    // `find_terrain_config` walks the generator map in sorted-key order so
    // every peer compiling this record picks the same entry — `HashMap`
    // iteration is SipHash-randomised per process, and without the helper
    // two clients could generate different terrains from the same record.
    let cfg = crate::pds::find_terrain_config(&record.0)
        .cloned()
        .unwrap_or_default();

    // The heightmap is the only product of this task. Roads are re-meshed
    // separately by `roads::maybe_rebuild_roads`, which drapes over the
    // finished heightmap and reacts to road-config edits without a regen.
    // Dispatched through `offload` so the heavy noise + erosion run off the
    // schedule (native: AsyncComputeTaskPool; wasm: task pool / Web Worker).
    let now = time.elapsed_secs_f64();
    let task = crate::offload::offload(GenJob::Heightmap(heightmap_params(&cfg)));
    // Mark the offload lifecycle (#631) so the `offload.task_never_resolves`
    // stall rule can pair this dispatch with its completion in the offline
    // analyzer. This system is gated on `not(TerrainTask)`, so it emits exactly
    // once per generation, not per frame.
    session_log.info(
        now,
        crate::diagnostics::event::EventPayload::OffloadJobStarted {
            job: "heightmap".into(),
        },
    );
    commands.insert_resource(TerrainTask(task, now));
}

pub(super) fn poll_terrain_task(
    mut commands: Commands,
    mut task_res: ResMut<TerrainTask>,
    time: Res<Time>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    record: Option<Res<LiveRoomRecord>>,
    // `Option` for the reason `GeneratorCaches::metrics` is: headless
    // embedders (the render tool, minimal test apps) run terrain generation
    // without the network plugin that owns the digest resource, and a digest
    // nobody exchanges is not worth making them insert.
    mut digest: Option<ResMut<crate::world_digest::WorldDigest>>,
) {
    if let Some(result) =
        futures_lite::future::block_on(futures_lite::future::poll_once(&mut task_res.0))
    {
        let now = time.elapsed_secs_f64();
        let spawned_at = task_res.1;
        commands.remove_resource::<TerrainTask>();
        match result {
            GenResult::Heightmap(data) => {
                crate::diagnostics::samplers::heightmap_latency_secs(
                    &mut metrics,
                    now - spawned_at,
                );
                // Content digest of what the generator produced (#1146). This
                // is the part of the world digest that the cross-target
                // determinism work (#1132) actually moves: erosion and the
                // octave amplitudes run on `exp`/`powf`, whose results Rust
                // documents as platform-dependent. It is quantised to a
                // millimetre, so it reports a terrain two peers would SEE
                // differently and stays quiet about last-bit arithmetic.
                let hm_digest =
                    crate::world_digest::heightmap_digest(data.width, data.height, &data.data);
                if let Some(digest) = digest.as_deref_mut() {
                    if let Some(record) = record.as_ref() {
                        digest.retarget(crate::world_digest::record_fingerprint(&record.0));
                    }
                    digest.heightmap = Some(hm_digest);
                }

                // Typed completion for the B-2 loading-gate heightmap distro.
                session_log.info(
                    now,
                    crate::diagnostics::event::EventPayload::HeightmapGenCompleted {
                        duration_secs: now - spawned_at,
                        width: data.width,
                        height: data.height,
                        digest: hm_digest,
                    },
                );
                // Generic offload-lifecycle completion (#631) — pairs with the
                // `OffloadJobStarted { job: "heightmap" }` at dispatch so the
                // stall rule can measure the round-trip.
                session_log.info(
                    now,
                    crate::diagnostics::event::EventPayload::OffloadJobCompleted {
                        job: "heightmap".into(),
                        duration_secs: now - spawned_at,
                    },
                );
                commands.insert_resource(FinishedHeightMap(heightmap_from_data(data)));
            }
            // A heightmap job only ever yields a heightmap; count an unexpected
            // variant as an offload error (E-4) and leave the terrain unloaded —
            // the loading-gate stall rule surfaces it — rather than panicking.
            _ => {
                crate::diagnostics::samplers::offload_job_error(&mut metrics);
                session_log.error(
                    now,
                    crate::diagnostics::event::EventPayload::OffloadJobFailed {
                        job: "heightmap".into(),
                        reason: "offload job yielded a non-heightmap result".into(),
                    },
                );
                warn!("heightmap offload job yielded an unexpected result — terrain will not load");
            }
        }
    }
}

/// The room's ground mesh: heightfield triangles, area-weighted normals, one
/// UV tile across the whole world, tangents for the splat material's normal
/// maps — and no CPU copy.
///
/// **Why `RENDER_WORLD` only (#1134).** At the default 512-square grid this
/// mesh is roughly 19 MB of positions, normals, UVs, tangents and indices.
/// With `MAIN_WORLD` set — the `RenderAssetUsages` default, which this used
/// to take — Bevy keeps that copy alive in `Assets<Mesh>` for the room's whole
/// life, and builds a fresh one on every re-roll. On wasm linear memory is
/// never returned to the browser, so each of those copies is a permanent floor
/// under the heap: the #565/#625 ratchet, of which this was the single largest
/// identified contributor. #565 established `RENDER_WORLD`-only as the safe
/// pattern for textures and stopped there; this is the mesh half of it.
///
/// Nothing in the main world reads terrain vertices any more. The editor's two
/// `MeshRayCast` pick sites used to — a `RENDER_WORLD` mesh answers
/// `try_attribute` with `ExtractedToRenderWorld`, so the mesh ray silently
/// stops seeing the ground — and they now ask the heightfield collider built
/// beside this mesh instead. That is the same surface described as physics
/// rather than as triangles, and it costs nothing extra because the collider
/// has to exist regardless: the player stands on it.
fn build_terrain_mesh(hm: &HeightMap, world_extent: f32) -> Mesh {
    let mut mesh = HeightMapMeshBuilder::new()
        .with_normal_method(NormalMethod::AreaWeighted)
        .with_uv_tile_size(world_extent)
        .build(hm);

    mesh.generate_tangents()
        .expect("terrain tangent generation failed");

    mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
    mesh
}

pub(super) fn spawn_terrain_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SplatTerrainMaterial>>,
    hm_res: Res<FinishedHeightMap>,
    outgoing: Query<Entity, With<OutgoingTerrain>>,
) {
    // Atomic hand-off from the previous terrain (which has been displaying
    // the player on its collider while the new heightmap generated) to the
    // freshly-spawned one. Queuing the despawn before the new-entity spawn
    // keeps the command order correct — the old colliders are gone by the
    // time physics observes a transform, and no frame ever has zero terrain
    // in the world.
    for e in &outgoing {
        commands.entity(e).try_despawn();
    }

    let hm = &hm_res.0;
    let world_extent = (hm.width() - 1) as f32 * hm.scale();
    let half = world_extent * 0.5;

    let mesh = build_terrain_mesh(hm, world_extent);

    let collider = build_heightfield_collider(hm);

    // Generate D2Array placeholders to satisfy WGPU validation until the real arrays load
    let albedo_placeholder = images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        vec![255u8; 16],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));

    let normal_placeholder = images.add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 4,
        },
        TextureDimension::D2,
        [128u8, 128, 255, 255].repeat(4),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    ));

    // Material starts disabled (flat colour) until the texture tasks finish.
    let pc = tcfg::splat::PLACEHOLDER_COLOR;
    let mat_handle = materials.add(bevy::pbr::ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(pc[0], pc[1], pc[2]),
            perceptual_roughness: tcfg::splat::PLACEHOLDER_ROUGHNESS,
            ..default()
        },
        extension: SplatExtension {
            albedo_array: albedo_placeholder,
            normal_array: normal_placeholder,
            uniforms: SplatUniforms {
                tile_scale: tcfg::TILE_SCALE,
                enabled: 0,
                triplanar_scale: tcfg::TILE_SCALE / world_extent.max(1.0),
                triplanar_sharpness: tcfg::splat::TRIPLANAR_SHARPNESS,
                // Damp-ground darkening stays OFF until the splat pass
                // resolves the room's water line (#913). The material
                // starts as a flat placeholder anyway, and a zero strength
                // is exactly the pre-#913 terrain.
                water_y: 0.0,
                moisture_depth: tcfg::splat::MOISTURE_DEPTH,
                moisture_strength: 0.0,
                _pad0: 0,
            },
            ..default() // weight_map defaults to 1x1 D2, which is fine for the weight sampler
        },
    });

    commands.insert_resource(SplatMaterialHandle(mat_handle.clone()));

    commands
        .spawn((
            Transform::IDENTITY,
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Static,
            collider,
            TerrainMesh,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(mat_handle),
                Transform::from_xyz(-half, 0.0, -half),
            ));
        });

    // Roads are spawned separately by `roads::maybe_rebuild_roads` (it drapes
    // over this finished heightmap and reacts to road-config edits). Water is
    // spawned by the `world_builder` module from the active record's `Water`.
}

/// Distil the app's terrain config into the platform-agnostic
/// [`gen_jobs::HeightmapParams`] the offload layer runs. The generation itself
/// lives in the Bevy-free [`gen_jobs`] crate so native and the wasm Web Worker
/// share one (deterministic) implementation.
pub(crate) fn heightmap_params(cfg: &SovereignTerrainConfig) -> gen_jobs::HeightmapParams {
    gen_jobs::HeightmapParams {
        grid_size: cfg.grid_size,
        cell_scale: cfg.cell_scale.0,
        height_scale: cfg.height_scale.0,
        generator_kind: cfg.generator_kind.to_gen_job(),
        seed: cfg.seed,
        octaves: cfg.octaves,
        persistence: cfg.persistence.0,
        lacunarity: cfg.lacunarity.0,
        base_frequency: cfg.base_frequency.0,
        ds_roughness: cfg.ds_roughness.0,
        voronoi_num_seeds: cfg.voronoi_num_seeds,
        voronoi_num_terraces: cfg.voronoi_num_terraces,
        erosion_enabled: cfg.erosion_enabled,
        erosion_drops: cfg.erosion_drops,
        inertia: cfg.inertia.0,
        erosion_rate: cfg.erosion_rate.0,
        deposition_rate: cfg.deposition_rate.0,
        evaporation_rate: cfg.evaporation_rate.0,
        capacity_factor: cfg.capacity_factor.0,
        thermal_enabled: cfg.thermal_enabled,
        thermal_iterations: cfg.thermal_iterations,
        thermal_talus_angle: cfg.thermal_talus_angle.0,
    }
}

/// Rebuild a [`HeightMap`] from the plain data returned by the offload job.
pub(super) fn heightmap_from_data(d: gen_jobs::HeightmapData) -> HeightMap {
    let mut hm = HeightMap::new(d.width as usize, d.height as usize, d.scale);
    hm.data_mut().copy_from_slice(&d.data);
    hm
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The terrain mesh must reach `Assets<Mesh>` without a CPU copy.
    ///
    /// The sequence this guards is a room re-roll: `spawn_terrain_mesh` runs
    /// once per completed heightmap, and with the `RenderAssetUsages` default
    /// each run leaves its ~19 MB of vertex data resident in the main world
    /// for the life of the room. On wasm the allocator never hands that back
    /// to the browser, so every roll raises the floor (#565/#625). A single
    /// bit is all that separates the two behaviours, and nothing else in the
    /// build would notice it flipping back, so it is asserted directly.
    #[test]
    fn terrain_mesh_keeps_no_main_world_vertex_copy() {
        let hm = HeightMap::new(16, 16, 1.0);
        let mesh = build_terrain_mesh(&hm, 15.0);

        assert_eq!(
            mesh.asset_usage,
            RenderAssetUsages::RENDER_WORLD,
            "terrain mesh must be RENDER_WORLD-only: a MAIN_WORLD copy is \
             permanent on wasm, and the editor's pick sites no longer read it"
        );
        assert!(
            !mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD),
            "MAIN_WORLD set — the vertex data will be retained per re-roll"
        );
    }

    /// The size of what the flag stops retaining, at the shipped default
    /// grid. Printed rather than merely asserted so the number in #1134 has a
    /// measurement behind it and not an estimate; `--nocapture` shows it.
    ///
    /// Asserted loosely (tens of MB, not an exact figure) because the exact
    /// total moves whenever the upstream mesher changes its attribute set,
    /// and the point of the test is the order of magnitude: this is a large
    /// buffer, once per re-roll, permanent on wasm.
    #[test]
    fn the_default_grid_mesh_is_tens_of_megabytes() {
        let hm = HeightMap::new(512, 512, 1.0);
        let mesh = build_terrain_mesh(&hm, 511.0);

        let vertex_bytes = mesh.get_vertex_size() as usize * mesh.count_vertices();
        let index_bytes = mesh.indices().map(|i| i.len() * 4).unwrap_or(0);
        let total = vertex_bytes + index_bytes;
        println!(
            "terrain mesh at the default 512-square grid: {} vertices, \
             {vertex_bytes} vertex bytes + {index_bytes} index bytes = {:.1} MB",
            mesh.count_vertices(),
            total as f64 / 1_048_576.0,
        );

        assert!(
            total > 8 * 1_048_576,
            "expected tens of MB, measured {total} bytes — if the mesher got \
             this much cheaper, #1134's premise is worth re-reading"
        );
    }

    /// The mesh is still fully built before its usage is narrowed: the splat
    /// material samples a normal map, which needs tangents, and a mesh whose
    /// tangent generation was skipped renders flat-lit rather than failing.
    #[test]
    fn terrain_mesh_carries_tangents_for_the_splat_normal_maps() {
        let hm = HeightMap::new(16, 16, 1.0);
        let mesh = build_terrain_mesh(&hm, 15.0);

        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(),
            "no tangents — the splat material's normal maps would be unlit"
        );
    }
}
