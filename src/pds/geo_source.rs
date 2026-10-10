//! A geodata region's source on the room record (#1583, epic #1580; design
//! in `docs/geodata.md`): the square of real map data the region is built
//! from, and what the owner changed of what is drawn from it.
//!
//! A record with no source is the procedural world every record was before
//! #1580, and stays byte-identical on the wire: the field is elided when
//! absent. A record with one names a dataset and a square of it in whole
//! metres - integers only, as the wire carries no floats.
//!
//! The source is also how an owner keeps a square: once DID-seeded regions
//! can draw Berlin (#1589), a seeded square is drawn afresh from the seed on
//! every visit, and saving it stores it here.
//!
//! What is drawn from the square - its buildings, trees and street
//! furniture - is never in the record; it is derived again on every visit
//! (#1588). What the owner changed of it is (#1590): the items removed, and
//! the items made the world's own, each named by its stable source id
//! (`alkis:<uuid>`, `tree:<gisid>`, `furniture:<id>`), neither of them drawn
//! any more. An adopted item's copy is ordinary record content: the
//! placements of the generators named after it ([`adopted_generator_name`]).
//! An id the square's data no longer holds is kept, and ignored. And the
//! content hash of each layer the world was drawn from at its last save, so
//! a visitor whose cache holds older answers fetches them again. All three
//! are elided while empty, so a source without edits keeps the bytes it had.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use geodata::GeoSquare;
use geodata::berlin::Coverage;
use geodata::square::snap_size;

/// The dataset id of GDI Berlin.
pub const BERLIN: &str = "berlin";

/// The longest dataset id a record may carry.
pub const MAX_DATASET_CHARS: usize = 32;

/// The longest source id a record may carry: an ALKIS id is 16 characters,
/// and the longest id a survey writes not much more.
pub const MAX_SOURCE_ID_CHARS: usize = 96;

/// The most items each list of edits may name. Removing is one click, so a
/// list could otherwise grow without end; past this, an edit is refused
/// before it is made.
pub const MAX_EDITS: usize = 1_024;

/// The most layer hashes a record may carry: a Berlin region draws nine.
pub const MAX_LAYERS: usize = 32;

/// The longest layer name.
pub const MAX_LAYER_CHARS: usize = 32;

/// The digits of a layer hash: 64 bits in lower-case hex.
pub const LAYER_HASH_DIGITS: usize = 16;

/// Where a geodata region's ground comes from, and what the owner changed
/// of what is drawn from it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct GeoSource {
    /// Which dataset: [`BERLIN`] today. An id this build does not know is
    /// kept, with its square, so a newer client's source survives an older
    /// client's save - though keys this build does not know are dropped on
    /// that save, as they are from every record field - and the region draws
    /// procedurally here.
    pub dataset: String,
    /// Easting of the square's west edge, metres in the dataset's CRS
    /// (EPSG:25833 for Berlin).
    pub min_e: i32,
    /// Northing of the square's south edge, metres.
    pub min_n: i32,
    /// The square's side, metres.
    pub size_m: u32,
    /// The derived items the owner removed, by source id, sorted: none of
    /// them is drawn (#1590).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// The derived items the owner made the world's own, by source id,
    /// sorted: none of them is drawn, its copy standing in the record
    /// instead (#1590).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adopted: Vec<String>,
    /// The content hash of each layer the world was drawn from when it was
    /// last saved, [`LAYER_HASH_DIGITS`] hex digits by layer name (#1590).
    /// Written by the save from what it drew, never by hand.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<String, String>,
}

/// The name of the `n`th generator of the copy of the derived item `id`
/// made the world's own: `alkis:DEBE00YY11100001#1`. A footprint's row can
/// be several catalogue buildings, each a generator its copies share.
pub fn adopted_generator_name(id: &str, n: usize) -> String {
    format!("{id}#{n}")
}

/// The source id a generator name was made from by
/// [`adopted_generator_name`], if it was.
pub fn adopted_source_of(name: &str) -> Option<&str> {
    let (id, n) = name.rsplit_once('#')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && is_source_id(id)).then_some(id)
}

/// Whether `id` is a source id a record may carry: 1 to
/// [`MAX_SOURCE_ID_CHARS`] of ASCII letters, digits and `.`, `_`, `:`, `-`.
pub fn is_source_id(id: &str) -> bool {
    (1..=MAX_SOURCE_ID_CHARS).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// How an item stands among the owner's edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Drawn from the data, as it is.
    Drawn,
    /// Removed: drawn no more.
    Removed,
    /// Made the world's own: drawn no more, its copy in the record.
    Adopted,
}

