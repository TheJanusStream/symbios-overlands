//! Terrain from real Berlin (#1584, epic #1580; design in
//! `docs/geodata.md`): a geodata region's ground, at real scale and real
//! altitude, decoded from GDI Berlin's terrain layer, and coloured and
//! watered from its land-use layer (#1586).
//!
//! The procedural pipeline runs one offloaded heightmap job. A region whose
//! record carries a Berlin [`crate::pds::GeoSource`] runs this instead. The
//! ground's answers are fetched through the [`GeoFetcher`] and its cache -
//! the terrain layer's legend and one render of the square, the land-use
//! layer's legend and one render of the same box, a page each of its street
//! and carriageway axes ([`streets`], #1595), and a page each of its
//! buildings, blocks, trees and kinds of street furniture ([`street_level`],
//! #1588), over each tile of a square wider than a kilometre
//! ([`street_tiles`], #1637) - decoded on the compute pool, and handed to the procedural
//! pipeline's own landing as a finished [`TerrainTask`]. So the world
//! digest, the session log, the mesh, the collider and the swap of an
//! outgoing terrain are the procedural ones.
//!
//! The ground is the whole square, walked at full detail to its edge (a
//! Kiez, #1637): the terrain config's `grid_size` points, spaced to span the
//! square's side and no closer than Berlin's terrain model resolves
//! ([`core_grid`]). A region is its square, and a small one is a small
//! world. So the ground always lies inside Berlin, where the data is.
//!
//! Heights are metres above sea level (DHHN2016), as the city measures
//! them, with no datum shift. The land use rides with them as a
//! [`GeoGround`] ([`ground`]): it paints the splat layers and settles the
//! water - the region's water level is where it draws its water plane, its
//! beds are carved below it and the rest of the ground is kept above it -
//! and carries the meshed streets and the street level with it.
//!
//! If Berlin's terrain cannot be had - the service is unreachable after the
//! fetcher's retries, or answers something that does not decode - the
//! region falls back to the procedural ground its terrain config describes,
//! and [`GeoTerrainFallback`] says why: on the loading screen's terrain row,
//! and in a toast when it happens in game. A visitor is never left on a
//! loading screen, or on stale ground, waiting for a service that is down.
//! If only the land use cannot be had, Berlin's terrain still lands, its
//! colours following the record's altitude bands and with no water, and
//! the fallback says that instead; so it does where only the streets or the
//! street level cannot be had.

mod ground;
pub(crate) mod street_level;
pub(crate) mod streets;

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

/// The closest the ground's points stand (m): Berlin's terrain model
/// (`c_dgm1`) is a one-metre grid, so a closer spacing draws nothing more
/// of it (#1637).
const MIN_CORE_CELL_M: f32 = 1.0;

/// The widest box a street-level page is asked over (m): a page's count
/// holds the densest square kilometre of central Berlin measured - 1,281
/// buildings, 2,695 park trees, 1,765 bollards - so a square wider than
/// this is asked in tiles ([`street_tiles`], #1637), and one this wide or
/// less in one box, as before. A page cut short draws an arbitrary part of
/// its layer: at 2 km in one box, 3,000 of 6,418 buildings round
/// Hermannplatz, holes beside the landing among them.
const STREET_TILE_M: i64 = 1_024;

/// The requests a core is built from: the terrain's and the land use's
/// legend and render over the core's box, its street and carriageway axes
/// (#1595), and its street level's pages (#1588).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoreRequests {
    /// The core's box, which the streets are read in.
    bbox: Bbox,
    terrain_legend: GeoRequestId,
    terrain: GeoRequestId,
    land_use_legend: GeoRequestId,
    land_use: GeoRequestId,
    street_axes: GeoRequestId,
    carriageway_axes: GeoRequestId,
    street_level: StreetLevelRequests,
}

/// The street level's requests (#1588): one set of pages a tile of the
/// core's box ([`street_tiles`], #1637).
#[derive(Clone, Debug, PartialEq)]
struct StreetLevelRequests {
    tiles: Vec<TilePages>,
}

impl StreetLevelRequests {
    fn submit(fetcher: &mut GeoFetcher, bbox: Bbox) -> Self {
        StreetLevelRequests {
            tiles: street_tiles(bbox)
                .into_iter()
                .map(|tile| TilePages::submit(fetcher, tile))
                .collect(),
        }
    }

    fn ids(&self) -> impl Iterator<Item = GeoRequestId> + use<> {
        self.tiles
            .iter()
            .flat_map(TilePages::ids)
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// One page of every tile, `page` picking it.
    fn each(&self, page: impl Fn(&TilePages) -> GeoRequestId) -> Vec<GeoRequestId> {
        self.tiles.iter().map(page).collect()
    }
}

/// The tiles a street level's pages are asked over: `bbox` cut into as few
/// equal squares a side as keep each within [`STREET_TILE_M`], row by row
/// from the south-west - one, `bbox` itself, for a square of a kilometre or
/// so. Their edges are whole metres and meet exactly, so every point of the
/// box lies in a tile.
fn street_tiles(bbox: Bbox) -> Vec<Bbox> {
    let (width, height) = (bbox.max_e - bbox.min_e, bbox.max_n - bbox.min_n);
    let n = (width.max(height) + STREET_TILE_M - 1)
        .div_euclid(STREET_TILE_M)
        .max(1);
    let cut = |from: i64, span: i64, i: i64| from + span * i / n;
    (0..n)
        .flat_map(|row| (0..n).map(move |col| (row, col)))
        .map(|(row, col)| Bbox {
            min_e: cut(bbox.min_e, width, col),
            min_n: cut(bbox.min_n, height, row),
            max_e: cut(bbox.min_e, width, col + 1),
            max_n: cut(bbox.min_n, height, row + 1),
        })
        .collect()
}

/// One tile's street-level pages: its buildings, its blocks by
/// urban-structure type (#1600), each tree inventory, and each furniture
/// kind, in [`geodata::berlin::FurnitureKind::ALL`]'s order.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TilePages {
    buildings: GeoRequestId,
    blocks: GeoRequestId,
    street_trees: GeoRequestId,
    park_trees: GeoRequestId,
    furniture: [GeoRequestId; 9],
}

