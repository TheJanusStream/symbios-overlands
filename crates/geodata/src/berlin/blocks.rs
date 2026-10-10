//! Berlin's blocks by how they are built up (#1600): the Environmental
//! Atlas's urban structure, "Stadtstruktur - Flaechentypen differenziert"
//! for 2021-2024, read from a WFS `GetFeature` page of
//! [`super::URBAN_STRUCTURE`].
//!
//! The atlas parts the city into some 26,600 blocks - the ground between
//! its streets - and types each by how it is built up, in about fifty
//! types (`typ`): a closed Wilhelminian block with its rear courtyards, a
//! 1960s estate of towers in green, a street of detached houses in their
//! gardens, a commercial area of sheds and yards, a park. [`Development`]
//! sorts them into the families a building's placement reads: whether a
//! house stands flush against its neighbours on the street line, free in
//! its garden, or in a slab among lawns.

use serde::Deserialize;

use super::buildings::whole;

/// The attributes a block is asked for: its key, its type and its outline.
pub const BLOCK_PROPERTIES: &[&str] = &["schluessel", "typ", "geom"];

/// The most blocks one page asks for: a square kilometre holds about 30
/// to 90 - the Museumsinsel's 600 m square 32, Hermannplatz's 2 km square
/// 169.
pub const BLOCK_PAGE: u32 = 1_000;

/// How a block is built up, coarsely: the family of its urban-structure
/// type (`typ`), as a building's placement reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Development {
    /// Closed and semi-open block edges - the Wilhelminian blocks with
    /// their courtyards, the 1920s-40s quadrangles, the post-war gap
    /// closures, the inner city's mixed blocks and its core (types 1-3,
    /// 6-8, 10, 29, 38): houses stand flush, party wall to party wall, on
    /// the street line.
    Perimeter,
    /// Estates of rows, slabs and towers standing free in green - the
    /// 1920s-30s parallel rows, the 1950s-70s free rows, the large estates
    /// of the 1960s-90s, the rental flats of the 1990s and later (9, 11,
    /// 72, 73): a building turns its long side to its lawns, not to the
    /// street.
    Estate,
    /// Houses in gardens - detached homes, villas, a village's mix, a
    /// single-family area densified (21, 23, 24, 25): each house stands
    /// free.
    Houses,
    /// Row houses and duplexes with their yards (22).
    RowHouses,
    /// Commercial and industrial areas and large-scale retail, utilities,
    /// and the sparser non-residential mix (30-33): halls, sheds, yards.
    Works,
    /// Public facilities - schools, hospitals, universities, offices of
    /// state, police and fire, culture, churches, day care and youth
    /// centres, covered sport, other special uses (12, 13, 17, 41, 43-47,
    /// 49, 51, 60).
    Civic,
    /// Allotment gardens, weekend plots and camping grounds (37, 58, 59):
    /// huts and cottages, each standing free.
    Gardens,
    /// Open land and the rest - parks, forest, water, farmland, cemeteries,
    /// sports grounds, railway, traffic and parking areas, building sites -
    /// and any type this build does not know.
    Open,
}

impl Development {
    /// The family of urban-structure type `typ`.
    pub fn from_type(typ: u16) -> Self {
        match typ {
            1..=3 | 6..=8 | 10 | 29 | 38 => Development::Perimeter,
            9 | 11 | 72 | 73 => Development::Estate,
            21 | 23..=25 => Development::Houses,
            22 => Development::RowHouses,
            30..=33 => Development::Works,
            12 | 13 | 17 | 41 | 43..=47 | 49 | 51 | 60 => Development::Civic,
            37 | 58 | 59 => Development::Gardens,
            _ => Development::Open,
        }
    }
}

/// One block.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The atlas's block key (`schluessel`): stable across fetches.
    pub key: String,
    /// Its urban-structure type, where the atlas has one that reads.
    pub typ: Option<u16>,
    /// Its outline's rings, E/N metres (EPSG:25833), each closed (the
    /// first point repeated last): every polygon's outer ring and its
    /// holes, as an even-odd test reads them - a block that rings another
    /// has the other as a hole.
    pub rings: Vec<Vec<[f64; 2]>>,
}

impl Block {
    /// How it is built up: [`Development::Open`] where its type is unknown.
    pub fn development(&self) -> Development {
        self.typ.map_or(Development::Open, Development::from_type)
    }

    /// Whether `point` (E/N metres) lies inside it: inside an odd number
    /// of its rings.
    pub fn contains(&self, point: [f64; 2]) -> bool {
        self.rings
            .iter()
            .filter(|ring| ring_contains(ring, point))
            .count()
            % 2
            == 1
    }
}

/// Whether `point` lies inside the closed `ring`, by the even-odd rule.
fn ring_contains(ring: &[[f64; 2]], [x, y]: [f64; 2]) -> bool {
    let mut inside = false;
    for edge in ring.windows(2) {
        let ([x0, y0], [x1, y1]) = (edge[0], edge[1]);
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) / (y1 - y0) * (x1 - x0) {
            inside = !inside;
        }
    }
    inside
}

