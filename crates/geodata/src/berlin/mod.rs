//! The Berlin dataset: GDI Berlin's services, the layers a geodata region
//! reads, and where on the map a region's square may lie.
//!
//! Every layer here was requested live and decoded on 2026-10-08 (#1579),
//! and every one is dl-de/zero-2.0. The services are GeoServer, in
//! EPSG:25833, with `Access-Control-Allow-Origin: *`.

mod buildings;
mod coverage;
mod coverage_table;
mod furniture;
mod streets;
mod trees;

pub use buildings::{
    BUILDING_PAGE, BUILDING_PROPERTIES, Building, BuildingPage, BuildingUse, parse_buildings,
};
pub use coverage::Coverage;
pub use furniture::{
    FURNITURE_PAGE, FurnitureItem, FurnitureKind, FurniturePage, is_lamp, parse_furniture,
};
pub use streets::{
    AXIS_PAGE, AXIS_PROPERTIES, AxisError, AxisPage, Dedication, StreetAxis, parse_axes,
};
pub use trees::{InventoryTree, TREE_PAGE, TREE_PROPERTIES, TreePage, parse_trees};

use crate::legend::ClassLegend;
use crate::request::{WfsType, WmsLayer};

/// Scheme and host of every service. The app's fetcher allows this host and
/// no other.
pub const BASE_URL: &str = "https://gdi.berlin.de";

/// The CRS every layer is requested in: ETRS89 / UTM zone 33N.
pub const EPSG: u32 = 25833;

/// The terrain: the ATKIS DGM1 bare-earth model (1 m grid), drawn in
/// one-metre height classes from 26 m to 123 m. Decode with
/// [`crate::raster::decode_terrain`] and its legend.
pub const TERRAIN: WmsLayer = WmsLayer {
    base: BASE_URL,
    service: "dgm1",
    layers: &["c_dgm1"],
    epsg: EPSG,
};

/// The surface model: the ATKIS DOM, ground with everything standing on
/// it - buildings, trees, bridges - drawn in two-metre height classes from
/// -12 m to 306 m. Decode with [`crate::raster::decode_terrain`] and its
/// legend, as the terrain; less the terrain, it is how high whatever stands
/// on the ground rises (the Berliner Dom, 90 m over its square).
pub const SURFACE: WmsLayer = WmsLayer {
    base: BASE_URL,
    service: "dom",
    layers: &["c_dom"],
    epsg: EPSG,
};

/// Land use 2015 (Umweltatlas "Reale Nutzung"): one opaque fill per
/// [`LandUse`] class, a grey block outline, and nothing over street space.
/// Decode with [`crate::raster::decode_classes`], then [`land_use_table`].
pub const LAND_USE: WmsLayer = WmsLayer {
    base: BASE_URL,
    service: "ua_flaechennutzung_2015",
    layers: &["c_ua_realnutz_2015"],
    epsg: EPSG,
};

/// Building storeys (ALKIS): one layer per [`StoreyBand`], drawn
/// half-transparent with a grey outline.
///
/// Street-level resolutions only: the outline is a pixel wide whatever the
/// scale, so at 40 m per pixel two thirds of the render is outline and
/// almost no building shows its fill (#1581). Its legend is one request per
/// layer, joined with [`ClassLegend::concat`] in this order.
pub const STOREYS: WmsLayer = WmsLayer {
    base: BASE_URL,
    service: "gebaeude_geschosse",
    layers: &[
        "a_geschosszahl_mehr_10",
        "b_geschosszahl_7_10",
        "c_geschosszahl_5_6",
        "d_geschosszahl_3_4",
        "e_geschosszahl_1_2",
        "f_geschosszahl_unter_1",
    ],
    epsg: EPSG,
};

/// Building footprints (ALKIS): multipolygons with function (`gfk`),
/// storeys above ground (`aog`), name and address, keyed by `uuid`. Read a
/// page with [`parse_buildings`], asked for with [`BUILDING_PROPERTIES`].
pub const BUILDINGS: WfsType = wfs("alkis_gebaeude", "alkis_gebaeude:gebaeude");

/// Street axes (ATKIS Basis-DLM `AX_Strassenachse`): one line per stretch
/// of street between junctions, with its carriageway width, lanes,
/// separation, function and dedication, keyed by `uuid`. Read a page with
/// [`parse_axes`], asked for with [`AXIS_PROPERTIES`].
pub const STREET_AXES: WfsType = wfs("atkis", "atkis:b08_ax_strassenachse_l");

