//! A seeded world's signature lake (#1793): water standing in a dip of its
//! land above the sea - a second terrain water layer, which the compile
//! fills round its point to its level within its rectangle
//! (`water::bodies::lake`, #1760).
//!
//! It is sited as the other features are ([`super::feature`]), after the
//! settlement and its paths, on the probe: a basin whose floor lies between
//! [`NEAREST`] and [`FARTHEST`] of the arrival - a cell lower than its four
//! neighbours, filled as water fills it until it would run over its rim
//! ([`TerrainProbe::basins_near`]). Its water stands [`SPILL_MARGIN`] under
//! that rim at the most, covers [`MOST_CELLS`] cells at the most, and is
//! [`LEAST_DEPTH`] deep and [`LEAST_CELLS`] cells wide at the least. The
//! probe under-reads the full map's relief (#1773) - its shores by many
//! metres along a gentle bank - so the layer's rectangle reaches
//! [`RECT_PAD`] past the probe's lake, and that whole rectangle keeps
//! [`MARGIN`] off every building, path, landing and gate, the view down to
//! the landmark among them: whatever the full map floods, it floods nothing
//! of the settlement. The full map's dip is deeper than the probe reads,
//! and its rim notched where the probe reads it whole: where its water
//! would run past the rectangle, the compile stands it at its brim
//! (`water::bodies::brim`), the highest its ground holds there - about half
//! the seeded lakes, a few metres under their level. The basin the first
//! frame shows, beside the landmark and near the reach the other features
//! keep, is the best; the first of equals.
//!
//! The probe's land holds few basins deep enough (#1793's census: of 4,317
//! over 211 worlds, 48 two metres deep), near enough all of them a mesa's
//! pits between its terraces: a lake stands where the land holds one, the
//! owner's choice of 2026-10-26, in about one world in ten.

use super::feature::KeepClear;
use super::paths::off_segment;
use super::siting::TerrainProbe;
use super::viewpoint::Viewpoint;

/// How near the arrival a lake's floor lies at the least, and at the most
/// (m): as the other features.
const NEAREST: f32 = 50.0;
const FARTHEST: f32 = 260.0;

/// How far over the sea a lake's floor lies at the least (m): a dip by the
/// shore is the sea's.
const OVER_THE_SEA: f32 = 4.0;

/// How many cells a basin's water may cover before it runs over its rim
/// for the basin to be read at all: a dip as wide as a valley floor is no
/// lake's.
const BASIN_CELLS: usize = 400;

/// How many probe cells a lake covers at the most (about 7,000 m2 on a
/// 10 m probe), and at the least.
const MOST_CELLS: usize = 60;
const LEAST_CELLS: usize = 4;

/// How far under its basin's rim a lake's water stands at the least (m):
/// the full map's rim can be notched where the probe reads it whole.
const SPILL_MARGIN: f32 = 1.5;

/// How deep a lake stands over its floor at the least, on the probe (m).
const LEAST_DEPTH: f32 = 1.0;

/// How far its rectangle reaches past the probe's lake (m): the full map's
/// shore lies metres off the probe's along a gentle bank.
const RECT_PAD: f32 = 25.0;

/// How far a lake's rectangle keeps off the ground of what stands in the
/// settlement, and off its paths' edges (m): as a feature does.
const MARGIN: f32 = 10.0;

/// How far off the view's heading a lake lies best, and at the most, as
/// the cosine of that angle: as a feature does.
const BEST_OFF: f32 = 0.94;
const LEAST_OFF: f32 = 0.99;

/// How far from the arrival a lake lies best (m).
const BEST_REACH: f32 = 120.0;

/// How calm a lake is beside the sea: its waves and foam this share of the
/// sea's.
pub const CALM: f32 = 0.3;

/// How opaque a lake reads from above at the least: a mesa's pit holds
/// water metres deep, and the sea's own plane stands in the bottom of most
/// (#1793's critic: 51 of 91 lakes over ground below the sea), which a
/// clear lake would show through it as a second shore.
pub const DEEP: f32 = 0.9;

/// A signature lake's water layer: its level and its rectangle about its
/// point - the feature's `at`, its basin's floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LakeLayer {
    /// Its water's level (world Y).
    pub level: f32,
    /// How far its rectangle reaches from its point along X and along Z
    /// (m).
    pub half: [f32; 2],
}