impl GeoSource {
    /// A Berlin source over `square`.
    pub fn berlin(square: GeoSquare) -> Self {
        GeoSource {
            dataset: BERLIN.to_owned(),
            min_e: square.min_e,
            min_n: square.min_n,
            size_m: square.size_m,
            ..Default::default()
        }
    }

    /// The square, whatever the dataset.
    pub fn square(&self) -> GeoSquare {
        GeoSquare {
            min_e: self.min_e,
            min_n: self.min_n,
            size_m: self.size_m,
        }
    }

    /// The Berlin square this source names, if it is a Berlin source -
    /// what a build that draws Berlin reads. After [`Self::sanitize`] the
    /// square lies wholly inside Berlin.
    pub fn berlin_square(&self) -> Option<GeoSquare> {
        (self.dataset == BERLIN).then(|| self.square())
    }

    /// This source moved to `square`: its edits kept - an id names its item
    /// wherever the square lies - and its layer hashes dropped, as they
    /// were the old square's.
    pub fn moved_to(&self, square: GeoSquare) -> Self {
        GeoSource {
            min_e: square.min_e,
            min_n: square.min_n,
            size_m: square.size_m,
            layers: BTreeMap::new(),
            ..self.clone()
        }
    }

    /// This source with none of the owner's edits: what a re-roll keeps
    /// under the square lock, which replaces everything else the world
    /// holds, the copies of adopted items with it.
    pub fn without_edits(&self) -> Self {
        GeoSource {
            removed: Vec::new(),
            adopted: Vec::new(),
            ..self.clone()
        }
    }

    /// How the item `id` stands among the edits.
    pub fn edit_of(&self, id: &str) -> Edit {
        if self
            .adopted
            .binary_search_by(|a| a.as_str().cmp(id))
            .is_ok()
        {
            Edit::Adopted
        } else if self
            .removed
            .binary_search_by(|r| r.as_str().cmp(id))
            .is_ok()
        {
            Edit::Removed
        } else {
            Edit::Drawn
        }
    }

    /// Set how the item `id` stands, keeping both lists sorted, or say why
    /// it cannot be: an id that is not one, or a list already holding
    /// [`MAX_EDITS`].
    pub fn set_edit(&mut self, id: &str, edit: Edit) -> Result<(), String> {
        if !is_source_id(id) {
            return Err(format!("\"{id}\" is not an item's id."));
        }
        let take = |list: &mut Vec<String>| {
            if let Ok(at) = list.binary_search_by(|e| e.as_str().cmp(id)) {
                list.remove(at);
            }
        };
        let list = match edit {
            Edit::Drawn => None,
            Edit::Removed => Some(&self.removed),
            Edit::Adopted => Some(&self.adopted),
        };
        if list.is_some_and(|list| {
            list.len() >= MAX_EDITS && list.binary_search_by(|e| e.as_str().cmp(id)).is_err()
        }) {
            return Err(format!(
                "This world already holds {MAX_EDITS} such changes to Berlin, the most it can."
            ));
        }
        take(&mut self.removed);
        take(&mut self.adopted);
        let list = match edit {
            Edit::Drawn => return Ok(()),
            Edit::Removed => &mut self.removed,
            Edit::Adopted => &mut self.adopted,
        };
        let at = list
            .binary_search_by(|e| e.as_str().cmp(id))
            .unwrap_or_else(|at| at);
        list.insert(at, id.to_owned());
        Ok(())
    }

    /// Whether the generator `name` is part of the copy of an item this
    /// source names adopted.
    pub fn is_adopted_generator(&self, name: &str) -> bool {
        adopted_source_of(name).is_some_and(|id| self.edit_of(id) == Edit::Adopted)
    }

    /// Make the source safe to build from, or say it cannot be: `false`
    /// means the caller drops it and the region draws procedurally.
    ///
    /// The dataset id must be 1 to [`MAX_DATASET_CHARS`] of `a-z`, `0-9`,
    /// `-` and `_`. A Berlin square has its side made drawable
    /// ([`snap_size`]: a whole 10 m within 250 m - 19 km) and is moved, if it
    /// has to be, to the nearest position wholly inside Berlin
    /// ([`Coverage::nearest`]) - so a hand-edited or hostile
    /// record still lands on the map, as close to what it asked for as it
    /// can. Another dataset's square is kept as it is: this build cannot
    /// judge it, and only a build that can will read it.
    ///
    /// The edits, whatever the dataset: an id that is not one
    /// ([`is_source_id`]) is dropped, each list sorted, deduplicated and cut
    /// to [`MAX_EDITS`], and an id in both kept only as adopted, which has
    /// content. A layer hash is kept only under a name of 1 to
    /// [`MAX_LAYER_CHARS`] of `a-z`, `0-9` and `_`, as [`LAYER_HASH_DIGITS`]
    /// lower-case hex digits, at most [`MAX_LAYERS`] of them.
    pub fn sanitize(&mut self) -> bool {
        let id_ok = (1..=MAX_DATASET_CHARS).contains(&self.dataset.len())
            && self
                .dataset
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !id_ok {
            return false;
        }
        self.sanitize_edits();
        if self.dataset != BERLIN {
            return true;
        }
        let size = snap_size(self.size_m);
        match Coverage::berlin().nearest(size, i64::from(self.min_e), i64::from(self.min_n)) {
            Some(square) => {
                self.min_e = square.min_e;
                self.min_n = square.min_n;
                self.size_m = square.size_m;
                true
            }
            None => false,
        }
    }

