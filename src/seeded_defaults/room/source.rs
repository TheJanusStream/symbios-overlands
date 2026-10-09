//! A seeded room's source (#1589, epic #1580; design in `docs/geodata.md`):
//! whether its ground is drawn procedurally or from a square of real
//! Berlin, and which square.
//!
//! The draw is a stream of its own, salted apart from every other, so
//! nothing a seed decided before it moves: the palette, the terrain, the
//! scatters and the settlement draw exactly as they did. Its first draw
//! picks the kind - [`BERLIN_SHARE`] of seeds draw Berlin - and its next
//! two the square ([`Coverage::square_from_draws`]: a log-uniform side, then
//! any position where that side fits wholly inside Berlin). The square is
//! drawn whatever the kind, so a seed's square stays where it is if the
//! share ever changes.
//!
//! A seeded room's square is drawn afresh on every visit; saving the room
//! stores it in the record's `geo_source`, which holds it exactly from then
//! on (#1583).

use geodata::GeoSquare;
use geodata::berlin::{Borough, Coverage};
use geodata::square::size_from_draw;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

/// The share of seeds that draw Berlin, as `(of these, out of)`: one in
/// four, the owner's choice (2026-10-09). Changing it moves seeded rooms
/// between the two kinds - never their squares, nor anything else they
/// draw.
pub const BERLIN_SHARE: (u64, u64) = (1, 4);

/// The salt of the source's stream.
const SOURCE_SALT: u64 = 0x5EED_6E05_0BCE_1589;

/// Where a room's ground comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// Drawn from the seed: the region's own terrain.
    Procedural,
    /// A square of real Berlin, at real scale.
    Berlin,
}

impl SourceKind {
    pub const ALL: [Self; 2] = [Self::Procedural, Self::Berlin];

    pub fn label(self) -> &'static str {
        match self {
            Self::Procedural => "Procedural",
            Self::Berlin => "Berlin",
        }
    }
}

/// How big a Berlin square is, coarsely: about a third of draws each, the
/// side being log-uniform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Up to 1 km: about today's walkable ground, so all of it walked.
    Small,
    /// Up to 5 km: a ring of city round the walkable ground.
    Medium,
    /// Up to 19 km: the city to the horizon.
    Large,
}

impl SizeClass {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    /// The largest side of each class but the last (m).
    const SMALL_MAX_M: u32 = 1_000;
    const MEDIUM_MAX_M: u32 = 5_000;

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small, to 1 km",
            Self::Medium => "Medium, to 5 km",
            Self::Large => "Large, to 19 km",
        }
    }

    /// The class of a square of side `size_m`.
    pub fn of(size_m: u32) -> Self {
        match size_m {
            s if s <= Self::SMALL_MAX_M => Self::Small,
            s if s <= Self::MEDIUM_MAX_M => Self::Medium,
            _ => Self::Large,
        }
    }

    /// The smallest side the class holds (m): where a square of it can
    /// stand, so can its smallest.
    pub fn smallest(self) -> u32 {
        match self {
            Self::Small => geodata::square::SIZE_MIN_M,
            Self::Medium => Self::SMALL_MAX_M + geodata::square::SIZE_STEP_M,
            Self::Large => Self::MEDIUM_MAX_M + geodata::square::SIZE_STEP_M,
        }
    }
}

/// A seed's source draw (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionSource {
    pub kind: SourceKind,
    size_draw: u64,
    place_draw: u64,
}

