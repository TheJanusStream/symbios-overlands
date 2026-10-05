use super::*;
use crate::pds::generator::RoadConfig;
use crate::urban::test_support::*;
use crate::urban::{
    Chain, Dims, RoadParts, active_degree, build_road_graph, drawn_graph, extract_chains,
    extrude_hubs,
};
use bevy_symbios_ground::HeightMap;

/// The planned pull-backs of `chains` when exactly the nodes in `junctions`
/// are junctions (degree 3) and every other node a dead end.
fn trims_with(chains: &[Chain], junctions: &[usize], dims: &Dims) -> Vec<[f32; 2]> {
    let top = chains
        .iter()
        .flat_map(|c| c.end_nodes)
        .max()
        .map_or(0, |m| m + 1);
    let degree: Vec<u32> = (0..top)
        .map(|nd| if junctions.contains(&nd) { 3 } else { 1 })
        .collect();
    plan_junctions(chains, &degree, dims).trims
}

/// #575: a clean orthogonal cross truncates every arm by exactly the outer
/// footprint half-width `wo` - the adjacent-boundary solve's closed form for
/// right-angle arms - while the non-junction far ends are left untrimmed.
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
    let trims = trims_with(&chains, &[0], &dims);
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
    let trims = trims_with(&chains, &[], &dims);
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

/// #575: truncation never changes the geometry's determinism - the same
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
    let a = trims_with(&mk(), &[0], &dims);
    let b = trims_with(&mk(), &[0], &dims);
    assert_eq!(a, b, "truncation must be deterministic");
}

/// #575: an acute fork would need an unbounded pull-back (the boundary
/// crossing runs to infinity as the branch angle → 0); the cap keeps it at a
/// width-relative maximum so the hub never becomes a long flat gore.
#[test]
fn truncation_caps_an_acute_fork() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let cap = MAX_TRUNCATION_FACTOR * (w + dims.curb_top_width + dims.chamfer_width);
    // Two arms leaving node 0 ~5° apart - a sliver fork. Long arms (60 m) so
    // the baseline heading is unambiguous and nothing else trims them.
    let ang = 5.0_f32.to_radians();
    let arm = |a: f32, far: usize| Chain {
        pts: vec![(0.0, 0.0), (a.cos() * 60.0, a.sin() * 60.0)],
        half_w: w,
        end_nodes: [0, far],
        clip: [false, false],
    };
    let chains = [arm(0.0, 1), arm(ang, 2)];
    let trims = trims_with(&chains, &[0], &dims);
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
/// truncates to `wo`. (This is the commonest real junction - the fallback is
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
    let trims = trims_with(&chains, &[0], &dims);
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
/// Pins the floor branch - the dominant organic-junction regime - which a
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
    // 90° / 210° / 330° - three arms 120° apart.
    let chains = [arm(90.0, 1), arm(210.0, 2), arm(330.0, 3)];
    let trims = trims_with(&chains, &[0], &dims);
    for (ci, t) in trims.iter().enumerate() {
        assert!(
            (t[0] - w).abs() < 1.0e-3,
            "wide-Y arm {ci} trim {} ≠ half_w floor {w}",
            t[0]
        );
    }
    assert!(w < wo, "sanity: the floor sits below the outer footprint");
}

