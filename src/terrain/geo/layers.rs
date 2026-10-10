//! The layers of Berlin a region is drawn from, as a save names them
//! (#1590): each one's content hash, so a visitor whose cache holds older
//! answers than the owner saved with can tell, and fetch them again.
//!
//! A layer's hash folds the content hashes of its answers
//! ([`crate::geodata::content_hash`]) in the order the job asks for them:
//! the terrain's legend then its render, each tree inventory's page, each
//! kind of furniture's. It is had only where every one of its answers was:
//! a layer left out draws nothing, so says nothing either.
//!
//! The cure for the divergence `docs/geodata.md` names: each peer keeps its
//! own 30-day cache, so if Berlin re-renders its data within that window two
//! peers can hold different ground. With the hashes the owner's save
//! recorded, a peer whose stored answer hashes otherwise fetches it past
//! the store once, and draws what the network says - which, where Berlin
//! has changed since the save, still differs, and is said
//! ([`DrawnLayers::changed`]); the owner's next save records it.

use std::collections::BTreeMap;

use geodata::GeoSquare;

use crate::pds::GeoSource;

/// A layer of Berlin a region is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Layer {
    /// The terrain's legend and render.
    Terrain,
    /// The land use's legend and render.
    LandUse,
    /// The street and carriageway axes.
    Streets,
    /// The buildings' page.
    Buildings,
    /// The blocks' page, by urban-structure type (#1600).
    Blocks,
    /// The two tree inventories' pages.
    Trees,
    /// Each furniture kind's page.
    Furniture,
    /// The far field's terrain and land-use renders.
    Horizon,
    /// The middle ring's surface legend and its two renders.
    Ring,
}

impl Layer {
    pub(crate) const ALL: [Layer; 9] = [
        Layer::Terrain,
        Layer::LandUse,
        Layer::Streets,
        Layer::Buildings,
        Layer::Blocks,
        Layer::Trees,
        Layer::Furniture,
        Layer::Horizon,
        Layer::Ring,
    ];

    /// Its name in a record's `layers`.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Layer::Terrain => "terrain",
            Layer::LandUse => "land_use",
            Layer::Streets => "streets",
            Layer::Buildings => "buildings",
            Layer::Blocks => "blocks",
            Layer::Trees => "trees",
            Layer::Furniture => "furniture",
            Layer::Horizon => "horizon",
            Layer::Ring => "ring",
        }
    }

    /// What it is, in words.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Layer::Terrain => "terrain",
            Layer::LandUse => "land use",
            Layer::Streets => "streets",
            Layer::Buildings => "buildings",
            Layer::Blocks => "block types",
            Layer::Trees => "trees",
            Layer::Furniture => "street furniture",
            Layer::Horizon => "horizon",
            Layer::Ring => "buildings round the walkable ground",
        }
    }

    /// The layer a record names `key`.
    pub(crate) fn of_key(key: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|layer| layer.key() == key)
    }
}

