//! `agent drive` (#1527): hold keys segment by segment, as a person does at
//! the wheel, and measure what the run did - its speed, each jump and how it
//! landed, how far the body tipped - every frame it runs.
//!
//! The other movements aim somewhere; this one does not. It is for a body on
//! wheels, a car or a hover-boat, driven at something: a ramp, a bend, a gap.
//! So it holds what it was told for as long as it was told, and says
//! afterwards what happened, which is the point of it: a stunt is an
//! experiment, and a jump's hang time is read, not guessed.
//!
//! A car is off the ground when the physics says so: none of its wheels is
//! down ([`crate::player::CarContact`], the count its traction uses, #1524)
//! and its body is not resting on anything either - a car lying on its side
//! has no wheel down too, and is not flying. A wheel counts while its ray
//! meets the ground within the suspension's rest length and 15 cm, so a car
//! skimming a tabletop's deck lower than that is on the deck as far as its
//! traction goes, and so for the report. A hover-boat has no such count, so
//! for it a jump is read from the body's height over what is below it (the
//! one reading [`super::sense`] gives every movement): its underside rides
//! within its suspension's rest length of what it drives on, so it is off
//! the ground once its underside is higher than that and a few centimetres.
//! Either way it takes two frames running - one frame over a bump is not a
//! jump - and it is back on the first frame it is not.

use bevy::prelude::*;

use crate::agent::control::events::{DriveReport, JumpReport, MoveOutcome};
use crate::agent::control::protocol::DriveSegment;
use crate::pds::LocomotionConfig;

use super::super::hundredths;
use super::flight::Craft;
use super::{Controls, Step};

/// The most segments one drive holds.
pub(super) const MAX_SEGMENTS: usize = 64;
/// The longest one segment is held (s).
pub(super) const MAX_SEGMENT_SECS: f32 = 60.0;
/// The longest a whole drive runs (s).
pub(super) const MAX_DRIVE_SECS: f32 = 180.0;
/// How far above its suspension's rest length a hover-boat's underside must
/// be for it to be off the ground (m) - and a car's, should its wheel count
/// be missing: its springs are unloaded there, so a few centimetres over it
/// they cannot touch. 0.2 m missed a whole jump over a tabletop's deck
/// (session 893, when cars were read by their height too).
const AIRBORNE_MARGIN_M: f32 = 0.05;
/// How many frames running it must be off the ground: one frame over a bump
/// is not a jump.
const AIRBORNE_FRAMES: u32 = 2;
/// A car with no wheel down whose underside is this close to what is below
/// it is lying on it, not flying (m).
const RESTING_M: f32 = 0.1;

/// The keys a drive may hold, by the names a client gives them.
const KEY_NAMES: [(&str, KeyCode); 8] = [
    ("W", KeyCode::KeyW),
    ("A", KeyCode::KeyA),
    ("S", KeyCode::KeyS),
    ("D", KeyCode::KeyD),
    ("Q", KeyCode::KeyQ),
    ("E", KeyCode::KeyE),
    ("SPACE", KeyCode::Space),
    ("SHIFT", KeyCode::ShiftLeft),
];

/// A key by its name (any case), as the client spells it.
pub(super) fn key_named(name: &str) -> Option<KeyCode> {
    let upper = name.trim().to_ascii_uppercase();
    KEY_NAMES
        .iter()
        .find(|(key_name, _)| *key_name == upper)
        .map(|(_, key)| *key)
}

/// How high a body's underside may ride over what is below it and still be
/// on the ground: its suspension's reach and a margin - or `None` for a body
/// that is not on wheels, which `drive` refuses.
pub(super) fn airborne_above(locomotion: &LocomotionConfig) -> Option<f32> {
    match locomotion {
        LocomotionConfig::Car(params) => Some(params.suspension_rest_length.0 + AIRBORNE_MARGIN_M),
        LocomotionConfig::HoverBoat(params) => {
            Some(params.suspension_rest_length.0 + AIRBORNE_MARGIN_M)
        }
        LocomotionConfig::Humanoid(_)
        | LocomotionConfig::Helicopter(_)
        | LocomotionConfig::Airplane(_)
        | LocomotionConfig::Unknown => None,
    }
}

