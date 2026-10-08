//! Water on a decoded terrain: the level a region's water lies at, and the
//! ground shaped around it so that one flat plane shows the water and
//! nothing else.
//!
//! The terrain layer draws a body of water flat, at its surface: the Spree
//! at the Museumsinsel decodes to 30-31 m wherever it is mapped, and its
//! level there is 30.5 m. A region draws its water as one plane, so
//! [`settle`] takes one level - the median height of the largest body the
//! plane can stand at (see below) - and shapes the ground to it:
//!
//! - every body within [`LEVEL_GAP_M`] of that level is carved below it, its
//!   bed falling [`SHELF_SLOPE`] metres per metre out from the shore, down
//!   to [`MAX_DEPTH_M`];
//! - all other ground, including a body at another level, is kept at least
//!   [`FREEBOARD_M`] above it, so the plane floods nothing that is not
//!   water: an underpass, a building pit, the lower basin of a lock;
//! - the shore between them follows the water's outline, not the map's
//!   pixel steps. A waterline along pixel edges is a staircase, and seen
//!   from the ground its far bank is a row of teeth. So the outline is
//!   blurred ([`SHORE_PIXELS`] pixels each side), and across that margin
//!   the ground crosses the level where the blurred outline crosses a half,
//!   easing from there into the bed on one side and into the bank on the
//!   other: the plane's edge is a curve, and so is the top of the bank.
//!   Within the margin the two rules above give way to it. Water narrower
//!   than about two pixels, a ditch or a single pixel, is smoothed away with
//!   the steps.
//!
//! Keeping the ground dry is only a repair for the low spots a plane would
//! flood, so a level that would sink much of the core is no level for it.
//! A pond on a hill over a valley would lift the whole valley to its
//! surface; a ditch the smoothing takes away would shape the ground for
//! water that is never drawn. So the largest bodies are tried in turn, and
//! the first is taken that sinks no more than [`MAX_SUNK_PER_MILLE`] of the
//! core over [`SUNK_DEPTH_M`] under its level and keeps some water once its
//! shore is smoothed. Where none does, nothing is settled.
//!
//! A body more than the gap above the plane keeps the flat surface the
//! terrain draws it with, and no water. All arithmetic is IEEE-exact and in
//! a fixed order, as in [`crate::raster`], so every peer shapes the same
//! heights the same way.

use std::collections::VecDeque;

/// The fewest pixels a body may have to set the level: smaller ones are a
/// fountain or a pond a few metres across.
pub const MIN_BODY_PIXELS: usize = 8;

/// How many of the largest bodies are tried for the level.
pub const LEVEL_CANDIDATES: usize = 8;

/// The most of the core, in thousandths, a level may sink more than
/// [`SUNK_DEPTH_M`] under its plane: what may be lifted out of it.
pub const MAX_SUNK_PER_MILLE: usize = 20;

/// How deep under the level ground counts as sunk: shallower ground at the
/// water's edge lies in the river's own height class.
pub const SUNK_DEPTH_M: f32 = 1.0;

/// How far another body's level may lie from the plane's and still be
/// carved as water: the fall of a lock, as at the Muehlendamm (1.8 m).
pub const LEVEL_GAP_M: f32 = 3.0;

/// The deepest a bed is carved below the plane.
pub const MAX_DEPTH_M: f32 = 3.0;

/// How fast a bed falls away from the shore: metres down per metre out.
pub const SHELF_SLOPE: f32 = 0.5;

/// How far all ground that is not carved is kept above the plane.
pub const FREEBOARD_M: f32 = 0.2;

/// How many pixels each side of the waterline the shore is smoothed over:
/// the reach of the two 3-pixel box passes that blur the outline.
pub const SHORE_PIXELS: usize = 2;

/// The scale of the shore's profile: how far above the level it would
/// stand at the margin's inland edge, and below it at its edge in the
/// water, before easing into the bank and the bed.
pub const SHORE_RISE_M: f32 = 1.0;

/// What [`settle`] found and did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Water {
    /// The plane's level, metres above sea level: the median height of the
    /// body that set it (the lower median of an even count).
    pub level: f32,
    /// Pixels left below the level: the water the plane shows.
    pub wet: u32,
    /// Water pixels of a body at another level, left as drawn.
    pub stranded: u32,
    /// Pixels raised: lifted out of the water, or into the shore's rise.
    pub raised: u32,
}

