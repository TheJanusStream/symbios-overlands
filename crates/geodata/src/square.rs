//! A square of the map, and the seeded draw of its size.
//!
//! A geodata region is a square of real Berlin at real scale: the square's
//! side is the region's extent. Where it lies is the coverage's business
//! ([`crate::berlin::Coverage::place`]); how big it is, this module's.

use serde::{Deserialize, Serialize};

/// The smallest square a region is drawn from, metres.
pub const SIZE_MIN_M: u32 = 250;

/// The largest, metres: the biggest square that fits wholly inside Berlin.
/// The owner asked for 20 km; the state's shape allows 19
/// (`berlin::coverage` tests hold that this one fits and the next does not).
pub const SIZE_MAX_M: u32 = 19_000;

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
    /// if it is no larger. The walkable core of a large region is this.
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

/// The size a uniform 64-bit `draw` picks: log-uniform over
/// [`SIZE_MIN_M`]..=[`SIZE_MAX_M`], so each doubling of size is as likely as
/// the next, rounded to a whole [`SIZE_STEP_M`].
///
/// The logarithm and exponential are `libm`'s, so the same draw gives the
/// same size on every platform.
pub fn size_from_draw(draw: u64) -> u32 {
    // The top 53 bits as a [0, 1) fraction: exact in an f64.
    let unit = (draw >> 11) as f64 / (1u64 << 53) as f64;
    let span = libm::log(f64::from(SIZE_MAX_M) / f64::from(SIZE_MIN_M));
    let size = f64::from(SIZE_MIN_M) * libm::exp(unit * span);
    let steps = libm::round(size / f64::from(SIZE_STEP_M)) as u32;
    (steps * SIZE_STEP_M).clamp(SIZE_MIN_M, SIZE_MAX_M)
}

/// `size_m` as a drawable side: the nearest whole [`SIZE_STEP_M`] (halves
/// up), within [`SIZE_MIN_M`]..=[`SIZE_MAX_M`] - what a drawn size always is,
/// and what a typed or stored one is made into (#1583).
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
        for draw in [0, 1 << 40, u64::MAX / 3, u64::MAX] {
            let size = size_from_draw(draw);
            assert_eq!(snap_size(size), size, "a drawn size is already snapped");
        }
    }

    #[test]
    fn size_draw_spans_the_range_on_the_step() {
        assert_eq!(size_from_draw(0), SIZE_MIN_M);
        assert_eq!(size_from_draw(u64::MAX), SIZE_MAX_M);
        let mut last = 0;
        for i in 0..=1000u64 {
            let size = size_from_draw(i * (u64::MAX / 1000));
            assert!((SIZE_MIN_M..=SIZE_MAX_M).contains(&size));
            assert_eq!(size % SIZE_STEP_M, 0);
            assert!(size >= last, "the draw is monotone");
            last = size;
        }
    }

    #[test]
    fn size_draw_is_log_uniform() {
        // Equal slices of the draw land in equal slices of log(size): the
        // median is the geometric mean of the bounds, not their midpoint.
        let median = size_from_draw(u64::MAX / 2);
        let geometric = (f64::from(SIZE_MIN_M) * f64::from(SIZE_MAX_M)).sqrt();
        assert!((f64::from(median) - geometric).abs() <= f64::from(SIZE_STEP_M));
        let quarter = size_from_draw(u64::MAX / 4);
        let expected = f64::from(SIZE_MIN_M) * (geometric / f64::from(SIZE_MIN_M)).sqrt();
        assert!((f64::from(quarter) - expected).abs() <= f64::from(SIZE_STEP_M));
    }

    #[test]
    fn size_draw_is_pinned() {
        // A seeded region must keep its square across releases: these are
        // the sizes today's draws give, and changing one moves every region
        // drawn from it. `tools/place_check.py` reproduces them.
        assert_eq!(size_from_draw(0x0123_4567_89AB_CDEF), 250);
        assert_eq!(size_from_draw(0x8000_0000_0000_0000), 2180);
        assert_eq!(size_from_draw(0xDEAD_BEEF_F00D_CAFE), 10_810);
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
