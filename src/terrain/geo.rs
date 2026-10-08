//! Terrain from real Berlin (#1584, epic #1580; design in
//! `docs/geodata.md`): a geodata region's walkable core, at real scale and
//! real altitude, decoded from GDI Berlin's terrain layer.
//!
//! The procedural pipeline runs one offloaded heightmap job. A region whose
//! record carries a Berlin [`crate::pds::GeoSource`] runs this instead. The
//! terrain layer's legend and one render of the core are fetched through the
//! [`GeoFetcher`] and its cache, decoded on the compute pool, and handed to
//! the procedural pipeline's own landing as a finished [`TerrainTask`]. So the
//! world digest, the session log, the mesh, the collider and the swap of an
//! outgoing terrain are the procedural ones.
//!
//! The core is the terrain config's grid - `grid_size` points `cell_scale`
//! apart - centred on the square, but never wider than the square
//! ([`core_grid`]): a region is its square, and a small one is a small
//! world. So the core always lies inside Berlin, where the data is.
//!
//! Heights are metres above sea level (DHHN2016), as the city measures
//! them, with no datum shift. Berlin's ground never lies below 26 m, so a
//! seeded world's water plane, a few metres up, stays under it until the
//! region has its own water (#1586), and the far field (#1585) will meet
//! the core at the same altitude.
//!
//! If Berlin's terrain cannot be had - the service is unreachable after the
//! fetcher's retries, or answers something that does not decode - the
//! region falls back to the procedural ground its terrain config describes,
//! and [`GeoTerrainFallback`] says why: on the loading screen's terrain row,
//! and in a toast when it happens in game. A visitor is never left on a
//! loading screen, or on stale ground, waiting for a service that is down.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use geodata::GeoSquare;
use geodata::request::Bbox;

use crate::geodata::{GeoFetchError, GeoFetcher, GeoRequest, GeoRequestId};
use crate::pds::SovereignTerrainConfig;

use super::TerrainTask;

/// The largest core grid a render may be: the decoders' pixel cap.
const MAX_CORE_GRID: u32 = 2048;

/// A geodata terrain on its way to becoming the [`TerrainTask`].
#[derive(Resource)]
pub(crate) enum GeoTerrainJob {
    /// Waiting for the terrain layer's legend and the core's render.
    Fetching {
        legend: GeoRequestId,
        render: GeoRequestId,
        grid: u32,
        cell: f32,
        /// Session-relative seconds at the start, for the heightmap latency.
        started: f64,
        /// The terrain fingerprint the job started on.
        source: Option<String>,
        /// The procedural ground to fall back to.
        fallback: gen_jobs::HeightmapParams,
    },
    /// Decoding them on the compute pool.
    Decoding {
        task: Task<Result<gen_jobs::HeightmapData, String>>,
        started: f64,
        source: Option<String>,
        fallback: gen_jobs::HeightmapParams,
    },
}

impl GeoTerrainJob {
    /// Give up on the job: its fetches are forgotten (a running decode is
    /// dropped with the resource).
    pub(crate) fn abandon(&self, fetcher: Option<&mut GeoFetcher>) {
        if let (GeoTerrainJob::Fetching { legend, render, .. }, Some(fetcher)) = (self, fetcher) {
            fetcher.forget(*legend);
            fetcher.forget(*render);
        }
    }
}

/// Present while a Berlin region shows its procedural ground because
/// Berlin's terrain could not be had; says why. Removed when the terrain is
/// next built.
#[derive(Resource, Clone, Debug)]
pub(crate) struct GeoTerrainFallback {
    pub reason: String,
}

/// The core's grid for a square of side `size_m`: the terrain config's
/// `grid_size` points `cell_scale` apart, but no more points than fit the
/// square - the render spans `grid * cell` metres, so at most `size_m`.
pub(crate) fn core_grid(size_m: u32, cfg: &SovereignTerrainConfig) -> (u32, f32) {
    let cell = cfg.cell_scale.0.max(0.01);
    let fits = (f64::from(size_m) / f64::from(cell)).floor() as u32;
    let grid = cfg.grid_size.min(fits).clamp(2, MAX_CORE_GRID);
    (grid, cell)
}

