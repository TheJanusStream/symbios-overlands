//! A seeded settlement's paths lined (#1780): the props a walker passes,
//! stood along the paths it routed (#1779) - a lamp, a lantern, a bench or a
//! sign a stride apart on alternate sides, or a fence run down both sides of
//! the way in. A world lines its paths with one prop of its theme's, as that
//! prop says it lines a path ([`Lining`]), drawn from a stream of its own so
//! that no other deriver's draws move.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::paths::{distance2, ground, off_segment, segments_apart};
use super::settlement::{
    SITE_SLOPE, SettlementMember, SettlementPlan, effective_theme, facing, ground_radius,
};
use super::siting::{FootprintGround, TerrainProbe};
use crate::catalogue::{CatalogueEntry, Lining, StructureRole, entries_for_room};
use crate::seeded_defaults::scene::{SceneCharacter, pick};

/// Sub-stream salt distinct from every sibling room deriver.
const LINING_STREAM_SALT: u64 = 0x1780_11E5_1780_11E5;

/// At most this many props line a world's paths.
const MAX_LINING: usize = 12;

/// How far down a path its first post stands at the most (m): one of a long
/// stride still stands by a short path.
const POST_START: f32 = 10.0;

/// How far down the way in a run of fence starts (m): past the gate and the
/// monument either side of it.
const RUN_START: f32 = 10.0;

/// How far past a path's edge a run of fence stands (m).
const RUN_OFFSET: f32 = 0.5;

/// The gap between two sections of a run (m), so that their end posts never
/// meet face to face.
const RUN_GAP: f32 = 0.1;

/// How far apart along a section of fence what it keeps off is read (m).
const RUN_READ: f32 = 0.5;

/// How much higher one end of a section of fence may stand than the other
/// (m): a section is drawn straight, so its low rail stays over the ground
/// at the high end.
const RUN_RELIEF: f32 = 0.3;

/// Line the settlement's paths (#1780) with props of one of the lining
/// entries of the theme `plan` was drawn in, the world's `seed` drawing it:
/// along `lines`, the paths as routed - the way in first - `width` metres
/// wide. Each stands clear of every path, of the ground of every building
/// and prop of `plan` and of `forecourt` (the gate and the monument, as
/// `(spot, radius)`), and of the others, on ground it stands on
/// ([`stands`]); at most [`MAX_LINING`] join the first cluster's props.
pub fn line_paths(
    probe: &TerrainProbe,
    plan: &mut SettlementPlan,
    scene: &SceneCharacter,
    seed: u64,
    lines: &[Vec<[f32; 2]>],
    width: f32,
    forecourt: &[([f32; 2], f32)],
) {
    let linings: Vec<(&'static dyn CatalogueEntry, Lining)> = entries_for_room(
        effective_theme(scene.theme),
        StructureRole::Prop,
        scene.prosperity_tier(),
        scene.escalation_tier(),
    )
    .filter_map(|entry| entry.lines_paths().map(|lining| (entry, lining)))
    .collect();
    if linings.is_empty() {
        return;
    }
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ LINING_STREAM_SALT);
    let (entry, lining) = pick(&linings, &mut rng);
    let mut blocked = forecourt.to_vec();
    for cluster in &plan.clusters {
        let members = cluster.landmark.iter().chain(&cluster.secondaries);
        blocked.extend(members.chain(&cluster.props).map(ground));
    }
    let ways = Ways {
        lines,
        half: width * 0.5,
    };
    let (stood, clearance) = match lining {
        Lining::Post { stride, face } => (
            posts(probe, entry, stride, face, &ways, &blocked),
            entry.footprint().clearance,
        ),
        Lining::Run { length } => (
            runs(probe, entry, length, &ways, &blocked),
            run_ground(entry, length),
        ),
    };
    for (offset, yaw_rad) in stood {
        plan.clusters[0].props.push(SettlementMember {
            slug: entry.slug(),
            offset,
            yaw_rad,
            scale: 1.0,
            grammar_seed: rng.next_u64(),
            clearance,
        });
    }
}

