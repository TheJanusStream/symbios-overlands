//! A rigged winged body flown by its chassis (#1877): the prototype's flight
//! handed to the engine's airborne motion, the landing held until it has
//! settled, the phase and the contact handed only where they mean what the
//! engine reads them as, a peer's words taken as its body is drawn there,
//! and persons and beasts flying the prototype drawn as they always were.

use super::*;
use crate::pds::FlightParams;
use crate::pds::avatar::locomotion::{default_winged, fitted_record};
use crate::player::flight::{Approach, Flight, FlightKeys, FlightMode, PeerFlight, Reading, Wings};
use crate::player::rigged::underfoot::TAKEOFF_SECS;
use symbios_avatar::{Flown, Source, WingedFamily};

/// The game's fixed step.
const STEP: f32 = 1.0 / 64.0;

/// A chassis flown by the prototype's own model - [`Flight::stepped`], the
/// pure step `fly_on_foot` takes - over level ground at `y = 0`, with the
/// physics' part done by hand: gravity, and the ground the capsule stands
/// on - level at `y = 0` unless told otherwise. A top is closed on straight
/// down the path, as the model's sphere cast reads level ground; it lands
/// within the prototype's ten centimetres.
struct Sky {
    flight: Flight,
    wings: Wings,
    params: FlightParams,
    /// The chassis' origin, its capsule's middle.
    at: Vec3,
    velocity: Vec3,
    /// How far the feet are under the origin.
    feet: f32,
    /// The walk's velocity, on its feet.
    walk: Vec3,
    keys: FlightKeys,
    /// A time to contact said whatever the ground is: a roof the path closes
    /// on.
    roof: Option<f32>,
    /// Physics owed to the next fixed step, seconds.
    owed: f32,
    /// A lake over the whole world, its surface at `y = 0` and its bed
    /// three metres down - and the body's water column, which says how deep
    /// it wades.
    lake: Option<(WaterSurfaces, WaterColumn)>,
    /// The ground's height at each world `(x, z)`, out of the lake.
    ground: fn(f32, f32) -> f32,
}

impl Sky {
    fn of(record: &AvatarRecord) -> Self {
        let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
            panic!("a body on foot");
        };
        let params = p.flight.clone().expect("a body that flies");
        let feet = p.total_height() * 0.5;
        Self {
            flight: Flight::default(),
            wings: Wings::of(&params, p).lying(p.is_lying()),
            params,
            at: Vec3::Y * feet,
            velocity: Vec3::ZERO,
            feet,
            walk: Vec3::ZERO,
            keys: FlightKeys::default(),
            roof: None,
            owed: 0.0,
            lake: None,
            ground: |_, _| 0.0,
        }
    }

    fn rotation(&self) -> Quat {
        if self.flight.mode().airborne() {
            self.flight.rotation()
        } else {
            Quat::from_rotation_y(self.flight.attitude().0)
        }
    }

    /// The ground's height under the chassis: the lake's bed, or the ground.
    fn floor(&self) -> f32 {
        if self.lake.is_some() {
            -3.0
        } else {
            (self.ground)(self.at.x, self.at.z)
        }
    }

    fn step(&mut self) {
        let over = self.at.y - self.feet - self.floor();
        let in_water = self.lake.as_ref().is_some_and(|(water, column)| {
            water_state(
                false,
                self.at.y,
                Vec2::new(self.at.x, self.at.z),
                *column,
                water,
            ) != WaterState::Dry
        });
        let mode = self.flight.mode();
        let looking = mode.airborne() && mode != FlightMode::Land;
        let approach = match self.roof {
            Some(tau) => Some(tau),
            None => (looking && self.velocity.y < 0.0 && self.velocity.length() >= 0.5)
                .then_some(over / -self.velocity.y)
                .filter(|tau| *tau <= 1.5),
        }
        .filter(|_| looking)
        .map(|tau| Approach {
            tau,
            point: self.at - Vec3::Y * self.feet + self.velocity * tau,
        });
        let now = Reading {
            position: self.at,
            velocity: self.velocity,
            facing: crate::player::flight::heading_of(self.rotation()),
            on_ground: over <= 0.05,
            resting: over <= 0.1 && self.roof.is_none(),
            approach,
            in_water,
            gravity: 9.81,
        };
        let (next, moved) = self.flight.stepped(&self.wings, self.keys, now, STEP);
        self.flight = next;
        self.keys.press = false;
        self.velocity = match moved {
            Some(velocity) => velocity - Vec3::Y * 9.81 * STEP,
            // In the lake it sinks to where it swims, then floats there.
            None if in_water => {
                let swimming = self.lake.as_ref().is_some_and(|(water, column)| {
                    matches!(
                        water_state(
                            true,
                            self.at.y,
                            Vec2::new(self.at.x, self.at.z),
                            *column,
                            water
                        ),
                        WaterState::Swimming { .. }
                    )
                });
                let drift = self.velocity.with_y(0.0) * 0.95;
                if swimming {
                    drift
                } else {
                    drift - Vec3::Y * 0.8
                }
            }
            None => self.walk,
        };
        self.at += self.velocity * STEP;
        let floor = self.floor() + self.feet;
        if self.at.y < floor {
            self.at.y = floor;
            self.velocity.y = self.velocity.y.max(0.0);
        }
    }

    /// The physics of a frame `frame` seconds long, at the fixed step.
    fn advance(&mut self, frame: f32) {
        self.owed += frame;
        while self.owed >= STEP {
            self.step();
            self.owed -= STEP;
        }
    }

    /// Write the chassis as the physics leaves it before `Update`.
    fn write(&self, app: &mut App, chassis: Entity) {
        let mut chassis = app.world_mut().entity_mut(chassis);
        let mut place = chassis.get_mut::<Transform>().expect("a place");
        place.translation = self.at;
        place.rotation = self.rotation();
        chassis.get_mut::<LinearVelocity>().expect("a velocity").0 = self.velocity;
        *chassis.get_mut::<Flight>().expect("a flight") = self.flight;
    }
}

/// One frame of a flown body: what the chassis was doing, what the engine
/// was handed and what it says it posed, what carried the body, and its
/// joints in its own frame.
struct Frame {
    chassis: FlightMode,
    flight: Flight,
    handed: Option<symbios_avatar::anim::Flight>,
    flown: Option<Flown>,
    source: Source,
    joints: Vec<Vec3>,
}

/// What the frame just run left on `root`.
fn read(app: &App, root: Entity, chassis: Entity) -> Frame {
    let rig = rig_of(app, root);
    let world = app.world();
    Frame {
        chassis: world.get::<Flight>(chassis).expect("a flight").mode(),
        flight: *world.get::<Flight>(chassis).expect("a flight"),
        handed: world.get::<Drive>(root).expect("a drive").flight,
        flown: world
            .get::<AvatarDriver>(root)
            .expect("a driver")
            .flight(&rig),
        source: world
            .get::<bevy_symbios_avatar::Drove>(root)
            .expect("driven")
            .source,
        joints: world
            .get::<bevy_symbios_avatar::AvatarPose>(root)
            .expect("posed")
            .0
            .forward(&rig)
            .positions,
    }
}

/// `record` landed on its owner's chassis with a flight, at `hz` frames a
/// second, standing: the app, the chassis, the body and its sky.
fn owned(record: &AvatarRecord, body: &EngineAvatarRecord, hz: f32) -> (App, Entity, Entity, Sky) {
    let offset = crate::player::visuals::rigged_root_offset(record);
    let mut app = app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / hz,
    )));
    let chassis = chassis(&mut app, Under::Local, record.clone(), Vec3::Y * offset);
    app.world_mut()
        .entity_mut(chassis)
        .insert(Flight::default());
    land(&mut app, chassis, body, 64, offset);
    let root = root_of(&mut app, chassis);
    (app, chassis, root, Sky::of(record))
}

/// A flown life of `family` at `hz`: stand half a second, take off and hold
/// the flap key for 2.5 s, glide down onto the ground, land, and stand a
/// second and a half on its feet - every frame read.
fn life(family: WingedFamily, hz: f32) -> (Vec<Frame>, f32) {
    let body = default_winged(family);
    let record = fitted_record(body.clone());
    let (mut app, chassis, root, mut sky) = owned(&record, &body, hz);
    let frame = 1.0 / hz;
    let mut frames = Vec::new();
    let mut t = 0.0f32;
    let mut stood: Option<f32> = None;
    while t < 20.0 && stood.is_none_or(|since| t - since < 1.5) {
        if (t - 0.5).abs() < frame * 0.5 {
            sky.keys.press = true;
        }
        sky.keys.held = (0.5..3.0).contains(&t);
        sky.advance(frame);
        sky.write(&mut app, chassis);
        app.update();
        let read = read(&app, root, chassis);
        if t > 0.5 && read.chassis == FlightMode::Ground && stood.is_none() {
            stood = Some(t);
        }
        frames.push(read);
        t += frame;
    }
    assert!(stood.is_some(), "{family:?} at {hz} Hz never landed");
    (frames, sky.params.wingbeat_rate.0)
}

/// The trunk's length, shoulder to hip: the engine's own scale of a pop.
fn trunk(rig: &Rig) -> f32 {
    let pelvis = rig.in_zone(Zone::Pelvis)[0];
    let withers = *rig.in_zone(Zone::Chest).last().expect("a chest");
    rig.joints[pelvis]
        .position
        .distance(rig.joints[withers].position)
}