    /// The edits' half of [`Self::sanitize`].
    fn sanitize_edits(&mut self) {
        let tidy = |list: &mut Vec<String>| {
            list.retain(|id| is_source_id(id));
            list.sort_unstable();
            list.dedup();
            list.truncate(MAX_EDITS);
        };
        tidy(&mut self.adopted);
        tidy(&mut self.removed);
        let adopted = &self.adopted;
        self.removed
            .retain(|id| adopted.binary_search_by(|a| a.cmp(id)).is_err());
        let name_ok = |name: &str| {
            (1..=MAX_LAYER_CHARS).contains(&name.len())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let hash_ok = |hash: &str| {
            hash.len() == LAYER_HASH_DIGITS
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        self.layers
            .retain(|name, hash| name_ok(name) && hash_ok(hash));
        while self.layers.len() > MAX_LAYERS {
            self.layers.pop_last();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn berlin(min_e: i32, min_n: i32, size_m: u32) -> GeoSource {
        GeoSource {
            dataset: BERLIN.into(),
            min_e,
            min_n,
            size_m,
            ..Default::default()
        }
    }

    #[test]
    fn a_square_inside_berlin_is_kept_as_it_is() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        assert!(source.sanitize());
        assert_eq!(source, berlin(391_000, 5_819_500, 1_000));
        assert_eq!(source.berlin_square(), Some(source.square()));
    }

    #[test]
    fn a_square_off_the_map_is_moved_onto_it_and_its_side_made_drawable() {
        use geodata::square::{SIZE_MAX_M, SIZE_MIN_M};
        let coverage = Coverage::berlin();
        // Potsdam: off the map, and far too small.
        let mut source = berlin(368_000, 5_806_000, 10);
        assert!(source.sanitize());
        assert_eq!(source.size_m, SIZE_MIN_M);
        // Off the 10 m step: snapped, so a centre is a whole metre.
        let mut odd = berlin(391_000, 5_819_500, 1_003);
        assert!(odd.sanitize());
        assert_eq!(odd.size_m, 1_000);
        assert!(coverage.contains(&source.square()));
        // Too big for any place in Berlin: clamped to the largest that fits.
        let mut huge = berlin(391_000, 5_819_500, u32::MAX);
        assert!(huge.sanitize());
        assert_eq!(huge.size_m, SIZE_MAX_M);
        assert!(coverage.contains(&huge.square()));
        // Coordinates at the ends of the type.
        let mut wild = berlin(i32::MIN, i32::MAX, 5_000);
        assert!(wild.sanitize());
        assert!(coverage.contains(&wild.square()));
    }

    #[test]
    fn another_dataset_is_kept_verbatim_and_draws_nothing_here() {
        let mut source = GeoSource {
            dataset: "hamburg".into(),
            min_e: 1,
            min_n: 2,
            size_m: 3,
            removed: vec!["harbour:crane-7".into()],
            ..Default::default()
        };
        let before = source.clone();
        assert!(source.sanitize());
        assert_eq!(source, before);
        assert_eq!(source.berlin_square(), None);
    }

    #[test]
    fn a_dataset_id_that_is_not_a_plain_name_drops_the_source() {
        for id in [
            "",
            "Berlin",
            "berlin ",
            "ber/lin",
            "berlin\u{200b}",
            "x".repeat(MAX_DATASET_CHARS + 1).as_str(),
        ] {
            let mut source = GeoSource {
                dataset: id.into(),
                ..berlin(391_000, 5_819_500, 1_000)
            };
            assert!(!source.sanitize(), "{id:?}");
        }
        let mut longest = GeoSource {
            dataset: "x".repeat(MAX_DATASET_CHARS),
            ..berlin(0, 0, 0)
        };
        assert!(longest.sanitize());
    }

    #[test]
    fn edits_are_kept_sorted_and_an_item_stands_in_one_list() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        assert_eq!(source.edit_of("tree:00008100_0014f258"), Edit::Drawn);
        source
            .set_edit("tree:00008100_0014f258", Edit::Removed)
            .unwrap();
        source
            .set_edit("alkis:DEBE00YY13U0001a", Edit::Removed)
            .unwrap();
        assert_eq!(
            source.removed,
            ["alkis:DEBE00YY13U0001a", "tree:00008100_0014f258"]
        );
        // Adopting a removed item moves it; restoring takes it out of both.
        source
            .set_edit("alkis:DEBE00YY13U0001a", Edit::Adopted)
            .unwrap();
        assert_eq!(source.removed, ["tree:00008100_0014f258"]);
        assert_eq!(source.edit_of("alkis:DEBE00YY13U0001a"), Edit::Adopted);
        source
            .set_edit("tree:00008100_0014f258", Edit::Drawn)
            .unwrap();
        assert!(source.removed.is_empty());
        assert!(source.set_edit("not an id", Edit::Removed).is_err());
        assert!(source.set_edit("", Edit::Removed).is_err());
        // The copy's generators name the adopted item.
        let name = adopted_generator_name("alkis:DEBE00YY13U0001a", 2);
        assert_eq!(name, "alkis:DEBE00YY13U0001a#2");
        assert_eq!(adopted_source_of(&name), Some("alkis:DEBE00YY13U0001a"));
        assert!(source.is_adopted_generator(&name));
        assert!(!source.is_adopted_generator("tree:00008100_0014f258#1"));
        for plain in ["house", "house#", "#1", "a b#1", "house#1a"] {
            assert_eq!(adopted_source_of(plain), None, "{plain:?}");
        }
    }

    #[test]
    fn a_full_list_refuses_one_more_and_says_why() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        source.removed = (0..MAX_EDITS).map(|i| format!("tree:{i:05}")).collect();
        source.removed.sort();
        let refused = source.set_edit("tree:new", Edit::Removed);
        assert!(refused.unwrap_err().contains("1024"));
        // One already there is no new edit.
        source.set_edit("tree:00007", Edit::Removed).unwrap();
        // And the other list has room of its own.
        source.set_edit("tree:new", Edit::Adopted).unwrap();
    }

    #[test]
    fn a_moved_square_keeps_its_edits_and_drops_its_hashes() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        source.removed = vec!["tree:1".into()];
        source
            .layers
            .insert("terrain".into(), "0123456789abcdef".into());
        let square = GeoSquare {
            min_e: 392_000,
            min_n: 5_820_000,
            size_m: 600,
        };
        let moved = source.moved_to(square);
        assert_eq!(moved.square(), square);
        assert_eq!(moved.removed, source.removed);
        assert!(moved.layers.is_empty());
        let rerolled = source.without_edits();
        assert!(rerolled.removed.is_empty());
        assert_eq!(rerolled.layers, source.layers);
        assert_eq!(rerolled.square(), source.square());
    }

