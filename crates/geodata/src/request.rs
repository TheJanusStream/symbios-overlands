//! Canonical request URLs for a GeoServer's WMS and WFS.
//!
//! Every builder writes its parameters in one fixed order with one fixed
//! encoding, so the same request is always the same string. The fetch cache
//! keys on that string: the services answer `Cache-Control: no-store`, so
//! nothing below the app would cache a response for it.

use std::fmt::Write as _;

use crate::GeoSquare;

/// A WMS service and the layers one `GetMap` draws, bottom first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WmsLayer {
    /// Scheme and host, e.g. `https://gdi.berlin.de`.
    pub base: &'static str,
    /// The service's path segment: requests go to `{base}/services/wms/{service}`.
    pub service: &'static str,
    /// Layer names, drawn in this order.
    pub layers: &'static [&'static str],
    /// The EPSG code of the CRS the layer is requested in.
    pub epsg: u32,
}

/// A WFS service and one of its feature types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WfsType {
    /// Scheme and host, e.g. `https://gdi.berlin.de`.
    pub base: &'static str,
    /// The service's path segment: requests go to `{base}/services/wfs/{service}`.
    pub service: &'static str,
    /// The qualified feature type name, e.g. `alkis_gebaeude:gebaeude`.
    pub type_name: &'static str,
    /// The EPSG code of the CRS the bbox is given in.
    pub epsg: u32,
}

/// An axis-aligned box in whole metres of the service CRS (easting, northing).
/// As wide as [`GeoSquare::max_e`], so every square converts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bbox {
    /// West edge.
    pub min_e: i64,
    /// South edge.
    pub min_n: i64,
    /// East edge.
    pub max_e: i64,
    /// North edge.
    pub max_n: i64,
}

impl From<GeoSquare> for Bbox {
    fn from(square: GeoSquare) -> Self {
        Bbox {
            min_e: i64::from(square.min_e),
            min_n: i64::from(square.min_n),
            max_e: square.max_e(),
            max_n: square.max_n(),
        }
    }
}

/// Optional parts of a WFS `GetFeature`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FeatureQuery<'a> {
    /// Only these attributes. Name the geometry attribute too if the
    /// geometry is wanted: the server leaves out whatever is not listed.
    /// Empty asks for every attribute.
    pub properties: &'a [&'a str],
    /// At most this many features.
    pub count: Option<u32>,
    /// Skip this many first (paging).
    pub start_index: Option<u32>,
}

/// A WMS 1.3.0 `GetMap` for a PNG of `layer` over `bbox`, `width` x `height`
/// pixels, transparent where nothing is drawn.
///
/// `format_options=antialias:none` is GeoServer's switch for drawing vector
/// fills without blended edges, so every pixel of a categorical layer is one
/// of its legend colours (or an outline); it changes nothing for a raster.
pub fn get_map(layer: &WmsLayer, bbox: Bbox, width: u32, height: u32) -> String {
    let mut url = service_url(layer.base, "wms", layer.service);
    param(&mut url, "service", "WMS");
    param(&mut url, "version", "1.3.0");
    param(&mut url, "request", "GetMap");
    param(&mut url, "layers", &layer.layers.join(","));
    param(&mut url, "styles", "");
    param(&mut url, "crs", &format!("EPSG:{}", layer.epsg));
    param(&mut url, "bbox", &bbox_value(bbox));
    param(&mut url, "width", &width.to_string());
    param(&mut url, "height", &height.to_string());
    param(&mut url, "format", "image/png");
    param(&mut url, "transparent", "true");
    param(&mut url, "format_options", "antialias:none");
    url
}

/// A `GetLegendGraphic` asking for the JSON legend of `layer.layers[index]`:
/// every class the layer is drawn with, and its colour. One layer per
/// request; a multi-layer [`WmsLayer`] needs one per layer.
///
/// # Panics
///
/// If `index` is out of range for `layer.layers`.
pub fn get_legend(layer: &WmsLayer, index: usize) -> String {
    let mut url = service_url(layer.base, "wms", layer.service);
    param(&mut url, "service", "WMS");
    param(&mut url, "version", "1.3.0");
    param(&mut url, "request", "GetLegendGraphic");
    param(&mut url, "layer", layer.layers[index]);
    param(&mut url, "format", "application/json");
    url
}

/// A WFS 2.0 `GetFeature` for the features of `feature` in `bbox`, as GeoJSON.
pub fn get_features(feature: &WfsType, bbox: Bbox, query: &FeatureQuery<'_>) -> String {
    let mut url = feature_url(feature, "GetFeature");
    param(&mut url, "outputFormat", "application/json");
    param(&mut url, "bbox", &wfs_bbox_value(feature, bbox));
    if !query.properties.is_empty() {
        param(&mut url, "propertyName", &query.properties.join(","));
    }
    if let Some(count) = query.count {
        param(&mut url, "count", &count.to_string());
    }
    if let Some(start) = query.start_index {
        param(&mut url, "startIndex", &start.to_string());
    }
    url
}

/// A WFS 2.0 `GetFeature` with `resultType=hits`: how many features of
/// `feature` lie in `bbox`, without any of them. Read the answer with
/// [`parse_hits`]. Sizes a request before it is made.
pub fn count_features(feature: &WfsType, bbox: Bbox) -> String {
    let mut url = feature_url(feature, "GetFeature");
    param(&mut url, "resultType", "hits");
    param(&mut url, "bbox", &wfs_bbox_value(feature, bbox));
    url
}

