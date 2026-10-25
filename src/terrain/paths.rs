//! Paths painted on the ground (#1779): the splat weights a world's
//! footways ([`crate::pds::SovereignPathsConfig`]) shift toward the ground
//! layer that paints them, which the ground's shader and the grass both
//! read - no texture of their own. The ground under them grows no grass
//! (`ground_cover`), and no scatter grows on them
//! (`world_builder::compile::scatter`).

use bevy::math::Vec2;

/// How far past a path's edge its paint fades out (m).
const FEATHER: f32 = 1.5;

/// The longest piece of a path painted at once (m).
const PIECE: f32 = 4.0;

/// How much of a track may lie under water before the ground leaves it
/// unpainted (#1782): a seeded world sites its airstrip and its car's track
/// on a probe that misses hollows the sea floods.
const TRACK_WET_SHARE: f32 = 0.1;

/// How far apart a track's ground is read for its water (m).
const TRACK_READ: f32 = 4.0;

/// The ground a track is read against: the heightmap, and the room's water
/// as the compile draws it.
pub(crate) type Ground<'a> = (
    &'a bevy_symbios_ground::HeightMap,
    &'a crate::water::bodies::RoomWater,
);

/// Whether `ground` paints a track along `line`, `width` wide (#1782): its
/// middle and its edges, read [`TRACK_READ`] apart, no more than
/// [`TRACK_WET_SHARE`] of them under water.
pub(crate) fn drawn((hm, water): Ground<'_>, line: &[Vec2], width: f32) -> bool {
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let half = extent * 0.5;
    let wet = |p: Vec2| {
        water.level_at(p.x, p.y).is_some_and(|level| {
            hm.get_height_at(
                (p.x + half).clamp(0.0, extent),
                (p.y + half).clamp(0.0, extent),
            ) < level
        })
    };
    let (mut read, mut under) = (0usize, 0usize);
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let Some(run) = (b - a).try_normalize() else {
            continue;
        };
        let side = Vec2::new(-run.y, run.x) * (width * 0.5);
        let steps = ((b - a).length() / TRACK_READ).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let p = a + (b - a) * (k as f32 / steps as f32);
            for q in [p, p + side, p - side] {
                read += 1;
                under += usize::from(wet(q));
            }
        }
    }
    read == 0 || (under as f32) <= TRACK_WET_SHARE * read as f32
}

/// `paths`' ways - its paths, then its tracks (#1782) - as world XZ points,
/// each with its width: on `ground`, only the tracks it paints
/// ([`drawn`]); every one where there is no ground to ask.
pub(crate) fn ways(
    paths: &crate::pds::SovereignPathsConfig,
    ground: Option<Ground<'_>>,
) -> Vec<(Vec<Vec2>, f32)> {
    let footways = paths.lines.len();
    paths
        .ways()
        .map(|(line, width)| {
            let line: Vec<Vec2> = line.iter().map(|p| Vec2::from_array(p.0)).collect();
            (line, width)
        })
        .enumerate()
        .filter(|(k, (line, width))| {
            *k < footways || ground.is_none_or(|ground| drawn(ground, line, *width))
        })
        .map(|(_, way)| way)
        .collect()
}

/// Paint `paths` into `weights`, one RGBA weight per heightmap vertex of an
/// `n` by `n` grid `cell` metres apart centred on the origin: each vertex's
/// weights shifted toward the paths' ground layer by how far inside a way
/// it stands - the paths' share of it within half the way's width (a
/// path's, or a track's own), fading over [`FEATHER`] beyond. On `ground`, a
/// track it mostly floods is left unpainted ([`drawn`]).
pub(crate) fn paint(
    weights: &mut [[u8; 4]],
    n: usize,
    cell: f32,
    paths: &crate::pds::SovereignPathsConfig,
    ground: Option<Ground<'_>>,
) {
    let layer = usize::from(paths.layer);
    if n < 2 || cell <= 0.0 || layer > 3 || paths.is_empty() {
        return;
    }
    let half = (n - 1) as f32 * cell * 0.5;
    let mut share = vec![0.0f32; n * n];
    for (line, width) in &ways(paths, ground) {
        let (inner, outer) = (width * 0.5, width * 0.5 + FEATHER);
        for pair in line.windows(2) {
            // In pieces no longer than PIECE, so each reads only the
            // vertices round it: the work grows with a path's length, not
            // with the square of a long straight's.
            let (a, b) = (pair[0], pair[1]);
            let pieces = ((b - a).length() / PIECE).ceil().max(1.0) as usize;
            for k in 0..pieces {
                let p = a + (b - a) * (k as f32 / pieces as f32);
                let q = a + (b - a) * ((k + 1) as f32 / pieces as f32);
                stroke(
                    &mut share,
                    n,
                    cell,
                    half,
                    (p, q),
                    (inner, outer),
                    paths.share.0,
                );
            }
        }
    }
    for (texel, s) in weights.iter_mut().zip(share) {
        if s <= 0.0 {
            continue;
        }
        for (c, w) in texel.iter_mut().enumerate() {
            let target = if c == layer { 255.0 } else { 0.0 };
            *w = (f32::from(*w) * (1.0 - s) + target * s).round() as u8;
        }
    }
}