/// Find the level of the water in `heights` and shape the ground to it (see
/// the module docs). `water` marks the pixels a land-use map calls water;
/// both grids are `width x height`, row-major, `pixel_m` metres apart.
///
/// Returns `None`, and changes nothing, where no body of at least
/// [`MIN_BODY_PIXELS`] pixels gives a level the plane can stand at. Bodies
/// are 4-connected: two pools touching only at a corner are two bodies.
/// Bodies are tried largest first; of two the same size, the first in
/// row-major order.
///
/// # Panics
///
/// If either grid is not `width x height`.
pub fn settle(
    heights: &mut [f32],
    water: &[bool],
    width: u32,
    height: u32,
    pixel_m: f32,
) -> Option<Water> {
    let (w, h) = (width as usize, height as usize);
    assert!(
        heights.len() == w * h && water.len() == w * h,
        "heights ({}) and water ({}) must both be {width} x {height}",
        heights.len(),
        water.len()
    );
    let (body, bodies) = label_bodies(heights, water, w, h);
    let mut candidates: Vec<usize> = (0..bodies.len())
        .filter(|&b| bodies[b].0 >= MIN_BODY_PIXELS)
        .collect();
    candidates.sort_by(|&a, &b| bodies[b].0.cmp(&bodies[a].0).then(a.cmp(&b)));
    candidates.truncate(LEVEL_CANDIDATES);
    for candidate in candidates {
        let level = bodies[candidate].1;
        let carved: Vec<bool> = body
            .iter()
            .map(|&b| b != NO_BODY && (bodies[b as usize].1 - level).abs() <= LEVEL_GAP_M)
            .collect();
        let sunk = heights
            .iter()
            .zip(&carved)
            .filter(|&(&height, &carved)| !carved && height < level - SUNK_DEPTH_M)
            .count();
        if sunk * 1000 > heights.len() * MAX_SUNK_PER_MILLE {
            continue;
        }
        let mut shaped = heights.to_vec();
        shape(&mut shaped, &carved, level, w, h, pixel_m);
        let mut settled = Water {
            level,
            wet: 0,
            stranded: 0,
            raised: 0,
        };
        for (i, (&now, &was)) in shaped.iter().zip(heights.iter()).enumerate() {
            settled.wet += u32::from(now < level);
            settled.stranded += u32::from(water[i] && !carved[i]);
            settled.raised += u32::from(now > was);
        }
        if settled.wet == 0 {
            // Smoothed away: water too narrow to draw.
            continue;
        }
        heights.copy_from_slice(&shaped);
        return Some(settled);
    }
    None
}

/// Shape `heights` to a plane at `level`: the `carved` beds below it, the
/// rest above it, and the shore between them smoothed (see the module docs).
fn shape(heights: &mut [f32], carved: &[bool], level: f32, w: usize, h: usize, pixel_m: f32) {
    // Pixels out from the shore, counted from every pixel not carved.
    let mut out = vec![u32::MAX; w * h];
    let mut queue = VecDeque::new();
    for (i, d) in out.iter_mut().enumerate() {
        if !carved[i] {
            *d = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        for j in neighbours4(i, w, h) {
            if out[j] == u32::MAX {
                out[j] = out[i] + 1;
                queue.push_back(j);
            }
        }
    }

    // The beds carved, the rest kept dry.
    let crest = level + FREEBOARD_M;
    for (i, height) in heights.iter_mut().enumerate() {
        if carved[i] {
            // A core that is water to its edges has no shore: full depth.
            let depth = if out[i] == u32::MAX {
                MAX_DEPTH_M
            } else {
                (out[i] as f32 * pixel_m * SHELF_SLOPE).min(MAX_DEPTH_M)
            };
            *height = height.min(level - depth);
        } else {
            *height = height.max(crest);
        }
    }

    // The shore. `s` is how much of a pixel the blurred outline calls water;
    // the ground crosses the level where `s` crosses a half, and eases into
    // the bed on the water's side and into the bank on the land's as `s`
    // reaches 1 or 0, so the margin has no edge of its own to step at.
    let mask: Vec<f32> = carved.iter().map(|&c| f32::from(u8::from(c))).collect();
    let wetness = box_blur(&box_blur(&mask, w, h), w, h);
    for (i, height) in heights.iter_mut().enumerate() {
        let s = wetness[i];
        if s <= 0.0 || s >= 1.0 {
            continue;
        }
        let shore = level + (0.5 - s) * 2.0 * SHORE_RISE_M;
        *height = if s >= 0.5 {
            // A pixel of land the outline takes into the water falls from
            // a shallow bed; water falls from its own.
            let bed = if carved[i] {
                *height
            } else {
                level - SHORE_RISE_M
            };
            bed + 2.0 * (1.0 - s) * (shore - bed)
        } else {
            // And one of water it gives to the land rises from a low bank.
            let bank = if carved[i] {
                level + SHORE_RISE_M
            } else {
                *height
            };
            bank + 2.0 * s * (shore - bank)
        };
    }
}

/// One pass of a 3-pixel box blur along each axis, the edge pixel standing
/// in for the one past it.
fn box_blur(values: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut across = vec![0.0; w * h];
    for y in 0..h {
        let row = &values[y * w..(y + 1) * w];
        for x in 0..w {
            across[y * w + x] = (row[x.saturating_sub(1)] + row[x] + row[(x + 1).min(w - 1)]) / 3.0;
        }
    }
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (across[up * w + x] + across[y * w + x] + across[down * w + x]) / 3.0;
        }
    }
    out
}