/// Where props of `entry` stand `stride` metres apart down every path from
/// half a stride in - [`POST_START`] at the most - on alternate sides, the
/// other where one will not take it, each the path's half width and its own
/// clearance off it, turned to look at the path (`face` turning its front),
/// with the yaw each stands at.
fn posts(
    probe: &TerrainProbe,
    entry: &dyn CatalogueEntry,
    stride: f32,
    face: f32,
    ways: &Ways,
    blocked: &[([f32; 2], f32)],
) -> Vec<([f32; 2], f32)> {
    let clearance = entry.footprint().clearance;
    let reach = ways.half + clearance;
    let mut stood: Vec<([f32; 2], f32)> = Vec::new();
    let mut side = 1.0;
    for line in ways.lines {
        let mut along = (stride * 0.5).min(POST_START);
        while let Some((on, run)) = Ways::at(line, along) {
            for side_taken in [side, -side] {
                if stood.len() == MAX_LINING {
                    return stood;
                }
                let at = beside(on, run, side_taken * reach);
                let fits = inside(probe, at, clearance)
                    && ways.clear_of(at, reach)
                    && keeps_off(at, clearance, blocked)
                    && stood
                        .iter()
                        .all(|&(other, _)| distance2(at, other) >= 4.0 * clearance * clearance)
                    && stands(probe, at, ground_radius(entry, 1.0));
                if fits {
                    stood.push((at, facing([on[0] - at[0], on[1] - at[1]]) + face));
                    side = -side_taken;
                    break;
                }
            }
            along += stride;
        }
    }
    stood
}

/// The ground a section of `entry`, `length` metres end to end, stands on:
/// its clearance, or half its length where that is more - so that the
/// scatters, which keep off a placement's disc, keep off its end posts too.
fn run_ground(entry: &dyn CatalogueEntry, length: f32) -> f32 {
    (length * 0.5).max(entry.footprint().clearance)
}

/// Where sections of `entry`, `length` metres end to end, stand down both
/// sides of the way in from [`RUN_START`], the path's half width and
/// [`RUN_OFFSET`] off it - or the section's own ground ([`run_ground`]),
/// where that is more, so the path keeps off it as it does every prop's - a
/// section only where all of it keeps off the paths, the ground of
/// `blocked` and the sections stood before it - on a bend's inside the
/// next would cross the last - where it [`stands`] and its ends stand
/// within [`RUN_RELIEF`] of each other; with the yaw each stands at, its
/// front to the path and its run along it.
fn runs(
    probe: &TerrainProbe,
    entry: &dyn CatalogueEntry,
    length: f32,
    ways: &Ways,
    blocked: &[([f32; 2], f32)],
) -> Vec<([f32; 2], f32)> {
    let Some(way_in) = ways.lines.first() else {
        return Vec::new();
    };
    let ground = run_ground(entry, length);
    let reach = (ways.half + RUN_OFFSET).max(ground);
    let spacing = length + RUN_GAP;
    let reads = (length / RUN_READ).ceil().max(1.0) as usize;
    let mut stood = Vec::new();
    let mut sections: Vec<[[f32; 2]; 2]> = Vec::new();
    let mut along = RUN_START + spacing * 0.5;
    while let Some((on, run)) = Ways::at(way_in, along) {
        for side in [1.0, -1.0] {
            if stood.len() == MAX_LINING {
                return stood;
            }
            let at = beside(on, run, side * reach);
            let section = [-0.5, 0.5]
                .map(|end| [at[0] + run[0] * end * length, at[1] + run[1] * end * length]);
            // Read down the section from end to end: straight, where the
            // path may bend.
            let clear = (0..=reads).all(|k| {
                let off = (k as f32 / reads as f32 - 0.5) * length;
                let q = [at[0] + run[0] * off, at[1] + run[1] * off];
                ways.clear_of(q, ways.half + RUN_OFFSET * 0.5) && keeps_off(q, 0.0, blocked)
            });
            let ends = section.map(|end| probe.height_between(end));
            if clear
                && sections
                    .iter()
                    .all(|&other| segments_apart(section, other) >= RUN_GAP * 0.5)
                && ways.clear_of(at, reach)
                && inside(probe, at, ground)
                && (ends[0] - ends[1]).abs() <= RUN_RELIEF
                && stands(probe, at, ground)
            {
                stood.push((at, facing([on[0] - at[0], on[1] - at[1]])));
                sections.push(section);
            }
        }
        along += spacing;
    }
    stood
}

