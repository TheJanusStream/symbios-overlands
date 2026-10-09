//! Berlin's streets on a geodata region's walkable ground (#1595), meshed as
//! a road network's are.
//!
//! The streets are ATKIS's street axes, and the carriageways of the streets
//! whose carriageways run apart, fetched over the core's box with its other
//! layers ([`geodata::berlin::STREET_AXES`],
//! [`geodata::berlin::CARRIAGEWAY_AXES`]). ATKIS cuts its axes at every
//! junction and joins them at exactly shared end points, so they are a
//! street graph as they come: [`street_chains`] welds the end points into
//! nodes, cuts the lines at the edge of the core, and joins the lines that
//! meet two to a node into the runs between junctions that the road
//! networks' mesher extrudes ([`crate::urban::mesh_chains`]) - curbs and
//! skirt, junction hubs, decks levelled across them.
//!
//! A carriageway is as wide as ATKIS says; with no width given, as wide as
//! its lanes, [`LANE_M`] each; with neither, as its kind of street
//! ([`fallback_width`]). A street whose carriageways run apart is drawn as
//! its carriageways, not as the line between them - which ATKIS marks as it
//! marks the carriageways themselves, so the two layers come in apart.
//!
//! Streets drape over the ground as a road network's do, but over Berlin's
//! water the ground is a carved bed - under its bridges too, which the
//! land use maps as street but the ground is settled as water - so there
//! they ride a copy of the ground with the water filled to
//! [`BRIDGE_DECK_M`] over its level: a bridge is a deck over the river, its
//! skirt the bridge's side ([`road_ground`]).
//!
//! The streets are Berlin's, derived on every visit and never saved; their
//! look is the theme's road palette.

use bevy_symbios_ground::HeightMap;
use geodata::berlin::{Dedication, StreetAxis};
use geodata::request::Bbox;

use crate::urban::{Chain, Dims, RoadParts};

/// How far over the water's level a bridge's ground is held (m): about the
/// height of the banks the water settles between.
pub(crate) const BRIDGE_DECK_M: f32 = 1.5;

/// A lane's width (m), for a carriageway ATKIS gives lanes but no width.
pub(crate) const LANE_M: f32 = 3.25;

/// How far inside the core's edge the streets end (m): their curbs and
/// end caps stay off the boundary walls.
pub(crate) const EDGE_MARGIN_M: f32 = 3.0;

/// The narrowest and widest deck half-width a street is drawn at (m).
const HALF_WIDTH_RANGE: (f32, f32) = (1.5, 12.0);

/// End points closer than this are one node (m): ATKIS's meet exactly, so
/// this only absorbs the rounding of the move into the heightmap's frame.
const WELD_M: f32 = 0.01;

/// The width a carriageway is drawn at when ATKIS gives neither its width
/// nor its lanes, by its kind of street (m).
pub(crate) fn fallback_width(dedication: Dedication) -> f32 {
    match dedication {
        Dedication::Motorway => 11.0,
        Dedication::Federal => 7.5,
        Dedication::State => 7.0,
        Dedication::District => 6.5,
        Dedication::Municipal => 5.5,
        Dedication::Other => 4.5,
    }
}

/// The deck half-width `axis` is drawn at (m).
fn half_width(axis: &StreetAxis) -> f32 {
    let width = axis
        .width
        .or_else(|| axis.lanes.map(|lanes| f32::from(lanes) * LANE_M))
        .unwrap_or_else(|| fallback_width(axis.dedication));
    (width / 2.0).clamp(HALF_WIDTH_RANGE.0, HALF_WIDTH_RANGE.1)
}

/// The core's frame: a heightmap of `grid` points `cell` metres apart over
/// the render box `bbox`, each point at its pixel's centre, row 0 north.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CoreFrame {
    pub bbox: Bbox,
    pub grid: u32,
    pub cell: f32,
}