/// The box a core of `grid` points `cell` metres apart is rendered over:
/// `grid` pixels, centred on `square`, so each pixel's centre falls on a
/// grid point - row 0 north, column 0 west, as the heightmap's rows run. The
/// side is whole metres, so where `grid * cell` is not, a pixel is that much
/// wider than `cell` (0.04 % for a seeded 2.21 m cell): a heightmap a few
/// decimetres off over a kilometre.
pub(crate) fn core_bbox(square: GeoSquare, grid: u32, cell: f32) -> Bbox {
    let side = (f64::from(grid) * f64::from(cell)).round() as i64;
    let (centre_e, centre_n) = square.centre();
    let half = side as f64 / 2.0;
    let (min_e, min_n) = (
        (centre_e - half).round() as i64,
        (centre_n - half).round() as i64,
    );
    Bbox {
        min_e,
        min_n,
        max_e: min_e + side,
        max_n: min_n + side,
    }
}

/// Start fetching the core of `square`'s terrain on `cfg`'s grid.
pub(crate) fn start(
    fetcher: &mut GeoFetcher,
    square: GeoSquare,
    cfg: &SovereignTerrainConfig,
    now: f64,
    source: Option<String>,
) -> GeoTerrainJob {
    let (grid, cell) = core_grid(square.size_m, cfg);
    let bbox = core_bbox(square, grid, cell);
    let terrain = &geodata::berlin::TERRAIN;
    GeoTerrainJob::Fetching {
        legend: fetcher.submit(GeoRequest::legend(terrain, 0)),
        render: fetcher.submit(GeoRequest::render(terrain, bbox, grid, grid)),
        grid,
        cell,
        started: now,
        source,
        fallback: super::heightmap::heightmap_params(cfg),
    }
}

/// Decode a core render through its legend: `grid` x `grid` heights, metres
/// above sea level, `cell` metres apart, row 0 north.
pub(crate) fn decode_core(
    legend: &[u8],
    render: &[u8],
    grid: u32,
    cell: f32,
) -> Result<gen_jobs::HeightmapData, String> {
    let legend = geodata::legend::parse_value_legend(legend)
        .map_err(|e| format!("Berlin's terrain legend could not be read: {e}"))?;
    let image = geodata::raster::decode_png(render, grid, grid)
        .map_err(|e| format!("Berlin's terrain could not be read: {e}"))?;
    let heights = geodata::raster::decode_terrain(&image, &legend)
        .map_err(|e| format!("Berlin's terrain could not be read: {e}"))?;
    let pixels = u64::from(grid) * u64::from(grid);
    if u64::from(heights.unmatched + heights.transparent) * 100 > pixels {
        warn!(
            "geodata terrain: {} of {pixels} pixels unmatched, {} transparent - filled from \
             their neighbours",
            heights.unmatched, heights.transparent
        );
    }
    Ok(gen_jobs::HeightmapData {
        width: grid,
        height: grid,
        scale: cell,
        data: heights.heights,
    })
}

/// Fetch and decode a core synchronously, for native tooling
/// ([`super::rebuild_heightmap_for_record`]): the game's own requests, store
/// and transport, so a tool reads the heightmap the game builds. One attempt
/// per request - a tool is rerun, not retried.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn fetch_core_blocking(
    square: GeoSquare,
    cfg: &SovereignTerrainConfig,
) -> Result<bevy_symbios_ground::HeightMap, String> {
    use crate::geodata::{GeoStore, GeoTransport, HttpTransport, fetch_once};
    let (grid, cell) = core_grid(square.size_m, cfg);
    let store = GeoStore::platform_default();
    let now = chrono::Utc::now().timestamp();
    let get = |request: GeoRequest| {
        futures_lite::future::block_on(fetch_once(&request, &store, now, |url, cap| {
            HttpTransport.get(url, cap)
        }))
        .map(|(body, _)| body)
        .map_err(fetch_failure)
    };
    let terrain = &geodata::berlin::TERRAIN;
    let legend = get(GeoRequest::legend(terrain, 0))?;
    let render = get(GeoRequest::render(
        terrain,
        core_bbox(square, grid, cell),
        grid,
        grid,
    ))?;
    let data = decode_core(&legend, &render, grid, cell)?;
    Ok(super::heightmap::heightmap_from_data(data))
}

