//! Terrain lifecycle: logout cleanup and the in-place regenerate (or,
//! for a paint-only edit, repaint, #1743) trigger driven by terrain-config
//! edits in the live room record.

use bevy::prelude::*;

use crate::interaction::TerrainSurfaceQuery;
use crate::state::LiveRoomRecord;

use super::referenced::PendingSplatLayerFetch;
use super::roads::{RoadMeshEntity, RoadRebuild};
use super::{
    FinishedHeightMap, LastTerrainConfigJson, OutgoingTerrain, PendingTerrainConfigJson,
    SplatMaterialHandle, TerrainMesh, TerrainSplatState, TerrainTask, TextureLayerIndex,
    TextureTasksStarted, WaterVolume,
};

/// Despawn terrain + water entities and reset terrain-specific resources so
/// the next login cycle restarts heightmap generation and splat texture
/// uploads from scratch.
///
/// Also registered against the loading screen's "Back to login" abort
/// flag (#849), which tears down a partially-built world without ever
/// passing through `InGame` - see the [`super::TerrainPlugin`]
/// registration.
#[allow(clippy::too_many_arguments)]
pub(super) fn cleanup_terrain(
    mut commands: Commands,
    terrain: Query<Entity, With<TerrainMesh>>,
    outgoing: Query<Entity, With<OutgoingTerrain>>,
    water: Query<Entity, With<WaterVolume>>,
    roads: Query<Entity, With<RoadMeshEntity>>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    pending_splat_refs: Query<Entity, With<PendingSplatLayerFetch>>,
    mut splat_state: ResMut<TerrainSplatState>,
    mut road_state: ResMut<RoadRebuild>,
    mut road_stats: ResMut<super::RoadPanelStats>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
    mut pending_cfg: ResMut<PendingTerrainConfigJson>,
    geo_job: Option<Res<super::geo::GeoTerrainJob>>,
    mut fetcher: Option<ResMut<crate::geodata::GeoFetcher>>,
) {
    // A geodata terrain still fetching is forgotten with the world (#1584).
    if let Some(job) = geo_job {
        job.abandon(fetcher.as_deref_mut());
        commands.remove_resource::<super::geo::GeoTerrainJob>();
    }
    commands.remove_resource::<super::geo::GeoTerrainFallback>();
    // The room's seeded default goes with its terrain (#1647).
    commands.remove_resource::<super::derived::SeededDefault>();
    // All `try_despawn` (#923): these flat sweeps overlap - a mid-swap
    // heightfield is in both `terrain` and `outgoing`, and the water
    // volumes are `RoomEntity`s that `end_attract_scene` (same
    // `OnExit(Login)` transition) and `ui::logout::cleanup_on_logout` also
    // retire - and a plain `despawn` warns per already-dead entity.
    // Teardown only needs the entities gone, not to be their sole owner.
    for e in &terrain {
        commands.entity(e).try_despawn();
    }
    for e in &outgoing {
        commands.entity(e).try_despawn();
    }
    for e in &water {
        commands.entity(e).try_despawn();
    }
    for e in &roads {
        commands.entity(e).try_despawn();
    }
    // In-flight splat texture bakes would otherwise survive into the next
    // login cycle, resolve onto orphaned entities, and leak until process
    // exit. Drain the marker-tagged bake entities here.
    for e in &pending_textures {
        commands.entity(e).try_despawn();
    }
    // In-flight Referenced-texture fetches likewise need to be drained
    // so a fast logout/login cycle doesn't leak network tasks past the
    // room they were dispatched for.
    for e in &pending_splat_refs {
        commands.entity(e).try_despawn();
    }
    *splat_state = TerrainSplatState::default();
    // Full reset (#884): clears the live fingerprint, disarms any pending
    // debounce, and drops (= cancels) an in-flight background extrusion.
    *road_state = RoadRebuild::default();
    *road_stats = super::RoadPanelStats::default();
    last_cfg.0 = None;
    pending_cfg.0 = None;
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<super::HeightMapSource>();
    commands.remove_resource::<SplatMaterialHandle>();
    commands.remove_resource::<TextureTasksStarted>();
    commands.remove_resource::<TerrainTask>();
    commands.remove_resource::<super::TerrainGenFailed>();
    // Drop the interaction CPU terrain mirror with its terrain so the
    // classifier doesn't probe a stale heightmap after logout.
    commands.remove_resource::<TerrainSurfaceQuery>();
}

