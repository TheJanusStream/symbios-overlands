//! A seeded settlement's paths (#1779): least-cost footways over the probe
//! its settlement is sited on - from the landing down the view to the
//! landmark and across to the gate, then on to every building of every
//! cluster, each joining the paths already laid where that costs least, so
//! they branch. Each building but the primary landmark is turned to front
//! onto its own (#1780), and [`super::lining`] stands props along them.
//!
//! The paths are routed on a grid [`SUBDIVIDE`] times finer than the
//! probe's, its heights read between the probe's samples. A step's cost is
//! its length, more on a slope ([`SLOPE_COST`]) and less along a path
//! already laid ([`REUSE`]); water, ground too steep to walk
//! ([`MAX_SLOPE`]) and the buildings' own ground refuse it. A route is
//! smoothed ([`CHAIKIN`]) and thinned to the points its bends need
//! ([`THIN`]) before the record keeps it
//! ([`crate::pds::SovereignPathsConfig`]). Costs are whole millimetres and
//! the search breaks ties by cell, so every peer routes the same paths
//! (#1132).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::settlement::{SettlementMember, SettlementPlan, facing, seaward};
use super::siting::TerrainProbe;

/// How many routing cells to a probe cell along each side.
const SUBDIVIDE: usize = 2;

/// The steepest step a path takes (rise/run).
const MAX_SLOPE: f32 = 0.45;

/// How much more a step costs per unit of its slope: a 0.2 slope costs a
/// path 2.2 times the level.
const SLOPE_COST: f32 = 6.0;

/// What a step along a path already laid costs, as a share of a new one's:
/// a building's path joins the others rather than running beside them.
const REUSE: f32 = 0.35;

/// How far over the water a path's ground stands at the least (m).
const FREEBOARD: f32 = 0.5;

/// How far past a building's ground its neighbours' paths keep off it (m).
const CLEARANCE: f32 = 1.5;

/// How far past its own ground a building's door stands (m): where its
/// path meets it.
const DOOR: f32 = 1.0;

/// Corner-cutting rounds a route is smoothed with.
const CHAIKIN: usize = 2;

/// How far a thinned route may stray from its smoothed line (m).
const THIN: f32 = 0.25;

/// The settlement's paths over `probe` (#1779), each a polyline of world XZ
/// points: from `landing` to the primary landmark of `plan`, from `landing`
/// to `gate` (its spot and its ground's radius), and from every other
/// building of every cluster to the paths laid before it ([`route`]).
/// `forecourt` holds what else stands by the landing - the monument - as
/// `(spot, radius)`, which the paths keep off, as they do every building
/// and prop. Each building a path reaches but the primary landmark, which
/// faces its arrival, is turned to front onto its path (#1780): its
/// catalogue front to the door its path leaves from - all but one that
/// looks out over the water ([`seaward`]), whose front is its sea face.
pub fn route_plan(
    probe: &TerrainProbe,
    plan: &mut SettlementPlan,
    landing: [f32; 2],
    gate: ([f32; 2], f32),
    forecourt: &[([f32; 2], f32)],
) -> Vec<Vec<[f32; 2]>> {
    // Every building but the primary landmark, by its cluster and which of
    // the cluster's it is: `None` for the cluster's landmark.
    let mut members: Vec<(usize, Option<usize>)> = Vec::new();
    let mut buildings = Vec::new();
    let mut blocked: Vec<([f32; 2], f32)> = forecourt.to_vec();
    blocked.push(gate);
    for (ci, cluster) in plan.clusters.iter().enumerate() {
        if let Some(member) = &cluster.landmark {
            blocked.push(ground(member));
            if ci > 0 {
                members.push((ci, None));
                buildings.push(ground(member));
            }
        }
        for (i, member) in cluster.secondaries.iter().enumerate() {
            blocked.push(ground(member));
            members.push((ci, Some(i)));
            buildings.push(ground(member));
        }
        for prop in &cluster.props {
            blocked.push(ground(prop));
        }
    }
    let landmark = ground(plan.primary_landmark());
    let routes = route(probe, landing, gate, landmark, &buildings, &blocked);
    for (&(ci, which), door) in members.iter().zip(&routes.doors) {
        let Some(door) = door else {
            continue;
        };
        let cluster = &mut plan.clusters[ci];
        let member = match which {
            None => cluster.landmark.as_mut().expect("listed for its landmark"),
            Some(i) => &mut cluster.secondaries[i],
        };
        let looks_out = crate::catalogue::by_slug(member.slug)
            .is_some_and(|entry| seaward(probe, entry, member.offset).is_some());
        if !looks_out {
            member.yaw_rad = facing([door[0] - member.offset[0], door[1] - member.offset[1]]);
        }
    }
    routes.lines
}