impl CoreFrame {
    /// Heightmap-local `(x, z)` metres of E/N `[e, n]`.
    pub(crate) fn local(&self, [e, n]: [f64; 2]) -> (f32, f32) {
        let pixel = (self.bbox.max_e - self.bbox.min_e) as f64 / f64::from(self.grid);
        let cell = f64::from(self.cell);
        let x = ((e - self.bbox.min_e as f64) / pixel - 0.5) * cell;
        let z = ((self.bbox.max_n as f64 - n) / pixel - 0.5) * cell;
        (x as f32, z as f32)
    }

    /// The heightmap's extent (m): its first point to its last.
    pub(crate) fn extent(&self) -> f32 {
        (self.grid.saturating_sub(1)) as f32 * self.cell
    }
}

/// One stretch of street after the cut at the core's edge, in the core's
/// frame: its points, deck half-width, and for each end whether the edge
/// cut it there.
struct Stretch {
    pts: Vec<(f32, f32)>,
    half_w: f32,
    cut: [bool; 2],
}

/// Berlin's streets as fetched: the street axes, and the carriageways of
/// the streets whose carriageways run apart.
pub(crate) struct Streets {
    pub axes: Vec<StreetAxis>,
    pub carriageways: Vec<StreetAxis>,
}

impl Streets {
    /// The lines drawn as carriageways: every carriageway, and every street
    /// axis but those that run between a street's carriageways, and but a
    /// pedestrian zone's, which is no carriageway - the land use paints it
    /// as the stone it is.
    fn drawn(&self) -> impl Iterator<Item = &StreetAxis> {
        self.axes
            .iter()
            .filter(|a| !a.separated)
            .chain(&self.carriageways)
            .filter(|a| !a.pedestrian)
    }
}

impl Streets {
    /// Every segment of the drawn carriageways, in the core's `frame`.
    pub(crate) fn segments(&self, frame: CoreFrame) -> Vec<((f32, f32), (f32, f32))> {
        let mut segments = Vec::new();
        for line in self.drawn().flat_map(|axis| &axis.lines) {
            let pts: Vec<(f32, f32)> = line.iter().map(|&p| frame.local(p)).collect();
            segments.extend(pts.windows(2).map(|w| (w[0], w[1])));
        }
        segments
    }
}

