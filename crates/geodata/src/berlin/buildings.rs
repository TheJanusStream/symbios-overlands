//! Berlin's buildings (#1588): ALKIS footprints with their use and storeys,
//! read from a WFS `GetFeature` page of [`super::BUILDINGS`].
//!
//! ALKIS draws a building (`AX_Gebaeude`) as its footprint, and parts of it
//! that differ from the rest (`AX_Bauteil`: a lower wing, a high-rise
//! section, an overhang, a passage) as footprints of their own inside it.
//! The building carries the use; a part only its storeys.

use serde::Deserialize;

/// The attributes a building is asked for: its id, use, storeys above
/// ground, whether it is a whole building or a part, and its footprint.
pub const BUILDING_PROPERTIES: &[&str] = &["uuid", "gfk", "aog", "bezeich", "geom"];

/// The most buildings one page asks for, buildings and parts together, at
/// about 1 KB each: the densest of four central square kilometres held
/// 1,281.
pub const BUILDING_PAGE: u32 = 3_000;

/// What a building is used for, from its ALKIS function code (`gfk`,
/// Gebaeudefunktion), coarsely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingUse {
    /// Homes: houses, flats, residential homes, weekend houses (10xx, 12xx,
    /// 13xx).
    Residential,
    /// Homes over shops, offices or workshops (11xx, 23xx).
    Mixed,
    /// Shops, offices, banks, hotels, restaurants, cinemas (20xx).
    Commercial,
    /// Factories, workshops, warehouses, filling stations, farm buildings
    /// (21xx, 22xx, 27xx).
    Industrial,
    /// Car parks and garages above ground (2460-2464).
    Parking,
    /// An underground car park (2465): nothing stands above ground.
    Underground,
    /// Small technical buildings: transport, supply and disposal - signal
    /// boxes, transformers, pumping stations, public toilets (24xx, 25xx,
    /// 26xx but parking).
    Utility,
    /// Administration, schools, hospitals, police, stations, sports halls
    /// (30xx-32xx but the two below).
    Public,
    /// Palaces, theatres, concert halls, museums, libraries, castles
    /// (3030-3039).
    Cultural,
    /// Churches, synagogues, chapels, mosques, temples, monasteries
    /// (3040-3049).
    Religious,
    /// Unknown, or a code this build does not place (9998, none).
    Unknown,
}

impl BuildingUse {
    /// The use an ALKIS function code names.
    pub fn from_function(code: u16) -> Self {
        match code {
            1100..=1199 | 2300..=2399 => BuildingUse::Mixed,
            1000..=1399 => BuildingUse::Residential,
            2465 => BuildingUse::Underground,
            2460..=2464 => BuildingUse::Parking,
            2000..=2099 => BuildingUse::Commercial,
            2100..=2299 | 2700..=2799 => BuildingUse::Industrial,
            2400..=2699 => BuildingUse::Utility,
            3030..=3039 => BuildingUse::Cultural,
            3040..=3049 => BuildingUse::Religious,
            3000..=3299 => BuildingUse::Public,
            _ => BuildingUse::Unknown,
        }
    }
}

/// One building or building part.
#[derive(Clone, Debug, PartialEq)]
pub struct Building {
    /// The feature's ALKIS id: stable across fetches.
    pub uuid: String,
    /// The footprint's outer rings, E/N metres (EPSG:25833): one per
    /// polygon, closed (the first point repeated last), courtyards left out.
    pub outlines: Vec<Vec<[f64; 2]>>,
    /// The ALKIS function code, where ALKIS has one: a part has none.
    pub function: Option<u16>,
    /// Storeys above ground, where ALKIS has them.
    pub storeys: Option<u8>,
    /// Whether this is a part of a building (`AX_Bauteil`) rather than a
    /// building (`AX_Gebaeude`).
    pub part: bool,
}

impl Building {
    /// What the building is used for; a part's is [`BuildingUse::Unknown`].
    pub fn usage(&self) -> BuildingUse {
        self.function
            .map_or(BuildingUse::Unknown, BuildingUse::from_function)
    }

    /// The footprint's area (m2): its outer rings', by the shoelace.
    pub fn area(&self) -> f64 {
        self.outlines
            .iter()
            .map(|ring| {
                ring.windows(2)
                    .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
                    .sum::<f64>()
                    .abs()
                    / 2.0
            })
            .sum()
    }
}