/// A settlement member's ground, `(spot, radius)`, as the record's
/// placement carries it ([`SettlementMember::ground_radius`]).
pub(super) fn ground(member: &SettlementMember) -> ([f32; 2], f32) {
    (member.offset, member.ground_radius())
}

/// What [`route`] lays (#1779).
#[derive(Debug, PartialEq)]
pub struct Routes {
    /// The paths, each a polyline of world XZ points.
    pub lines: Vec<Vec<[f32; 2]>>,
    /// Where the path of each of the buildings meets it, in their order -
    /// `None` for a building no route reached (#1780).
    pub doors: Vec<Option<[f32; 2]>>,
}

/// Paths over `probe` (#1779): the first from `landing` to the door of the
/// `landmark` (its spot and its ground's radius) that faces the landing,
/// the second straight from `landing` to `gate`'s door where that way is
/// dry and clear, then one from each of `buildings` to a point on the
/// paths laid before it, nearest the landing first, from a door on
/// whichever side that way costs least. A door stands [`DOOR`] past its
/// building's ground. Each path keeps off the ground of `blocked`, its own
/// building's included. A building no route reaches - across water, up a
/// cliff - has no path.
pub fn route(
    probe: &TerrainProbe,
    landing: [f32; 2],
    gate: ([f32; 2], f32),
    landmark: ([f32; 2], f32),
    buildings: &[([f32; 2], f32)],
    blocked: &[([f32; 2], f32)],
) -> Routes {
    let grid = Grid::of(probe);
    let mut net = Network::new(&grid);
    // The lines the network's cells were laid along, which a building's
    // path ends on.
    let mut laid: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut lines = Vec::new();

    // Down the view to the landmark: to the cell round it nearest the door
    // that faces the landing - the first of equals - the door then set
    // between that cell and its ground. It leaves the landing straight down
    // the view where that way is dry and clear: the gate and the monument
    // stand either side of it (#1777), nearer each other than a route on
    // the grid passes between.
    let around = blocked_but(blocked, landmark.0);
    let front = toward(landmark.0, landing, landmark.1 + DOOR);
    let target = grid.ring(landmark, &around).into_iter().min_by(|&a, &b| {
        distance2(grid.centre(a), front).total_cmp(&distance2(grid.centre(b), front))
    });
    let main = target.and_then(|target| {
        grid.search(
            &[grid.lead(landing, front, &around, landmark)],
            |cell| cell == target,
            &around,
            landmark,
            &net,
        )
    });
    if let Some(cells) = main {
        net.lay(&cells);
        let reached = grid.centre(*cells.last().expect("a route has a cell"));
        let door = toward(landmark.0, reached, landmark.1 + DOOR);
        let line = finish(&grid, landing, &cells, door, &around, landmark);
        laid.push(line.clone());
        lines.push(line);
    }
    // Across to the gate, where the straight way there is dry and clear.
    let spur = vec![landing, toward(gate.0, landing, gate.1 + DOOR)];
    if grid.dry_along(&spur) && clear_of(&spur, &blocked_but(blocked, gate.0)) {
        lines.push(spur);
    }

    // Each building to the network, nearest the landing first - the stable
    // sort keeps the given order between equals - from whichever cell round
    // it the way costs least, its door set between that cell and its
    // ground.
    let mut doors = vec![None; buildings.len()];
    let mut order: Vec<usize> = (0..buildings.len()).collect();
    order.sort_by(|&a, &b| {
        distance2(buildings[a].0, landing).total_cmp(&distance2(buildings[b].0, landing))
    });
    for k in order {
        // No path reached the landmark: there is no network to join.
        if laid.is_empty() {
            break;
        }
        let (at, radius) = buildings[k];
        let around = blocked_but(blocked, at);
        let ring = grid.ring((at, radius), &around);
        let Some(cells) = grid.search(&ring, |cell| net.holds(cell), &around, (at, radius), &net)
        else {
            continue;
        };
        net.lay(&cells);
        let door = toward(at, grid.centre(cells[0]), radius + DOOR);
        doors[k] = Some(door);
        // It ends on the line it joins, not at the middle of that line's
        // cell, which the line may pass a few metres off.
        let reached = grid.centre(*cells.last().expect("a route has a cell"));
        let end = nearest_on(&laid, reached).unwrap_or(reached);
        let line = finish(&grid, door, &cells, end, &around, (at, radius));
        laid.push(line.clone());
        lines.push(line);
    }
    lines.retain(|line| line.len() >= 2);
    Routes { lines, doors }
}