/// Carriageway axes (ATKIS `AX_Fahrbahnachse`): each carriageway of a
/// street whose carriageways run apart, read as [`STREET_AXES`] is.
pub const CARRIAGEWAY_AXES: WfsType = wfs("atkis", "atkis:b07_ax_fahrbahnachse_l");

/// Street trees: points with species and genus, planting year, height,
/// crown diameter and trunk girth, keyed by `gisid`. Read a page with
/// [`parse_trees`], asked for with [`TREE_PROPERTIES`].
pub const STREET_TREES: WfsType = wfs("baumbestand", "baumbestand:strassenbaeume");

/// Park trees, attributed and read as [`STREET_TREES`].
pub const PARK_TREES: WfsType = wfs("baumbestand", "baumbestand:anlagenbaeume");

/// Water bodies: named polygons, whole rivers at a time (clip them).
pub const WATER: WfsType = wfs("gewaesserkarte", "gewaesserkarte:e_gew_gewaesser_fl");

/// Land-use blocks, the vector form of [`LAND_USE`]: per block its use
/// (`woz`/`grz`), urban-structure type (`typ`), sealing and built-up
/// shares, keyed by `schl5`.
pub const LAND_USE_BLOCKS: WfsType = wfs(
    "ua_flaechennutzung_2015",
    "ua_flaechennutzung_2015:c_ua_realnutz_2015",
);

/// The state boundary - an input of `tools/coverage.py`.
pub const STATE: WfsType = wfs("alkis_land", "alkis_land:landesgrenze");

/// The borough boundaries - an input of `tools/coverage.py`.
pub const BOROUGHS: WfsType = wfs("alkis_bezirke", "alkis_bezirke:bezirksgrenzen");

const fn wfs(service: &'static str, type_name: &'static str) -> WfsType {
    WfsType {
        base: BASE_URL,
        service,
        type_name,
        epsg: EPSG,
    }
}

/// The land-use classes of [`LAND_USE`], by their official English names.
/// Codes 10-90 are built-up uses (`woz`), 100-200 green and open space
/// (`grz`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandUse {
    /// Housing area (woz 10).
    Housing,
    /// Mixed area (woz 21).
    Mixed,
    /// Core area (woz 30).
    Core,
    /// Commercial and industrial area (woz 40).
    Commercial,
    /// Public / special use (woz 50).
    PublicSpecial,
    /// Utility area (woz 60).
    Utility,
    /// Weekend cottage / allotment-garden-type area (woz 70).
    Cottage,
    /// Traffic area without roads: rail, airfields (woz 80).
    Traffic,
    /// Construction site (woz 90).
    Construction,
    /// Forest (grz 100).
    Forest,
    /// Body of water (grz 110).
    Water,
    /// Meadow and pasture (grz 121).
    Meadow,
    /// Farmland (grz 122).
    Farmland,
    /// Park / green space (grz 130).
    Park,
    /// City square / promenade (grz 140).
    Square,
    /// Cemetery (grz 150).
    Cemetery,
    /// Allotment garden (grz 160).
    Allotment,
    /// Fallow area, no vegetation (grz 171).
    FallowBare,
    /// Fallow area, meadow-like vegetation (grz 172).
    FallowMeadow,
    /// Fallow area, mixed vegetation - meadows, trees, bushes (grz 173).
    FallowMixed,
    /// Sport use (grz 190).
    Sport,
    /// Tree nursery / horticulture (grz 200).
    Nursery,
}

impl LandUse {
    /// Every class, built-up uses first.
    pub const ALL: [LandUse; 22] = [
        LandUse::Housing,
        LandUse::Mixed,
        LandUse::Core,
        LandUse::Commercial,
        LandUse::PublicSpecial,
        LandUse::Utility,
        LandUse::Cottage,
        LandUse::Traffic,
        LandUse::Construction,
        LandUse::Forest,
        LandUse::Water,
        LandUse::Meadow,
        LandUse::Farmland,
        LandUse::Park,
        LandUse::Square,
        LandUse::Cemetery,
        LandUse::Allotment,
        LandUse::FallowBare,
        LandUse::FallowMeadow,
        LandUse::FallowMixed,
        LandUse::Sport,
        LandUse::Nursery,
    ];

    /// The class's code, unique across both attributes.
    pub fn code(self) -> u16 {
        match self {
            LandUse::Housing => 10,
            LandUse::Mixed => 21,
            LandUse::Core => 30,
            LandUse::Commercial => 40,
            LandUse::PublicSpecial => 50,
            LandUse::Utility => 60,
            LandUse::Cottage => 70,
            LandUse::Traffic => 80,
            LandUse::Construction => 90,
            LandUse::Forest => 100,
            LandUse::Water => 110,
            LandUse::Meadow => 121,
            LandUse::Farmland => 122,
            LandUse::Park => 130,
            LandUse::Square => 140,
            LandUse::Cemetery => 150,
            LandUse::Allotment => 160,
            LandUse::FallowBare => 171,
            LandUse::FallowMeadow => 172,
            LandUse::FallowMixed => 173,
            LandUse::Sport => 190,
            LandUse::Nursery => 200,
        }
    }

