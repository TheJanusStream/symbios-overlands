//! Water on a decoded terrain: each body of a region's water settled at its
//! own level, and the ground shaped round it so that its surface shows the
//! water and nothing else (#1724).
//!
//! The terrain layer draws a body of water flat, at its surface: the Spree
//! at the Museumsinsel decodes to 30-31 m wherever it is mapped, and its
//! level there is 30.5 m. [`settle`] takes each body - the water's pixels,
//! 4-connected - of at least [`MIN_BODY_PIXELS`] at its own level, the
//! median height of its pixels, and shapes the ground to it:
//!
//! - its bed is carved below its level, falling [`SHELF_SLOPE`] metres per
//!   metre out from its shore, down to [`MAX_DEPTH_M`];
//! - its shore follows the water's outline, not the map's pixel steps. A
//!   waterline along pixel edges is a staircase, and seen from the ground
//!   its far bank is a row of teeth. So the outline is blurred
//!   ([`SHORE_PIXELS`] pixels each side), and across that margin the ground
//!   crosses the level where the blurred outline crosses a half, easing from
//!   there into the bed on one side and into the bank on the other, the bank
//!   first kept [`FREEBOARD_M`] over the level so that the water's edge
//!   floods no low spot: the water's edge is a curve, and so is the top of
//!   the bank. Water narrower than about two pixels, a ditch or a single
//!   pixel, is smoothed away with the steps.
//!
//! Each body is drawn as its own surface over its own pixels
//! ([`Body::wet`]), never as one plane across the region, so the rest of the
//! ground keeps the heights the terrain draws: an underpass or a building
//! pit beside a river and below its level stays the hollow it is, and a lake
//! on the plateau and the river in the valley below it each lie at their own
//! levels, as do the two reaches of a lock. Each body is shaped against the
//! ground as drawn, so none depends on another or on the order they are
//! taken in: a body's own pixels are its alone, and land in two bodies'
//! shores - a dam between the reaches - takes the higher of their banks.
//!
//! A body whose smoothed shore leaves no water shapes nothing, and keeps the
//! flat surface the terrain draws it with. All arithmetic is IEEE-exact and
//! in a fixed order, as in [`crate::raster`], so every peer shapes the same
//! heights the same way.

use std::collections::VecDeque;

/// The fewest pixels a body may have to be settled: smaller ones are a
/// fountain or a pond a few metres across.
pub const MIN_BODY_PIXELS: usize = 8;

/// The deepest a bed is carved below its body's level.
pub const MAX_DEPTH_M: f32 = 3.0;

/// How fast a bed falls away from the shore: metres down per metre out.
pub const SHELF_SLOPE: f32 = 0.5;

/// How far the land in a body's shore is kept above its level, before the
/// shore is eased: no low spot at the water's edge lies under it.
pub const FREEBOARD_M: f32 = 0.2;

/// How many pixels each side of the waterline the shore is smoothed over:
/// the reach of the two 3-pixel box passes that blur the outline.
pub const SHORE_PIXELS: usize = 2;

/// The scale of the shore's profile: how far above the level it would
/// stand at the margin's inland edge, and below it at its edge in the
/// water, before easing into the bank and the bed.
pub const SHORE_RISE_M: f32 = 1.0;

/// One body of water, settled at its own level.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Its level, metres above sea level: the median height of its pixels
    /// (the lower median of an even count).
    pub level: f32,
    /// The pixels its water covers once the ground is settled, row-major
    /// indices in ascending order: those below its level, of its own and of
    /// the land its smoothed outline takes in. A pixel two bodies' water
    /// would cover is the higher body's.
    pub wet: Vec<u32>,
}

/// What [`settle`] found and did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Water {
    /// The bodies settled, in the row-major order of their first pixels.
    pub bodies: Vec<Body>,
    /// Water pixels left dry: of bodies too small or too narrow to settle,
    /// and the narrow parts a smoothed outline gives to the land.
    pub stranded: u32,
    /// Pixels raised: into a shore's rise.
    pub raised: u32,
}

