use super::*;
use crate::urban::Chain;
use crate::urban::test_support::*;
use bevy_symbios_ground::HeightMap;

/// #584 STEP-A lock: with no junction pins, `level_chain` is the #573 two-pass
/// upward grade limit — a flat floor stays flat, a dip is bridged UPWARD (never
/// buries) at no more than the longitudinal grade.
#[test]
fn level_chain_no_pins_keeps_the_floor() {
    let flat = vec![5.0; 5];
    let seg = vec![10.0; 4];
    assert_eq!(level_chain(&flat, &seg, [None, None]), flat);

    let dip = vec![5.0, 5.0, 0.0, 5.0, 5.0];
    let out = level_chain(&dip, &seg, [None, None]);
    for (i, &b) in out.iter().enumerate() {
        assert!(b >= dip[i] - 1.0e-6, "buried below the floor at {i}: {b}");
    }
    for i in 1..out.len() {
        assert!(
            (out[i - 1] - out[i]) <= MAX_LONGITUDINAL_GRADE * seg[i - 1] + 1.0e-4,
            "descends faster than the longitudinal grade at {i}"
        );
    }
}

/// #584: a junction pin ramps the deck back to its natural floor at the gentler
/// [`JUNCTION_APPROACH_GRADE`] — over a flat floor the height is exactly the
/// pin minus grade × arc-distance (until it meets the floor), so the transition
/// spreads over many frames, not a kick at the mouth.
#[test]
fn level_chain_pin_ramps_back_at_the_junction_grade() {
    let floor = vec![0.0; 30];
    let seg = vec![1.0; 29];
    let out = level_chain(&floor, &seg, [Some(5.0), None]);
    for (i, &b) in out.iter().enumerate() {
        let cone = (5.0 - JUNCTION_APPROACH_GRADE * i as f32).max(0.0);
        assert!(
            (b - cone).abs() < 1.0e-4,
            "frame {i}: {b} ≠ ramp cone {cone}"
        );
    }
}

/// #584: every road meeting a junction is levelled to ONE shared height — the
/// max incident mouth — and the deck never carves below its terrain floor. The
/// pass is deterministic (a pure function of its inputs).
#[test]
fn level_network_pins_all_arm_mouths_to_the_max() {
    let hm = HeightMap::new(64, 64, 2.0); // flat at 0 → seed = bias
    let chains = vec![
        Chain {
            pts: vec![(0.0, 0.0), (30.0, 0.0)],
            half_w: 4.0,
            end_nodes: [0, 1],
            clip: [false, false],
        },
        Chain {
            pts: vec![(0.0, 1.0), (0.0, 30.0)],
            half_w: 4.0,
            end_nodes: [0, 2],
            clip: [false, false],
        },
        Chain {
            pts: vec![(0.0, -1.0), (0.0, -30.0)],
            half_w: 4.0,
            end_nodes: [0, 3],
            clip: [false, false],
        },
    ];
    let samples = vec![
        Some(mk_sample(&[(0.0, 0.0), (30.0, 0.0)], &[1.0, 0.0])),
        Some(mk_sample(&[(0.0, 1.0), (0.0, 30.0)], &[2.0, 0.0])),
        Some(mk_sample(&[(0.0, -1.0), (0.0, -30.0)], &[3.0, 0.0])),
    ];
    let degree = vec![3u32, 1, 1, 1];
    let base = level_network(&chains, &samples, &degree, &hm);
    for (ci, b) in base.iter().enumerate() {
        assert!(
            (b[0] - 3.0).abs() < 1.0e-3,
            "arm {ci} mouth {} ≠ shared H 3.0",
            b[0]
        );
    }
    for (ci, b) in base.iter().enumerate() {
        for (i, &v) in b.iter().enumerate() {
            let floor = samples[ci].as_ref().unwrap().frames[i].floor;
            assert!(
                v >= floor - 1.0e-6,
                "arm {ci} frame {i} carved: {v} < floor {floor}"
            );
        }
    }
    assert_eq!(
        base,
        level_network(&chains, &samples, &degree, &hm),
        "not deterministic"
    );
}

/// #584: a connector between TWO junctions satisfies both — each junction's
/// incident mouths come out level, and raising the high junction propagates up
/// the low one through the connector (the relaxation's cross-junction coupling).
#[test]
fn level_network_two_junction_chain_levels_both_junctions() {
    let hm = HeightMap::new(64, 64, 2.0);
    let chains = vec![
        Chain {
            pts: vec![(0.0, 0.0), (40.0, 0.0)],
            half_w: 4.0,
            end_nodes: [0, 1],
            clip: [false, false],
        },
        Chain {
            pts: vec![(0.0, 0.0), (0.0, 30.0)],
            half_w: 4.0,
            end_nodes: [0, 2],
            clip: [false, false],
        },
        Chain {
            pts: vec![(0.0, 0.0), (0.0, -30.0)],
            half_w: 4.0,
            end_nodes: [0, 3],
            clip: [false, false],
        },
        Chain {
            pts: vec![(40.0, 0.0), (40.0, 30.0)],
            half_w: 4.0,
            end_nodes: [1, 4],
            clip: [false, false],
        },
        Chain {
            pts: vec![(40.0, 0.0), (40.0, -30.0)],
            half_w: 4.0,
            end_nodes: [1, 5],
            clip: [false, false],
        },
    ];
    let samples = vec![
        Some(mk_sample(&[(0.0, 0.0), (40.0, 0.0)], &[0.0, 0.0])),
        Some(mk_sample(&[(0.0, 0.0), (0.0, 30.0)], &[5.0, 0.0])),
        Some(mk_sample(&[(0.0, 0.0), (0.0, -30.0)], &[5.0, 0.0])),
        Some(mk_sample(&[(40.0, 0.0), (40.0, 30.0)], &[1.0, 0.0])),
        Some(mk_sample(&[(40.0, 0.0), (40.0, -30.0)], &[1.0, 0.0])),
    ];
    let degree = vec![3u32, 3, 1, 1, 1, 1];
    let base = level_network(&chains, &samples, &degree, &hm);
    let last = base[0].len() - 1;
    let (h0, h1) = (base[0][0], base[0][last]);
    // Both junctions are internally flat (all incident mouths share one height).
    assert!(
        (base[1][0] - h0).abs() < 1.0e-3 && (base[2][0] - h0).abs() < 1.0e-3,
        "junction 0 not level"
    );
    assert!(
        (base[3][0] - h1).abs() < 1.0e-3 && (base[4][0] - h1).abs() < 1.0e-3,
        "junction 1 not level"
    );
    // Junction 0 clears its 5.0 arms; junction 1 is pulled up above its 1.0 arms
    // by the connector ramping down from the high junction.
    assert!(h0 >= 5.0 - 1.0e-3, "junction 0 below its 5.0 arm: {h0}");
    assert!(h1 >= 1.0 - 1.0e-3, "junction 1 below its 1.0 arm: {h1}");
    assert!(h0 > h1, "the high junction should sit above the low one");
}