/// One page of buildings, and how many the request matched in all.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingPage {
    pub buildings: Vec<Building>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
    /// How many features the page held, before any was left out: what
    /// [`Self::is_cut_short`] measures against.
    pub features: usize,
}

impl BuildingPage {
    /// Whether the server matched more buildings than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.features as u64)
    }
}

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    gfk: Option<serde_json::Value>,
    #[serde(default)]
    aog: Option<serde_json::Value>,
    #[serde(default)]
    bezeich: Option<String>,
}

/// A whole number from a property GeoServer may give as a number or as a
/// string.
pub(super) fn whole(v: &Option<serde_json::Value>) -> Option<u64> {
    match v.as_ref()? {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Read a WFS `GetFeature` page of [`super::BUILDINGS`] asked for with
/// [`BUILDING_PROPERTIES`]. A feature with no footprint of three corners
/// or more is left out.
pub fn parse_buildings(body: &[u8]) -> Result<BuildingPage, crate::features::FeatureError> {
    let page = crate::features::parse_page::<Properties>(body)?;
    let held = page.features.len();
    let buildings = page
        .features
        .into_iter()
        .filter_map(|f| {
            let outlines: Vec<Vec<[f64; 2]>> = f
                .geometry
                .outer_rings()
                .into_iter()
                .filter(|ring| ring.len() >= 4)
                .map(<[[f64; 2]]>::to_vec)
                .collect();
            if outlines.is_empty() {
                return None;
            }
            let p = f.properties;
            Some(Building {
                uuid: p.uuid.unwrap_or_default(),
                outlines,
                function: whole(&p.gfk).and_then(|c| u16::try_from(c).ok()),
                storeys: whole(&p.aog).and_then(|s| u8::try_from(s).ok()),
                part: p.bezeich.as_deref() == Some("AX_Bauteil"),
            })
        })
        .collect();
    Ok(BuildingPage {
        buildings,
        matched: page.matched,
        features: held,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_building_reads_its_footprint_use_and_storeys() {
        let body = br#"{"type":"FeatureCollection","numberMatched":3,"features":[
            {"type":"Feature","geometry":{"type":"MultiPolygon","coordinates":
              [[[[0,0],[20,0],[20,10],[0,10],[0,0]],[[5,5],[6,5],[6,6],[5,5]]]]},
             "properties":{"uuid":"A","gfk":3041,"aog":"2","bezeich":"AX_Gebaeude"}},
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[4,0],[4,4],[0,4],[0,0]]]},
             "properties":{"uuid":"B","gfk":null,"aog":7,"bezeich":"AX_Bauteil"}},
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[1,1],[0,0]]]},
             "properties":{"uuid":"C"}}
        ]}"#;
        let page = parse_buildings(body).unwrap();
        assert_eq!(page.buildings.len(), 2, "a two-corner ring is no footprint");
        let church = &page.buildings[0];
        assert_eq!(
            (church.function, church.storeys, church.part),
            (Some(3041), Some(2), false)
        );
        assert_eq!(church.usage(), BuildingUse::Religious);
        assert_eq!(church.outlines.len(), 1);
        assert_eq!(
            church.area(),
            200.0,
            "the courtyard is left out of the outline"
        );
        let part = &page.buildings[1];
        assert!(part.part && part.function.is_none());
        assert_eq!(
            (part.storeys, part.usage()),
            (Some(7), BuildingUse::Unknown)
        );
    }

    #[test]
    fn the_function_codes_sort_into_uses() {
        for (code, usage) in [
            (1010, BuildingUse::Residential),
            (1120, BuildingUse::Mixed),
            (1312, BuildingUse::Residential),
            (2020, BuildingUse::Commercial),
            (2071, BuildingUse::Commercial),
            (2143, BuildingUse::Industrial),
            (2310, BuildingUse::Mixed),
            (2461, BuildingUse::Parking),
            (2465, BuildingUse::Underground),
            (2523, BuildingUse::Utility),
            (2612, BuildingUse::Utility),
            (2721, BuildingUse::Industrial),
            (3010, BuildingUse::Public),
            (3034, BuildingUse::Cultural),
            (3041, BuildingUse::Religious),
            (3211, BuildingUse::Public),
            (9998, BuildingUse::Unknown),
        ] {
            assert_eq!(BuildingUse::from_function(code), usage, "{code}");
        }
    }
}