impl RegionSource {
    /// The source `seed` draws.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ SOURCE_SALT);
        let kind_draw = rng.next_u64();
        let (size_draw, place_draw) = (rng.next_u64(), rng.next_u64());
        // The draw scaled into [0, out of) by a widening product.
        let (share, out_of) = BERLIN_SHARE;
        let berlin = ((u128::from(kind_draw) * u128::from(out_of)) >> 64) < u128::from(share);
        RegionSource {
            kind: if berlin {
                SourceKind::Berlin
            } else {
                SourceKind::Procedural
            },
            size_draw,
            place_draw,
        }
    }

    /// The side of the seed's square (m): cheap, with no walk of the map.
    /// Every drawn side fits somewhere in Berlin (a test of the coverage
    /// holds the largest), so this is the square's own.
    pub fn size_m(&self) -> u32 {
        size_from_draw(self.size_draw)
    }

    /// The class of [`Self::size_m`].
    pub fn size_class(&self) -> SizeClass {
        SizeClass::of(self.size_m())
    }

    /// The seed's square, whatever its kind: a walk of the map, a fraction
    /// of a millisecond.
    pub fn square(&self) -> GeoSquare {
        Coverage::berlin()
            .square_from_draws(self.size_draw, self.place_draw)
            .expect("every drawn size fits somewhere in Berlin")
    }

    /// The square, where the seed draws Berlin.
    pub fn berlin_square(&self) -> Option<GeoSquare> {
        (self.kind == SourceKind::Berlin).then(|| self.square())
    }

    /// The borough the square's middle lies in.
    pub fn borough(&self) -> Option<Borough> {
        let (e, n) = self.square().centre();
        Coverage::berlin().borough_at(e, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seeded room keeps its source across releases: these are what
    /// today's seeds draw. Changing one moves every room drawn from it - a
    /// migration, never a re-bless.
    #[test]
    fn seeded_sources_are_pinned() {
        let drawn: Vec<(SourceKind, GeoSquare)> = [0u64, 1, 2, 3, 253, 0xDEAD_BEEF]
            .into_iter()
            .map(|seed| {
                let source = RegionSource::for_seed(seed);
                (source.kind, source.square())
            })
            .collect();
        let square = |min_e, min_n, size_m| GeoSquare {
            min_e,
            min_n,
            size_m,
        };
        use SourceKind::{Berlin, Procedural};
        let pinned = vec![
            (Procedural, square(390_902, 5_825_009, 600)),
            (Procedural, square(391_113, 5_826_042, 1_680)),
            (Procedural, square(377_891, 5_818_109, 3_480)),
            (Berlin, square(380_329, 5_812_271, 17_320)),
            (Procedural, square(379_843, 5_816_933, 7_730)),
            (Procedural, square(376_781, 5_811_745, 15_230)),
        ];
        assert_eq!(drawn, pinned);
    }

    #[test]
    fn a_quarter_of_seeds_draw_berlin_and_the_classes_a_third_each() {
        let sources: Vec<RegionSource> = (0..4_000u64).map(RegionSource::for_seed).collect();
        let berlin = sources
            .iter()
            .filter(|s| s.kind == SourceKind::Berlin)
            .count();
        assert!((900..1_100).contains(&berlin), "{berlin} of 4,000");
        for class in SizeClass::ALL {
            let n = sources.iter().filter(|s| s.size_class() == class).count();
            assert!((1_150..1_550).contains(&n), "{class:?}: {n} of 4,000");
        }
    }

    #[test]
    fn a_square_is_drawn_whatever_the_kind_and_lies_in_berlin() {
        let coverage = Coverage::berlin();
        for seed in 0..64u64 {
            let source = RegionSource::for_seed(seed);
            let square = source.square();
            assert!(coverage.contains(&square), "seed {seed}: {square:?}");
            assert_eq!(square.size_m, source.size_m());
            assert_eq!(
                source.berlin_square().is_some(),
                source.kind == SourceKind::Berlin
            );
            assert!(source.borough().is_some());
        }
    }

    #[test]
    fn a_class_holds_the_sides_between_its_bounds() {
        assert_eq!(SizeClass::of(250), SizeClass::Small);
        assert_eq!(SizeClass::of(1_000), SizeClass::Small);
        assert_eq!(SizeClass::of(1_010), SizeClass::Medium);
        assert_eq!(SizeClass::of(5_000), SizeClass::Medium);
        assert_eq!(SizeClass::of(5_010), SizeClass::Large);
        for class in SizeClass::ALL {
            assert_eq!(SizeClass::of(class.smallest()), class);
        }
    }
}
