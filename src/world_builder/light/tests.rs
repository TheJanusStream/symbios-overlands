use bevy::math::Affine3A;

use super::budget::{ADMIT_LIMIT, CUT_LIMIT, HOLD_SHARE};
use super::clusters::INDEX_LIST_LEN;
use super::*;
use crate::pds::sanitize::limits;
use crate::pds::types::Fp;

/// The camera space of an eye at `eye` looking along `look`.
fn view_of(eye: Vec3, look: Vec3) -> impl Fn(Vec3) -> Vec3 {
    let to_view = GlobalTransform::from(Transform::from_translation(eye).looking_to(look, Vec3::Y))
        .affine()
        .inverse();
    move |at| to_view.transform_point3(at)
}

/// A light at `at` (camera space) of `range`, wanted nowhere yet.
fn slot(at: Vec3, range: f32) -> Slot {
    Slot {
        view: at,
        range,
        eligible: true,
        held: false,
        drawn: false,
    }
}

fn drawn(verdicts: &[Verdict]) -> Vec<usize> {
    (0..verdicts.len())
        .filter(|&i| verdicts[i] == Verdict::Draw)
        .collect()
}

fn lens() -> ClusterView {
    ClusterView::default()
}

/// A fresh light is the investigation's helipad lamp (10,179 lm at a
/// 4.5 m reach; the record's 5.6 m range puts it at 4.48 m), and power
/// grows with the brightness and with the square of the reach.
#[test]
fn a_fresh_light_is_the_prototypes_lamp() {
    let lamp = lumens(1.0, crate::pds::light::DEFAULT_RANGE_M);
    assert!((10_000.0..10_200.0).contains(&lamp), "{lamp}");
    assert!((lumens(2.0, 5.6) - 2.0 * lamp).abs() < 1.0);
    assert!((lumens(1.0, 11.2) - 4.0 * lamp).abs() < 4.0);
    assert_eq!(lumens(0.0, 5.6), 0.0);
}

/// The nearest lights are drawn, however they are listed, up to the
/// count; the rest wait.
#[test]
fn the_nearest_lights_are_drawn_up_to_the_count() {
    // A row of small lamps ahead, listed out of order.
    let order: Vec<usize> = (0..100).map(|k| (k * 37) % 100).collect();
    let slots: Vec<Slot> = order
        .iter()
        .map(|&k| slot(Vec3::new(1.0, 0.0, -5.0 - 2.0 * k as f32), 1.0))
        .collect();
    let verdicts = plan(&slots, FULL_BUDGET, &lens());
    let mut kept: Vec<usize> = drawn(&verdicts).into_iter().map(|i| order[i]).collect();
    kept.sort_unstable();
    assert_eq!(kept, (0..FULL_BUDGET).collect::<Vec<_>>());
    assert!(
        verdicts.iter().all(|v| *v != Verdict::Cut),
        "nothing drawn, nothing cut"
    );
}

/// Effects Full draws 64, Reduced 32, Off none - and at Off a drawn
/// light fades out rather than snapping.
#[test]
fn the_effects_setting_sets_the_count() {
    assert_eq!(budget(EffectsIntensity::Full), 64);
    assert_eq!(budget(EffectsIntensity::Reduced), 32);
    assert_eq!(budget(EffectsIntensity::Off), 0);
    let mut slots: Vec<Slot> = (0..100)
        .map(|k| slot(Vec3::new(0.0, 0.0, -5.0 - 2.0 * k as f32), 1.0))
        .collect();
    for (effects, count) in [
        (EffectsIntensity::Full, 64),
        (EffectsIntensity::Reduced, 32),
        (EffectsIntensity::Off, 0),
    ] {
        let verdicts = plan(&slots, budget(effects), &lens());
        assert_eq!(drawn(&verdicts).len(), count, "{effects:?}");
    }
    for s in &mut slots[..10] {
        s.drawn = true;
        s.held = true;
    }
    let off = plan(&slots, 0, &lens());
    assert!(off.iter().all(|v| *v == Verdict::Fade), "{off:?}");
}

/// A drawn light keeps its place against one only a little nearer, and
/// gives it up to one past the hold; the newcomer then waits until the
/// light it replaces has faded out, so the count is never passed.
#[test]
fn a_drawn_light_keeps_its_place_against_one_a_hair_nearer() {
    let held = Slot {
        held: true,
        drawn: true,
        ..slot(Vec3::new(0.0, 0.0, -10.5), 1.0)
    };
    let close = slot(Vec3::new(0.5, 0.0, -10.0), 1.0);
    let verdicts = plan(&[held, close], 1, &lens());
    assert_eq!(verdicts, vec![Verdict::Draw, Verdict::Fade]);
    // Unheld, the nearer one would be drawn: the hold is what keeps it.
    let unheld = Slot {
        held: false,
        drawn: false,
        ..held
    };
    let verdicts = plan(&[unheld, close], 1, &lens());
    assert_eq!(verdicts, vec![Verdict::Fade, Verdict::Draw]);
    assert!(10.5 * HOLD_SHARE < 10.0f32.hypot(0.5));

    let nearer = slot(Vec3::new(0.5, 0.0, -9.0), 1.0);
    let verdicts = plan(&[held, nearer], 1, &lens());
    assert_eq!(
        verdicts,
        vec![Verdict::Fade, Verdict::Fade],
        "the old one fades out first; the new one waits for its place"
    );
    let gone = Slot {
        drawn: false,
        held: false,
        ..held
    };
    let verdicts = plan(&[gone, nearer], 1, &lens());
    assert_eq!(verdicts, vec![Verdict::Fade, Verdict::Draw]);
}