/// #1558 (was #575's keep-a-stub clamp): a connector between two junctions
/// shorter than its combined pull-back is drawn inside ONE hub - the two
/// junctions merge, the connector becomes the hub's link, and every street
/// leaving either junction opens into it. The clamp it replaces kept a 1 m
/// stub of ribbon and two hubs whose corners interleaved over each other's
/// mouths (the shards seen at dense junctions).
#[test]
fn a_short_junction_connector_merges_its_junctions_into_one_hub() {
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
        chain(vec![(30.0, 30.0), (33.0, 30.0)], [0, 1]), // the short connector
        chain(vec![(30.0, 30.0), (10.0, 10.0)], [0, 2]),
        chain(vec![(30.0, 30.0), (10.0, 50.0)], [0, 3]),
        chain(vec![(33.0, 30.0), (53.0, 10.0)], [1, 4]),
        chain(vec![(33.0, 30.0), (53.0, 50.0)], [1, 5]),
    ];
    let mut degree = vec![0u32; 6];
    degree[0] = 3;
    degree[1] = 3;
    degree[2..].fill(1);
    let plan = plan_junctions(&chains, &degree, &dims);
    assert!(plan.internal[0], "the connector is drawn inside the hub");
    assert_eq!(plan.hubs.len(), 1, "the two junctions are one hub");
    assert_eq!(plan.hubs[0].nodes, vec![0, 1]);
    assert_eq!(plan.hubs[0].links.len(), 1, "joined by the connector");
    assert_eq!(
        plan.hubs[0].arms.len(),
        4,
        "with all four streets leaving it"
    );

    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        if plan.internal[ci] {
            continue;
        }
        let [s, e] = plan.trims[ci];
        let sample = crate::urban::sample_chain(c, s, e, &hm, &dims).expect("a long arm meshes");
        let floor: Vec<f32> = sample.frames.iter().map(|r| r.floor).collect();
        let base = crate::urban::level_chain(&floor, &sample.seg, [None, None]);
        crate::urban::extrude_ribbon(
            c,
            &sample,
            &base,
            [0.0; 2],
            &dims,
            plan.chain_ends(ci),
            &mut road_ends,
            &mut parts,
        );
    }
    assert_eq!(road_ends.len(), 4, "every street records its mouth");
    let deck_before = parts.deck.indices.len();
    extrude_hubs(&road_ends, &plan.hubs, [0.0; 2], &dims, &mut parts);
    assert!(
        parts.deck.indices.len() > deck_before,
        "the merged hub grew no deck"
    );
}

