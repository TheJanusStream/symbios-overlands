use super::*;
use crate::urban::test_support::*;
use crate::urban::{Chain, Dims, RoadParts, build_road_graph, extract_chains, extrude_hubs};
use bevy_symbios_ground::HeightMap;

/// #575: a clean orthogonal cross truncates every arm by exactly the outer
/// footprint half-width `wo` — the adjacent-boundary solve's closed form for
/// right-angle arms — while the non-junction far ends are left untrimmed.
#[test]
fn truncation_pulls_arms_back_at_an_orthogonal_cross() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    // Four arms leaving junction node 0 along ±x / ±z; the far ends (nodes
    // 1..4) are dead-ends, so only the slot-0 (junction) end truncates.
    let arm = |to: (f32, f32), far: usize| Chain {
        pts: vec![
            (0.0, 0.0),
            (to.0 * 10.0, to.1 * 10.0),
            (to.0 * 40.0, to.1 * 40.0),
        ],
        half_w: w,
        end_nodes: [0, far],
        clip: [false, false],
    };
    let chains = [
        arm((1.0, 0.0), 1),
        arm((-1.0, 0.0), 2),
        arm((0.0, 1.0), 3),
        arm((0.0, -1.0), 4),
    ];
    let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
    for (ci, t) in trims.iter().enumerate() {
        assert!(
            (t[0] - wo).abs() < 1.0e-3,
            "arm {ci} start trim {} ≠ wo {wo}",
            t[0]
        );
        assert_eq!(t[1], 0.0, "non-junction far end of arm {ci} must not trim");
    }
}

/// #575: with no junction ends, nothing truncates (every trim is zero).
#[test]
fn truncation_skips_non_junction_ends() {
    let dims = Dims::from_config(&cfg(7));
    let chains = [Chain {
        pts: vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)],
        half_w: dims.minor_half_width,
        end_nodes: [0, 1],
        clip: [false, false],
    }];
    // No node is a junction → no pull-back anywhere.
    let trims = compute_truncations(&chains, |_| false, &dims);
    assert_eq!(trims, vec![[0.0, 0.0]]);
}

/// #575: `trim_polyline` removes arc length from each end, interpolating the
/// cut points, and keeps the interior vertices that survive.
#[test]
fn trim_polyline_shortens_both_ends() {
    let pts = vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
    let out = trim_polyline(&pts, 3.0, 4.0);
    assert!(
        (out[0].0 - 3.0).abs() < 1.0e-4,
        "start cut at x=3, got {out:?}"
    );
    assert!(
        (out.last().unwrap().0 - 16.0).abs() < 1.0e-4,
        "end cut at x=16, got {out:?}"
    );
    // The mid vertex (x=10) lies inside (3, 16) → retained.
    assert!(out.iter().any(|p| (p.0 - 10.0).abs() < 1.0e-4));
}

/// #575: a chain shorter than the combined pull-back is wholly consumed by
/// the hubs and grows no ribbon (fewer than two points back).
#[test]
fn trim_polyline_consumes_short_chain() {
    let pts = vec![(0.0, 0.0), (5.0, 0.0)];
    assert!(trim_polyline(&pts, 4.0, 4.0).len() < 2);
}

/// #575: truncation never changes the geometry's determinism — the same
/// chains yield byte-identical pull-backs each run.
#[test]
fn truncation_is_deterministic() {
    let dims = Dims::from_config(&cfg(7));
    let mk = || {
        let arm = |to: (f32, f32), far: usize| Chain {
            pts: vec![(0.0, 0.0), (to.0 * 12.0, to.1 * 12.0)],
            half_w: dims.major_half_width,
            end_nodes: [0, far],
            clip: [false, false],
        };
        [
            arm((1.0, 0.2), 1),
            arm((-0.3, 1.0), 2),
            arm((-0.7, -0.7), 3),
        ]
    };
    let a = compute_truncations(&mk(), |nd| nd == 0, &dims);
    let b = compute_truncations(&mk(), |nd| nd == 0, &dims);
    assert_eq!(a, b, "truncation must be deterministic");
}

