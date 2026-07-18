use super::*;
use crate::urban::test_support::*;

/// #583: a degree-1 dead-end ending transversely a few metres off a through-road
/// welds in — the touched edge splits at the foot-of-perpendicular, a real
/// (degree-3) junction appears there, and the dead-end becomes a degree-2 node.
#[test]
fn weld_creates_junction_for_real_near_miss() {
    // Through-road 0—1 along +x; shaft 2→3 drops toward it, tip 3 is 5 m above.
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 20.0), (50.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(active_degrees(&g)[3], 1, "node 3 starts as a dead-end");
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        1,
        "the near-miss dead-end should weld"
    );
    let deg = active_degrees(&g);
    assert_eq!(deg[3], 2, "the welded dead-end becomes a through node");
    let junction = (0..g.nodes.len())
        .find(|&i| deg[i] == 3)
        .expect("a real degree-3 junction must appear");
    let p = g.nodes[junction].position;
    assert!(
        (p.x - 50.0).abs() < 1.0e-3 && p.y.abs() < 1.0e-3,
        "junction sits at the foot of perpendicular (50,0), got {p:?}"
    );
}

/// #583: a dead-end running NEAR-PARALLEL to a road (crossing angle below
/// WELD_MIN_CROSS_ANGLE) is a graze, not a junction — it must NOT weld (the
/// additive twin of the #571 graze cut must not re-introduce false junctions).
#[test]
fn graze_is_left_alone() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (30.0, 3.0), (70.0, 3.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a parallel graze must not weld"
    );
}

/// #583: a genuine cul-de-sac ending in open space (no edge within tolerance) is
/// left for the #579 cap — the weld only fires when another road is near.
#[test]
fn true_cul_de_sac_is_left_alone() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 50.0), (50.0, 30.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a far cul-de-sac (30 m off) must not weld"
    );
}

/// #583: a dead-end whose foot lands in the outer margin of a segment (near an
/// endpoint) is a near-NODE case owned by merge_coincident_nodes — it must NOT
/// split the edge mid-span.
#[test]
fn endpoint_near_node_is_not_welded() {
    // Tip 3 is 5 m off the road but its foot is at t≈0.02 (< WELD_T_MARGIN).
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (2.0, 25.0), (2.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "a near-endpoint foot must not split the edge"
    );
}

/// #583: a dead-end must never weld onto its OWN chain (a hairpin curling back
/// near an earlier segment of the same road) — the self-chain guard excludes it.
#[test]
fn self_chain_is_not_welded() {
    // Chain 0—1—2—3—4: edge 0—1 lies at y=0; the tip 4=(5,3) comes back 3 m
    // above it, but 0—1 is part of 4's own chain.
    let mut g = weld_graph(
        &[
            (0.0, 0.0),
            (50.0, 0.0),
            (50.0, 30.0),
            (5.0, 30.0),
            (5.0, 3.0),
        ],
        &[(0, 1), (1, 2), (2, 3), (3, 4)],
    );
    assert_eq!(active_degrees(&g)[4], 1, "node 4 is the dead-end");
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "the only nearby edge is the dead-end's own chain → no weld"
    );
    assert_eq!(active_degrees(&g)[4], 1, "node 4 stays a dead-end");
}

/// #583: welding is idempotent — a second pass over an already-welded graph
/// welds nothing (the dead-end is now a degree-2 through node).
#[test]
fn weld_is_idempotent() {
    let mut g = weld_graph(
        &[(0.0, 0.0), (100.0, 0.0), (50.0, 20.0), (50.0, 5.0)],
        &[(0, 1), (2, 3)],
    );
    assert_eq!(weld_endpoint_dangles(&mut g, 8.0), 1);
    assert_eq!(
        weld_endpoint_dangles(&mut g, 8.0),
        0,
        "second pass welds nothing"
    );
}

/// #890: the style presets genuinely change the traced topology — a Grid
/// district on sloped terrain differs from the Hillside default — while the
/// forward-compat `Unknown` arm traces exactly as Hillside.
#[test]
fn road_style_changes_the_traced_graph() {
    use crate::pds::generator::RoadStyle;
    let hm = sloped_heightmap();
    let graph_for = |style: RoadStyle| {
        let mut c = cfg(7);
        c.style = style;
        build_road_graph_raw(&hm, &c).map(|(g, _, _)| {
            (
                g.nodes.len(),
                g.nodes
                    .iter()
                    .map(|n| (n.position.x, n.position.y))
                    .collect::<Vec<_>>(),
            )
        })
    };
    let hillside = graph_for(RoadStyle::Hillside).expect("hillside traces");
    let grid = graph_for(RoadStyle::Grid).expect("grid traces");
    let organic = graph_for(RoadStyle::Organic).expect("organic traces");
    let unknown = graph_for(RoadStyle::Unknown).expect("unknown traces");
    assert_eq!(unknown, hillside, "Unknown must trace as Hillside");
    assert_ne!(grid.1, hillside.1, "Grid must reshape the network");
    assert_ne!(organic.1, hillside.1, "Organic must reshape the network");
}