impl TilePages {
    /// The street level's pages over `bbox`, in the order of the struct's
    /// fields: the buildings', the blocks', the street trees', the park
    /// trees', then each furniture kind's, every attribute.
    fn requests(bbox: Bbox) -> Vec<GeoRequest> {
        use geodata::berlin::{self, FurnitureKind};
        let page = |layer: &geodata::request::WfsType, properties: &[&str], count: u32| {
            GeoRequest::features(
                layer,
                bbox,
                &geodata::request::FeatureQuery {
                    properties,
                    count: Some(count),
                    start_index: None,
                },
            )
        };
        let trees = (berlin::TREE_PROPERTIES, berlin::TREE_PAGE);
        [
            page(
                &berlin::BUILDINGS,
                berlin::BUILDING_PROPERTIES,
                berlin::BUILDING_PAGE,
            ),
            page(
                &berlin::URBAN_STRUCTURE,
                berlin::BLOCK_PROPERTIES,
                berlin::BLOCK_PAGE,
            ),
            page(&berlin::STREET_TREES, trees.0, trees.1),
            page(&berlin::PARK_TREES, trees.0, trees.1),
        ]
        .into_iter()
        .chain(FurnitureKind::ALL.map(|kind| page(&kind.layer(), &[], berlin::FURNITURE_PAGE)))
        .collect()
    }

    fn submit(fetcher: &mut GeoFetcher, bbox: Bbox) -> Self {
        let mut ids = Self::requests(bbox)
            .into_iter()
            .map(|request| fetcher.submit(request));
        let mut next = || ids.next().expect("a request per page");
        // A struct's fields are evaluated as written: in the requests' order.
        TilePages {
            buildings: next(),
            blocks: next(),
            street_trees: next(),
            park_trees: next(),
            furniture: std::array::from_fn(|_| next()),
        }
    }

    fn ids(&self) -> impl Iterator<Item = GeoRequestId> + use<> {
        [
            self.buildings,
            self.blocks,
            self.street_trees,
            self.park_trees,
        ]
        .into_iter()
        .chain(self.furniture)
    }
}

impl CoreRequests {
    /// Submit the requests for a core of `grid` points over `bbox`.
    fn submit(fetcher: &mut GeoFetcher, bbox: Bbox, grid: u32) -> Self {
        let (terrain, land_use) = (&geodata::berlin::TERRAIN, &geodata::berlin::LAND_USE);
        CoreRequests {
            bbox,
            terrain_legend: fetcher.submit(GeoRequest::legend(terrain, 0)),
            terrain: fetcher.submit(GeoRequest::render(terrain, bbox, grid, grid)),
            land_use_legend: fetcher.submit(GeoRequest::legend(land_use, 0)),
            land_use: fetcher.submit(GeoRequest::render(land_use, bbox, grid, grid)),
            street_axes: fetcher.submit(axes_request(&geodata::berlin::STREET_AXES, bbox)),
            carriageway_axes: fetcher
                .submit(axes_request(&geodata::berlin::CARRIAGEWAY_AXES, bbox)),
            street_level: StreetLevelRequests::submit(fetcher, bbox),
        }
    }

    /// The walkable ground's own requests.
    fn core_ids(&self) -> Vec<GeoRequestId> {
        [
            self.terrain_legend,
            self.terrain,
            self.land_use_legend,
            self.land_use,
            self.street_axes,
            self.carriageway_axes,
        ]
        .into_iter()
        .chain(self.street_level.ids())
        .collect()
    }

    /// Every request, for counting and forgetting.
    pub(crate) fn ids(&self) -> impl Iterator<Item = GeoRequestId> + use<> {
        self.core_ids().into_iter()
    }
}

/// Why a street level was left out, wholly or in part, as one reason: the
/// first layer's, and how many more were left out; `None` where none was.
fn street_level_loss(lost: &[String]) -> Option<String> {
    let (first, rest) = lost.split_first()?;
    Some(match rest.len() {
        0 => first.clone(),
        n => format!("{first} {n} more of its street level's layers could not be had either."),
    })
}

/// The page of `axes` over the core's `bbox` (#1595).
fn axes_request(axes: &geodata::request::WfsType, bbox: Bbox) -> GeoRequest {
    GeoRequest::features(
        axes,
        bbox,
        &geodata::request::FeatureQuery {
            properties: geodata::berlin::AXIS_PROPERTIES,
            count: Some(geodata::berlin::AXIS_PAGE),
            start_index: None,
        },
    )
}

