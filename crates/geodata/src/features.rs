//! WFS `GetFeature` pages as GeoJSON (#1588): each feature's geometry, its
//! properties as the layer's reader declares them, and how many features
//! the request matched in all.
//!
//! Geometry is read loosely: coordinates past the second (a height) are
//! ignored. A point that is not two finite numbers spoils what it is part
//! of: a part of a multi-geometry - a point, a line, a polygon - is left out
//! with it, and so is a polygon's hole; a lone point, a line string, or the
//! ring that outlines a polygon leaves nothing to read. A feature with
//! nothing left, or a geometry of an unexpected kind, reads as
//! [`Geometry::Other`]. A feature's id may be a string or a number. What a
//! layer can draw from all that is its reader's call, so a page with odd
//! features still reads.

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// One feature's geometry, as E/N points (or whatever the request's CRS
/// is: GDI Berlin answers in EPSG:25833).
#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Point([f64; 2]),
    MultiPoint(Vec<[f64; 2]>),
    LineString(Vec<[f64; 2]>),
    MultiLineString(Vec<Vec<[f64; 2]>>),
    /// Rings, the outer one first.
    Polygon(Vec<Vec<[f64; 2]>>),
    /// Polygons, each its rings, the outer one first.
    MultiPolygon(Vec<Vec<Vec<[f64; 2]>>>),
    /// Anything else, or a geometry that did not read.
    Other,
}

impl Geometry {
    /// Every line of the geometry: a line string's, a multi-line's lines.
    pub fn lines(&self) -> Vec<&[[f64; 2]]> {
        match self {
            Geometry::LineString(line) => vec![line.as_slice()],
            Geometry::MultiLineString(lines) => lines.iter().map(Vec::as_slice).collect(),
            _ => Vec::new(),
        }
    }

    /// Every polygon's outer ring.
    pub fn outer_rings(&self) -> Vec<&[[f64; 2]]> {
        match self {
            Geometry::Polygon(rings) => rings.first().map(Vec::as_slice).into_iter().collect(),
            Geometry::MultiPolygon(polygons) => polygons
                .iter()
                .filter_map(|rings| rings.first().map(Vec::as_slice))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Every polygon's rings, outer and holes alike: what an even-odd test
    /// of a point reads, where a point inside a hole lies outside.
    pub fn rings(&self) -> Vec<&[[f64; 2]]> {
        match self {
            Geometry::Polygon(rings) => rings.iter().map(Vec::as_slice).collect(),
            Geometry::MultiPolygon(polygons) => {
                polygons.iter().flatten().map(Vec::as_slice).collect()
            }
            _ => Vec::new(),
        }
    }

    /// The point, for a point or a one-point multi-point.
    pub fn point(&self) -> Option<[f64; 2]> {
        match self {
            Geometry::Point(p) => Some(*p),
            Geometry::MultiPoint(points) if points.len() == 1 => Some(points[0]),
            _ => None,
        }
    }
}

/// One feature: its geometry, if it has one that read, its id, and its
/// properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Feature<P> {
    /// The feature's id, as the server gives it (`layer.key`).
    pub id: Option<String>,
    pub geometry: Geometry,
    pub properties: P,
}

/// One page of features, and how many the request matched in all.
#[derive(Clone, Debug, PartialEq)]
pub struct FeaturePage<P> {
    pub features: Vec<Feature<P>>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
}

impl<P> FeaturePage<P> {
    /// Whether the server matched more features than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.features.len() as u64)
    }
}

/// Why a page of features could not be read.
#[derive(Debug)]
pub struct FeatureError(String);

impl std::fmt::Display for FeatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not a page of features: {}", self.0)
    }
}

impl std::error::Error for FeatureError {}

#[derive(Deserialize)]
struct RawPage<P> {
    #[serde(default = "Vec::new")]
    features: Vec<RawFeature<P>>,
    #[serde(rename = "numberMatched", default)]
    number_matched: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawFeature<P> {
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default)]
    geometry: Option<RawGeometry>,
    #[serde(default = "Option::default")]
    properties: Option<P>,
}

#[derive(Deserialize)]
struct RawGeometry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    coordinates: serde_json::Value,
}

/// Read a page of GeoJSON features, each feature's properties into `P`. A
/// feature whose properties are missing, or do not read as `P`, takes
/// `P::default()`.
pub fn parse_page<P: DeserializeOwned + Default>(
    body: &[u8],
) -> Result<FeaturePage<P>, FeatureError> {
    let page: RawPage<serde_json::Value> =
        serde_json::from_slice(body).map_err(|e| FeatureError(e.to_string()))?;
    let features = page
        .features
        .into_iter()
        .map(|f| Feature {
            id: f.id.and_then(|id| match id {
                serde_json::Value::String(id) => Some(id),
                serde_json::Value::Number(id) => Some(id.to_string()),
                _ => None,
            }),
            geometry: f.geometry.map_or(Geometry::Other, geometry),
            properties: f
                .properties
                .and_then(|p| serde_json::from_value(p).ok())
                .unwrap_or_default(),
        })
        .collect();
    Ok(FeaturePage {
        features,
        matched: page.number_matched.and_then(|v| v.as_u64()),
    })
}