/// The spot `reach` metres from `from` toward `to` - `from` itself where
/// the two are one.
fn toward(from: [f32; 2], to: [f32; 2], reach: f32) -> [f32; 2] {
    let (dx, dz) = (to[0] - from[0], to[1] - from[1]);
    let length = (dx * dx + dz * dz).sqrt();
    if length < 1e-3 {
        return from;
    }
    [from[0] + dx / length * reach, from[1] + dz / length * reach]
}

/// `blocked` without the building standing at `own`: its own path leaves
/// from its door.
fn blocked_but(blocked: &[([f32; 2], f32)], own: [f32; 2]) -> Vec<([f32; 2], f32)> {
    blocked
        .iter()
        .copied()
        .filter(|&(at, _)| at != own)
        .collect()
}

/// A route of `cells` from `start` to `end` as the record keeps it:
/// smoothed and thinned, its ends exact - or, where smoothing would cut a
/// corner into the ground of `blocked` or of `own`, the building at one
/// end, thinned only, or neither where thinning would.
fn finish(
    grid: &Grid,
    start: [f32; 2],
    cells: &[usize],
    end: [f32; 2],
    blocked: &[([f32; 2], f32)],
    own: ([f32; 2], f32),
) -> Vec<[f32; 2]> {
    let all: Vec<([f32; 2], f32)> = blocked.iter().copied().chain([own]).collect();
    let mut raw = Vec::with_capacity(cells.len() + 2);
    raw.push(start);
    raw.extend(cells.iter().map(|&cell| grid.centre(cell)));
    raw.push(end);
    raw.dedup();
    let mut points = raw.clone();
    for _ in 0..CHAIKIN {
        points = chaikin(&points);
    }
    [thin(&points, THIN), thin(&raw, THIN)]
        .into_iter()
        .find(|line| clear_of(line, &all))
        .unwrap_or(raw)
}

/// Whether `line` keeps off the ground of every disc of `blocked`, each
/// `(spot, radius)`.
fn clear_of(line: &[[f32; 2]], blocked: &[([f32; 2], f32)]) -> bool {
    line.windows(2).all(|pair| {
        blocked
            .iter()
            .all(|&(at, radius)| off_segment(at, pair[0], pair[1]) >= radius)
    })
}

/// The point of `lines` nearest `at` - the first of equals - `None` where
/// they hold no segment.
fn nearest_on(lines: &[Vec<[f32; 2]>], at: [f32; 2]) -> Option<[f32; 2]> {
    let mut best: Option<(f32, [f32; 2])> = None;
    for pair in lines.iter().flat_map(|line| line.windows(2)) {
        let on = closest_on(at, pair[0], pair[1]);
        let d = distance2(on, at);
        if best.is_none_or(|(kept, _)| d < kept) {
            best = Some((d, on));
        }
    }
    best.map(|(_, on)| on)
}

/// The square of the distance between `a` and `b`.
pub(super) fn distance2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (a[0] - b[0], a[1] - b[1]);
    dx * dx + dz * dz
}

/// The point of the segment `a`-`b` nearest `p`.
fn closest_on(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dz * dz;
    let t = if length2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    [a[0] + dx * t, a[1] + dz * t]
}