/// Why a fetch failed for good, as a sentence.
fn fetch_failure(error: GeoFetchError) -> String {
    match error {
        GeoFetchError::Fetch(e) => {
            format!("Berlin's terrain could not be fetched: {}", e.sentence())
        }
        GeoFetchError::BadResponse => {
            "Berlin's map service answered with something that is not terrain.".to_owned()
        }
        GeoFetchError::Redirected => {
            "Berlin's map service sent the terrain request somewhere else.".to_owned()
        }
        GeoFetchError::Refused => "The terrain request was refused before sending.".to_owned(),
    }
}

/// Drive a [`GeoTerrainJob`]: once both answers have settled, decode them
/// on the compute pool; once decoded, hand the heightmap on as a finished
/// [`TerrainTask`]. A failure falls back to the procedural ground and says
/// so ([`GeoTerrainFallback`]). Either way the task is logged as the
/// heightmap offload it is or stands in for, so the stall rule pairs it with
/// its completion.
///
/// Ordered after `maybe_regenerate_terrain` and the cleanup paths, with the
/// sync point that ordering inserts: a job they abandon this frame is gone
/// before this runs, so a stale square's heightmap cannot land.
///
/// Waiting frames read both resources through `Deref` and change nothing.
pub(super) fn poll_geo_terrain(
    mut commands: Commands,
    mut job: ResMut<GeoTerrainJob>,
    mut fetcher: Option<ResMut<GeoFetcher>>,
    time: Res<Time>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
) {
    let now = time.elapsed_secs_f64();
    let log = &mut *session_log;
    match &*job {
        GeoTerrainJob::Fetching {
            legend,
            render,
            grid,
            cell,
            started,
            source,
            fallback,
        } => {
            let (legend, render, grid, cell, started) = (*legend, *render, *grid, *cell, *started);
            let Some(fetcher) = fetcher.as_mut() else {
                let reason = "This build cannot fetch Berlin's terrain.".to_owned();
                fall_back(
                    &mut commands,
                    log,
                    now,
                    fallback,
                    started,
                    source.clone(),
                    reason,
                );
                return;
            };
            // `is_settled` reads through `Deref`: no change tick until both
            // answers are here and are taken.
            if !(fetcher.is_settled(legend) && fetcher.is_settled(render)) {
                return;
            }
            match (fetcher.take(legend), fetcher.take(render)) {
                (Some(Ok(legend_body)), Some(Ok(render_body))) => {
                    let (source, fallback) = (source.clone(), fallback.clone());
                    let task = AsyncComputeTaskPool::get()
                        .spawn(async move { decode_core(&legend_body, &render_body, grid, cell) });
                    *job = GeoTerrainJob::Decoding {
                        task,
                        started,
                        source,
                        fallback,
                    };
                }
                (Some(Err(error)), _) | (_, Some(Err(error))) => {
                    let reason = fetch_failure(error);
                    fall_back(
                        &mut commands,
                        log,
                        now,
                        fallback,
                        started,
                        source.clone(),
                        reason,
                    );
                }
                _ => {
                    let reason = fetch_failure(GeoFetchError::BadResponse);
                    fall_back(
                        &mut commands,
                        log,
                        now,
                        fallback,
                        started,
                        source.clone(),
                        reason,
                    );
                }
            }
        }
        GeoTerrainJob::Decoding { task, .. } => {
            if !task.is_finished() {
                return;
            }
            let GeoTerrainJob::Decoding {
                task,
                started,
                source,
                fallback,
            } = &mut *job
            else {
                return;
            };
            let Some(result) =
                futures_lite::future::block_on(futures_lite::future::poll_once(task))
            else {
                return;
            };
            match result {
                Ok(data) => {
                    let landed = AsyncComputeTaskPool::get()
                        .spawn(async move { crate::offload::GenResult::Heightmap(data) });
                    land(&mut commands, log, now, landed, *started, source.clone());
                }
                Err(reason) => {
                    let fallback = fallback.clone();
                    fall_back(
                        &mut commands,
                        log,
                        now,
                        &fallback,
                        *started,
                        source.clone(),
                        reason,
                    );
                }
            }
        }
    }
}