/// One stretch of a drive: these keys, held for so long.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Segment {
    pub(super) keys: Vec<KeyCode>,
    pub(super) secs: f64,
}

/// The segments a client asked for, checked: a key this build does not
/// know, a time that is not a positive number, or too much of either is
/// refused by name.
pub(super) fn segments(asked: Vec<DriveSegment>) -> Result<Vec<Segment>, String> {
    if asked.is_empty() {
        return Err("a drive needs at least one segment".to_owned());
    }
    if asked.len() > MAX_SEGMENTS {
        return Err(format!("a drive holds at most {MAX_SEGMENTS} segments"));
    }
    let mut total = 0.0;
    let mut out = Vec::with_capacity(asked.len());
    for (index, segment) in asked.into_iter().enumerate() {
        if !segment.secs.is_finite() || segment.secs <= 0.0 || segment.secs > MAX_SEGMENT_SECS {
            return Err(format!(
                "segment {} is held for {} s: it takes more than 0 and at most {MAX_SEGMENT_SECS} s",
                index + 1,
                segment.secs
            ));
        }
        let mut keys = Vec::with_capacity(segment.keys.len());
        for name in &segment.keys {
            let key = key_named(name).ok_or_else(|| {
                format!(
                    "segment {} holds {name:?}, which is not one of W, A, S, D, Q, E, SPACE, SHIFT",
                    index + 1
                )
            })?;
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        total += segment.secs;
        out.push(Segment {
            keys,
            secs: f64::from(segment.secs),
        });
    }
    if total > MAX_DRIVE_SECS {
        return Err(format!(
            "the drive runs {total} s: at most {MAX_DRIVE_SECS} s in all"
        ));
    }
    Ok(out)
}

/// A drive under way: what to hold when, and what it has done so far.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct KeyRun {
    segments: Vec<Segment>,
    started: f64,
    airborne_above: f32,
    log: RunLog,
}

impl KeyRun {
    pub(super) fn new(segments: Vec<Segment>, started: f64, airborne_above: f32) -> Self {
        Self {
            segments,
            started,
            airborne_above,
            log: RunLog::default(),
        }
    }

    /// How long it runs in all (s).
    pub(super) fn total(&self) -> f64 {
        self.segments.iter().map(|segment| segment.secs).sum()
    }

    /// The segment held `elapsed` seconds in, by its index - `None` once
    /// every one has been held.
    pub(super) fn segment_at(&self, elapsed: f64) -> Option<usize> {
        let mut until = 0.0;
        for (index, segment) in self.segments.iter().enumerate() {
            until += segment.secs;
            if elapsed < until {
                return Some(index);
            }
        }
        None
    }

    /// One frame at `now`: note what the body did, and hold the keys of the
    /// segment under way - or end, every segment held.
    pub(super) fn step(&mut self, now: f64, craft: Option<&Craft>, wheels: Option<u8>) -> Step {
        let elapsed = now - self.started;
        if let Some(craft) = craft {
            self.log
                .note(elapsed, &Sample::of(craft, wheels), self.airborne_above);
        }
        match self.segment_at(elapsed) {
            Some(index) => Step::Drive(Controls::keys(self.segments[index].keys.clone())),
            None => Step::End(MoveOutcome::Driven),
        }
    }

    /// What `status` says of it at `now`.
    pub(super) fn describe(&self, now: f64) -> (Option<usize>, f64) {
        let elapsed = now - self.started;
        (self.segment_at(elapsed), (self.total() - elapsed).max(0.0))
    }

    /// What the run did, ending at `now`.
    pub(super) fn report(&self, now: f64) -> DriveReport {
        self.log.report(now - self.started)
    }
}

