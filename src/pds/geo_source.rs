//! A geodata region's source on the room record (#1583, epic #1580; design
//! in `docs/geodata.md`): the square of real map data the region is built
//! from.
//!
//! A record with no source is the procedural world every record was before
//! #1580, and stays byte-identical on the wire: the field is elided when
//! absent. A record with one names a dataset and a square of it in whole
//! metres - integers only, as the wire carries no floats.
//!
//! The source is also how an owner keeps a square: once DID-seeded regions
//! can draw Berlin (#1589), a seeded square is drawn afresh from the seed on
//! every visit, and saving it stores it here.

use serde::{Deserialize, Serialize};

use geodata::GeoSquare;
use geodata::berlin::Coverage;
use geodata::square::snap_size;

/// The dataset id of GDI Berlin.
pub const BERLIN: &str = "berlin";

/// The longest dataset id a record may carry.
pub const MAX_DATASET_CHARS: usize = 32;

/// Where a geodata region's ground comes from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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
}

impl GeoSource {
    /// A Berlin source over `square`.
    pub fn berlin(square: GeoSquare) -> Self {
        GeoSource {
            dataset: BERLIN.to_owned(),
            min_e: square.min_e,
            min_n: square.min_n,
            size_m: square.size_m,
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
        match Coverage::berlin().nearest(size, i64::from(self.min_e), i64::from(self.min_n)) {
            Some(square) => {
                *self = GeoSource::berlin(square);
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
}
