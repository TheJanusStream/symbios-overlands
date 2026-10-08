//! One request to GDI Berlin: its URL, its byte cap, and how to tell the
//! answer asked for from an error page before anything is kept.

use geodata::raster::MAX_PIXELS;
use geodata::request::{self as wire, Bbox, FeatureQuery, WfsType, WmsLayer};

use super::GeoFetchError;

/// The most bytes a legend may be. The terrain legend, the largest, is 16 KB.
pub const LEGEND_CAP: usize = 64 << 10;

/// The most bytes a `resultType=hits` answer may be: one XML element.
pub const HITS_CAP: usize = 16 << 10;

/// The most bytes one page of GeoJSON features may be. A thousand ALKIS
/// footprints with every attribute come to about 1.3 MB.
pub const FEATURES_CAP: usize = 16 << 20;

/// The most bytes a render may be, whatever its size: a 2048 x 2048 render
/// (the decoders' cap) stored raw, plus the PNG's framing.
pub const RENDER_CAP: usize = (MAX_PIXELS as usize) * 4 + (64 << 10);

/// What a request asks for, which decides its cap and what a valid answer
/// looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeoKind {
    /// A layer's JSON legend.
    Legend,
    /// A PNG render of exactly this many pixels.
    Render {
        /// Columns.
        width: u32,
        /// Rows.
        height: u32,
    },
    /// A page of GeoJSON features.
    Features,
    /// A feature count (`resultType=hits`).
    Hits,
}

/// One request: the canonical URL from the `geodata` crate's builders, which
/// is also its cache key, and what it asks for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GeoRequest {
    url: String,
    kind: GeoKind,
}

impl GeoRequest {
    /// The JSON legend of `layer.layers[index]`.
    ///
    /// # Panics
    ///
    /// If `index` is out of range for `layer.layers`.
    pub fn legend(layer: &WmsLayer, index: usize) -> Self {
        GeoRequest {
            url: wire::get_legend(layer, index),
            kind: GeoKind::Legend,
        }
    }

    /// A `width x height` PNG render of `layer` over `bbox`. A render past
    /// the decoders' pixel cap is refused when it is fetched.
    pub fn render(layer: &WmsLayer, bbox: Bbox, width: u32, height: u32) -> Self {
        GeoRequest {
            url: wire::get_map(layer, bbox, width, height),
            kind: GeoKind::Render { width, height },
        }
    }

    /// A page of `feature`'s features in `bbox`, as GeoJSON.
    pub fn features(feature: &WfsType, bbox: Bbox, query: &FeatureQuery<'_>) -> Self {
        GeoRequest {
            url: wire::get_features(feature, bbox, query),
            kind: GeoKind::Features,
        }
    }

    /// How many of `feature`'s features lie in `bbox`.
    pub fn hits(feature: &WfsType, bbox: Bbox) -> Self {
        GeoRequest {
            url: wire::count_features(feature, bbox),
            kind: GeoKind::Hits,
        }
    }

    /// The URL, which is also the cache key.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// What it asks for.
    pub fn kind(&self) -> GeoKind {
        self.kind
    }

    /// The most bytes its answer may be.
    pub fn cap(&self) -> usize {
        match self.kind {
            GeoKind::Legend => LEGEND_CAP,
            GeoKind::Render { width, height } => {
                let raw = u64::from(width) * u64::from(height) * 4 + (64 << 10);
                raw.min(RENDER_CAP as u64) as usize
            }
            GeoKind::Features => FEATURES_CAP,
            GeoKind::Hits => HITS_CAP,
        }
    }

    /// Whether this client will send it: GDI Berlin's services only, and no
    /// render the decoders would refuse anyway.
    pub fn is_allowed(&self) -> bool {
        let on_host = is_gdi_services_url(&self.url);
        let decodable = match self.kind {
            GeoKind::Render { width, height } => {
                width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS
            }
            _ => true,
        };
        on_host && decodable
    }

    /// Whether `body` is the answer asked for. GeoServer reports a bad
    /// request as an XML exception, at times under a success status, and an
    /// error page kept in the cache would stand in for the data until it
    /// expired - so nothing is kept, or used, without passing this.
    pub fn validate(&self, body: &[u8]) -> Result<(), GeoFetchError> {
        let valid = match self.kind {
            GeoKind::Legend => json_object_naming(body, b"\"Legend\""),
            GeoKind::Render { width, height } => png_size(body) == Some((width, height)),
            GeoKind::Features => json_object_naming(body, b"\"FeatureCollection\""),
            GeoKind::Hits => wire::parse_hits(body).is_some(),
        };
        if valid {
            Ok(())
        } else {
            Err(GeoFetchError::BadResponse)
        }
    }
}

/// Whether `url` is one of GDI Berlin's services: the only URLs this client
/// asks, and the only ones it takes an answer from - a redirect elsewhere is
/// refused ([`GeoFetchError::Redirected`]).
pub fn is_gdi_services_url(url: &str) -> bool {
    url.strip_prefix(geodata::berlin::BASE_URL)
        .is_some_and(|rest| rest.starts_with("/services/"))
}