/// The body index of a pixel that is not water.
const NO_BODY: u32 = u32::MAX;

/// Label the 4-connected bodies of `water` in row-major order of their
/// first pixel: each pixel's body index ([`NO_BODY`] for land), and per body
/// its pixel count and the lower median of its heights.
fn label_bodies(
    heights: &[f32],
    water: &[bool],
    w: usize,
    h: usize,
) -> (Vec<u32>, Vec<(usize, f32)>) {
    let mut body = vec![NO_BODY; w * h];
    let mut bodies = Vec::new();
    let (mut stack, mut members) = (Vec::new(), Vec::new());
    for start in 0..w * h {
        if !water[start] || body[start] != NO_BODY {
            continue;
        }
        let id = bodies.len() as u32;
        body[start] = id;
        stack.push(start);
        members.clear();
        while let Some(i) = stack.pop() {
            members.push(heights[i]);
            for j in neighbours4(i, w, h) {
                if water[j] && body[j] == NO_BODY {
                    body[j] = id;
                    stack.push(j);
                }
            }
        }
        members.sort_by(f32::total_cmp);
        bodies.push((members.len(), members[(members.len() - 1) / 2]));
    }
    (body, bodies)
}

fn neighbours4(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (i % w, i / w);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < w).then(|| i + 1),
        (y > 0).then(|| i - w),
        (y + 1 < h).then(|| i + w),
    ]
    .into_iter()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w x h` grid of `land` metres, and the water mask to paint on it.
    fn plain(w: usize, h: usize, land: f32) -> (Vec<f32>, Vec<bool>) {
        (vec![land; w * h], vec![false; w * h])
    }

    /// Mark the pixels `cols` x `rows` as water at `level`.
    fn pool(
        heights: &mut [f32],
        water: &mut [bool],
        w: usize,
        cols: std::ops::Range<usize>,
        rows: std::ops::Range<usize>,
        level: f32,
    ) {
        for y in rows {
            for x in cols.clone() {
                heights[y * w + x] = level;
                water[y * w + x] = true;
            }
        }
    }

    #[test]
    fn a_river_is_carved_and_its_shore_runs_smoothly_through_the_level() {
        // 12 columns of 2 m pixels: land at 33 m, a 6-pixel river at 30.5 m.
        let (w, h) = (12, 4);
        let (mut heights, mut water) = plain(w, h, 33.0);
        pool(&mut heights, &mut water, w, 3..9, 0..h, 30.5);
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0).unwrap();
        assert_eq!(
            settled,
            Water {
                level: 30.5,
                wet: 24,
                stranded: 0,
                raised: 0
            }
        );
        let row: Vec<f32> = heights[..w].to_vec();
        assert!(heights.chunks(w).all(|r| r == row.as_slice()));
        // Out of the shore's reach the land keeps its height and the bed
        // its full depth; across it the ground falls steadily, and crosses
        // the level where the map's waterline is, between pixels 2 and 3
        // and between 8 and 9.
        assert_eq!((row[0], row[11]), (33.0, 33.0));
        assert_eq!((row[5], row[6]), (27.5, 27.5));
        assert!(row[..6].windows(2).all(|p| p[0] > p[1]), "{row:?}");
        assert!(row[2] > 30.5 && row[3] < 30.5 && row[8] < 30.5 && row[9] > 30.5);
        for (a, b) in row.iter().zip(row.iter().rev()) {
            assert!((a - b).abs() < 1e-5, "symmetric: {row:?}");
        }
    }

    #[test]
    fn a_diagonal_shore_is_a_line_not_a_staircase() {
        // Water wherever x + y < 12 on a 16 x 16 grid: a 45 degree shore,
        // drawn by the map as a staircase of single pixels.
        let (w, h) = (16, 16);
        let (mut heights, mut water) = plain(w, h, 34.0);
        for y in 0..h {
            for x in 0..w {
                if x + y < 12 {
                    heights[y * w + x] = 30.5;
                    water[y * w + x] = true;
                }
            }
        }
        settle(&mut heights, &water, w as u32, h as u32, 2.0).unwrap();
        // Away from the edges every pixel on one diagonal has one height,
        // so the waterline between two diagonals is straight.
        for diagonal in 8..15 {
            let along: Vec<f32> = (3..=diagonal - 3)
                .filter(|&x| diagonal - x < h)
                .map(|x| heights[(diagonal - x) * w + x])
                .collect();
            assert!(
                along.windows(2).all(|p| (p[0] - p[1]).abs() < 1e-5),
                "diagonal {diagonal}: {along:?}"
            );
        }
    }

    #[test]
    fn ground_below_the_level_is_raised_and_far_lakes_are_left_alone() {
        let (w, h) = (30, 20);
        let (mut heights, mut water) = plain(w, h, 34.0);
        // The river: the largest body, at 30.5 m.
        pool(&mut heights, &mut water, w, 0..4, 0..h, 30.5);
        // A basin 1 m higher (a lock's upper reach): within the gap, carved,
        // its pixel corners rounded off.
        pool(&mut heights, &mut water, w, 8..12, 2..6, 31.5);
        // A lake 5 m higher: beyond the gap, left as drawn.
        pool(&mut heights, &mut water, w, 18..21, 1..4, 35.5);
        // A pool 5 m lower, and an underpass: both under the plane, and
        // together 10 pixels of 600, inside the share a level may sink.
        pool(&mut heights, &mut water, w, 18..21, 10..13, 25.5);
        heights[26 + 16 * w] = 28.0;
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0).unwrap();
        assert_eq!(settled.level, 30.5);
        assert_eq!(settled.stranded, 9 + 9);
        assert_eq!(settled.raised, 9 + 1, "the low pool and the underpass");
        assert_eq!(
            settled.wet,
            80 + 12,
            "the river, and the basin less its corners"
        );
        let at = |x: usize, y: usize| heights[y * w + x];
        assert!(at(9, 3) < 30.5 && at(10, 4) < 30.5, "the basin is carved");
        assert!(
            at(8, 2) > 30.5 && at(11, 5) > 30.5,
            "its corners are rounded off"
        );
        assert_eq!(
            (at(18, 1), at(20, 3)),
            (35.5, 35.5),
            "the lake is left alone"
        );
        assert_eq!(at(19, 11), 30.5 + FREEBOARD_M);
        assert_eq!(at(26, 16), 30.5 + FREEBOARD_M);
        assert_eq!(at(29, 19), 34.0, "dry ground above the plane is kept");
    }

    /// The critic's case (#1586): ground rising from 30 m to 50 m across
    /// 80 m, its only water a pond near the top. A plane at the pond's level
    /// would lift the valley to it, so no level is taken and nothing moves.
    #[test]
    fn a_pond_on_a_hill_over_a_valley_sets_no_level() {
        let (w, h) = (40, 40);
        let mut heights: Vec<f32> = (0..w * h).map(|i| 30.0 + (i % w) as f32 * 0.5).collect();
        let mut water = vec![false; w * h];
        pool(&mut heights, &mut water, w, 35..38, 18..21, 48.0);
        let before = heights.clone();
        assert_eq!(settle(&mut heights, &water, w as u32, h as u32, 2.0), None);
        assert_eq!(heights, before);
    }

    /// A lake on the plateau, larger than the river in the valley below it:
    /// its level would sink the valley, so the river's is taken, and the
    /// lake is left as drawn.
    #[test]
    fn a_plateau_lake_over_the_valley_leaves_the_level_to_the_river() {
        let (w, h) = (60, 30);
        let mut heights: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 30 { 33.0 } else { 48.0 })
            .collect();
        let mut water = vec![false; w * h];
        pool(&mut heights, &mut water, w, 0..4, 0..h, 30.5);
        pool(&mut heights, &mut water, w, 40..52, 9..21, 46.0);
        let before = heights.clone();
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0).unwrap();
        assert_eq!(settled.level, 30.5);
        assert_eq!((settled.wet, settled.stranded), (120, 144));
        for (i, (&now, &was)) in heights.iter().zip(&before).enumerate() {
            if i % w >= 6 {
                assert_eq!(
                    now, was,
                    "pixel {i}, beyond the river's shore, is untouched"
                );
            }
        }
    }

    #[test]
    fn small_or_absent_water_changes_nothing() {
        let (w, h) = (6, 6);
        let (mut heights, mut water) = plain(w, h, 33.0);
        assert_eq!(settle(&mut heights, &water, 6, 6, 2.0), None);
        // Seven pixels: one short of a body that sets the level.
        for i in 0..MIN_BODY_PIXELS - 1 {
            heights[i] = 30.0;
            water[i] = true;
        }
        heights[30] = 20.0;
        let before = heights.clone();
        assert_eq!(settle(&mut heights, &water, 6, 6, 2.0), None);
        assert_eq!(heights, before, "nothing is raised without water");
    }

    #[test]
    fn of_two_largest_bodies_the_first_sets_the_level() {
        let (w, h) = (9, 4);
        let (mut heights, mut water) = plain(w, h, 40.0);
        pool(&mut heights, &mut water, w, 0..2, 0..h, 31.0);
        pool(&mut heights, &mut water, w, 7..9, 0..h, 32.0);
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0).unwrap();
        assert_eq!(settled.level, 31.0);
        // Both within the gap: both drawn below the one plane.
        assert_eq!(settled.wet, 16);
        assert!((0..h).all(|y| [0, 1, 7, 8].iter().all(|&x| heights[y * w + x] < 31.0)));
    }

    #[test]
    fn the_level_is_the_median_of_the_body() {
        let (w, h) = (4, 3);
        let (mut heights, mut water) = plain(w, h, 40.0);
        let wet = [30.2, 30.9, 30.4, 30.6, 31.0, 30.5, 30.1, 30.8];
        for (i, v) in wet.into_iter().enumerate() {
            heights[i] = v;
            water[i] = true;
        }
        let settled = settle(&mut heights, &water, w as u32, h as u32, 1.0).unwrap();
        // Sorted: 30.1 30.2 30.4 30.5 | 30.6 30.8 30.9 31.0 - the lower median.
        assert_eq!(settled.level, 30.5);
    }

    #[test]
    fn a_core_of_water_to_its_edges_is_carved_full_depth() {
        let (mut heights, water) = (vec![30.5; 9], vec![true; 9]);
        let settled = settle(&mut heights, &water, 3, 3, 2.0).unwrap();
        assert_eq!(settled.wet, 9);
        assert!(heights.iter().all(|&h| h == 30.5 - MAX_DEPTH_M));
    }

    #[test]
    fn bodies_touching_at_a_corner_are_two() {
        let (w, h) = (6, 6);
        let (mut heights, mut water) = plain(w, h, 40.0);
        // Two 3 x 3 blocks meeting corner to corner; the second 1 m lower.
        pool(&mut heights, &mut water, w, 0..3, 0..3, 31.0);
        pool(&mut heights, &mut water, w, 3..6, 3..6, 30.0);
        let settled = settle(&mut heights, &water, 6, 6, 2.0).unwrap();
        assert_eq!(settled.level, 31.0, "the first block sets the level");
        assert_eq!(settled.wet, 18);
    }

    /// Water a pixel wide is below the smoothed outline's resolution: it
    /// draws nothing, so it shapes nothing either (the critic's second case,
    /// #1586). On level ground, where its level sinks nothing, this rule
    /// alone refuses it.
    #[test]
    fn a_ditch_a_pixel_wide_settles_nothing() {
        let (w, h) = (20, 20);
        let (mut heights, mut water) = plain(w, h, 33.0);
        pool(&mut heights, &mut water, w, 10..11, 0..h, 30.5);
        let before = heights.clone();
        assert_eq!(settle(&mut heights, &water, w as u32, h as u32, 2.0), None);
        assert_eq!(heights, before);
    }

    #[test]
    #[should_panic(expected = "must both be 3 x 3")]
    fn grids_of_another_size_are_refused() {
        let mut heights = vec![30.0; 9];
        settle(&mut heights, &[true; 8], 3, 3, 2.0);
    }
}