/// Paint the piece `a`-`b` of a path into `share`, an `n` by `n` grid of
/// vertices `cell` metres apart reaching `half` from the middle: each
/// vertex within `outer` of it raised to `full` - its share at the path's
/// middle - wholly within `inner`, fading between.
fn stroke(
    share: &mut [f32],
    n: usize,
    cell: f32,
    half: f32,
    (a, b): (Vec2, Vec2),
    (inner, outer): (f32, f32),
    full: f32,
) {
    let last = (n - 1) as f32;
    let lo = ((a.min(b) - Vec2::splat(outer) + Vec2::splat(half)) / cell).floor();
    let hi = ((a.max(b) + Vec2::splat(outer) + Vec2::splat(half)) / cell).ceil();
    let (i0, j0) = (
        lo.x.clamp(0.0, last) as usize,
        lo.y.clamp(0.0, last) as usize,
    );
    let (i1, j1) = (
        hi.x.clamp(0.0, last) as usize,
        hi.y.clamp(0.0, last) as usize,
    );
    let along = b - a;
    let length2 = along.length_squared();
    for j in j0..=j1 {
        for i in i0..=i1 {
            let at = Vec2::new(i as f32 * cell - half, j as f32 * cell - half);
            let t = if length2 > 0.0 {
                ((at - a).dot(along) / length2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let off = (at - (a + along * t)).length();
            let s = (1.0 - ((off - inner) / (outer - inner)).clamp(0.0, 1.0)) * full;
            let k = j * n + i;
            share[k] = share[k].max(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::{Fp, Fp2};

    /// A path shifts the weights under it toward its layer - its share at
    /// its middle, fading out past its edge - and leaves the ground away
    /// from it as it was.
    #[test]
    fn a_path_shifts_the_weights_under_it_toward_its_layer() {
        // 21 vertices 2 m apart: -20 to 20 m, all grass.
        let n = 21;
        let mut weights = vec![[255u8, 0, 0, 0]; n * n];
        let paths = crate::pds::SovereignPathsConfig {
            lines: vec![vec![Fp2([-16.0, 0.0]), Fp2([16.0, 0.0])]],
            layer: 1,
            share: Fp(0.8),
            width: Fp(2.0),
            ..Default::default()
        };
        paint(&mut weights, n, 2.0, &paths, None);
        let at = |x: usize, z: usize| weights[z * n + x];
        // The middle row, z = 0: 80 % dirt.
        assert_eq!(at(10, 10), [51, 204, 0, 0]);
        // Two metres off, inside the feather: some dirt; past it none.
        let edge = at(10, 11);
        assert!(edge[1] > 0 && edge[1] < 204, "{edge:?}");
        assert_eq!(at(10, 12), [255, 0, 0, 0]);
        // Past the path's ends nothing moves.
        assert_eq!(at(0, 10), [255, 0, 0, 0]);
        // No paths, no change.
        let mut untouched = vec![[255u8, 0, 0, 0]; n * n];
        paint(
            &mut untouched,
            n,
            2.0,
            &crate::pds::SovereignPathsConfig::default(),
            None,
        );
        assert!(untouched.iter().all(|w| *w == [255, 0, 0, 0]));
    }

    /// A track (#1782) paints at its own width, a path's beside it at the
    /// paths': eight metres off the track's line is still inside it.
    #[test]
    fn a_track_paints_at_its_own_width() {
        let n = 21;
        let mut weights = vec![[255u8, 0, 0, 0]; n * n];
        let paths = crate::pds::SovereignPathsConfig {
            lines: vec![vec![Fp2([-16.0, -16.0]), Fp2([16.0, -16.0])]],
            layer: 1,
            share: Fp(0.8),
            width: Fp(2.0),
            tracks: vec![crate::pds::SovereignTrack {
                line: vec![Fp2([-16.0, 4.0]), Fp2([16.0, 4.0])],
                width: Fp(18.0),
            }],
        };
        paint(&mut weights, n, 2.0, &paths, None);
        let at = |x: usize, z: usize| weights[z * n + x];
        // The track's line, z = 4, and eight metres off it, z = 12.
        assert_eq!(at(10, 12), [51, 204, 0, 0]);
        assert_eq!(at(10, 16), [51, 204, 0, 0]);
        // The path, z = -16: its own two metres.
        assert_eq!(at(10, 2), [51, 204, 0, 0]);
        assert_eq!(at(10, 4), [255, 0, 0, 0]);
    }

    /// #1782: the ground paints a track it keeps dry and leaves one it
    /// mostly floods - a seeded airstrip the probe sited on a hollow the sea
    /// fills - and paints every path whatever the water.
    #[test]
    fn a_track_the_water_floods_is_not_painted() {
        // 129 vertices a metre apart, -64 to 64 m: dry east of x = 0, the
        // sea west of it.
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                hm.set(x, z, if x < 64 { -2.0 } else { 2.0 });
            }
        }
        let sea = crate::water::bodies::RoomWater::sea_alone(Some(0.0));
        let line = |x: f32| vec![Vec2::new(x, -30.0), Vec2::new(x, 30.0)];
        assert!(drawn((&hm, &sea), &line(30.0), 12.0), "on dry ground");
        assert!(!drawn((&hm, &sea), &line(-30.0), 12.0), "in the sea");
        let track = |x: f32| crate::pds::SovereignTrack {
            line: line(x).into_iter().map(|p| Fp2(p.to_array())).collect(),
            width: Fp(12.0),
        };
        let paths = crate::pds::SovereignPathsConfig {
            lines: vec![vec![Fp2([-30.0, 0.0]), Fp2([-20.0, 0.0])]],
            tracks: vec![track(30.0), track(-30.0)],
            ..Default::default()
        };
        let on = ways(&paths, Some((&hm, &sea)));
        let widths: Vec<f32> = on.iter().map(|(_, width)| *width).collect();
        assert_eq!(
            widths,
            vec![2.5, 12.0],
            "the path under water stays, the track does not"
        );
        assert_eq!(on[1].0[0].x, 30.0);
        assert_eq!(
            ways(&paths, None).len(),
            3,
            "with no ground to ask, every way"
        );
    }
}
