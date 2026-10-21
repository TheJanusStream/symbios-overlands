use super::*;
use crate::urban::test_support::*;
use crate::urban::{Chain, Dims, plan_junctions};
use bevy_symbios_ground::HeightMap;

/// The junction plan of `chains` for these active `degree`s.
fn plan(chains: &[Chain], degree: &[u32]) -> JunctionPlan {
    plan_junctions(chains, degree, &Dims::from_config(&cfg(7)))
}

/// #584 STEP-A lock: with no junction pins, `level_chain` is the #573 two-pass
/// upward grade limit - a flat floor stays flat, a dip is bridged UPWARD (never
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
/// [`JUNCTION_APPROACH_GRADE`] - over a flat floor the height is exactly the
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

/// A road of `n` frames 1 m apart whose floor falls `grade` a metre from 20 m
/// at its first frame (a negative grade climbs): its floor and segment lengths.
fn falling(n: usize, grade: f32) -> (Vec<f32>, Vec<f32>) {
    let floor = (0..n).map(|i| 20.0 - grade * i as f32).collect();
    (floor, vec![1.0; n - 1])
}

/// The ramp #584 drew from a junction pinned at `pin` over the first frame of
/// a road 1 m a frame: the pin less [`JUNCTION_APPROACH_GRADE`] times the
/// distance, never below the floor.
fn absolute_ramp(floor: &[f32], pin: f32) -> Vec<f32> {
    floor
        .iter()
        .enumerate()
        .map(|(i, &f)| f.max(pin - JUNCTION_APPROACH_GRADE * i as f32))
        .collect()
}

/// #1737: a road falling away from a junction faster than the approach grade
/// sheds the junction's lift over its own floor and comes back down to it.
/// #584's absolute ramp held it at `pin - 0.09 d` for good: on ground falling
/// 0.1 a metre it stood 2 m up at its start and a metre higher every 100.
/// It never buries, never falls faster than the longitudinal grade, and once
/// down it stays on its floor.
#[test]
fn a_downhill_road_comes_back_down_to_its_floor() {
    let (floor, seg) = falling(80, 0.1);
    let pin = floor[0] + 2.0;
    let out = level_chain(&floor, &seg, [Some(pin), None]);
    assert_eq!(out[0], pin, "the mouth left its junction");
    for (i, (&b, &f)) in out.iter().zip(&floor).enumerate() {
        assert!(b >= f - 1.0e-6, "buried below the floor at {i}: {b} < {f}");
    }
    for i in 1..out.len() {
        assert!(
            out[i - 1] - out[i] <= MAX_LONGITUDINAL_GRADE * seg[i - 1] + 1.0e-4,
            "descends faster than the longitudinal grade at {i}"
        );
    }
    // 2 m shed at 0.18 - 0.10 = 0.08 a metre once the grade limit holds it: 25 m.
    let down = out
        .iter()
        .zip(&floor)
        .position(|(b, f)| b - f < 1.0e-4)
        .expect("never came back down to its floor");
    assert!(down <= 26, "came down to its floor only at frame {down}");
    for (i, (b, f)) in out.iter().zip(&floor).enumerate().skip(down) {
        assert!((b - f).abs() < 1.0e-4, "rose off its floor again at {i}");
    }
    let old = absolute_ramp(&floor, pin);
    assert!(
        old[79] - floor[79] > 2.7,
        "the fixture no longer shows #584's lift: {} over the floor at the end",
        old[79] - floor[79]
    );
}

/// #1737's twin turned round: the same road pinned at its far end, falling
/// toward its first frame, comes down as the mirror image.
#[test]
fn a_downhill_road_pinned_at_its_far_end_comes_down_alike() {
    let (floor, seg) = falling(80, 0.1);
    let pin = floor[0] + 2.0;
    let ahead = level_chain(&floor, &seg, [Some(pin), None]);
    let back: Vec<f32> = floor.iter().rev().copied().collect();
    let turned = level_chain(&back, &seg, [None, Some(pin)]);
    for (i, (a, b)) in ahead.iter().zip(turned.iter().rev()).enumerate() {
        assert!(
            (a - b).abs() < 1.0e-4,
            "frame {i}: {a} ahead, {b} turned round"
        );
    }
}

/// #1737 leaves alone what #584 had right: a flat road and a road climbing away
/// from its junction come out as the absolute ramp drew them, to the bit.
#[test]
fn a_flat_or_uphill_road_keeps_the_absolute_ramp() {
    for grade in [0.0_f32, -0.1] {
        let (floor, seg) = falling(60, grade);
        let pin = floor[0] + 3.0;
        assert_eq!(
            level_chain(&floor, &seg, [Some(pin), None]),
            absolute_ramp(&floor, pin),
            "a road climbing {} a metre",
            -grade
        );
    }
}

/// #1737's limit, pinned: on ground falling as fast as the longitudinal grade
/// the natural road already falls at it, so there is no grade left to shed a
/// junction's lift with - the lift runs on undiminished. Unlike #584's ramp,
/// which grew it by the approach grade a metre, it never grows.
#[test]
fn a_lift_on_ground_falling_at_the_longitudinal_grade_runs_on_undiminished() {
    let (floor, seg) = falling(80, 0.2);
    let natural = level_chain(&floor, &seg, [None, None]);
    let pin = natural[0] + 2.0;
    let out = level_chain(&floor, &seg, [Some(pin), None]);
    for (i, (b, n)) in out.iter().zip(&natural).enumerate() {
        assert!(
            (b - n - 2.0).abs() < 1.0e-3,
            "frame {i}: lift {} over the natural road",
            b - n
        );
    }
    let old = absolute_ramp(&natural, pin);
    assert!(
        old[79] - natural[79] > 8.0,
        "the fixture no longer shows #584's growing lift"
    );
}