/// The cells of the core's `frame` a drawn street's line passes over: where
/// the water may run on under a bridge ([`super::ground::water_mask`]).
pub(crate) fn street_cells(streets: &Streets, frame: CoreFrame) -> Vec<bool> {
    let side = frame.grid as usize;
    let mut cells = vec![false; side * side];
    let mut mark = |(x, z): (f32, f32)| {
        let (col, row) = ((x / frame.cell).round(), (z / frame.cell).round());
        if (0.0..side as f32).contains(&col) && (0.0..side as f32).contains(&row) {
            cells[row as usize * side + col as usize] = true;
        }
    };
    for axis in streets.drawn() {
        for line in &axis.lines {
            let pts: Vec<(f32, f32)> = line.iter().map(|&p| frame.local(p)).collect();
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                // Samples half a cell apart pass over every cell the line does.
                let steps = ((b.0 - a.0).hypot(b.1 - a.1) / (frame.cell * 0.5))
                    .ceil()
                    .max(1.0);
                for i in 0..=steps as usize {
                    let t = i as f32 / steps;
                    mark((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
                }
            }
        }
    }
    cells
}

/// The chains Berlin's `streets` make in `frame`, and every node's count of
/// streets meeting at it (see the module docs). The streets end
/// [`EDGE_MARGIN_M`] inside the core's edge; an end the edge cut is marked
/// as clipped, so the ribbon caps it.
pub(crate) fn street_chains(streets: &Streets, frame: CoreFrame) -> (Vec<Chain>, Vec<u32>) {
    let (lo, hi) = (EDGE_MARGIN_M, frame.extent() - EDGE_MARGIN_M);
    let mut stretches: Vec<Stretch> = Vec::new();
    if hi > lo {
        for axis in streets.drawn() {
            let half_w = half_width(axis);
            for line in &axis.lines {
                let pts: Vec<(f32, f32)> = line.iter().map(|&p| frame.local(p)).collect();
                for (pts, cut) in clip_to_square(&pts, lo, hi) {
                    stretches.push(Stretch { pts, half_w, cut });
                }
            }
        }
    }

    // Weld the ends into nodes; an end the edge cut is a node of its own.
    let mut nodes: Vec<(f32, f32)> = Vec::new();
    let mut ends: Vec<[usize; 2]> = Vec::with_capacity(stretches.len());
    let mut node_of = |p: (f32, f32), cut: bool| -> usize {
        if !cut
            && let Some(i) = nodes
                .iter()
                .position(|q| (q.0 - p.0).hypot(q.1 - p.1) <= WELD_M)
        {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    for s in &stretches {
        let first = node_of(s.pts[0], s.cut[0]);
        let last = node_of(s.pts[s.pts.len() - 1], s.cut[1]);
        ends.push([first, last]);
    }
    let mut degree = vec![0u32; nodes.len()];
    let mut at: Vec<Vec<(usize, usize)>> = vec![Vec::new(); nodes.len()];
    for (si, e) in ends.iter().enumerate() {
        for (slot, &node) in e.iter().enumerate() {
            degree[node] += 1;
            at[node].push((si, slot));
        }
    }

    // Join the stretches that meet two to a node into chains.
    let mut used = vec![false; stretches.len()];
    let mut chains = Vec::new();
    let mut walk = |start: usize, used: &mut [bool]| {
        // Run back from `start`'s first end to where the chain begins.
        let (mut si, mut slot) = (start, 0);
        loop {
            let node = ends[si][slot];
            if degree[node] != 2 {
                break;
            }
            let Some(&(next, next_slot)) = at[node].iter().find(|&&(s, _)| s != si) else {
                break;
            };
            if next == start {
                break; // a loop: begin anywhere
            }
            (si, slot) = (next, 1 - next_slot);
        }
        // Then forward, gathering the stretches.
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let (mut length, mut weighted) = (0.0f32, 0.0f32);
        let start_node = ends[si][slot];
        let start_cut = stretches[si].cut[slot];
        let (end_node, end_cut) = loop {
            used[si] = true;
            let s = &stretches[si];
            let run: Vec<(f32, f32)> = if slot == 0 {
                s.pts.clone()
            } else {
                s.pts.iter().rev().copied().collect()
            };
            let len: f32 = run
                .windows(2)
                .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
                .sum();
            length += len;
            weighted += len * s.half_w;
            for p in run {
                if pts
                    .last()
                    .is_none_or(|q: &(f32, f32)| (q.0 - p.0).hypot(q.1 - p.1) > WELD_M)
                {
                    pts.push(p);
                }
            }
            let far = 1 - slot;
            let node = ends[si][far];
            let next = (degree[node] == 2)
                .then(|| at[node].iter().find(|&&(s, _)| s != si).copied())
                .flatten()
                .filter(|&(s, _)| !used[s]);
            match next {
                Some((s, next_slot)) => (si, slot) = (s, next_slot),
                None => break (node, s.cut[far]),
            }
        };
        if pts.len() >= 2 && length > 0.0 {
            chains.push(Chain {
                pts,
                half_w: weighted / length,
                end_nodes: [start_node, end_node],
                clip: [start_cut, end_cut],
            });
        }
    };
    for start in 0..stretches.len() {
        if !used[start] {
            walk(start, &mut used);
        }
    }
    (chains, degree)
}

/// A part of a polyline the core's edge cut out: its points, and for each
/// end whether the edge cut it there.
type Cut = (Vec<(f32, f32)>, [bool; 2]);

/// The parts of the polyline `pts` inside the square `[lo, hi]` on both
/// axes, each with whether the square's edge cut it at its first and last
/// point.
fn clip_to_square(pts: &[(f32, f32)], lo: f32, hi: f32) -> Vec<Cut> {
    let inside = |p: (f32, f32)| (lo..=hi).contains(&p.0) && (lo..=hi).contains(&p.1);
    let mut parts = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let mut cut_first = false;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Liang-Barsky: the part of a..b inside the square, as t in [0, 1].
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        let mut visible = true;
        for (p, q) in [
            (-dx, a.0 - lo),
            (dx, hi - a.0),
            (-dz, a.1 - lo),
            (dz, hi - a.1),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    visible = false;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        if !visible || t0 > t1 {
            if !current.is_empty() {
                parts.push((std::mem::take(&mut current), [cut_first, true]));
            }
            continue;
        }
        let at = |t: f32| (a.0 + dx * t, a.1 + dz * t);
        if current.is_empty() {
            cut_first = t0 > 0.0 || !inside(a);
            current.push(at(t0));
        }
        current.push(at(t1));
        if t1 < 1.0 {
            parts.push((std::mem::take(&mut current), [cut_first, true]));
        }
    }
    if !current.is_empty() {
        parts.push((current, [cut_first, false]));
    }
    parts.retain(|(p, _)| p.len() >= 2);
    parts
}

/// The ground Berlin's streets drape over, `grid` x `grid` cells `cell`
/// metres apart: the higher of the core's terrain as Berlin draws it
/// (`raw`) and as it was settled (`settled`), and over every `wet` cell -
/// the water and the bridges over it - a span from bank to bank.
///
/// The settle eases the banks down to the water ([`geodata::water`]), where
/// Berlin's quays stand metres over it; a street draped on the eased bank
/// dipped at every bridge. So a street takes the raw height there, which
/// elsewhere the settle only ever raised. Over the water each cell takes
/// the height between the dry ground at the two ends of its run of wet
/// cells along its row or its column, the shorter run of the two - the
/// way a bridge crosses the river - and at least [`BRIDGE_DECK_M`] over the
/// water's `level`: a bridge is a deck from quay to quay.
pub(crate) fn road_ground(
    raw: &[f32],
    settled: &[f32],
    grid: u32,
    cell: f32,
    wet: &[bool],
    level: Option<f32>,
) -> HeightMap {
    let side = grid as usize;
    let mut ground = HeightMap::new(side, side, cell);
    for ((h, r), s) in ground.data_mut().iter_mut().zip(raw).zip(settled) {
        *h = r.max(*s);
    }
    let Some(level) = level else {
        return ground;
    };
    let floor = level + BRIDGE_DECK_M;
    let land = ground.data().to_vec();
    // Per wet cell, the shortest run through it bounded by dry ground at
    // both ends, and the height between those ends there.
    let mut span: Vec<Option<(usize, f32)>> = vec![None; side * side];
    let lines = (0..side)
        .map(|r| (r * side, 1))
        .chain((0..side).map(|c| (c, side)));
    for (first, step) in lines {
        let at = |i: usize| first + i * step;
        let mut i = 0;
        while i < side {
            if !wet[at(i)] {
                i += 1;
                continue;
            }
            let start = i;
            while i < side && wet[at(i)] {
                i += 1;
            }
            if start == 0 || i == side {
                continue;
            }
            let (from, to, len) = (land[at(start - 1)], land[at(i)], i - start);
            for j in start..i {
                let t = (j - start + 1) as f32 / (len + 1) as f32;
                if span[at(j)].is_none_or(|(shortest, _)| len < shortest) {
                    span[at(j)] = Some((len, from + (to - from) * t));
                }
            }
        }
    }
    for ((h, &wet), span) in ground.data_mut().iter_mut().zip(wet).zip(&span) {
        if wet {
            *h = span.map_or(floor, |(_, height)| height.max(floor));
        }
    }
    ground
}

/// Berlin's streets meshed on `ground` in `frame`, or `None` where no
/// street reaches the core.
pub(crate) fn mesh_streets(
    streets: &Streets,
    frame: CoreFrame,
    ground: &HeightMap,
) -> Option<RoadParts> {
    let (chains, degree) = street_chains(streets, frame);
    let widest = chains.iter().map(|c| c.half_w).fold(0.0f32, f32::max);
    if chains.is_empty() {
        return None;
    }
    let parts =
        crate::urban::mesh_chains(&chains, &degree, ground, [0, 0], &Dims::for_streets(widest));
    (!parts.deck.is_empty()).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/crates/geodata/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The Museumsinsel square's street and carriageway axes.
    fn museum_streets() -> Streets {
        let read = |name| geodata::berlin::parse_axes(&fixture(name)).unwrap().axes;
        Streets {
            axes: read("atkis_strassenachse_391200_5819700_600m.json"),
            carriageways: read("atkis_fahrbahnachse_391200_5819700_600m.json"),
        }
    }

    /// `axes`, as street axes with no carriageways of their own.
    fn streets(axes: Vec<StreetAxis>) -> Streets {
        Streets {
            axes,
            carriageways: Vec::new(),
        }
    }

    /// The square as the geo job's museum core: 300 points 2 m apart.
    fn museum_frame() -> CoreFrame {
        CoreFrame {
            bbox: Bbox {
                min_e: 391_200,
                min_n: 5_819_700,
                max_e: 391_800,
                max_n: 5_820_300,
            },
            grid: 300,
            cell: 2.0,
        }
    }

    fn axis(lines: Vec<Vec<[f64; 2]>>, width: f32) -> StreetAxis {
        StreetAxis {
            uuid: String::new(),
            lines,
            width: Some(width),
            lanes: None,
            separated: false,
            pedestrian: false,
            dedication: Dedication::Municipal,
        }
    }

    #[test]
    fn a_carriageway_is_as_wide_as_atkis_says_or_its_lanes_or_its_kind() {
        let mut a = axis(vec![vec![[0.0, 0.0], [1.0, 0.0]]], 9.0);
        assert_eq!(half_width(&a), 4.5);
        a.width = None;
        a.lanes = Some(2);
        assert_eq!(half_width(&a), LANE_M);
        a.lanes = None;
        assert_eq!(half_width(&a), fallback_width(Dedication::Municipal) / 2.0);
        a.width = Some(80.0);
        assert_eq!(half_width(&a), HALF_WIDTH_RANGE.1);
    }

    #[test]
    fn a_line_crossing_the_edge_is_cut_there_and_capped() {
        // Across the whole square and out, then back in at a corner.
        let parts = clip_to_square(&[(-10.0, 50.0), (50.0, 50.0), (150.0, 50.0)], 0.0, 100.0);
        assert_eq!(parts.len(), 1);
        let (pts, cut) = &parts[0];
        assert_eq!(pts.first(), Some(&(0.0, 50.0)));
        assert_eq!(pts.last(), Some(&(100.0, 50.0)));
        assert_eq!(*cut, [true, true]);
        // Inside all the way: nothing cut.
        let parts = clip_to_square(&[(10.0, 10.0), (20.0, 20.0)], 0.0, 100.0);
        assert_eq!(
            parts,
            vec![(vec![(10.0, 10.0), (20.0, 20.0)], [false, false])]
        );
        // Out and back in: two parts.
        let parts = clip_to_square(
            &[(10.0, 50.0), (120.0, 50.0), (120.0, 60.0), (90.0, 60.0)],
            0.0,
            100.0,
        );
        assert_eq!(parts.len(), 2);
        assert_eq!((parts[0].1, parts[1].1), ([false, true], [true, false]));
    }

    #[test]
    fn stretches_meeting_two_to_a_node_are_one_chain_and_junctions_end_them() {
        // A T: a through street of two stretches meeting at (100, 100), the
        // stem a third from there; and a street of two stretches bent at a
        // two-street node.
        let frame = CoreFrame {
            bbox: Bbox {
                min_e: 0,
                min_n: 0,
                max_e: 300,
                max_n: 300,
            },
            grid: 301,
            cell: 300.0 / 301.0,
        };
        let n = |e: f64, z: f64| [e, 300.0 - z];
        let axes = vec![
            axis(vec![vec![n(20.0, 100.0), n(100.0, 100.0)]], 8.0),
            axis(vec![vec![n(100.0, 100.0), n(200.0, 100.0)]], 8.0),
            axis(vec![vec![n(100.0, 100.0), n(100.0, 200.0)]], 6.0),
            axis(vec![vec![n(20.0, 250.0), n(150.0, 250.0)]], 6.0),
            axis(vec![vec![n(150.0, 250.0), n(250.0, 200.0)]], 10.0),
        ];
        let (chains, degree) = street_chains(&streets(axes), frame);
        assert_eq!(chains.len(), 4, "the T's three arms and the bent street");
        let junctions = degree.iter().filter(|&&d| d >= 3).count();
        assert_eq!(junctions, 1);
        // The bent street is one chain, its width the length-weighted mean.
        let bent = chains
            .iter()
            .find(|c| c.pts.len() == 3)
            .expect("one chain round the bend");
        let (l1, l2) = (130.0f32, (100.0f32).hypot(50.0));
        let mean = (l1 * 3.0 + l2 * 5.0) / (l1 + l2);
        assert!(
            (bent.half_w - mean).abs() < 0.05,
            "{} vs {mean}",
            bent.half_w
        );
        assert_eq!(bent.clip, [false, false]);
    }

    /// A street whose carriageways run apart is drawn as its carriageways:
    /// its own line is not, and theirs - marked as it is - are.
    #[test]
    fn a_boulevard_is_drawn_as_its_carriageways() {
        let museum = museum_streets();
        let frame = museum_frame();
        let (all, _) = street_chains(&museum, frame);
        let without = Streets {
            axes: museum.axes.clone(),
            carriageways: Vec::new(),
        };
        let (axes_only, _) = street_chains(&without, frame);
        assert!(
            all.len() > axes_only.len() + 5,
            "{} vs {}",
            all.len(),
            axes_only.len()
        );
        // The Liebknechtbruecke's carriageway runs over the Spree beside the
        // Dom: E 391606 N 5819950 to E 391578 N 5819933.
        let (bx, bz) = frame.local([391_592.0, 5_819_941.6]);
        let near = |c: &Chain| c.pts.iter().any(|p| (p.0 - bx).hypot(p.1 - bz) < 20.0);
        assert!(all.iter().any(near), "the bridge is drawn");
        assert!(
            !axes_only
                .iter()
                .any(|c| near(c) && (c.half_w - 2.75).abs() < 0.01)
        );
    }

    #[test]
    fn the_museumsinsel_streets_are_chains_inside_the_core() {
        let frame = museum_frame();
        let (chains, degree) = street_chains(&museum_streets(), frame);
        let (lo, hi) = (EDGE_MARGIN_M, frame.extent() - EDGE_MARGIN_M);
        assert!(chains.len() > 20, "{} chains", chains.len());
        for c in &chains {
            assert!(c.pts.iter().all(|p| (lo - 1e-3..=hi + 1e-3).contains(&p.0)
                && (lo - 1e-3..=hi + 1e-3).contains(&p.1)));
            assert!((HALF_WIDTH_RANGE.0..=HALF_WIDTH_RANGE.1).contains(&c.half_w));
            for (slot, &node) in c.end_nodes.iter().enumerate() {
                // A cut end is a dead end of its own; a junction has three.
                if c.clip[slot] {
                    assert_eq!(degree[node], 1);
                }
                assert_ne!(degree[node], 2, "a two-street node is inside a chain");
            }
        }
        assert!(degree.iter().filter(|&&d| d >= 3).count() >= 10);
        assert!(
            chains.iter().any(|c| c.clip.contains(&true)),
            "streets run off the core"
        );
    }

    /// The critic's finding (#1595): bridges sagged. The settle eases the
    /// banks down to the water and the decks spanned at a fixed height
    /// over it, so a street dipped a metre and more at every bridge. Now
    /// the quay keeps Berlin's height and the bridge spans quay to quay.
    #[test]
    fn the_museumsinsel_streets_mesh_and_their_bridges_span_the_spree() {
        let frame = museum_frame();
        let n = frame.grid as usize;
        // Flat ground at 33 m, the Spree a north-south band at 30.57 m:
        // as Berlin draws it, quays straight down to the water; as settled,
        // its bed carved and two cells of eased bank either side.
        let level = 30.57;
        let spree = |x: usize| (150..170).contains(&x);
        let bank = |x: usize| (148..150).contains(&x) || (170..172).contains(&x);
        let raw: Vec<f32> = (0..n * n)
            .map(|i| if spree(i % n) { level } else { 33.0 })
            .collect();
        let settled: Vec<f32> = (0..n * n)
            .map(|i| match i % n {
                x if spree(x) => level - 2.0,
                x if bank(x) => level + 0.6,
                _ => 33.0,
            })
            .collect();
        let wet: Vec<bool> = (0..n * n).map(|i| spree(i % n)).collect();
        let ground = road_ground(&raw, &settled, frame.grid, frame.cell, &wet, Some(level));
        assert_eq!(ground.data()[160], 33.0, "mid-river, quay to quay");
        assert_eq!(ground.data()[149], 33.0, "the quay keeps Berlin's height");
        assert_eq!(ground.data()[10], 33.0, "the land is the land");
        let parts = mesh_streets(&museum_streets(), frame, &ground).expect("streets");
        assert!(parts.chains > 20 && parts.junctions >= 10);
        let (triangles, surfaces) = parts.draw_cost();
        assert!(triangles > 1_000 && surfaces >= 2);
        // No deck over the river sags below its quays.
        let mesh = crate::urban::to_bevy_mesh(&parts.deck);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let over: Vec<f32> = points
            .iter()
            .filter(|p| spree((p[0] / frame.cell).round() as usize))
            .map(|p| p[1])
            .collect();
        assert!(over.len() > 20, "the streets cross the river");
        let lowest = over.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(lowest >= 33.0, "a deck over the river at {lowest}");
    }

    /// A street's line marks the cells it passes over, and only drawn
    /// streets do: not a pedestrian zone, nor the line between a
    /// boulevard's carriageways.
    #[test]
    fn a_drawn_street_marks_the_cells_it_crosses() {
        let frame = CoreFrame {
            bbox: Bbox {
                min_e: 0,
                min_n: 0,
                max_e: 100,
                max_n: 100,
            },
            grid: 100,
            cell: 1.0,
        };
        // E/N to the frame: x = E - 0.5, z = 99.5 - N.
        let mut street = axis(vec![vec![[10.5, 49.5], [60.5, 49.5]]], 6.0);
        let mut walk = axis(vec![vec![[10.5, 79.5], [60.5, 79.5]]], 6.0);
        walk.pedestrian = true;
        let mut middle = axis(vec![vec![[10.5, 19.5], [60.5, 19.5]]], 6.0);
        middle.separated = true;
        let cells = street_cells(&streets(vec![street.clone(), walk, middle]), frame);
        let row = |z: usize| (0..100).filter(|&x| cells[z * 100 + x]).count();
        assert_eq!(row(50), 51, "columns 10 to 60 on row 50");
        assert_eq!((row(20), row(80)), (0, 0));
        street.lines[0][1] = [10.5, 9.5];
        let cells = street_cells(&streets(vec![street]), frame);
        assert_eq!(
            cells.iter().filter(|&&c| c).count(),
            41,
            "a column, end to end"
        );
    }
}