/// The spot `off` metres from `on` square across a path running `run` (a
/// unit vector): `run` turned a quarter from +X toward +Z, times `off`.
fn beside(on: [f32; 2], run: [f32; 2], off: f32) -> [f32; 2] {
    [on[0] - run[1] * off, on[1] + run[0] * off]
}

/// Whether the ground `radius` round `at` takes a prop that carries no
/// plinth: dry all round, and gentle enough at its middle and rim that the
/// compile's walk leaves it where it stands ([`SITE_SLOPE`]). A sited prop
/// asks a flat footprint of its region too; one beside a path stands where
/// the path runs, and a post's own foot is small enough for any such slope.
fn stands(probe: &TerrainProbe, at: [f32; 2], radius: f32) -> bool {
    let fits = |ground: &FootprintGround| ground.is_dry() && ground.steepest <= SITE_SLOPE;
    probe
        .footprint_unless(at, radius, |partial| !fits(partial))
        .is_some_and(|ground| fits(&ground))
}

/// Whether a prop `clearance` round at `at` stands on the probe's ground.
fn inside(probe: &TerrainProbe, at: [f32; 2], clearance: f32) -> bool {
    let edge = probe.half_extent() - clearance;
    at[0].abs() <= edge && at[1].abs() <= edge
}

/// Whether `at` stands `margin` metres or more off the ground of every disc
/// of `blocked`, each `(spot, radius)`.
fn keeps_off(at: [f32; 2], margin: f32, blocked: &[([f32; 2], f32)]) -> bool {
    blocked.iter().all(|&(spot, radius)| {
        let reach = radius + margin;
        distance2(at, spot) >= reach * reach
    })
}

/// A settlement's paths, each `half` metres either side of its line.
struct Ways<'a> {
    lines: &'a [Vec<[f32; 2]>],
    half: f32,
}

