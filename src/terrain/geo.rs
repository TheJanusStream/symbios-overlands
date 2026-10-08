//! Terrain from real Berlin (#1584, epic #1580; design in
//! `docs/geodata.md`): a geodata region's walkable core, at real scale and
//! real altitude, decoded from GDI Berlin's terrain layer, and coloured and
//! watered from its land-use layer (#1586).
//!
//! The procedural pipeline runs one offloaded heightmap job. A region whose
//! record carries a Berlin [`crate::pds::GeoSource`] runs this instead. Four
//! answers are fetched through the [`GeoFetcher`] and its cache - the
//! terrain layer's legend and one render of the core, and the land-use
//! layer's legend and one render of the same box - decoded on the compute
//! pool, and handed to the procedural pipeline's own landing as a finished
//! [`TerrainTask`]. So the world digest, the session log, the mesh, the
//! collider and the swap of an outgoing terrain are the procedural ones.
//!
//! The core is the terrain config's grid - `grid_size` points `cell_scale`
//! apart - centred on the square, but never wider than the square
//! ([`core_grid`]): a region is its square, and a small one is a small
//! world. So the core always lies inside Berlin, where the data is.
//!
//! Heights are metres above sea level (DHHN2016), as the city measures
//! them, with no datum shift; the far field (#1585) will meet the core at
//! the same altitude. The land use rides with them as a [`GeoGround`]
//! ([`ground`]): it paints the splat layers and settles the water - the
//! core's water level is where the region draws its water plane, its beds
//! are carved below it and the rest of the ground is kept above it.
//!
//! If Berlin's terrain cannot be had - the service is unreachable after the
//! fetcher's retries, or answers something that does not decode - the
//! region falls back to the procedural ground its terrain config describes,
//! and [`GeoTerrainFallback`] says why: on the loading screen's terrain row,
//! and in a toast when it happens in game. A visitor is never left on a
//! loading screen, or on stale ground, waiting for a service that is down.
//! If only the land use cannot be had, Berlin's terrain still lands, its
//! colours following the record's altitude bands and with no water, and
//! the fallback says that instead.

mod ground;

pub(crate) use ground::GeoGround;

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use geodata::GeoSquare;
use geodata::request::Bbox;

use crate::geodata::{GeoFetchError, GeoFetcher, GeoRequest, GeoRequestId};
use crate::pds::SovereignTerrainConfig;

use super::TerrainTask;

/// The largest core grid a render may be: the decoders' pixel cap.
const MAX_CORE_GRID: u32 = 2048;

/// The four requests a core is built from, all over the core's box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CoreRequests {
    terrain_legend: GeoRequestId,
    terrain: GeoRequestId,
    land_use_legend: GeoRequestId,
    land_use: GeoRequestId,
}

impl CoreRequests {
    /// Submit the four requests for a core of `grid` points over `bbox`.
    fn submit(fetcher: &mut GeoFetcher, bbox: Bbox, grid: u32) -> Self {
        let (terrain, land_use) = (&geodata::berlin::TERRAIN, &geodata::berlin::LAND_USE);
        CoreRequests {
            terrain_legend: fetcher.submit(GeoRequest::legend(terrain, 0)),
            terrain: fetcher.submit(GeoRequest::render(terrain, bbox, grid, grid)),
            land_use_legend: fetcher.submit(GeoRequest::legend(land_use, 0)),
            land_use: fetcher.submit(GeoRequest::render(land_use, bbox, grid, grid)),
        }
    }

    /// All four, for counting and forgetting.
    pub(crate) fn ids(&self) -> [GeoRequestId; 4] {
        [
            self.terrain_legend,
            self.terrain,
            self.land_use_legend,
            self.land_use,
        ]
    }
}

/// A geodata terrain on its way to becoming the [`TerrainTask`].
#[derive(Resource)]
pub(crate) enum GeoTerrainJob {
    /// Waiting for the four answers.
    Fetching {
        requests: CoreRequests,
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
        task: Task<Result<GeoCore, String>>,
        started: f64,
        source: Option<String>,
        fallback: gen_jobs::HeightmapParams,
    },
}