/// One page of blocks, and how many the request matched in all.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockPage {
    pub blocks: Vec<Block>,
    /// `numberMatched`, where the server counted.
    pub matched: Option<u64>,
    /// How many features the page held, before any was left out: what
    /// [`Self::is_cut_short`] measures against.
    pub features: usize,
}

impl BlockPage {
    /// Whether the server matched more blocks than the page holds.
    pub fn is_cut_short(&self) -> bool {
        self.matched
            .is_some_and(|matched| matched > self.features as u64)
    }
}

#[derive(Deserialize, Default)]
struct Properties {
    #[serde(default)]
    schluessel: Option<String>,
    #[serde(default)]
    typ: Option<serde_json::Value>,
}

/// Read a WFS `GetFeature` page of [`super::URBAN_STRUCTURE`] asked for
/// with [`BLOCK_PROPERTIES`]. A feature with no ring of three corners or
/// more is left out.
pub fn parse_blocks(body: &[u8]) -> Result<BlockPage, crate::features::FeatureError> {
    let page = crate::features::parse_page::<Properties>(body)?;
    let held = page.features.len();
    let blocks = page
        .features
        .into_iter()
        .filter_map(|f| {
            let rings: Vec<Vec<[f64; 2]>> = f
                .geometry
                .rings()
                .into_iter()
                .filter(|ring| ring.len() >= 4)
                .map(<[[f64; 2]]>::to_vec)
                .collect();
            if rings.is_empty() {
                return None;
            }
            let p = f.properties;
            Some(Block {
                key: p.schluessel.or(f.id).unwrap_or_default(),
                typ: whole(&p.typ).and_then(|t| u16::try_from(t).ok()),
                rings,
            })
        })
        .collect();
    Ok(BlockPage {
        blocks,
        matched: page.matched,
        features: held,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_reads_its_key_type_and_rings() {
        let body = br#"{"type":"FeatureCollection","numberMatched":4,"features":[
            {"type":"Feature","id":"b.1","geometry":{"type":"MultiPolygon","coordinates":
              [[[[0,0],[100,0],[100,100],[0,100],[0,0]],[[40,40],[60,40],[60,60],[40,60],[40,40]]]]},
             "properties":{"schluessel":"0100","typ":"23"}},
            {"type":"Feature","id":"b.2","geometry":{"type":"Polygon","coordinates":
              [[[40,40],[60,40],[60,60],[40,60],[40,40]]]},
             "properties":{"schluessel":"0200","typ":2}},
            {"type":"Feature","id":"b.3","geometry":{"type":"Polygon","coordinates":
              [[[0,0],[1,1],[0,0]]]},"properties":{"typ":"9"}},
            {"type":"Feature","id":"b.4","geometry":{"type":"Polygon","coordinates":
              [[[0,0],[5,0],[5,5],[0,0]]]},"properties":{"typ":"x"}}
        ]}"#;
        let page = parse_blocks(body).unwrap();
        assert!(!page.is_cut_short());
        assert_eq!(page.blocks.len(), 3, "a two-corner ring is no block");
        let houses = &page.blocks[0];
        assert_eq!(
            (houses.key.as_str(), houses.typ, houses.rings.len()),
            ("0100", Some(23), 2)
        );
        assert_eq!(houses.development(), Development::Houses);
        assert!(houses.contains([10.0, 10.0]));
        assert!(
            !houses.contains([50.0, 50.0]),
            "the hole is the other block's"
        );
        assert!(!houses.contains([150.0, 50.0]));
        let block = &page.blocks[1];
        assert_eq!(block.typ, Some(2), "a numeric type reads");
        assert_eq!(block.development(), Development::Perimeter);
        assert!(block.contains([50.0, 50.0]));
        let odd = &page.blocks[2];
        assert_eq!(odd.key, "b.4", "no key: the feature's id");
        assert_eq!(
            (odd.typ, odd.development()),
            (None, Development::Open),
            "a type that does not read"
        );
    }

    #[test]
    fn the_types_sort_into_families() {
        use Development::*;
        for (typ, family) in [
            (1, Perimeter),
            (2, Perimeter),
            (3, Perimeter),
            (7, Perimeter),
            (10, Perimeter),
            (29, Perimeter),
            (38, Perimeter),
            (9, Estate),
            (11, Estate),
            (72, Estate),
            (73, Estate),
            (21, Houses),
            (23, Houses),
            (24, Houses),
            (25, Houses),
            (22, RowHouses),
            (30, Works),
            (31, Works),
            (32, Works),
            (33, Works),
            (12, Civic),
            (17, Civic),
            (46, Civic),
            (49, Civic),
            (60, Civic),
            (37, Gardens),
            (59, Gardens),
            (53, Open),
            (55, Open),
            (99, Open),
            (100, Open),
            (0, Open),
            (999, Open),
        ] {
            assert_eq!(Development::from_type(typ), family, "{typ}");
        }
    }
}