/// A geodata terrain on its way to becoming the [`TerrainTask`].
#[derive(Resource)]
pub(crate) enum GeoTerrainJob {
    /// Waiting for the answers.
    Fetching {
        requests: Box<CoreRequests>,
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
    /// Only its land use: Berlin's terrain, coloured by height, dry, with
    /// nothing on it.
    LandUse,
    /// Only the streets: the walkable ground is Berlin's, its streets
    /// painted on it but not built (#1595).
    Streets,
    /// Only the street level, or some of its layers: the walkable ground is
    /// Berlin's, with none, or not all, of its buildings, trees and street
    /// furniture (#1588).
    StreetLevel,
}

impl GeoLoss {
    /// Whether the region's ground itself stands in for Berlin's (#1593):
    /// its procedural heights, or Berlin's heights uncarved at the record's
    /// own water line - not the ground a saved road district was grown on.
    /// Losing the streets or the street level leaves Berlin's ground.
    pub(crate) fn stands_in_for_the_ground(self) -> bool {
        matches!(self, GeoLoss::Terrain | GeoLoss::LandUse)
    }

    /// What the region shows instead, as a sentence.
    pub(crate) fn instead(self) -> &'static str {
        match self {
            GeoLoss::Terrain => {
                "The ground is drawn from the world's terrain settings until it can be fetched."
            }
            GeoLoss::LandUse => {
                "Berlin's terrain stands alone, coloured by height - no water, streets or \
                 buildings - until its land use can be fetched."
            }
            GeoLoss::Streets => {
                "Berlin's streets are painted on the ground, not built, until they can be \
                 fetched - and a world not seeded yet stands bare until then, as they face its \
                 buildings and street furniture."
            }
            GeoLoss::StreetLevel => {
                "Berlin's buildings, trees and street furniture are seeded into a world from its \
                 whole street level, so one not seeded yet stands bare until all of it can be \
                 fetched."
            }
        }
    }
}

/// A layer's legend and render, as fetched.
pub(crate) type LayerBodies = (Arc<[u8]>, Arc<[u8]>);

/// The answers' bodies, as fetched.
pub(crate) struct CoreBodies {
    /// The core's box.
    pub bbox: Bbox,
    pub terrain_legend: Arc<[u8]>,
    pub terrain: Arc<[u8]>,
    /// The land use's legend and render, or why they could not be had.
    pub land_use: Result<LayerBodies, String>,
    /// The street and carriageway axes over the core, or why they could not
    /// be had; `None` where they were not asked for.
    pub streets: Option<Result<LayerBodies, String>>,
    /// The street level's pages over the core, each or why it could not be
    /// had; `None` where they were not asked for.
    pub street_level: Option<street_level::StreetLevelBodies>,
}

/// A decoded core: its heights, and what covers them - or why the land use
/// could not be read, in which case the heights are the terrain's own,
/// with no water settled on them - and, when it has no streets, or not all
/// of its street level, why.
pub(crate) struct GeoCore {
    pub heights: gen_jobs::HeightmapData,
    pub ground: Result<GeoGround, String>,
    pub streets: Option<String>,
    pub street_level: Option<String>,
}

/// The ground's grid for a square of side `size_m`: the terrain config's
/// `grid_size` points, spaced so the render - `grid * cell` metres - spans
/// the square (#1637), but no closer than [`MIN_CORE_CELL_M`], where a
/// small square takes fewer points instead. The config's `cell_scale` is
/// not read: the square sets the spacing, as Berlin sets the heights.
pub(crate) fn core_grid(size_m: u32, cfg: &SovereignTerrainConfig) -> (u32, f32) {
    let points = cfg.grid_size.clamp(2, MAX_CORE_GRID);
    let cell = (size_m as f32 / points as f32).max(MIN_CORE_CELL_M);
    let grid = ((f64::from(size_m) / f64::from(cell)).round() as u32).clamp(2, MAX_CORE_GRID);
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
        requests: Box::new(CoreRequests::submit(
            fetcher,
            core_bbox(square, grid, cell),
            grid,
        )),
        grid,
        cell,
        started: now,
        source,
        fallback: super::heightmap::heightmap_params(cfg),
    }
}