/// Watch the active room record for terrain-config changes. When the owner
/// edits grid size, noise params, erosion, or any other field that shapes
/// the ground, despawn the existing heightfield, drop the cached heightmap /
/// splat resources, and let the generic `Update` pipeline re-kick terrain +
/// texture tasks from scratch. An edit to what is only painted on the
/// ground, the splat material or the paths, bakes and lays the splat again
/// over the standing heightmap instead (#1743). The first observation of a
/// config simply records its keys ([`super::TerrainKeys`]) - Loading handled
/// the initial build - so this only fires on *changes* after the player is
/// already InGame.
#[allow(clippy::too_many_arguments)]
pub(super) fn maybe_regenerate_terrain(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
    mut pending_cfg: ResMut<PendingTerrainConfigJson>,
    terrain_q: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    pending_splat_refs: Query<Entity, With<PendingSplatLayerFetch>>,
    mut splat_state: ResMut<TerrainSplatState>,
    terrain_task: Option<Res<TerrainTask>>,
    geo_job: Option<Res<super::geo::GeoTerrainJob>>,
    mut fetcher: Option<ResMut<crate::geodata::GeoFetcher>>,
) {
    // Capture the terrain *target* of any observed change *before* we
    // decide whether to act on it. `Res::is_changed` is a per-system tick
    // that fires exactly once; if we let a frame with an in-flight terrain
    // task consume the tick via an early return, the edit is silently
    // lost - `record.is_changed()` won't re-fire unless the owner edits
    // again. Serde-serialising the full `SovereignTerrainConfig` is
    // non-trivial (deeply nested record), so we still gate it on
    // `is_changed` rather than rebuilding every frame.
    //
    // A missing terrain generator (`find_terrain_config` → `None`) is a
    // real, distinct target - the owner deleted the terrain - and queues
    // `Some(None)` so the heightfield is torn down rather than left as an
    // orphaned phantom mesh. We only skip updating the target on a
    // serialisation failure, which preserves the old "leave it alone"
    // behaviour for that (vanishingly rare) case.
    if record.is_changed() {
        // Only the terrain config gates a heightmap regen. A road-config edit
        // does *not* regenerate terrain - `roads::maybe_rebuild_roads` re-meshes
        // the road from the existing heightmap instead.
        let berlin = record
            .0
            .geo_source
            .as_ref()
            .and_then(crate::pds::GeoSource::berlin_square)
            .is_some();
        // No terrain generator, but a region built from real Berlin: its
        // ground is built on the default grid, exactly as the start system
        // builds it (#1584), so the default terrain's keys are the target.
        if crate::pds::find_terrain_config(&record.0).is_some() || berlin {
            if let Some(keys) = super::TerrainKeys::of(&record.0) {
                pending_cfg.0 = Some(Some(keys));
            }
        } else {
            pending_cfg.0 = Some(None);
        }
    }

    // Refuse to tear down in-flight generation - the previous async
    // task's output would still land in `FinishedHeightMap` and the new
    // pipeline couldn't start. The pending target stays queued and will
    // be applied on a later frame once the task completes.
    if terrain_task.is_some() {
        return;
    }
    // A geodata terrain still fetching or decoding is abandoned rather than
    // waited for (#1584): a fetch can take seconds, and its answer would be
    // for the square the owner just left.
    let abandon_geo =
        |commands: &mut Commands, fetcher: &mut Option<ResMut<crate::geodata::GeoFetcher>>| {
            if let Some(job) = geo_job.as_deref() {
                job.abandon(fetcher.as_deref_mut());
                commands.remove_resource::<super::geo::GeoTerrainJob>();
            }
        };
    let Some(target) = pending_cfg.0.take() else {
        return;
    };
    // Decide what to do by comparing the target against what's currently
    // built (`last_cfg`). The first observation of a config is a no-op
    // because the initial Loading pipeline already built it.
    enum Apply {
        Nothing,
        Repaint,
        Regenerate,
        Teardown,
    }
    let apply = match (&last_cfg.0, &target) {
        (Some(prev), Some(keys)) if prev == keys => Apply::Nothing,
        // The same ground, painted otherwise (#1743).
        (Some(prev), Some(keys)) if prev.ground == keys.ground => Apply::Repaint,
        (Some(_), Some(_)) => Apply::Regenerate,
        (Some(_), None) => Apply::Teardown, // terrain generator deleted
        (None, _) => Apply::Nothing,        // first observation / nothing built
    };
    last_cfg.0 = target;

    match apply {
        Apply::Nothing => {}
        Apply::Repaint => {
            // Only what is painted on the ground changed (#1743) - a splat
            // layer's texture or colours, a rule, the tile scale, a path -
            // and the heightmap job reads none of it: the ground stands,
            // with its colliders, its ground cover, its roads and its lots.
            // The four layers bake again and `apply_splat_textures` lays
            // them, the weights and the paths onto the standing material,
            // which shows the old ones until they land. Tearing it all down
            // here ran noise and erosion again, or refetched a Berlin
            // square, to build the same ground - and opened the window
            // #1738's panic and #1741's regrow came through. A geodata
            // fetch on its way is for this same ground and is left to land.
            for e in &pending_textures {
                commands.entity(e).despawn();
            }
            for e in &pending_splat_refs {
                commands.entity(e).despawn();
            }
            commands.remove_resource::<TextureTasksStarted>();
            *splat_state = TerrainSplatState::default();
            info!("Terrain paint changed - baking the splat again over the standing ground");
        }
        Apply::Regenerate => {
            // Mark the current terrain as outgoing instead of despawning it
            // immediately: the player sits on its heightfield collider, so
            // removing it before the new heightmap task completes would drop
            // them through the world for the ~frame(s) generation takes, and
            // every peer would see an abrupt flash to empty sky.
            // `spawn_terrain_mesh` despawns outgoing entries atomically when
            // the fresh mesh spawns. Water is a `RoomEntity`, so
            // `compile_room_record` despawns and rebuilds it in response to
            // the same record change - touching it here would race and
            // double-despawn.
            for e in &terrain_q {
                commands.entity(e).insert(OutgoingTerrain);
            }
            for e in &pending_textures {
                commands.entity(e).despawn();
            }
            // Drop in-flight Referenced-layer fetches from the previous
            // config too - they'd otherwise land on the new splat state
            // with stale textures from the old config's layers.
            for e in &pending_splat_refs {
                commands.entity(e).despawn();
            }
            commands.remove_resource::<FinishedHeightMap>();
            commands.remove_resource::<SplatMaterialHandle>();
            commands.remove_resource::<TextureTasksStarted>();
            commands.remove_resource::<TerrainTask>();
            abandon_geo(&mut commands, &mut fetcher);
            // A new config is a new job, so a previous job's failure is not
            // about it (#1230 f21) - leaving the marker would refuse to
            // start the replacement.
            commands.remove_resource::<super::TerrainGenFailed>();
            *splat_state = TerrainSplatState::default();
            info!("Terrain config changed - regenerating heightmap + splat textures");
        }
        Apply::Teardown => {
            // The owner removed the terrain generator and no replacement is
            // coming, so - unlike the regenerate path - despawn the
            // heightfield outright rather than marking it outgoing. The
            // player loses the ground they were standing on, which is the
            // correct consequence of deleting it. Water is left to
            // `compile_room_record` as in the regenerate path.
            for e in &terrain_q {
                commands.entity(e).despawn();
            }
            for e in &pending_textures {
                commands.entity(e).despawn();
            }
            for e in &pending_splat_refs {
                commands.entity(e).despawn();
            }
            commands.remove_resource::<FinishedHeightMap>();
            commands.remove_resource::<SplatMaterialHandle>();
            commands.remove_resource::<TextureTasksStarted>();
            commands.remove_resource::<TerrainTask>();
            abandon_geo(&mut commands, &mut fetcher);
            // A new config is a new job, so a previous job's failure is not
            // about it (#1230 f21) - leaving the marker would refuse to
            // start the replacement.
            commands.remove_resource::<super::TerrainGenFailed>();
            // Drop the CPU terrain mirror so the interaction classifier
            // doesn't keep probing a heightmap that no longer exists.
            commands.remove_resource::<TerrainSurfaceQuery>();
            *splat_state = TerrainSplatState::default();
            info!("Terrain generator removed - despawning heightfield + splat textures");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy_symbios_ground::HeightMap;

    use super::super::{
        FinishedHeightMap, LastTerrainConfigJson, PendingTerrainConfigJson, SplatMaterialHandle,
        TerrainSplatState, TerrainTask, TextureTasksStarted, geo, heightmap, splat,
    };
    use crate::pds::{
        Fp, Fp3, GeneratorKind, RoomRecord, SovereignGroundConfig, SovereignTerrainConfig,
        SovereignTextureConfig,
    };
    use crate::splat::SplatTerrainMaterial;
    use crate::state::LiveRoomRecord;

    fn terrain(record: &mut RoomRecord) -> &mut SovereignTerrainConfig {
        record
            .generators
            .values_mut()
            .find_map(|generator| match &mut generator.kind {
                GeneratorKind::Terrain(terrain) => Some(terrain),
                _ => None,
            })
            .expect("the seeded world has a terrain")
    }

    /// A seeded world whose first layer is plain green ground, small enough
    /// to build fast: what a recolour is made on.
    fn world() -> RoomRecord {
        let mut record = RoomRecord::default_for_did("did:plc:recolour");
        let cfg = terrain(&mut record);
        cfg.grid_size = 33;
        cfg.erosion_drops = 100;
        cfg.material.texture_size = 16;
        cfg.material.layers[0] = SovereignTextureConfig::Ground(SovereignGroundConfig {
            color_dry: Fp3([0.1, 0.4, 0.1]),
            color_moist: Fp3([0.1, 0.3, 0.1]),
            ..Default::default()
        });
        record
    }

    /// The owner paints the first layer another colour, as the editor's
    /// colour picker writes it into the live record.
    fn recolour(app: &mut App) {
        let mut record = app.world().resource::<LiveRoomRecord>().0.clone();
        if let SovereignTextureConfig::Ground(ground) = &mut terrain(&mut record).material.layers[0]
        {
            ground.color_dry = Fp3([0.8, 0.2, 0.1]);
            ground.color_moist = Fp3([0.6, 0.1, 0.1]);
        }
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = record;
    }

    /// A geodata transport whose every request fails at once: nothing
    /// leaves the machine, and a fetch asked for still shows as one in
    /// flight.
    struct Offline;

    impl crate::geodata::GeoTransport for Offline {
        fn get(&self, _url: String, _cap: usize) -> crate::geodata::GetFuture {
            Box::pin(async {
                Err(crate::world_builder::asset_failure::AssetFetchError::HttpStatus(503))
            })
        }
    }

    fn offline_fetcher() -> crate::geodata::GeoFetcher {
        crate::geodata::GeoFetcher::new(crate::geodata::GeoStore::Off, Arc::new(Offline))
    }

    /// The live terrain's systems as the plugin orders them - the regenerate
    /// trigger, the heightmap's start, and the splat's bakes, collection and
    /// application - over `record` with a heightmap and a material standing,
    /// as a world that has finished loading has them.
    fn live_app(record: RoomRecord) -> App {
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<SplatTerrainMaterial>()
            .init_resource::<crate::diagnostics::SessionLog>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .init_resource::<super::super::referenced::ReferencedLayerStatus>()
            .init_resource::<crate::state::LocalSettings>()
            .init_resource::<TerrainSplatState>()
            .init_resource::<LastTerrainConfigJson>()
            .init_resource::<PendingTerrainConfigJson>()
            .insert_resource(LiveRoomRecord(record))
            .insert_resource(FinishedHeightMap(HeightMap::new(33, 33, 2.0), None))
            .add_systems(
                Update,
                (
                    super::maybe_regenerate_terrain,
                    heightmap::start_terrain_generation.run_if(
                        not(resource_exists::<TerrainTask>)
                            .and_then(not(resource_exists::<geo::GeoTerrainJob>))
                            .and_then(not(resource_exists::<FinishedHeightMap>)),
                    ),
                    splat::start_texture_tasks.run_if(not(resource_exists::<TextureTasksStarted>)),
                    splat::collect_texture_results,
                    splat::apply_splat_textures,
                )
                    .chain(),
            );
        let material = app
            .world_mut()
            .resource_mut::<Assets<SplatTerrainMaterial>>()
            .add(SplatTerrainMaterial::default());
        app.insert_resource(SplatMaterialHandle(material));
        app
    }

    /// Run frames until the splat is applied, the bakes landing off the
    /// schedule; `each` reads every frame on the way.
    fn until_applied(app: &mut App, mut each: impl FnMut(&App)) {
        for _ in 0..20_000 {
            app.update();
            each(app);
            if app.world().resource::<TerrainSplatState>().applied {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the splat never applied");
    }

    /// The ground's layer textures as its material holds them, a layer each.
    fn layers(app: &App) -> Vec<Vec<u8>> {
        let world = app.world();
        let handle = &world.resource::<SplatMaterialHandle>().0;
        let material = world
            .resource::<Assets<SplatTerrainMaterial>>()
            .get(handle)
            .expect("the ground's material");
        let bytes = world
            .resource::<Assets<Image>>()
            .get(&material.extension.albedo_array)
            .and_then(|image| image.data.clone())
            .expect("the ground's layers");
        bytes.chunks(bytes.len() / 4).map(<[u8]>::to_vec).collect()
    }

    /// #1743: a recolour rebuilds the ground's splat over the heightmap that
    /// stands - no heightmap job, the same heightmap, the same material -
    /// and the new colour reaches the material, that layer's alone. Before,
    /// the terrain's key held the material: a splat layer's colour tore the
    /// whole terrain down and ran the heightmap job (noise, erosion) for
    /// ground that could not have moved.
    #[test]
    fn a_recolour_paints_the_standing_ground_again() {
        let mut app = live_app(world());
        until_applied(&mut app, |_| {});
        let before = layers(&app);
        let material = app.world().resource::<SplatMaterialHandle>().0.clone();
        let ground = app
            .world()
            .get_resource_change_ticks::<FinishedHeightMap>()
            .expect("the heightmap")
            .added;

        recolour(&mut app);
        app.update();
        {
            let world = app.world();
            assert!(
                !world.contains_resource::<TerrainTask>(),
                "a recolour started a heightmap job"
            );
            assert!(
                world.contains_resource::<FinishedHeightMap>()
                    && world.contains_resource::<SplatMaterialHandle>(),
                "a recolour tore the ground down"
            );
        }
        let mut stood = true;
        until_applied(&mut app, |app| {
            let world = app.world();
            stood &= !world.contains_resource::<TerrainTask>()
                && world
                    .get_resource_change_ticks::<FinishedHeightMap>()
                    .is_some_and(|ticks| ticks.added == ground);
        });
        assert!(
            stood,
            "a recolour started a heightmap job or replaced the heightmap"
        );
        assert_eq!(
            app.world().resource::<SplatMaterialHandle>().0,
            material,
            "the ground keeps its material"
        );
        let after = layers(&app);
        assert_ne!(
            after[0], before[0],
            "the new colour never reached the ground"
        );
        assert_eq!(after[1..], before[1..], "the other layers are as they were");
    }

    /// #1743: an edit that can move the ground still builds it again.
    #[test]
    fn a_height_edit_still_rebuilds_the_ground() {
        let mut app = live_app(world());
        app.insert_resource(TextureTasksStarted);
        app.update();
        let mut record = app.world().resource::<LiveRoomRecord>().0.clone();
        terrain(&mut record).height_scale = Fp(terrain(&mut record).height_scale.0 + 1.0);
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = record;
        app.update();
        let world = app.world();
        assert!(
            world.contains_resource::<TerrainTask>(),
            "a height edit started no heightmap job"
        );
        assert!(!world.contains_resource::<SplatMaterialHandle>());
    }

    /// #1743 in a region built from real Berlin: a recolour fetches no
    /// square again, nor abandons one on its way.
    #[test]
    fn a_recolour_fetches_no_berlin_square() {
        let square = geodata::GeoSquare {
            min_e: 391_200,
            min_n: 5_819_700,
            size_m: 600,
        };
        let mut record = world();
        record.geo_source = Some(crate::pds::GeoSource::berlin(square));

        // Its ground standing.
        let mut app = live_app(record.clone());
        app.insert_resource(offline_fetcher())
            .insert_resource(TextureTasksStarted);
        app.update();
        recolour(&mut app);
        app.update();
        let world = app.world();
        assert!(
            world.contains_resource::<FinishedHeightMap>(),
            "a recolour tore the ground down"
        );
        assert!(
            !world.contains_resource::<geo::GeoTerrainJob>(),
            "a recolour fetched the square again"
        );
        assert!(world.resource::<crate::geodata::GeoFetcher>().is_idle());

        // Its ground on its way.
        let mut app = live_app(record.clone());
        app.world_mut().remove_resource::<FinishedHeightMap>();
        let mut fetcher = offline_fetcher();
        let job = geo::start(
            &mut fetcher,
            square,
            &SovereignTerrainConfig::default(),
            0.0,
            super::super::terrain_source_key(&record),
        );
        app.insert_resource(fetcher)
            .insert_resource(job)
            .insert_resource(TextureTasksStarted);
        app.update();
        recolour(&mut app);
        app.update();
        assert!(
            app.world().contains_resource::<geo::GeoTerrainJob>(),
            "a recolour abandoned the square's fetch"
        );
    }
}