/// Hand `task` on as the terrain task, logged as the heightmap offload it
/// is or stands in for.
fn land(
    commands: &mut Commands,
    log: &mut crate::diagnostics::SessionLog,
    now: f64,
    task: Task<crate::offload::GenResult>,
    started: f64,
    source: Option<String>,
) {
    log.info(
        now,
        crate::diagnostics::event::EventPayload::OffloadJobStarted {
            job: "heightmap".into(),
        },
    );
    commands.insert_resource(TerrainTask(task, started, source));
    commands.remove_resource::<GeoTerrainJob>();
}

/// Start the region's procedural ground instead, and say why.
fn fall_back(
    commands: &mut Commands,
    log: &mut crate::diagnostics::SessionLog,
    now: f64,
    fallback: &gen_jobs::HeightmapParams,
    started: f64,
    source: Option<String>,
    reason: String,
) {
    warn!("geodata terrain: {reason} - drawing the region's procedural ground");
    commands.insert_resource(GeoTerrainFallback { reason });
    let task = crate::offload::offload(crate::offload::GenJob::Heightmap(fallback.clone()));
    land(commands, log, now, task, started, source);
}

/// Say in game why a Berlin region shows its procedural ground: the
/// loading screen's terrain row says it for a fallback during loading.
pub(super) fn announce_geo_terrain_fallback(
    fallback: Res<GeoTerrainFallback>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
) {
    toasts.warn(
        format!(
            "{} The ground is drawn from the world's terrain settings until it can be fetched.",
            fallback.reason
        ),
        time.elapsed_secs_f64(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodata::{GeoStore, GeoTransport, GetFuture};
    use crate::pds::{Environment, Fp, Generator, GeneratorKind, GeoSource, RoomRecord};
    use crate::state::LiveRoomRecord;
    use crate::terrain::TerrainGenFailed;
    use crate::world_builder::asset_failure::AssetFetchError;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The recorded GDI Berlin answers the `geodata` crate's decode tests
    /// use: the terrain legend, and a 256 x 256 render of E 392000-393024,
    /// N 5820000-5821024 (4 m pixels).
    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/crates/geodata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The square whose core, on a 256-point grid 4 m apart, is exactly the
    /// recorded render's box: centred on it, and wide enough (1,030 m) for
    /// all 256 points to fit.
    fn fixture_square() -> GeoSquare {
        GeoSquare {
            min_e: 391_997,
            min_n: 5_819_997,
            size_m: 1_030,
        }
    }

    fn fixture_config() -> SovereignTerrainConfig {
        SovereignTerrainConfig {
            grid_size: 256,
            cell_scale: Fp(4.0),
            ..Default::default()
        }
    }

    #[test]
    fn a_core_box_centres_its_pixels_on_the_square() {
        let bbox = core_bbox(fixture_square(), 256, 4.0);
        assert_eq!(
            (bbox.min_e, bbox.min_n, bbox.max_e, bbox.max_n),
            (392_000, 5_820_000, 393_024, 5_821_024)
        );
        // The default terrain: 512 points 2 m apart, the same metre box.
        assert_eq!(core_bbox(fixture_square(), 512, 2.0), bbox);
        // A side that is not a whole number of metres rounds to one.
        let odd = core_bbox(fixture_square(), 100, 1.5);
        assert_eq!(odd.max_e - odd.min_e, 150);
        assert_eq!((odd.min_e + odd.max_e) / 2, 392_512);
    }

    #[test]
    fn the_recorded_render_decodes_to_the_core_heightmap() {
        let data = decode_core(
            &fixture("dgm1_legend.json"),
            &fixture("dgm1_392000_5820000_1024m_256px.png"),
            256,
            4.0,
        )
        .unwrap();
        assert_eq!((data.width, data.height, data.scale), (256, 256, 4.0));
        // Friedrichshain lies 31-51 m up, but the render (another survey
        // than the raw tile) has building pits drawn in its lowest classes:
        // 333 pixels below 26 m, read as 25-26 m by the open-class rule.
        assert!(data.data.iter().all(|h| (25.0..52.0).contains(h)));
        // The render is 256 pixels: a 512-point grid cannot be read from it.
        let wrong = decode_core(
            &fixture("dgm1_legend.json"),
            &fixture("dgm1_392000_5820000_1024m_256px.png"),
            512,
            2.0,
        );
        assert!(wrong.unwrap_err().contains("could not be read"));
    }

    /// Serves the recorded legend and render, or a status for the render.
    struct Recorded {
        render_status: Option<u16>,
    }

    impl GeoTransport for Recorded {
        fn get(&self, url: String, _cap: usize) -> GetFuture {
            let legend_url = GeoRequest::legend(&geodata::berlin::TERRAIN, 0)
                .url()
                .to_owned();
            let body = if url == legend_url {
                Ok(fixture("dgm1_legend.json"))
            } else if let Some(status) = self.render_status {
                Err(AssetFetchError::HttpStatus(status))
            } else {
                Ok(fixture("dgm1_392000_5820000_1024m_256px.png"))
            };
            Box::pin(async move { body.map(|bytes| (bytes, url)) })
        }
    }

    /// The terrain pipeline's start, geodata and landing systems over a
    /// record built from real Berlin, fetching through `transport`.
    fn berlin_app(transport: Recorded) -> App {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut generators = HashMap::new();
        generators.insert(
            "base_terrain".to_owned(),
            Generator::from_kind(GeneratorKind::Terrain(fixture_config())),
        );
        let record = RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Environment::default(),
            generators,
            placements: Vec::new(),
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            geo_source: Some(GeoSource::berlin(fixture_square())),
            opaque_refs: Default::default(),
        };
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .init_resource::<crate::diagnostics::SessionLog>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .insert_resource(LiveRoomRecord(record))
            .insert_resource(GeoFetcher::new(GeoStore::Off, Arc::new(transport)))
            .add_systems(
                Update,
                (
                    super::super::heightmap::start_terrain_generation.run_if(
                        resource_exists::<LiveRoomRecord>
                            .and_then(not(resource_exists::<TerrainTask>))
                            .and_then(not(resource_exists::<GeoTerrainJob>))
                            .and_then(not(resource_exists::<super::super::FinishedHeightMap>))
                            .and_then(not(resource_exists::<TerrainGenFailed>)),
                    ),
                    crate::geodata::drive_geo_fetches,
                    poll_geo_terrain.run_if(resource_exists::<GeoTerrainJob>),
                    super::super::heightmap::poll_terrain_task
                        .run_if(resource_exists::<TerrainTask>),
                )
                    .chain(),
            );
        app
    }

    /// Update until `done` holds, on the real task pools.
    fn run_until(app: &mut App, done: impl Fn(&World) -> bool) {
        let start = std::time::Instant::now();
        while !done(app.world()) {
            assert!(
                start.elapsed().as_secs() < 30,
                "the pipeline did not settle"
            );
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn a_berlin_region_lands_its_fetched_terrain_as_the_heightmap() {
        let mut app = berlin_app(Recorded {
            render_status: None,
        });
        run_until(&mut app, |w| {
            w.contains_resource::<super::super::FinishedHeightMap>()
                || w.contains_resource::<TerrainGenFailed>()
        });
        let world = app.world();
        assert!(!world.contains_resource::<TerrainGenFailed>());
        assert!(!world.contains_resource::<GeoTerrainJob>());
        let finished = world.resource::<super::super::FinishedHeightMap>();
        let hm = &finished.0;
        assert_eq!((hm.width(), hm.height(), hm.scale()), (256, 256, 4.0));
        let expected = decode_core(
            &fixture("dgm1_legend.json"),
            &fixture("dgm1_392000_5820000_1024m_256px.png"),
            256,
            4.0,
        )
        .unwrap();
        assert_eq!(
            hm.data(),
            &expected.data[..],
            "the decoded heights, bit for bit"
        );
        // Real altitude: the Friedrichshain ground, tens of metres up.
        assert!((30.0..52.0).contains(&finished.world_height_at(0.0, 0.0)));
        let record = &world.resource::<LiveRoomRecord>().0;
        assert_eq!(
            world.resource::<super::super::HeightMapSource>().0,
            super::super::terrain_source_key(record)
        );
    }

    #[test]
    fn a_map_service_failure_falls_back_to_the_procedural_ground_and_says_why() {
        let mut app = berlin_app(Recorded {
            render_status: Some(404),
        });
        run_until(&mut app, |w| {
            w.contains_resource::<super::super::FinishedHeightMap>()
                || w.contains_resource::<TerrainGenFailed>()
        });
        let world = app.world();
        assert!(
            !world.contains_resource::<TerrainGenFailed>(),
            "not a dead end"
        );
        assert!(!world.contains_resource::<GeoTerrainJob>());
        let fallback = world.resource::<GeoTerrainFallback>();
        assert!(
            fallback.reason.contains("could not be fetched") && fallback.reason.contains("404"),
            "{}",
            fallback.reason
        );
        // The ground is the record's own procedural terrain, bit for bit.
        let record = &world.resource::<LiveRoomRecord>().0;
        let procedural = super::super::heightmap::heightmap_params(&fixture_config());
        let crate::offload::GenResult::Heightmap(expected) =
            crate::offload::GenJob::Heightmap(procedural).run()
        else {
            unreachable!("a heightmap job yields a heightmap")
        };
        let hm = &world.resource::<super::super::FinishedHeightMap>().0;
        assert_eq!(hm.data(), &expected.data[..]);
        assert_eq!(
            world.resource::<super::super::HeightMapSource>().0,
            super::super::terrain_source_key(record)
        );
    }

    #[test]
    fn a_small_square_is_a_small_world() {
        let default = SovereignTerrainConfig::default();
        assert_eq!(default.grid_size, 512);
        // 250 m of 2 m cells: 125 points, the render exactly the square.
        assert_eq!(core_grid(250, &default), (125, 2.0));
        let square = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 250,
        };
        let bbox = core_bbox(square, 125, 2.0);
        assert_eq!(
            (bbox.min_e, bbox.min_n, bbox.max_e, bbox.max_n),
            (391_000, 5_819_500, 391_250, 5_819_750)
        );
        // A big square keeps the configured grid.
        assert_eq!(core_grid(19_000, &default), (512, 2.0));
        // A seeded 2.21 m cell: as many points as fit, never past the side.
        let seeded = SovereignTerrainConfig {
            cell_scale: Fp(2.21),
            ..Default::default()
        };
        let (grid, cell) = core_grid(1_000, &seeded);
        assert_eq!(grid, 452);
        assert!(f64::from(grid) * f64::from(cell) <= 1_000.0);
    }

    /// The race the critic found: a decode finishes in the frame the owner
    /// moves the square. Regeneration abandons the job before the poll runs,
    /// because the ordering inserts a sync point, so the old square's
    /// heightmap never lands.
    #[test]
    fn a_decode_finished_as_the_square_moves_is_dropped_not_landed() {
        use super::super::{
            LastTerrainConfigJson, PendingTerrainConfigJson, TerrainSplatState, lifecycle,
        };
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = berlin_app(Recorded {
            render_status: None,
        });
        // Replace the start chain with just the two systems in the plugin's
        // order, and a job whose decode has already finished.
        let mut app_systems = App::new();
        std::mem::swap(&mut app, &mut app_systems);
        let record = app_systems.world().resource::<LiveRoomRecord>().0.clone();
        let data = decode_core(
            &fixture("dgm1_legend.json"),
            &fixture("dgm1_392000_5820000_1024m_256px.png"),
            256,
            4.0,
        )
        .unwrap();
        let task = AsyncComputeTaskPool::get().spawn(async move { Ok(data) });
        while !task.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        app.init_resource::<Time>()
            .init_resource::<crate::diagnostics::SessionLog>()
            .init_resource::<TerrainSplatState>()
            .init_resource::<PendingTerrainConfigJson>()
            .insert_resource(LastTerrainConfigJson(super::super::terrain_source_key(
                &record,
            )))
            .insert_resource(GeoTerrainJob::Decoding {
                task,
                started: 0.0,
                source: super::super::terrain_source_key(&record),
                fallback: super::super::heightmap::heightmap_params(&fixture_config()),
            })
            .add_systems(
                Update,
                (
                    lifecycle::maybe_regenerate_terrain.before(poll_geo_terrain),
                    poll_geo_terrain.run_if(resource_exists::<GeoTerrainJob>),
                ),
            );
        // The owner moves the square in the same frame.
        let mut moved = record;
        moved.geo_source = Some(GeoSource::berlin(GeoSquare {
            min_e: 391_000,
            ..fixture_square()
        }));
        app.insert_resource(LiveRoomRecord(moved));
        app.update();
        let world = app.world();
        assert!(!world.contains_resource::<GeoTerrainJob>(), "abandoned");
        assert!(
            !world.contains_resource::<TerrainTask>(),
            "the old square's heightmap must not land"
        );
    }

    #[test]
    fn leaving_the_world_mid_fetch_forgets_the_job() {
        use super::super::{
            LastTerrainConfigJson, PendingTerrainConfigJson, RoadPanelStats, TerrainSplatState,
            lifecycle, roads::RoadRebuild,
        };
        let mut app = App::new();
        let mut fetcher = GeoFetcher::new(
            GeoStore::Off,
            Arc::new(Recorded {
                render_status: None,
            }),
        );
        let job = start(&mut fetcher, fixture_square(), &fixture_config(), 0.0, None);
        app.init_resource::<TerrainSplatState>()
            .init_resource::<RoadRebuild>()
            .init_resource::<RoadPanelStats>()
            .init_resource::<LastTerrainConfigJson>()
            .init_resource::<PendingTerrainConfigJson>()
            .insert_resource(fetcher)
            .insert_resource(job)
            .insert_resource(GeoTerrainFallback {
                reason: "earlier".into(),
            })
            .add_systems(Update, lifecycle::cleanup_terrain);
        app.update();
        let world = app.world();
        assert!(!world.contains_resource::<GeoTerrainJob>());
        assert!(!world.contains_resource::<GeoTerrainFallback>());
        assert!(
            world.resource::<GeoFetcher>().is_idle(),
            "its fetches are forgotten"
        );
    }

    /// A Berlin region whose record has no terrain generator is built on the
    /// default grid, so moving its square must regenerate like any terrain
    /// edit - not read as "the terrain was deleted".
    #[test]
    fn a_berlin_region_without_a_terrain_generator_still_regenerates() {
        use super::super::{
            LastTerrainConfigJson, PendingTerrainConfigJson, TerrainSplatState, lifecycle,
        };
        let mut record = berlin_app(Recorded {
            render_status: None,
        })
        .world()
        .resource::<LiveRoomRecord>()
        .0
        .clone();
        record.generators.clear();
        let mut app = App::new();
        app.init_resource::<TerrainSplatState>()
            .init_resource::<PendingTerrainConfigJson>()
            .insert_resource(LastTerrainConfigJson(Some("an earlier square".into())))
            .insert_resource(LiveRoomRecord(record.clone()))
            .add_systems(Update, lifecycle::maybe_regenerate_terrain);
        app.update();
        assert_eq!(
            app.world().resource::<LastTerrainConfigJson>().0,
            super::super::terrain_source_key(&record),
            "the target is the Berlin terrain on the default grid, not a teardown"
        );
    }

    #[test]
    fn an_abandoned_job_forgets_its_fetches() {
        let mut fetcher = GeoFetcher::new(
            GeoStore::Off,
            Arc::new(Recorded {
                render_status: None,
            }),
        );
        let job = start(&mut fetcher, fixture_square(), &fixture_config(), 0.0, None);
        assert!(!fetcher.is_idle());
        job.abandon(Some(&mut fetcher));
        assert!(
            fetcher.is_idle(),
            "nothing is fetched for a terrain nobody waits on"
        );
    }
}