/// #575: an acute fork would need an unbounded pull-back (the boundary
/// crossing runs to infinity as the branch angle → 0); the cap keeps it at a
/// width-relative maximum so the chains survive for the merge pass (#578).
#[test]
fn truncation_caps_an_acute_fork() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let cap = MAX_TRUNCATION_FACTOR * (w + dims.curb_top_width + dims.chamfer_width);
    // Two arms leaving node 0 ~5° apart — a sliver fork. Long arms (60 m) so
    // the baseline heading is unambiguous and nothing else trims them.
    let ang = 5.0_f32.to_radians();
    let arm = |a: f32, far: usize| Chain {
        pts: vec![(0.0, 0.0), (a.cos() * 60.0, a.sin() * 60.0)],
        half_w: w,
        end_nodes: [0, far],
        clip: [false, false],
    };
    let chains = [arm(0.0, 1), arm(ang, 2)];
    let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
    for (ci, t) in trims.iter().enumerate() {
        assert!(
            t[0].is_finite() && t[0] <= cap + 1.0e-3,
            "acute arm {ci} pull-back {} exceeded the cap {cap}",
            t[0]
        );
    }
    // The fork is acute enough that at least one arm is pinned to the cap
    // (proving the bound actually engaged, not a coincidentally small solve).
    assert!(
        trims.iter().any(|t| (t[0] - cap).abs() < 1.0e-3),
        "cap never engaged on a 5° fork: {trims:?}"
    );
}

/// #575: a T-junction's straight through road is two anti-parallel adjacent
/// arms, so its 2×2 boundary solve is singular and takes the parallel
/// fallback `(w_a + w_b)/2 = wo`. Every arm (through pair + side street)
/// truncates to `wo`. (This is the commonest real junction — the fallback is
/// load-bearing, so it gets its own pin.)
#[test]
fn truncation_handles_a_t_junction_through_pair() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    // Through road ±x with a side street +z, meeting node 0. Long arms so the
    // floor/clamp never interfere.
    let arm = |to: (f32, f32), far: usize| Chain {
        pts: vec![(0.0, 0.0), (to.0 * 40.0, to.1 * 40.0)],
        half_w: w,
        end_nodes: [0, far],
        clip: [false, false],
    };
    let chains = [arm((1.0, 0.0), 1), arm((-1.0, 0.0), 2), arm((0.0, 1.0), 3)];
    let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
    for (ci, t) in trims.iter().enumerate() {
        assert!(
            (t[0] - wo).abs() < 1.0e-3,
            "T-junction arm {ci} trim {} ≠ wo {wo}",
            t[0]
        );
    }
}

/// #575: a wide-open 120° Y is so splayed the adjacent-boundary solve returns
/// *less* than the half-width floor, so every arm pins to `half_w` (not `wo`).
/// Pins the floor branch — the dominant organic-junction regime — which a
/// dropped floor-init would silently under-truncate.
#[test]
fn truncation_floors_a_wide_y_at_the_half_width() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let wo = w + dims.curb_top_width + dims.chamfer_width;
    let arm = |deg: f32, far: usize| {
        let a = deg.to_radians();
        Chain {
            pts: vec![(0.0, 0.0), (a.cos() * 40.0, a.sin() * 40.0)],
            half_w: w,
            end_nodes: [0, far],
            clip: [false, false],
        }
    };
    // 90° / 210° / 330° — three arms 120° apart.
    let chains = [arm(90.0, 1), arm(210.0, 2), arm(330.0, 3)];
    let trims = compute_truncations(&chains, |nd| nd == 0, &dims);
    for (ci, t) in trims.iter().enumerate() {
        assert!(
            (t[0] - w).abs() < 1.0e-3,
            "wide-Y arm {ci} trim {} ≠ half_w floor {w}",
            t[0]
        );
    }
    assert!(w < wo, "sanity: the floor sits below the outer footprint");
}

