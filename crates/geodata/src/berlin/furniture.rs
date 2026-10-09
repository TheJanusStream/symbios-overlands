//! Berlin's street furniture (#1588): its street lamps, from the city's
//! lighting register (`beleuchtung`), and the other common kinds of the
//! 2014 street survey (`strassenbefahrung`), each kind a WFS layer of its
//! own, read from a `GetFeature` page of [`FurnitureKind::layer`].
//!
//! The survey's own masts are not its lamps: a square kilometre of
//! Prenzlauer Berg has 6 of them and 604 lamps in the register. The
//! register also holds switch cabinets, light strips and a catch-all for
//! other lighting, which are left out: a lamp is a lamp post or a gas lamp
//! ([`is_lamp`]).
//!
//! Most kinds are points. A bench is surveyed as a line along its seat, a
//! bike rack and a fountain as an area; each is read as where it stands,
//! which way its long side runs, and how long that side is. GDI Berlin's
//! GeoServer refuses a query over several of these layers at once with a
//! box, so each kind is its own request.

use serde::Deserialize;

use crate::features::Geometry;
use crate::request::WfsType;

/// The most items of one kind one page asks for. A square kilometre of
/// Kreuzberg holds 1,765 bollards, the most numerous kind.
pub const FURNITURE_PAGE: u32 = 3_000;

/// A kind of street furniture the survey records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FurnitureKind {
    /// A street lamp on its post.
    Lamp,
    /// A bench.
    Bench,
    /// A litter bin.
    Bin,
    /// A bollard.
    Bollard,
    /// A bus or tram shelter.
    Shelter,
    /// A traffic sign.
    Sign,
    /// A fountain, ornamental or drinking.
    Fountain,
    /// An advertising column.
    Column,
    /// A bike rack, or a row of them.
    BikeRack,
}

impl FurnitureKind {
    /// Every kind, in the order the job asks for them.
    pub const ALL: [FurnitureKind; 9] = [
        FurnitureKind::Lamp,
        FurnitureKind::Bench,
        FurnitureKind::Bin,
        FurnitureKind::Bollard,
        FurnitureKind::Shelter,
        FurnitureKind::Sign,
        FurnitureKind::Fountain,
        FurnitureKind::Column,
        FurnitureKind::BikeRack,
    ];

    /// What a page of the kind holds, as a plural: for saying what could
    /// not be had.
    pub const fn name(self) -> &'static str {
        match self {
            FurnitureKind::Lamp => "street lamps",
            FurnitureKind::Bench => "benches",
            FurnitureKind::Bin => "litter bins",
            FurnitureKind::Bollard => "bollards",
            FurnitureKind::Shelter => "bus and tram shelters",
            FurnitureKind::Sign => "traffic signs",
            FurnitureKind::Fountain => "fountains",
            FurnitureKind::Column => "advertising columns",
            FurnitureKind::BikeRack => "bike racks",
        }
    }

    /// The layer the kind is read from: the lighting register's, or the
    /// survey's.
    pub const fn layer(self) -> WfsType {
        let type_name = match self {
            FurnitureKind::Lamp => return super::wfs("beleuchtung", "beleuchtung:beleuchtung"),
            FurnitureKind::Bench => "strassenbefahrung:bj_sitzbank",
            FurnitureKind::Bin => "strassenbefahrung:ah_abfallbehaelter_muellbox",
            FurnitureKind::Bollard => "strassenbefahrung:av_poller",
            FurnitureKind::Shelter => "strassenbefahrung:br_fahrgastunterstand",
            FurnitureKind::Sign => "strassenbefahrung:aa_verkehrszeichen",
            FurnitureKind::Fountain => "strassenbefahrung:bv_springbrunnen_zierbrunnen",
            FurnitureKind::Column => "strassenbefahrung:ag_werbesaeule",
            FurnitureKind::BikeRack => "strassenbefahrung:bq_fahrradstaender",
        };
        super::wfs("strassenbefahrung", type_name)
    }
}

/// One item of street furniture.
#[derive(Clone, Debug, PartialEq)]
pub struct FurnitureItem {
    /// Its id in its layer (a lamp's `leuchtstelle`, a sign's `sdatenid`,
    /// any other item's `gis_id`): stable across fetches.
    pub id: String,
    pub kind: FurnitureKind,
    /// Where it stands, E/N metres (EPSG:25833): a line's middle, an
    /// area's centroid.
    pub at: [f64; 2],
    /// Which way its long side runs, as a unit E/N vector pointing east (or
    /// north, where it runs north-south): a line's, first point to last; an
    /// area's longest edge. A side has no direction of its own, so the
    /// vector is only its line. `None` for a point.
    pub axis: Option<[f64; 2]>,
    /// How long its long side is (m): a line's length, an area's extent
    /// along its axis. `None` for a point.
    pub length: Option<f64>,
}