/// Settle each body of the water in `heights` at its own level, and shape
/// the ground to it (see the module docs). `water` marks the pixels a
/// land-use map calls water; both grids are `width x height`, row-major,
/// `pixel_m` metres apart. Bodies are 4-connected: two pools touching only
/// at a corner are two bodies. Where no body settles, the water has none,
/// and the heights are unchanged.
///
/// # Panics
///
/// If either grid is not `width x height`.
pub fn settle(heights: &mut [f32], water: &[bool], width: u32, height: u32, pixel_m: f32) -> Water {
    let (w, h) = (width as usize, height as usize);
    assert!(
        heights.len() == w * h && water.len() == w * h,
        "heights ({}) and water ({}) must both be {width} x {height}",
        heights.len(),
        water.len()
    );
    let (body, bodies) = label_bodies(heights, water, w, h);
    let drawn = heights.to_vec();
    let shapings: Vec<Shaping> = bodies
        .iter()
        .enumerate()
        .filter(|(_, labelled)| labelled.pixels >= MIN_BODY_PIXELS)
        .filter_map(|(id, labelled)| shape(&drawn, &body, id as u32, labelled, (w, h), pixel_m))
        .collect();
    let mut settled = vec![false; bodies.len()];
    for shaping in &shapings {
        settled[shaping.id as usize] = true;
    }
    // A settled body's own pixel is its alone; land takes the highest bank
    // shaped over it.
    let anothers =
        |i: usize, id: u32| body[i] != id && body[i] != NO_BODY && settled[body[i] as usize];
    for shaping in &shapings {
        for &(i, shaped) in &shaping.own {
            heights[i] = shaped;
        }
    }
    let mut banked = vec![false; w * h];
    for shaping in &shapings {
        for &(i, shaped) in &shaping.land {
            if anothers(i, shaping.id) {
                continue;
            }
            heights[i] = if banked[i] {
                heights[i].max(shaped)
            } else {
                shaped
            };
            banked[i] = true;
        }
    }
    // Which body's water covers each pixel: the highest whose level the
    // settled ground lies under there.
    let mut owner = vec![NO_BODY; w * h];
    for (k, shaping) in shapings.iter().enumerate() {
        let candidates = shaping.own.iter().chain(&shaping.land);
        for &(i, _) in candidates.filter(|&&(i, _)| !anothers(i, shaping.id)) {
            let higher = owner[i] == NO_BODY || shaping.level > shapings[owner[i] as usize].level;
            if heights[i] < shaping.level && higher {
                owner[i] = k as u32;
            }
        }
    }
    let mut settled_water = Water {
        bodies: shapings
            .iter()
            .map(|shaping| Body {
                level: shaping.level,
                wet: Vec::new(),
            })
            .collect(),
        stranded: 0,
        raised: 0,
    };
    for (i, &k) in owner.iter().enumerate() {
        if k != NO_BODY {
            settled_water.bodies[k as usize].wet.push(i as u32);
        }
        settled_water.stranded += u32::from(water[i] && k == NO_BODY);
        settled_water.raised += u32::from(heights[i] > drawn[i]);
    }
    settled_water
}

/// One body's shaping of the ground (see the module docs): the heights it
/// gives its own pixels, and the land in its shore.
struct Shaping {
    /// Its index among the labelled bodies.
    id: u32,
    level: f32,
    own: Vec<(usize, f32)>,
    land: Vec<(usize, f32)>,
}