/// A drawn light keeps its place while what is drawn stays under the
/// keeping line, though a light not drawn yet would not be let in at that
/// cost: the band between the two lines is what stops a light the sway
/// let in from being sent away again.
#[test]
fn a_drawn_light_keeps_its_place_in_the_band_above_the_admitting_line() {
    let view = view_of(Vec3::new(0.0, 1.7, 0.0), Vec3::new(0.0, -0.17, -1.0));
    let lamps: Vec<Slot> = (0..400)
        .map(|k| {
            let at = Vec3::new(
                (k % 20) as f32 * 1.5 - 14.0,
                1.0,
                -3.0 - (k / 20) as f32 * 1.5,
            );
            slot(view(at), 2.5)
        })
        .collect();
    let held = |slot: Slot| Slot {
        held: true,
        drawn: true,
        ..slot
    };
    // The nearest lamps, as many as cost past the admitting line but
    // within the keeping one.
    let mut by_distance: Vec<Slot> = lamps.clone();
    by_distance.sort_by(|a, b| a.view.length().total_cmp(&b.view.length()));
    let set = (1..by_distance.len())
        .map(|n| {
            by_distance[..n]
                .iter()
                .copied()
                .map(held)
                .collect::<Vec<_>>()
        })
        .find(|set| {
            let all = vec![Verdict::Draw; set.len()];
            Planner::drawn_cost(set, &all, &lens()) > ADMIT_LIMIT
        })
        .expect("a set past the admitting line");
    let all = vec![Verdict::Draw; set.len()];
    let cost = Planner::drawn_cost(&set, &all, &lens());
    assert!(cost <= super::budget::KEEP_LIMIT, "{cost}");
    let verdicts = plan(&set, FULL_BUDGET, &lens());
    assert_eq!(verdicts, all, "drawn, they stay");
    let fresh: Vec<Slot> = set
        .iter()
        .map(|s| Slot {
            held: false,
            drawn: false,
            ..*s
        })
        .collect();
    let verdicts = plan(&fresh, FULL_BUDGET, &lens());
    assert!(drawn(&verdicts).len() < set.len(), "new, not all come in");
}

/// Walking along a street of lamps: lights fade in and out over
/// `FADE_S`, never more than the count are drawn at once, and the set
/// drawn once the walk stops is the nearest.
#[test]
fn a_walk_fades_lights_in_and_out_and_never_passes_the_count() {
    let lamps: Vec<Vec3> = (0..60)
        .map(|k| Vec3::new(if k % 2 == 0 { -4.0 } else { 4.0 }, 3.0, -3.0 * k as f32))
        .collect();
    let count = 8;
    let mut levels = vec![0.0f32; lamps.len()];
    let mut held = vec![false; lamps.len()];
    let dt = 1.0 / 60.0;
    let mut eye = Vec3::new(0.0, 1.7, 0.0);
    let mut steps_to_full = None;
    let mut planner = Planner::default();
    for frame in 0..900 {
        if frame < 600 {
            eye.z -= 0.2;
        }
        let view = view_of(eye, Vec3::NEG_Z);
        let slots: Vec<Slot> = lamps
            .iter()
            .enumerate()
            .map(|(i, &at)| Slot {
                held: held[i],
                drawn: levels[i] > 0.0,
                ..slot(view(at), 5.6)
            })
            .collect();
        let verdicts = planner.plan(&slots, count, &lens());
        for (i, verdict) in verdicts.iter().enumerate() {
            held[i] = *verdict == Verdict::Draw;
            levels[i] = match verdict {
                Verdict::Cut => 0.0,
                v => faded(levels[i], *v == Verdict::Draw, dt),
            };
        }
        let lit = levels.iter().filter(|l| **l > 0.0).count();
        assert!(lit <= count, "frame {frame}: {lit} drawn");
        if steps_to_full.is_none() && levels.iter().any(|l| *l >= 1.0) {
            steps_to_full = Some(frame + 1);
        }
    }
    let frames = steps_to_full.expect("a light faded in") as f32;
    assert!(
        (frames * dt - FADE_S).abs() <= dt,
        "a whole fade: {frames} frames"
    );
    let mut nearest: Vec<usize> = (0..lamps.len()).collect();
    nearest.sort_by(|&a, &b| lamps[a].distance(eye).total_cmp(&lamps[b].distance(eye)));
    for &i in &nearest[..count] {
        assert_eq!(levels[i], 1.0, "lamp {i} is among the nearest");
    }
    assert_eq!(levels.iter().filter(|l| **l > 0.0).count(), count);
}

/// Long lights round the eye are counted by the places they take, so
/// fewer are drawn - never fewer than the bound the sanitiser's reach
/// was chosen for - and the list is kept; the same lamps reaching less
/// are all drawn.
#[test]
fn long_lights_are_counted_by_the_clusters_they_take() {
    let view = view_of(Vec3::new(0.0, 1.7, 0.0), Vec3::new(0.0, -0.17, -1.0));
    let packed = |spacing: f32, range: f32| -> Vec<Slot> {
        let mut slots = Vec::new();
        for i in -20..=20 {
            for j in -20..=20 {
                let at = Vec3::new(i as f32 * spacing + 0.3, 2.0, j as f32 * spacing + 0.2);
                slots.push(slot(view(at), range));
            }
        }
        slots
    };
    for spacing in [2.0, 4.0, 12.0] {
        let slots = packed(spacing, limits::MAX_LIGHT_RANGE_M);
        let verdicts = plan(&slots, FULL_BUDGET, &lens());
        let kept = drawn(&verdicts);
        assert!(
            (limits::MIN_LIGHTS_AT_MAX_RANGE..FULL_BUDGET).contains(&kept.len()),
            "spacing {spacing}: {} drawn",
            kept.len()
        );
        let total = Planner::drawn_cost(&slots, &verdicts, &lens());
        assert!(total <= ADMIT_LIMIT, "spacing {spacing}: {total}");
    }
    // Spaced at their reach, lights of every reach keep the whole count.
    for range in [5.6, 12.0, limits::MAX_LIGHT_RANGE_M] {
        let verdicts = plan(&packed(range, range), FULL_BUDGET, &lens());
        assert_eq!(drawn(&verdicts).len(), FULL_BUDGET, "range {range}");
    }
}

