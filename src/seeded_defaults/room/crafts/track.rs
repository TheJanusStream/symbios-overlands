//! The car's track (#1782): a loop painted [`WIDTH`] wide - two
//! [`STRAIGHT`]s joined by half circles [`BEND`] round - on ground that
//! rises and falls gently all the way round, beside the view, and a stack
//! of tyres outside each bend.

use super::{
    BESIDE_THE_VIEW, Ground, MARGIN, Place, Siting, Stands, Wiring, facing, placed, turned,
};
use crate::pds::{Fp, Fp2, SovereignTrack};
use crate::world_builder::compile::scatter::TYRES;

/// How wide the track is painted, how long each straight and how far round
/// each bend from its middle to the track's (m).
pub const WIDTH: f32 = 6.0;
pub const STRAIGHT: f32 = 48.0;
pub const BEND: f32 = 15.0;
/// How many pieces each bend is drawn in.
const BEND_PIECES: usize = 8;
/// How far its middle stands from the landing at the most (m).
const FARTHEST: f32 = 220.0;
/// The bearings its straights may lie at (deg, from +X).
const TURNS: [f32; 4] = [0.0, 45.0, 90.0, 135.0];
/// How steeply its ground may rise from one point of its loop to the next,
/// and how far round the whole loop - on the probe, which reads the full
/// map's slopes about half as steep (#1782).
const SLOPE: f32 = 0.07;
const RISE: f32 = 3.5;
/// How far outside a bend's apex its tyres stand, off the track's edge (m).
const TYRES_OFF: f32 = 1.5;
/// How far round its tyres the scatters keep off (m).
const TYRES_KEEP: f32 = 2.0;

/// A track: its middle, and which way its straights run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub middle: [f32; 2],
    pub run: [f32; 2],
}

impl Track {
    /// Its loop, closed - its last point its first - in world XZ.
    pub fn loop_points(&self) -> Vec<[f32; 2]> {
        let (u, n) = (self.run, [-self.run[1], self.run[0]]);
        let at = |x: f32, z: f32| {
            [
                self.middle[0] + u[0] * x + n[0] * z,
                self.middle[1] + u[1] * x + n[1] * z,
            ]
        };
        let half = STRAIGHT * 0.5;
        let mut points = vec![at(-half, -BEND)];
        // Out along one straight, round the far bend, back along the other
        // and round the near one: each bend's points libm's (#1132).
        for (centre, from) in [
            (half, -std::f32::consts::FRAC_PI_2),
            (-half, std::f32::consts::FRAC_PI_2),
        ] {
            for k in 0..=BEND_PIECES {
                let a = from + std::f32::consts::PI * k as f32 / BEND_PIECES as f32;
                points.push(at(centre + BEND * libm::cosf(a), BEND * libm::sinf(a)));
            }
        }
        // The near bend ends where the loop began: close it there exactly.
        let first = points[0];
        if let Some(last) = points.last_mut() {
            *last = first;
        }
        points
    }

    /// Whether the ground under its loop takes it: dry all round and its
    /// infield, gentle from point to point and round the whole loop.
    fn fits(&self, siting: &Siting) -> bool {
        let points = self.loop_points();
        if !siting.dry(self.middle) || !points.iter().all(|&p| siting.dry(p)) {
            return false;
        }
        let heights: Vec<f32> = points
            .iter()
            .map(|&p| siting.probe.height_between(p))
            .collect();
        let gentle = points.windows(2).zip(heights.windows(2)).all(|(p, h)| {
            let run = ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt();
            (h[1] - h[0]).abs() <= SLOPE * run.max(1.0)
        });
        let (low, high) = heights
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        gentle && high - low <= RISE
    }

    /// The two bends' apexes, outside them, and which way each faces in.
    fn tyre_spots(&self) -> [([f32; 2], [f32; 2]); 2] {
        let reach = STRAIGHT * 0.5 + BEND + WIDTH * 0.5 + TYRES_OFF;
        let u = self.run;
        [1.0, -1.0].map(|side| {
            (
                [
                    self.middle[0] + u[0] * reach * side,
                    self.middle[1] + u[1] * reach * side,
                ],
                [-u[0] * side, -u[1] * side],
            )
        })
    }
}

impl Place for Track {
    /// The middle nearest the landing, at the first bearing its ground takes
    /// (see the module docs), clear of what is kept and beside the view.
    fn site(siting: &Siting) -> Option<Self> {
        let reach = STRAIGHT * 0.5 + BEND + WIDTH;
        siting
            .lattice(2.0, FARTHEST, reach)
            .into_iter()
            .filter(|&(middle, _)| siting.off_the_view(middle) <= BESIDE_THE_VIEW)
            .find_map(|(middle, _)| {
                TURNS.into_iter().find_map(|turn| {
                    let track = Track {
                        middle,
                        run: turned([1.0, 0.0], turn),
                    };
                    (siting.kept.clear(&track.ground()[0], MARGIN) && track.fits(siting))
                        .then_some(track)
                })
            })
    }

    /// Its loop and its tyres: a way round its straights' line as wide as
    /// its bends reach.
    fn ground(&self) -> Vec<Ground> {
        let half = STRAIGHT * 0.5;
        let end = |side: f32| {
            [
                self.middle[0] + self.run[0] * half * side,
                self.middle[1] + self.run[1] * half * side,
            ]
        };
        vec![Ground {
            a: end(-1.0),
            b: end(1.0),
            r: BEND + WIDTH * 0.5 + TYRES_OFF + 1.0,
        }]
    }

    /// The loop painted on the ground, and the tyres outside its bends.
    fn wire(&self, wiring: &mut Wiring) {
        wiring.tracks.push(SovereignTrack {
            line: self.loop_points().into_iter().map(Fp2).collect(),
            width: Fp(WIDTH),
        });
        wiring.generators.push((
            TYRES.to_string(),
            crate::catalogue::items::crafts::tyre_stack::tyre_stack(),
        ));
        for (at, inward) in self.tyre_spots() {
            wiring.placements.push(placed(
                TYRES,
                at,
                0.0,
                facing(inward),
                Stands::Snapped(TYRES_KEEP),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loop closes on itself, its straights STRAIGHT long and its bends
    /// BEND round its bends' middles, and its tyres stand outside its bends.
    #[test]
    fn the_loop_closes_round_its_bends() {
        let track = Track {
            middle: [100.0, -40.0],
            run: turned([1.0, 0.0], 45.0),
        };
        let points = track.loop_points();
        assert_eq!(points.first(), points.last());
        assert_eq!(points.len(), 2 * (BEND_PIECES + 1) + 1);
        let length: f32 = points
            .windows(2)
            .map(|p| ((p[1][0] - p[0][0]).powi(2) + (p[1][1] - p[0][1]).powi(2)).sqrt())
            .sum();
        let round = 2.0 * STRAIGHT + 2.0 * std::f32::consts::PI * BEND;
        assert!((length - round).abs() < round * 0.01, "{length} m round");
        for (at, _) in track.tyre_spots() {
            let off = ((at[0] - 100.0).powi(2) + (at[1] + 40.0).powi(2)).sqrt();
            assert!((off - (STRAIGHT * 0.5 + BEND + WIDTH * 0.5 + TYRES_OFF)).abs() < 1e-3);
        }
    }
}