impl LakeLayer {
    /// How far round its point its rectangle reaches at the most (m): the
    /// ground what is sited after it keeps off.
    pub fn reach(&self) -> f32 {
        (self.half[0] * self.half[0] + self.half[1] * self.half[1]).sqrt()
    }
}

/// The best basin for a lake round the arrival of `viewpoint` (see the
/// module docs), clear of what `clear` holds: its point, its reach and its
/// layer. `None` where no basin takes one.
pub fn site(
    probe: &TerrainProbe,
    viewpoint: &Viewpoint,
    clear: &KeepClear,
) -> Option<([f32; 2], f32, LakeLayer)> {
    let landing = viewpoint.landing;
    let heading = viewpoint.toward;
    let edge = probe.half_extent();
    let mut best: Option<(f32, [f32; 2], LakeLayer)> = None;
    for basin in probe.basins_near(landing, NEAREST, FARTHEST, OVER_THE_SEA, BASIN_CELLS) {
        let at = basin.floor_at;
        let (dx, dz) = (at[0] - landing[0], at[1] - landing[1]);
        let far = (dx * dx + dz * dz).sqrt();
        let off = (dx * heading[0] + dz * heading[1]) / far;
        if off > LEAST_OFF {
            continue;
        }
        let level = match basin.level_for(MOST_CELLS) {
            Some(full) => full.min(basin.spill - SPILL_MARGIN),
            None => basin.spill - SPILL_MARGIN,
        };
        if level - basin.floor < LEAST_DEPTH {
            continue;
        }
        let water: Vec<[f32; 2]> = basin.under(level).collect();
        if water.len() < LEAST_CELLS {
            continue;
        }
        // Its rectangle about its point: the probe's lake, each cell's half
        // and the pad; and how far its water reaches round it.
        let cell = probe.cell_size();
        let half = water.iter().fold([0.0f32; 2], |half, p| {
            [
                half[0].max((p[0] - at[0]).abs()),
                half[1].max((p[1] - at[1]).abs()),
            ]
        });
        let water_reach = (half[0] * half[0] + half[1] * half[1]).sqrt() + cell * 0.5;
        let half = half.map(|h| h + cell * 0.5 + RECT_PAD);
        if at[0].abs() + half[0] > edge || at[1].abs() + half[1] > edge {
            continue;
        }
        let lake = LakeLayer { level, half };
        if !keeps_clear(at, half, clear) {
            continue;
        }
        // Its water seen over its pit's rim, not past it.
        let mut score = if viewpoint.shows(probe, at, level + 0.5, water_reach) {
            3.0
        } else {
            0.0
        };
        score -= (off - BEST_OFF).abs() * 8.0;
        score -= ((far - BEST_REACH) / BEST_REACH).abs();
        if off < 0.0 {
            // Behind the arrival: met on the way out, not seen coming in.
            score -= 2.0;
        }
        if best.as_ref().is_none_or(|(kept, ..)| score > *kept) {
            best = Some((score, at, lake));
        }
    }
    best.map(|(_, at, lake)| (at, lake.reach(), lake))
}

/// Whether the rectangle `half` round `at` keeps [`MARGIN`] off every disc
/// of `clear` and off its paths' edges.
fn keeps_clear(at: [f32; 2], half: [f32; 2], clear: &KeepClear) -> bool {
    // How far `p` lies outside the rectangle: nought inside it.
    let outside = |p: [f32; 2]| {
        let (dx, dz) = (
            ((p[0] - at[0]).abs() - half[0]).max(0.0),
            ((p[1] - at[1]).abs() - half[1]).max(0.0),
        );
        (dx * dx + dz * dz).sqrt()
    };
    let width = clear.width * 0.5;
    let reach = (half[0] * half[0] + half[1] * half[1]).sqrt();
    clear
        .stands
        .iter()
        .all(|&(spot, r)| outside(spot) >= r + MARGIN)
        && clear
            .paths
            .iter()
            .flat_map(|line| line.windows(2))
            .all(|pair| {
                // A way past the rectangle's reach is clear of it; a nearer
                // one is read a metre apart along it.
                off_segment(at, pair[0], pair[1]) >= reach + width + MARGIN
                    || way_clear(pair[0], pair[1], &outside, width + MARGIN)
            })
}