/// #575 regression on the real pilot network (review wf_e27b3d8b-91d measured
/// 12 of 45 junctions losing their hub before the clamp): replays the
/// mesher's mouth collection and asserts every hub records exactly the arms
/// the plan gave it - no arm is silently trimmed out of existence, so no
/// real intersection is left a hole.
#[test]
fn pilot_junctions_keep_every_mouth_after_truncation() {
    use std::collections::BTreeMap;
    let hm = pilot_heightmap();
    let config = cfg(PILOT_ROAD_SEED);
    let (graph, sub, _lo) = build_road_graph(&hm, &config, None).expect("pilot must trace");
    let dims = Dims::from_config(&config);
    let drawn = drawn_graph(&graph, &sub);
    let chains = extract_chains(&drawn, &sub, &dims);
    let plan = plan_junctions(&chains, &active_degree(&drawn), &dims);

    let expected: BTreeMap<usize, usize> = plan
        .hubs
        .iter()
        .enumerate()
        .map(|(h, hub)| (h, hub.arms.len()))
        .collect();
    let mut road_ends = Vec::new();
    let mut parts = RoadParts::default();
    for (ci, c) in chains.iter().enumerate() {
        if plan.internal[ci] {
            continue;
        }
        let [s, e] = plan.trims[ci];
        if let Some(sample) = crate::urban::sample_chain(c, s, e, &sub, &dims) {
            let floor: Vec<f32> = sample.frames.iter().map(|r| r.floor).collect();
            let base = crate::urban::level_chain(&floor, &sample.seg, [None, None]);
            crate::urban::extrude_ribbon(
                c,
                &sample,
                &base,
                [0.0; 2],
                &dims,
                plan.chain_ends(ci),
                &mut road_ends,
                &mut parts,
            );
        }
    }
    let mut recorded: BTreeMap<usize, usize> = BTreeMap::new();
    for r in &road_ends {
        *recorded.entry(r.hub).or_default() += 1;
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

/// The widest any hub of `plan` spans (m): the furthest apart two of its
/// junction nodes stand.
fn widest_hub(plan: &JunctionPlan) -> f32 {
    plan.hubs
        .iter()
        .flat_map(|h| {
            h.points.iter().flat_map(|a| {
                h.points
                    .iter()
                    .map(move |b| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt())
            })
        })
        .fold(0.0, f32::max)
}

/// The span cap of a hub under `dims` (m): two pull-backs at their cap and
/// a ribbon's worth, the longest chain one swallow takes.
fn span_cap(dims: &Dims) -> f32 {
    let widest =
        dims.major_half_width.max(dims.minor_half_width) + dims.curb_top_width + dims.chamfer_width;
    2.0 * MAX_TRUNCATION_FACTOR * widest + MIN_RIBBON_LEN_M
}

/// #1558: a run of junctions 3 m apart, every connector too short to carry
/// its pull-backs, used to be swallowed into one hub however long the run;
/// a hub now stops at MAX_HUB_NODES nodes, and the connector that would
/// grow it further is drawn as a short ribbon between two hubs, its
/// pull-backs shrunk to leave MIN_RIBBON_LEN_M of it.
#[test]
fn a_hub_never_grows_past_its_cap() {
    let dims = Dims::from_config(&cfg(7));
    let w = dims.minor_half_width;
    let chain = |pts: Vec<(f32, f32)>, ends: [usize; 2]| Chain {
        pts,
        half_w: w,
        end_nodes: ends,
        clip: [false, false],
    };
    // Junctions 0..12 at x = 30 + 3k along z = 60, each with a dead-end arm
    // to alternate sides, and a dead end beyond either end of the run.
    let n = 12;
    let x = |k: usize| 30.0 + 3.0 * k as f32;
    let mut chains: Vec<Chain> = (0..n - 1)
        .map(|k| chain(vec![(x(k), 60.0), (x(k + 1), 60.0)], [k, k + 1]))
        .collect();
    for k in 0..n {
        let z = if k % 2 == 0 { 30.0 } else { 90.0 };
        chains.push(chain(vec![(x(k), 60.0), (x(k), z)], [k, n + k]));
    }
    chains.push(chain(vec![(x(0), 60.0), (0.0, 60.0)], [0, 2 * n]));
    chains.push(chain(
        vec![(x(n - 1), 60.0), (100.0, 60.0)],
        [n - 1, 2 * n + 1],
    ));
    let mut degree = vec![1u32; 2 * n + 2];
    degree[..n].fill(3);
    let plan = plan_junctions(&chains, &degree, &dims);

    let largest = plan.hubs.iter().map(|h| h.nodes.len()).max().unwrap_or(0);
    assert_eq!(largest, MAX_HUB_NODES, "the run fills one hub to its cap");
    assert_eq!(
        plan.hubs.iter().map(|h| h.nodes.len()).sum::<usize>(),
        n,
        "and every junction of the run is in a hub"
    );
    let held: Vec<usize> = (0..chains.len())
        .filter(|&ci| plan.short_ribbon[ci])
        .collect();
    assert_eq!(
        held,
        vec![MAX_HUB_NODES - 1],
        "one connector is held out of it"
    );
    let ci = held[0];
    assert!(!plan.internal[ci], "the held connector is drawn");
    let [s, e] = plan.trims[ci];
    assert!(
        s > 0.0 && e > 0.0 && (3.0 - s - e - MIN_RIBBON_LEN_M).abs() < 1.0e-4,
        "its pull-backs {s} + {e} leave a {MIN_RIBBON_LEN_M} m ribbon of its 3 m"
    );
}

/// #1558: on a dense grid (10 x 8 m blocks, the editor's least spacing)
/// every block side is too short to carry its pull-backs, and the swallow
/// used to pave the district over as one hub - with the default widths
/// the swallowed minor streets chained each column into one, with 20 m
/// half-widths the whole grid. Now no hub passes its cap and much of the
/// grid is still drawn as streets.
#[test]
fn a_dense_grid_is_drawn_as_small_hubs_not_one_plate() {
    use crate::pds::types::Fp;
    let xs: Vec<f32> = (0..13).map(|k| 68.0 + 10.0 * k as f32).collect();
    let zs: Vec<f32> = (0..15).map(|k| 72.0 + 8.0 * k as f32).collect();
    let nodes: Vec<(f32, f32)> = zs
        .iter()
        .flat_map(|&z| xs.iter().map(move |&x| (x, z)))
        .collect();
    let (cols, rows) = (xs.len() as u32, zs.len() as u32);
    let mut edges = Vec::new();
    for r in 0..rows {
        for c in 0..cols - 1 {
            edges.push((r * cols + c, r * cols + c + 1, true));
        }
    }
    for c in 0..cols {
        for r in 0..rows - 1 {
            edges.push((r * cols + c, (r + 1) * cols + c, false));
        }
    }
    let graph = typed_graph(&nodes, &edges);
    let sub = HeightMap::new(128, 128, 2.0);
    for (major, minor) in [(3.5, 2.0), (20.0, 20.0)] {
        let config = RoadConfig {
            major_half_width: Fp(major),
            minor_half_width: Fp(minor),
            ..cfg(7)
        };
        let dims = Dims::from_config(&config);
        let drawn = drawn_graph(&graph, &sub);
        let chains = extract_chains(&drawn, &sub, &dims);
        let plan = plan_junctions(&chains, &active_degree(&drawn), &dims);
        let largest = plan.hubs.iter().map(|h| h.nodes.len()).max().unwrap_or(0);
        assert!(
            largest <= MAX_HUB_NODES,
            "{major}/{minor} m: a hub covers {largest} junctions"
        );
        assert!(
            widest_hub(&plan) <= span_cap(&dims),
            "{major}/{minor} m: a hub spans {} m",
            widest_hub(&plan)
        );
        let drawn_chains = plan.internal.iter().filter(|&&i| !i).count();
        assert!(
            4 * drawn_chains >= chains.len(),
            "{major}/{minor} m: {drawn_chains} of {} streets drawn",
            chains.len()
        );
        assert!(
            plan.short_ribbon.iter().any(|&s| s),
            "{major}/{minor} m: the cap held no street out (the fixture lost its point)"
        );
        let parts = crate::urban::mesh_road_graph(&graph, &sub, [0, 0], &dims);
        for g in [&parts.deck, &parts.structure] {
            assert!(
                g.vertices.iter().flatten().all(|v| v.is_finite()),
                "{major}/{minor} m: a vertex is not finite"
            );
        }
        assert!(!parts.deck.is_empty(), "{major}/{minor} m: no deck");
    }
}

/// #1558: the editor allows a 20/12 m spacing, on which the swallow used to
/// merge most of a traced district into a few hubs of up to 345 junction
/// nodes, flat plates over the blocks. No hub passes its cap now, and most
/// of the streets are still drawn.
#[test]
fn a_dense_traced_plan_draws_no_hub_past_its_cap() {
    use crate::pds::types::Fp;
    let hm = pilot_heightmap();
    let config = RoadConfig {
        major_spacing: Fp(20.0),
        minor_spacing: Fp(12.0),
        ..cfg(PILOT_ROAD_SEED)
    };
    let (graph, sub, _lo) = build_road_graph(&hm, &config, None).expect("traces");
    let dims = Dims::from_config(&config);
    let drawn = drawn_graph(&graph, &sub);
    let chains = extract_chains(&drawn, &sub, &dims);
    let plan = plan_junctions(&chains, &active_degree(&drawn), &dims);
    let largest = plan.hubs.iter().map(|h| h.nodes.len()).max().unwrap_or(0);
    assert!(largest <= MAX_HUB_NODES, "a hub covers {largest} junctions");
    assert!(widest_hub(&plan) <= span_cap(&dims));
    let drawn_chains = plan.internal.iter().filter(|&&i| !i).count();
    assert!(
        3 * drawn_chains >= chains.len(),
        "{drawn_chains} of {} streets drawn",
        chains.len()
    );
}