/// One page of one kind of furniture, and how many the request matched.
#[derive(Clone, Debug, PartialEq)]
pub struct FurniturePage {
    pub items: Vec<FurnitureItem>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
}

impl FurniturePage {
    /// Whether the server matched more items than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.items.len() as u64)
    }
}

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(default)]
    gis_id: Option<String>,
    #[serde(default)]
    sdatenid: Option<serde_json::Value>,
    #[serde(default)]
    leuchtstelle: Option<String>,
    #[serde(default)]
    leuchtentyp: Option<String>,
}

/// Whether the lighting register's `leuchtentyp` names a lamp on a post:
/// a lamp post of any kind (`Lichtmast ...`, a floodlight on a mast) or a
/// gas lamp, which Berlin stands on posts - in ten central districts every
/// lamp was one of these. Not a switch cabinet, a light strip, the
/// register's catch-all, nor any lamp hung on a wire or a wall, which a post
/// would stand in the road or against a house. An entry with no type is
/// taken for a lamp.
pub fn is_lamp(leuchtentyp: Option<&str>) -> bool {
    leuchtentyp.is_none_or(|kind| {
        let kind = kind.to_lowercase();
        kind.contains("mast") || kind.starts_with("gas")
    })
}

/// Read a WFS `GetFeature` page of `kind`'s layer, asked for with every
/// attribute. An item whose geometry gives no place is left out, and so is
/// a lighting register entry that is no lamp ([`is_lamp`]).
pub fn parse_furniture(
    kind: FurnitureKind,
    body: &[u8],
) -> Result<FurniturePage, crate::features::FeatureError> {
    let page = crate::features::parse_page::<Properties>(body)?;
    let items = page
        .features
        .into_iter()
        .filter_map(|f| {
            let p = f.properties;
            if kind == FurnitureKind::Lamp && !is_lamp(p.leuchtentyp.as_deref()) {
                return None;
            }
            let (at, axis, length) = place(&f.geometry)?;
            let survey_id = p.leuchtstelle.or(p.gis_id).or_else(|| match p.sdatenid? {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
            Some(FurnitureItem {
                id: survey_id.or(f.id).unwrap_or_default(),
                kind,
                at,
                axis,
                length,
            })
        })
        .collect();
    Ok(FurniturePage {
        items,
        matched: page.matched,
    })
}

/// Where an item stands, which way its long side runs and how long that
/// is (see [`FurnitureItem`]).
type Place = ([f64; 2], Option<[f64; 2]>, Option<f64>);

/// Where a geometry stands, its long axis and its length.
fn place(geometry: &Geometry) -> Option<Place> {
    if let Some(p) = geometry.point() {
        return Some((p, None, None));
    }
    let lines = geometry.lines();
    if let Some(line) = lines.iter().max_by(|a, b| arc(a).total_cmp(&arc(b))) {
        let length = arc(line);
        let (first, last) = (line.first()?, line.last()?);
        let axis = unit([last[0] - first[0], last[1] - first[1]]);
        return Some((along(line, length / 2.0)?, axis, Some(length)));
    }
    let rings = geometry.outer_rings();
    let ring = rings
        .iter()
        .max_by(|a, b| area(a).abs().total_cmp(&area(b).abs()))?;
    let at = centroid(ring)?;
    let longest = ring
        .windows(2)
        .map(|w| [w[1][0] - w[0][0], w[1][1] - w[0][1]])
        .max_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])))?;
    let axis = unit(longest)?;
    let (lo, hi) = ring.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
        let t = (p[0] - at[0]) * axis[0] + (p[1] - at[1]) * axis[1];
        (lo.min(t), hi.max(t))
    });
    Some((at, Some(axis), Some(hi - lo)))
}

fn arc(line: &[[f64; 2]]) -> f64 {
    line.windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum()
}

/// The point `distance` along `line`.
fn along(line: &[[f64; 2]], mut distance: f64) -> Option<[f64; 2]> {
    for w in line.windows(2) {
        let step = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if distance <= step && step > 0.0 {
            let t = distance / step;
            return Some([
                w[0][0] + (w[1][0] - w[0][0]) * t,
                w[0][1] + (w[1][1] - w[0][1]) * t,
            ]);
        }
        distance -= step;
    }
    line.last().copied()
}

/// `v` as a unit vector pointing east, or north where it points neither
/// way: a side's line, whichever way round it was drawn.
fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let length = v[0].hypot(v[1]);
    if length <= 1e-9 {
        return None;
    }
    let flip = v[0] < 0.0 || (v[0] == 0.0 && v[1] < 0.0);
    let sign = if flip { -1.0 } else { 1.0 };
    Some([sign * v[0] / length, sign * v[1] / length])
}

/// A ring's signed area, by the shoelace.
fn area(ring: &[[f64; 2]]) -> f64 {
    ring.windows(2)
        .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
        .sum::<f64>()
        / 2.0
}

