//! Where a geodata square may lie: wholly inside Berlin, so every layer has
//! data under all of it.
//!
//! The map is a grid of 250 m cells (`coverage_table.rs`, generated); a
//! square may cover only cells that lie wholly inside the state - to the
//! generator's 25 m sampling, so a cell may reach about 25 m past the
//! border, a sliver the decoders fill from its neighbours. Placing a
//! square of a given size is then exact: [`Coverage::place`] picks uniformly
//! among *every* whole-metre position where it fits, so no part of Berlin is
//! likelier than another and no draw is ever rejected.

use std::sync::OnceLock;

use super::Borough;
use super::coverage_table::{CELL_M, COLS, ORIGIN_E, ORIGIN_N, ROWS};
use crate::square::{GeoSquare, SIZE_MIN_M, SIZE_STEP_M, size_from_draw};

/// Berlin's coverage grid.
#[derive(Debug)]
pub struct Coverage {
    /// Per cell, row-major from the south-west: 0 where the cell is not
    /// wholly inside Berlin, else its borough's code.
    cells: Vec<u8>,
    /// Summed-area table of the outside cells, `(rows + 1) x (COLS + 1)`:
    /// how many outside cells any block of cells holds, in four reads.
    outside: Vec<u32>,
    rows: usize,
}