/// How far `p` stands from the segment `a`-`b`.
pub(super) fn off_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    distance2(p, closest_on(p, a, b)).sqrt()
}

/// One round of Chaikin's corner cutting, the ends kept.
fn chaikin(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        out.push([a[0] * 0.75 + b[0] * 0.25, a[1] * 0.75 + b[1] * 0.25]);
        out.push([a[0] * 0.25 + b[0] * 0.75, a[1] * 0.25 + b[1] * 0.75]);
    }
    out.push(points[points.len() - 1]);
    out
}

/// `points` thinned to those its line needs to stay within `tolerance` of
/// itself (Ramer-Douglas-Peucker), the ends kept.
fn thin(points: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut spans = vec![(0, points.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        let (a, b) = (points[first], points[last]);
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx * dx + dz * dz).sqrt();
        let mut worst = (0.0f32, first);
        for (k, p) in points.iter().enumerate().take(last).skip(first + 1) {
            let off = if length < 1e-6 {
                ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt()
            } else {
                ((p[0] - a[0]) * dz - (p[1] - a[1]) * dx).abs() / length
            };
            if off > worst.0 {
                worst = (off, k);
            }
        }
        if worst.0 > tolerance {
            keep[worst.1] = true;
            spans.push((first, worst.1));
            spans.push((worst.1, last));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(&p, kept)| kept.then_some(p))
        .collect()
}

/// The routing grid over the probe's ground: `side` cells a side `cell`
/// metres apart, centred on the origin, each with its height and whether
/// it stands over the water.
struct Grid {
    side: usize,
    cell: f32,
    half: f32,
    heights: Vec<f32>,
    dry: Vec<bool>,
}

impl Grid {
    fn of(probe: &TerrainProbe) -> Self {
        let cell = probe.cell_size() / SUBDIVIDE as f32;
        let half = probe.half_extent();
        let side = ((2.0 * half / cell).round() as usize).max(1) + 1;
        let mut heights = Vec::with_capacity(side * side);
        for j in 0..side {
            for i in 0..side {
                heights
                    .push(probe.height_between([i as f32 * cell - half, j as f32 * cell - half]));
            }
        }
        let dry = heights
            .iter()
            .map(|&h| h > probe.water_y() + FREEBOARD)
            .collect();
        Self {
            side,
            cell,
            half,
            heights,
            dry,
        }
    }

    /// Whether every cell `line` crosses, read every metre, stands over
    /// the water.
    fn dry_along(&self, line: &[[f32; 2]]) -> bool {
        line.windows(2).all(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let steps = (dx * dx + dz * dz).sqrt().ceil().max(1.0) as usize;
            (0..=steps).all(|k| {
                let t = k as f32 / steps as f32;
                self.dry[self.cell_at([a[0] + dx * t, a[1] + dz * t])]
            })
        })
    }

    /// The cell nearest world `at`, the grid's edge past it.
    fn cell_at(&self, at: [f32; 2]) -> usize {
        let last = (self.side - 1) as f32;
        let i = ((at[0] + self.half) / self.cell).round().clamp(0.0, last) as usize;
        let j = ((at[1] + self.half) / self.cell).round().clamp(0.0, last) as usize;
        j * self.side + i
    }

    /// Cell `cell`'s middle, world XZ.
    fn centre(&self, cell: usize) -> [f32; 2] {
        let (i, j) = (cell % self.side, cell / self.side);
        [
            i as f32 * self.cell - self.half,
            j as f32 * self.cell - self.half,
        ]
    }

    /// Whether a route keeps off `cell`: over the water, within
    /// [`CLEARANCE`] of the ground of a disc of `blocked`, or within
    /// [`THIN`] of `own` - the ground of the building at the route's end,
    /// which its thinned line may stray that far toward - each reach
    /// widened by half a step's diagonal in square ([`Grid::reach2`]).
    fn refused(&self, cell: usize, blocked: &[([f32; 2], f32)], own: ([f32; 2], f32)) -> bool {
        let at = self.centre(cell);
        let near = |(c, r): ([f32; 2], f32), margin: f32| distance2(at, c) < self.reach2(r, margin);
        !self.dry[cell] || near(own, THIN) || blocked.iter().any(|&disc| near(disc, CLEARANCE))
    }

    /// The square of how far from a disc of `radius` a route's cells keep
    /// for no step between two of them to come within `margin` of it: the
    /// point of a step nearest a disc's middle lies square to the step, at
    /// most half a diagonal along from one of its ends.
    fn reach2(&self, radius: f32, margin: f32) -> f32 {
        (radius + margin) * (radius + margin) + self.cell * self.cell * 0.5
    }

    /// Down the straight way from `from` toward `to`, read a cell apart, the
    /// first cell a route may leave from ([`Grid::refused`]) - where the way
    /// to its middle is dry and keeps off the ground of `blocked` and `own`;
    /// else, as where there is no such cell, the cell `from` stands in.
    fn lead(
        &self,
        from: [f32; 2],
        to: [f32; 2],
        blocked: &[([f32; 2], f32)],
        own: ([f32; 2], f32),
    ) -> usize {
        let reach = distance2(from, to).sqrt();
        let all: Vec<([f32; 2], f32)> = blocked.iter().copied().chain([own]).collect();
        (1..)
            .map(|k| k as f32 * self.cell)
            .take_while(|&along| along <= reach)
            .map(|along| self.cell_at(toward(from, to, along)))
            .find(|&cell| !self.refused(cell, blocked, own))
            .filter(|&cell| {
                let way = [from, self.centre(cell)];
                self.dry_along(&way) && clear_of(&way, &all)
            })
            .unwrap_or_else(|| self.cell_at(from))
    }

    /// The cells round `own`, a building's ground, its route leaves it from
    /// or reaches it at: those left open of the band one cell wide just past
    /// what [`Grid::refused`] keeps the route off.
    fn ring(&self, own: ([f32; 2], f32), blocked: &[([f32; 2], f32)]) -> Vec<usize> {
        let inner = self.reach2(own.1, THIN).sqrt();
        let outer = inner + self.cell;
        let (low, high) = (
            self.cell_at([own.0[0] - outer, own.0[1] - outer]),
            self.cell_at([own.0[0] + outer, own.0[1] + outer]),
        );
        let mut ring = Vec::new();
        for j in low / self.side..=high / self.side {
            for i in low % self.side..=high % self.side {
                let cell = j * self.side + i;
                if distance2(self.centre(cell), own.0) < outer * outer
                    && !self.refused(cell, blocked, own)
                {
                    ring.push(cell);
                }
            }
        }
        ring
    }

    /// The cheapest way from one of `starts` to a cell `done` says yes to,
    /// its cells in order - `None` where none is reached. A step onto a
    /// cell [`Grid::refused`] for `blocked` and `own` is refused, unless
    /// `done` says yes to it, and one too steep is; one onto `net` costs
    /// [`REUSE`] of the rest. Whole millimetres; ties go to the lower cell.
    fn search(
        &self,
        starts: &[usize],
        done: impl Fn(usize) -> bool,
        blocked: &[([f32; 2], f32)],
        own: ([f32; 2], f32),
        net: &Network,
    ) -> Option<Vec<usize>> {
        let side = self.side as i64;
        let mut best = vec![u32::MAX; self.heights.len()];
        let mut from = vec![usize::MAX; self.heights.len()];
        let mut queue = BinaryHeap::new();
        for &start in starts {
            best[start] = 0;
            queue.push(Reverse((0u32, start)));
        }
        while let Some(Reverse((cost, cell))) = queue.pop() {
            if cost > best[cell] {
                continue;
            }
            if done(cell) {
                let mut cells = vec![cell];
                let mut at = cell;
                while from[at] != usize::MAX {
                    at = from[at];
                    cells.push(at);
                }
                cells.reverse();
                return Some(cells);
            }
            let (i, j) = ((cell as i64) % side, (cell as i64) / side);
            for (di, dj) in STEPS {
                let (ni, nj) = (i + di, j + dj);
                if ni < 0 || nj < 0 || ni >= side || nj >= side {
                    continue;
                }
                let next = (nj * side + ni) as usize;
                if self.refused(next, blocked, own) && !done(next) {
                    continue;
                }
                let length = if di != 0 && dj != 0 {
                    self.cell * std::f32::consts::SQRT_2
                } else {
                    self.cell
                };
                let slope = (self.heights[next] - self.heights[cell]).abs() / length;
                if slope > MAX_SLOPE {
                    continue;
                }
                let mut step = length * (1.0 + SLOPE_COST * slope);
                if net.holds(next) {
                    step *= REUSE;
                }
                let next_cost = cost.saturating_add(((step * 1000.0).round() as u32).max(1));
                if next_cost < best[next] {
                    best[next] = next_cost;
                    from[next] = cell;
                    queue.push(Reverse((next_cost, next)));
                }
            }
        }
        None
    }
}