impl GeoTerrainJob {
    /// Give up on the job: its fetches are forgotten (a running decode is
    /// dropped with the resource).
    pub(crate) fn abandon(&self, fetcher: Option<&mut GeoFetcher>) {
        if let (GeoTerrainJob::Fetching { requests, .. }, Some(fetcher)) = (self, fetcher) {
            for id in requests.ids() {
                fetcher.forget(id);
            }
        }
    }
}

/// Present while a Berlin region shows less of Berlin than its record asks
/// for, because part of it could not be had; says what and why. Removed
/// when the terrain is next built.
#[derive(Resource, Clone, Debug)]
pub(crate) struct GeoTerrainFallback {
    /// Why, as a sentence.
    pub reason: String,
    /// What was lost.
    pub lost: GeoLoss,
}

/// What part of Berlin a region had to do without.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeoLoss {
    /// Its terrain: the region shows its procedural ground.
    Terrain,
    /// Only its land use: Berlin's terrain, coloured by height, dry.
    LandUse,
}

impl GeoLoss {
    /// What the region shows instead, as a sentence.
    pub(crate) fn instead(self) -> &'static str {
        match self {
            GeoLoss::Terrain => {
                "The ground is drawn from the world's terrain settings until it can be fetched."
            }
            GeoLoss::LandUse => {
                "Berlin's ground is coloured by height, with no water, until its land use can be \
                 fetched."
            }
        }
    }
}

/// A layer's legend and render, as fetched.
pub(crate) type LayerBodies = (Arc<[u8]>, Arc<[u8]>);

/// The four answers' bodies, as fetched.
pub(crate) struct CoreBodies {
    pub terrain_legend: Arc<[u8]>,
    pub terrain: Arc<[u8]>,
    /// The land use's legend and render, or why they could not be had.
    pub land_use: Result<LayerBodies, String>,
}