/// One light of the longest reach takes under a quarter of WebGL2's list
/// wherever it stands in view, the eye inside it too - the bound
/// `MIN_LIGHTS_AT_MAX_RANGE` rests on.
#[test]
fn one_light_takes_a_bounded_share() {
    let view = lens();
    let range = limits::MAX_LIGHT_RANGE_M;
    let mut worst = 0;
    for x in -10..=10 {
        for y in -4..=4 {
            for z in -10..=60 {
                let at = Vec3::new(x as f32 * 2.0, y as f32, -(z as f32));
                let far = view.reach(at, range).unwrap_or(0.0);
                worst = worst.max(view.count(&view.slices(far), at, range));
            }
        }
    }
    assert!(worst > 0 && worst < INDEX_LIST_LEN / 4, "{worst}");
}

/// Turned toward more light than the list holds, drawn lights are cut at
/// once rather than faded - none out of view - and what stays fits.
#[test]
fn lights_past_the_list_are_cut_at_once() {
    let view = view_of(Vec3::new(0.0, 1.7, 0.0), Vec3::NEG_Z);
    let mut slots: Vec<Slot> = (0..12)
        .map(|k| Slot {
            drawn: true,
            held: true,
            ..slot(view(Vec3::new(0.0, 2.0, -4.0 * k as f32 - 2.0)), 20.0)
        })
        .collect();
    // And one behind, drawn, which frees nothing if cut.
    slots.push(Slot {
        drawn: true,
        held: true,
        ..slot(view(Vec3::new(0.0, 2.0, 60.0)), 5.0)
    });
    let verdicts = plan(&slots, FULL_BUDGET, &lens());
    let cut: Vec<usize> = (0..slots.len())
        .filter(|&i| verdicts[i] == Verdict::Cut)
        .collect();
    assert!(!cut.is_empty(), "{verdicts:?}");
    assert!(!cut.contains(&12), "a light out of view is never cut");
    assert!(Planner::drawn_cost(&slots, &verdicts, &lens()) <= CUT_LIMIT);
    assert!(drawn(&verdicts).len() >= limits::MIN_LIGHTS_AT_MAX_RANGE);
}

/// Past the list, the lights cut are those that free the most places for
/// how near they are: long lights behind a row of them go, not the small
/// lamps farther off, which free next to nothing.
#[test]
fn the_costliest_light_for_its_distance_is_cut_first() {
    let view = view_of(Vec3::new(0.0, 1.7, 0.0), Vec3::NEG_Z);
    let mut slots: Vec<Slot> = (0..4)
        .map(|k| Slot {
            drawn: true,
            held: true,
            ..slot(
                view(Vec3::new(-3.0 + 2.0 * k as f32, 2.0, -6.0)),
                limits::MAX_LIGHT_RANGE_M,
            )
        })
        .collect();
    // Small far lamps, drawn: the farthest by distance, the cheapest.
    for k in 0..6 {
        slots.push(Slot {
            drawn: true,
            held: true,
            ..slot(view(Vec3::new(-5.0 + 2.0 * k as f32, 1.0, -40.0)), 1.0)
        });
    }
    // More long lights, which the list cannot hold beside the rest.
    for k in 0..3 {
        slots.push(Slot {
            drawn: true,
            held: true,
            ..slot(
                view(Vec3::new(
                    -2.0 + 2.0 * k as f32,
                    3.0,
                    -10.0 - 2.0 * k as f32,
                )),
                limits::MAX_LIGHT_RANGE_M,
            )
        });
    }
    let verdicts = plan(&slots, FULL_BUDGET, &lens());
    assert!(
        (4..10).all(|i| verdicts[i] == Verdict::Draw),
        "the small far lamps stay: {verdicts:?}"
    );
    assert!(verdicts.contains(&Verdict::Cut), "{verdicts:?}");
    assert!(Planner::drawn_cost(&slots, &verdicts, &lens()) <= CUT_LIMIT);
}

/// One frame of a scene: a light standing at `at` of `range`, with what
/// the drawing keeps of it.
#[derive(Clone)]
struct Lamp {
    at: Vec3,
    range: f32,
    level: f32,
    held: bool,
    turns: u32,
}

fn lamp(at: Vec3, range: f32) -> Lamp {
    Lamp {
        at,
        range,
        level: 0.0,
        held: false,
        turns: 0,
    }
}

/// What a scene did: the most places Bevy counted in a frame, the lights
/// cut from full or part brightness, and the most times one light
/// changed between wanted and not.
#[derive(Debug, Default)]
struct Run {
    worst: u32,
    cuts: usize,
    turns: u32,
}