/// A ring's area-weighted centroid; its vertices' mean where it has no
/// area.
fn centroid(ring: &[[f64; 2]]) -> Option<[f64; 2]> {
    let a = area(ring);
    if a.abs() < 1e-9 {
        let n = ring.len() as f64;
        return (n > 0.0).then(|| {
            let (e, nn) = ring
                .iter()
                .fold((0.0, 0.0), |(e, n), p| (e + p[0], n + p[1]));
            [e / n, nn / n]
        });
    }
    let (mut e, mut n) = (0.0, 0.0);
    for w in ring.windows(2) {
        let cross = w[0][0] * w[1][1] - w[1][0] * w[0][1];
        e += (w[0][0] + w[1][0]) * cross;
        n += (w[0][1] + w[1][1]) * cross;
    }
    Some([e / (6.0 * a), n / (6.0 * a)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn furniture_reads_where_it_stands_which_way_and_how_long() {
        let body = br#"{"type":"FeatureCollection","numberMatched":4,"features":[
            {"type":"Feature","id":"av_poller.1","geometry":{"type":"Point","coordinates":[10,20]},
             "properties":{"gis_id":"P1"}},
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":[[[0,0],[3,4],[6,8]]]},
             "properties":{"gis_id":"B1","laenge":10}},
            {"type":"Feature","geometry":{"type":"MultiPolygon","coordinates":[[[[0,0],[8,0],[8,2],[0,2],[0,0]]]]},
             "properties":{"gis_id":"R1"}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[5,5]},"properties":{"sdatenid":4711}}
        ]}"#;
        let bollard = parse_furniture(FurnitureKind::Bollard, body).unwrap();
        assert_eq!(bollard.items.len(), 4);
        let [point, bench, rack, sign] = &bollard.items[..] else {
            panic!("four items");
        };
        assert_eq!(
            (point.id.as_str(), point.at, point.axis),
            ("P1", [10.0, 20.0], None)
        );
        assert_eq!(bench.at, [3.0, 4.0], "the middle of the seat");
        assert_eq!(bench.axis, Some([0.6, 0.8]));
        assert_eq!(bench.length, Some(10.0));
        assert_eq!(rack.at, [4.0, 1.0], "the rack's centroid");
        assert_eq!((rack.axis, rack.length), (Some([1.0, 0.0]), Some(8.0)));
        assert_eq!(sign.id, "4711", "a sign is keyed by its sdatenid");
        assert!(
            bollard
                .items
                .iter()
                .all(|i| i.kind == FurnitureKind::Bollard)
        );
        assert_eq!(
            FurnitureKind::Bench.layer().type_name,
            "strassenbefahrung:bj_sitzbank"
        );
    }

    #[test]
    fn lamps_are_read_from_the_lighting_register_by_their_lamp_ids() {
        let body = br#"{"type":"FeatureCollection","numberMatched":4,"features":[
            {"type":"Feature","id":"beleuchtung.1","geometry":{"type":"MultiPoint","coordinates":[[1,2]]},
             "properties":{"leuchtstelle":"40477-2210001-00","leuchtentyp":"Lichtmast mit Auslegerleuchte"}},
            {"type":"Feature","id":"beleuchtung.2","geometry":{"type":"MultiPoint","coordinates":[[3,4]]},
             "properties":{"leuchtstelle":"40477-2210002-00","leuchtentyp":"Oberirdischer Schaltkasten"}},
            {"type":"Feature","id":"beleuchtung.3","geometry":{"type":"MultiPoint","coordinates":[[5,6]]},
             "properties":{"leuchtstelle":"40477-2210003-00","leuchtentyp":"Gas-Sonderleuchte"}},
            {"type":"Feature","id":"beleuchtung.4","geometry":{"type":"MultiPoint","coordinates":[[7,8]]},
             "properties":{"leuchtstelle":"40477-2210004-00","leuchtentyp":"Leuchtband"}}
        ]}"#;
        let page = parse_furniture(FurnitureKind::Lamp, body).unwrap();
        let ids: Vec<&str> = page.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["40477-2210001-00", "40477-2210003-00"]);
        assert_eq!(page.items[0].at, [1.0, 2.0]);
        assert_eq!(
            FurnitureKind::Lamp.layer().type_name,
            "beleuchtung:beleuchtung"
        );
        assert!(is_lamp(None) && is_lamp(Some("Lichtmast/Rohrst\u{e4}nder")));
        assert!(is_lamp(Some("Gas-H\u{e4}ngeleuchte")) && is_lamp(Some("Anstrahlung am Mast")));
        assert!(!is_lamp(Some("Elektrische Beleuchtung allgemein")));
        assert!(!is_lamp(Some("Seilleuchte")) && !is_lamp(Some("Wandleuchte")));
    }
}
