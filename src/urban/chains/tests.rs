use super::*;

/// #582: when a run is cut because the next sampled node falls outside the
/// district interior, that end is flagged `clip`; the run's other end, a real
/// graph terminus, is not.
#[test]
fn push_interior_runs_marks_a_boundary_clip_end() {
    // Nodes 0,1,2 inside (x<100); node 3 outside → the run clips at the rim.
    let pos = |i: usize| (i as f32 * 40.0, 0.0); // 0, 40, 80, 120
    let inside = |x: f32, _z: f32| x < 100.0;
    let mut out = Vec::new();
    push_interior_runs(&[0, 1, 2, 3], &pos, &inside, 5.0, &mut out);
    assert_eq!(out.len(), 1, "one interior sub-run");
    assert_eq!(out[0].end_nodes, [0, 2]);
    assert_eq!(
        out[0].clip,
        [false, true],
        "start is a real terminus, end is clipped at the rim"
    );
}

/// #582 (review risk: re-entrant bookkeeping): a street that dips out of the
/// interior and back in (in→out→in) yields two runs whose INNER ends — both at
/// the rim — are clipped, while the outer ends stay real termini. `prev_outside`
/// must be function-local so the second run picks up the clipped start.
#[test]
fn push_interior_runs_reentrant_street_marks_both_inner_ends() {
    let xs = [0.0_f32, 40.0, 120.0, 200.0, 240.0]; // node 2 outside the band
    let pos = move |i: usize| (xs[i], 0.0);
    let inside = |x: f32, _z: f32| x < 100.0 || x > 150.0;
    let mut out = Vec::new();
    push_interior_runs(&[0, 1, 2, 3, 4], &pos, &inside, 5.0, &mut out);
    assert_eq!(out.len(), 2, "two interior sub-runs straddling the gap");
    assert_eq!(out[0].end_nodes, [0, 1]);
    assert_eq!(out[0].clip, [false, true], "first run clips at its rim end");
    assert_eq!(out[1].end_nodes, [3, 4]);
    assert_eq!(
        out[1].clip,
        [true, false],
        "second run clips at its rim start"
    );
}

/// #582 (review risk: first-node-outside): a walked chain whose FIRST node is
/// outside must not flush an empty run nor mis-set the flag, but the following
/// run must still pick up the clipped start from `prev_outside`.
#[test]
fn push_interior_runs_leading_outside_marks_start_clip() {
    let xs = [120.0_f32, 0.0, 40.0, 80.0]; // node 0 outside, then 1,2,3 inside
    let pos = move |i: usize| (xs[i], 0.0);
    let inside = |x: f32, _z: f32| x < 100.0;
    let mut out = Vec::new();
    push_interior_runs(&[0, 1, 2, 3], &pos, &inside, 5.0, &mut out);
    assert_eq!(
        out.len(),
        1,
        "one interior sub-run after the leading-outside node"
    );
    assert_eq!(out[0].end_nodes, [1, 3]);
    assert_eq!(
        out[0].clip,
        [true, false],
        "leading-outside → clipped start, real end"
    );
}
