//! A geodata region's source on the room record (#1583, epic #1580; design
//! in `docs/geodata.md`): the square of real map data the region is built
//! from, and what of it was seeded into the record.
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
//! What stands on the square - its buildings, trees and street furniture -
//! is seeded into the record once its street level lands (#1647): instance
//! sets the owner edits as any other, saved with the world. The source
//! marks what it was seeded for ([`GeoSource::seeded`]), elided while
//! nothing is, so a source with nothing seeded keeps the bytes it had.

use serde::{Deserialize, Serialize};

use geodata::GeoSquare;
use geodata::berlin::Coverage;
use geodata::square::{SIZE_MAX_M, snap_size};

/// The dataset id of GDI Berlin.
pub const BERLIN: &str = "berlin";

/// The longest dataset id a record may carry.
pub const MAX_DATASET_CHARS: usize = 32;

/// What every generator a seeding of a square writes into the record is
/// named with first (#1647): what a later seeding takes out, what the World
/// Editor will not rename into, and what a copy detached from a seeded set
/// leaves off, so that it is the owner's own.
pub const SEEDED_PREFIX: &str = "kiez_";

/// Whether `name` is a generator a seeding wrote ([`SEEDED_PREFIX`]).
pub fn is_seeded_generator(name: &str) -> bool {
    name.starts_with(SEEDED_PREFIX)
}

/// Where a geodata region's ground comes from, and what of it was seeded
/// into the record.
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
    /// What its square was last seeded into the record for (#1647): the
    /// dataset, the square and the scene's seed, hashed
    /// ([`crate::terrain::derived::seed::mark`]). A record whose mark is its
    /// own is never seeded again, whatever its owner did to what was seeded;
    /// any other mark - a square moved, a re-roll - has its seeded content
    /// replaced. A quoted decimal on the wire, left out while unseeded.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::pds::types::option_u64_as_string"
    )]
    pub seeded: Option<u64>,
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

    /// This source moved to `square`, its mark kept: the mark names the
    /// square it was seeded for, so the record is seeded anew for this one.
    pub fn moved_to(&self, square: GeoSquare) -> Self {
        GeoSource {
            min_e: square.min_e,
            min_n: square.min_n,
            size_m: square.size_m,
            ..self.clone()
        }
    }

    /// This source with nothing seeded under it: what a re-roll keeps under
    /// the square lock, which replaces everything else the world holds, its
    /// seeded content with it - so the square is seeded anew, even for a
    /// scene the re-roll left as it was.
    pub fn unseeded(&self) -> Self {
        GeoSource {
            seeded: None,
            ..self.clone()
        }
    }

    /// Make the source safe to build from, or say it cannot be: `false`
    /// means the caller drops it and the region draws procedurally.
    ///
    /// The dataset id must be 1 to [`MAX_DATASET_CHARS`] of `a-z`, `0-9`,
    /// `-` and `_`. A Berlin square has its side made one a square may have
    /// ([`snap_size`]: a whole 10 m within 250 m - 2 km) and is moved, if it
    /// has to be, to the nearest position wholly inside Berlin
    /// ([`Coverage::nearest`]) - so a hand-edited or hostile
    /// record still lands on the map, as close to what it asked for as it
    /// can. A square wider than 2 km - one saved before #1637, when squares
    /// ran to 19 km round a walkable core of about a kilometre - shrinks
    /// round its middle, which holds that core. A square moved or shrunk
    /// keeps its mark, which then names another square: it is seeded anew.
    /// Another dataset's square is kept as it is: this build cannot judge
    /// it, and only a build that can will read it.
    pub fn sanitize(&mut self) -> bool {
        let id_ok = (1..=MAX_DATASET_CHARS).contains(&self.dataset.len())
            && self
                .dataset
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !id_ok {
            return false;
        }
        if self.dataset != BERLIN {
            return true;
        }
        let size = snap_size(self.size_m);
        let stored = self.square();
        let at = if self.size_m > SIZE_MAX_M {
            stored.centred(size)
        } else {
            stored
        };
        match Coverage::berlin().nearest(size, i64::from(at.min_e), i64::from(at.min_n)) {
            Some(square) => {
                self.min_e = square.min_e;
                self.min_n = square.min_n;
                self.size_m = square.size_m;
                true
            }
            None => false,
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

    /// #1637: a square saved when squares ran to 19 km shrinks to the
    /// largest side round its middle, where its full-detail ground lay,
    /// while a square of a side a square may have keeps its south-west
    /// corner, as it always did.
    #[test]
    fn a_square_saved_wider_than_the_largest_side_shrinks_round_its_middle() {
        // Seed 3's square before #1637: 17.32 km.
        let mut wide = berlin(380_329, 5_812_271, 17_320);
        let centre = wide.square().centre();
        assert!(wide.sanitize());
        assert_eq!(wide.size_m, SIZE_MAX_M);
        assert_eq!(wide.square().centre(), centre);
    }

    #[test]
    fn a_square_off_the_map_is_moved_onto_it_and_its_side_made_drawable() {
        use geodata::square::SIZE_MIN_M;
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
            seeded: Some(7),
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

    /// #1647: a moved square keeps its mark, which names the square it was
    /// seeded for; a re-roll's kept source has none, as the re-roll
    /// replaces what was seeded.
    #[test]
    fn a_moved_square_keeps_its_mark_and_a_rerolled_one_drops_it() {
        let mut source = berlin(391_000, 5_819_500, 1_000);
        source.seeded = Some(42);
        let square = GeoSquare {
            min_e: 392_000,
            min_n: 5_820_000,
            size_m: 600,
        };
        let moved = source.moved_to(square);
        assert_eq!(moved.square(), square);
        assert_eq!(moved.seeded, Some(42));
        let rerolled = source.unseeded();
        assert_eq!(rerolled.seeded, None);
        assert_eq!(rerolled.square(), source.square());
    }
}