/// #575 regression (review wf_e27b3d8b-91d): a short connector between two
/// junctions is shorter than its combined pull-back, so before the clamp it
/// trimmed to nothing and BOTH junctions silently lost an arm — and a
/// mouth-driven hub with < 3 arms is dropped entirely (a hole at a real
/// intersection). The clamp keeps a meshable stub, so each junction still
/// records all three mouths and grows its hub.
#[test]
fn short_junction_connector_keeps_both_hubs() {
    let dims = Dims::from_config(&cfg(7));
    let hm = HeightMap::new(64, 64, 2.0);
    let w = dims.minor_half_width;
    let chain = |pts: Vec<(f32, f32)>, ends: [usize; 2]| Chain {
        pts,
        half_w: w,
        end_nodes: ends,
        clip: [false, false],
    };
    // Two degree-3 junctions (nodes 0, 1) 3 m apart, each with two splayed
    // dead-end arms; the connector abuts a junction at both ends.
    let chains = [
        chain(vec![(0.0, 0.0), (3.0, 0.0)], [0, 1]), // the short connector
        chain(vec![(0.0, 0.0), (-20.0, -20.0)], [0, 2]),
        chain(vec![(0.0, 0.0), (-20.0, 20.0)], [0, 3]),
        chain(vec![(3.0, 0.0), (23.0, -20.0)], [1, 4]),
        chain(vec![(3.0, 0.0), (23.0, 20.0)], [1, 5]),
    ];
    let mut degree = vec![0u32; 6];
    degree[0] = 3;
    degree[1] = 3;
    for d in degree.iter_mut().skip(2) {
        *d = 1;
    }
    let trims = compute_truncations(&chains, |nd| degree[nd] >= 3, &dims);
    // The connector keeps at least the floor (not consumed).
    let surviving = 3.0 - (trims[0][0] + trims[0][1]);
    assert!(
        surviving + 1.0e-4 >= MIN_RIBBON_LEN_M,
        "connector consumed: only {surviving} m left"
    );

    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        extrude_chain(
            c,
            s,
            e,
            &hm,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
    }
    let arms_at = |n: usize| road_ends.iter().filter(|r| r.node == n).count();
    assert_eq!(arms_at(0), 3, "node 0 lost an arm to over-truncation");
    assert_eq!(arms_at(1), 3, "node 1 lost an arm to over-truncation");

    extrude_hubs(&road_ends, &hm, 0.0, &dims, &mut parts);
    assert!(
        !parts.deck.is_empty(),
        "both junctions failed to grow a hub"
    );
}

/// #575 regression on the real pilot network (review wf_e27b3d8b-91d measured
/// 12 of 45 junctions losing their hub before the clamp): replays
/// `build_road_geometry`'s mouth collection and asserts every junction keeps
/// exactly the mouths its incident chains carry — no arm is silently trimmed
/// out of existence, so no real intersection is left a hole.
#[test]
fn pilot_junctions_keep_every_mouth_after_truncation() {
    use std::collections::BTreeMap;
    let hm = pilot_heightmap();
    let config = cfg(PILOT_ROAD_SEED);
    let (graph, sub, _lo) = build_road_graph(&hm, &config).expect("pilot must trace");
    let dims = Dims::from_config(&config);
    let chains = extract_chains(&graph, &sub, &dims);

    let mut degree = vec![0u32; graph.nodes.len()];
    for e in &graph.edges {
        if e.active {
            degree[e.start as usize] += 1;
            degree[e.end as usize] += 1;
        }
    }
    let is_junction = |nd: usize| degree.get(nd).copied().unwrap_or(0) >= 3;
    let trims = compute_truncations(&chains, is_junction, &dims);

    // Mouths each junction *should* carry = chain ends abutting a degree≥3 node.
    let mut expected: BTreeMap<usize, usize> = BTreeMap::new();
    for c in &chains {
        for &nd in &c.end_nodes {
            if is_junction(nd) {
                *expected.entry(nd).or_default() += 1;
            }
        }
    }
    // Mouths actually recorded after truncation + trimming.
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        let [s, e] = trims[ci];
        extrude_chain(
            c,
            s,
            e,
            &sub,
            0.0,
            &dims,
            &degree,
            &mut road_ends,
            &mut parts,
        );
    }
    let mut recorded: BTreeMap<usize, usize> = BTreeMap::new();
    for r in &road_ends {
        *recorded.entry(r.node).or_default() += 1;
    }

    assert_eq!(
        recorded, expected,
        "truncation dropped a junction mouth on the pilot network"
    );
    // Sanity: the pilot really does exercise multi-arm junctions (so the
    // assertion above is non-vacuous).
    assert!(
        expected.values().filter(|&&c| c >= 3).count() > 10,
        "pilot expected to have many real junctions"
    );
}