/// The eight steps from a cell, in a fixed order.
const STEPS: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (-1, 1),
    (1, -1),
    (-1, -1),
];

/// The cells the paths laid so far run over. Every one but the landing's
/// stands at least as far off each building's ground as [`THIN`] and half a
/// step's diagonal in square take it ([`Grid::reach2`]) - a building's own
/// ring is no further off it, its neighbours' cells are [`CLEARANCE`] off -
/// so no step onto one from a cell left open comes onto a building's
/// ground.
struct Network {
    cells: Vec<bool>,
}

impl Network {
    fn new(grid: &Grid) -> Self {
        Self {
            cells: vec![false; grid.heights.len()],
        }
    }

    fn holds(&self, cell: usize) -> bool {
        self.cells[cell]
    }

    fn lay(&mut self, cells: &[usize]) {
        for &cell in cells {
            self.cells[cell] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe as wide as a seeded world's, in its 10 m cells, over ground
    /// of `height` at world XZ, its water at 0.
    fn probe(height: impl Fn(f32, f32) -> f32) -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let data = (0..GRID * GRID)
            .map(|i| {
                let (x, z) = (i % GRID, i / GRID);
                height(x as f32 * CELL - half, z as f32 * CELL - half)
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

    fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
        (a[0] - b[0]).hypot(a[1] - b[1])
    }

    /// How far `p` stands from the segment `a`-`b`.
    fn off_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let length2 = dx * dx + dz * dz;
        let t = if length2 > 0.0 {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        distance(p, [a[0] + dx * t, a[1] + dz * t])
    }

    const LANDING: [f32; 2] = [0.0, 60.0];
    const GATE: ([f32; 2], f32) = ([8.0, 52.0], 2.0);
    const LANDMARK: ([f32; 2], f32) = ([0.0, 0.0], 8.0);

    /// On level, dry ground the first path runs straight down from the
    /// landing to the landmark's door - a metre past its ground toward the
    /// landing - and the second across to the gate's.
    #[test]
    fn on_level_ground_the_first_path_runs_straight_to_the_landmark() {
        let level = probe(|_, _| 10.0);
        let lines = route(&level, LANDING, GATE, LANDMARK, &[], &[LANDMARK]).lines;
        assert_eq!(lines.len(), 2);
        let main = &lines[0];
        assert_eq!(main[0], LANDING);
        assert!(
            distance(main[main.len() - 1], [0.0, 9.0]) < 1e-4,
            "{main:?}"
        );
        for p in main {
            assert!(p[0].abs() < 2.6, "{p:?} wanders off the straight way");
        }
        let gate_door = lines[1][1];
        assert_eq!(lines[1][0], LANDING);
        assert!((distance(gate_door, GATE.0) - (GATE.1 + DOOR)).abs() < 1e-4);
    }

    /// Each building's path leaves its door - the one [`route`] reports for
    /// it (#1780) - and joins the paths laid before it, keeping off the
    /// other buildings' ground on the way.
    #[test]
    fn a_buildings_path_joins_the_network_and_keeps_off_its_neighbours() {
        let level = probe(|_, _| 10.0);
        let buildings = [
            ([40.0, 20.0], 6.0),
            ([-35.0, 10.0], 5.0),
            ([30.0, -30.0], 7.0),
        ];
        let mut blocked = vec![LANDMARK];
        blocked.extend(buildings);
        let Routes { lines, doors } = route(&level, LANDING, GATE, LANDMARK, &buildings, &blocked);
        assert_eq!(lines.len(), 2 + buildings.len());
        for (&(at, radius), door) in buildings.iter().zip(&doors) {
            let door = door.unwrap_or_else(|| panic!("no door for the building at {at:?}"));
            assert!((distance(door, at) - (radius + DOOR)).abs() < 1e-3);
            assert!(
                lines.iter().skip(2).any(|line| line[0] == door),
                "no path leaves the door {door:?}"
            );
        }
        for (k, line) in lines.iter().enumerate().skip(2) {
            let (start, end) = (line[0], line[line.len() - 1]);
            let own = buildings
                .iter()
                .find(|(at, r)| (distance(start, *at) - (r + DOOR)).abs() < 1e-3)
                .unwrap_or_else(|| panic!("path {k} leaves no building's door: {start:?}"));
            // On a path laid before it, not a cell's width off: the gate's
            // spur aside, which no building's path joins.
            let joins = lines[..k]
                .iter()
                .enumerate()
                .filter(|(earlier, _)| *earlier != 1)
                .any(|(_, line)| line.windows(2).any(|w| off_segment(end, w[0], w[1]) < 1e-3));
            assert!(joins, "path {k} ends off the paths before it: {end:?}");
            for &(at, radius) in blocked.iter().filter(|(at, _)| *at != own.0) {
                for p in line {
                    assert!(
                        distance(*p, at) > radius,
                        "path {k} crosses the ground of the building at {at:?}: {p:?}"
                    );
                }
            }
        }
    }

    /// A path never runs over water: it goes round a river's end where it
    /// can, and where a river runs all the way across there is no path to
    /// the landmark - only the gate's, which stands this side of it.
    #[test]
    fn a_path_never_crosses_water() {
        let ford = probe(|x, z| {
            if (20.0..=30.0).contains(&z) && x < 30.0 {
                -5.0
            } else {
                10.0
            }
        });
        let lines = route(&ford, LANDING, GATE, LANDMARK, &[], &[LANDMARK]).lines;
        let main = &lines[0];
        assert!(distance(main[main.len() - 1], [0.0, 9.0]) < 1e-4);
        for pair in main.windows(2) {
            for k in 0..=10 {
                let t = k as f32 / 10.0;
                let at = [
                    pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                    pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                ];
                assert!(
                    ford.height_between(at) > ford.water_y(),
                    "{at:?} is under water"
                );
            }
        }
        let river = probe(|_, z| {
            if (20.0..=30.0).contains(&z) {
                -5.0
            } else {
                10.0
            }
        });
        let lines = route(&river, LANDING, GATE, LANDMARK, &[], &[LANDMARK]).lines;
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0][0], LANDING);
    }

    /// The same ground and buildings route the same paths, bit for bit.
    #[test]
    fn the_paths_are_routed_alike_every_time() {
        let rolling = probe(|x, z| 10.0 + (x * 0.05).sin() * 4.0 + (z * 0.03).cos() * 3.0);
        let buildings = [([40.0, 20.0], 6.0), ([-60.0, -40.0], 5.0)];
        let mut blocked = vec![LANDMARK];
        blocked.extend(buildings);
        let a = route(&rolling, LANDING, GATE, LANDMARK, &buildings, &blocked);
        let b = route(&rolling, LANDING, GATE, LANDMARK, &buildings, &blocked);
        assert_eq!(a, b);
        assert_eq!(a.lines.len(), 4);
    }

    /// Smoothing and thinning keep a route's ends where they were and stay
    /// near its corners.
    #[test]
    fn a_route_keeps_its_ends() {
        let zigzag = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [20.0, 10.0]];
        let mut points = zigzag.to_vec();
        for _ in 0..CHAIKIN {
            points = chaikin(&points);
        }
        let thinned = thin(&points, THIN);
        assert_eq!(thinned[0], zigzag[0]);
        assert_eq!(thinned[thinned.len() - 1], zigzag[3]);
        assert!(thinned.len() < points.len());
        for p in &thinned {
            let near = zigzag.windows(2).any(|w| off_segment(*p, w[0], w[1]) < 2.6);
            assert!(near, "{p:?} strays from the corners");
        }
    }
}