impl Coverage {
    /// The grid, decoded once.
    pub fn berlin() -> &'static Coverage {
        static COVERAGE: OnceLock<Coverage> = OnceLock::new();
        COVERAGE.get_or_init(|| Coverage::from_rows(&ROWS))
    }

    /// Decode the run-length rows of `coverage_table.rs`.
    ///
    /// # Panics
    ///
    /// On a malformed row. The table is generated and checked in, and a test
    /// decodes it, so this cannot reach a build.
    fn from_rows(rows: &[&str]) -> Coverage {
        let mut cells = Vec::with_capacity(rows.len() * COLS);
        for (r, row) in rows.iter().enumerate() {
            let start = cells.len();
            let mut chars = row.chars().peekable();
            while let Some(symbol) = chars.next() {
                let value = match symbol {
                    '.' => 0,
                    'a'..='l' => symbol as u8 - b'a' + 1,
                    _ => panic!("coverage row {r}: symbol {symbol:?}"),
                };
                let mut count = 0usize;
                while let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
                    count = count * 10 + digit as usize;
                    chars.next();
                }
                cells.extend(std::iter::repeat_n(value, count));
            }
            assert_eq!(cells.len() - start, COLS, "coverage row {r} width");
        }
        let width = COLS + 1;
        let mut outside = vec![0u32; (rows.len() + 1) * width];
        for r in 0..rows.len() {
            for c in 0..COLS {
                let here = u32::from(cells[r * COLS + c] == 0);
                outside[(r + 1) * width + c + 1] =
                    here + outside[r * width + c + 1] + outside[(r + 1) * width + c]
                        - outside[r * width + c];
            }
        }
        Coverage {
            cells,
            outside,
            rows: rows.len(),
        }
    }

    /// The borough the cell under `(e, n)` lies in, or `None` where that cell
    /// is not wholly inside Berlin.
    pub fn borough_at(&self, e: f64, n: f64) -> Option<Borough> {
        let col = ((e - f64::from(ORIGIN_E)) / f64::from(CELL_M)).floor();
        let row = ((n - f64::from(ORIGIN_N)) / f64::from(CELL_M)).floor();
        if !(0.0..COLS as f64).contains(&col) || !(0.0..self.rows as f64).contains(&row) {
            return None;
        }
        Borough::from_code(self.cells[row as usize * COLS + col as usize])
    }

    /// Whether `square` lies wholly on inside cells.
    pub fn contains(&self, square: &GeoSquare) -> bool {
        if square.size_m == 0 {
            return false;
        }
        // In i64 throughout: any square converts, and with a side of at least
        // a metre the last cell is never before the first.
        let cell =
            |metres: i64, origin: i32| (metres - i64::from(origin)).div_euclid(i64::from(CELL_M));
        let (c0, c1) = (
            cell(i64::from(square.min_e), ORIGIN_E),
            cell(square.max_e() - 1, ORIGIN_E),
        );
        let (r0, r1) = (
            cell(i64::from(square.min_n), ORIGIN_N),
            cell(square.max_n() - 1, ORIGIN_N),
        );
        if c0 < 0 || r0 < 0 || c1 >= COLS as i64 || r1 >= self.rows as i64 {
            return false;
        }
        let (c0, r0) = (c0 as usize, r0 as usize);
        self.outside_in(c0, r0, c1 as usize - c0 + 1, r1 as usize - r0 + 1) == 0
    }

    /// How many whole-metre positions a square of `size_m` can take wholly
    /// inside Berlin. Zero for a size that fits nowhere.
    pub fn placements(&self, size_m: u32) -> u64 {
        let mut total = 0;
        self.walk(size_m, |_, _, _, weight| {
            total += weight;
            false
        });
        total
    }

    /// The square of side `size_m` that a uniform 64-bit `draw` picks among
    /// all [`Self::placements`] of it, or `None` if it fits nowhere.
    pub fn place(&self, size_m: u32, draw: u64) -> Option<GeoSquare> {
        let total = self.placements(size_m);
        if total == 0 {
            return None;
        }
        // Scale the draw into [0, total) by the high half of a widening
        // product: no modulo bias worth the name at these totals.
        let target = ((u128::from(draw) * u128::from(total)) >> 64) as u64;
        let mut before = 0;
        let mut found = None;
        self.walk(size_m, |col, row, class, weight| {
            if target < before + weight {
                found = Some((col, row, class, target - before));
                return true;
            }
            before += weight;
            false
        });
        let (col, row, class, index) = found?;
        let (base_e, count_e) = class.0;
        let (base_n, _) = class.1;
        let offset_e = base_e + (index % count_e) as i32;
        let offset_n = base_n + (index / count_e) as i32;
        Some(GeoSquare {
            min_e: ORIGIN_E + col as i32 * CELL_M + offset_e,
            min_n: ORIGIN_N + row as i32 * CELL_M + offset_n,
            size_m,
        })
    }

    /// A region's square from two uniform draws of its seeded stream: the
    /// size from the first ([`size_from_draw`]), the position from the
    /// second ([`Self::place`]).
    ///
    /// Every drawn size fits somewhere (a test holds the largest one), so
    /// this is `None` only for a broken table; it steps the size down rather
    /// than give up if one ever does not.
    pub fn square_from_draws(&self, size_draw: u64, place_draw: u64) -> Option<GeoSquare> {
        let mut size = size_from_draw(size_draw);
        loop {
            if let Some(square) = self.place(size, place_draw) {
                return Some(square);
            }
            if size <= SIZE_MIN_M {
                return None;
            }
            size = size.saturating_sub(SIZE_STEP_M).max(SIZE_MIN_M);
        }
    }

    /// Outside cells in the block of `cols x rows` cells from `(col, row)`.
    fn outside_in(&self, col: usize, row: usize, cols: usize, rows: usize) -> u32 {
        let width = COLS + 1;
        let at = |r: usize, c: usize| self.outside[r * width + c];
        at(row + rows, col + cols) + at(row, col) - at(row, col + cols) - at(row + rows, col)
    }

    /// Visit every footprint a square of `size_m` can have - the corner cell
    /// it starts in, and how many cells it spans each way - with the number
    /// of whole-metre positions that give that footprint wholly inside, in
    /// one fixed order, until `visit` returns `true`.
    ///
    /// A square starting `o` metres into its corner cell spans `a` cells if
    /// `o <= a * CELL_M - size_m` (with `a` the fewest cells it can span) and
    /// `a + 1` otherwise, so each axis has two classes of offset: `(first
    /// offset, how many)`.
    fn walk(&self, size_m: u32, mut visit: impl FnMut(usize, usize, Span, u64) -> bool) {
        let cell = CELL_M as u32;
        // A square wider than the grid fits nowhere, and stopping here keeps
        // `fewest * cell` far below u32::MAX.
        let extent = COLS.min(self.rows) as u32 * cell;
        if size_m == 0 || size_m > extent {
            return;
        }
        let fewest = size_m.div_ceil(cell);
        let tight = fewest * cell - size_m + 1;
        let classes = [(fewest, 0, tight), (fewest + 1, tight, cell - tight)];
        for &(span_n, base_n, count_n) in &classes {
            for &(span_e, base_e, count_e) in &classes {
                let (span_e, span_n) = (span_e as usize, span_n as usize);
                if count_e == 0 || count_n == 0 || span_e > COLS || span_n > self.rows {
                    continue;
                }
                let weight = u64::from(count_e) * u64::from(count_n);
                let span: Span = (
                    (base_e as i32, u64::from(count_e)),
                    (base_n as i32, u64::from(count_n)),
                );
                for row in 0..=self.rows - span_n {
                    for col in 0..=COLS - span_e {
                        if self.outside_in(col, row, span_e, span_n) == 0
                            && visit(col, row, span, weight)
                        {
                            return;
                        }
                    }
                }
            }
        }
    }
}