/// What the run reads of the body each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Sample {
    pub(super) position: Vec3,
    pub(super) velocity: Vec3,
    pub(super) forward: Vec3,
    pub(super) up: Vec3,
    /// Its underside above what is below it (m).
    pub(super) height: f32,
    /// A car's wheels on the ground, as its traction counts them - `None`
    /// for a body without that count (a hover-boat).
    pub(super) wheels: Option<u8>,
}

impl Sample {
    fn of(craft: &Craft, wheels: Option<u8>) -> Self {
        Self {
            position: craft.position,
            velocity: craft.velocity,
            forward: craft.forward,
            up: craft.up,
            height: craft.height(),
            wheels,
        }
    }

    /// Off the ground this frame: a car by its wheels (and not lying on
    /// something), any other body by its height.
    fn off_the_ground(&self, airborne_above: f32) -> bool {
        match self.wheels {
            Some(wheels) => wheels == 0 && self.height > RESTING_M,
            None => self.height > airborne_above,
        }
    }

    fn speed(&self) -> f32 {
        self.velocity.xz().length()
    }

    /// How far its up leans from straight up (degrees).
    fn tilt_deg(&self) -> f32 {
        self.up.y.clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// Nose up positive (degrees).
    fn pitch_deg(&self) -> f32 {
        self.forward.y.clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// Right side up positive (degrees).
    fn roll_deg(&self) -> f32 {
        let right = self.forward.cross(self.up);
        right.y.clamp(-1.0, 1.0).asin().to_degrees()
    }
}

/// A jump under way: when and where it left the ground, and the most it has
/// done since.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Air {
    off_s: f64,
    from: Vec2,
    off_y: f32,
    off_speed: f32,
    peak_y: f32,
    max_clearance: f32,
}

/// What a run has done so far.
#[derive(Debug, Clone, Default, PartialEq)]
struct RunLog {
    last: Option<Sample>,
    distance: f32,
    max_speed: f32,
    max_tilt: f32,
    /// Frames running the body has been high enough to be off the ground,
    /// and the first of them - where a jump starts if it holds.
    above: u32,
    first_above: Option<(f64, Sample)>,
    air: Option<Air>,
    jumps: Vec<JumpReport>,
}

impl RunLog {
    fn note(&mut self, elapsed: f64, sample: &Sample, airborne_above: f32) {
        if let Some(last) = self.last {
            self.distance += last.position.xz().distance(sample.position.xz());
        }
        self.max_speed = self.max_speed.max(sample.speed());
        self.max_tilt = self.max_tilt.max(sample.tilt_deg());
        let high = sample.off_the_ground(airborne_above);
        match (high, self.air.as_mut()) {
            (true, Some(air)) => {
                air.peak_y = air.peak_y.max(sample.position.y);
                air.max_clearance = air.max_clearance.max(sample.height);
            }
            (true, None) => {
                self.above += 1;
                let (off_s, off) = *self.first_above.get_or_insert((elapsed, *sample));
                if self.above >= AIRBORNE_FRAMES {
                    // Off the ground since the first frame it was this high.
                    self.air = Some(Air {
                        off_s,
                        from: off.position.xz(),
                        off_y: off.position.y,
                        off_speed: off.speed(),
                        peak_y: off.position.y.max(sample.position.y),
                        max_clearance: off.height.max(sample.height),
                    });
                }
            }
            (false, Some(_)) => {
                if let Some(air) = self.air.take() {
                    self.jumps.push(landed(&air, Some(elapsed), sample));
                }
                self.above = 0;
                self.first_above = None;
            }
            (false, None) => {
                self.above = 0;
                self.first_above = None;
            }
        }
        self.last = Some(*sample);
    }

