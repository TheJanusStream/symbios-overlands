//! Sanitiser for [`SovereignPathsConfig`] (#1779): a world's paths and its
//! tracks (#1782) kept to what the ground can paint and a record can carry,
//! and rounded onto the wire's grid as a peer reads them back.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::paths::{SovereignPathsConfig, SovereignTrack};
use crate::pds::types::{Fp, Fp2};

impl Sanitize for SovereignPathsConfig {
    fn sanitize(&mut self) {
        self.sanitize_within(limits::MAX_PATH_REACH);
    }
}

impl SovereignPathsConfig {
    /// [`Sanitize`] the paths of a terrain `reach` metres from its middle to
    /// its edge: their points kept on it.
    pub(crate) fn sanitize_within(&mut self, reach: f32) {
        let defaults = SovereignPathsConfig::default();
        let reach = if reach.is_finite() {
            reach.clamp(0.0, limits::MAX_PATH_REACH)
        } else {
            limits::MAX_PATH_REACH
        };
        self.layer = self.layer.min(3);
        self.share = Fp(clamp_finite(self.share.0, 0.0, 1.0, defaults.share.0)).on_wire();
        self.width = Fp(clamp_finite(
            self.width.0,
            limits::MIN_PATH_WIDTH,
            limits::MAX_PATH_WIDTH,
            defaults.width.0,
        ))
        .on_wire();
        // Points that are no number are dropped and the rest kept on the
        // terrain; a line left with fewer than two goes; and at most so many
        // lines, so many points in all and so many metres in all - what the
        // painting and the scatters' keep-outs cost grows with them - so a
        // hostile record cannot run either away.
        let mut budget = (limits::MAX_PATH_POINTS, limits::MAX_PATH_LENGTH);
        self.lines.truncate(limits::MAX_PATH_LINES);
        for line in &mut self.lines {
            keep_line(line, reach, &mut budget);
        }
        self.lines.retain(|line| line.len() >= 2);
        // The tracks (#1782) alike, on a budget of their own, each its own
        // width within a track's bounds.
        let mut budget = (limits::MAX_TRACK_POINTS, limits::MAX_TRACK_LENGTH);
        self.tracks.truncate(limits::MAX_TRACKS);
        for track in &mut self.tracks {
            track.width = Fp(clamp_finite(
                track.width.0,
                limits::MIN_TRACK_WIDTH,
                limits::MAX_TRACK_WIDTH,
                SovereignTrack::default().width.0,
            ))
            .on_wire();
            keep_line(&mut track.line, reach, &mut budget);
        }
        self.tracks.retain(|track| track.line.len() >= 2);
    }
}