/// Run `poses` through the budget as the game does: the plan reads the
/// camera's pose of this frame (it runs after propagation), and Bevy then
/// counts the lights drawn there, ending the slices where the lights it
/// drew the frame before reached - from where the camera stood then.
fn run(lamps: &mut [Lamp], poses: &[Transform], screen: UVec2, count: usize) -> Run {
    let view = ClusterView::of(&Projection::default(), screen);
    let mut planner = Planner::default();
    let mut bevy_far = 1000.0;
    let mut out = Run::default();
    let inverse = |pose: &Transform| GlobalTransform::from(*pose).affine().inverse();
    for pose in poses {
        let to_view: Affine3A = inverse(pose);
        let slots: Vec<Slot> = lamps
            .iter()
            .map(|l| Slot {
                view: to_view.transform_point3(l.at),
                range: l.range,
                eligible: true,
                held: l.held,
                drawn: l.level > 0.0,
            })
            .collect();
        let verdicts = planner.plan(&slots, count, &view);
        for (l, &v) in lamps.iter_mut().zip(verdicts) {
            if v == Verdict::Cut && l.level > 0.0 {
                out.cuts += 1;
            }
            let want = v == Verdict::Draw;
            if l.held != want {
                l.turns += 1;
                out.turns = out.turns.max(l.turns);
            }
            l.held = want;
            l.level = match v {
                Verdict::Cut => 0.0,
                Verdict::Hurry => faded(l.level, false, FADE_S / budget::HURRY_FADE_S / 60.0),
                Verdict::Draw | Verdict::Fade => faded(l.level, want, 1.0 / 60.0),
            };
        }
        let shown: Vec<(Vec3, f32)> = lamps
            .iter()
            .filter(|l| l.level > 0.0)
            .map(|l| (to_view.transform_point3(l.at), l.range))
            .collect();
        let slices = view.slices(bevy_far);
        let total: u32 = shown.iter().map(|&(c, r)| view.count(&slices, c, r)).sum();
        out.worst = out.worst.max(total);
        bevy_far = shown
            .iter()
            .filter_map(|&(c, r)| view.reach(c, r))
            .fold(0.0, f32::max);
    }
    out
}

fn eye(at: Vec3, yaw: f32, pitch: f32) -> Transform {
    Transform::from_translation(at).with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0))
}

fn grid(spacing: f32, half: i32, y: f32, range: f32) -> Vec<Lamp> {
    let mut lamps = Vec::new();
    for i in -half..=half {
        for j in -half..=half {
            lamps.push(lamp(
                Vec3::new(i as f32 * spacing + 0.3, y, j as f32 * spacing + 0.2),
                range,
            ));
        }
    }
    lamps
}

const HD: UVec2 = UVec2::new(1280, 720);

/// Walking toward a row of long lights over a field of lamps (the critic's
/// browser scene 1, #1921): a long light far ahead stretches Bevy's slices
/// only from the frame after it is drawn, and the drawn set never
/// overflows the list meanwhile - nor is a light cut.
#[test]
fn walking_toward_far_long_lights_never_overflows_the_list() {
    let mut lamps = grid(3.0, 15, 2.5, 5.6);
    for k in 0..12 {
        lamps.push(lamp(Vec3::new(-30.0 + 6.0 * k as f32, 6.0, -40.0), 20.0));
    }
    let poses: Vec<Transform> = (0..600)
        .map(|f| eye(Vec3::new(0.0, 1.7, 30.0 - f as f32 * 0.12), 0.0, -0.1))
        .collect();
    let run = run(&mut lamps, &poses, HD, FULL_BUDGET);
    assert!(run.worst < INDEX_LIST_LEN, "{run:?}");
    assert_eq!(run.cuts, 0, "{run:?}");
}

/// Ordinary motion fades lights rather than cutting them (the critic's
/// popping scenes, #1921): a ring of lamps round a swaying eye turning
/// once round in 30 s, an eye 8 m up looking down over a field as it
/// turns, and a tall narrow window turning over a dense field, both at
/// 1.2 rad/s. Bevy's count stays under the list, no light is cut, and
/// none changes its mind often - a lamp of the ring enters and leaves
/// the view once.
#[test]
fn slow_motion_fades_lights_and_cuts_none() {
    let ring = || -> Vec<Lamp> {
        (0..100)
            .map(|k| {
                let a = k as f32 / 100.0 * std::f32::consts::TAU;
                lamp(Vec3::new(a.cos() * 10.0, 2.5, a.sin() * 10.0), 5.6)
            })
            .collect()
    };
    let bob: Vec<Transform> = (0..1800)
        .map(|f| {
            let t = f as f32 / 60.0;
            let at = Vec3::new(
                0.05 * (t * 1.3).sin(),
                1.7 + 0.03 * (t * 7.0).sin(),
                0.05 * (t * 0.9).cos(),
            );
            eye(at, t * 0.2, -0.1)
        })
        .collect();
    let down: Vec<Transform> = (0..240)
        .map(|f| eye(Vec3::new(0.0, 8.0, 0.0), f as f32 * 0.02, -1.54))
        .collect();
    let narrow: Vec<Transform> = (0..240)
        .map(|f| eye(Vec3::new(0.0, 1.7, 0.0), f as f32 * 0.02, -0.17))
        .collect();
    for (name, mut lamps, poses, screen) in [
        ("ring", ring(), &bob, HD),
        ("down", grid(3.0, 20, 0.5, 5.6), &down, HD),
        (
            "narrow",
            grid(3.0, 20, 2.0, 5.6),
            &narrow,
            UVec2::new(400, 1200),
        ),
    ] {
        let run = run(&mut lamps, poses, screen, FULL_BUDGET);
        assert!(run.worst < INDEX_LIST_LEN, "{name}: {run:?}");
        assert_eq!(run.cuts, 0, "{name}: {run:?}");
        assert!(run.turns <= 10, "{name}: {run:?}");
    }
}

