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

/// `paths`' lines as world XZ points.
pub(crate) fn lines(paths: &crate::pds::SovereignPathsConfig) -> Vec<Vec<Vec2>> {
    paths
        .lines
        .iter()
        .map(|line| line.iter().map(|p| Vec2::from_array(p.0)).collect())
        .collect()
}

/// Paint `paths` into `weights`, one RGBA weight per heightmap vertex of an
/// `n` by `n` grid `cell` metres apart centred on the origin: each vertex's
/// weights shifted toward the paths' ground layer by how far inside a path
/// it stands - the paths' share of it within half their width, fading over
/// [`FEATHER`] beyond.
pub(crate) fn paint(
    weights: &mut [[u8; 4]],
    n: usize,
    cell: f32,
    paths: &crate::pds::SovereignPathsConfig,
) {
    let layer = usize::from(paths.layer);
    if n < 2 || cell <= 0.0 || layer > 3 || paths.is_empty() {
        return;
    }
    let lines = lines(paths);
    let width = paths.width.0;
    let half = (n - 1) as f32 * cell * 0.5;
    let (inner, outer) = (width * 0.5, width * 0.5 + FEATHER);
    let mut share = vec![0.0f32; n * n];
    for line in &lines {
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
        };
        paint(&mut weights, n, 2.0, &paths);
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
        );
        assert!(untouched.iter().all(|w| *w == [255, 0, 0, 0]));
    }
}
