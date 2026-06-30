use super::*;
use crate::urban::Dims;
use crate::urban::test_support::*;
use crate::urban::truncation::MAX_TRUNCATION_FACTOR;

/// The road-graph diagnostic must run on the pilot network and report
/// internally-consistent counts — a guard for the filtering work that
/// reads these numbers to size thresholds.
#[test]
fn road_graph_diagnostics_reports_consistent_stats() {
    let hm = pilot_heightmap();
    let dims = Dims::from_config(&cfg(PILOT_ROAD_SEED));
    let diag = road_graph_diagnostics(&hm, &cfg(PILOT_ROAD_SEED))
        .expect("pilot network must yield diagnostics");
    for stats in [&diag.raw, &diag.sanitized] {
        // The degree histogram partitions every node exactly once.
        let hist_sum: usize = stats.degree_hist.values().sum();
        assert_eq!(
            hist_sum, stats.nodes,
            "degree histogram must cover all nodes"
        );
        // Spurious-hub sub-counts are each a subset of all hubs.
        assert!(stats.hubs_spurious <= stats.hubs_total);
        assert!(stats.hubs_with_stub <= stats.hubs_total);
        assert!(stats.hubs_collinear_graze <= stats.hubs_total);
        assert!(stats.hubs_near_duplicate <= stats.hubs_total);
        // Spike-risk vertices are a subset of densified vertices, and the
        // builder's miter clamp (3.0) is never exceeded.
        assert!(stats.spike_vertices <= stats.densified_vertices);
        assert!(
            stats.spike_max_scale <= 3.0 + 1.0e-3,
            "miter scale clamp is 3.0"
        );
        // One sample per hub / per spur.
        assert_eq!(stats.hub_min_branch_angle.len(), stats.hubs_total);
        assert_eq!(stats.spur_lengths.len(), stats.dead_ends_total);
        // Every reported truncation is a finite, positive pull-back bounded
        // by the per-class cap (#575) — the dump can't render NaN or a
        // cap-escape from a degenerate fan.
        let trunc_cap = MAX_TRUNCATION_FACTOR
            * (dims.major_half_width + dims.curb_top_width + dims.chamfer_width);
        for &t in &stats.truncation_dists {
            assert!(t.is_finite() && t > 0.0, "bad truncation distance {t}");
            assert!(
                t <= trunc_cap + 1.0e-3,
                "truncation {t} exceeds the cap {trunc_cap}"
            );
        }
    }
    // WS1 acceptance, adjusted for the #583 weld: sanitation still only removes
    // the spurious-hub / spike-risk artefacts it targets and the merge leaves
    // no near-duplicate nodes. The weld is ADDITIVE (splits an edge + adds a
    // connector, ≤ 2 active edges each) but only ever CLOSES near-miss dead-ends
    // — so the within-tolerance near-miss population only shrinks, and the edge
    // count grows by no more than the weld contribution (bounded by the raw
    // candidate count).
    assert!(diag.sanitized.hubs_spurious <= diag.raw.hubs_spurious);
    // EXACT weld count: only `split_edge` adds a node during sanitation, exactly
    // one per weld (merge and cuts never add nodes) — a tight basis for the
    // additive bounds, independent of any sweep tolerance.
    let welds = diag.sanitized.nodes - diag.raw.nodes;
    // Each weld adds at most 2 active edges (split nets +1, connector +1); cuts
    // only remove. So edge growth is bounded tightly by 2 per weld.
    assert!(
        diag.sanitized.edges_active <= diag.raw.edges_active + 2 * welds,
        "edge growth {} exceeds 2 per weld over raw {} ({welds} welds)",
        diag.sanitized.edges_active,
        diag.raw.edges_active
    );
    // Each weld adds at most ONE spike-risk vertex (the bend where the welded arm
    // meets its junction — the split point is collinear, the junction a chain
    // end). The 3.0 miter clamp (asserted per-stats above) still caps the worst
    // spike, so a weld is never sharper than the existing network.
    assert!(
        diag.sanitized.spike_vertices <= diag.raw.spike_vertices + welds,
        "spike-risk {} grew beyond one per weld over raw {} ({welds} welds)",
        diag.sanitized.spike_vertices,
        diag.raw.spike_vertices
    );
    // The weld only ever CLOSES near-miss dead-ends, never creates them.
    assert!(
        diag.sanitized.near_miss_dangles[0] <= diag.raw.near_miss_dangles[0],
        "weld must not increase near-miss dead-ends"
    );
    assert_eq!(
        diag.sanitized.coincident_pairs, 0,
        "merge must leave no near-duplicate (non-adjacent) nodes"
    );
    // The report renders without panicking and labels the room.
    assert!(diag.report("pilot").contains("road-graph diagnostics"));
}