    fn report(&self, elapsed: f64) -> DriveReport {
        let mut jumps = self.jumps.clone();
        if let (Some(air), Some(last)) = (self.air.as_ref(), self.last.as_ref()) {
            // Still in the air as the drive ended.
            let mut jump = landed(air, None, last);
            jump.airtime_s = hundredths((elapsed - air.off_s) as f32);
            jumps.push(jump);
        }
        DriveReport {
            duration_s: hundredths(elapsed.max(0.0) as f32),
            distance_m: hundredths(self.distance),
            max_speed_ms: hundredths(self.max_speed),
            max_tilt_deg: hundredths(self.max_tilt),
            rolled_over: self.max_tilt > 90.0,
            end_tilt_deg: hundredths(self.last.map_or(0.0, |last| last.tilt_deg())),
            jumps,
        }
    }
}

/// A jump that left as `air` says, back on the ground at `landed_s` (or not
/// yet) as `sample` shows.
fn landed(air: &Air, landed_s: Option<f64>, sample: &Sample) -> JumpReport {
    let to = sample.position.xz();
    JumpReport {
        off_s: hundredths(air.off_s as f32),
        landed_s: landed_s.map(|t| hundredths(t as f32)),
        airtime_s: landed_s.map_or(0.0, |t| hundredths((t - air.off_s) as f32)),
        from: [hundredths(air.from.x), hundredths(air.from.y)],
        to: [hundredths(to.x), hundredths(to.y)],
        distance_m: hundredths(air.from.distance(to)),
        rise_m: hundredths(air.peak_y - air.off_y),
        max_clearance_m: hundredths(air.max_clearance),
        off_speed_ms: hundredths(air.off_speed),
        landing_speed_ms: hundredths(sample.speed()),
        landing_sink_ms: hundredths(-sample.velocity.y),
        landing_pitch_deg: hundredths(sample.pitch_deg()),
        landing_roll_deg: hundredths(sample.roll_deg()),
        landing_tilt_deg: hundredths(sample.tilt_deg()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(keys: &[&str], secs: f32) -> DriveSegment {
        DriveSegment {
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            secs,
        }
    }

    #[test]
    fn keys_are_named_in_any_case_and_unknown_ones_are_refused_by_name() {
        let ok = segments(vec![seg(&["w", "Shift", "SPACE"], 1.0)]).expect("known keys");
        assert_eq!(
            ok[0].keys,
            vec![KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space]
        );
        let err = segments(vec![seg(&["W"], 1.0), seg(&["X"], 1.0)]).expect_err("an unknown key");
        assert!(err.contains("segment 2") && err.contains("\"X\""), "{err}");
    }

    #[test]
    fn a_time_must_be_positive_finite_and_within_the_caps() {
        for secs in [0.0, -1.0, f32::NAN, f32::INFINITY, MAX_SEGMENT_SECS + 0.5] {
            assert!(
                segments(vec![seg(&["W"], secs)]).is_err(),
                "{secs} s accepted"
            );
        }
        let too_long: Vec<_> = (0..4).map(|_| seg(&["W"], 50.0)).collect();
        assert!(segments(too_long).is_err(), "200 s accepted");
        assert!(segments(vec![]).is_err(), "no segment accepted");
        assert!(segments((0..=MAX_SEGMENTS).map(|_| seg(&[], 0.1)).collect()).is_err());
    }

    #[test]
    fn each_segment_is_held_for_its_time_and_the_drive_ends_after_the_last() {
        let run = KeyRun::new(
            segments(vec![seg(&["W"], 2.0), seg(&["W", "D"], 0.5), seg(&[], 1.0)]).unwrap(),
            10.0,
            0.8,
        );
        assert_eq!(run.total(), 3.5);
        assert_eq!(run.segment_at(0.0), Some(0));
        assert_eq!(run.segment_at(1.99), Some(0));
        assert_eq!(run.segment_at(2.0), Some(1));
        assert_eq!(run.segment_at(2.49), Some(1));
        assert_eq!(run.segment_at(3.49), Some(2));
        assert_eq!(run.segment_at(3.5), None);
        let mut run = run;
        match run.step(12.2, None, None) {
            Step::Drive(controls) => assert_eq!(controls.keys, vec![KeyCode::KeyW, KeyCode::KeyD]),
            _ => panic!("the second segment is under way"),
        }
        assert!(matches!(
            run.step(13.6, None, None),
            Step::End(MoveOutcome::Driven)
        ));
    }

    /// A body on a ballistic arc: 14 m/s along -Z, thrown up at 5 m/s from
    /// a lip at y 2 m, its underside `h` above what is below - the frames a
    /// jump off a ramp gives, at 30 frames a second.
    fn frame(t: f64, height: f32, y: f32, vy: f32) -> Sample {
        Sample {
            position: Vec3::new(0.0, y, -14.0 * t as f32),
            velocity: Vec3::new(0.0, vy, -14.0),
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            height,
            wheels: None,
        }
    }

    #[test]
    fn a_jump_is_timed_from_the_first_frame_off_to_the_first_frame_back() {
        let mut log = RunLog::default();
        let above = 0.8;
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        // on the ground for a second: underside riding at 0.47 m
        while t < 1.0 {
            log.note(t, &frame(t, 0.47, 1.0, 0.0), above);
            t += dt;
        }
        // one frame over a bump: not a jump
        log.note(t, &frame(t, 0.9, 1.1, 1.0), above);
        t += dt;
        log.note(t, &frame(t, 0.47, 1.0, 0.0), above);
        t += dt;
        // off at `off`, a 1 s arc peaking 1.25 m up, back on the ground
        let off = t;
        while t < off + 1.0 {
            let s = (t - off) as f32;
            let rise = 5.0 * s - 4.905 * s * s;
            log.note(t, &frame(t, 0.9 + rise, 1.0 + rise, 5.0 - 9.81 * s), above);
            t += dt;
        }
        let back = t;
        log.note(t, &frame(t, 0.47, 1.0, -4.8), above);
        let report = log.report(t + dt);

        assert_eq!(report.jumps.len(), 1, "{:?}", report.jumps);
        let jump = &report.jumps[0];
        assert_eq!(jump.off_s, hundredths(off as f32));
        assert_eq!(jump.landed_s, Some(hundredths(back as f32)));
        assert!((jump.airtime_s - 1.0).abs() < 0.05, "{jump:?}");
        assert!((jump.rise_m - 1.27).abs() < 0.05, "{jump:?}");
        assert!((jump.distance_m - 14.0).abs() < 0.8, "{jump:?}");
        assert_eq!(jump.off_speed_ms, 14.0);
        assert_eq!(jump.landing_sink_ms, 4.8);
        assert!(!report.rolled_over);
    }

    /// A car is read by its wheels: off the ground only with none down and
    /// its body clear of what is below - over a tabletop's deck its height
    /// alone would read low, and a car lying on its side after a crash has no
    /// wheel down either but is not flying.
    #[test]
    fn a_car_is_off_the_ground_by_its_wheels_not_its_height() {
        let above = 0.65;
        let with = |height: f32, wheels: u8| Sample {
            wheels: Some(wheels),
            ..frame(0.0, height, 1.0, 0.0)
        };
        // over a deck, 0.5 m clear, no wheel down: flying, though low
        assert!(with(0.5, 0).off_the_ground(above));
        // riding its springs, one wheel down: on the ground, though high
        assert!(!with(0.9, 1).off_the_ground(above));
        // on its side on the ground: no wheel down, but resting
        assert!(!with(0.0, 0).off_the_ground(above));
        // a body with no count keeps the height rule
        assert!(frame(0.0, 0.7, 1.0, 0.0).off_the_ground(above));
        assert!(!frame(0.0, 0.5, 1.0, 0.0).off_the_ground(above));

        let mut log = RunLog::default();
        let mut t = 0.0;
        for (height, wheels) in [
            (0.47, 4),
            (0.5, 0),
            (0.6, 0),
            (0.7, 0),
            (0.47, 2),
            (0.0, 0),
            (0.0, 0),
        ] {
            log.note(t, &with(height, wheels), above);
            t += 0.1;
        }
        let report = log.report(t);
        assert_eq!(report.jumps.len(), 1, "{:?}", report.jumps);
        assert_eq!(report.jumps[0].off_s, 0.1);
        assert_eq!(report.jumps[0].landed_s, Some(0.4));
    }

    #[test]
    fn a_body_still_in_the_air_as_the_drive_ends_reports_its_jump_open() {
        let mut log = RunLog::default();
        log.note(0.0, &frame(0.0, 0.47, 1.0, 0.0), 0.8);
        log.note(0.1, &frame(0.1, 1.5, 2.0, 3.0), 0.8);
        log.note(0.2, &frame(0.2, 2.0, 2.5, 2.0), 0.8);
        let report = log.report(0.5);
        assert_eq!(report.jumps.len(), 1);
        assert_eq!(report.jumps[0].landed_s, None);
        assert_eq!(report.jumps[0].airtime_s, 0.4);
    }

    #[test]
    fn a_roll_past_ninety_degrees_is_a_rollover_and_the_end_tilt_is_kept() {
        let mut log = RunLog::default();
        let mut on_side = frame(0.0, 0.2, 1.0, 0.0);
        on_side.up = Vec3::X;
        on_side.forward = Vec3::NEG_Z;
        log.note(0.0, &on_side, 0.8);
        let mut roof = frame(0.1, 0.2, 1.0, 0.0);
        roof.up = Vec3::new(0.1, -0.99, 0.0).normalize();
        log.note(0.1, &roof, 0.8);
        let upright = frame(0.2, 0.47, 1.0, 0.0);
        log.note(0.2, &upright, 0.8);
        let report = log.report(0.3);
        assert!(
            report.rolled_over && report.max_tilt_deg > 170.0,
            "{report:?}"
        );
        assert_eq!(report.end_tilt_deg, 0.0);
    }

    #[test]
    fn landing_attitude_reads_nose_up_and_right_side_up_as_positive() {
        let mut nose_up = frame(0.0, 0.47, 1.0, 0.0);
        nose_up.forward = Vec3::new(0.0, 0.5, -0.866);
        nose_up.up = Vec3::new(0.0, 0.866, 0.5);
        assert!((nose_up.pitch_deg() - 30.0).abs() < 0.1);
        let mut right_up = frame(0.0, 0.47, 1.0, 0.0);
        // rolled left: the right side rises
        right_up.up = Vec3::new(-0.5, 0.866, 0.0);
        assert!(right_up.roll_deg() > 29.0, "{}", right_up.roll_deg());
    }

    /// A body that comes down on its roof reads a landing tilt past 90,
    /// where its pitch and roll - each an arcsine, which folds at 90 - read
    /// the same as a level landing's (session 893's end review).
    #[test]
    fn a_landing_on_the_roof_reads_its_tilt() {
        let air = Air {
            off_s: 0.0,
            from: Vec2::ZERO,
            off_y: 1.0,
            off_speed: 14.0,
            peak_y: 2.0,
            max_clearance: 1.5,
        };
        let level = frame(1.0, 0.47, 1.0, -5.0);
        let mut roof = level;
        roof.up = Vec3::NEG_Y;
        let flat = landed(&air, Some(1.0), &level);
        let upside_down = landed(&air, Some(1.0), &roof);
        assert_eq!(
            (upside_down.landing_pitch_deg, upside_down.landing_roll_deg),
            (flat.landing_pitch_deg, flat.landing_roll_deg),
            "the premise: pitch and roll cannot tell the two apart"
        );
        assert_eq!(flat.landing_tilt_deg, 0.0);
        assert_eq!(upside_down.landing_tilt_deg, 180.0);
    }

    #[test]
    fn only_a_body_on_wheels_can_drive() {
        let car = LocomotionConfig::Car(Box::default());
        assert!(airborne_above(&car).is_some_and(|h| h > 0.6));
        let heli = LocomotionConfig::Helicopter(Box::default());
        assert!(airborne_above(&heli).is_none());
    }
}
