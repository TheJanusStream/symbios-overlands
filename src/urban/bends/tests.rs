use super::*;
use crate::urban::{RIBBON_STEP_M, densify, frame_right};

/// The outer half-width (deck, curb and chamfer) of a major street at the
/// default dimensions: 3.5 m of deck, 0.62 m of curb and chamfer.
const MAJOR_OUTER: f32 = 4.12;
/// ... and of a minor street: 2 m of deck.
const MINOR_OUTER: f32 = 2.62;

/// The point `len` metres from `from` along compass-free angle `deg`
/// (counter-clockwise from +x in `(x, z)`).
fn step(from: (f32, f32), deg: f32, len: f32) -> (f32, f32) {
    let a = deg.to_radians();
    (from.0 + a.cos() * len, from.1 + a.sin() * len)
}

/// The owner's first corner (#1567): a major street turning 117.2 degrees
/// at one vertex, `(20, 0)`, between straight legs of 13.4 m and 17.0 m.
fn major_hairpin() -> Vec<(f32, f32)> {
    let v = (20.0, 0.0);
    vec![(20.0 - 13.4, 0.0), v, step(v, 117.2, 17.0)]
}

/// The owner's second corner: a minor street turning -116.5 degrees at `a`,
/// 6.2 m from its start, then a further -31.5 degrees at `b`, 3.1 m on -
/// the short leg the two bends share.
fn minor_hairpin() -> Vec<(f32, f32)> {
    let p = (0.0, 0.0);
    let a = step(p, 0.0, 6.2);
    let b = step(a, -116.5, 3.1);
    vec![p, a, b, step(b, -148.0, 8.0)]
}

/// The signed turn (radians) at each interior point of `pts`.
fn turns(pts: &[(f32, f32)]) -> Vec<f32> {
    pts.windows(3)
        .map(|w| {
            let u = [w[1].0 - w[0].0, w[1].1 - w[0].1];
            let v = [w[2].0 - w[1].0, w[2].1 - w[1].1];
            cross2(u, v).atan2(dot2(u, v))
        })
        .collect()
}