/// A GeoJSON geometry, read as the module docs say.
fn geometry(raw: RawGeometry) -> Geometry {
    use serde_json::Value;
    fn point(v: &Value) -> Option<[f64; 2]> {
        let a = v.as_array()?;
        let (e, n) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
        (e.is_finite() && n.is_finite()).then_some([e, n])
    }
    /// A run of points, or `None` where one of them is not a point.
    fn run(v: &Value) -> Option<Vec<[f64; 2]>> {
        v.as_array()?.iter().map(point).collect()
    }
    /// A polygon's rings, less the holes that do not read; `None` where its
    /// outline does not.
    fn polygon(v: &Value) -> Option<Vec<Vec<[f64; 2]>>> {
        let mut rings = v.as_array()?.iter();
        let outline = run(rings.next()?)?;
        Some(
            std::iter::once(outline)
                .chain(rings.filter_map(run))
                .collect(),
        )
    }
    /// The parts of a multi-geometry that read; `None` where none does.
    fn parts<T>(v: &Value, part: impl Fn(&Value) -> Option<T>) -> Option<Vec<T>> {
        let read: Vec<T> = v.as_array()?.iter().filter_map(part).collect();
        (!read.is_empty()).then_some(read)
    }
    let c = &raw.coordinates;
    let read = match raw.kind.as_str() {
        "Point" => point(c).map(Geometry::Point),
        "MultiPoint" => parts(c, point).map(Geometry::MultiPoint),
        "LineString" => run(c).map(Geometry::LineString),
        "MultiLineString" => parts(c, run).map(Geometry::MultiLineString),
        "Polygon" => polygon(c).map(Geometry::Polygon),
        "MultiPolygon" => parts(c, polygon).map(Geometry::MultiPolygon),
        _ => None,
    };
    read.unwrap_or(Geometry::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Default, Debug, PartialEq)]
    struct Props {
        #[serde(default)]
        name: Option<String>,
    }

    #[test]
    fn every_geometry_reads_and_odd_ones_read_as_other() {
        let body = br#"{"type":"FeatureCollection","numberMatched":7,"features":[
            {"type":"Feature","id":"a.1","geometry":{"type":"Point","coordinates":[1.0,2.0,9.0]},"properties":{"name":"p"}},
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]},"properties":null},
            {"type":"Feature","geometry":{"type":"MultiPolygon","coordinates":[[[[0,0],[4,0],[4,4],[0,0]],[[1,1],[2,1],[2,2],[1,1]]]]},"properties":{"name":5}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":["x",2]}},
            {"type":"Feature","geometry":{"type":"GeometryCollection","geometries":[]}},
            {"type":"Feature"}
        ]}"#;
        let page = parse_page::<Props>(body).unwrap();
        assert_eq!(page.features.len(), 6);
        assert!(page.is_cut_short());
        let f = &page.features;
        assert_eq!(f[0].id.as_deref(), Some("a.1"));
        assert_eq!(
            f[0].geometry.point(),
            Some([1.0, 2.0]),
            "a height is ignored"
        );
        assert_eq!(f[0].properties.name.as_deref(), Some("p"));
        assert_eq!(f[1].geometry.lines().len(), 2);
        assert_eq!(f[1].properties, Props::default(), "null properties default");
        let rings = f[2].geometry.outer_rings();
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 4, "the outer ring, the hole left out");
        assert_eq!(
            f[2].geometry.rings().len(),
            2,
            "the outer ring and its hole"
        );
        assert!(f[0].geometry.rings().is_empty(), "a point has no rings");
        assert_eq!(
            f[2].properties,
            Props::default(),
            "properties that do not read default"
        );
        assert_eq!(
            f[3].geometry,
            Geometry::Other,
            "a point that is not numbers"
        );
        assert_eq!(f[4].geometry, Geometry::Other);
        assert_eq!(f[5].geometry, Geometry::Other);
        assert!(parse_page::<Props>(b"<html>").is_err());
    }

    /// The critic's finding (#1588): one bad point dropped a whole axis, and
    /// a numeric id failed the whole page.
    #[test]
    fn a_bad_part_is_left_out_and_a_numeric_id_reads() {
        let body = br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","id":7,"geometry":{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,"x"],[3,3]]]}},
            {"type":"Feature","id":"b.2","geometry":{"type":"MultiPolygon","coordinates":[
                [[[0,0],[4,0],[4,4],[0,0]],[[1,1],[null,1],[2,2],[1,1]]],
                [[[9,9],[9,"y"],[8,8],[9,9]]]]}},
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":[[[2,"x"],[3,3]]]}}
        ]}"#;
        let page = parse_page::<Props>(body).expect("the page reads");
        let f = &page.features;
        assert_eq!(f[0].id.as_deref(), Some("7"));
        assert_eq!(f[0].geometry.lines(), vec![&[[0.0, 0.0], [1.0, 1.0]][..]]);
        let Geometry::MultiPolygon(polygons) = &f[1].geometry else {
            panic!("{:?}", f[1].geometry);
        };
        assert_eq!(polygons.len(), 1, "the bad polygon left out");
        assert_eq!(polygons[0].len(), 1, "and the bad hole");
        assert_eq!(f[2].geometry, Geometry::Other, "nothing left");
    }
}