/// The hashes a record was saved with, by the layers this build knows: a
/// name it does not know, or a hash that is not one, is passed over.
pub(crate) fn saved_hashes(source: Option<&GeoSource>) -> BTreeMap<Layer, u64> {
    source
        .map(|source| {
            source
                .layers
                .iter()
                .filter_map(|(key, hash)| {
                    Some((Layer::of_key(key)?, u64::from_str_radix(hash, 16).ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What a region's ground was drawn from: the square it was fetched for,
/// each layer it drew by its content hash, and those whose answers differ
/// from the hashes its record was saved with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DrawnLayers {
    pub square: GeoSquare,
    pub hashes: BTreeMap<Layer, u64>,
    /// The layers that differ from the record's saved hashes after a fetch
    /// past the cache: Berlin's data has changed since the save.
    pub changed: Vec<Layer>,
}

impl DrawnLayers {
    /// The layers drawn over `square` with `hashes`, against the hashes
    /// `saved` a record holds.
    pub(crate) fn new(
        square: GeoSquare,
        hashes: BTreeMap<Layer, u64>,
        saved: &BTreeMap<Layer, u64>,
    ) -> Self {
        let changed = hashes
            .iter()
            .filter(|(layer, hash)| saved.get(layer).is_some_and(|saved| saved != *hash))
            .map(|(layer, _)| *layer)
            .collect();
        DrawnLayers {
            square,
            hashes,
            changed,
        }
    }

    /// Write the drawn layers' hashes into `source`, a record's about to be
    /// saved, where it names the square they were drawn for: each drawn
    /// layer's hash in place of the one it held, the rest - a layer that
    /// could not be had this visit - kept.
    pub(crate) fn stamp(&self, source: &mut GeoSource) {
        if source.berlin_square() != Some(self.square) {
            return;
        }
        for (layer, hash) in &self.hashes {
            source
                .layers
                .insert(layer.key().to_owned(), crate::geodata::hash_text(*hash));
        }
    }

    /// The changed layers as a line for the Region source section and the
    /// log, or `None` where none changed. The world is drawn as Berlin has
    /// it now, and the next save records it.
    pub(crate) fn changed_sentence(&self) -> Option<String> {
        let words: Vec<&str> = self.changed.iter().map(|layer| layer.words()).collect();
        let (last, rest) = words.split_last()?;
        let list = if rest.is_empty() {
            (*last).to_owned()
        } else {
            format!("{} and {last}", rest.join(", "))
        };
        Some(format!("Berlin's {list} changed since the last save"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: GeoSquare = GeoSquare {
        min_e: 391_000,
        min_n: 5_819_500,
        size_m: 1_000,
    };

    #[test]
    fn every_layer_has_its_own_key_and_reads_back() {
        let keys: std::collections::BTreeSet<&str> = Layer::ALL.map(Layer::key).into();
        assert_eq!(keys.len(), Layer::ALL.len());
        for layer in Layer::ALL {
            assert_eq!(Layer::of_key(layer.key()), Some(layer));
        }
        assert_eq!(Layer::of_key("aerial"), None);
    }

    #[test]
    fn a_save_stamps_the_drawn_layers_and_keeps_the_rest() {
        let mut source = GeoSource::berlin(SQUARE);
        source
            .layers
            .insert("ring".into(), "1111111111111111".into());
        source
            .layers
            .insert("terrain".into(), "2222222222222222".into());
        let saved = saved_hashes(Some(&source));
        assert_eq!(
            saved,
            BTreeMap::from([
                (Layer::Terrain, 0x2222_2222_2222_2222),
                (Layer::Ring, 0x1111_1111_1111_1111)
            ])
        );
        let drawn = DrawnLayers::new(
            SQUARE,
            BTreeMap::from([(Layer::Terrain, 0xab), (Layer::Trees, 0xcd)]),
            &saved,
        );
        assert_eq!(drawn.changed, [Layer::Terrain], "trees had no saved hash");
        drawn.stamp(&mut source);
        assert_eq!(source.layers["terrain"], "00000000000000ab");
        assert_eq!(source.layers["trees"], "00000000000000cd");
        assert_eq!(source.layers["ring"], "1111111111111111", "not had, kept");
        // Another square's record is left alone.
        let mut moved = source.moved_to(GeoSquare {
            min_e: 391_010,
            ..SQUARE
        });
        drawn.stamp(&mut moved);
        assert!(moved.layers.is_empty());
    }

    #[test]
    fn the_changed_layers_are_said_as_a_list() {
        let saved = BTreeMap::from([
            (Layer::Buildings, 1),
            (Layer::Trees, 1),
            (Layer::Furniture, 1),
        ]);
        let drawn = |hashes: BTreeMap<Layer, u64>| DrawnLayers::new(SQUARE, hashes, &saved);
        assert_eq!(
            drawn(BTreeMap::from([(Layer::Buildings, 1)])).changed_sentence(),
            None
        );
        let one = drawn(BTreeMap::from([(Layer::Trees, 2)]))
            .changed_sentence()
            .unwrap();
        assert!(one.starts_with("Berlin's trees changed"), "{one}");
        let three = drawn(BTreeMap::from([
            (Layer::Buildings, 2),
            (Layer::Trees, 2),
            (Layer::Furniture, 2),
        ]))
        .changed_sentence()
        .unwrap();
        assert!(
            three.starts_with("Berlin's buildings, trees and street furniture changed"),
            "{three}"
        );
        // A hash that is not one, or a layer this build does not know, is
        // passed over.
        let mut source = GeoSource::berlin(SQUARE);
        source
            .layers
            .insert("aerial".into(), "0000000000000001".into());
        source.layers.insert("trees".into(), "zz".into());
        assert!(saved_hashes(Some(&source)).is_empty());
        assert!(saved_hashes(None).is_empty());
    }
}