impl Ways<'_> {
    /// The point `along` metres down `line` from its first, and the way the
    /// line runs there (a unit vector) - `None` past its end.
    fn at(line: &[[f32; 2]], along: f32) -> Option<([f32; 2], [f32; 2])> {
        let mut left = along;
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = distance2(a, b).sqrt();
            if length > 0.0 && left <= length {
                let run = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
                return Some(([a[0] + run[0] * left, a[1] + run[1] * left], run));
            }
            left -= length;
        }
        None
    }

    /// Whether `at` stands `reach` metres or more off the line of every
    /// path - a millimetre short allowed, for the path it was measured off.
    fn clear_of(&self, at: [f32; 2], reach: f32) -> bool {
        self.lines
            .iter()
            .flat_map(|line| line.windows(2))
            .all(|pair| off_segment(at, pair[0], pair[1]) >= reach - 1e-3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe as wide as a seeded world's, in its 10 m cells, over level
    /// ground 10 m over its water.
    fn level() -> TerrainProbe {
        const GRID: usize = 96;
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: 10.0,
            data: vec![10.0; GRID * GRID],
        };
        TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT)
    }

    fn entry(slug: &str) -> &'static dyn CatalogueEntry {
        crate::catalogue::by_slug(slug).unwrap_or_else(|| panic!("no {slug}"))
    }

    /// Down a straight path, posts stand a stride apart from half a stride
    /// in, on alternate sides, their clearance off the path's edge, each
    /// looking at the path - and no more than [`MAX_LINING`] of them.
    #[test]
    fn posts_stand_a_stride_apart_on_alternate_sides_looking_at_the_path() {
        let lantern = entry("lantern");
        let Some(Lining::Post { stride, face }) = lantern.lines_paths() else {
            panic!("a lantern is a post");
        };
        let lines = [vec![[0.0, -100.0], [0.0, 100.0]]];
        let ways = Ways {
            lines: &lines,
            half: 1.25,
        };
        let stood = posts(&level(), lantern, stride, face, &ways, &[]);
        assert_eq!(stood.len(), MAX_LINING);
        let reach = 1.25 + lantern.footprint().clearance;
        for (k, &(at, yaw)) in stood.iter().enumerate() {
            assert!((at[0].abs() - reach).abs() < 1e-4, "{k}: {at:?}");
            assert!((at[1] - (-100.0 + stride * (0.5 + k as f32))).abs() < 1e-3);
            if k > 0 {
                assert_eq!(at[0].signum(), -stood[k - 1].0[0].signum(), "{k}");
            }
            // Its front (-Z) turned to the path: (-sin, -cos) of the yaw
            // `face` was added to.
            let front = [-(yaw - face).sin(), -(yaw - face).cos()];
            assert!((front[0] + at[0].signum()).abs() < 1e-4 && front[1].abs() < 1e-4);
        }
    }

    /// A post keeps off the ground of what stands by the path and off the
    /// other paths, taking the other side where one side will not have it.
    #[test]
    fn a_post_keeps_off_what_stands_by_the_path_and_the_other_paths() {
        let lantern = entry("lantern");
        let Some(Lining::Post { stride, face }) = lantern.lines_paths() else {
            panic!("a lantern is a post");
        };
        let reach = 1.25 + lantern.footprint().clearance;
        // A building over the first post's spot on the first side, and a
        // second path along the far side of the second post.
        let blocked = [([reach, stride * 0.5], 3.0)];
        let lines = [
            vec![[0.0, 0.0], [0.0, 100.0]],
            vec![[reach + 1.0, stride], [reach + 1.0, stride * 2.0]],
        ];
        let ways = Ways {
            lines: &lines,
            half: 1.25,
        };
        let stood = posts(&level(), lantern, stride, face, &ways, &blocked);
        for &(at, _) in &stood {
            assert!(
                keeps_off(at, lantern.footprint().clearance, &blocked),
                "{at:?}"
            );
            assert!(ways.clear_of(at, reach), "{at:?}");
        }
        assert!(stood[0].0[0] < 0.0, "the first took the other side");
        assert!(stood[1].0[0] < 0.0, "the second keeps off the other path");
    }

    /// A fence runs down both sides of the way in from [`RUN_START`], its
    /// sections end to end a gap apart, each along the path with its front
    /// to it - none on the paths beyond the way in.
    #[test]
    fn a_fence_runs_down_both_sides_of_the_way_in_end_to_end() {
        let fence = entry("picket_fence");
        let Some(Lining::Run { length }) = fence.lines_paths() else {
            panic!("a picket fence is a run");
        };
        let lines = [
            vec![[0.0, 0.0], [0.0, 60.0]],
            vec![[30.0, 0.0], [30.0, 60.0]],
        ];
        let ways = Ways {
            lines: &lines,
            half: 1.25,
        };
        let stood = runs(&level(), fence, length, &ways, &[]);
        assert_eq!(stood.len(), MAX_LINING);
        let reach = (1.25 + RUN_OFFSET).max(run_ground(fence, length));
        for k in 0..stood.len() / 2 {
            let ((left, left_yaw), (right, right_yaw)) = (stood[2 * k], stood[2 * k + 1]);
            let along = RUN_START + (length + RUN_GAP) * (0.5 + k as f32);
            for (at, yaw) in [(left, left_yaw), (right, right_yaw)] {
                assert!((at[0].abs() - reach).abs() < 1e-4 && (at[1] - along).abs() < 1e-3);
                let front = [-yaw.sin(), -yaw.cos()];
                assert!((front[0] + at[0].signum()).abs() < 1e-4 && front[1].abs() < 1e-4);
            }
            assert_eq!(left[0], -right[0]);
        }
    }

    /// Round a bend, no section of a fence crosses or touches another: on
    /// the bend's inside, where the sections a path's stride apart would
    /// meet, the run leaves a gap - and goes on past the bend.
    #[test]
    fn a_fence_round_a_bend_keeps_its_sections_apart() {
        let fence = entry("iron_fence");
        let Some(Lining::Run { length }) = fence.lines_paths() else {
            panic!("an iron fence is a run");
        };
        // Up +Z, then turned 70 degrees toward +X.
        let lines = [vec![[0.0, 0.0], [0.0, 25.0], [28.2, 35.3]]];
        let ways = Ways {
            lines: &lines,
            half: 1.25,
        };
        let stood = runs(&level(), fence, length, &ways, &[]);
        let sections: Vec<[[f32; 2]; 2]> = stood
            .iter()
            .map(|&(at, yaw)| {
                // Along its catalogue X: a yaw turns +X to (cos, -sin).
                let run = [yaw.cos() * length * 0.5, -yaw.sin() * length * 0.5];
                [
                    [at[0] - run[0], at[1] - run[1]],
                    [at[0] + run[0], at[1] + run[1]],
                ]
            })
            .collect();
        for (i, &a) in sections.iter().enumerate() {
            for &b in &sections[i + 1..] {
                let apart = segments_apart(a, b);
                assert!(apart >= RUN_GAP * 0.5 - 1e-3, "{a:?} and {b:?}: {apart} m");
            }
        }
        assert!(
            stood.iter().any(|&(at, _)| at[0] > 5.0),
            "the run stops at the bend: {stood:?}"
        );
    }

    /// Two segments are as far apart as their nearest ends reach, and none
    /// where they cross.
    #[test]
    fn segments_are_apart_by_their_nearest_reach() {
        let a = [[0.0, 0.0], [4.0, 0.0]];
        assert_eq!(segments_apart(a, [[2.0, -1.0], [2.0, 1.0]]), 0.0);
        assert!((segments_apart(a, [[4.1, 0.0], [8.0, 0.0]]) - 0.1).abs() < 1e-5);
        assert!((segments_apart(a, [[1.0, 2.0], [3.0, 1.5]]) - 1.5).abs() < 1e-6);
    }

    /// The way `along` a polyline is read off its segments in turn, and a
    /// spot beside it lies square to it.
    #[test]
    fn a_path_is_read_along_and_beside() {
        let line = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        assert_eq!(Ways::at(&line, 4.0), Some(([4.0, 0.0], [1.0, 0.0])));
        assert_eq!(Ways::at(&line, 15.0), Some(([10.0, 5.0], [0.0, 1.0])));
        assert_eq!(Ways::at(&line, 20.5), None);
        // Running +X, a positive offset lies toward +Z.
        assert_eq!(beside([4.0, 0.0], [1.0, 0.0], 2.0), [4.0, 2.0]);
        assert_eq!(beside([4.0, 0.0], [1.0, 0.0], -2.0), [4.0, -2.0]);
    }

    /// A spot keeps off a disc and a path by what it is asked to, and no
    /// more.
    #[test]
    fn a_spot_keeps_off_what_it_is_asked_to() {
        let blocked = [([0.0, 0.0], 3.0)];
        assert!(keeps_off([4.0, 0.0], 1.0, &blocked));
        assert!(!keeps_off([3.9, 0.0], 1.0, &blocked));
        let lines = [vec![[0.0, 10.0], [20.0, 10.0]]];
        let ways = Ways {
            lines: &lines,
            half: 1.25,
        };
        assert!(ways.clear_of([5.0, 12.25], 2.25));
        assert!(!ways.clear_of([5.0, 12.0], 2.25));
    }
}