/// Every frame where some joint moved, in the body's own frame, more than
/// twice the most it moved in the four frames either side and `slack`: the
/// engine's own bound on a winged life (its `winged_life`, `pops`).
fn pops(frames: &[Frame], slack: f32) -> Vec<(usize, f32, f32)> {
    let steps: Vec<f32> = (1..frames.len())
        .map(|at| moved(&frames[at - 1].joints, &frames[at].joints))
        .collect();
    (4..steps.len().saturating_sub(4))
        .filter_map(|at| {
            let around = (at - 4..at)
                .chain(at + 1..=at + 4)
                .map(|frame| steps[frame])
                .fold(0.0f32, f32::max);
            (steps[at] > around * 2.0 + slack).then_some((at + 1, steps[at], around))
        })
        .collect()
}

/// The engine's names, for the comparisons.
type Posed = symbios_avatar::FlightMode;

/// **A bird and a bat fly a whole life through their chassis' modes, and the
/// engine poses each** (#1877): on the ground, the take-off, flapping, a
/// glide, the flare by time to contact, the landing and on the ground again,
/// at 30, 60 and 144 frames a second. Every frame the chassis is off its
/// feet the engine carries the body (`Source::Fly`); the landing is handed
/// on after the chassis is on its feet, and handed back - no flight - only
/// once the engine has said it settled; and no joint jumps across a hand-on
/// or a hand-back (the engine's own `winged_life` bound). Handed back with
/// the chassis, 0.25 s after it lands, the landing was cut mid-fold: the
/// engine had not settled.
#[test]
fn a_winged_body_flies_its_chassis_modes_and_lands_settled() {
    for family in [WingedFamily::Bird, WingedFamily::Bat] {
        for hz in [30.0, 60.0, 144.0] {
            let (frames, _) = life(family, hz);
            let label = format!("{family:?} at {hz} Hz");
            let handed: Vec<Option<Posed>> = frames
                .iter()
                .map(|frame| frame.handed.map(|flight| flight.mode))
                .collect();
            let mut seen: Vec<(Option<Posed>, FlightMode, usize)> = Vec::new();
            for (frame, mode) in frames.iter().zip(&handed) {
                match seen.last_mut() {
                    Some(last) if last.0 == *mode && last.1 == frame.chassis => last.2 += 1,
                    _ => seen.push((*mode, frame.chassis, 1)),
                }
            }
            for wanted in [
                Posed::Takeoff,
                Posed::Flap,
                Posed::Glide,
                Posed::Flare,
                Posed::Land,
            ] {
                assert!(
                    handed.contains(&Some(wanted)),
                    "{label}: never {wanted:?}: {seen:?}"
                );
            }
            for (at, frame) in frames.iter().enumerate() {
                if frame.chassis.airborne() {
                    assert!(frame.handed.is_some(), "{label}: frame {at} handed nothing");
                    assert_eq!(frame.source, Source::Fly, "{label}: frame {at}");
                }
            }
            let held = frames
                .iter()
                .filter(|frame| {
                    frame.chassis == FlightMode::Ground
                        && frame
                            .handed
                            .is_some_and(|flight| flight.mode == Posed::Land)
                })
                .count();
            assert!(
                held > 0,
                "{label}: the landing was handed back with the chassis"
            );
            let back = (1..frames.len())
                .find(|&at| frames[at].handed.is_none() && frames[at - 1].handed.is_some())
                .unwrap_or_else(|| panic!("{label}: never handed back"));
            let before = frames[back - 1].flown.expect("flown before the hand-back");
            assert!(before.settled, "{label}: handed back before it settled");
            let rig = default_rig(family);
            let found = pops(&frames, 0.01 * trunk(&rig));
            assert!(
                found.is_empty(),
                "{label}: joints popped {}",
                found
                    .iter()
                    .map(|(at, moved, around)| format!(
                        "frame {at} ({:?} after {:?}) {:.1} mm against {:.1}",
                        frames[*at].handed.map(|f| f.mode),
                        frames[*at - 1].handed.map(|f| f.mode),
                        moved * 1000.0,
                        around * 1000.0
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }
}

/// The rig a body of `family` builds.
fn default_rig(family: WingedFamily) -> Rig {
    built(&default_winged(family), 64).rig
}

/// **The beat's phase goes only in Flap, and the time to contact only in
/// Flare** (#1877): the phase off the chassis' own count of beats while it
/// beats, and the time to contact of the top it closes on while it flares -
/// and neither anywhere else, nor ever from a peer.
///
/// **The chassis' push is the middle of the engine's downstroke** (the
/// engine's `Flight::phase`: a host that pushes at mid-downstroke hands half
/// the downstroke at its push): the phase handed is the chassis' count of
/// beats, half a [`Flown::downstroke`] on. Handed the count alone, every push
/// was drawn at the top of a downstroke, the wings still raised.
#[test]
fn the_phase_goes_only_in_flap_and_the_contact_only_in_flare() {
    let (frames, rate) = life(WingedFamily::Bird, 60.0);
    let downstroke = frames
        .iter()
        .find_map(|frame| frame.flown)
        .expect("the premise: it flew")
        .downstroke;
    assert!(
        downstroke > 0.1,
        "the premise: a downstroke of {downstroke}"
    );
    let (mut flapped, mut flared) = (0, 0);
    for (at, frame) in frames.iter().enumerate() {
        let Some(handed) = frame.handed else {
            continue;
        };
        let phase = (handed.mode == Posed::Flap && frame.chassis == FlightMode::Flap)
            .then(|| (frame.flight.beat_phase(rate) + 0.5 * downstroke).rem_euclid(64.0));
        assert_eq!(handed.phase, phase, "frame {at} in {:?}", handed.mode);
        let contact = (handed.mode == Posed::Flare)
            .then(|| frame.flight.contact())
            .flatten();
        assert_eq!(handed.contact, contact, "frame {at} in {:?}", handed.mode);
        assert_eq!(handed.air, None, "frame {at}");
        flapped += usize::from(phase.is_some());
        flared += usize::from(contact.is_some());
    }
    assert!(
        flapped > 10 && flared > 2,
        "the premise: {flapped} flapping, {flared} flaring"
    );
}

/// **A pass with the flap key held near a top is posed beating, not
/// flaring** (#1877): the chassis does not flare while its key is held, and
/// the engine flares whatever it is told once the contact it is handed is
/// under its 0.45 s - so the contact goes in Flare alone. Handed in every
/// mode, a bird beating its way over a roof 0.3 s off was posed in a flare.
#[test]
fn a_pass_beating_near_a_top_is_not_posed_as_a_flare() {
    let body = default_winged(WingedFamily::Bird);
    let record = fitted_record(body.clone());
    let (mut app, chassis, root, mut sky) = owned(&record, &body, 60.0);
    sky.at.y += 3.0;
    sky.velocity = Vec3::new(0.0, -0.2, -6.0);
    let mut flying = Flight::default();
    flying.set_mode_for_tests(FlightMode::Flap);
    sky.flight = flying;
    sky.keys.held = true;
    sky.roof = Some(0.3);
    let mut posed = Vec::new();
    for _ in 0..60 {
        sky.advance(1.0 / 60.0);
        sky.write(&mut app, chassis);
        app.update();
        let frame = read(&app, root, chassis);
        assert_eq!(frame.chassis, FlightMode::Flap, "the premise: it beats on");
        posed.push(frame.flown.map(|flown| flown.mode));
    }
    assert!(
        posed[10..].iter().all(|mode| *mode == Some(Posed::Flap)),
        "posed {posed:?}"
    );
}

/// **Walking out of a landing lets it go at once** (#1877): a body held in
/// its landing until it settles, set walking faster than the engine's idle,
/// is handed back to the ground that frame, so its stride does not skate on
/// feet the landing planted. Held on, it walked a landing.
#[test]
fn walking_out_of_a_landing_lets_it_go() {
    let body = default_winged(WingedFamily::Bird);
    let record = fitted_record(body.clone());
    let (mut app, chassis, root, mut sky) = owned(&record, &body, 60.0);
    sky.at.y += 2.0;
    sky.velocity = Vec3::new(0.0, -1.0, -4.0);
    let mut gliding = Flight::default();
    gliding.set_mode_for_tests(FlightMode::Glide);
    sky.flight = gliding;
    let mut held = false;
    for _ in 0..600 {
        sky.advance(1.0 / 60.0);
        sky.write(&mut app, chassis);
        app.update();
        let frame = read(&app, root, chassis);
        let holding = frame.chassis == FlightMode::Ground
            && frame
                .handed
                .is_some_and(|flight| flight.mode == Posed::Land);
        if holding && !held {
            held = true;
            sky.walk = Vec3::new(0.0, 0.0, -1.2);
            continue;
        }
        if held {
            assert_eq!(
                frame.handed.map(|flight| flight.mode),
                None,
                "walking at {:.2} m/s, still landing",
                sky.velocity.length()
            );
            return;
        }
    }
    panic!("the premise: it never held a landing");
}

/// The frames of a peer's flown life, played out as its owner flew it: the
/// owner's chassis recorded at each of its frames, played back
/// [`crate::player::flight::PEER_DELAY_S`] late as the transform playout
/// does, and its words heard as the owner's mode changed - except that a
/// landing's Land and the Ground after it are heard in one frame, a quick
/// pair the reliable channel delivers together. Returns the frames, and the
/// ground's height the drawn chassis stood at when the engine was first
/// handed Land.
fn peer_life(family: WingedFamily) -> (Vec<Frame>, f32) {
    let body = default_winged(family);
    let record = fitted_record(body.clone());
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let frame = 1.0 / 60.0;
    // The owner's flight, frame by frame.
    let mut sky = Sky::of(&record);
    sky.at.y += 2.5;
    sky.velocity = Vec3::new(0.0, -1.0, -5.0);
    let mut gliding = Flight::default();
    gliding.set_mode_for_tests(FlightMode::Glide);
    sky.flight = gliding;
    let mut owner: Vec<(Vec3, Quat, FlightMode)> = Vec::new();
    for _ in 0..480 {
        sky.advance(frame);
        owner.push((sky.at, sky.rotation(), sky.flight.mode()));
    }
    let mut app = app();
    let chassis = chassis(&mut app, Under::Peer, record, owner[0].0);
    // The chassis stands an update before its body lands, as a peer's does
    // in the game, whose build lands frames after the chassis spawns (#1902,
    // the rule #1889 found in `taking_off`). Here the first fill read
    // the chassis' real place either way, and no figure below moved.
    app.update();
    land(&mut app, chassis, &body, 64, offset);
    let root = root_of(&mut app, chassis);
    let delay = (crate::player::flight::PEER_DELAY_S as f32 / frame).round() as usize;
    let mut frames = Vec::new();
    let mut heard = FlightMode::Ground;
    let mut first_land = None;
    for at in 0..owner.len() {
        let now = app.world().resource::<Time>().elapsed_secs_f64();
        // The word of this frame: the owner's mode, Land and Ground at once.
        let said = owner[at].2;
        let words: Vec<FlightMode> = match said {
            FlightMode::Land => vec![],
            FlightMode::Ground if heard != FlightMode::Ground => {
                vec![FlightMode::Land, FlightMode::Ground]
            }
            mode if mode != heard => vec![mode],
            _ => vec![],
        };
        for word in words {
            heard = word;
            let mut entity = app.world_mut().entity_mut(chassis);
            match entity.get_mut::<PeerFlight>() {
                Some(mut flight) => flight.hear(word, now),
                None => {
                    entity.insert(PeerFlight::heard_at(word, now));
                }
            }
        }
        let (place, turn, _) = owner[at.saturating_sub(delay)];
        let mut transform = app
            .world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a place");
        transform.translation = place;
        transform.rotation = turn;
        app.update();
        let handed = app.world().get::<Drive>(root).expect("a drive").flight;
        if first_land.is_none() && handed.is_some_and(|flight| flight.mode == Posed::Land) {
            first_land = Some(place.y - offset);
        }
        let rig = rig_of(&app, root);
        let world = app.world();
        frames.push(Frame {
            chassis: owner[at].2,
            flight: Flight::default(),
            handed,
            flown: world
                .get::<AvatarDriver>(root)
                .expect("a driver")
                .flight(&rig),
            source: world
                .get::<bevy_symbios_avatar::Drove>(root)
                .expect("driven")
                .source,
            joints: world
                .get::<bevy_symbios_avatar::AvatarPose>(root)
                .expect("posed")
                .0
                .forward(&rig)
                .positions,
        });
    }
    (
        frames,
        first_land.expect("the engine was never handed a landing"),
    )
}

/// **A peer's quick Land then Ground still plays its landing, as its body is
/// drawn coming down** (#1877): its words wait the transform playout's delay,
/// so the engine is handed Land when the drawn chassis is down rather than a
/// tenth of a second over the ground, and a pair heard in one frame is taken
/// one a frame, so the Land is played and held until it settles. Kept as the
/// last word only, the pair was a Ground straight from the glide: no landing
/// at all.
#[test]
fn a_peers_quick_land_then_ground_still_plays_its_landing() {
    for family in [WingedFamily::Bird, WingedFamily::Bat] {
        let (frames, over) = peer_life(family);
        assert!(
            over.abs() < 0.03,
            "{family:?}: handed Land {:.0} mm over the ground",
            over * 1000.0
        );
        let back = (1..frames.len())
            .find(|&at| frames[at].handed.is_none() && frames[at - 1].handed.is_some())
            .unwrap_or_else(|| panic!("{family:?}: never handed back"));
        assert!(
            frames[back - 1].flown.is_some_and(|flown| flown.settled),
            "{family:?}: handed back before it settled"
        );
        assert!(
            frames.iter().any(|frame| frame.source == Source::Fly),
            "{family:?}: never flew"
        );
    }
}

/// **Space in a held landing takes off with a crouch and a push** (#1877):
/// a body taken off while its landing is still held until it settles is
/// handed no flight for a frame, so the engine starts a fresh takeoff - its
/// crouch and its legs' push, which it plays only into a new flight - rather
/// than turning the landing it holds into a takeoff with no ground to push
/// off. Bird and bat, at 30, 60 and 144 frames a second, with no joint
/// jumping (the engine's own `winged_life` bound). Turned straight from the
/// held landing, the engine said the feet were off from the takeoff's first
/// frame: no crouch, no push.
#[test]
fn space_in_a_held_landing_takes_off_with_a_crouch_and_a_push() {
    for family in [WingedFamily::Bird, WingedFamily::Bat] {
        for hz in [30.0, 60.0, 144.0] {
            let label = format!("{family:?} at {hz} Hz");
            let body = default_winged(family);
            let record = fitted_record(body.clone());
            let (mut app, chassis, root, mut sky) = owned(&record, &body, hz);
            sky.at.y += 1.5;
            sky.velocity = Vec3::new(0.0, -1.0, -4.0);
            let mut gliding = Flight::default();
            gliding.set_mode_for_tests(FlightMode::Glide);
            sky.flight = gliding;
            let frame = 1.0 / hz;
            let mut frames = Vec::new();
            let mut pressed: Option<usize> = None;
            let mut held_for = 0.0f32;
            for at in 0..(4.0 * hz) as usize {
                sky.keys.held = pressed.is_some_and(|from| (at - from) as f32 * frame < 1.0);
                sky.advance(frame);
                sky.write(&mut app, chassis);
                app.update();
                let read = read(&app, root, chassis);
                let holding = read.chassis == FlightMode::Ground
                    && read.handed.is_some_and(|flight| flight.mode == Posed::Land);
                if holding && pressed.is_none() {
                    held_for += frame;
                    // A tenth of a second into the hold, well before it settles.
                    if held_for >= 0.1 {
                        assert!(
                            read.flown.is_some_and(|flown| !flown.settled),
                            "{label}: the premise, the landing still held"
                        );
                        sky.keys.press = true;
                        pressed = Some(at);
                    }
                }
                frames.push(read);
            }
            let from = pressed.unwrap_or_else(|| panic!("{label}: the premise, a held landing"));
            let after = &frames[from + 1..];
            assert!(
                after
                    .iter()
                    .any(|frame| frame.chassis == FlightMode::Takeoff),
                "{label}: the premise, it took off"
            );
            assert!(
                after.iter().any(|frame| frame
                    .flown
                    .is_some_and(|flown| flown.mode == Posed::Takeoff && !flown.lifted)),
                "{label}: the takeoff never pushed: handed {:?}",
                after
                    .iter()
                    .take(12)
                    .map(|frame| frame.handed.map(|flight| flight.mode))
                    .collect::<Vec<_>>()
            );
            let rig = default_rig(family);
            let found = pops(&frames, 0.01 * trunk(&rig));
            assert!(
                found.is_empty(),
                "{label}: joints popped {}",
                found
                    .iter()
                    .map(|(at, moved, around)| format!(
                        "frame {at} ({:?} after {:?}) {:.1} mm against {:.1}",
                        frames[*at].handed.map(|f| f.mode),
                        frames[*at - 1].handed.map(|f| f.mode),
                        moved * 1000.0,
                        around * 1000.0
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }
}

/// **A body landing tilted is handed no patch until it is level** (#1877):
/// a bat and a glider gliding down across a 20 degree hillside and onto a
/// 0.6 m deck - the terrain's ground and a structure's, each laid as the game
/// lays it - are handed no ground patch on any frame their flight is handed
/// on with the root turned more than [`crate::player::rigged::underfoot::LEVEL_WITHIN`] off level,
/// as a landing's chassis is for its first tenth of a second down. Once
/// level and down, the patch is handed from that frame, easing in, and no
/// joint jumps on it: its step is no more than twice the most in the four
/// frames either side and a hundredth of the trunk (the engine's own
/// `winged_life` bound). Handed while still pitched, the patch - sampled for
/// a root turned about `Y` alone - put the feet a tilted body reaches for on
/// ground that is not there; handed whole once level, a bat across the
/// hillside moved a joint 330 mm on that frame against at most 133 in the
/// four frames either side, and a glider 176 against 33 (symbios-avatar
/// 0.23.1). Eased in, 64 against 77 and 34 against 42.
#[test]
fn a_body_landing_tilted_is_handed_no_patch_until_it_is_level() {
    const DECK: f32 = 0.6;
    // Rising to its right as it flies: across its path, so the flight's
    // look straight down reads its time to contact true.
    let slope = |x: f32, _: f32| x * 20f32.to_radians().tan();
    let deck = |x: f32, z: f32| {
        if x.abs() < 4.0 && z.abs() < 4.0 {
            DECK
        } else {
            0.0
        }
    };
    for family in [WingedFamily::Bat, WingedFamily::Glider] {
        for (name, ground) in [
            ("a 20 degree slope", slope as fn(f32, f32) -> f32),
            ("a deck", deck as fn(f32, f32) -> f32),
        ] {
            let label = format!("{family:?} on {name}");
            let solids: Vec<(Vec3, Transform)> = if name == "a deck" {
                vec![block(Vec3::new(-4.0, 0.0, -4.0), Vec3::new(4.0, DECK, 4.0))]
            } else {
                Vec::new()
            };
            let terrain_height: &dyn Fn(f32, f32) -> f32 = if name == "a deck" {
                &|_: f32, _: f32| 0.0
            } else {
                &slope
            };
            let mut app = grounded(terrain_height, &solids);
            let body = default_winged(family);
            let record = fitted_record(body.clone());
            let offset = crate::player::visuals::rigged_root_offset(&record);
            let mut sky = Sky::of(&record);
            sky.ground = ground;
            sky.at = Vec3::new(0.0, ground(0.0, 2.0) + sky.feet + 1.2, 2.0);
            sky.velocity = Vec3::new(0.0, -1.0, -4.0);
            let mut gliding = Flight::default();
            gliding.set_mode_for_tests(FlightMode::Glide);
            sky.flight = gliding;
            let chassis = chassis(&mut app, Under::Local, record.clone(), sky.at);
            app.world_mut()
                .entity_mut(chassis)
                .insert(Flight::default());
            land(&mut app, chassis, &body, 64, offset);
            let root = root_of(&mut app, chassis);
            // Every frame: its joints, whether it was handed a patch, whether
            // its flight was handed on, its tilt off level and its chassis'
            // mode.
            let mut frames: Vec<(Vec<Vec3>, bool, bool, f32, FlightMode)> = Vec::new();
            for _ in 0..120 {
                sky.advance(FRAME);
                sky.write(&mut app, chassis);
                app.update();
                let tilt = (sky.rotation() * Vec3::Y).y.clamp(-1.0, 1.0).acos();
                let drive = app.world().get::<Drive>(root).expect("a drive");
                let (patch, aloft) = (drive.ground.is_some(), drive.flight.is_some());
                let joints = read(&app, root, chassis).joints;
                frames.push((joints, patch, aloft, tilt, sky.flight.mode()));
            }
            let tilted = |frame: &(Vec<Vec3>, bool, bool, f32, FlightMode)| {
                frame.2 && frame.3 > crate::player::rigged::underfoot::LEVEL_WITHIN
            };
            let down_tilted = frames
                .iter()
                .filter(|frame| tilted(frame) && frame.4 == FlightMode::Land)
                .count();
            assert!(
                down_tilted > 2,
                "{label}: the premise, down and tilted for {down_tilted} frames"
            );
            for (at, frame) in frames.iter().enumerate() {
                assert!(
                    !(tilted(frame) && frame.1),
                    "{label}: frame {at} handed a patch tilted {:.1} degrees in {:?}",
                    frame.3.to_degrees(),
                    frame.4
                );
            }
            // Down - the chassis landing or on its feet - and level.
            let down = |frame: &(Vec<Vec3>, bool, bool, f32, FlightMode)| {
                matches!(frame.4, FlightMode::Land | FlightMode::Ground)
            };
            let level = (1..frames.len())
                .find(|&at| {
                    tilted(&frames[at - 1])
                        && down(&frames[at - 1])
                        && !tilted(&frames[at])
                        && frames[at].2
                })
                .unwrap_or_else(|| panic!("{label}: never levelled in its landing"));
            assert!(
                frames[level].1,
                "{label}: level at frame {level} in {:?} and handed no patch",
                frames[level].4
            );
            let steps: Vec<f32> = (1..frames.len())
                .map(|at| moved(&frames[at - 1].0, &frames[at].0))
                .collect();
            let arrives = level - 1;
            let around = (arrives - 4..arrives)
                .chain(arrives + 1..=arrives + 4)
                .map(|at| steps[at])
                .fold(0.0f32, f32::max);
            let slack = 0.01 * trunk(&default_rig(family));
            assert!(
                steps[arrives] <= around * 2.0 + slack,
                "{label}: the patch arrived with a {:.1} mm step against {:.1}",
                steps[arrives] * 1000.0,
                around * 1000.0
            );
        }
    }
}

/// One frame of a take-off ([`taking_off`]): the body's joints and their
/// turns in its own frame, the least a foot stands over the ground under it,
/// the flight it was handed, whether it was handed a patch, how much of the
/// terrain's and a structure's ground its hold carries, and whether its
/// chassis was rising.
struct Lifting {
    joints: Vec<Vec3>,
    turns: Vec<Quat>,
    clearance: f32,
    handed: Option<Posed>,
    patch: bool,
    carried: f32,
    rising: bool,
}

impl Lifting {
    /// Whether its flight was handed on.
    fn aloft(&self) -> bool {
        self.handed.is_some()
    }
}

/// How a take-off is begun and carried ([`taking_off`]).
#[derive(Clone, Copy, Debug)]
struct Launched {
    /// Whose chassis carries it.
    under: Under,
    /// For the owner's chassis, whether it carries its own collider and is
    /// read by its contacts, as the game's own is, rather than cast down as
    /// a peer is.
    contacts: bool,
    /// Standing a second, or gliding down onto the ground and taking off this
    /// many seconds into its held landing.
    held: Option<f32>,
}

/// One frame of the owner's chassis ([`flown_by_the_owner`]): where it is,
/// how it is turned, its velocity and its flight.
type Owned = (Vec3, Quat, Vec3, Flight);

/// The owner's chassis, frame by frame: what [`Sky`] leaves it at, at `hz`,
/// on `ground`, with its flight. Standing a second and taking off with the
/// flap key held for a second; or, `held` - seconds into the hold, and where
/// along `z` it starts - gliding down from 1.5 m over the ground and taking
/// off that long after its chassis is on its feet, the flap key held for a
/// second. And where a glide's chassis went to its feet.
fn flown_by_the_owner(
    record: &AvatarRecord,
    hz: f32,
    ground: fn(f32, f32) -> f32,
    held: Option<(f32, f32)>,
) -> (Vec<Owned>, Option<Vec3>) {
    let frame = 1.0 / hz;
    let mut sky = Sky::of(record);
    sky.ground = ground;
    let mut owner = Vec::new();
    let Some((wait, z)) = held else {
        sky.at = Vec3::new(0.0, ground(0.0, 0.0) + sky.feet, 0.0);
        for at in 0..(2.0 * hz).round() as usize {
            let t = at as f32 * frame;
            if (t - 1.0).abs() < frame * 0.5 {
                sky.keys.press = true;
            }
            sky.keys.held = (1.0..2.0).contains(&t);
            sky.advance(frame);
            owner.push((sky.at, sky.rotation(), sky.velocity, sky.flight));
        }
        return (owner, None);
    };
    sky.at = Vec3::new(0.0, ground(0.0, 0.0) + sky.feet + 1.5, z);
    sky.velocity = Vec3::new(0.0, -1.0, -4.0);
    let mut gliding = Flight::default();
    gliding.set_mode_for_tests(FlightMode::Glide);
    sky.flight = gliding;
    let mut landed: Option<(usize, Vec3)> = None;
    let mut pressed: Option<usize> = None;
    for at in 0..(4.0 * hz).round() as usize {
        sky.keys.held = pressed.is_some_and(|from| (at - from) as f32 * frame < 1.0);
        sky.advance(frame);
        owner.push((sky.at, sky.rotation(), sky.velocity, sky.flight));
        if sky.flight.mode() == FlightMode::Ground && pressed.is_none() {
            let (since, _) = *landed.get_or_insert((at, sky.at));
            // Pressed after this frame, as a key read once it is drawn.
            if (at + 1 - since) as f32 * frame >= wait - 1e-4 {
                sky.keys.press = true;
                pressed = Some(at);
            }
        }
    }
    (owner, landed.map(|(_, at)| at))
}

/// What a take-off stands on ([`taking_off`]): the terrain's height, the
/// ground's - the terrain's, or the top of what stands on it - and the static
/// solids standing on the terrain.
type Grounds<'a> = (
    fn(f32, f32) -> f32,
    fn(f32, f32) -> f32,
    &'a [(Vec3, Transform)],
);

/// `family` taking off at `hz` on `ground` - the terrain's own, `terrain`,
/// with `solids` standing on it - as `launched` says ([`flown_by_the_owner`]),
/// every frame read: under its owner's chassis, written as the physics leaves
/// it, or played out as a peer, its words heard as its owner's mode changed
/// and its place [`crate::player::flight::PEER_DELAY_S`] late. A glide is
/// started where along `z` it comes down over the middle of the ground.
fn taking_off(
    family: WingedFamily,
    hz: f32,
    (terrain, ground, solids): Grounds<'_>,
    launched: Launched,
) -> Vec<Lifting> {
    let Launched {
        under,
        contacts,
        held,
    } = launched;
    let body = default_winged(family);
    let record = fitted_record(body.clone());
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let frame = 1.0 / hz;
    // The owner's chassis, frame by frame - a glide started back along its
    // path by however far from the middle the last try came down.
    let (owner, _) = match held {
        None => flown_by_the_owner(&record, hz, ground, None),
        Some(wait) => {
            let mut z = 0.0;
            let mut flown = flown_by_the_owner(&record, hz, ground, Some((wait, z)));
            for _ in 0..3 {
                let landed = flown.1.expect("the glide comes down").z;
                if landed.abs() < 0.25 {
                    break;
                }
                z -= landed;
                flown = flown_by_the_owner(&record, hz, ground, Some((wait, z)));
            }
            let landed = flown.1.expect("the glide comes down");
            assert!(
                landed.z.abs() < 0.5,
                "the premise: the glide comes down over the middle, at {landed}"
            );
            flown
        }
    };
    let frames = owner.len();
    let mut app = grounded(&terrain, solids);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        frame,
    )));
    let chassis = if contacts {
        // The game's own chassis, a dynamic body with the record's collider.
        app.insert_resource(LiveAvatarRecord(record.clone()));
        let chassis = app
            .world_mut()
            .spawn((
                Transform::from_translation(owner[0].0),
                avian3d::prelude::RigidBody::Dynamic,
                avian3d::prelude::CollidingEntities::default(),
                LocalPlayer,
                LinearVelocity::ZERO,
            ))
            .id();
        let mut commands = app.world_mut().commands();
        crate::player::preset::build_preset_components(&mut commands, chassis, &record.locomotion);
        app.world_mut().flush();
        chassis
    } else {
        chassis(&mut app, under, record.clone(), owner[0].0)
    };
    if under == Under::Peer {
        // A peer's chassis stands a frame before its body lands, as the
        // game's always does - its build lands frames after it spawns - so
        // its first speed is differenced from where it stands (#1889). Landed
        // in the frame it spawned, its place was differenced against the
        // origin its transform had not yet been carried from: the root's
        // height in one frame, a leap of 13.7 m/s at 144 Hz, and the body was
        // still in it a second later, a hind leg turning 2.7 rad a frame.
        app.update();
    }
    if under == Under::Local {
        app.world_mut()
            .entity_mut(chassis)
            .insert(Flight::default());
    }
    land(&mut app, chassis, &body, 64, offset);
    let root = root_of(&mut app, chassis);
    // One idle for every run: a body's room seed is the process' count, and
    // unseeded, a head glancing on one run and not the one it is measured
    // against turned 0.9 rad further in a frame.
    app.world_mut()
        .entity_mut(root)
        .insert(AvatarDriver::seeded(INSTRUMENT_SEED));
    let rig = rig_of(&app, root);
    let feet: Vec<usize> = rig
        .ground_contacts()
        .iter()
        .filter_map(|&limb| rig.in_zone(Zone::Extremity(limb)).first().copied())
        .collect();
    let delay = (crate::player::flight::PEER_DELAY_S as f32 / frame).round() as usize;
    let mut heard = FlightMode::Ground;
    let mut read = Vec::new();
    for at in 0..frames {
        match under {
            Under::Local => {
                let (place, turn, velocity, flight) = owner[at];
                let mut entity = app.world_mut().entity_mut(chassis);
                let mut transform = entity.get_mut::<Transform>().expect("a place");
                transform.translation = place;
                transform.rotation = turn;
                entity.get_mut::<LinearVelocity>().expect("a velocity").0 = velocity;
                *entity.get_mut::<Flight>().expect("a flight") = flight;
            }
            Under::Peer => {
                let now = app.world().resource::<Time>().elapsed_secs_f64();
                let said = owner[at].3.mode();
                if said != heard {
                    heard = said;
                    let mut entity = app.world_mut().entity_mut(chassis);
                    match entity.get_mut::<PeerFlight>() {
                        Some(mut flight) => flight.hear(said, now),
                        None => {
                            entity.insert(PeerFlight::heard_at(said, now));
                        }
                    }
                }
                let (place, turn, ..) = owner[at.saturating_sub(delay)];
                let mut transform = app
                    .world_mut()
                    .get_mut::<Transform>(chassis)
                    .expect("a place");
                transform.translation = place;
                transform.rotation = turn;
            }
        }
        app.update();
        let world = app.world();
        let posed = world
            .get::<bevy_symbios_avatar::AvatarPose>(root)
            .expect("posed")
            .0
            .forward(&rig);
        let drawn = *world.get::<GlobalTransform>(root).expect("drawn");
        let clearance = feet
            .iter()
            .map(|&foot| {
                let at = drawn.transform_point(posed.positions[foot]);
                at.y - ground(at.x, at.z)
            })
            .fold(f32::INFINITY, f32::min);
        let drive = world.get::<Drive>(root).expect("a drive");
        let trail = world.get::<RiggedTrail>(root).expect("a trail");
        let place = match under {
            Under::Local => at,
            Under::Peer => at.saturating_sub(delay),
        };
        read.push(Lifting {
            joints: posed.positions,
            turns: posed.rotations,
            clearance,
            handed: drive.flight.map(|flight| flight.mode),
            patch: drive.ground.is_some(),
            carried: trail.rest.held().0 + trail.rest.stands_by().1,
            rising: owner[place].2.y > 0.0 && place > 0 && owner[place].0.y > owner[place - 1].0.y,
        });
    }
    read
}

/// How long the engine takes to let a winged body's feet go after its push,
/// which [`TAKEOFF_SECS`] spans, seconds (symbios-avatar's `PLANT`).
const RELEASE_SECS: f32 = 0.08;

/// How long a takeoff asked of a held landing crouches before it pushes, at
/// least, seconds (symbios-avatar's `MIN_CROUCH`, its `crouch_first`).
const CROUCH_SECS: f32 = 0.18;

/// The most a joint of a winged body taking off from a slope may move on one
/// frame further than the same body taking off from level ground, as a
/// share of its trunk (#1887): this much on any frame, and this much more a
/// second of the frame's length - 0.21 of the trunk at 60 frames a second,
/// 0.27 at 30 and 0.175 at 144.
const MOVED_A_FRAME: f32 = 0.15;
const MOVED_A_SECOND: f32 = 3.6;

/// How much more than [`MOVED_A_FRAME`] and [`MOVED_A_SECOND`] allow a bat's
/// joint may move on a frame of the ground's ease-out - [`TAKEOFF_SECS`] from
/// the flight's first frame - as a share of its trunk: its hands peeling off
/// the slope (#1903).
///
/// Since symbios-avatar 0.24.2 (its #441, `keep_clear`) a bat's crawling
/// hands lie along the ground they stand on, so they turn back upright as
/// this crate eases that ground out from the take-off's first frame
/// ([`TAKEOFF_SECS`]) - over the push itself, and from a held landing over
/// the crouch before the push, the ground easing out as the body crouches.
/// Under its owner's chassis at 30 frames a second, a bat taking off across
/// a 20 degree slope then moved a joint 80.5 mm - 0.30 of its trunk -
/// further than from level ground on the flight's second frame, and 73.4 mm
/// on it three tenths of a second into a held landing, where the bound is
/// 72.6 mm (0.27) and its hands standing upright had moved 53 at most; at 60
/// and 144 frames a second it stays inside the bound. The owner took it as
/// the hands' own motion rather than the ground's (2026-11-10) over a release
/// of each hand's tilt kept and eased out by the engine itself.
const HANDS_PEEL: f32 = 0.06;

/// The most a joint of a winged body taking off from a slope may turn on one
/// frame further than the same body taking off from level ground, radians
/// (#1887).
const TURNED_A_FRAME: f32 = 0.5;

/// **A winged body taking off from a slope or a pitched roof eases the
/// ground out** (#1887): a bat, a glider and a bird (on two legs, handed no
/// patch at all) standing across a 20 degree hillside, and across a roof
/// pitched 20 degrees over level terrain, take off at 30, 60 and 144 frames
/// a second, under their owner's chassis and as a peer, beside the same
/// body taking off from level ground. The bat and the glider also take off
/// under their owner's chassis carrying its own collider, read by its
/// contacts as the game's chassis is rather than cast down as a peer is;
/// and, every way, from a held landing (#1887 end review): glided down onto
/// the same ground and taking off a tenth and three tenths of a second after
/// the chassis is on its feet, its landing still held, so that its flight
/// goes from the landing to none for a frame and then to a takeoff. On each
/// frame from the one before its takeoff is handed on - a held landing's
/// frame of no flight - to a few after the engine has let its feet go
/// (after its crouch, [`CROUCH_SECS`], from a held landing), how much
/// further each joint moved than the level take-off's moved on the same
/// frame is under [`MOVED_A_FRAME`] of the trunk and [`MOVED_A_SECOND`] of
/// it a second of the frame's length - and a bat's, on the frames the ground
/// eases out over, [`HANDS_PEEL`] more, its hands peeling off the slope - and how much
/// further it turned under [`TURNED_A_FRAME`]: a bound of its own on every
/// frame, which no neighbour's raises. Once the ease has run, a rising body carries none of
/// the ground it rose off and is handed no patch, and no foot is held to
/// that ground: each stands over it at least half as high as the level
/// take-off's lowest stands over its own. Every run's idle is seeded alike:
/// unseeded, a head glancing on one run and not on the one it is measured
/// against turned 0.9 rad further in a frame.
///
/// Eased over the push from the take-off's first frame, the most any joint
/// moved further than on level ground on one frame was 53 mm at 30 Hz, 30
/// at 60 and 13 at 144, and turned 0.21 radians at 30, 0.09 at 60 and 0.07
/// at 144, while a bat's hands stood upright on the slope. Since
/// symbios-avatar 0.24.2 they lie along it and peel off it in the push: a
/// bat under its owner's chassis at 30 Hz moved one 80.5 mm further on the
/// flight's second frame ([`HANDS_PEEL`]). A peer's body landed in the frame its chassis spawned once made
/// the 144 Hz figures 22 mm and 0.26 rad (#1889, #1890: see
/// [`a_peer_standing_on_a_pitched_roof_stands_as_on_the_level`]). Handed the level floor at
/// once, under its owner's chassis, a bat moved one 93 mm further at 144 Hz
/// and 92 at 60, and a glider 147 and 138 (151 at 60 as a peer), turning
/// one up to 0.87 radians; and as a peer at 30 Hz, its hand-over a frame
/// after its flight's first, a bat 93 mm and a glider 98. At 30 Hz the
/// owner's rows pass with or without the ease (44 mm at most without it),
/// since the engine already spreads the old jump over its push. Eased over
/// 0.18 s from the frame it rose past the engine's launch speed, a bat
/// taking off as a peer at 30 Hz swung its wing-hands out sideways and
/// back, 103 mm further than on level ground on one frame.
///
/// From a held landing, the ground held as it was on the frame of no flight
/// and eased out from the takeoff after it, the most was 52 mm at 30 Hz, 31
/// at 60 and 13 at 144, and 0.25 radians. Read on that frame as any other,
/// the ground handed over at once and the ease left nothing to carry, a bat
/// three tenths of a second into its landing moved a joint 89 mm further
/// than on level ground at 30 Hz under its owner's chassis cast down, and
/// read by its contacts 94 at 30 and 103 at 60 - and 64 at 60 a tenth of a
/// second in; the peers' rows passed.
#[test]
fn a_winged_take_off_from_a_slope_eases_the_ground_out() {
    let ([level, slope, roof], solids) = level_slope_and_roof();
    for family in [WingedFamily::Bat, WingedFamily::Glider, WingedFamily::Bird] {
        let size = trunk(&default_rig(family));
        let patched = family != WingedFamily::Bird;
        for hz in [30.0f32, 60.0, 144.0] {
            for (under, contacts) in [
                (Under::Local, false),
                (Under::Local, true),
                (Under::Peer, false),
            ] {
                for held in [None, Some(0.1f32), Some(0.3)] {
                    // A bird is handed no patch: one way of taking off is
                    // enough to say so.
                    if !patched && (contacts || held.is_some()) {
                        continue;
                    }
                    let launched = Launched {
                        under,
                        contacts,
                        held,
                    };
                    took_off_as_on_the_level(
                        family,
                        hz,
                        launched,
                        size,
                        [level, slope, roof],
                        &solids,
                    );
                }
            }
        }
    }
}

/// [`a_winged_take_off_from_a_slope_eases_the_ground_out`] for `family` at
/// `hz`, `launched` so: on the slope and the roof against the level, of the
/// ground functions `[level, slope, roof]`, with the roof's `solids`, its
/// joints bounded by its trunk, `size`.
fn took_off_as_on_the_level(
    family: WingedFamily,
    hz: f32,
    launched: Launched,
    size: f32,
    [level, slope, roof]: [fn(f32, f32) -> f32; 3],
    solids: &[(Vec3, Transform)],
) {
    let flat = taking_off(family, hz, (level, level, &[]), launched);
    for (name, run) in [
        (
            "slope",
            taking_off(family, hz, (slope, slope, &[]), launched),
        ),
        (
            "roof",
            taking_off(family, hz, (level, roof, solids), launched),
        ),
    ] {
        let Launched {
            under,
            contacts,
            held,
        } = launched;
        let label = format!(
            "{family:?} at {hz} Hz on the {name}, {under:?}{}{}",
            if contacts { " by its contacts" } else { "" },
            held.map_or(String::new(), |wait| format!(
                ", {wait} s into a held landing"
            )),
        );
        // The take-off's first frame handed a takeoff: from a held landing,
        // after the landing's own and a frame of no flight.
        let lift = |frames: &[Lifting]| {
            let from = frames
                .iter()
                .position(|frame| frame.handed == Some(Posed::Takeoff))
                .unwrap_or_else(|| panic!("{label}: never took off"));
            if held.is_some() {
                assert!(
                    from >= 2
                        && frames[from - 1].handed.is_none()
                        && frames[from - 2].handed == Some(Posed::Land),
                    "{label}: the premise, a takeoff from a held landing: handed {:?}",
                    frames[from.saturating_sub(4)..from]
                        .iter()
                        .map(|frame| frame.handed)
                        .collect::<Vec<_>>()
                );
            }
            from
        };
        let (from, level_from) = (lift(&run), lift(&flat));
        let span = (hz / 3.0).ceil() as usize;
        // The frames `k` from the flight's first, here and on the level.
        let at = |k: i64| ((from as i64 + k) as usize, (level_from as i64 + k) as usize);
        // How much further the worst joint moved, or turned, on frame `k`
        // than on the level.
        let beyond = |k: i64, turns: bool| -> f32 {
            let ((r, l), (r0, l0)) = (at(k), at(k - 1));
            (0..run[r].joints.len())
                .map(|j| {
                    if turns {
                        let now = flat[l].turns[j].inverse() * run[r].turns[j];
                        let was = flat[l0].turns[j].inverse() * run[r0].turns[j];
                        now.angle_between(was)
                    } else {
                        ((run[r].joints[j] - flat[l].joints[j])
                            - (run[r0].joints[j] - flat[l0].joints[j]))
                            .length()
                    }
                })
                .fold(0.0, f32::max)
        };
        // From the frame before the flight's first - a held landing's frame of
        // no flight - to a few after the engine has let the feet go: the ease,
        // and the push's release after it, which from a held landing waits
        // for its crouch.
        let crouch = if held.is_some() { CROUCH_SECS } else { 0.0 };
        let release = ((crouch + TAKEOFF_SECS + RELEASE_SECS) * hz).ceil() as i64 + 4;
        // The frames the ground eases out over, from the flight's first,
        // where a bat's hands peel off the slope ([`HANDS_PEEL`]): the push,
        // or from a held landing the crouch before it.
        let easing = 0..=(TAKEOFF_SECS * hz).ceil() as i64;
        let peel = |k: i64| {
            if family == WingedFamily::Bat && easing.contains(&k) {
                HANDS_PEEL * size
            } else {
                0.0
            }
        };
        for (what, turns, most) in [
            ("moved", false, (MOVED_A_FRAME + MOVED_A_SECOND / hz) * size),
            ("turned", true, TURNED_A_FRAME),
        ] {
            // Each frame against its own bound: the worst is the one most past
            // it, and it is reported with that bound.
            let bound = |k: i64| if turns { most } else { most + peel(k) };
            let (worst_at, worst) =
                (-1..=release)
                    .map(|k| (k, beyond(k, turns)))
                    .fold((0, 0.0f32), |worse, step| {
                        if step.1 - bound(step.0) > worse.1 - bound(worse.0) {
                            step
                        } else {
                            worse
                        }
                    });
            let allowed = bound(worst_at);
            assert!(
                worst <= allowed,
                "{label}: frame {worst_at} of the flight {what} {worst:.4} further than on the \
                 level, past {allowed:.4}"
            );
        }
        let eased = from + (TAKEOFF_SECS * hz).ceil() as usize + 1;
        let mut checked = 0;
        for frame in eased..from + span {
            let (here, there) = (&run[frame], &flat[level_from + (frame - from)]);
            if !(here.aloft() && here.rising) {
                continue;
            }
            checked += 1;
            assert!(
                !here.patch && here.carried == 0.0,
                "{label}: frame {} of the flight still carries {:.3} of the ground (patch {})",
                frame - from,
                here.carried,
                here.patch
            );
            assert!(
                here.clearance >= 0.5 * there.clearance,
                "{label}: frame {} of the flight has a foot {:.3} m over the ground, against \
                 {:.3} on the level",
                frame - from,
                here.clearance,
                there.clearance
            );
        }
        assert!(checked > 0, "{label}: the premise, rising after the ease");
    }
}

/// A ground's height at each world `(x, z)`.
type Ground = fn(f32, f32) -> f32;

/// The level ground, a hillside rising 20 degrees with `x`, and a roof pitched
/// 20 degrees over level terrain - its top 0.6 m up over the body's middle -
/// with the roof's solid ([`a_winged_take_off_from_a_slope_eases_the_ground_out`]'s).
fn level_slope_and_roof() -> ([Ground; 3], [(Vec3, Transform); 1]) {
    let level: fn(f32, f32) -> f32 = |_, _| 0.0;
    let slope: fn(f32, f32) -> f32 = |x, _| x * 20f32.to_radians().tan();
    let roof: fn(f32, f32) -> f32 = |x, z| {
        if x.abs() < 3.5 && z.abs() < 3.5 {
            0.6 + x * 20f32.to_radians().tan()
        } else {
            0.0
        }
    };
    let pitch = 20f32.to_radians();
    let solids = [(
        Vec3::new(4.0, 0.3, 4.0),
        Transform::from_translation(Vec3::Y * (0.6 - 0.3 / pitch.cos()))
            .with_rotation(Quat::from_rotation_z(pitch)),
    )];
    ([level, slope, roof], solids)
}

/// The first frame of `frames` handed a takeoff.
fn first_takeoff(frames: &[Lifting]) -> usize {
    frames
        .iter()
        .position(|frame| frame.handed == Some(Posed::Takeoff))
        .expect("took off")
}

/// The most joint `j` moved and turned between frames `r - 1` and `r` of
/// `run`, further than it did between frames `l - 1` and `l` of `flat`.
fn beyond_on(run: &[Lifting], r: usize, flat: &[Lifting], l: usize, j: usize) -> (f32, f32) {
    let moved = ((run[r].joints[j] - flat[l].joints[j])
        - (run[r - 1].joints[j] - flat[l - 1].joints[j]))
        .length();
    let now = flat[l].turns[j].inverse() * run[r].turns[j];
    let was = flat[l - 1].turns[j].inverse() * run[r - 1].turns[j];
    (moved, now.angle_between(was))
}

/// How far joint `j` moved and turned between frames `r - 1` and `r` of
/// `run`.
fn step_of(run: &[Lifting], r: usize, j: usize) -> (f32, f32) {
    (
        (run[r].joints[j] - run[r - 1].joints[j]).length(),
        run[r].turns[j].angle_between(run[r - 1].turns[j]),
    )
}

/// The most a joint of a winged body standing still may move on one frame,
/// as a share of its trunk, and turn, radians - on its own and further than
/// the same body standing on level ground (#1889). A standing bat, glider
/// or bird moved one at most 1.0 mm and turned one 0.018 rad a frame.
const STOOD_MOVED: f32 = 0.01;
const STOOD_TURNED: f32 = 0.05;

/// **A peer standing on a pitched roof stands as on level ground** (#1889):
/// a bat, a glider and a bird on a peer's client, standing a second on a roof
/// pitched 20 degrees and on level ground before it takes off, at 30, 60 and
/// 144 frames a second, every idle seeded alike: on every frame before its
/// takeoff is handed on, no joint moves more than [`STOOD_MOVED`] of its trunk
/// or turns more than [`STOOD_TURNED`] - on either ground, and further on the
/// roof than on the level on the same frame.
///
/// The take-off's harness once landed a peer's body in the frame its chassis
/// spawned, before the chassis' transform had been carried to its place: its
/// first speed was differenced from the origin, the root's height in one
/// frame, a leap of 13.7 m/s at 144 Hz, and the body was still in it a second
/// later, the level run's hind leg turning 2.7 rad and its foot moving 199 mm
/// in a frame seven frames before its flight (a glider's 3.1 rad and 197 mm).
/// Compared against it, the roof's peer - whose structure set it down - was
/// the figure filed. Stood a frame first, as the game's chassis always is, a
/// peer stands as its owner does: 1.0 mm and 0.018 rad a frame at most.
#[test]
fn a_peer_standing_on_a_pitched_roof_stands_as_on_the_level() {
    let ([level, _, roof], solids) = level_slope_and_roof();
    let launched = Launched {
        under: Under::Peer,
        contacts: false,
        held: None,
    };
    for family in [WingedFamily::Bat, WingedFamily::Glider, WingedFamily::Bird] {
        let size = trunk(&default_rig(family));
        for hz in [30.0f32, 60.0, 144.0] {
            let flat = taking_off(family, hz, (level, level, &[]), launched);
            let run = taking_off(family, hz, (level, roof, &solids), launched);
            let (from, level_from) = (first_takeoff(&run), first_takeoff(&flat));
            let joints = run[0].joints.len();
            let mut checked = 0;
            for (name, frames, until) in [("level", &flat, level_from), ("roof", &run, from)] {
                for r in 1..until {
                    for j in 0..joints {
                        let (moved, turned) = step_of(frames, r, j);
                        assert!(
                            moved <= STOOD_MOVED * size && turned <= STOOD_TURNED,
                            "{family:?} at {hz} Hz standing on the {name}: frame {r} moved joint \
                             {j} {:.1} mm and turned it {turned:.3} rad",
                            moved * 1000.0
                        );
                    }
                }
            }
            // Frames `k` before the takeoff, here and on the level.
            for k in -(from.min(level_from) as i64) + 1..0 {
                let (r, l) = ((from as i64 + k) as usize, (level_from as i64 + k) as usize);
                checked += 1;
                for j in 0..joints {
                    let (moved, turned) = beyond_on(&run, r, &flat, l, j);
                    assert!(
                        moved <= STOOD_MOVED * size && turned <= STOOD_TURNED,
                        "{family:?} at {hz} Hz on the roof: {} frames before its takeoff joint \
                         {j} moved {:.1} mm and turned {turned:.3} rad further than on the level",
                        -k,
                        moved * 1000.0
                    );
                }
            }
            assert!(
                checked as f32 >= 0.9 * hz,
                "{family:?} at {hz} Hz: the premise, standing a second ({checked} frames)"
            );
        }
    }
}

/// The most a winged body's hind foot may move further, on one frame of its
/// take-off from a slope or a pitched roof, than the same body's taking off
/// from level ground, as a share of its trunk a second of the frame's length,
/// and turn, radians a second (#1890). A glider's moved 14 mm and turned
/// 0.10 rad further at 30 Hz, 7.9 and 0.052 at 60 and 4.2 and 0.022 at 144:
/// about two trunks and three radians a second.
const HIND_BEYOND_A_SECOND: f32 = 4.0;
const HIND_TURNED_BEYOND_A_SECOND: f32 = 6.0;

/// The fastest a winged body's hind foot may move through its take-off, as a
/// share of its trunk a second, and turn, radians a second (#1890). A
/// glider's tucks at about 12 trunks (3.1 m/s) and 20 rad a second, at every
/// frame rate.
const HIND_SPEED: f32 = 16.0;
const HIND_SPIN: f32 = 30.0;

/// **A peer glider's hind feet go up with its take-off as its owner's do**
/// (#1890): a glider taking off from level ground, a 20 degree hillside and
/// a roof pitched 20 degrees, at 30, 60 and 144 frames a second, on a peer's
/// client and under its owner's chassis, every idle seeded alike. On every
/// frame from the one before its takeoff is handed on to a few after the
/// engine has let its feet go, no joint of a hind foot moves faster than
/// [`HIND_SPEED`] trunks a second or turns faster than [`HIND_SPIN`] radians a
/// second, and none moves further than on the level by more than
/// [`HIND_BEYOND_A_SECOND`] trunks a second of the frame's length or turns
/// further by more than [`HIND_TURNED_BEYOND_A_SECOND`] radians a second.
///
/// Filed at 35 mm and 0.40 rad a frame beyond the level at 144 Hz as a peer,
/// with an unseeded idle and the take-off's harness landing a peer's body in
/// the frame its chassis spawned (see
/// [`a_peer_standing_on_a_pitched_roof_stands_as_on_the_level`]): that peer
/// was still in a leap it had been handed on its first frame as its take-off
/// began, and seeded it was 10.5 mm and 0.175 rad. A peer's hind foot now
/// moves as its owner's to a tenth of a millimetre: 22 mm and 0.14 rad a frame
/// at 144 Hz on the level, its push's own tuck, 4.2 mm and 0.022 rad further
/// on the roof.
#[test]
fn a_peer_gliders_hind_feet_take_off_as_its_owners() {
    use symbios_avatar::plan::Limb;
    let ([level, slope, roof], solids) = level_slope_and_roof();
    let family = WingedFamily::Glider;
    let rig = default_rig(family);
    let size = trunk(&rig);
    let hind: Vec<usize> = (0..rig.joints.len())
        .filter(|&j| {
            matches!(
                rig.joints[j].zone,
                Zone::Extremity(Limb::HindLeft | Limb::HindRight)
            )
        })
        .collect();
    assert!(!hind.is_empty(), "the premise: hind feet");
    for hz in [30.0f32, 60.0, 144.0] {
        let release = ((TAKEOFF_SECS + RELEASE_SECS) * hz).ceil() as i64 + 4;
        for under in [Under::Peer, Under::Local] {
            let launched = Launched {
                under,
                contacts: false,
                held: None,
            };
            let flat = taking_off(family, hz, (level, level, &[]), launched);
            let level_from = first_takeoff(&flat);
            for (name, run) in [
                ("level", None),
                (
                    "slope",
                    Some(taking_off(family, hz, (slope, slope, &[]), launched)),
                ),
                (
                    "roof",
                    Some(taking_off(family, hz, (level, roof, &solids), launched)),
                ),
            ] {
                let run = run.as_deref().unwrap_or(&flat[..]);
                let from = first_takeoff(run);
                let label = format!("{family:?} at {hz} Hz on the {name}, {under:?}");
                for k in -1..=release {
                    let (r, l) = ((from as i64 + k) as usize, (level_from as i64 + k) as usize);
                    for &j in &hind {
                        let (moved, turned) = step_of(run, r, j);
                        assert!(
                            moved * hz <= HIND_SPEED * size && turned * hz <= HIND_SPIN,
                            "{label}: frame {k} of the flight moved hind joint {j} {:.1} mm and \
                             turned it {turned:.3} rad",
                            moved * 1000.0
                        );
                        let (moved, turned) = beyond_on(run, r, &flat, l, j);
                        assert!(
                            moved * hz <= HIND_BEYOND_A_SECOND * size
                                && turned * hz <= HIND_TURNED_BEYOND_A_SECOND,
                            "{label}: frame {k} of the flight moved hind joint {j} {:.1} mm and \
                             turned it {turned:.3} rad further than on the level",
                            moved * 1000.0
                        );
                    }
                }
            }
        }
    }
}

/// A person or a beast wearing a flight section, its chassis flown through
/// a take-off, flapping, a glide and a landing, with the chassis' [`Flight`]
/// on it or not: every joint drawn, every frame, and what its drive was
/// handed in flight and turn.
fn flown_without_wings(
    record: &AvatarRecord,
    body: &EngineAvatarRecord,
    with: bool,
) -> Vec<(Vec<Vec3>, f32, bool)> {
    let offset = crate::player::visuals::rigged_root_offset(record);
    let mut app = app();
    let chassis = chassis(&mut app, Under::Local, record.clone(), Vec3::Y * offset);
    app.world_mut()
        .entity_mut(chassis)
        .insert(Flight::default());
    land(&mut app, chassis, body, 64, offset);
    let root = root_of(&mut app, chassis);
    // One idle for both runs: a body's room seed is the process' count.
    app.world_mut()
        .entity_mut(root)
        .insert(AvatarDriver::seeded(INSTRUMENT_SEED));
    let mut sky = Sky::of(record);
    let mut out = Vec::new();
    for at in 0..360 {
        let t = at as f32 / 60.0;
        sky.keys.press = at == 30;
        sky.keys.held = (0.5..1.5).contains(&t);
        sky.keys.bank = if (1.0..1.8).contains(&t) { 1 } else { 0 };
        sky.advance(1.0 / 60.0);
        sky.write(&mut app, chassis);
        if !with {
            app.world_mut().entity_mut(chassis).remove::<Flight>();
        }
        app.update();
        if !with {
            app.world_mut()
                .entity_mut(chassis)
                .insert(Flight::default());
        }
        let drive = app.world().get::<Drive>(root).expect("a drive");
        out.push((drawn(&app, root), drive.turn, drive.flight.is_some()));
    }
    out
}

/// **A person or a beast flying the prototype is drawn as it always was**
/// (#1877): with its chassis' flight on it or not, every joint of every
/// frame the same to the bit, its drive handed no flight and no turn. Only a
/// rig with wings is fed: the engine ignores a flight on any other, but a
/// turn would step a person round.
#[test]
fn a_person_or_a_beast_flying_the_prototype_is_drawn_as_before() {
    let did = "did:plc:flightlesspersonaaaaaaaaa";
    let mut cases = vec![
        (person_record(did), engine_default_for_did(did)),
        (beast_record(Family::Dog), default_beast(Family::Dog)),
    ];
    for (record, body) in &mut cases {
        if let crate::pds::LocomotionConfig::Humanoid(p) = &mut record.locomotion {
            p.flight = Some(FlightParams::default());
        }
        let with = flown_without_wings(record, body, true);
        let without = flown_without_wings(record, body, false);
        assert!(with.iter().all(|(_, turn, flight)| *turn == 0.0 && !flight));
        for (at, (a, b)) in with.iter().zip(&without).enumerate() {
            assert!(
                a.0 == b.0,
                "frame {at}: drawn {} mm apart",
                moved(&a.0, &b.0) * 1000.0
            );
        }
    }
}

/// **Probe, not a gate** (#1877): `family` flown through a life - or, with
/// `lake`, gliding from a take-off into a lake - every frame's pose and its
/// chassis' attitude written to `OVERLANDS_POSES` (default under the system's
/// temp directory) for the engine's renderer, six frames a row: the
/// take-off, flapping, the glide, the flare, the landing and its hold, and
/// the hand-back; and a line a frame of what was handed and what carried it.
fn dump(family: WingedFamily, lake: bool) {
    use std::fmt::Write as _;
    let body = default_winged(family);
    let record = fitted_record(body.clone());
    let (mut app, chassis, root, mut sky) = owned(&record, &body, 60.0);
    if lake {
        app.insert_resource(pond());
        let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
            panic!("on foot");
        };
        sky.lake = Some((pond(), WaterColumn::of(p)));
        sky.at = Vec3::new(0.0, 2.0, 0.0);
        sky.velocity = Vec3::new(0.0, -1.2, -5.0);
        let mut gliding = Flight::default();
        gliding.set_mode_for_tests(FlightMode::Glide);
        sky.flight = gliding;
    }
    let rig = rig_of(&app, root);
    let half = Quat::from_rotation_y(std::f32::consts::PI);
    let mut frames: Vec<(String, String)> = Vec::new();
    let mut stood: Option<usize> = None;
    let mut was: Option<Vec<Vec3>> = None;
    for at in 0..1200 {
        let t = at as f32 / 60.0;
        if !lake {
            sky.keys.press = at == 30;
            sky.keys.held = (0.5..2.0).contains(&t);
        }
        sky.advance(1.0 / 60.0);
        sky.write(&mut app, chassis);
        app.update();
        let world = app.world();
        let drive = world.get::<Drive>(root).expect("a drive");
        let handed = drive.flight.map(|flight| flight.mode);
        let source = world
            .get::<bevy_symbios_avatar::Drove>(root)
            .expect("driven")
            .source;
        let flown = world
            .get::<AvatarDriver>(root)
            .expect("a driver")
            .flight(&rig);
        let pose = &world
            .get::<bevy_symbios_avatar::AvatarPose>(root)
            .expect("posed")
            .0;
        let chassis_at = world.get::<Transform>(chassis).expect("a place");
        // The body in its own frame, its chassis' attitude laid on it.
        let turn = half.inverse() * chassis_at.rotation * half;
        let offset = half.inverse()
            * (Vec3::Y * chassis_at.translation.y - chassis_at.rotation * Vec3::Y * sky.feet);
        let root_joint = rig
            .joints
            .iter()
            .position(|j| j.parent.is_none())
            .expect("a root");
        let rest = rig.joints[root_joint].position;
        let mut rotations = pose.rotations.clone();
        rotations[root_joint] = turn * rotations[root_joint];
        let translation = turn * (rest + pose.translation) + offset - rest;
        let mut line = format!("{} {} {}", translation.x, translation.y, translation.z);
        for q in &rotations {
            let _ = write!(line, " {} {} {} {}", q.x, q.y, q.z, q.w);
        }
        let joints = pose.forward(&rig).positions;
        let step = was.as_ref().map_or(0.0, |was| moved(was, &joints));
        was = Some(joints);
        let label = format!(
            "{at:4} {:?} handed {:?} flown {:?} settled {} source {:?} swimming {} y {:.3} step {:.1} mm",
            sky.flight.mode(),
            handed,
            flown.map(|f| f.mode),
            flown.is_some_and(|f| f.settled),
            source,
            drive.swimming,
            chassis_at.translation.y - sky.feet,
            step * 1000.0,
        );
        println!("{label}");
        let tag = match (handed, sky.flight.mode()) {
            _ if drive.swimming => "swim".to_string(),
            (Some(mode), _) => format!("{mode:?}"),
            (None, _) => "ground".to_string(),
        };
        frames.push((tag, line));
        if at > 40 && handed.is_none() && sky.flight.mode() == FlightMode::Ground && stood.is_none()
        {
            stood = Some(at);
        }
        if stood.is_some_and(|since| at > since + 30) {
            break;
        }
    }
    let rows: Vec<Vec<usize>> = if lake {
        let first = frames
            .iter()
            .position(|(tag, _)| tag != "Glide")
            .unwrap_or(0);
        vec![
            (0..12)
                .map(|i| (first + i * 3).saturating_sub(9).min(frames.len() - 1))
                .collect(),
        ]
    } else {
        let mut rows = Vec::new();
        for tag in ["Takeoff", "Flap", "Glide", "Flare", "Land"] {
            let of: Vec<usize> = (0..frames.len()).filter(|&i| frames[i].0 == tag).collect();
            if of.is_empty() {
                continue;
            }
            rows.push((0..6).map(|c| of[c * (of.len() - 1) / 5]).collect());
        }
        let back = stood.unwrap_or(frames.len() - 1);
        rows.push(
            (0..6)
                .map(|c| (back + c * 3).saturating_sub(6).min(frames.len() - 1))
                .collect(),
        );
        rows
    };
    let mut out = String::new();
    for (row, picks) in rows.iter().enumerate() {
        for (column, &i) in picks.iter().enumerate() {
            let _ = writeln!(out, "{row} {column} 0 0 0 0 1 {}", frames[i].1);
        }
    }
    let path = std::env::var("OVERLANDS_POSES").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join(format!(
                "poses-{family:?}{}.txt",
                if lake { "-lake" } else { "" }
            ))
            .display()
            .to_string()
    });
    std::fs::write(&path, out).expect("the dump is written");
    println!("wrote {path}: rows {rows:?}");
}

#[test]
#[ignore = "a probe: dumps a flown life's poses for the engine's renderer"]
fn probe_dump_a_flown_life() {
    for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
        dump(family, false);
    }
    dump(WingedFamily::Bird, true);
}
