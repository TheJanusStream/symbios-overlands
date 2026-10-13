//! A seeded room's source (#1589, epic #1580; design in `docs/geodata.md`):
//! whether its ground is drawn procedurally or from a square of real
//! Berlin, and which square.
//!
//! The draw is a stream of its own, salted apart from every other, so
//! nothing a seed decided before it moves: the palette, the terrain, the
//! scatters and the settlement draw exactly as they did. Its first draw
//! picks the kind - [`BERLIN_SHARE`] of seeds draw Berlin - and its third
//! the square ([`Coverage::seeded_square`]: a kilometre, a Kiez (#1637), at
//! any whole-metre position where it fits wholly inside Berlin, each as
//! likely as any other, whatever is built there). The second draw picked the
//! side until #1637 fixed it; it is still drawn, so the position keeps its
//! own draw. The square is drawn whatever the kind, so a seed's square stays
//! where it is if the share ever changes.
//!
//! A seeded room's square is drawn afresh on every visit; saving the room
//! stores it in the record's `geo_source`, which holds it exactly from then
//! on (#1583).

use geodata::GeoSquare;
use geodata::berlin::{Borough, Coverage};
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

/// A seed's source draw (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionSource {
    pub kind: SourceKind,
    place_draw: u64,
}

impl RegionSource {
    /// The source `seed` draws.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ SOURCE_SALT);
        let kind_draw = rng.next_u64();
        // The side's draw until #1637: drawn and left, so the position keeps
        // its place in the stream.
        let _retired_size_draw = rng.next_u64();
        let place_draw = rng.next_u64();
        // The draw scaled into [0, out of) by a widening product.
        let (share, out_of) = BERLIN_SHARE;
        let berlin = ((u128::from(kind_draw) * u128::from(out_of)) >> 64) < u128::from(share);
        RegionSource {
            kind: if berlin {
                SourceKind::Berlin
            } else {
                SourceKind::Procedural
            },
            place_draw,
        }
    }

    /// The seed's square, whatever its kind: a walk of the map, a fraction
    /// of a millisecond.
    pub fn square(&self) -> GeoSquare {
        Coverage::berlin()
            .seeded_square(self.place_draw)
            .expect("a kilometre fits all over Berlin")
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
        let square = |min_e, min_n| GeoSquare {
            min_e,
            min_n,
            size_m: geodata::square::SEEDED_SIZE_M,
        };
        use SourceKind::{Berlin, Procedural};
        // Moved once, by the owner's decision: #1637 made every seeded
        // square a kilometre. The kinds are as they were.
        let pinned = vec![
            (Procedural, square(390_768, 5_818_530)),
            (Procedural, square(410_297, 5_811_541)),
            (Procedural, square(403_981, 5_820_616)),
            (Berlin, square(384_753, 5_816_516)),
            (Procedural, square(392_961, 5_821_740)),
            (Procedural, square(399_347, 5_820_099)),
        ];
        assert_eq!(drawn, pinned);
    }

    #[test]
    fn a_quarter_of_seeds_draw_berlin() {
        let sources: Vec<RegionSource> = (0..4_000u64).map(RegionSource::for_seed).collect();
        let berlin = sources
            .iter()
            .filter(|s| s.kind == SourceKind::Berlin)
            .count();
        assert!((900..1_100).contains(&berlin), "{berlin} of 4,000");
    }

    #[test]
    fn a_kilometre_is_drawn_whatever_the_kind_and_lies_in_berlin() {
        let coverage = Coverage::berlin();
        for seed in 0..64u64 {
            let source = RegionSource::for_seed(seed);
            let square = source.square();
            assert!(coverage.contains(&square), "seed {seed}: {square:?}");
            assert_eq!(square.size_m, geodata::square::SEEDED_SIZE_M);
            assert_eq!(
                source.berlin_square().is_some(),
                source.kind == SourceKind::Berlin
            );
            assert!(source.borough().is_some());
        }
    }
}