/// Whether `body` opens a JSON object and names `needle` somewhere in it.
/// Enough to tell a GeoServer answer from its XML exception; the parsers
/// downstream do the real reading.
fn json_object_naming(body: &[u8], needle: &[u8]) -> bool {
    let start = body.iter().position(|b| !b.is_ascii_whitespace());
    start.is_some_and(|at| body[at] == b'{') && body.windows(needle.len()).any(|w| w == needle)
}

/// The `(width, height)` a PNG's header declares, if `body` is a PNG.
fn png_size(body: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if body.len() < 24 || &body[..8] != SIGNATURE || &body[12..16] != b"IHDR" {
        return None;
    }
    let word = |at: usize| u32::from_be_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    Some((word(16), word(20)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geodata::berlin;

    const BOX: Bbox = Bbox {
        min_e: 391_200,
        min_n: 5_819_700,
        max_e: 391_800,
        max_n: 5_820_300,
    };

    /// A minimal PNG header declaring `width x height`.
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 3, 0, 0, 0]);
        bytes
    }

    #[test]
    fn requests_carry_the_crate_urls_and_their_caps() {
        let legend = GeoRequest::legend(&berlin::TERRAIN, 0);
        assert_eq!(legend.url(), wire::get_legend(&berlin::TERRAIN, 0));
        assert_eq!((legend.kind(), legend.cap()), (GeoKind::Legend, LEGEND_CAP));

        let render = GeoRequest::render(&berlin::TERRAIN, BOX, 512, 256);
        assert_eq!(render.url(), wire::get_map(&berlin::TERRAIN, BOX, 512, 256));
        assert_eq!(render.cap(), 512 * 256 * 4 + (64 << 10));
        let huge = GeoRequest::render(&berlin::TERRAIN, BOX, 2048, 2048);
        assert_eq!(huge.cap(), RENDER_CAP);

        let query = FeatureQuery {
            count: Some(1000),
            ..FeatureQuery::default()
        };
        let features = GeoRequest::features(&berlin::BUILDINGS, BOX, &query);
        assert_eq!(
            features.url(),
            wire::get_features(&berlin::BUILDINGS, BOX, &query)
        );
        assert_eq!(features.cap(), FEATURES_CAP);
        let hits = GeoRequest::hits(&berlin::BUILDINGS, BOX);
        assert_eq!((hits.kind(), hits.cap()), (GeoKind::Hits, HITS_CAP));
    }

    #[test]
    fn only_gdi_berlin_services_and_decodable_renders_are_allowed() {
        assert!(GeoRequest::legend(&berlin::TERRAIN, 0).is_allowed());
        assert!(GeoRequest::render(&berlin::TERRAIN, BOX, 2048, 2048).is_allowed());
        assert!(!GeoRequest::render(&berlin::TERRAIN, BOX, 2049, 2048).is_allowed());
        assert!(!GeoRequest::render(&berlin::TERRAIN, BOX, 0, 16).is_allowed());
        let elsewhere = |url: &str| GeoRequest {
            url: url.into(),
            kind: GeoKind::Legend,
        };
        assert!(!elsewhere("https://example.org/services/wms/dgm1?x").is_allowed());
        assert!(!elsewhere("https://gdi.berlin.de.example.org/services/wms").is_allowed());
        assert!(!elsewhere("https://gdi.berlin.de/data/dgm1/atom/x.zip").is_allowed());
        assert!(!elsewhere("http://gdi.berlin.de/services/wms/dgm1").is_allowed());
    }

    #[test]
    fn answers_validate_by_kind() {
        let exception = br#"<?xml version="1.0"?><ServiceExceptionReport><ServiceException>LayerNotDefined</ServiceException></ServiceExceptionReport>"#;

        let legend = GeoRequest::legend(&berlin::TERRAIN, 0);
        assert_eq!(legend.validate(b" {\"Legend\": []}"), Ok(()));
        assert_eq!(legend.validate(exception), Err(GeoFetchError::BadResponse));
        assert_eq!(
            legend.validate(b"{\"other\": 1}"),
            Err(GeoFetchError::BadResponse)
        );

        let render = GeoRequest::render(&berlin::TERRAIN, BOX, 300, 200);
        assert_eq!(render.validate(&png_header(300, 200)), Ok(()));
        assert_eq!(
            render.validate(&png_header(200, 300)),
            Err(GeoFetchError::BadResponse)
        );
        assert_eq!(render.validate(exception), Err(GeoFetchError::BadResponse));
        assert_eq!(
            render.validate(&png_header(300, 200)[..20]),
            Err(GeoFetchError::BadResponse)
        );

        let features = GeoRequest::features(&berlin::BUILDINGS, BOX, &FeatureQuery::default());
        assert_eq!(
            features.validate(br#"{"type":"FeatureCollection","features":[]}"#),
            Ok(())
        );
        assert_eq!(
            features.validate(exception),
            Err(GeoFetchError::BadResponse)
        );

        let hits = GeoRequest::hits(&berlin::BUILDINGS, BOX);
        assert_eq!(
            hits.validate(br#"<wfs:FeatureCollection numberMatched="42"/>"#),
            Ok(())
        );
        assert_eq!(hits.validate(exception), Err(GeoFetchError::BadResponse));
    }
}