/// `line`'s points that are numbers, kept within `reach` of the middle on
/// the wire's grid, as many as `budget`'s points and metres left allow -
/// spent by what is kept.
fn keep_line(line: &mut Vec<Fp2>, reach: f32, budget: &mut (usize, f32)) {
    let (points, length) = budget;
    line.retain(|p| p.0.iter().all(|v| v.is_finite()));
    for p in line.iter_mut() {
        *p = Fp2(p.0.map(|v| v.clamp(-reach, reach))).on_wire();
    }
    let mut kept = 0;
    for (k, p) in line.iter().enumerate() {
        if kept == *points {
            break;
        }
        if k > 0 {
            let q = line[k - 1].0;
            let step = ((p.0[0] - q[0]).powi(2) + (p.0[1] - q[1]).powi(2)).sqrt();
            if step > *length {
                break;
            }
            *length -= step;
        }
        kept += 1;
    }
    line.truncate(kept);
    *points -= kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hostile or mistaken group is kept paintable: the layer one of the
    /// four, the share and the width within bounds, points off the world
    /// pulled in and NaN ones dropped, short lines gone - all on the wire's
    /// grid, so a sanitised group equals itself read back.
    #[test]
    fn a_path_group_is_kept_paintable() {
        let mut paths = SovereignPathsConfig {
            lines: vec![
                vec![
                    Fp2([0.123_456, 1.0]),
                    Fp2([f32::NAN, 2.0]),
                    Fp2([1.0e9, -3.0]),
                ],
                vec![Fp2([5.0, 5.0])],
            ],
            layer: 9,
            share: Fp(f32::NAN),
            width: Fp(100.0),
            ..Default::default()
        };
        // On a terrain 500 m from its middle to its edge.
        paths.sanitize_within(500.0);
        assert_eq!(paths.layer, 3);
        assert_eq!(paths.share, SovereignPathsConfig::default().share);
        assert_eq!(paths.width, Fp(limits::MAX_PATH_WIDTH));
        assert_eq!(
            paths.lines,
            vec![vec![Fp2([0.1235, 1.0]), Fp2([500.0, -3.0])]]
        );
        let json = serde_json::to_string(&paths).expect("writes");
        let back: SovereignPathsConfig = serde_json::from_str(&json).expect("reads");
        assert_eq!(back, paths, "a sanitised group is what a peer reads");
    }

    /// The points and the metres are bounded in all, whatever the lines
    /// they are split over, and a terrain keeps its paths on itself.
    #[test]
    fn the_length_is_bounded_and_the_points_kept_on_the_terrain() {
        let zigzag: Vec<Fp2> = (0..32)
            .map(|k| {
                Fp2(if k % 2 == 0 {
                    [-1.0e4, -1.0e4]
                } else {
                    [1.0e4, 1.0e4]
                })
            })
            .collect();
        let mut paths = SovereignPathsConfig {
            lines: vec![zigzag; 64],
            ..Default::default()
        };
        paths.sanitize_within(500.0);
        let length: f32 = paths
            .lines
            .iter()
            .flat_map(|line| line.windows(2))
            .map(|w| (w[1].0[0] - w[0].0[0]).hypot(w[1].0[1] - w[0].0[1]))
            .sum();
        assert!(length <= limits::MAX_PATH_LENGTH, "{length} m");
        assert!(
            paths
                .lines
                .iter()
                .flatten()
                .all(|p| p.0.iter().all(|v| v.abs() <= 500.0)),
            "a point off the terrain"
        );
        let mut terrain = crate::pds::SovereignTerrainConfig {
            grid_size: 101,
            cell_scale: Fp(2.0),
            ..Default::default()
        };
        terrain.paths.lines = vec![vec![Fp2([0.0, 0.0]), Fp2([900.0, -900.0])]];
        terrain.sanitize();
        assert_eq!(terrain.paths.lines[0][1], Fp2([100.0, -100.0]));
    }

    /// The points are bounded in all, whatever the lines they are split
    /// over.
    #[test]
    fn the_points_are_bounded_in_all() {
        let line = vec![Fp2([0.0, 0.0]); limits::MAX_PATH_POINTS];
        let mut paths = SovereignPathsConfig {
            lines: vec![line.clone(), line],
            ..Default::default()
        };
        paths.sanitize();
        let points: usize = paths.lines.iter().map(Vec::len).sum();
        assert_eq!(points, limits::MAX_PATH_POINTS);
    }

    /// #1782: hostile tracks are kept paintable as the paths are - each
    /// width within a track's bounds, points that are no number dropped and
    /// the rest pulled onto the terrain on the wire's grid, a track left
    /// short gone, and no more of them than a world may carry.
    #[test]
    fn tracks_are_kept_paintable() {
        let track = |line: Vec<Fp2>, width: f32| SovereignTrack {
            line,
            width: Fp(width),
        };
        let mut paths = SovereignPathsConfig {
            tracks: vec![
                track(
                    vec![
                        Fp2([0.123_456, 1.0]),
                        Fp2([f32::NAN, 2.0]),
                        Fp2([1.0e9, 3.0]),
                    ],
                    500.0,
                ),
                track(vec![Fp2([5.0, 5.0])], 6.0),
                track(vec![Fp2([0.0, 0.0]), Fp2([0.0, 10.0])], f32::NAN),
            ],
            ..Default::default()
        };
        paths
            .tracks
            .extend(vec![paths.tracks[2].clone(); limits::MAX_TRACKS]);
        paths.sanitize_within(500.0);
        assert_eq!(
            paths.tracks.len(),
            limits::MAX_TRACKS - 1,
            "the short one gone"
        );
        assert_eq!(paths.tracks[0].width, Fp(limits::MAX_TRACK_WIDTH));
        assert_eq!(
            paths.tracks[0].line,
            vec![Fp2([0.1235, 1.0]), Fp2([500.0, 3.0])]
        );
        assert_eq!(paths.tracks[1].width, SovereignTrack::default().width);
        let json = serde_json::to_string(&paths).expect("writes");
        let back: SovereignPathsConfig = serde_json::from_str(&json).expect("reads");
        assert_eq!(back, paths, "sanitised tracks are what a peer reads");
    }
}