/// A light's flicker is a function of the clock and its seed alone:
/// the same on every peer, apart from another light's, within its
/// amount, and smooth at UTC's size of clock.
#[test]
fn a_flicker_is_the_same_everywhere_and_stays_within_its_amount() {
    let flicker = Flicker::Flicker {
        amount: 0.4,
        rate: 0.7,
        fine: true,
    };
    let pulse = Flicker::Pulse {
        amount: 0.6,
        rate: 0.5,
    };
    let utc = 1.8e9;
    let seed = light_seed("lamp", &[0, 2], 12.5, 0);
    assert_eq!(seed, light_seed("lamp", &[0, 2], 12.5, 0));
    assert_ne!(seed, light_seed("lamp", &[0, 3], 12.5, 0));
    assert_ne!(seed, light_seed("lamp", &[0, 2], 13.5, 0));
    assert_ne!(seed, light_seed("lamq", &[0, 2], 12.5, 0));
    assert_ne!(
        seed,
        light_seed("lamp", &[0, 2], 12.5, 1),
        "a grid's stacked copy, its epoch its column's"
    );
    let other = light_seed("lamp", &[1], 12.5, 0);
    let mut apart = 0;
    for k in 0..400 {
        let now = utc + f64::from(k) * 0.05;
        for f in [flicker, pulse] {
            let share = flicker_share(f, seed, now);
            assert_eq!(share, flicker_share(f, seed, now), "deterministic");
            let amount = match f {
                Flicker::Flicker { amount, .. } | Flicker::Pulse { amount, .. } => amount,
            };
            assert!((1.0 - amount..=1.0).contains(&share), "{share}");
            let next = flicker_share(f, seed, now + 0.002);
            assert!((share - next).abs() < 0.05, "smooth: {share} {next}");
        }
        apart += usize::from(
            (flicker_share(flicker, seed, now) - flicker_share(flicker, other, now)).abs() > 0.02,
        );
    }
    assert!(apart > 200, "two lights waver apart: {apart}");
    // A pulse comes round once a period.
    let at = utc + 0.3;
    assert!((flicker_share(pulse, seed, at) - flicker_share(pulse, seed, at + 2.0)).abs() < 1e-3);
}

/// At UTC's size of clock a flicker and a pulse still move through their
/// whole depth within a period, rather than holding still between the
/// steps an `f32` clock could tell apart.
#[test]
fn a_flicker_moves_through_its_depth_at_utc_scale() {
    let utc = 1.8e9 + 0.25;
    let seed = light_seed("lamp", &[4], 0.0, 0);
    let pulse = Flicker::Pulse {
        amount: 0.6,
        rate: 0.5,
    };
    let flicker = Flicker::Flicker {
        amount: 0.6,
        rate: 0.5,
        fine: true,
    };
    for (f, swing) in [(pulse, 0.55), (flicker, 0.05)] {
        let shares: Vec<f32> = (0..40)
            .map(|k| flicker_share(f, seed, utc + f64::from(k) * 0.05))
            .collect();
        let lo = shares.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = shares.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(hi - lo > swing, "{f:?}: {lo} to {hi}");
        let steps = shares.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(steps, shares.len() - 1, "{f:?} moves every frame");
    }
}

/// The Effects setting calms a flicker at the drawing: half as deep, at
/// most once a second and without its fine shimmer at Reduced, steady at
/// Off.
#[test]
fn the_effects_setting_calms_a_flicker() {
    let fast = Flicker::Flicker {
        amount: 0.8,
        rate: 0.7,
        fine: true,
    };
    assert_eq!(calmed(fast, EffectsIntensity::Full), Some(fast));
    assert_eq!(
        calmed(fast, EffectsIntensity::Reduced),
        Some(Flicker::Flicker {
            amount: 0.4,
            rate: 0.7,
            fine: false
        })
    );
    let quick = Flicker::Pulse {
        amount: 0.08,
        rate: 6.0,
    };
    assert_eq!(
        calmed(quick, EffectsIntensity::Reduced),
        Some(Flicker::Pulse {
            amount: 0.04,
            rate: 1.0
        }),
        "at most once a second"
    );
    let slow = Flicker::Pulse {
        amount: 0.5,
        rate: 0.25,
    };
    assert_eq!(
        calmed(slow, EffectsIntensity::Reduced),
        Some(Flicker::Pulse {
            amount: 0.25,
            rate: 0.25
        }),
        "a slow pulse keeps its pace"
    );
    assert_eq!(calmed(fast, EffectsIntensity::Off), None);
    // Without its shimmer a flicker is its broad waver alone: at most one
    // new value a second at Reduced.
    let seed = light_seed("lamp", &[1], 0.0, 0);
    let calm = calmed(fast, EffectsIntensity::Reduced).expect("calmed");
    for k in 0..50 {
        let now = 1.8e9 + f64::from(k) * 0.1;
        let lone = Flicker::Flicker {
            amount: 0.4,
            rate: 0.7,
            fine: false,
        };
        assert_eq!(
            flicker_share(calm, seed, now),
            flicker_share(lone, seed, now)
        );
    }
}

/// An app holding a camera, a dusk-to-dawn light and one lit at every
/// hour, run under the world's darkness.
fn lit_app(lights: f32) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(WorldWeather::default())
        .add_systems(Update, follow_node_lights);
    app.world_mut()
        .resource_mut::<WorldWeather>()
        .uniforms
        .lights = lights;
    app.world_mut().spawn((
        Camera::default(),
        Projection::Perspective(PerspectiveProjection::default()),
        WorldCamera,
        GlobalTransform::from(Transform::from_xyz(0.0, 2.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)),
    ));
    let node = app
        .world_mut()
        .spawn((Transform::default(), InheritedVisibility::VISIBLE))
        .id();
    let mut commands = app.world_mut().commands();
    attach(&mut commands, node, &LightSource::default(), 1);
    attach(
        &mut commands,
        node,
        &LightSource {
            dusk_to_dawn: false,
            ..LightSource::spot()
        },
        2,
    );
    app.world_mut().flush();
    let mut lights = app
        .world_mut()
        .query_filtered::<(Entity, Has<PointLight>), With<NodeLight>>();
    let found: Vec<(Entity, bool)> = lights.iter(app.world()).collect();
    let dusk = found.iter().find(|(_, point)| *point).expect("the point").0;
    let always = found.iter().find(|(_, point)| !*point).expect("the spot").0;
    (app, dusk, always)
}