    /// The attribute the code is a value of: `woz` (built-up) or `grz`.
    pub fn attribute(self) -> &'static str {
        if self.code() < 100 { "woz" } else { "grz" }
    }

    /// The official English name (`ewoz_name` / `egrz_name`).
    pub fn name(self) -> &'static str {
        match self {
            LandUse::Housing => "Housing area",
            LandUse::Mixed => "Mixed area",
            LandUse::Core => "Core area",
            LandUse::Commercial => "Commercial and industrial area",
            LandUse::PublicSpecial => "Public / special use",
            LandUse::Utility => "Utility area",
            LandUse::Cottage => "Weekend cottage / allotment-garden-type area",
            LandUse::Traffic => "Traffic area (without roads)",
            LandUse::Construction => "Construction site",
            LandUse::Forest => "Forest",
            LandUse::Water => "Body of water",
            LandUse::Meadow => "Meadow and pasture",
            LandUse::Farmland => "Farmland",
            LandUse::Park => "Park / green space",
            LandUse::Square => "City square / promenade",
            LandUse::Cemetery => "Cemetery",
            LandUse::Allotment => "Allotment garden",
            LandUse::FallowBare => "Fallow area, no vegetation",
            LandUse::FallowMeadow => "Fallow area, meadow-like vegetation",
            LandUse::FallowMixed => "Fallow area, mixed vegetation - meadows, trees, bushes",
            LandUse::Sport => "Sport use",
            LandUse::Nursery => "Tree nursery / horticulture",
        }
    }

    /// The class with this code.
    pub fn from_code(code: u16) -> Option<LandUse> {
        LandUse::ALL.into_iter().find(|u| u.code() == code)
    }
}

/// What each class id of a decoded [`LAND_USE`] render is: entry `k` for
/// id `k` (entry 0, [`crate::raster::CLASS_NONE`], is street space and
/// `None`, as is any fill whose filter is not a known class).
pub fn land_use_table(legend: &ClassLegend) -> Vec<Option<LandUse>> {
    let classes = legend.classes.iter().map(|class| {
        let attribute = class.attribute.as_deref()?;
        let code: u16 = class.value.as_deref()?.parse().ok()?;
        LandUse::from_code(code).filter(|u| u.attribute() == attribute)
    });
    std::iter::once(None).chain(classes).collect()
}

/// The storey bands of [`STOREYS`], one per layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoreyBand {
    /// More than 10 storeys.
    Over10,
    /// 7 to 10 storeys.
    From7To10,
    /// 5 or 6 storeys.
    From5To6,
    /// 3 or 4 storeys.
    From3To4,
    /// 1 or 2 storeys.
    From1To2,
    /// Less than one storey above ground.
    Under1,
}

impl StoreyBand {
    /// Every band, in the layer order of [`STOREYS`].
    pub const ALL: [StoreyBand; 6] = [
        StoreyBand::Over10,
        StoreyBand::From7To10,
        StoreyBand::From5To6,
        StoreyBand::From3To4,
        StoreyBand::From1To2,
        StoreyBand::Under1,
    ];

    /// The band's `geschoss_kategorie`, the value its layer's rule filters on.
    pub fn category(self) -> &'static str {
        match self {
            StoreyBand::Over10 => "Geschoss_gr_10",
            StoreyBand::From7To10 => "Geschoss_7_10",
            StoreyBand::From5To6 => "Geschoss_5_6",
            StoreyBand::From3To4 => "Geschoss_3_4",
            StoreyBand::From1To2 => "Geschoss_1_2",
            StoreyBand::Under1 => "Geschoss_kl_1",
        }
    }

    /// The storeys above ground the band spans, inclusive. `Over10` is
    /// open above; 11 is its floor, not a typical value.
    pub fn storeys(self) -> (u8, u8) {
        match self {
            StoreyBand::Over10 => (11, u8::MAX),
            StoreyBand::From7To10 => (7, 10),
            StoreyBand::From5To6 => (5, 6),
            StoreyBand::From3To4 => (3, 4),
            StoreyBand::From1To2 => (1, 2),
            StoreyBand::Under1 => (0, 0),
        }
    }
}