/// One offset class per axis, east then north: `(first offset, how many)`.
type Span = ((i32, u64), (i32, u64));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::square::SIZE_MAX_M;

    #[test]
    fn the_table_decodes_to_berlin() {
        let coverage = Coverage::berlin();
        assert_eq!(coverage.rows, ROWS.len());
        let inside = coverage.cells.iter().filter(|&&c| c != 0).count();
        // 13,787 cells of 250 m: 862 km2 of Berlin's 891 wholly inside.
        assert_eq!(inside, 13_787);
        for b in Borough::ALL {
            assert!(coverage.cells.contains(&b.code()), "{b:?} has no cell");
        }
    }

    #[test]
    fn landmarks_lie_in_their_boroughs() {
        let coverage = Coverage::berlin();
        let cases = [
            (391_513.0, 5_819_975.0, Borough::Mitte), // Berliner Dom
            (394_503.0, 5_817_965.0, Borough::FriedrichshainKreuzberg), // Oberbaumbruecke
            (391_696.0, 5_825_527.0, Borough::Pankow), // Rathaus Pankow
            (380_605.0, 5_817_828.0, Borough::CharlottenburgWilmersdorf), // Teufelsberg
            (378_805.0, 5_822_747.0, Borough::Spandau), // Citadel
            (385_937.0, 5_813_150.0, Borough::SteglitzZehlendorf), // Rathaus Steglitz
            (387_592.0, 5_816_283.0, Borough::TempelhofSchoeneberg), // Rathaus Schoeneberg
            (393_735.0, 5_815_712.0, Borough::Neukoelln), // Rathaus Neukoelln
            (407_440.0, 5_808_291.0, Borough::TreptowKoepenick), // Grosser Mueggelberg
            (403_421.0, 5_821_832.0, Borough::MarzahnHellersdorf), // Gaerten der Welt
            (396_856.0, 5_819_406.0, Borough::Lichtenberg), // Rathaus Lichtenberg
            (383_727.0, 5_827_991.0, Borough::Reinickendorf), // Alt-Tegel
        ];
        for (e, n, borough) in cases {
            assert_eq!(coverage.borough_at(e, n), Some(borough), "({e}, {n})");
        }
        assert_eq!(coverage.borough_at(368_293.0, 5_806_246.0), None, "Potsdam");
        assert_eq!(
            coverage.borough_at(300_000.0, 5_820_000.0),
            None,
            "off the grid"
        );
    }

    #[test]
    fn contains_needs_every_cell_inside() {
        let coverage = Coverage::berlin();
        let dom = GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        assert!(coverage.contains(&dom));
        let potsdam = GeoSquare {
            min_e: 368_000,
            min_n: 5_806_000,
            size_m: 500,
        };
        assert!(!coverage.contains(&potsdam));
        let off_grid = GeoSquare {
            min_e: ORIGIN_E - 10,
            min_n: 5_819_500,
            size_m: 1_000,
        };
        assert!(!coverage.contains(&off_grid));
        assert!(!coverage.contains(&GeoSquare { size_m: 0, ..dom }));
    }

    #[test]
    fn squares_off_the_map_are_outside_without_overflow() {
        // A square comes from a record one day (#1583): no value may panic.
        let coverage = Coverage::berlin();
        for (min_e, min_n, size_m) in [
            (391_000, 5_819_500, u32::MAX),
            (391_000, 5_819_500, 1 << 31),
            (391_000, 5_819_500, 4_294_967_000),
            (i32::MAX - 10, 5_819_500, 1_000),
            (i32::MAX, i32::MAX, u32::MAX),
            (i32::MIN, i32::MIN, u32::MAX),
        ] {
            let square = GeoSquare {
                min_e,
                min_n,
                size_m,
            };
            assert!(!coverage.contains(&square), "{square:?}");
        }
    }

    #[test]
    fn the_largest_size_fits_and_the_next_does_not() {
        let coverage = Coverage::berlin();
        assert!(coverage.placements(SIZE_MAX_M) > 0);
        assert_eq!(coverage.placements(SIZE_MAX_M + SIZE_STEP_M), 0);
        assert_eq!(coverage.place(SIZE_MAX_M + SIZE_STEP_M, 0), None);
        // Sizes past the grid, up to the top of u32, fit nowhere and say so.
        for size in [40_250, 4_294_967_100, 4_294_967_251, u32::MAX] {
            assert_eq!(coverage.placements(size), 0, "{size}");
            assert_eq!(coverage.place(size, 0), None, "{size}");
        }
    }

    #[test]
    fn placements_count_every_whole_metre_position() {
        // A square exactly one cell wide fits a single-cell footprint only
        // at offset 0, and spans two cells at the other 249 offsets.
        let coverage = Coverage::berlin();
        let inside = coverage.cells.iter().filter(|&&c| c != 0).count() as u64;
        let mut pairs_e = 0;
        let mut pairs_n = 0;
        let mut quads = 0;
        for row in 0..coverage.rows {
            for col in 0..COLS {
                let fits = |cols: usize, rows: usize| {
                    col + cols <= COLS
                        && row + rows <= coverage.rows
                        && coverage.outside_in(col, row, cols, rows) == 0
                };
                pairs_e += u64::from(fits(2, 1));
                pairs_n += u64::from(fits(1, 2));
                quads += u64::from(fits(2, 2));
            }
        }
        let expected = inside + 249 * pairs_e + 249 * pairs_n + 249 * 249 * quads;
        assert_eq!(coverage.placements(250), expected);
    }

    #[test]
    fn placed_squares_lie_inside_at_their_size() {
        let coverage = Coverage::berlin();
        for size in [250, 260, 999, 4_000, 12_340, SIZE_MAX_M] {
            for k in 0..64u64 {
                let draw = k.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let square = coverage.place(size, draw).unwrap();
                assert_eq!(square.size_m, size);
                assert!(coverage.contains(&square), "{square:?}");
            }
        }
        // The extremes of the draw land on the first and last positions.
        let first = coverage.place(1_000, 0).unwrap();
        let last = coverage.place(1_000, u64::MAX).unwrap();
        assert!(coverage.contains(&first) && coverage.contains(&last));
        assert_ne!(first, last);
    }

    #[test]
    fn placement_spreads_like_the_area() {
        // Evenly spaced draws put small squares in each borough about as
        // often as the borough's share of the inside cells.
        let coverage = Coverage::berlin();
        let inside = coverage.cells.iter().filter(|&&c| c != 0).count() as f64;
        let draws = 4_000u64;
        let mut hits = [0u32; 13];
        for k in 0..draws {
            let square = coverage.place(250, k * (u64::MAX / draws)).unwrap();
            let (e, n) = square.centre();
            hits[coverage
                .borough_at(e, n)
                .map_or(0, |b| usize::from(b.code()))] += 1;
        }
        for b in Borough::ALL {
            let share = coverage.cells.iter().filter(|&&c| c == b.code()).count() as f64 / inside;
            let got = f64::from(hits[usize::from(b.code())]) / draws as f64;
            assert!((got - share).abs() < 0.02, "{b:?}: {got:.3} vs {share:.3}");
        }
    }

    #[test]
    fn squares_from_draws_are_pinned() {
        // A seeded region keeps its square across releases: these are the
        // squares today's draws give, and `tools/place_check.py` (an
        // independent implementation of the draw and the walk, reading the
        // table) gives the same. Changing one moves every region drawn from
        // it - that needs a migration, not a re-bless.
        let coverage = Coverage::berlin();
        let squares: Vec<GeoSquare> = [
            (0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210),
            (0x8000_0000_0000_0000, 0x8000_0000_0000_0000),
            (0xDEAD_BEEF_F00D_CAFE, 0x0BAD_C0DE_1234_5678),
        ]
        .into_iter()
        .map(|(size, place)| coverage.square_from_draws(size, place).unwrap())
        .collect();
        let square = |min_e, min_n, size_m| GeoSquare {
            min_e,
            min_n,
            size_m,
        };
        assert_eq!(
            squares,
            vec![
                square(396_482, 5_833_873, 250),
                square(402_454, 5_806_171, 2_180),
                square(378_010, 5_809_802, 10_810),
            ]
        );
    }
}