/// A second of frames, a tenth of a second each: past a whole fade.
fn settle(app: &mut App) {
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_millis(100),
    ));
    for _ in 0..12 {
        app.update();
    }
}

/// By day a light lit from dusk to dawn is dark and hidden - no
/// clusterable object at all - while one lit at every hour shines; in
/// the dark both shine at their full power.
#[test]
fn a_dusk_to_dawn_light_is_dark_by_day() {
    let (mut day, dusk, always) = lit_app(0.0);
    settle(&mut day);
    let world = day.world();
    assert_eq!(
        world.get::<PointLight>(dusk).map(|l| l.intensity),
        Some(0.0)
    );
    assert_eq!(world.get::<Visibility>(dusk), Some(&Visibility::Hidden));
    assert!(
        !world.get::<NodeLight>(dusk).expect("light").held,
        "no place by day"
    );
    let spot = world.get::<SpotLight>(always).expect("the spot");
    assert!(spot.intensity > 0.0, "a light at every hour shines by day");
    assert_eq!(
        world.get::<Visibility>(always),
        Some(&Visibility::Inherited)
    );

    let (mut night, dusk, _) = lit_app(1.0);
    settle(&mut night);
    let world = night.world();
    let point = world.get::<PointLight>(dusk).expect("the point");
    assert_eq!(
        point.intensity,
        lumens(1.0, crate::pds::light::DEFAULT_RANGE_M)
    );
    assert!(!point.shadow_maps_enabled, "no light casts a shadow");
    assert_eq!(world.get::<Visibility>(dusk), Some(&Visibility::Inherited));
}

/// An app with one full-depth pulsing light lit at every hour, on the
/// scene clock, its frames a twentieth of a second.
fn pulsing_app(effects: EffectsIntensity) -> (App, Entity) {
    let mut app = App::new();
    let settings = crate::state::LocalSettings {
        effects_intensity: effects,
        ..Default::default()
    };
    app.add_plugins(MinimalPlugins)
        .insert_resource(WorldWeather::default())
        .insert_resource(SpinClock::Scene)
        .insert_resource(settings)
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(50),
        ))
        .add_systems(Update, follow_node_lights);
    app.world_mut().spawn((
        Camera::default(),
        Projection::Perspective(PerspectiveProjection::default()),
        WorldCamera,
        GlobalTransform::from(Transform::from_xyz(0.0, 2.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)),
    ));
    let node = app
        .world_mut()
        .spawn((Transform::default(), InheritedVisibility::VISIBLE))
        .id();
    let mut commands = app.world_mut().commands();
    let light = LightSource {
        dusk_to_dawn: false,
        flicker: Some(LightFlicker::Pulse {
            amount: Fp(1.0),
            rate: Fp(limits::MAX_LIGHT_FLASHES_HZ),
        }),
        ..LightSource::default()
    };
    attach(&mut commands, node, &light, 7);
    app.world_mut().flush();
    let mut lights = app.world_mut().query_filtered::<Entity, With<NodeLight>>();
    let light = lights.single(app.world()).expect("the light");
    (app, light)
}

/// The share of its faded-in power the light gives on each of `frames`.
fn shares(app: &mut App, light: Entity, frames: usize) -> Vec<f32> {
    (0..frames)
        .filter_map(|_| {
            app.update();
            let world = app.world();
            let state = world.get::<NodeLight>(light).expect("state");
            let intensity = world.get::<PointLight>(light).expect("light").intensity;
            (state.level > 0.0).then(|| intensity / (state.lumens * state.level))
        })
        .collect()
}

/// The drawing calms a flicker by the viewer's Effects setting (#1921): a
/// full-depth pulse dips to nothing at Full, to no less than half at
/// Reduced, and holds steady at Off while it fades out.
#[test]
fn the_drawing_calms_a_flicker_by_the_effects_setting() {
    let (mut full, light) = pulsing_app(EffectsIntensity::Full);
    let seen = shares(&mut full, light, 60);
    let low = seen.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(low < 0.1, "a full pulse dips deep at Full: {low}");

    let (mut reduced, light) = pulsing_app(EffectsIntensity::Reduced);
    let seen = shares(&mut reduced, light, 60);
    let low = seen.iter().copied().fold(f32::INFINITY, f32::min);
    assert!(
        (0.5 - 1e-4..0.7).contains(&low),
        "half as deep at Reduced, and still moving: {low}"
    );

    // Lit at Full, then turned Off: it fades out, steady all the way.
    let (mut off, light) = pulsing_app(EffectsIntensity::Full);
    shares(&mut off, light, 20);
    off.world_mut()
        .resource_mut::<crate::state::LocalSettings>()
        .effects_intensity = EffectsIntensity::Off;
    let seen = shares(&mut off, light, 20);
    assert!(!seen.is_empty(), "it fades rather than snapping");
    assert!(seen.iter().all(|&s| s == 1.0), "steady at Off: {seen:?}");
}