/// Decode a core: `grid` x `grid` heights, metres above sea level, `cell`
/// metres apart, row 0 north - and, where the land use was had, what covers
/// them, with the core's water settled on the heights, its streets and its
/// street level. An error is the terrain's: it is what the core cannot be
/// built without.
pub(crate) fn decode_core(bodies: &CoreBodies, grid: u32, cell: f32) -> Result<GeoCore, String> {
    let mut heights = decode_heights(&bodies.terrain_legend, &bodies.terrain, grid, cell)?;
    // The streets are read first: the water runs on under the bridges they
    // cross, which the settle must know (#1595).
    let frame = streets::CoreFrame {
        bbox: bodies.bbox,
        grid,
        cell,
    };
    let read = bodies.streets.as_ref().map(|bodies| {
        bodies
            .as_ref()
            .map_err(String::clone)
            .and_then(|(axes, carriageways)| read_axes(axes, carriageways))
    });
    let bridges = match &read {
        Some(Ok(read)) => Some(streets::street_cells(read, frame)),
        _ => None,
    };
    let raw = bridges.as_ref().map(|_| heights.data.clone());
    let mut ground = match &bodies.land_use {
        Ok((legend, render)) => ground::decode_ground(
            legend,
            render,
            &mut heights.data,
            grid,
            cell,
            bridges.as_deref(),
        ),
        Err(reason) => Err(reason.clone()),
    };
    let (mut streets, mut street_level) = (None, None);
    if let (Ok(ground), Some(bodies)) = (ground.as_mut(), &bodies.street_level) {
        // Each layer is read on its own, and one that could not be had
        // leaves the rest read; but a level that left one out is never
        // seeded into a record (#1647), nor one whose streets could not be
        // had, which face its buildings and street furniture: a later visit
        // that has them all seeds it.
        let axes = read.as_ref().and_then(|read| read.as_ref().ok());
        let (mut level, lost) =
            street_level::decode_street_level(bodies, frame, ground.cover(), axes);
        if !lost.is_empty() || matches!(read, Some(Err(_))) {
            level.mark_incomplete();
        }
        if !level.is_empty() {
            ground.set_street_level(level);
        }
        street_level = street_level_loss(&lost);
    }
    if let Ok(ground) = ground.as_mut() {
        match (read, raw) {
            (Some(Ok(read)), Some(raw)) => {
                let road_ground = streets::road_ground(
                    &raw,
                    &heights.data,
                    grid,
                    cell,
                    &ground.wet(bridges.as_deref()),
                    |i| ground.level_over(((i as u32) % grid, (i as u32) / grid)),
                );
                if let Some(parts) = streets::mesh_streets(&read, frame, &road_ground) {
                    ground.set_streets(parts);
                }
            }
            (Some(Err(reason)), _) => streets = Some(reason),
            _ => {}
        }
    }
    Ok(GeoCore {
        heights,
        ground,
        streets,
        street_level,
    })
}