/// Whether every point of the way `a`-`b`, read a metre apart, lies
/// `margin` or more outside the rectangle `outside` measures.
fn way_clear(a: [f32; 2], b: [f32; 2], outside: &impl Fn([f32; 2]) -> f32, margin: f32) -> bool {
    let length = ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt();
    let steps = length.ceil().max(1.0) as usize;
    (0..=steps).all(|k| {
        let t = k as f32 / steps as f32;
        outside([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]) >= margin
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain about 10 m over the sea, rising a centimetre a metre east,
    /// and a bowl 6 m deep, 60 m across, 120 m east of an arrival looking
    /// north: its rim lowest to the west, where its water runs over.
    fn bowl() -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let height = |x: f32, z: f32| {
            let r = (x - 120.0).hypot(z);
            10.0 + (x - 120.0) * 0.01 - 6.0 * (1.0 - (r / 30.0).min(1.0)).powi(2)
        };
        let data = (0..GRID * GRID)
            .map(|i| {
                height(
                    (i % GRID) as f32 * CELL - half,
                    (i / GRID) as f32 * CELL - half,
                )
            })
            .collect();
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: CELL,
            data,
        };
        TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT)
    }

    fn view() -> Viewpoint {
        Viewpoint {
            landing: [0.0, 0.0],
            toward: [0.0, -1.0],
            pitch: 0.2,
            score: 0.0,
        }
    }

    /// The bowl takes a lake: its point the bowl's floor, its water under
    /// the rim by the margin, deep enough, and its rectangle round the
    /// bowl with the pad.
    #[test]
    fn a_bowl_holds_a_lake() {
        let probe = bowl();
        let view = view();
        let clear = KeepClear {
            stands: &[],
            paths: &[],
            width: 2.5,
        };
        let (at, reach, lake) = site(&probe, &view, &clear).expect("a lake");
        assert!(
            (at[0] - 120.0).abs() <= 10.0 && at[1].abs() <= 10.0,
            "{at:?}"
        );
        // Its rim's lowest, to the west, is 9.7 m.
        assert!(lake.level <= 9.7 - SPILL_MARGIN + 0.1, "{lake:?}");
        assert!(
            lake.level - probe.height_between(at) >= LEAST_DEPTH,
            "{lake:?}"
        );
        for half in lake.half {
            assert!(
                (RECT_PAD..=30.0 + RECT_PAD + 10.0).contains(&half),
                "{lake:?}"
            );
        }
        assert!((reach - lake.reach()).abs() < 1e-6);
    }

    /// A building or a path inside the lake's rectangle and its margin
    /// keeps the lake away; one well clear of it does not.
    #[test]
    fn a_lake_keeps_off_the_settlement_and_its_paths() {
        let probe = bowl();
        let view = view();
        let near = [([150.0, 0.0], 8.0)];
        let blocked = KeepClear {
            stands: &near,
            paths: &[],
            width: 2.5,
        };
        assert!(
            site(&probe, &view, &blocked).is_none(),
            "a building in its way"
        );
        let way = vec![vec![[120.0, -200.0], [120.0, 200.0]]];
        let crossed = KeepClear {
            stands: &[],
            paths: &way,
            width: 2.5,
        };
        assert!(site(&probe, &view, &crossed).is_none(), "a path across it");
        let far = [([-200.0, 0.0], 8.0)];
        let clear = KeepClear {
            stands: &far,
            paths: &[],
            width: 2.5,
        };
        assert!(site(&probe, &view, &clear).is_some(), "a building far off");
    }

    /// A probe with no dip holds no lake.
    #[test]
    fn a_plain_holds_none() {
        let map = gen_jobs::HeightmapData {
            width: 96,
            height: 96,
            scale: 10.0,
            data: (0..96 * 96)
                .map(|i| 10.0 + (i % 96) as f32 * 0.01)
                .collect(),
        };
        let probe = TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT);
        let clear = KeepClear {
            stands: &[],
            paths: &[],
            width: 2.5,
        };
        assert!(site(&probe, &view(), &clear).is_none());
    }
}