/// What each class id of a decoded [`STOREYS`] render is, as
/// [`land_use_table`] is for land use.
pub fn storey_table(legend: &ClassLegend) -> Vec<Option<StoreyBand>> {
    let classes = legend.classes.iter().map(|class| {
        let value = class.value.as_deref()?;
        StoreyBand::ALL.into_iter().find(|b| b.category() == value)
    });
    std::iter::once(None).chain(classes).collect()
}

/// Berlin's twelve boroughs, by their ALKIS `gem` code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Borough {
    /// 1.
    Mitte,
    /// 2.
    FriedrichshainKreuzberg,
    /// 3.
    Pankow,
    /// 4.
    CharlottenburgWilmersdorf,
    /// 5.
    Spandau,
    /// 6.
    SteglitzZehlendorf,
    /// 7.
    TempelhofSchoeneberg,
    /// 8.
    Neukoelln,
    /// 9.
    TreptowKoepenick,
    /// 10.
    MarzahnHellersdorf,
    /// 11.
    Lichtenberg,
    /// 12.
    Reinickendorf,
}

impl Borough {
    /// Every borough, in code order.
    pub const ALL: [Borough; 12] = [
        Borough::Mitte,
        Borough::FriedrichshainKreuzberg,
        Borough::Pankow,
        Borough::CharlottenburgWilmersdorf,
        Borough::Spandau,
        Borough::SteglitzZehlendorf,
        Borough::TempelhofSchoeneberg,
        Borough::Neukoelln,
        Borough::TreptowKoepenick,
        Borough::MarzahnHellersdorf,
        Borough::Lichtenberg,
        Borough::Reinickendorf,
    ];

    /// The ALKIS `gem` code, 1 to 12.
    pub fn code(self) -> u8 {
        Borough::ALL.iter().position(|&b| b == self).unwrap_or(0) as u8 + 1
    }

    /// The borough with this code.
    pub fn from_code(code: u8) -> Option<Borough> {
        Borough::ALL.get(usize::from(code).checked_sub(1)?).copied()
    }

    /// The official name.
    pub fn name(self) -> &'static str {
        match self {
            Borough::Mitte => "Mitte",
            Borough::FriedrichshainKreuzberg => "Friedrichshain-Kreuzberg",
            Borough::Pankow => "Pankow",
            Borough::CharlottenburgWilmersdorf => "Charlottenburg-Wilmersdorf",
            Borough::Spandau => "Spandau",
            Borough::SteglitzZehlendorf => "Steglitz-Zehlendorf",
            Borough::TempelhofSchoeneberg => "Tempelhof-Sch\u{f6}neberg",
            Borough::Neukoelln => "Neuk\u{f6}lln",
            Borough::TreptowKoepenick => "Treptow-K\u{f6}penick",
            Borough::MarzahnHellersdorf => "Marzahn-Hellersdorf",
            Borough::Lichtenberg => "Lichtenberg",
            Borough::Reinickendorf => "Reinickendorf",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legend::FillClass;

    #[test]
    fn land_use_codes_are_unique_and_round_trip() {
        for u in LandUse::ALL {
            assert_eq!(LandUse::from_code(u.code()), Some(u));
        }
        let mut codes: Vec<u16> = LandUse::ALL.iter().map(|u| u.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LandUse::ALL.len());
        assert_eq!(LandUse::from_code(11), None);
    }

    #[test]
    fn borough_codes_round_trip() {
        for (i, b) in Borough::ALL.into_iter().enumerate() {
            assert_eq!(b.code(), i as u8 + 1);
            assert_eq!(Borough::from_code(b.code()), Some(b));
        }
        assert_eq!(Borough::from_code(0), None);
        assert_eq!(Borough::from_code(13), None);
    }

    fn fill(attribute: &str, value: &str) -> FillClass {
        FillClass {
            rgb: [0, 0, 0],
            opacity: 1.0,
            layer: "l".into(),
            name: None,
            attribute: Some(attribute.into()),
            value: Some(value.into()),
        }
    }

    #[test]
    fn tables_name_each_class_id() {
        let legend = ClassLegend {
            classes: vec![
                fill("grz", "100"),
                fill("woz", "10"),
                fill("woz", "100"),
                fill("grz", "x"),
            ],
        };
        assert_eq!(
            land_use_table(&legend),
            vec![
                None,
                Some(LandUse::Forest),
                Some(LandUse::Housing),
                None,
                None
            ]
        );
        let storeys = ClassLegend {
            classes: vec![
                fill("geschoss_kategorie", "Geschoss_kl_1"),
                fill("geschoss_kategorie", "Geschoss_99"),
            ],
        };
        assert_eq!(
            storey_table(&storeys),
            vec![None, Some(StoreyBand::Under1), None]
        );
    }
}