/// A lamp respawned with its node - its item compiled again, as every
/// World Editor edit to it does (#1922) - shines on at once at the level
/// the light it replaces had reached, rather than going dark and fading in
/// again; while a new lamp of another seed still fades in from nothing.
#[test]
fn a_respawned_light_takes_up_where_its_predecessor_left_off() {
    let (mut app, _, always) = lit_app(1.0);
    settle(&mut app);
    let old = app
        .world()
        .get::<NodeLight>(always)
        .expect("the spot")
        .clone();
    assert_eq!(old.level, 1.0, "control: faded in fully");

    // The item compiled again: its node and light despawned, a new node
    // spawned with the same light, at the same place of the same copy.
    let node = app
        .world()
        .get::<ChildOf>(always)
        .expect("parented")
        .parent();
    app.world_mut().entity_mut(node).despawn();
    let again = app
        .world_mut()
        .spawn((Transform::default(), InheritedVisibility::VISIBLE))
        .id();
    let lamp = LightSource {
        dusk_to_dawn: false,
        ..LightSource::spot()
    };
    let mut commands = app.world_mut().commands();
    attach(&mut commands, again, &lamp, old.seed);
    attach(&mut commands, again, &LightSource::default(), 99);
    app.world_mut().flush();
    app.update();

    let mut lights = app.world_mut().query::<(&NodeLight, Option<&SpotLight>)>();
    let now: Vec<(NodeLight, Option<f32>)> = lights
        .iter(app.world())
        .map(|(light, spot)| (light.clone(), spot.map(|s| s.intensity)))
        .collect();
    let (respawned, intensity) = now
        .iter()
        .find(|(light, _)| light.seed == old.seed)
        .expect("the respawned spot");
    assert_eq!(respawned.level, 1.0, "it shines on: {now:?}");
    assert!(respawned.held);
    assert_eq!(*intensity, Some(respawned.lumens));
    let (fresh, _) = now
        .iter()
        .find(|(light, _)| light.seed == 99)
        .expect("the new lamp");
    assert!(
        fresh.level > 0.0 && fresh.level < 0.5,
        "control: a new light still fades in: {}",
        fresh.level
    );
}

/// The point and the spot of [`lit_app`], as attached there.
fn lit_app_lamps() -> (LightSource, LightSource) {
    (
        LightSource::default(),
        LightSource {
            dusk_to_dawn: false,
            ..LightSource::spot()
        },
    )
}

/// The item compiled again: the node `old` hangs from despawned, `gap`
/// frames run, and a new node spawned carrying each of `lamps` - a light,
/// its seed and where it stands in the world, as propagation would put it.
/// One frame then runs; what each new light is then, by seed.
fn respawn(
    app: &mut App,
    old: Entity,
    gap: usize,
    lamps: &[(LightSource, u64, Vec3)],
) -> Vec<(u64, NodeLight)> {
    let node = app.world().get::<ChildOf>(old).expect("parented").parent();
    app.world_mut().entity_mut(node).despawn();
    for _ in 0..gap {
        app.update();
    }
    let again = app
        .world_mut()
        .spawn((Transform::default(), InheritedVisibility::VISIBLE))
        .id();
    let mut commands = app.world_mut().commands();
    for (lamp, seed, _) in lamps {
        attach(&mut commands, again, lamp, *seed);
    }
    app.world_mut().flush();
    let children: Vec<Entity> = app
        .world()
        .get::<Children>(again)
        .expect("its lights")
        .iter()
        .collect();
    for &child in &children {
        let seed = app.world().get::<NodeLight>(child).expect("a light").seed;
        let (_, _, at) = lamps.iter().find(|(_, s, _)| *s == seed).expect("asked");
        app.world_mut()
            .entity_mut(child)
            .insert(GlobalTransform::from_translation(*at));
    }
    app.update();
    children
        .iter()
        .map(|&child| {
            let light = app.world().get::<NodeLight>(child).expect("a light");
            (light.seed, light.clone())
        })
        .collect()
}

fn by_seed(lights: &[(u64, NodeLight)], seed: u64) -> &NodeLight {
    &lights
        .iter()
        .find(|(s, _)| *s == seed)
        .expect("the light")
        .1
}

/// A light respawned reaching further than the one it replaces costs the
/// cluster list more, so it does not take over that one's place: it comes
/// in from nothing, as any newcomer does, under the budget's admitting
/// line - a Reach dragged out in the World Editor recompiles the lamp at
/// every flush (#1922). One reaching less takes over.
#[test]
fn a_respawned_light_reaching_further_comes_in_as_a_newcomer() {
    let (mut app, dusk, always) = lit_app(1.0);
    settle(&mut app);
    for light in [dusk, always] {
        let level = app.world().get::<NodeLight>(light).expect("lit").level;
        assert_eq!(level, 1.0, "control: faded in fully");
    }
    let (point, spot) = lit_app_lamps();
    let further = LightSource {
        range: Fp(spot.range.0 + 4.0),
        ..spot
    };
    let nearer = LightSource {
        range: Fp(point.range.0 - 1.0),
        ..point
    };
    let now = respawn(
        &mut app,
        always,
        0,
        &[(further, 2, Vec3::ZERO), (nearer, 1, Vec3::ZERO)],
    );
    let grown = by_seed(&now, 2);
    assert!(
        grown.level > 0.0 && grown.level < 0.5,
        "a longer reach fades in from nothing: {grown:?}"
    );
    let shrunk = by_seed(&now, 1);
    assert_eq!(shrunk.level, 1.0, "a shorter reach takes over: {shrunk:?}");
    assert!(shrunk.held);
}

/// A light respawned with a gone light's seed but standing elsewhere (a
/// sibling deleted or a placement moved hands one light's seed to another)
/// is another light, and fades in from nothing; one moved a hair takes
/// over (#1922).
#[test]
fn a_respawned_light_elsewhere_is_another_light() {
    let (mut app, _, always) = lit_app(1.0);
    settle(&mut app);
    let (point, spot) = lit_app_lamps();
    let now = respawn(
        &mut app,
        always,
        0,
        &[
            (spot, 2, Vec3::new(300.0, 0.0, 0.0)),
            (point, 1, Vec3::new(0.3, 0.0, 0.2)),
        ],
    );
    let far = by_seed(&now, 2);
    assert!(
        far.level < 0.5,
        "a light 300 m away is another light: {far:?}"
    );
    assert_eq!(by_seed(&now, 1).level, 1.0, "a hair off, it takes over");
}

