//! Berlin's tree inventory (#1588): the street trees and the park trees
//! the districts keep, each a point with its genus and its measured size,
//! read from a WFS `GetFeature` page of [`super::STREET_TREES`] or
//! [`super::PARK_TREES`].
//!
//! The inventory covers the trees the city looks after - along its
//! streets, in its parks, squares and cemeteries - not its forests.

use serde::Deserialize;

/// The attributes a tree is asked for: its id, genus, height, crown
/// diameter, trunk girth, and where it stands.
pub const TREE_PROPERTIES: &[&str] = &[
    "gisid",
    "gattung",
    "baumhoehe",
    "kronedurch",
    "stammumfg",
    "geom",
];

/// The most trees one page asks for, at about 350 bytes each. A square
/// kilometre of the Tiergarten holds 2,695 park trees.
pub const TREE_PAGE: u32 = 6_000;

/// One tree of the inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryTree {
    /// The inventory's id (`gisid`): stable across fetches.
    pub id: String,
    /// Where the tree stands, E/N metres (EPSG:25833).
    pub at: [f64; 2],
    /// The genus, in Latin (`Tilia`, `Aesculus`), where the inventory names
    /// one.
    pub genus: Option<String>,
    /// The tree's height (m).
    pub height: Option<f32>,
    /// Its crown's diameter (m).
    pub crown: Option<f32>,
    /// Its trunk's girth at one metre up (cm).
    pub girth: Option<f32>,
}

/// One page of trees, and how many the request matched in all.
#[derive(Clone, Debug, PartialEq)]
pub struct TreePage {
    pub trees: Vec<InventoryTree>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
    /// How many features the page held, before any was left out: what
    /// [`Self::is_cut_short`] measures against.
    pub features: usize,
}

impl TreePage {
    /// Whether the server matched more trees than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.features as u64)
    }
}

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(default)]
    gisid: Option<String>,
    #[serde(default)]
    gattung: Option<String>,
    #[serde(default)]
    baumhoehe: Option<serde_json::Value>,
    #[serde(default)]
    kronedurch: Option<serde_json::Value>,
    #[serde(default)]
    stammumfg: Option<serde_json::Value>,
}

/// A positive measure from a property GeoServer may give as a number or as
/// a string.
fn measure(v: &Option<serde_json::Value>) -> Option<f32> {
    let v = match v.as_ref()? {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    } as f32;
    (v.is_finite() && v > 0.0).then_some(v)
}

/// Read a WFS `GetFeature` page of [`super::STREET_TREES`] or
/// [`super::PARK_TREES`] asked for with [`TREE_PROPERTIES`]. A tree with no
/// point is left out; a measure that is not a positive number is none.
pub fn parse_trees(body: &[u8]) -> Result<TreePage, crate::features::FeatureError> {
    let page = crate::features::parse_page::<Properties>(body)?;
    let held = page.features.len();
    let trees = page
        .features
        .into_iter()
        .filter_map(|f| {
            let at = f.geometry.point()?;
            let p = f.properties;
            Some(InventoryTree {
                id: p.gisid.or(f.id).unwrap_or_default(),
                at,
                genus: p
                    .gattung
                    .map(|g| g.trim().to_owned())
                    .filter(|g| !g.is_empty()),
                height: measure(&p.baumhoehe),
                crown: measure(&p.kronedurch),
                girth: measure(&p.stammumfg),
            })
        })
        .collect();
    Ok(TreePage {
        trees,
        matched: page.matched,
        features: held,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_reads_its_point_genus_and_size() {
        let body = br#"{"type":"FeatureCollection","features":[
            {"type":"Feature","id":"strassenbaeume.1","geometry":{"type":"Point","coordinates":[391500.5,5820000.25]},
             "properties":{"gisid":"00008100_0014f258","gattung":"Platanus","baumhoehe":18,"kronedurch":"20","stammumfg":239}},
            {"type":"Feature","id":"strassenbaeume.2","geometry":{"type":"Point","coordinates":[1,2]},
             "properties":{"gattung":" ","baumhoehe":"0","kronedurch":null}},
            {"type":"Feature","geometry":null,"properties":{"gisid":"x"}}
        ]}"#;
        let page = parse_trees(body).unwrap();
        assert_eq!(page.trees.len(), 2, "a tree with no point is left out");
        assert!(!page.is_cut_short());
        let plane = &page.trees[0];
        assert_eq!(plane.id, "00008100_0014f258");
        assert_eq!(plane.at, [391_500.5, 5_820_000.25]);
        assert_eq!(plane.genus.as_deref(), Some("Platanus"));
        assert_eq!(
            (plane.height, plane.crown, plane.girth),
            (Some(18.0), Some(20.0), Some(239.0))
        );
        let bare = &page.trees[1];
        assert_eq!(
            bare.id, "strassenbaeume.2",
            "no gisid: the feature's own id"
        );
        assert_eq!(
            (bare.genus.as_deref(), bare.height, bare.crown),
            (None, None, None)
        );
    }
}