/// The `numberMatched` of a `resultType=hits` answer, or `None` if the body
/// does not carry a whole number there (`"unknown"` is a legal answer).
pub fn parse_hits(body: &[u8]) -> Option<u64> {
    const KEY: &[u8] = b"numberMatched=\"";
    let at = body.windows(KEY.len()).position(|w| w == KEY)? + KEY.len();
    let digits = body[at..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || body.get(at + digits) != Some(&b'"') {
        return None;
    }
    std::str::from_utf8(&body[at..at + digits])
        .ok()?
        .parse()
        .ok()
}

fn service_url(base: &str, kind: &str, service: &str) -> String {
    format!("{base}/services/{kind}/{service}?")
}

fn feature_url(feature: &WfsType, request: &str) -> String {
    let mut url = service_url(feature.base, "wfs", feature.service);
    param(&mut url, "service", "WFS");
    param(&mut url, "version", "2.0.0");
    param(&mut url, "request", request);
    param(&mut url, "typeNames", feature.type_name);
    url
}

fn bbox_value(b: Bbox) -> String {
    format!("{},{},{},{}", b.min_e, b.min_n, b.max_e, b.max_n)
}

fn wfs_bbox_value(feature: &WfsType, b: Bbox) -> String {
    format!("{},urn:ogc:def:crs:EPSG::{}", bbox_value(b), feature.epsg)
}

/// Append `key=value` (after a `&` unless the URL ends in `?`), encoding
/// everything but unreserved characters and the `,` `:` `/` that the values
/// here use as separators - all legal in a query, and kept literal so a URL
/// reads the way the services document it.
fn param(url: &mut String, key: &str, value: &str) {
    if !url.ends_with('?') {
        url.push('&');
    }
    url.push_str(key);
    url.push('=');
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b','
            | b':'
            | b'/' => url.push(byte as char),
            _ => {
                let _ = write!(url, "%{byte:02X}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYER: WmsLayer = WmsLayer {
        base: "https://example.org",
        service: "dgm",
        layers: &["a", "b"],
        epsg: 25833,
    };
    const TYPE: WfsType = WfsType {
        base: "https://example.org",
        service: "trees",
        type_name: "trees:street",
        epsg: 25833,
    };
    const BOX: Bbox = Bbox {
        min_e: 392_000,
        min_n: 5_820_000,
        max_e: 393_024,
        max_n: 5_821_024,
    };

    #[test]
    fn get_map_is_canonical() {
        assert_eq!(
            get_map(&LAYER, BOX, 256, 128),
            "https://example.org/services/wms/dgm?service=WMS&version=1.3.0&request=GetMap\
             &layers=a,b&styles=&crs=EPSG:25833&bbox=392000,5820000,393024,5821024\
             &width=256&height=128&format=image/png&transparent=true\
             &format_options=antialias:none"
        );
    }

    #[test]
    fn every_square_converts_to_a_box() {
        let square = GeoSquare {
            min_e: i32::MAX,
            min_n: i32::MIN,
            size_m: u32::MAX,
        };
        let b = Bbox::from(square);
        assert_eq!(b.min_e, i64::from(i32::MAX));
        assert_eq!(b.max_e, i64::from(i32::MAX) + i64::from(u32::MAX));
        assert_eq!(b.max_n, i64::from(i32::MIN) + i64::from(u32::MAX));
    }

    #[test]
    fn get_legend_names_one_layer() {
        assert_eq!(
            get_legend(&LAYER, 1),
            "https://example.org/services/wms/dgm?service=WMS&version=1.3.0\
             &request=GetLegendGraphic&layer=b&format=application/json"
        );
    }

    #[test]
    fn get_features_writes_only_what_is_asked() {
        assert_eq!(
            get_features(&TYPE, BOX, &FeatureQuery::default()),
            "https://example.org/services/wfs/trees?service=WFS&version=2.0.0\
             &request=GetFeature&typeNames=trees:street&outputFormat=application/json\
             &bbox=392000,5820000,393024,5821024,urn:ogc:def:crs:EPSG::25833"
        );
        let query = FeatureQuery {
            properties: &["geom", "art_dtsch"],
            count: Some(100),
            start_index: Some(200),
        };
        assert!(get_features(&TYPE, BOX, &query).ends_with(
            "urn:ogc:def:crs:EPSG::25833&propertyName=geom,art_dtsch&count=100&startIndex=200"
        ));
    }

    #[test]
    fn count_features_asks_for_hits() {
        assert_eq!(
            count_features(&TYPE, BOX),
            "https://example.org/services/wfs/trees?service=WFS&version=2.0.0\
             &request=GetFeature&typeNames=trees:street&resultType=hits\
             &bbox=392000,5820000,393024,5821024,urn:ogc:def:crs:EPSG::25833"
        );
    }

    #[test]
    fn param_encodes_everything_a_query_cannot_carry() {
        let mut url = String::from("x?");
        param(&mut url, "q", "grz = '100'&a#b");
        assert_eq!(url, "x?q=grz%20%3D%20%27100%27%26a%23b");
    }

    #[test]
    fn hits_parse_the_number_matched() {
        let body = br#"<?xml version="1.0"?><wfs:FeatureCollection numberMatched="5408" numberReturned="0" timeStamp="x"/>"#;
        assert_eq!(parse_hits(body), Some(5408));
        assert_eq!(parse_hits(br#"<x numberMatched="unknown"/>"#), None);
        assert_eq!(parse_hits(br#"<x numberMatched="12"#), None);
        assert_eq!(parse_hits(b"<x/>"), None);
    }
}