/// What a gone light leaves for one respawned in its stead lasts a
/// respawn's time ([`KEPT_S`]) and no longer - with other lights still
/// drawn, and in a world left with no light at all, whose lights a
/// visitor coming back to it later sees fade in again rather than pop on
/// all at once past the admitting line (#1922). A respawn a frame later
/// still takes over.
#[test]
fn a_gone_light_is_forgotten_after_a_respawns_time() {
    let frames = |s: f64| (s / 0.1).round() as usize;
    let (point, spot) = lit_app_lamps();
    let lamps = [(spot, 2, Vec3::ZERO), (point, 1, Vec3::ZERO)];

    // No light left at all meanwhile.
    for (gap, kept) in [(1, true), (frames(KEPT_S - 0.5), true), (100, false)] {
        let (mut app, _, always) = lit_app(1.0);
        settle(&mut app);
        let now = respawn(&mut app, always, gap, &lamps);
        let level = by_seed(&now, 2).level;
        assert_eq!(
            level == 1.0,
            kept,
            "{gap} frames gone, no light left: {level}"
        );
    }

    // Another lamp drawn all the while.
    for (gap, kept) in [(frames(KEPT_S - 0.5), true), (frames(KEPT_S + 0.5), false)] {
        let (mut app, _, always) = lit_app(1.0);
        let other = app
            .world_mut()
            .spawn((Transform::default(), InheritedVisibility::VISIBLE))
            .id();
        let mut commands = app.world_mut().commands();
        attach(&mut commands, other, &LightSource::default(), 3);
        app.world_mut().flush();
        settle(&mut app);
        let now = respawn(&mut app, always, gap, &lamps);
        let level = by_seed(&now, 2).level;
        assert_eq!(
            level == 1.0,
            kept,
            "{gap} frames gone, a lamp beside: {level}"
        );
    }
}

/// A body (#1924): a chassis standing `at` and heading `turn`, with a node
/// under it carrying the avatar light `seed`, `offset` from the chassis in
/// its frame - both posed as propagation would pose them. The node.
fn body_at(
    app: &mut App,
    chassis: Entity,
    at: Vec3,
    turn: Quat,
    offset: Vec3,
    seed: u64,
) -> Entity {
    let pose = GlobalTransform::from(Transform::from_translation(at).with_rotation(turn));
    app.world_mut().entity_mut(chassis).insert(pose);
    let node = app
        .world_mut()
        .spawn((
            Transform::default(),
            InheritedVisibility::VISIBLE,
            ChildOf(chassis),
        ))
        .id();
    let mut commands = app.world_mut().commands();
    attach_to_body(&mut commands, node, &LightSource::default(), seed, chassis);
    app.world_mut().flush();
    let light = app.world().get::<Children>(node).expect("its light")[0];
    app.world_mut()
        .entity_mut(light)
        .insert(GlobalTransform::from_translation(
            pose.transform_point(offset),
        ));
    node
}

/// A body whose craft moves at `speed` m/s, drawn at `fps`, is respawned -
/// its owner's edit, a record update from a peer - in the frame between,
/// so it stands `speed / fps` further on when its new lights first draw:
/// at 50 m/s at 60 fps, or at 20 m/s at 30 fps, further than a room light
/// may stand from where it stood and take up its fade ([`KEPT_REACH_M`]).
/// Its lights stand where they stood in its own frame, and shine on at
/// once; a body respawned with a lamp moved a metre along it brings that
/// lamp in as a newcomer, as a room's lamp moved so far, which keeps the
/// world's test, does.
#[test]
fn a_body_respawned_while_it_moves_keeps_its_lights_fades() {
    let seed = avatar_light_seed(LightOwner::body(Some("did:plc:alice")), &[0, 3]);
    let offset = Vec3::new(0.4, 0.6, 1.8);
    let turn = Quat::from_rotation_y(0.6);
    for (speed, fps) in [(50.0f32, 60.0f32), (20.0, 30.0)] {
        let step = speed / fps;
        assert!(step > KEPT_REACH_M, "the premise: {step} m on");
        let ahead = turn * Vec3::Z * step;
        for (moved, kept) in [(Vec3::ZERO, true), (Vec3::Z, false)] {
            let (mut app, _, _) = lit_app(1.0);
            let chassis = app.world_mut().spawn_empty().id();
            let node = body_at(&mut app, chassis, Vec3::ZERO, turn, offset, seed);
            settle(&mut app);
            let old = app.world().get::<Children>(node).expect("its light")[0];
            assert_eq!(app.world().get::<NodeLight>(old).unwrap().level, 1.0);
            app.world_mut().entity_mut(node).despawn();
            let node = body_at(&mut app, chassis, ahead, turn, offset + moved, seed);
            app.update();
            let light = app.world().get::<Children>(node).expect("its light")[0];
            let level = app.world().get::<NodeLight>(light).unwrap().level;
            assert_eq!(
                level == 1.0,
                kept,
                "{speed} m/s at {fps} fps, its lamp moved {moved} in its frame: {level}"
            );
        }
        // A room's lamp respawned as far off is another light.
        let (mut app, _, always) = lit_app(1.0);
        settle(&mut app);
        let (_, spot) = lit_app_lamps();
        let now = respawn(&mut app, always, 0, &[(spot, 2, ahead)]);
        let level = by_seed(&now, 2).level;
        assert!(level < 0.5, "a room's lamp {step} m off: {level}");
    }
}
