//! Real-world geodata for geodata regions (#1580): a region whose ground,
//! water and buildings come from a square of real Berlin, dressed in the
//! region's theme.
//!
//! The data is GDI Berlin's (<https://gdi.berlin.de>), fetched when a region
//! is visited. Everything this crate touches is licensed dl-de/zero-2.0
//! (<https://www.govdata.de/dl-de/zero-2-0>), which needs no attribution;
//! the region info credits it anyway.
//!
//! This crate is the half of that pipeline that does no I/O, so the app and
//! the wasm worker can share it:
//!
//! - [`square`]: a [`GeoSquare`] of the map, and the log-uniform size draw;
//! - [`berlin`]: the Berlin dataset - its host, its layer catalogue, its
//!   land-use classes and boroughs, and the [`berlin::Coverage`] that says
//!   where a square may lie and places one from a seeded draw;
//! - [`request`]: the WMS `GetMap`/`GetLegendGraphic` and WFS `GetFeature`
//!   URLs, built canonically so a URL can key a cache;
//! - [`legend`]: GeoServer's JSON legends, which name the colour of every
//!   class a WMS layer is drawn with;
//! - [`raster`]: decoding a WMS render back into data - heights from the
//!   terrain layer's colour classes, class ids from a categorical layer's
//!   fills.
//!
//! # Why a styled render, not raw data
//!
//! GDI Berlin offers no coverage service (WCS), and its WMS ignores
//! client-supplied styles, so raw elevation comes only as whole 2 km tiles of
//! 16 MB. But a WMS renders any square at any size in one small request, and
//! its JSON legend names the exact colour of every class. The terrain layer
//! is drawn in one-metre height classes, so a render decodes to heights
//! within a metre, and [`raster::decode_terrain`] smooths out the steps
//! within each pixel's class: 0.26 m mean error against the raw 1 m model,
//! for 9 KB per 256 x 256 render. That is the zoom-fitting loading the GDI
//! viewer does, used as distance LOD.
//!
//! # Conventions
//!
//! Coordinates are ETRS89 / UTM zone 33N (EPSG:25833) in whole metres:
//! easting `e`, northing `n`. Rasters are row-major with **row 0 at the
//! north edge**, as a WMS serves them.

pub mod berlin;
pub mod legend;
pub mod raster;
pub mod request;
pub mod square;

pub use square::GeoSquare;
