//! Berlin's streets as lines (#1595): the ATKIS street and carriageway
//! axes, read from a WFS `GetFeature` page of GeoJSON.
//!
//! [`super::STREET_AXES`] has one line per stretch of street between two
//! junctions (or a junction and a dead end), and lines meet at exactly
//! shared end points. Where a street's carriageways run apart - a boulevard
//! with a central reservation - its axis runs down the middle of the
//! street, between them, and is no carriageway itself
//! ([`StreetAxis::separated`]); each carriageway is then a line of
//! [`super::CARRIAGEWAY_AXES`], which meets the street network at shared
//! end points too.

use serde::Deserialize;

/// The attributes an axis is asked for: its id, carriageway width, lanes,
/// carriageway separation, function and dedication, and the line.
pub const AXIS_PROPERTIES: &[&str] = &["uuid", "brf", "fsz", "ftr", "fkt", "wdm", "geom"];

/// The most axes one page asks for. A walkable core of central Berlin
/// holds a few hundred, at about 300 bytes each.
pub const AXIS_PAGE: u32 = 2_000;

/// ATKIS `ftr` (Fahrbahntrennung): the carriageways run apart.
const SEPARATED: &str = "2000";

/// ATKIS `fkt` (Funktion): a pedestrian zone.
const PEDESTRIAN_ZONE: &str = "1808";

/// What a street is dedicated to, by ATKIS `wdm` (Widmung).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dedication {
    /// A motorway (1301).
    Motorway,
    /// A federal road (1303).
    Federal,
    /// A state road (1305).
    State,
    /// A district road (1306).
    District,
    /// A municipal street (1307).
    Municipal,
    /// Anything else, or nothing said (9997, 9999, empty).
    Other,
}

impl Dedication {
    /// The dedication an ATKIS `wdm` code names.
    pub fn from_code(code: &str) -> Self {
        match code {
            "1301" => Dedication::Motorway,
            "1303" => Dedication::Federal,
            "1305" => Dedication::State,
            "1306" => Dedication::District,
            "1307" => Dedication::Municipal,
            _ => Dedication::Other,
        }
    }
}

/// One street or carriageway axis.
#[derive(Clone, Debug, PartialEq)]
pub struct StreetAxis {
    /// The feature's ATKIS id: stable across fetches.
    pub uuid: String,
    /// The line, as E/N metres in EPSG:25833: each part of a multi-line a
    /// line of its own.
    pub lines: Vec<Vec<[f64; 2]>>,
    /// The carriageway's width (m), where ATKIS has it (`brf`).
    pub width: Option<f32>,
    /// The carriageway's lanes, where ATKIS has them (`fsz`).
    pub lanes: Option<u8>,
    /// Whether the street's carriageways run apart, so this axis runs
    /// between them and is no carriageway itself.
    pub separated: bool,
    /// Whether the street is a pedestrian zone.
    pub pedestrian: bool,
    /// What the street is dedicated to.
    pub dedication: Dedication,
}

/// One page of axes, and how many the request matched in all: more than
/// the page holds means the page was cut short.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisPage {
    pub axes: Vec<StreetAxis>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
}

impl AxisPage {
    /// Whether the server matched more axes than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.axes.len() as u64)
    }
}

/// Why a page of axes could not be read.
pub type AxisError = crate::features::FeatureError;

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    brf: Option<String>,
    #[serde(default)]
    fsz: Option<String>,
    #[serde(default)]
    ftr: Option<String>,
    #[serde(default)]
    fkt: Option<String>,
    #[serde(default)]
    wdm: Option<String>,
}

/// Read a WFS `GetFeature` page of [`super::STREET_AXES`] or
/// [`super::CARRIAGEWAY_AXES`] asked for with [`AXIS_PROPERTIES`]. A
/// feature with no line, or whose lines have fewer than two points, is
/// left out; a width or lane count that is not a positive number is
/// none.
pub fn parse_axes(body: &[u8]) -> Result<AxisPage, AxisError> {
    let page = crate::features::parse_page::<Properties>(body)?;
    let positive = |s: &Option<String>| {
        s.as_deref()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
    };
    let axes = page
        .features
        .into_iter()
        .filter_map(|f| {
            let lines: Vec<Vec<[f64; 2]>> = f
                .geometry
                .lines()
                .into_iter()
                .filter(|line| line.len() >= 2)
                .map(<[[f64; 2]]>::to_vec)
                .collect();
            if lines.is_empty() {
                return None;
            }
            let p = f.properties;
            Some(StreetAxis {
                uuid: p.uuid.unwrap_or_default(),
                lines,
                width: positive(&p.brf),
                lanes: positive(&p.fsz).map(|v| v.round().min(f32::from(u8::MAX)) as u8),
                separated: p.ftr.as_deref() == Some(SEPARATED),
                pedestrian: p.fkt.as_deref() == Some(PEDESTRIAN_ZONE),
                dedication: Dedication::from_code(p.wdm.as_deref().unwrap_or("")),
            })
        })
        .collect();
    Ok(AxisPage {
        axes,
        matched: page.matched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_reads_its_line_and_what_it_says_about_its_carriageway() {
        let body = br#"{"type":"FeatureCollection","numberMatched":3,"features":[
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":
              [[[391726.3,5819761.5],[391732.766,5819765.6]]]},
             "properties":{"uuid":"A","brf":"10","fsz":"2","ftr":"","fkt":"","wdm":"1307"}},
            {"type":"Feature","geometry":{"type":"LineString","coordinates":
              [[1.0,2.0],[3.0,4.0],[5.0,6.0]]},
             "properties":{"uuid":"B","brf":"","fsz":"x","ftr":"2000","fkt":"1808","wdm":"9999"}},
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":[[[1.0,2.0]]]},
             "properties":{"uuid":"C"}}
        ]}"#;
        let page = parse_axes(body).unwrap();
        assert_eq!(page.axes.len(), 2, "a one-point line is no line");
        assert!(page.is_cut_short(), "three matched, two read");
        let a = &page.axes[0];
        assert_eq!(a.uuid, "A");
        assert_eq!(
            a.lines,
            vec![vec![[391726.3, 5819761.5], [391732.766, 5819765.6]]]
        );
        assert_eq!((a.width, a.lanes), (Some(10.0), Some(2)));
        assert!(!a.separated && !a.pedestrian);
        assert_eq!(a.dedication, Dedication::Municipal);
        let b = &page.axes[1];
        assert_eq!(b.lines[0].len(), 3);
        assert_eq!((b.width, b.lanes), (None, None));
        assert!(b.separated && b.pedestrian);
        assert_eq!(b.dedication, Dedication::Other);
        assert!(parse_axes(b"<html>").is_err());
    }
}
