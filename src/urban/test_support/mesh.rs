//! Whole-mesh soundness checks shared by the road tests (#1558, #1567):
//! every deck triangle faces up and none overlaps another (a fold), no curb
//! lies over the asphalt, and nothing reaches past any street's curb line.

use bevy_symbios_ground::HeightMap;
use symbios_tensor::RoadGraph;

use crate::urban::math::{cross, sub3};
use crate::urban::test_support::cfg;
use crate::urban::{Dims, RoadGeometry, RoadParts, mesh_road_graph};

/// The triangles of one surface, as XZ-plane corner triples with their
/// stored winding.
pub(crate) fn triangles(g: &RoadGeometry) -> Vec<[[f32; 3]; 3]> {
    g.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|i| g.vertices[i as usize]))
        .collect()
}

/// Twice the signed XZ area of a triangle, positive when its stored winding
/// faces up (+Y).
pub(crate) fn up_area2(t: &[[f32; 3]; 3]) -> f32 {
    cross(sub3(t[1], t[0]), sub3(t[2], t[0]))[1]
}

/// Whether two XZ triangles' interiors overlap - a separating-axis test on
/// copies shrunk 1 mm toward their centroids, so triangles that merely share
/// an edge or a corner never count.
pub(crate) fn interiors_overlap(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> bool {
    let shrink = |t: &[[f32; 3]; 3]| {
        let c = [
            (t[0][0] + t[1][0] + t[2][0]) / 3.0,
            (t[0][2] + t[1][2] + t[2][2]) / 3.0,
        ];
        t.map(|p| {
            let (dx, dz) = (p[0] - c[0], p[2] - c[1]);
            let l = dx.hypot(dz).max(1.0e-6);
            let k = ((l - 1.0e-3) / l).max(0.0);
            [c[0] + dx * k, c[1] + dz * k]
        })
    };
    let (pa, pb) = (shrink(a), shrink(b));
    for poly in [&pa, &pb] {
        for i in 0..3 {
            let (p, q) = (poly[i], poly[(i + 1) % 3]);
            let axis = [q[1] - p[1], p[0] - q[0]];
            let proj = |t: &[[f32; 2]; 3]| {
                let v: Vec<f32> = t.iter().map(|c| c[0] * axis[0] + c[1] * axis[1]).collect();
                (
                    v.iter().copied().fold(f32::INFINITY, f32::min),
                    v.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                )
            };
            let ((a0, a1), (b0, b1)) = (proj(&pa), proj(&pb));
            if a1 <= b0 || b1 <= a0 {
                return false;
            }
        }
    }
    true
}

/// The deck of a hub (or a whole mesh) is sound: every triangle finite,
/// non-degenerate and wound to face up, and no two overlapping (a fold).
pub(crate) fn assert_deck_sound(deck: &RoadGeometry, what: &str) {
    let tris = triangles(deck);
    assert!(!tris.is_empty(), "{what}: no deck");
    for t in &tris {
        assert!(
            t.iter().flatten().all(|c| c.is_finite()),
            "{what}: non-finite deck triangle {t:?}"
        );
        let a2 = up_area2(t);
        assert!(
            a2 > 2.0e-4,
            "{what}: deck triangle degenerate or facing down (2·area {a2}): {t:?}"
        );
    }
    for (i, a) in tris.iter().enumerate() {
        for b in &tris[i + 1..] {
            assert!(
                !interiors_overlap(a, b),
                "{what}: deck triangles overlap (a fold): {a:?} / {b:?}"
            );
        }
    }
    for n in &deck.normals {
        assert!(n[1] > 0.0, "{what}: deck normal {n:?} not upward");
    }
}

/// No curb top or chamfer (structure at or above the deck height `deck_y`)
/// lies over the asphalt: no shard poking out of the deck.
pub(crate) fn assert_no_curb_over_deck(parts: &RoadParts, deck_y: f32, what: &str) {
    let deck = triangles(&parts.deck);
    for s in triangles(&parts.structure) {
        if s.iter().any(|p| p[1] < deck_y - 1.0e-2) || up_area2(&s).abs() < 1.0e-4 {
            continue; // a skirt, a bottom cap, or a vertical wall
        }
        for d in &deck {
            assert!(
                !interiors_overlap(&s, d),
                "{what}: curb over the asphalt: {s:?} over deck {d:?}"
            );
        }
    }
}

/// Every vertex of the mesh lies within the outer footprint (deck, curb,
/// chamfer) of some street of `graph`: no curb or skirt past the curb line.
pub(crate) fn assert_within_footprints(
    parts: &RoadParts,
    graph: &RoadGraph,
    dims: &Dims,
    what: &str,
) {
    let outer = dims.curb_top_width + dims.chamfer_width;
    for v in parts.deck.vertices.iter().chain(&parts.structure.vertices) {
        let gap = graph
            .edges
            .iter()
            .filter(|e| e.active)
            .map(|e| {
                let (a, b) = (
                    graph.nodes[e.start as usize].position,
                    graph.nodes[e.end as usize].position,
                );
                let (ab, av) = ([b.x - a.x, b.y - a.y], [v[0] - a.x, v[2] - a.y]);
                let t = ((av[0] * ab[0] + av[1] * ab[1])
                    / (ab[0] * ab[0] + ab[1] * ab[1]).max(1.0e-9))
                .clamp(0.0, 1.0);
                let w = match e.road_type {
                    symbios_tensor::RoadType::Major => dims.major_half_width,
                    symbios_tensor::RoadType::Minor => dims.minor_half_width,
                } + outer;
                (av[0] - ab[0] * t).hypot(av[1] - ab[1] * t) - w
            })
            .fold(f32::INFINITY, f32::min);
        assert!(
            gap <= 0.05,
            "{what}: vertex {v:?} lies {gap} m past every curb line"
        );
    }
}

/// Mesh `graph` over a flat 256 m window (the junctions near its centre,
/// well inside the district interior) and check the whole deck is sound -
/// every triangle up, none folded or overlapping another, ribbons included
/// unless `ribbons_may_overlap` (a pair too sharp for any pull-back) - with
/// no curb over the asphalt and nothing past any street's curb line.
pub(crate) fn assert_network_sound(
    graph: &RoadGraph,
    what: &str,
    ribbons_may_overlap: bool,
) -> RoadParts {
    let dims = Dims::from_config(&cfg(7));
    let sub = HeightMap::new(128, 128, 2.0);
    let parts = mesh_road_graph(graph, &sub, [0, 0], &dims);
    let deck_y = parts
        .deck
        .vertices
        .iter()
        .map(|v| v[1])
        .fold(f32::MIN, f32::max);
    for v in &parts.deck.vertices {
        assert!(
            (v[1] - deck_y).abs() < 1.0e-3,
            "{what}: flat ground grew an unlevel deck {v:?}"
        );
    }
    if ribbons_may_overlap {
        let tris = triangles(&parts.deck);
        for t in &tris {
            assert!(
                up_area2(t) > 2.0e-4,
                "{what}: degenerate or downward deck {t:?}"
            );
        }
    } else {
        assert_deck_sound(&parts.deck, what);
        assert_no_curb_over_deck(&parts, deck_y, what);
    }
    assert_within_footprints(&parts, graph, &dims, what);
    parts
}