/// Read the street and carriageway axes (#1595). A page the server cut
/// short still draws the streets it holds.
fn read_axes(axes: &[u8], carriageways: &[u8]) -> Result<streets::Streets, String> {
    let read = |body: &[u8]| {
        let page = geodata::berlin::parse_axes(body)
            .map_err(|e| format!("Berlin's streets could not be read: {e}"))?;
        if page.is_cut_short() {
            warn!(
                "geodata streets: {} of {:?} axes in one page - drawing those",
                page.axes.len(),
                page.matched
            );
        }
        Ok::<_, String>(page.axes)
    };
    Ok(streets::Streets {
        axes: read(axes)?,
        carriageways: read(carriageways)?,
    })
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
        bbox,
        terrain_legend: get("terrain", GeoRequest::legend(terrain, 0))?,
        terrain: get("terrain", GeoRequest::render(terrain, bbox, grid, grid))?,
        land_use: Ok((
            get("land use", GeoRequest::legend(land_use, 0))?,
            get("land use", GeoRequest::render(land_use, bbox, grid, grid))?,
        )),
        // A tool reads the ground; the streets and the street level are only
        // drawn.
        streets: None,
        street_level: None,
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

/// The ground's answers to `requests`, taken - the terrain's, the land
/// use's, the streets' and the street level's, each or why it could not be
/// had - as the bodies a decode reads; or why the terrain could not be had,
/// which nothing is drawn without. Every answer is taken, whatever failed.
fn take_walkable(fetcher: &mut GeoFetcher, requests: &CoreRequests) -> Result<CoreBodies, String> {
    let terrain = taken(fetcher, requests.terrain_legend, "terrain")
        .and_then(|legend| Ok((legend, taken(fetcher, requests.terrain, "terrain")?)));
    let land_use = taken(fetcher, requests.land_use_legend, "land use")
        .and_then(|legend| Ok((legend, taken(fetcher, requests.land_use, "land use")?)));
    let streets = taken(fetcher, requests.street_axes, "streets")
        .and_then(|axes| Ok((axes, taken(fetcher, requests.carriageway_axes, "streets")?)));
    // Each street-level page, every tile's, or why it could not be had.
    let street_level = {
        let level = &requests.street_level;
        let mut take = |ids: Vec<GeoRequestId>, what: &str| {
            ids.into_iter()
                .map(|id| taken(fetcher, id, what))
                .collect::<Vec<_>>()
        };
        street_level::StreetLevelBodies {
            buildings: take(level.each(|tile| tile.buildings), "buildings"),
            blocks: take(level.each(|tile| tile.blocks), "block types"),
            street_trees: take(level.each(|tile| tile.street_trees), "street trees"),
            park_trees: take(level.each(|tile| tile.park_trees), "park trees"),
            furniture: geodata::berlin::FurnitureKind::ALL
                .into_iter()
                .enumerate()
                .map(|(k, kind)| {
                    (
                        kind,
                        take(level.each(|tile| tile.furniture[k]), kind.name()),
                    )
                })
                .collect(),
        }
    };
    let (terrain_legend, terrain) = terrain?;
    Ok(CoreBodies {
        bbox: requests.bbox,
        terrain_legend,
        terrain,
        land_use,
        streets: Some(streets),
        street_level: Some(street_level),
    })
}

/// Drive a [`GeoTerrainJob`]: once the ground's answers have settled,
/// decode them on the compute pool; once decoded, hand the heightmap and
/// its ground on as a finished [`TerrainTask`]. A terrain failure falls back
/// to the procedural ground; a land-use, streets or street-level failure
/// lands the terrain with what it has; each says so
/// ([`GeoTerrainFallback`]). The task is logged as the heightmap offload it
/// is or stands in for, so the stall rule pairs it with its completion.
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
            let (grid, cell, started) = (*grid, *cell, *started);
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
            // `is_settled` reads through `Deref`: no change tick until the
            // answers are here and are taken, bar the one stamp below.
            if !requests.core_ids().iter().all(|&id| fetcher.is_settled(id)) {
                return;
            }
            // Owned only once every answer is in, not on every frame of the
            // wait: a 2 km square's tiles make it a few vectors.
            let requests = (**requests).clone();
            let walkable = take_walkable(fetcher, &requests);
            // Whatever failed, nothing of the job stays behind.
            for id in requests.ids() {
                fetcher.forget(id);
            }
            match walkable {
                Ok(bodies) => {
                    let (source, fallback) = (source.clone(), fallback.clone());
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
                Ok(GeoCore {
                    heights,
                    ground,
                    streets,
                    street_level,
                }) => {
                    let ground = match ground {
                        Ok(ground) => {
                            // Every loss is logged; the one said is the
                            // nearest the player: the streets they walk,
                            // then what stands along them.
                            let mut said = None;
                            for (reason, lost, instead) in [
                                (streets, GeoLoss::Streets, "painting the streets only"),
                                (street_level, GeoLoss::StreetLevel, "leaving that out"),
                            ] {
                                if let Some(reason) = reason {
                                    warn!("geodata: {reason} - {instead}");
                                    said.get_or_insert(GeoTerrainFallback { reason, lost });
                                }
                            }
                            if let Some(fallback) = said {
                                commands.insert_resource(fallback);
                            }
                            Some(ground)
                        }
                        Err(reason) => {
                            warn!("geodata ground: {reason} - colouring Berlin by height");
                            commands.insert_resource(GeoTerrainFallback {
                                reason,
                                lost: GeoLoss::LandUse,
                            });
                            None
                        }
                    };
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
pub(crate) mod tests {
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
    const MUSEUM_STREETS: &str = "atkis_strassenachse_391200_5819700_600m.json";
    const MUSEUM_CARRIAGEWAYS: &str = "atkis_fahrbahnachse_391200_5819700_600m.json";

    /// The street and carriageway axes' URLs over `bbox`, each with the
    /// recorded page of the Museumsinsel square: whole, it holds every axis
    /// a core inside it reads.
    fn street_answers(bbox: Bbox) -> [(String, &'static str); 2] {
        [
            (
                axes_request(&geodata::berlin::STREET_AXES, bbox)
                    .url()
                    .to_owned(),
                MUSEUM_STREETS,
            ),
            (
                axes_request(&geodata::berlin::CARRIAGEWAY_AXES, bbox)
                    .url()
                    .to_owned(),
                MUSEUM_CARRIAGEWAYS,
            ),
        ]
    }
    /// The street level's URLs over `bbox`, in the order
    /// [`TilePages::requests`] asks for them, each with the
    /// recorded page of the Museumsinsel square (#1588): whole, it holds
    /// everything a core inside it reads.
    fn street_level_answers(bbox: Bbox) -> Vec<(String, String)> {
        let names = [
            "alkis_gebaeude",
            "ua_stadtstruktur_b_stadtstruktur_differenziert_2024",
            "baumbestand_strassenbaeume",
            "baumbestand_anlagenbaeume",
        ]
        .map(str::to_owned)
        .into_iter()
        .chain(
            geodata::berlin::FurnitureKind::ALL
                .map(|kind| kind.layer().type_name.replace(':', "_")),
        );
        TilePages::requests(bbox)
            .into_iter()
            .zip(names)
            .map(|(request, name)| {
                (
                    request.url().to_owned(),
                    format!("{name}_391200_5819700_600m.json"),
                )
            })
            .collect()
    }

    /// The Museumsinsel's street-level pages, as the job hands them on.
    fn museum_street_level() -> street_level::StreetLevelBodies {
        let bbox = core_bbox(museum_square(), 300, 2.0);
        let mut pages = street_level_answers(bbox)
            .into_iter()
            .map(|(_, name)| Ok(Arc::<[u8]>::from(fixture(&name))));
        // The square is one tile: a page a layer.
        let mut page = || vec![pages.next().expect("a page")];
        street_level::StreetLevelBodies {
            buildings: page(),
            blocks: page(),
            street_trees: page(),
            park_trees: page(),
            furniture: geodata::berlin::FurnitureKind::ALL
                .into_iter()
                .map(|kind| (kind, page()))
                .collect(),
        }
    }

    /// The Museumsinsel's street level, decoded with its ground (#1588).
    pub(crate) fn museum_level() -> Arc<street_level::StreetLevel> {
        let core = decode_core(&museum_bodies(), 300, 2.0).expect("decodes");
        core.ground
            .expect("its ground")
            .street_level()
            .cloned()
            .expect("its street level")
    }

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

    /// The Museumsinsel's answers, as the job hands them to the decode.
    pub(crate) fn museum_bodies() -> CoreBodies {
        CoreBodies {
            bbox: core_bbox(museum_square(), 300, 2.0),
            terrain_legend: fixture(TERRAIN_LEGEND).into(),
            terrain: fixture(MUSEUM_TERRAIN).into(),
            land_use: Ok((
                fixture(LAND_USE_LEGEND).into(),
                fixture(MUSEUM_LAND_USE).into(),
            )),
            streets: Some(Ok((
                fixture(MUSEUM_STREETS).into(),
                fixture(MUSEUM_CARRIAGEWAYS).into(),
            ))),
            street_level: Some(museum_street_level()),
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
        // The Spree, the largest of its bodies, at its level (#1724).
        let spree = ground
            .water()
            .iter()
            .max_by_key(|body| body.cells.mask.len())
            .expect("the Spree runs through it");
        assert!((30.3..30.8).contains(&spree.level), "level {}", spree.level);
        // The heights are the terrain's, with the water settled on them
        // where the land use maps it, and under its bridges (#1595) - the
        // `geodata` crate's own tests hold `settle` to its rules on these
        // renders.
        let read = read_axes(&fixture(MUSEUM_STREETS), &fixture(MUSEUM_CARRIAGEWAYS)).unwrap();
        let frame = streets::CoreFrame {
            bbox: core_bbox(museum_square(), 300, 2.0),
            grid: 300,
            cell: 2.0,
        };
        let wet = ground.wet(Some(&streets::street_cells(&read, frame)));
        let half = 299.0;
        let mapped = (0..300 * 300)
            .filter(|i| {
                let (x, z) = ((i % 300) as f32 * 2.0 - half, (i / 300) as f32 * 2.0 - half);
                ground.cover_at(x, z) == Some(geodata::berlin::LandUse::Water)
            })
            .count();
        let bridged = wet.iter().filter(|&&w| w).count() - mapped;
        assert!(
            bridged > 50,
            "the Spree's bridges are its water: {bridged} cells"
        );
        let mut settled = raw.data.clone();
        let water = geodata::water::settle(&mut settled, &wet, 300, 300, 2.0);
        let settled_levels: Vec<f32> = water.bodies.iter().map(|body| body.level).collect();
        let drawn_levels: Vec<f32> = ground.water().iter().map(|body| body.level).collect();
        assert_eq!(
            settled_levels, drawn_levels,
            "a body drawn for every body settled"
        );
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
    /// the terrain's render, the land use's, the streets', every street-level
    /// page's, or one furniture kind's.
    struct Recorded {
        terrain_status: Option<u16>,
        land_use_status: Option<u16>,
        streets_status: Option<u16>,
        street_level_status: Option<u16>,
        furniture_status: Option<(geodata::berlin::FurnitureKind, u16)>,
    }

    impl Recorded {
        fn answering() -> Self {
            Recorded {
                terrain_status: None,
                land_use_status: None,
                streets_status: None,
                street_level_status: None,
                furniture_status: None,
            }
        }
    }

    impl GeoTransport for Recorded {
        fn get(&self, url: String, _cap: usize) -> GetFuture {
            let legend = |layer| GeoRequest::legend(layer, 0).url().to_owned();
            let bbox = core_bbox(museum_square(), 300, 2.0);
            let render = |layer| GeoRequest::render(layer, bbox, 300, 300).url().to_owned();
            let (terrain, land_use) = (&geodata::berlin::TERRAIN, &geodata::berlin::LAND_USE);
            let answer = |status: Option<u16>, name: &str| match status {
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
            } else if let Some((_, name)) = street_answers(bbox).iter().find(|(u, _)| *u == url) {
                answer(self.streets_status, name)
            } else if let Some((_, name)) = street_level_answers(bbox)
                .into_iter()
                .find(|(u, _)| *u == url)
            {
                let kind = self.furniture_status.and_then(|(kind, status)| {
                    let layer = kind.layer().type_name.replace(':', "_");
                    name.starts_with(&layer).then_some(status)
                });
                answer(self.street_level_status.or(kind), &name)
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
        berlin_app_with(museum_record(), transport)
    }

    /// [`berlin_app`] for any record and transport.
    fn berlin_app_with(record: RoomRecord, transport: impl GeoTransport + 'static) -> App {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
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
        // The ground rides with them, the Spree's level in it, and a whole
        // street level, which seeds a record (#1647).
        let ground = finished.ground().expect("Berlin's ground lands with it");
        assert_eq!(ground, &expected.ground.clone().expect("decodes"));
        assert!(
            ground
                .street_level()
                .is_some_and(|level| level.is_complete())
        );
        // The Spree off the island's northern tip is under its level; the
        // Lustgarten (mapped as a city square) and the Marx-Engels-Forum
        // across the river (a park) stand above it, and only the park is
        // ground a stand of trees may take.
        use geodata::berlin::LandUse;
        let (spree, lustgarten, forum) = ((-150.0, -230.0), (-123.0, 61.0), (181.0, 85.0));
        let level = ground
            .water()
            .iter()
            .find(|body| body.cells.holds(spree.0, spree.1))
            .map(|body| body.level)
            .expect("the Spree");
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
        // And its streets, meshed (#1595).
        let streets = ground.streets().and_then(|s| s.with(|parts| parts.chains));
        assert!(streets.is_some_and(|chains| chains > 20));
        // And its buildings, trees and street furniture (#1588).
        let level = ground.street_level().expect("its street level");
        assert!(
            level.buildings.len() > 30 && level.trees.len() > 400 && level.furniture.len() > 600,
            "{} buildings, {} trees, {} items of furniture",
            level.buildings.len(),
            level.trees.len(),
            level.furniture.len()
        );
    }

    /// #1588: without its street level a region keeps Berlin's ground and
    /// its streets, and says its buildings, trees and furniture are left
    /// out.
    #[test]
    fn a_street_level_failure_keeps_berlins_ground_and_says_so() {
        let app = landed(Recorded {
            street_level_status: Some(404),
            ..Recorded::answering()
        });
        let world = app.world();
        assert!(world.resource::<GeoFetcher>().is_idle());
        let fallback = world.resource::<GeoTerrainFallback>();
        assert_eq!(fallback.lost, GeoLoss::StreetLevel);
        assert!(
            fallback.reason.contains("buildings could not be fetched")
                && fallback.reason.contains("404"),
            "{}",
            fallback.reason
        );
        let ground = world
            .resource::<super::super::FinishedHeightMap>()
            .ground()
            .expect("the ground is Berlin's");
        assert!(ground.streets().is_some() && ground.street_level().is_none());
    }

    /// The critic's finding (#1588): one layer that cannot be had - a survey
    /// layer withdrawn - left out the buildings and trees with it. Now it is
    /// left out alone, and said.
    #[test]
    fn one_street_level_layer_lost_leaves_the_rest_standing() {
        use geodata::berlin::FurnitureKind;
        let app = landed(Recorded {
            furniture_status: Some((FurnitureKind::Bench, 404)),
            ..Recorded::answering()
        });
        let world = app.world();
        let fallback = world.resource::<GeoTerrainFallback>();
        assert_eq!(fallback.lost, GeoLoss::StreetLevel);
        assert!(
            fallback
                .reason
                .starts_with("Berlin's benches could not be fetched")
                && fallback.reason.contains("404")
                && !fallback.reason.contains("more of its"),
            "{}",
            fallback.reason
        );
        let level = world
            .resource::<super::super::FinishedHeightMap>()
            .ground()
            .and_then(|ground| ground.street_level())
            .expect("the rest of the street level stands");
        let whole = museum_level();
        assert_eq!(level.buildings, whole.buildings);
        assert_eq!(level.trees, whole.trees);
        assert!(
            level
                .furniture
                .iter()
                .all(|f| f.kind != FurnitureKind::Bench)
        );
        assert_eq!(
            level.furniture.len(),
            whole
                .furniture
                .iter()
                .filter(|f| f.kind != FurnitureKind::Bench)
                .count()
        );
    }

    /// #1595: without its streets a region keeps Berlin's ground, its
    /// streets painted on it, and says so. Its street level is read, but
    /// seeds nothing (#1647): the streets face its buildings and furniture.
    #[test]
    fn a_streets_failure_keeps_berlins_ground_and_says_so() {
        let app = landed(Recorded {
            streets_status: Some(404),
            ..Recorded::answering()
        });
        let world = app.world();
        assert!(world.resource::<GeoFetcher>().is_idle());
        let fallback = world.resource::<GeoTerrainFallback>();
        assert_eq!(fallback.lost, GeoLoss::Streets);
        assert!(
            fallback.reason.contains("streets could not be fetched")
                && fallback.reason.contains("404"),
            "{}",
            fallback.reason
        );
        let ground = world
            .resource::<super::super::FinishedHeightMap>()
            .ground()
            .expect("the ground is Berlin's");
        assert!(!ground.water().is_empty() && ground.streets().is_none());
        let level = ground.street_level().expect("its street level is read");
        assert!(!level.buildings.is_empty());
        assert!(!level.is_complete(), "and never seeded without its streets");
    }

    /// #1595: the Museumsinsel's streets are meshed on its ground as
    /// decoded, and where they cross the Spree they are bridges: the river
    /// runs on under them, and their decks span it from quay to quay - no
    /// deck comes down to the carved bed, or sags under its quays.
    #[test]
    fn the_museumsinsel_streets_bridge_the_spree() {
        let core = decode_core(&museum_bodies(), 300, 2.0).unwrap();
        assert!(core.streets.is_none(), "{:?}", core.streets);
        let ground = core.ground.expect("the land use decodes");
        let read = read_axes(&fixture(MUSEUM_STREETS), &fixture(MUSEUM_CARRIAGEWAYS)).unwrap();
        let frame = streets::CoreFrame {
            bbox: core_bbox(museum_square(), 300, 2.0),
            grid: 300,
            cell: 2.0,
        };
        let wet = ground.wet(Some(&streets::street_cells(&read, frame)));
        // The bridges: wet cells the land use maps as street.
        let half = 299.0;
        let bridge = |x: f32, z: f32| {
            let (col, row) = ((x / 2.0).round() as usize, (z / 2.0).round() as usize);
            col < 300
                && row < 300
                && wet[row * 300 + col]
                && ground.cover_at(x - half, z - half).is_none()
        };
        let deck = ground
            .streets()
            .and_then(|s| s.with(|parts| crate::urban::to_bevy_mesh(&parts.deck)))
            .expect("the streets");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            deck.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        // Each deck stands over the body of water under it, at its level
        // (#1724).
        let over: Vec<(f32, f32)> = points
            .iter()
            .filter(|p| bridge(p[0], p[2]))
            .filter_map(|p| {
                let cell = ((p[0] / 2.0).round() as u32, (p[2] / 2.0).round() as u32);
                ground.level_over(cell).map(|level| (p[1], level))
            })
            .collect();
        assert!(over.len() > 20, "the streets cross on bridges");
        for &(deck, level) in &over {
            assert!(
                deck >= level + streets::BRIDGE_DECK_M,
                "a bridge deck at {deck}, the water at {level}"
            );
        }
        // Streets the build cannot read leave the ground without them.
        let mut bodies = museum_bodies();
        bodies.streets = Some(Ok((b"<html>".as_slice().into(), b"".as_slice().into())));
        let core = decode_core(&bodies, 300, 2.0).unwrap();
        assert!(core.streets.unwrap().contains("streets could not be read"));
        assert!(core.ground.unwrap().streets().is_none());
    }

    #[test]
    fn a_map_service_failure_falls_back_to_the_procedural_ground_and_says_why() {
        let app = landed(Recorded {
            terrain_status: Some(404),
            ..Recorded::answering()
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
            land_use_status: Some(404),
            ..Recorded::answering()
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

    /// #1637: a square past a kilometre asks its street level in tiles, so
    /// no page is cut short and leaves an arbitrary hole; a kilometre or so
    /// asks in one box, as before. The tiles cover the box exactly.
    #[test]
    fn a_square_past_a_kilometre_is_asked_in_tiles() {
        let bbox = |side: i64| Bbox {
            min_e: 391_000,
            min_n: 5_819_000,
            max_e: 391_000 + side,
            max_n: 5_819_000 + side,
        };
        assert_eq!(street_tiles(bbox(1_000)), vec![bbox(1_000)]);
        assert_eq!(street_tiles(bbox(STREET_TILE_M)), vec![bbox(STREET_TILE_M)]);
        for side in [1_500, 2_000] {
            let whole = bbox(side);
            let tiles = street_tiles(whole);
            assert_eq!(tiles.len(), 4, "{side} m");
            let area: i64 = tiles
                .iter()
                .map(|t| (t.max_e - t.min_e) * (t.max_n - t.min_n))
                .sum();
            assert_eq!(area, side * side, "{side} m: the tiles cover it once");
            for t in &tiles {
                assert!(t.max_e - t.min_e <= STREET_TILE_M && t.max_n - t.min_n <= STREET_TILE_M);
                assert!(t.min_e >= whole.min_e && t.max_e <= whole.max_e);
                assert!(t.min_n >= whole.min_n && t.max_n <= whole.max_n);
            }
        }
    }

    /// #1637: a feature two tiles both hold - a building across their edge -
    /// is read once, by its id; and a layer one tile could not have is left
    /// out whole, and said, rather than drawn with that tile's hole.
    #[test]
    fn a_layer_is_read_once_from_its_tiles_or_left_out_whole() {
        let one = decode_core(&museum_bodies(), 300, 2.0).expect("decodes");
        let one = one.ground.expect("its ground");
        let one = one.street_level().expect("its street level");
        let mut bodies = museum_bodies();
        let level = bodies.street_level.as_mut().expect("asked for");
        // A second tile holding every feature the first does.
        let twice = |pages: &mut Vec<street_level::Page>| {
            let again = pages.clone();
            pages.extend(again);
        };
        twice(&mut level.buildings);
        twice(&mut level.blocks);
        twice(&mut level.street_trees);
        twice(&mut level.park_trees);
        for (_, pages) in &mut level.furniture {
            twice(pages);
        }
        let two = decode_core(&bodies, 300, 2.0).expect("decodes");
        let two = two.ground.expect("its ground");
        let two = two.street_level().expect("its street level");
        assert!(one.buildings.len() > 30 && one.trees.len() > 400);
        assert_eq!(two.buildings.len(), one.buildings.len());
        assert_eq!(two.trees.len(), one.trees.len());
        assert_eq!(two.furniture.len(), one.furniture.len());

        let mut bodies = museum_bodies();
        let level = bodies.street_level.as_mut().expect("asked for");
        level.buildings.push(Err(
            "Berlin's buildings could not be fetched: a tile.".into()
        ));
        let core = decode_core(&bodies, 300, 2.0).expect("decodes");
        let lost = core.street_level.clone().expect("the loss is said");
        assert!(lost.contains("a tile"), "{lost}");
        let ground = core.ground.expect("its ground");
        let level = ground.street_level().expect("the trees still stand");
        assert!(level.buildings.is_empty(), "left out whole");
        assert!(!level.trees.is_empty());
    }

    /// #1637: the ground spans its whole square - the configured points
    /// spaced to fit it, never closer than a metre - so a region is its
    /// square, a small one a small world and a large one coarser ground.
    #[test]
    fn the_ground_spans_its_square() {
        let default = SovereignTerrainConfig::default();
        assert_eq!(default.grid_size, 512);
        // 250 m at a metre: 250 points, the render exactly the square.
        assert_eq!(core_grid(250, &default), (250, 1.0));
        let square = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 250,
        };
        let bbox = core_bbox(square, 250, 1.0);
        assert_eq!(
            (bbox.min_e, bbox.min_n, bbox.max_e, bbox.max_n),
            (391_000, 5_819_500, 391_250, 5_819_750)
        );
        // A seeded kilometre and the editor's two keep the configured
        // points, spaced to the side.
        for side in [1_000, 2_000] {
            let (grid, cell) = core_grid(side, &default);
            assert_eq!(grid, 512);
            assert_eq!((f64::from(grid) * f64::from(cell)).round(), f64::from(side));
        }
        // The configured spacing is not read: a seeded 2.21 m cell spans the
        // same kilometre.
        let seeded = SovereignTerrainConfig {
            cell_scale: Fp(2.21),
            ..Default::default()
        };
        assert_eq!(core_grid(1_000, &seeded), core_grid(1_000, &default));
        // The recorded Museumsinsel: 300 points over 600 m, 2 m apart.
        assert_eq!(core_grid(600, &museum_config()), (300, 2.0));
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
            .init_resource::<Time<Real>>()
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
