//! The airstrip (#1782): a straight [`LENGTH`] of ground painted
//! [`WIDTH`] wide along the world's wind, where an airplane takes off into
//! it and lands, and a windsock beside its upwind end that shows the wind
//! it runs along.
//!
//! The wind's own bearing first, then a quarter-turn's sixths either side
//! of it (`TURNS`), so a world whose ground nowhere runs flat along its
//! wind lays one across it a little; never in front of the view. An
//! airplane needs a short run (`pds::avatar::locomotion::airplane`: about
//! 8 m to its stall speed): the length is for its landing, and for the look
//! of a strip.

use super::{BESIDE_THE_VIEW, Ground, MARGIN, Place, Siting, Stands, Wiring, placed, turned, unit};
use crate::pds::{Fp, Fp2, SovereignTrack};
use crate::world_builder::compile::scatter::WINDSOCK;

/// How long the strip is, and how wide (m).
pub const LENGTH: f32 = 80.0;
pub const WIDTH: f32 = 12.0;
/// How far its middle stands from the landing at the most (m).
const FARTHEST: f32 = 220.0;
/// The turns off the wind it may lie at, in the order each is tried (deg).
const TURNS: [f32; 5] = [0.0, 15.0, -15.0, 30.0, -30.0];
/// How far apart its ground is read along it (m).
const READ: f32 = 8.0;
/// How steeply its ground may rise along it from one reading to the next,
/// how far end to end, and how steeply across it - on the probe, which
/// reads a strip's slopes about half as steep as the full map draws them
/// (#1782's critic: along p50 10 % on ground the probe read under 5 %).
const ALONG: f32 = 0.03;
const RISE: f32 = 2.0;
const ACROSS: f32 = 0.04;
/// How far past each end it keeps clear (m): an approach and a climb out.
const OVERRUN: f32 = 15.0;
/// Where the windsock stands: this far in from the upwind end, and off the
/// strip's edge (m).
const SOCK_IN: f32 = 12.0;
const SOCK_OFF: f32 = 5.0;
/// How far round the windsock the scatters keep off (m).
const SOCK_KEEP: f32 = 2.0;

/// An airstrip: its two ends, upwind first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub upwind: [f32; 2],
    pub downwind: [f32; 2],
}

impl Strip {
    /// Which way it runs, from its upwind end.
    pub(super) fn run(&self) -> [f32; 2] {
        unit([
            self.downwind[0] - self.upwind[0],
            self.downwind[1] - self.upwind[1],
        ])
        .unwrap_or([1.0, 0.0])
    }

    /// Whether the ground under a strip from `a` to `b` takes it: dry along
    /// its middle and its edges, gentle along and across it.
    fn fits(siting: &Siting, a: [f32; 2], b: [f32; 2]) -> bool {
        let probe = siting.probe;
        let Some(run) = unit([b[0] - a[0], b[1] - a[1]]) else {
            return false;
        };
        let across = [-run[1] * WIDTH * 0.5, run[0] * WIDTH * 0.5];
        let readings = (LENGTH / READ).round() as usize;
        let mut heights = Vec::with_capacity(readings + 1);
        for k in 0..=readings {
            let s = LENGTH * k as f32 / readings as f32;
            let mid = [a[0] + run[0] * s, a[1] + run[1] * s];
            let left = [mid[0] + across[0], mid[1] + across[1]];
            let right = [mid[0] - across[0], mid[1] - across[1]];
            if !(siting.dry(mid) && siting.dry(left) && siting.dry(right)) {
                return false;
            }
            let (l, r) = (probe.height_between(left), probe.height_between(right));
            if (l - r).abs() > ACROSS * WIDTH {
                return false;
            }
            heights.push(probe.height_between(mid));
        }
        let step = LENGTH / readings as f32;
        let along = heights
            .windows(2)
            .all(|pair| (pair[1] - pair[0]).abs() <= ALONG * step);
        let (low, high) = heights
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
        along && high - low <= RISE
    }
}

impl Place for Strip {
    /// The middle nearest the landing, at the first turn off the wind its
    /// ground takes (see the module docs), clear of what is kept and of the
    /// view's front.
    fn site(siting: &Siting) -> Option<Self> {
        // Upwind of its middle, toward the wind; an airplane takes off into
        // it.
        let into = unit([-siting.wind[0], -siting.wind[1]])?;
        siting
            .lattice(2.0, FARTHEST, LENGTH * 0.5)
            .into_iter()
            .filter(|&(mid, _)| siting.off_the_view(mid) <= BESIDE_THE_VIEW)
            .find_map(|(mid, _)| {
                TURNS.into_iter().find_map(|turn| {
                    let run = turned(into, turn);
                    let half = [run[0] * LENGTH * 0.5, run[1] * LENGTH * 0.5];
                    let strip = Strip {
                        upwind: [mid[0] + half[0], mid[1] + half[1]],
                        downwind: [mid[0] - half[0], mid[1] - half[1]],
                    };
                    (siting.kept.clear(&strip.ground()[0], MARGIN)
                        && Strip::fits(siting, strip.upwind, strip.downwind))
                    .then_some(strip)
                })
            })
    }

    /// The strip and its overruns, its windsock beside it.
    fn ground(&self) -> Vec<Ground> {
        let run = self.run();
        let past = |at: [f32; 2], s: f32| [at[0] + run[0] * s, at[1] + run[1] * s];
        vec![Ground {
            a: past(self.upwind, -OVERRUN),
            b: past(self.downwind, OVERRUN),
            r: WIDTH * 0.5 + SOCK_OFF + 1.0,
        }]
    }

    /// The strip painted on the ground, and its windsock on the ground
    /// beside its upwind end.
    fn wire(&self, wiring: &mut Wiring) {
        wiring.tracks.push(SovereignTrack {
            line: vec![Fp2(self.upwind), Fp2(self.downwind)],
            width: Fp(WIDTH),
        });
        let run = self.run();
        let off = WIDTH * 0.5 + SOCK_OFF;
        let sock = [
            self.upwind[0] + run[0] * SOCK_IN - run[1] * off,
            self.upwind[1] + run[1] * SOCK_IN + run[0] * off,
        ];
        wiring.generators.push((
            WINDSOCK.to_string(),
            crate::catalogue::items::crafts::windsock::windsock(),
        ));
        wiring
            .placements
            .push(placed(WINDSOCK, sock, 0.0, 0.0, Stands::Snapped(SOCK_KEEP)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The windsock stands beside the strip, off its edge, near its upwind
    /// end - whichever way the strip runs.
    #[test]
    fn the_windsock_stands_beside_the_upwind_end() {
        for run in [[1.0, 0.0], [0.0, -1.0], [0.6, 0.8]] {
            let strip = Strip {
                upwind: [10.0, 20.0],
                downwind: [10.0 + run[0] * LENGTH, 20.0 + run[1] * LENGTH],
            };
            let mut wiring = Wiring::default();
            strip.wire(&mut wiring);
            let crate::pds::Placement::Absolute { transform, .. } = &wiring.placements[0] else {
                panic!("an absolute placement");
            };
            let [x, _, z] = transform.translation.0;
            let (dx, dz) = (x - 10.0, z - 20.0);
            let along = dx * run[0] + dz * run[1];
            let off = (-dx * run[1] + dz * run[0]).abs();
            assert!((along - SOCK_IN).abs() < 1e-3, "{run:?}: {along}");
            assert!(
                (off - (WIDTH * 0.5 + SOCK_OFF)).abs() < 1e-3,
                "{run:?}: {off}"
            );
            assert_eq!(wiring.tracks[0].width, Fp(WIDTH));
        }
    }
}