fn gap(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The distance from `p` to the line through `a` and `b`, and how far
/// along it from `a` its foot lies.
fn off_line(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (d, _) = unit_leg(a, b);
    let ap = [p.0 - a.0, p.1 - a.1];
    (cross2(d, ap).abs(), dot2(d, ap))
}

#[test]
fn a_chain_with_no_sharp_bend_comes_back_as_it_went_in() {
    // Bends of 11.3 degrees each way: under one arc step, so mitred.
    let pts = vec![(0.0, 0.0), (10.0, 0.0), (20.0, 2.0), (30.0, 2.0)];
    assert_eq!(round_bends(&pts, MAJOR_OUTER), pts);
}

/// The first corner as the arc it is drawn as: tangent to both legs, its
/// chords mitred out to the outer half-width, and nowhere a turn sharper
/// than one arc step - so no frame's mitre comes near the 1.92 the single
/// vertex had.
#[test]
fn a_hairpin_is_drawn_as_an_arc_tangent_to_both_legs() {
    let pts = major_hairpin();
    let out = round_bends(&pts, MAJOR_OUTER);
    assert_eq!(out.first(), pts.first(), "the start never moves");
    assert_eq!(out.last(), pts.last(), "the end never moves");
    for t in turns(&out) {
        assert!(
            t.abs() <= BEND_STEP_RAD + 1.0e-4,
            "a turn of {} degrees is left",
            t.to_degrees()
        );
    }

    let turn = 117.2_f32.to_radians();
    let steps = (turn / BEND_STEP_RAD).ceil();
    let radius = MAJOR_OUTER / (turn / steps * 0.5).cos();
    let tangent = radius * (turn * 0.5).tan();
    // The arc leaves leg 1 and joins leg 2 a tangent length from the vertex.
    let (start, end) = (out[1], out[out.len() - 2]);
    assert!((gap(start, pts[1]) - tangent).abs() < 1.0e-3);
    assert!(
        off_line(start, pts[0], pts[1]).0 < 1.0e-4,
        "the arc starts on leg 1"
    );
    assert!((gap(end, pts[1]) - tangent).abs() < 1.0e-3);
    assert!(
        off_line(end, pts[1], pts[2]).0 < 1.0e-4,
        "the arc ends on leg 2"
    );
    // Every point between lies on one circle of that radius, its centre
    // `radius` in from where the arc leaves leg 1.
    let centre = (start.0, start.1 + radius);
    for &p in &out[1..out.len() - 1] {
        assert!(
            (gap(p, centre) - radius).abs() < 2.0e-3,
            "{p:?} off the arc: {} from its centre, radius {radius}",
            gap(p, centre)
        );
    }
}

/// The second corner: its first bend wants more of the 3.1 m leg than the
/// leg has once the second bend has its share, so the two split it in
/// proportion and their arcs meet on it at one radius - one curve round the
/// whole 148 degrees, tighter than the outer half-width.
#[test]
fn bends_sharing_a_short_leg_split_it_and_meet_on_it() {
    let pts = minor_hairpin();
    let out = round_bends(&pts, MINOR_OUTER);
    assert_eq!(out.first(), pts.first());
    assert_eq!(out.last(), pts.last());
    for t in turns(&out) {
        assert!(t.abs() <= BEND_STEP_RAD + 1.0e-4, "turn {}", t.to_degrees());
    }
    // Exactly one point of the rounded chain lies on the short leg between
    // its ends: where the two arcs meet, each tangent to it there.
    let meet: Vec<(f32, f32)> = out
        .iter()
        .copied()
        .filter(|&p| {
            let (off, along) = off_line(p, pts[1], pts[2]);
            off < 1.0e-4 && along > 1.0e-3 && along < 3.1 - 1.0e-3
        })
        .collect();
    assert_eq!(
        meet.len(),
        1,
        "the arcs meet once on the short leg: {meet:?}"
    );
    // An arc's tangent length over tan(half its turn) is its radius: the leg
    // is shared out whole, at one radius (to within the two arcs' chord
    // mitres), tighter than the outer half-width it has no room for.
    let (ta, tb) = (gap(meet[0], pts[1]), gap(meet[0], pts[2]));
    let ra = ta / (116.5_f32.to_radians() * 0.5).tan();
    let rb = tb / (31.5_f32.to_radians() * 0.5).tan();
    assert!(
        (ta + tb - 3.1).abs() < 1.0e-3,
        "the leg is shared out whole"
    );
    assert!(
        (ra - rb).abs() < 0.01 * ra,
        "the two arcs differ in radius: {ra} and {rb}"
    );
    assert!(
        ra < MINOR_OUTER,
        "the leg leaves no room for the full radius"
    );
}

/// A bend near a chain's end leaves the end leg's last half metre straight,
/// so the end frame still faces along it - the heading its hub mouth or end
/// cap is built on.
#[test]
fn an_end_leg_keeps_its_straight() {
    let v = (2.0, 0.0);
    let pts = vec![(0.0, 0.0), v, step(v, 100.0, 20.0)];
    let out = round_bends(&pts, MAJOR_OUTER);
    assert!(out.len() > 3, "the bend is rounded");
    assert!(
        (gap(out[0], out[1]) - END_STRAIGHT_M).abs() < 1.0e-4,
        "the end keeps {END_STRAIGHT_M} m of straight, got {}",
        gap(out[0], out[1])
    );
    assert!(
        off_line(out[1], pts[0], pts[1]).0 < 1.0e-5,
        "along the end leg"
    );
}

/// One rail of the profile `lateral` metres right of `pts`, mitred as the
/// ribbon mitres it, with the frames' centres.
fn mitred_rail(pts: &[(f32, f32)], lateral: f32) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
    let frames = densify(pts, RIBBON_STEP_M);
    let centre: Vec<[f32; 2]> = frames.iter().map(|p| [p.0, p.1]).collect();
    let rail = (0..frames.len())
        .map(|i| {
            let (rx, rz, scale) = frame_right(&frames, i);
            [
                frames[i].0 + rx * lateral * scale,
                frames[i].1 + rz * lateral * scale,
            ]
        })
        .collect();
    (rail, centre)
}

/// Whether any step of `rail` runs backwards against its frames.
fn any_backwards(rail: &[[f32; 2]], centre: &[[f32; 2]]) -> bool {
    (0..rail.len() - 1).any(|s| runs_backwards(rail[s], rail[s + 1], centre[s], centre[s + 1]))
}