    #[test]
    fn hostile_edits_and_hashes_are_put_right() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        source.removed = vec![
            "tree:2".into(),
            "tree:1".into(),
            "tree:2".into(),
            "alkis:A".into(),
            "has space".into(),
            "quote\"".into(),
            "x".repeat(MAX_SOURCE_ID_CHARS + 1),
            String::new(),
        ];
        source.adopted = vec!["alkis:A".into()];
        source.layers = [
            ("terrain", "0123456789abcdef"),
            ("land_use", "0123456789ABCDEF"),
            ("streets", "0123"),
            ("Trees", "0123456789abcdef"),
            ("", "0123456789abcdef"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert!(source.sanitize());
        assert_eq!(source.removed, ["tree:1", "tree:2"], "adopted wins");
        assert_eq!(source.adopted, ["alkis:A"]);
        assert_eq!(
            source.layers.keys().collect::<Vec<_>>(),
            ["terrain"],
            "only a plain name with a lower-case 16-digit hash"
        );
        // Too many of each: cut, deterministically.
        source.removed = (0..MAX_EDITS + 5)
            .rev()
            .map(|i| format!("tree:{i:05}"))
            .collect();
        source.layers = (0..MAX_LAYERS + 3)
            .map(|i| (format!("layer{i:02}"), "0123456789abcdef".to_owned()))
            .collect();
        assert!(source.sanitize());
        assert_eq!(source.removed.len(), MAX_EDITS);
        assert_eq!(source.removed[0], "tree:00000");
        assert_eq!(source.layers.len(), MAX_LAYERS);
        assert!(source.layers.contains_key("layer00"));
    }
}