/// Body `id`'s shaping of `drawn` at its level, or `None` where its smoothed
/// shore leaves no pixel under its level. Worked in a window round the body
/// as far as its shore reaches, which gives each pixel there what a pass
/// over the whole grid would: past the window the body's outline is nothing.
fn shape(
    drawn: &[f32],
    body: &[u32],
    id: u32,
    labelled: &Labelled,
    (w, h): (usize, usize),
    pixel_m: f32,
) -> Option<Shaping> {
    let level = labelled.median;
    let crest = level + FREEBOARD_M;
    let pad = SHORE_PIXELS + 2;
    let (x0, y0) = (
        labelled.x0.saturating_sub(pad),
        labelled.y0.saturating_sub(pad),
    );
    let (x1, y1) = (
        (labelled.x1 + pad).min(w - 1),
        (labelled.y1 + pad).min(h - 1),
    );
    let (ww, wh) = (x1 - x0 + 1, y1 - y0 + 1);
    let at = |local: usize| (y0 + local / ww) * w + x0 + local % ww;
    let own: Vec<bool> = (0..ww * wh).map(|local| body[at(local)] == id).collect();

    // Pixels out from the shore, counted from every pixel not the body's.
    let mut out = vec![u32::MAX; ww * wh];
    let mut queue = VecDeque::new();
    for (local, d) in out.iter_mut().enumerate() {
        if !own[local] {
            *d = 0;
            queue.push_back(local);
        }
    }
    while let Some(local) = queue.pop_front() {
        for next in neighbours4(local, ww, wh) {
            if out[next] == u32::MAX {
                out[next] = out[local] + 1;
                queue.push_back(next);
            }
        }
    }

    // The bed carved, the land kept dry.
    let mut ground: Vec<f32> = (0..ww * wh).map(|local| drawn[at(local)]).collect();
    for (local, height) in ground.iter_mut().enumerate() {
        if own[local] {
            // A core that is water to its edges has no shore: full depth.
            let depth = if out[local] == u32::MAX {
                MAX_DEPTH_M
            } else {
                (out[local] as f32 * pixel_m * SHELF_SLOPE).min(MAX_DEPTH_M)
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
    let mask: Vec<f32> = own.iter().map(|&o| f32::from(u8::from(o))).collect();
    let wetness = box_blur(&box_blur(&mask, ww, wh), ww, wh);
    let (mut shaped_own, mut land, mut keeps_water) = (Vec::new(), Vec::new(), false);
    for (local, &s) in wetness.iter().enumerate() {
        let carved = own[local];
        let in_shore = s > 0.0 && s < 1.0;
        if !carved && !in_shore {
            continue;
        }
        let mut height = ground[local];
        if in_shore {
            let shore = level + (0.5 - s) * 2.0 * SHORE_RISE_M;
            height = if s >= 0.5 {
                // A pixel of land the outline takes into the water falls from
                // a shallow bed; water falls from its own.
                let bed = if carved { height } else { level - SHORE_RISE_M };
                bed + 2.0 * (1.0 - s) * (shore - bed)
            } else {
                // And one of water it gives to the land rises from a low bank.
                let bank = if carved { level + SHORE_RISE_M } else { height };
                bank + 2.0 * s * (shore - bank)
            };
        }
        keeps_water |= height < level;
        let entry = (at(local), height);
        if carved {
            shaped_own.push(entry);
        } else {
            land.push(entry);
        }
    }
    keeps_water.then_some(Shaping {
        id,
        level,
        own: shaped_own,
        land,
    })
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

/// A labelled body: its pixel count, the lower median of its heights, and
/// the box its pixels lie in, inclusive.
struct Labelled {
    pixels: usize,
    median: f32,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Label the 4-connected bodies of `water` in row-major order of their
/// first pixel: each pixel's body index ([`NO_BODY`] for land), and each
/// body.
fn label_bodies(heights: &[f32], water: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<Labelled>) {
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
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            members.push(heights[i]);
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            for j in neighbours4(i, w, h) {
                if water[j] && body[j] == NO_BODY {
                    body[j] = id;
                    stack.push(j);
                }
            }
        }
        members.sort_by(f32::total_cmp);
        bodies.push(Labelled {
            pixels: members.len(),
            median: members[(members.len() - 1) / 2],
            x0,
            y0,
            x1,
            y1,
        });
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

    /// The bodies' levels, in their order.
    fn levels(water: &Water) -> Vec<f32> {
        water.bodies.iter().map(|b| b.level).collect()
    }

    #[test]
    fn a_river_is_carved_and_its_shore_runs_smoothly_through_the_level() {
        // 12 columns of 2 m pixels: land at 33 m, a 6-pixel river at 30.5 m.
        let (w, h) = (12, 4);
        let (mut heights, mut water) = plain(w, h, 33.0);
        pool(&mut heights, &mut water, w, 3..9, 0..h, 30.5);
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert_eq!(levels(&settled), [30.5]);
        assert_eq!(settled.bodies[0].wet.len(), 24);
        assert_eq!((settled.stranded, settled.raised), (0, 0));
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
        assert_eq!(
            levels(&settle(&mut heights, &water, w as u32, h as u32, 2.0)),
            [30.5]
        );
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

    /// #1724: every body lies at its own level - a lock's upper reach a
    /// metre over the river, a lake five over it and a pool five under it -
    /// and the ground between them keeps the heights it was drawn with, an
    /// underpass below the river's level among them: no plane spans the
    /// world to flood it.
    #[test]
    fn every_body_settles_at_its_own_level_and_the_ground_between_keeps_its_heights() {
        let (w, h) = (30, 20);
        let (mut heights, mut water) = plain(w, h, 34.0);
        pool(&mut heights, &mut water, w, 0..4, 0..h, 30.5);
        pool(&mut heights, &mut water, w, 8..12, 2..6, 31.5);
        pool(&mut heights, &mut water, w, 18..21, 1..4, 35.5);
        pool(&mut heights, &mut water, w, 18..21, 10..13, 25.5);
        heights[26 + 16 * w] = 28.0;
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        // In the order of their first pixels: the river, the lake, the
        // basin, the pool.
        assert_eq!(levels(&settled), [30.5, 35.5, 31.5, 25.5]);
        let at = |x: usize, y: usize| heights[y * w + x];
        for (body, (x, y)) in settled
            .bodies
            .iter()
            .zip([(1, 10), (19, 2), (9, 3), (19, 11)])
        {
            assert!(at(x, y) < body.level, "the body at {x}, {y} holds water");
            assert!(body.wet.contains(&((y * w + x) as u32)));
        }
        // A 3 x 3 body's corners are smoothed away; its middle keeps water.
        assert!(at(18, 1) > 35.5 && at(19, 1) < 35.5);
        assert_eq!(settled.bodies[1].wet.len(), 5);
        assert_eq!(
            settled.bodies[2].wet.len(),
            12,
            "the basin less its corners"
        );
        assert_eq!(at(26, 16), 28.0, "the underpass is left as it is");
        assert_eq!(at(29, 19), 34.0, "dry ground is kept");
        assert_eq!(settled.stranded, 4 + 4 + 4, "three bodies' corners");
    }

    /// A pond on a hill over a valley lies at its own level, and the valley
    /// beyond its shore keeps every height it was drawn with - where one
    /// plane at the pond's level would have lifted the valley to it.
    #[test]
    fn a_pond_on_a_hill_settles_at_its_level_and_leaves_the_valley_alone() {
        let (w, h) = (40, 40);
        let mut heights: Vec<f32> = (0..w * h).map(|i| 30.0 + (i % w) as f32 * 0.5).collect();
        let mut water = vec![false; w * h];
        pool(&mut heights, &mut water, w, 35..38, 18..21, 48.0);
        let before = heights.clone();
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert_eq!(levels(&settled), [48.0]);
        for (i, (&now, &was)) in heights.iter().zip(&before).enumerate() {
            let (x, y) = (i % w, i / w);
            if x + 3 < 35 || y + 3 < 18 || y > 20 + 3 {
                assert_eq!(now, was, "pixel ({x}, {y}), beyond the pond's shore");
            }
        }
        assert!(heights[19 * w + 36] < 48.0);
    }

    /// A lake on the plateau, larger than the river in the valley below it:
    /// each lies at its own level, and the ground beyond their shores is
    /// untouched.
    #[test]
    fn a_plateau_lake_and_the_river_below_it_keep_their_levels() {
        let (w, h) = (60, 30);
        let mut heights: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 30 { 33.0 } else { 48.0 })
            .collect();
        let mut water = vec![false; w * h];
        pool(&mut heights, &mut water, w, 0..4, 0..h, 30.5);
        pool(&mut heights, &mut water, w, 40..52, 9..21, 46.0);
        let before = heights.clone();
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert_eq!(levels(&settled), [30.5, 46.0]);
        assert_eq!(settled.stranded, 4, "the lake's corners");
        assert!(heights[15 * w + 46] < 46.0 && heights[15 * w + 1] < 30.5);
        for (i, (&now, &was)) in heights.iter().zip(&before).enumerate() {
            let x = i % w;
            if (6..37).contains(&x) || x >= 55 {
                assert_eq!(now, was, "pixel {i}, beyond either shore, is untouched");
            }
        }
    }

    #[test]
    fn small_or_absent_water_changes_nothing() {
        let (w, h) = (6, 6);
        let (mut heights, mut water) = plain(w, h, 33.0);
        assert_eq!(settle(&mut heights, &water, 6, 6, 2.0), Water::default());
        // Seven pixels: one short of a body that settles.
        for i in 0..MIN_BODY_PIXELS - 1 {
            heights[i] = 30.0;
            water[i] = true;
        }
        heights[30] = 20.0;
        let before = heights.clone();
        let settled = settle(&mut heights, &water, 6, 6, 2.0);
        assert!(settled.bodies.is_empty());
        assert_eq!(settled.stranded, 7, "left as drawn");
        assert_eq!(heights, before, "nothing is raised without water");
    }

    /// Two bodies a metre apart each keep their own level - where one plane
    /// would have drawn the higher at the lower's.
    #[test]
    fn two_bodies_a_metre_apart_keep_their_levels() {
        let (w, h) = (9, 4);
        let (mut heights, mut water) = plain(w, h, 40.0);
        pool(&mut heights, &mut water, w, 0..2, 0..h, 31.0);
        pool(&mut heights, &mut water, w, 7..9, 0..h, 32.0);
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert_eq!(levels(&settled), [31.0, 32.0]);
        assert_eq!(
            settled.bodies[0].wet.len() + settled.bodies[1].wet.len(),
            16
        );
        assert!((0..h).all(|y| heights[y * w] < 31.0 && heights[y * w + 8] < 32.0));
    }

    /// #1724: where two reaches meet across a dam a pixel wide, each keeps
    /// its own water under its own level, the dam stands over both, and no
    /// pixel is under the water of both.
    #[test]
    fn where_two_shores_meet_the_dam_takes_the_higher_bank() {
        let (w, h) = (21, 8);
        let (mut heights, mut water) = plain(w, h, 36.0);
        pool(&mut heights, &mut water, w, 0..10, 0..h, 30.5);
        pool(&mut heights, &mut water, w, 11..21, 0..h, 32.3);
        heights
            .iter_mut()
            .skip(10)
            .step_by(w)
            .for_each(|dam| *dam = 33.0);
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert_eq!(levels(&settled), [30.5, 32.3]);
        for y in 0..h {
            assert!(heights[y * w + 10] > 32.3, "the dam at row {y}");
            assert!(heights[y * w + 4] < 30.5 && heights[y * w + 16] < 32.3);
        }
        let (lower, upper) = (&settled.bodies[0].wet, &settled.bodies[1].wet);
        assert!(lower.iter().all(|i| !upper.contains(i)));
        assert!(lower.iter().all(|&i| (i as usize) % w < 10));
        assert!(upper.iter().all(|&i| (i as usize) % w > 10));
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
        let settled = settle(&mut heights, &water, w as u32, h as u32, 1.0);
        // Sorted: 30.1 30.2 30.4 30.5 | 30.6 30.8 30.9 31.0 - the lower median.
        assert_eq!(levels(&settled), [30.5]);
    }

    #[test]
    fn a_core_of_water_to_its_edges_is_carved_full_depth() {
        let (mut heights, water) = (vec![30.5; 9], vec![true; 9]);
        let settled = settle(&mut heights, &water, 3, 3, 2.0);
        assert_eq!(settled.bodies[0].wet.len(), 9);
        assert!(heights.iter().all(|&h| h == 30.5 - MAX_DEPTH_M));
    }

    #[test]
    fn bodies_touching_at_a_corner_are_two() {
        let (w, h) = (6, 6);
        let (mut heights, mut water) = plain(w, h, 40.0);
        // Two 3 x 3 blocks meeting corner to corner; the second 1 m lower.
        pool(&mut heights, &mut water, w, 0..3, 0..3, 31.0);
        pool(&mut heights, &mut water, w, 3..6, 3..6, 30.0);
        let settled = settle(&mut heights, &water, 6, 6, 2.0);
        assert_eq!(levels(&settled), [31.0, 30.0], "each its own");
        assert!(heights[7] < 31.0 && heights[28] < 30.0);
    }

    /// Water a pixel wide is below the smoothed outline's resolution: it
    /// draws nothing, so it shapes nothing either (the critic's second case,
    /// #1586).
    #[test]
    fn a_ditch_a_pixel_wide_settles_nothing() {
        let (w, h) = (20, 20);
        let (mut heights, mut water) = plain(w, h, 33.0);
        pool(&mut heights, &mut water, w, 10..11, 0..h, 30.5);
        let before = heights.clone();
        let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
        assert!(settled.bodies.is_empty());
        assert_eq!(heights, before);
    }

    /// A body is shaped as a pass over the whole grid would shape it: the
    /// window it is worked in reaches past every pixel its shore touches.
    /// Pinned against the single-body arithmetic this module had before
    /// #1724, which shaped the whole grid at once.
    #[test]
    fn a_body_is_shaped_as_the_whole_grid_would_shape_it() {
        // In the middle, against the north-west corner, and against the east
        // edge - where the window is clamped to the grid.
        for (cx, cy) in [(11.3, 8.6), (1.2, 0.4), (22.6, 9.0)] {
            let (w, h) = (24, 18);
            let mut heights: Vec<f32> = (0..w * h)
                .map(|i| 33.0 + ((i * 7919) % 13) as f32 * 0.1)
                .collect();
            let mut water = vec![false; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                    if dx * dx + dy * dy < 30.0 {
                        heights[y * w + x] = 30.5 + ((x + y) % 3) as f32 * 0.1;
                        water[y * w + x] = true;
                    }
                }
            }
            let mut whole = heights.clone();
            let level = {
                let mut members: Vec<f32> = (0..w * h)
                    .filter(|&i| water[i])
                    .map(|i| heights[i])
                    .collect();
                members.sort_by(f32::total_cmp);
                members[(members.len() - 1) / 2]
            };
            whole_grid_shape(&mut whole, &water, level, w, h, 2.0);
            let settled = settle(&mut heights, &water, w as u32, h as u32, 2.0);
            assert_eq!(settled.bodies.len(), 1, "one body round ({cx}, {cy})");
            assert_eq!(heights, whole, "the body round ({cx}, {cy})");
        }
    }

    /// The shaping this module did before #1724, word for word (`git show
    /// 5d3a6ff:crates/geodata/src/water.rs`): one plane at `level` over the
    /// whole grid, every pixel not `carved` lifted over it. Where no land lies
    /// under the crest it is what [`shape`] must give a single body.
    fn whole_grid_shape(
        heights: &mut [f32],
        carved: &[bool],
        level: f32,
        w: usize,
        h: usize,
        pixel_m: f32,
    ) {
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

    #[test]
    #[should_panic(expected = "must both be 3 x 3")]
    fn grids_of_another_size_are_refused() {
        let mut heights = vec![30.0; 9];
        settle(&mut heights, &[true; 8], 3, 3, 2.0);
    }
}
