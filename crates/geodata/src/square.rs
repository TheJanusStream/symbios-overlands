//! A square of the map, and the sides a region's square may have.
//!
//! A geodata region is a square of real Berlin at real scale, walked at full
//! detail to its edge (a Kiez, #1637): the square's side is the region's
//! extent. Where it lies is the coverage's business
//! ([`crate::berlin::Coverage::place`]); how big it may be, this module's.

use serde::{Deserialize, Serialize};

/// The smallest side a region's square may have, metres.
pub const SIZE_MIN_M: u32 = 250;

/// The largest, metres: a region is walked at full detail to its edge, so
/// its side is held to what that costs (#1637, the owner's choice). The
/// World Editor may set any side up to it; Berlin's shape would allow 19 km.
pub const SIZE_MAX_M: u32 = 2_000;

/// The side of a seeded region's square, metres: a Kiez's kilometre (#1637,
/// the owner's choice).
pub const SEEDED_SIZE_M: u32 = 1_000;

/// Every drawn size is a whole multiple of this many metres, so a square's
/// centre is a whole metre and every request box is whole.
pub const SIZE_STEP_M: u32 = 10;

/// A square of the map in whole metres of the dataset's CRS (EPSG:25833 for
/// Berlin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GeoSquare {
    /// Easting of the west edge.
    pub min_e: i32,
    /// Northing of the south edge.
    pub min_n: i32,
    /// Side length.
    pub size_m: u32,
}

impl GeoSquare {
    /// Easting of the east edge. Wider than the fields, so no square - not
    /// even one that was never on the map - overflows it.
    pub fn max_e(&self) -> i64 {
        i64::from(self.min_e) + i64::from(self.size_m)
    }

    /// Northing of the north edge, wide like [`Self::max_e`].
    pub fn max_n(&self) -> i64 {
        i64::from(self.min_n) + i64::from(self.size_m)
    }

    /// The centre `(e, n)`, metres.
    pub fn centre(&self) -> (f64, f64) {
        let half = f64::from(self.size_m) / 2.0;
        (f64::from(self.min_e) + half, f64::from(self.min_n) + half)
    }

    /// The square of side `size_m` sharing this one's centre - or this one,
    /// if it is no larger. A square saved wider than [`SIZE_MAX_M`] shrinks
    /// to this (#1637).
    ///
    /// When the two sides differ by an odd number of metres, the smaller
    /// square sits half a metre south-west of true centre: corners stay
    /// whole.
    pub fn centred(&self, size_m: u32) -> GeoSquare {
        if size_m >= self.size_m {
            return *self;
        }
        // Half a u32 always fits an i32; the sum saturates only for a square
        // that was never on the map.
        let inset = ((self.size_m - size_m) / 2) as i32;
        GeoSquare {
            min_e: self.min_e.saturating_add(inset),
            min_n: self.min_n.saturating_add(inset),
            size_m,
        }
    }

    /// Whether the point `(e, n)` lies in the square: west and south edges
    /// in, east and north edges out.
    pub fn contains(&self, e: f64, n: f64) -> bool {
        e >= f64::from(self.min_e)
            && e < self.max_e() as f64
            && n >= f64::from(self.min_n)
            && n < self.max_n() as f64
    }
}

/// `size_m` as a side a square may have: the nearest whole [`SIZE_STEP_M`]
/// (halves up), within [`SIZE_MIN_M`]..=[`SIZE_MAX_M`] - what a typed or
/// stored side is made into (#1583).
pub fn snap_size(size_m: u32) -> u32 {
    let steps = size_m.saturating_add(SIZE_STEP_M / 2) / SIZE_STEP_M;
    (steps * SIZE_STEP_M).clamp(SIZE_MIN_M, SIZE_MAX_M)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapped_sizes_are_drawable() {
        assert_eq!(snap_size(1_003), 1_000);
        assert_eq!(snap_size(1_005), 1_010);
        assert_eq!(snap_size(0), SIZE_MIN_M);
        assert_eq!(snap_size(u32::MAX), SIZE_MAX_M);
        assert_eq!(
            snap_size(SEEDED_SIZE_M),
            SEEDED_SIZE_M,
            "a seeded side is one"
        );
    }

    #[test]
    fn centred_keeps_whole_corners() {
        let square = GeoSquare {
            min_e: 390_000,
            min_n: 5_818_000,
            size_m: 4_000,
        };
        assert_eq!(square.centre(), (392_000.0, 5_820_000.0));
        let core = square.centred(1_000);
        assert_eq!(
            (core.min_e, core.min_n, core.size_m),
            (391_500, 5_819_500, 1_000)
        );
        assert_eq!(core.centre(), square.centre());
        assert_eq!(square.centred(5_000), square);
        let odd = square.centred(999);
        assert_eq!((odd.min_e, odd.min_n), (391_500, 5_819_500));
    }

    #[test]
    fn contains_is_half_open() {
        let square = GeoSquare {
            min_e: 0,
            min_n: 0,
            size_m: 10,
        };
        assert!(square.contains(0.0, 0.0));
        assert!(square.contains(9.99, 9.99));
        assert!(!square.contains(10.0, 5.0));
        assert!(!square.contains(5.0, 10.0));
        assert!(!square.contains(-0.01, 5.0));
    }

    #[test]
    fn squares_off_the_map_do_not_overflow() {
        // The fields are public and deserialisable: no value may panic.
        let far = GeoSquare {
            min_e: i32::MAX - 10,
            min_n: i32::MAX,
            size_m: u32::MAX,
        };
        assert_eq!(far.max_e(), i64::from(i32::MAX) - 10 + i64::from(u32::MAX));
        assert_eq!(far.max_n(), i64::from(i32::MAX) + i64::from(u32::MAX));
        assert!(far.contains(f64::from(i32::MAX), f64::from(i32::MAX)));
        let core = far.centred(0);
        assert_eq!(
            (core.min_e, core.min_n, core.size_m),
            (i32::MAX, i32::MAX, 0)
        );
        let low = GeoSquare {
            min_e: i32::MIN,
            min_n: i32::MIN,
            size_m: u32::MAX,
        };
        assert_eq!(low.centred(1).min_e, i32::MIN + (u32::MAX / 2) as i32);
    }
}