/// The point the lines `lateral` metres right of two legs (`a`→`v`,
/// `v`→`b`) meet in - the corner a mitre draws on the inside of a bend.
fn inner_corner(a: (f32, f32), v: (f32, f32), b: (f32, f32), lateral: f32) -> [f32; 2] {
    let (u, _) = unit_leg(a, v);
    let (w, _) = unit_leg(v, b);
    // Right of travel in (x, z) is (-dz, dx), as the ribbon's frames have it.
    let p = [a.0 - u[1] * lateral, a.1 + u[0] * lateral];
    let q = [v.0 - w[1] * lateral, v.1 + w[0] * lateral];
    let den = cross2(u, w);
    let t = cross2(sub2(q, p), w) / den;
    [p[0] + u[0] * t, p[1] + u[1] * t]
}

/// The fold itself (#1567): the first corner mitred, unrounded, its frames
/// 2.7-2.8 m apart - the inner outer-curb line runs backwards past the
/// vertex, and once unfolded meets in the corner the two legs' lines make,
/// with no step backwards left and its ends where they were.
#[test]
fn a_mitred_fold_collapses_onto_the_corner_its_legs_meet_in() {
    let pts = major_hairpin();
    // Which side is the inside is read off the rail that folds.
    let (left, centre) = mitred_rail(&pts, -MAJOR_OUTER);
    let (right, _) = mitred_rail(&pts, MAJOR_OUTER);
    let (inner, lateral) = if any_backwards(&left, &centre) {
        (left, -MAJOR_OUTER)
    } else {
        (right, MAJOR_OUTER)
    };
    assert!(any_backwards(&inner, &centre), "the mitred hairpin folds");

    let mut unfolded = inner.clone();
    unfold_rail(&mut unfolded, &centre);
    assert!(
        !any_backwards(&unfolded, &centre),
        "a fold is left: {unfolded:?}"
    );
    assert_eq!(unfolded.first(), inner.first(), "the start never moves");
    assert_eq!(unfolded.last(), inner.last(), "the end never moves");
    let corner = inner_corner(pts[0], pts[1], pts[2], lateral);
    let moved: Vec<&[f32; 2]> = unfolded
        .iter()
        .zip(&inner)
        .filter(|(u, i)| u != i)
        .map(|(u, _)| u)
        .collect();
    assert!(!moved.is_empty(), "the fold was collapsed");
    for p in moved {
        assert!(
            (p[0] - corner[0]).hypot(p[1] - corner[1]) < 1.0e-3,
            "{p:?} collapsed off the legs' corner {corner:?}"
        );
    }
}

/// The outside of the same bend runs forward all the way and is left as
/// the mitre draws it.
#[test]
fn a_rail_running_forward_is_left_alone() {
    let pts = major_hairpin();
    for lateral in [-MAJOR_OUTER, MAJOR_OUTER] {
        let (rail, centre) = mitred_rail(&pts, lateral);
        if any_backwards(&rail, &centre) {
            continue; // the inside, covered above
        }
        let mut unfolded = rail.clone();
        unfold_rail(&mut unfolded, &centre);
        assert_eq!(unfolded, rail);
    }
}

/// A fold whose corner lies behind the chain's start - a sharp vertex a
/// metre from it - has no crossing to collapse onto: the rail is held at
/// its start, which never moves, until it runs on ahead of it.
#[test]
fn a_fold_reaching_the_start_holds_the_rail_there() {
    let v = (1.0, 0.0);
    let pts = vec![(0.0, 0.0), v, step(v, 120.0, 12.0)];
    let (left, centre) = mitred_rail(&pts, -MAJOR_OUTER);
    let (right, _) = mitred_rail(&pts, MAJOR_OUTER);
    let inner = if any_backwards(&left, &centre) {
        left
    } else {
        right
    };
    assert!(
        any_backwards(&inner, &centre),
        "the vertex by the start folds"
    );

    let mut unfolded = inner.clone();
    unfold_rail(&mut unfolded, &centre);
    assert!(
        !any_backwards(&unfolded, &centre),
        "a fold is left: {unfolded:?}"
    );
    assert_eq!(unfolded.first(), inner.first());
    assert_eq!(unfolded.last(), inner.last());
}