/// #1737 on a road pinned at both ends over a ridge, its frames unevenly
/// spaced: each mouth leaves its own junction exactly, no frame sinks under the
/// natural road, and nothing falls faster than the longitudinal grade - though
/// each half runs downhill from the crest toward a junction that sheds its lift.
#[test]
fn a_ridge_road_pinned_at_both_ends_keeps_its_bounds() {
    let seg: Vec<f32> = (0..70)
        .map(|i| 0.6 + 0.9 * ((i * 7) % 5) as f32 / 4.0)
        .collect();
    let mut floor = vec![10.0_f32];
    for (i, &s) in seg.iter().enumerate() {
        let grade = if i < 35 { 0.08 } else { -0.12 };
        floor.push(floor[i] + grade * s);
    }
    let natural = level_chain(&floor, &seg, [None, None]);
    let (pin0, pin1) = (natural[0] + 1.5, natural[70] + 2.5);
    let out = level_chain(&floor, &seg, [Some(pin0), Some(pin1)]);
    assert_eq!(out[0], pin0, "the first mouth left its junction");
    assert_eq!(out[70], pin1, "the last mouth left its junction");
    for (i, (b, n)) in out.iter().zip(&natural).enumerate() {
        assert!(b >= n, "frame {i} under the natural road: {b} < {n}");
    }
    for i in 1..out.len() {
        let limit = MAX_LONGITUDINAL_GRADE * seg[i - 1] + 1.0e-4;
        assert!(
            out[i - 1] - out[i] <= limit,
            "falls too fast after frame {}",
            i - 1
        );
        assert!(
            out[i] - out[i - 1] <= limit,
            "falls too fast before frame {i}"
        );
    }
}

/// #1737 across a network: a junction high on a slope no longer lifts the next
/// one down it. The connector between them falls 6 m over 60 m from 9 m at the
/// high junction, which its 10 m roads hold at 10 m. #584 held the connector's
/// low mouth at 10 - 0.09 x 60 = 4.6 m and so lifted the low junction 1.6 m
/// off its 3 m roads, and them with it; now the connector sheds the lift and
/// the low junction stands at its roads' own height.
#[test]
fn a_high_junction_no_longer_lifts_the_next_one_down_the_slope() {
    // Flat at 0: every junction's own ground (0.2 with the bias) lies under its roads.
    let hm = HeightMap::new(64, 64, 2.0);
    let road = |a: (f32, f32), b: (f32, f32), ends: [usize; 2]| Chain {
        pts: vec![a, b],
        half_w: 4.0,
        end_nodes: ends,
        clip: [false, false],
    };
    let chains = vec![
        road((0.0, 0.0), (60.0, 0.0), [0, 1]),
        road((0.0, 0.0), (0.0, 30.0), [0, 2]),
        road((0.0, 0.0), (0.0, -30.0), [0, 3]),
        road((60.0, 0.0), (60.0, 30.0), [1, 4]),
        road((60.0, 0.0), (60.0, -30.0), [1, 5]),
    ];
    let centres: Vec<(f32, f32)> = (0..=60).map(|i| (i as f32, 0.0)).collect();
    let fall: Vec<f32> = (0..=60).map(|i| 9.0 - 0.1 * i as f32).collect();
    let samples = vec![
        Some(mk_sample(&centres, &fall)),
        Some(mk_sample(&[(0.0, 0.0), (0.0, 30.0)], &[10.0, 10.0])),
        Some(mk_sample(&[(0.0, 0.0), (0.0, -30.0)], &[10.0, 10.0])),
        Some(mk_sample(&[(60.0, 0.0), (60.0, 30.0)], &[3.0, 3.0])),
        Some(mk_sample(&[(60.0, 0.0), (60.0, -30.0)], &[3.0, 3.0])),
    ];
    let degree = vec![3u32, 3, 1, 1, 1, 1];
    let plan = plan(&chains, &degree);
    let base = level_network(&chains, &samples, &plan, &[], &hm);
    let last = base[0].len() - 1;
    assert!(
        (base[0][0] - 10.0).abs() < 1.0e-3,
        "the high junction left its 10 m roads: {}",
        base[0][0]
    );
    assert!(
        (base[0][last] - 3.0).abs() < 1.0e-3,
        "the connector lifted the low junction to {}",
        base[0][last]
    );
    for ci in [3, 4] {
        assert!(
            (base[ci][0] - 3.0).abs() < 1.0e-3,
            "road {ci} at the low junction lifted to {}",
            base[ci][0]
        );
    }
}

/// #584: every road meeting a junction is levelled to ONE shared height - the
/// max incident mouth - and the deck never carves below its terrain floor. The
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
    let plan = plan(&chains, &degree);
    let base = level_network(&chains, &samples, &plan, &[], &hm);
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
        level_network(&chains, &samples, &plan, &[], &hm),
        "not deterministic"
    );
}

/// #584: a connector between TWO junctions satisfies both - each junction's
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
    let plan = plan(&chains, &degree);
    let base = level_network(&chains, &samples, &plan, &[], &hm);
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