/// A decoded core: its heights, and what covers them - or why the land use
/// could not be read, in which case the heights are the terrain's own,
/// with no water settled on them.
pub(crate) struct GeoCore {
    pub heights: gen_jobs::HeightmapData,
    pub ground: Result<GeoGround, String>,
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

/// Start fetching the core of `square` on `cfg`'s grid.
pub(crate) fn start(
    fetcher: &mut GeoFetcher,
    square: GeoSquare,
    cfg: &SovereignTerrainConfig,
    now: f64,
    source: Option<String>,
) -> GeoTerrainJob {
    let (grid, cell) = core_grid(square.size_m, cfg);
    GeoTerrainJob::Fetching {
        requests: CoreRequests::submit(fetcher, core_bbox(square, grid, cell), grid),
        grid,
        cell,
        started: now,
        source,
        fallback: super::heightmap::heightmap_params(cfg),
    }
}

/// Decode a core: `grid` x `grid` heights, metres above sea level, `cell`
/// metres apart, row 0 north - and, where the land use was had, what covers
/// them, with the core's water settled on the heights. An error is the
/// terrain's: it is what the core cannot be built without.
pub(crate) fn decode_core(bodies: &CoreBodies, grid: u32, cell: f32) -> Result<GeoCore, String> {
    let mut heights = decode_heights(&bodies.terrain_legend, &bodies.terrain, grid, cell)?;
    let ground = match &bodies.land_use {
        Ok((legend, render)) => {
            ground::decode_ground(legend, render, &mut heights.data, grid, cell)
        }
        Err(reason) => Err(reason.clone()),
    };
    Ok(GeoCore { heights, ground })
}

/// Decode a terrain render through its legend: `grid` x `grid` heights.
fn decode_heights(
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
/// ([`super::rebuild_terrain_for_record`]): the game's own requests, store
/// and transport, so a tool reads the ground the game builds. One attempt
/// per request - a tool is rerun, not retried - and an error for any part
/// of Berlin missing, the land use included: a tool would otherwise report
/// on ground the game does not draw.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn fetch_core_blocking(
    square: GeoSquare,
    cfg: &SovereignTerrainConfig,
) -> Result<(bevy_symbios_ground::HeightMap, GeoGround), String> {
    use crate::geodata::{GeoStore, GeoTransport, HttpTransport, fetch_once};
    let (grid, cell) = core_grid(square.size_m, cfg);
    let bbox = core_bbox(square, grid, cell);
    let store = GeoStore::platform_default();
    let now = chrono::Utc::now().timestamp();
    let get = |what: &str, request: GeoRequest| {
        futures_lite::future::block_on(fetch_once(&request, &store, now, |url, cap| {
            HttpTransport.get(url, cap)
        }))
        .map(|(body, _)| body)
        .map_err(|e| fetch_failure(what, e))
    };
    let (terrain, land_use) = (&geodata::berlin::TERRAIN, &geodata::berlin::LAND_USE);
    let bodies = CoreBodies {
        terrain_legend: get("terrain", GeoRequest::legend(terrain, 0))?,
        terrain: get("terrain", GeoRequest::render(terrain, bbox, grid, grid))?,
        land_use: Ok((
            get("land use", GeoRequest::legend(land_use, 0))?,
            get("land use", GeoRequest::render(land_use, bbox, grid, grid))?,
        )),
    };
    let core = decode_core(&bodies, grid, cell)?;
    Ok((
        super::heightmap::heightmap_from_data(core.heights),
        core.ground?,
    ))
}

/// Why a fetch of Berlin's `what` ("terrain", "land use") failed for good,
/// as a sentence.
fn fetch_failure(what: &str, error: GeoFetchError) -> String {
    match error {
        GeoFetchError::Fetch(e) => {
            format!("Berlin's {what} could not be fetched: {}", e.sentence())
        }
        GeoFetchError::BadResponse => {
            format!("Berlin's map service answered the {what} request with something else.")
        }
        GeoFetchError::Redirected => {
            format!("Berlin's map service sent the {what} request somewhere else.")
        }
        GeoFetchError::Refused => format!("The {what} request was refused before sending."),
    }
}

/// One answer, taken: its body, or why there is none. An answer that is
/// settled but gone (taken already, or forgotten) is a bad response.
fn taken(fetcher: &mut GeoFetcher, id: GeoRequestId, what: &str) -> Result<Arc<[u8]>, String> {
    match fetcher.take(id) {
        Some(Ok(body)) => Ok(body),
        Some(Err(error)) => Err(fetch_failure(what, error)),
        None => Err(fetch_failure(what, GeoFetchError::BadResponse)),
    }
}

/// Drive a [`GeoTerrainJob`]: once all four answers have settled, decode
/// them on the compute pool; once decoded, hand the heightmap and its
/// ground on as a finished [`TerrainTask`]. A terrain failure falls back to
/// the procedural ground, a land-use failure lands the terrain without its
/// ground, and either says so ([`GeoTerrainFallback`]). The task is logged
/// as the heightmap offload it is or stands in for, so the stall rule pairs
/// it with its completion.
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
            requests,
            grid,
            cell,
            started,
            source,
            fallback,
        } => {
            let (requests, grid, cell, started) = (*requests, *grid, *cell, *started);
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
            // `is_settled` reads through `Deref`: no change tick until all
            // four answers are here and are taken.
            if !requests.ids().into_iter().all(|id| fetcher.is_settled(id)) {
                return;
            }
            let terrain = taken(fetcher, requests.terrain_legend, "terrain")
                .and_then(|legend| Ok((legend, taken(fetcher, requests.terrain, "terrain")?)));
            let land_use = taken(fetcher, requests.land_use_legend, "land use")
                .and_then(|legend| Ok((legend, taken(fetcher, requests.land_use, "land use")?)));
            // Whatever failed, nothing of the job stays behind.
            for id in requests.ids() {
                fetcher.forget(id);
            }
            match terrain {
                Ok((terrain_legend, terrain)) => {
                    let (source, fallback) = (source.clone(), fallback.clone());
                    let bodies = CoreBodies {
                        terrain_legend,
                        terrain,
                        land_use,
                    };
                    let task = AsyncComputeTaskPool::get()
                        .spawn(async move { decode_core(&bodies, grid, cell) });
                    *job = GeoTerrainJob::Decoding {
                        task,
                        started,
                        source,
                        fallback,
                    };
                }
                Err(reason) => fall_back(
                    &mut commands,
                    log,
                    now,
                    fallback,
                    started,
                    source.clone(),
                    reason,
                ),
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
                Ok(GeoCore { heights, ground }) => {
                    let ground = ground
                        .map_err(|reason| {
                            warn!("geodata ground: {reason} - colouring Berlin by height");
                            commands.insert_resource(GeoTerrainFallback {
                                reason,
                                lost: GeoLoss::LandUse,
                            });
                        })
                        .ok();
                    let landed = AsyncComputeTaskPool::get()
                        .spawn(async move { crate::offload::GenResult::Heightmap(heights) });
                    land(
                        &mut commands,
                        log,
                        now,
                        landed,
                        *started,
                        source.clone(),
                        ground,
                    );
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

/// Hand `task` on as the terrain task, with the `ground` its heightmap
/// lands beside, logged as the heightmap offload it is or stands in for.
fn land(
    commands: &mut Commands,
    log: &mut crate::diagnostics::SessionLog,
    now: f64,
    task: Task<crate::offload::GenResult>,
    started: f64,
    source: Option<String>,
    ground: Option<GeoGround>,
) {
    log.info(
        now,
        crate::diagnostics::event::EventPayload::OffloadJobStarted {
            job: "heightmap".into(),
        },
    );
    commands.insert_resource(TerrainTask(task, started, source, ground));
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
    commands.insert_resource(GeoTerrainFallback {
        reason,
        lost: GeoLoss::Terrain,
    });
    let task = crate::offload::offload(crate::offload::GenJob::Heightmap(fallback.clone()));
    land(commands, log, now, task, started, source, None);
}

/// Say in game why a Berlin region shows less of Berlin than its record
/// asks for: the loading screen's terrain row says it for a fallback during
/// loading.
pub(super) fn announce_geo_terrain_fallback(
    fallback: Res<GeoTerrainFallback>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
) {
    toasts.warn(
        format!("{} {}", fallback.reason, fallback.lost.instead()),
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

    /// The recorded GDI Berlin answers the `geodata` crate's decode tests
    /// use: the two legends; a 256 x 256 terrain render of E 392000-393024,
    /// N 5820000-5821024 (Friedrichshain, 4 m pixels); and 300 x 300
    /// terrain and land-use renders of E 391200-391800, N 5819700-5820300
    /// (the Museumsinsel, the Spree through it, 2 m pixels).
    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/crates/geodata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    const TERRAIN_LEGEND: &str = "dgm1_legend.json";
    const LAND_USE_LEGEND: &str = "landuse_legend.json";
    const MUSEUM_TERRAIN: &str = "dgm1_391200_5819700_600m_300px.png";
    const MUSEUM_LAND_USE: &str = "landuse_391200_5819700_600m_300px.png";

    /// The square whose core, on a 300-point grid 2 m apart, is exactly the
    /// recorded Museumsinsel renders' box.
    fn museum_square() -> GeoSquare {
        GeoSquare {
            min_e: 391_200,
            min_n: 5_819_700,
            size_m: 600,
        }
    }

    fn museum_config() -> SovereignTerrainConfig {
        SovereignTerrainConfig {
            grid_size: 300,
            cell_scale: Fp(2.0),
            ..Default::default()
        }
    }

    /// The Museumsinsel's four answers, as the job hands them to the decode.
    fn museum_bodies() -> CoreBodies {
        CoreBodies {
            terrain_legend: fixture(TERRAIN_LEGEND).into(),
            terrain: fixture(MUSEUM_TERRAIN).into(),
            land_use: Ok((
                fixture(LAND_USE_LEGEND).into(),
                fixture(MUSEUM_LAND_USE).into(),
            )),
        }
    }

    #[test]
    fn a_core_box_centres_its_pixels_on_the_square() {
        // The square whose core, on a 256-point grid 4 m apart, is exactly
        // the recorded Friedrichshain render's box: 1,030 m, so all 256
        // points fit.
        let square = GeoSquare {
            min_e: 391_997,
            min_n: 5_819_997,
            size_m: 1_030,
        };
        let bbox = core_bbox(square, 256, 4.0);
        assert_eq!(
            (bbox.min_e, bbox.min_n, bbox.max_e, bbox.max_n),
            (392_000, 5_820_000, 393_024, 5_821_024)
        );
        // The default terrain: 512 points 2 m apart, the same metre box.
        assert_eq!(core_bbox(square, 512, 2.0), bbox);
        // A side that is not a whole number of metres rounds to one.
        let odd = core_bbox(square, 100, 1.5);
        assert_eq!(odd.max_e - odd.min_e, 150);
        assert_eq!((odd.min_e + odd.max_e) / 2, 392_512);
        // The Museumsinsel's core is the renders' box.
        let museum = core_bbox(museum_square(), 300, 2.0);
        assert_eq!(
            (museum.min_e, museum.min_n, museum.max_e, museum.max_n),
            (391_200, 5_819_700, 391_800, 5_820_300)
        );
        assert_eq!(core_grid(600, &museum_config()), (300, 2.0));
    }

    #[test]
    fn the_recorded_render_decodes_to_the_core_heightmap() {
        let data = decode_heights(
            &fixture(TERRAIN_LEGEND),
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
        let wrong = decode_heights(
            &fixture(TERRAIN_LEGEND),
            &fixture("dgm1_392000_5820000_1024m_256px.png"),
            512,
            2.0,
        );
        assert!(wrong.unwrap_err().contains("could not be read"));
    }

    #[test]
    fn the_museumsinsel_decodes_with_its_land_use_and_the_spree_settled() {
        let core = decode_core(&museum_bodies(), 300, 2.0).unwrap();
        let raw =
            decode_heights(&fixture(TERRAIN_LEGEND), &fixture(MUSEUM_TERRAIN), 300, 2.0).unwrap();
        let ground = core.ground.expect("the land use decodes");
        let level = ground.water_level().expect("the Spree runs through it");
        assert!((30.3..30.8).contains(&level), "level {level}");
        // The heights are the terrain's, with the water settled on them
        // where the land use maps it (the `geodata` crate's own tests hold
        // `settle` to its rules on these renders).
        let half = 299.0;
        let wet: Vec<bool> = (0..300 * 300)
            .map(|i| {
                let (x, z) = ((i % 300) as f32 * 2.0 - half, (i / 300) as f32 * 2.0 - half);
                ground.cover_at(x, z) == Some(geodata::berlin::LandUse::Water)
            })
            .collect();
        let mut settled = raw.data.clone();
        let water = geodata::water::settle(&mut settled, &wet, 300, 300, 2.0).unwrap();
        assert_eq!(water.level, level);
        assert_eq!(core.heights.data, settled);
        // A land use this build cannot read keeps the terrain as drawn.
        let mut bodies = museum_bodies();
        bodies.land_use = Ok((
            fixture(LAND_USE_LEGEND).into(),
            fixture(MUSEUM_TERRAIN)[..64].into(),
        ));
        let core = decode_core(&bodies, 300, 2.0).unwrap();
        assert!(
            core.ground
                .unwrap_err()
                .contains("land use could not be read")
        );
        assert_eq!(core.heights.data, raw.data);
    }

    /// Serves the recorded Museumsinsel answers, or a status in place of
    /// the terrain's render or the land use's.
    struct Recorded {
        terrain_status: Option<u16>,
        land_use_status: Option<u16>,
    }

    impl Recorded {
        fn answering() -> Self {
            Recorded {
                terrain_status: None,
                land_use_status: None,
            }
        }
    }

    impl GeoTransport for Recorded {
        fn get(&self, url: String, _cap: usize) -> GetFuture {
            let legend = |layer| GeoRequest::legend(layer, 0).url().to_owned();
            let bbox = core_bbox(museum_square(), 300, 2.0);
            let render = |layer| GeoRequest::render(layer, bbox, 300, 300).url().to_owned();
            let (terrain, land_use) = (&geodata::berlin::TERRAIN, &geodata::berlin::LAND_USE);
            let answer = |status: Option<u16>, name| match status {
                Some(status) => Err(AssetFetchError::HttpStatus(status)),
                None => Ok(fixture(name)),
            };
            let body = if url == legend(terrain) {
                Ok(fixture(TERRAIN_LEGEND))
            } else if url == legend(land_use) {
                Ok(fixture(LAND_USE_LEGEND))
            } else if url == render(terrain) {
                answer(self.terrain_status, MUSEUM_TERRAIN)
            } else if url == render(land_use) {
                answer(self.land_use_status, MUSEUM_LAND_USE)
            } else {
                Err(AssetFetchError::HttpStatus(404))
            };
            Box::pin(async move { body.map(|bytes| (bytes, url)) })
        }
    }

    fn museum_record() -> RoomRecord {
        let mut generators = HashMap::new();
        generators.insert(
            "base_terrain".to_owned(),
            Generator::from_kind(GeneratorKind::Terrain(museum_config())),
        );
        RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Environment::default(),
            generators,
            placements: Vec::new(),
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            geo_source: Some(GeoSource::berlin(museum_square())),
            opaque_refs: Default::default(),
        }
    }

    /// The terrain pipeline's start, geodata and landing systems over a
    /// record built from the Museumsinsel, fetching through `transport`.
    fn berlin_app(transport: Recorded) -> App {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = App::new();
        app.init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .init_resource::<crate::diagnostics::SessionLog>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .insert_resource(LiveRoomRecord(museum_record()))
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

    /// Run `transport`'s Berlin region until its ground lands.
    fn landed(transport: Recorded) -> App {
        let mut app = berlin_app(transport);
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
        assert!(
            world.resource::<GeoFetcher>().is_idle(),
            "nothing of the job stays behind in the fetcher"
        );
        app
    }

    #[test]
    fn a_berlin_region_lands_its_terrain_with_its_ground() {
        let app = landed(Recorded::answering());
        let world = app.world();
        assert!(!world.contains_resource::<GeoTerrainFallback>());
        let finished = world.resource::<super::super::FinishedHeightMap>();
        let hm = &finished.0;
        assert_eq!((hm.width(), hm.height(), hm.scale()), (300, 300, 2.0));
        let expected = decode_core(&museum_bodies(), 300, 2.0).unwrap();
        assert_eq!(
            hm.data(),
            &expected.heights.data[..],
            "the decoded, settled heights, bit for bit"
        );
        // The ground rides with them, the Spree's level in it.
        let ground = finished.ground().expect("Berlin's ground lands with it");
        assert_eq!(Some(ground), expected.ground.as_ref().ok());
        let level = ground.water_level().expect("the Spree");
        // The Spree off the island's northern tip is under the level; the
        // Lustgarten (mapped as a city square) and the Marx-Engels-Forum
        // across the river (a park) stand above it, and only the park is
        // ground a stand of trees may take.
        use geodata::berlin::LandUse;
        let (spree, lustgarten, forum) = ((-150.0, -230.0), (-123.0, 61.0), (181.0, 85.0));
        assert_eq!(ground.cover_at(spree.0, spree.1), Some(LandUse::Water));
        assert_eq!(
            ground.cover_at(lustgarten.0, lustgarten.1),
            Some(LandUse::Square)
        );
        assert_eq!(ground.cover_at(forum.0, forum.1), Some(LandUse::Park));
        assert!(finished.world_height_at(spree.0, spree.1) < level);
        assert!(finished.world_height_at(lustgarten.0, lustgarten.1) > level);
        assert!(finished.world_height_at(forum.0, forum.1) > level);
        assert_eq!(ground.scatter_layer_at(forum.0, forum.1), 0);
        assert_eq!(
            ground.scatter_layer_at(lustgarten.0, lustgarten.1),
            ground::NOT_NATURAL
        );
        let record = &world.resource::<LiveRoomRecord>().0;
        assert_eq!(
            world.resource::<super::super::HeightMapSource>().0,
            super::super::terrain_source_key(record)
        );
    }

    #[test]
    fn a_map_service_failure_falls_back_to_the_procedural_ground_and_says_why() {
        let app = landed(Recorded {
            terrain_status: Some(404),
            land_use_status: None,
        });
        let world = app.world();
        let fallback = world.resource::<GeoTerrainFallback>();
        assert_eq!(fallback.lost, GeoLoss::Terrain);
        assert!(
            fallback.reason.contains("terrain could not be fetched")
                && fallback.reason.contains("404"),
            "{}",
            fallback.reason
        );
        // The ground is the record's own procedural terrain, bit for bit,
        // with none of Berlin's ground.
        let record = &world.resource::<LiveRoomRecord>().0;
        let procedural = super::super::heightmap::heightmap_params(&museum_config());
        let crate::offload::GenResult::Heightmap(expected) =
            crate::offload::GenJob::Heightmap(procedural).run()
        else {
            unreachable!("a heightmap job yields a heightmap")
        };
        let finished = world.resource::<super::super::FinishedHeightMap>();
        assert_eq!(finished.0.data(), &expected.data[..]);
        assert!(finished.ground().is_none());
        assert_eq!(
            world.resource::<super::super::HeightMapSource>().0,
            super::super::terrain_source_key(record)
        );
    }

    #[test]
    fn a_land_use_failure_keeps_berlins_terrain_and_says_so() {
        // A status the fetcher does not retry: this app's clock stands still.
        let app = landed(Recorded {
            terrain_status: None,
            land_use_status: Some(404),
        });
        let world = app.world();
        let fallback = world.resource::<GeoTerrainFallback>();
        assert_eq!(fallback.lost, GeoLoss::LandUse);
        assert!(
            fallback.reason.contains("land use could not be fetched")
                && fallback.reason.contains("404"),
            "{}",
            fallback.reason
        );
        // Berlin's terrain as drawn: no ground, no water settled on it.
        let raw =
            decode_heights(&fixture(TERRAIN_LEGEND), &fixture(MUSEUM_TERRAIN), 300, 2.0).unwrap();
        let finished = world.resource::<super::super::FinishedHeightMap>();
        assert_eq!(finished.0.data(), &raw.data[..]);
        assert!(finished.ground().is_none());
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
        let record = museum_record();
        let task = AsyncComputeTaskPool::get()
            .spawn(async move { decode_core(&museum_bodies(), 300, 2.0) });
        while !task.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut app = App::new();
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
                fallback: super::super::heightmap::heightmap_params(&museum_config()),
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
            ..museum_square()
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
        let mut fetcher = GeoFetcher::new(GeoStore::Off, Arc::new(Recorded::answering()));
        let job = start(&mut fetcher, museum_square(), &museum_config(), 0.0, None);
        app.init_resource::<TerrainSplatState>()
            .init_resource::<RoadRebuild>()
            .init_resource::<RoadPanelStats>()
            .init_resource::<LastTerrainConfigJson>()
            .init_resource::<PendingTerrainConfigJson>()
            .insert_resource(fetcher)
            .insert_resource(job)
            .insert_resource(GeoTerrainFallback {
                reason: "earlier".into(),
                lost: GeoLoss::LandUse,
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
        let mut record = museum_record();
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
        let mut fetcher = GeoFetcher::new(GeoStore::Off, Arc::new(Recorded::answering()));
        let job = start(&mut fetcher, museum_square(), &museum_config(), 0.0, None);
        assert!(!fetcher.is_idle());
        job.abandon(Some(&mut fetcher));
        assert!(
            fetcher.is_idle(),
            "nothing is fetched for a terrain nobody waits on"
        );
    }
}
