//! A winged body through the fill, a rebuild and the chat words (#1875,
//! #1878): what its words play, what a peer's playout does to its rests, and
//! what it keeps across the draft-to-full ladder.

use super::*;
use crate::pds::avatar::locomotion::{default_winged, fitted_record};
use symbios_avatar::WingedFamily;

const WINGED: [WingedFamily; 3] = [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider];

/// The rest a body of `family` goes down into, from its own roster.
fn rest_of(family: WingedFamily) -> &'static str {
    match family {
        WingedFamily::Bird => "Perch",
        WingedFamily::Bat => "Rest",
        WingedFamily::Glider => "Curl",
    }
}

/// **A chat line plays what a winged body's own family has** (#1878, the
/// owner's words): through the table and the system a chat line reaches a body
/// by, on each family - a person's and a beast's words by its own names, a
/// refusal nothing, "lie down" a bird's roost and a bat's or a glider's own
/// lying rest, and none of a person's or a beast's gestures by their names.
/// Before #1878 every word played nothing.
#[test]
fn a_chat_line_plays_what_a_winged_body_has() {
    use WingedFamily::{Bat, Bird, Glider};
    let table: [(&str, [Option<&str>; 3]); 14] = [
        (
            "hello",
            [Some("Wing Stretch"), Some("Wing Stretch"), Some("Stretch")],
        ),
        ("yes", [Some("Head Tilt"); 3]),
        ("thanks", [Some("Preen"), Some("Groom"), Some("Groom")]),
        ("groom", [Some("Preen"), Some("Groom"), Some("Groom")]),
        ("no", [None; 3]),
        ("sit", [Some("Perch"), Some("Rest"), Some("Curl")]),
        ("perch", [Some("Perch"), Some("Rest"), Some("Curl")]),
        ("curl", [Some("Perch"), Some("Rest"), Some("Curl")]),
        ("lie down", [Some("Roost"), Some("Rest"), Some("Curl")]),
        ("lay down", [Some("Roost"), Some("Rest"), Some("Curl")]),
        ("roost", [Some("Roost"), Some("Rest"), Some("Curl")]),
        ("shake", [Some("Shake"); 3]),
        ("get up", [None; 3]),
        ("the weather is fine", [None; 3]),
    ];
    for (index, family) in [Bird, Bat, Glider].into_iter().enumerate() {
        let body = default_winged(family);
        let (mut app, chassis, root) = speaking(fitted_record(body.clone()), &body);
        for (text, plays) in table {
            say(&mut app, chassis, text);
            assert_eq!(
                gesture_of(&app, root).as_deref(),
                plays[index],
                "a {family:?}: {text:?}"
            );
            // Back to standing, and past the cooldown, for the next word.
            say(&mut app, chassis, "get up");
            run(&mut app, 4.0);
            assert_eq!(gesture_of(&app, root), None, "a {family:?} after {text:?}");
        }
    }
}

/// **Told to rest, a winged body stays down until it is told to get up**
/// (#1878), said in chat as its owner says it: a bird's perch and its roost,
/// a bat's rest and a glider's curl, each held, and each got up from where it
/// is.
#[test]
fn a_winged_body_told_to_rest_stays_until_told_to_get_up() {
    for (family, down, up) in [
        (WingedFamily::Bird, "perch", "get up"),
        (WingedFamily::Bird, "lie down", "up"),
        (WingedFamily::Bat, "sit", "get up"),
        (WingedFamily::Glider, "curl", "up"),
    ] {
        let body = default_winged(family);
        let (mut app, chassis, root) = speaking(fitted_record(body.clone()), &body);
        say(&mut app, chassis, down);
        let told = gesture_of(&app, root).expect("it starts");
        run(&mut app, 8.0);
        assert_eq!(
            gesture_of(&app, root).as_deref(),
            Some(told.as_str()),
            "a {family:?}: {down:?}"
        );
        say(&mut app, chassis, up);
        run(&mut app, 4.0);
        assert_eq!(
            gesture_of(&app, root),
            None,
            "a {family:?} still down after {up:?}"
        );
    }
}

/// **A peer creeping down a slope does not get a resting winged body up**
/// (#1878), fed what a sender under the throttle's line sends - a sample every
/// 30th tick, about twice a second - at 60 and 144 Hz: a perched bird, a
/// resting bat and a curled glider each stay down, reading no speed at all.
/// Without the hold the playout's crossing read 0.3 m/s and over, past the
/// engine's line, and stood each one up.
#[test]
fn a_creeping_winged_peer_stays_at_rest() {
    for family in WINGED {
        let body = default_winged(family);
        let rest = rest_of(family);
        for (creep, hz) in [(0.012, 60.0), (0.012, 144.0), (0.09, 60.0)] {
            let (app, root, played, read) = creeping_peer(
                creep,
                hz,
                6.0,
                Some(rest),
                &body,
                fitted_record(body.clone()),
            );
            println!("a {family:?} creeping {creep} m/s at {hz} Hz plays out at {played:.3} m/s");
            assert!(
                played > 0.1,
                "the premise: over the engine's line, at {played:.3}"
            );
            assert_eq!(
                gesture_of(&app, root).as_deref(),
                Some(rest),
                "a creep of {creep} m/s got a peer's {family:?} up at {hz} Hz"
            );
            assert_eq!(
                read, 0.0,
                "the fill read a {family:?}'s creep of {creep} at {hz} Hz"
            );
        }
    }
}

/// A peer's chassis under the real jitter buffer, fed a still sender's
/// cadence (a sample every 30th tick) while it creeps at `creep` m/s, with
/// `body` landed at the draft atlas and told to `rest`, then rebuilt as
/// `next` at the full atlas a second into the creep, and run on for two
/// seconds more: the rest it holds then, and the fastest the fill read.
fn rebuilt_resting_peer(
    body: &EngineAvatarRecord,
    next: &EngineAvatarRecord,
    rest: &str,
    creep: f32,
) -> (Option<String>, f32) {
    let record = fitted_record(body.clone());
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = app();
    app.insert_resource(crate::network::SmootherConfigRes::default());
    fn play_out(
        time: Res<Time>,
        cfg: Res<crate::network::SmootherConfigRes>,
        mut peers: Query<(&mut Transform, &mut TransformBuffer)>,
    ) {
        for (mut place, mut buffer) in &mut peers {
            if let Some((position, rotation)) = buffer.smoothed_at(time.elapsed_secs_f64(), &cfg.0)
            {
                place.translation = position;
                place.rotation = rotation;
            }
        }
    }
    app.add_systems(
        Update,
        play_out
            .before(fill_rigged_drive)
            .before(land_rigged_builds),
    );
    let at = Vec3::new(-300.0, offset, 700.0);
    let chassis = chassis(&mut app, Under::Peer, record, at);
    app.world_mut()
        .entity_mut(chassis)
        .insert(TransformBuffer::default());
    land(&mut app, chassis, body, 64, offset);
    let cfg = app
        .world()
        .resource::<crate::network::SmootherConfigRes>()
        .0;
    let send = cfg.expected_send_interval_secs
        * f64::from(crate::config::network::STATIONARY_BROADCAST_DIVISOR);
    let mut sent = 0.0f64;
    let mut gone = 0.0f32;
    let mut feed = |app: &mut App, speed: f32| {
        let now = app.world().resource::<Time>().elapsed_secs_f64();
        while sent + send <= now {
            sent += send;
            gone += speed * send as f32;
            app.world_mut()
                .get_mut::<TransformBuffer>(chassis)
                .expect("a buffer")
                .push_sample(at + Vec3::X * gone, Quat::IDENTITY, now, &cfg);
        }
    };
    let frames = |seconds: f32| (seconds / FRAME) as usize;
    for _ in 0..frames(1.5) {
        feed(&mut app, 0.0);
        app.update();
    }
    let root = root_of(&mut app, chassis);
    ask(&mut app, root, rest);
    for _ in 0..frames(3.0) {
        feed(&mut app, 0.0);
        app.update();
    }
    assert_eq!(gesture_of(&app, root).as_deref(), Some(rest), "the premise");
    let mut read = 0.0f32;
    let mut watch = |app: &mut App| {
        let root = root_of(app, chassis);
        let velocity = app.world().get::<Drive>(root).expect("a drive").velocity;
        read = read.max(Vec2::new(velocity.x, velocity.z).length());
    };
    for _ in 0..frames(1.0) {
        feed(&mut app, creep);
        app.update();
        watch(&mut app);
    }
    let offset = crate::player::visuals::rigged_root_offset(&fitted_record(next.clone()));
    feed(&mut app, creep);
    land(&mut app, chassis, next, 128, offset);
    assert_ne!(root_of(&mut app, chassis), root, "the body was rebuilt");
    for _ in 0..frames(2.0) {
        feed(&mut app, creep);
        app.update();
        watch(&mut app);
    }
    (gesture_under(&mut app, chassis), read)
}

/// **A perched bird stays perched across a draft-to-full rebuild, at the real
/// wire cadence** (#1878): a peer's rest, read off the jitter buffer a still
/// sender feeds twice a second while its chassis creeps down a slope, rebuilt
/// at the full atlas as every peer's body is about a second after it lands -
/// and a resting bat and a curled glider the same. Without the hold the creep
/// stood each one up before the rebuild ever landed.
#[test]
fn a_perched_peer_stays_perched_across_a_rebuild() {
    for family in WINGED {
        let body = default_winged(family);
        let rest = rest_of(family);
        let (held, read) = rebuilt_resting_peer(&body, &body, rest, 0.012);
        assert_eq!(read, 0.0, "a {family:?}'s creep was read as travel");
        assert_eq!(held.as_deref(), Some(rest), "a {family:?} got up");
    }
}

/// **A rest goes on as the new family's across a change of family** (#1878):
/// a perched bird made a bat rests as a bat rests, a resting bat made a
/// glider curls, a curled glider made a bird perches, and a roosting bird made
/// a glider curls too - each at the real wire cadence, through the rebuild
/// every peer runs. Before #1878 the new body's driver dropped a rest name its
/// roster lacks, and each stood up.
#[test]
fn a_rest_goes_on_as_the_new_familys_across_a_change_of_family() {
    use WingedFamily::{Bat, Bird, Glider};
    for (from, rest, to) in [
        (Bird, "Perch", Bat),
        (Bat, "Rest", Glider),
        (Glider, "Curl", Bird),
        (Bird, "Roost", Glider),
    ] {
        let (held, _) = rebuilt_resting_peer(&default_winged(from), &default_winged(to), rest, 0.0);
        assert_eq!(
            held.as_deref(),
            Some(rest_of(to)),
            "a {from:?} in its {rest} made a {to:?}"
        );
    }
}

/// What a resting winged body is told before its record changes family
/// ([`rest_then_rebuilt`]).
#[derive(Clone, Copy, Debug)]
enum Told {
    /// Held, then "get up", and rebuilt 0.3 s into the rise.
    UpFromHeld,
    /// "Get up" 40 % of the way down, and rebuilt 0.1 s later.
    UpPartWay,
    /// Held, then a shake, which gets it up first, and rebuilt 0.3 s later.
    ShakeFromHeld,
    /// Nothing: rebuilt 40 % of the way down.
    NothingPartWay,
}

/// A peer's body of `from` told to rest, then `told`, rebuilt as `to` and run
/// on eight seconds at `hz`: the gesture it is in then.
fn rest_then_rebuilt(from: WingedFamily, to: WingedFamily, hz: f32, told: Told) -> Option<String> {
    let body = default_winged(from);
    let (mut app, chassis) = standing(&body);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / hz,
    )));
    let run_at = |app: &mut App, seconds: f32| {
        for _ in 0..(seconds * hz).ceil() as usize {
            app.update();
        }
    };
    let root = root_of(&mut app, chassis);
    let rest = rest_of(from);
    let down = emote::seconds(&rig_of(&app, root), rest).expect("a rest");
    ask(&mut app, root, rest);
    let wait = match told {
        Told::UpFromHeld | Told::ShakeFromHeld => {
            run_at(&mut app, down + 0.5);
            let then = match told {
                Told::UpFromHeld => emote::GET_UP,
                _ => "Shake",
            };
            ask(&mut app, root, then);
            0.3
        }
        Told::UpPartWay => {
            run_at(&mut app, 0.4 * down);
            ask(&mut app, root, emote::GET_UP);
            0.1
        }
        Told::NothingPartWay => 0.4 * down,
    };
    run_at(&mut app, wait);
    assert_eq!(gesture_of(&app, root).as_deref(), Some(rest), "the premise");
    let next = default_winged(to);
    let offset = crate::player::visuals::rigged_root_offset(&fitted_record(next.clone()));
    land(&mut app, chassis, &next, 128, offset);
    run_at(&mut app, 8.0);
    gesture_under(&mut app, chassis)
}

/// **A winged body getting up when its family changes stands up** (#1878),
/// at 30, 60 and 144 Hz: told "get up" from its rest, part way down or held,
/// or got up by another gesture, and rebuilt as another family a moment
/// later - as a peer is when its owner's new record lands within a second of
/// the word - it is standing eight seconds on, as its owner sees it, and the
/// same family rebuilt stands too. Read only off the rest's name and how long
/// it had run, each was seated again in the new family's rest and held there
/// (a bird made a bat in its Rest, a bat a glider in its Curl, a glider a bird
/// on its Perch), and one told part way down went on down.
#[test]
fn a_winged_body_getting_up_stands_up_across_a_change_of_family() {
    use WingedFamily::{Bat, Bird, Glider};
    let mut seated = Vec::new();
    for hz in [30.0, 60.0, 144.0] {
        for (from, to) in [(Bird, Bat), (Bat, Glider), (Glider, Bird), (Bird, Bird)] {
            for told in [Told::UpFromHeld, Told::UpPartWay, Told::ShakeFromHeld] {
                let after = rest_then_rebuilt(from, to, hz, told);
                let resting = after
                    .as_deref()
                    .is_some_and(|name| WINGED.iter().any(|&f| rest_of(f) == name));
                if resting {
                    seated.push(format!(
                        "{from:?} to {to:?} at {hz} Hz after {told:?}: {after:?}"
                    ));
                }
            }
        }
    }
    assert!(
        seated.is_empty(),
        "{} still resting:\n{}",
        seated.len(),
        seated.join("\n")
    );
}

/// **A rest still going down goes on as the new family's** (#1878): a body
/// rebuilt as another family 40 % of the way into its rest, untouched, is in
/// the new family's rest eight seconds on, held.
#[test]
fn a_rest_going_down_goes_on_as_the_new_familys() {
    use WingedFamily::{Bat, Bird, Glider};
    for (from, to) in [(Bird, Bat), (Bat, Glider), (Glider, Bird)] {
        let after = rest_then_rebuilt(from, to, 60.0, Told::NothingPartWay);
        assert_eq!(
            after.as_deref(),
            Some(rest_of(to)),
            "{from:?} made a {to:?}"
        );
    }
}

/// `body` on a peer's chassis, landed at the draft atlas and stood a moment.
fn standing(body: &EngineAvatarRecord) -> (App, Entity) {
    let record = fitted_record(body.clone());
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = app();
    let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * offset);
    land(&mut app, chassis, body, 64, offset);
    run(&mut app, 0.5);
    (app, chassis)
}

/// **A resting winged body stays down through the draft-to-full rebuild, and
/// no joint jumps** (#1875): its driver is carried across a rebuild of the
/// same record as a beast's is, every family, so a perched bird, a resting bat
/// and a curled glider are still in their rest on the new body and posed where
/// the old one was. Given a fresh driver, each stood up.
#[test]
fn a_resting_winged_body_stays_down_through_a_rebuild() {
    for family in WINGED {
        let body = default_winged(family);
        let (mut app, chassis) = standing(&body);
        let root = root_of(&mut app, chassis);
        let rest = rest_of(family);
        let down = emote::seconds(&rig_of(&app, root), rest).expect("a rest");
        ask(&mut app, root, rest);
        run(&mut app, down + 0.5);
        assert_eq!(gesture_of(&app, root).as_deref(), Some(rest), "{family:?}");
        let mut was = drawn(&app, root);
        let resting = worst_step(&mut app, chassis, &mut was, 30);
        let offset = crate::player::visuals::rigged_root_offset(&fitted_record(body.clone()));
        land(&mut app, chassis, &body, 128, offset);
        let new_root = root_of(&mut app, chassis);
        assert_ne!(new_root, root, "{family:?}: the body was rebuilt");
        let landed = moved(&was, &drawn(&app, new_root));
        was = drawn(&app, new_root);
        let after = worst_step(&mut app, chassis, &mut was, 30);
        let allowed = resting + 0.001;
        assert!(
            landed <= allowed && after <= allowed,
            "{family:?}: a joint jumped {:.1} mm, {:.1} mm after, where a resting frame moves \
             {:.1} mm",
            landed * 1000.0,
            after * 1000.0,
            resting * 1000.0
        );
        assert_eq!(
            gesture_of(&app, new_root).as_deref(),
            Some(rest),
            "{family:?}: the rebuild got it up"
        );
    }
}

/// **A flying winged body flies on through a rebuild** (#1875): told by its
/// host that it glides, it is still gliding, in the same flight, on the new
/// body - not snapped shut and stood on the ground by a fresh driver. Across a
/// rebuild of the same record, and across a change of family: a bird made a
/// bat, a bat a glider and a glider a bird.
#[test]
fn a_flying_winged_body_flies_on_through_a_rebuild() {
    for (index, family) in WINGED.into_iter().enumerate() {
        let body = default_winged(family);
        let changed = default_winged(WINGED[(index + 1) % WINGED.len()]);
        for next in [&body, &changed] {
            flies_on(&body, next, family);
        }
    }
}

/// A body of `family` gliding when it is rebuilt as `next`: still gliding,
/// in the flight it was in.
fn flies_on(body: &EngineAvatarRecord, next: &EngineAvatarRecord, family: WingedFamily) {
    let to = crate::pds::avatar::RiggedKind::of_record(next);
    let (mut app, chassis) = standing(body);
    let root = root_of(&mut app, chassis);
    // Its owner says it glides (#1877: the fill hands the engine a peer's
    // word, so a flight written on the drive is written over).
    app.world_mut()
        .entity_mut(chassis)
        .insert(crate::player::PeerFlight::heard_at(
            crate::player::FlightMode::Glide,
            -1.0,
        ));
    run(&mut app, 1.0);
    let flown = |app: &App, root: Entity| {
        let rig = rig_of(app, root);
        app.world()
            .get::<AvatarDriver>(root)
            .expect("a driver")
            .flight(&rig)
    };
    let before = flown(&app, root).expect("the premise: it flies");
    let offset = crate::player::visuals::rigged_root_offset(&fitted_record(next.clone()));
    land(&mut app, chassis, next, 128, offset);
    let new_root = root_of(&mut app, chassis);
    let after = flown(&app, new_root).unwrap_or_else(|| panic!("{family:?} to {to:?}: landed"));
    assert_eq!(after.mode, before.mode, "{family:?} to {to:?}");
    assert!(
        after.seconds > before.seconds,
        "{family:?} to {to:?}: a fresh flight, {:.2} s in after {:.2} s",
        after.seconds,
        before.seconds
    );
}

/// The pose the body under `root` was drawn in: across a change of family,
/// whose joints differ, its root is what can be compared.
fn pose_of(app: &App, root: Entity) -> symbios_avatar::Pose {
    app.world()
        .get::<bevy_symbios_avatar::AvatarPose>(root)
        .expect("a pose")
        .0
        .clone()
}

/// **A winged driver carried across a change of family runs, and the body
/// does not jump** (#1875): a resting bird made a bat, a bat made a glider and
/// a glider made a bird, each carried as a beast's is across a change of
/// beast. The new body's pose is whole and finite on its own rig, its root
/// where the old one's was, and it can get up from there. (The rest goes on as
/// the new family's, #1878: `a_rest_goes_on_as_the_new_familys_across_a_change_of_family`.)
#[test]
fn a_winged_driver_carried_across_a_change_of_family_runs() {
    let shapes: Vec<EngineAvatarRecord> = WINGED.iter().map(|&f| default_winged(f)).collect();
    let (mut app, chassis) = standing(&shapes[0]);
    for next in 1..=shapes.len() {
        let (from, to) = (&shapes[next - 1], &shapes[next % shapes.len()]);
        let family =
            |body: &EngineAvatarRecord| match crate::pds::avatar::RiggedKind::of_record(body) {
                crate::pds::avatar::RiggedKind::Winged(family) => family,
                other => panic!("not winged: {other:?}"),
            };
        let root = root_of(&mut app, chassis);
        let rest = rest_of(family(from));
        ask(&mut app, root, rest);
        let down = emote::seconds(&rig_of(&app, root), rest).expect("a rest");
        run(&mut app, down + 0.5);
        let was = pose_of(&app, root);
        let offset = crate::player::visuals::rigged_root_offset(&fitted_record(to.clone()));
        land(&mut app, chassis, to, 64, offset);
        let new_root = root_of(&mut app, chassis);
        let check = |app: &App, when: &str| {
            let rig = rig_of(app, new_root);
            let pose = pose_of(app, new_root);
            assert_eq!(pose.rotations.len(), rig.len(), "{when}");
            assert!(pose.translation.is_finite(), "{when}");
            assert!(pose.rotations.iter().all(|turn| turn.is_finite()), "{when}");
        };
        check(&app, "landed");
        let jumped = pose_of(&app, new_root)
            .translation
            .distance(was.translation);
        assert!(
            jumped < 0.25,
            "{:?} to {:?}: the root jumped {:.0} mm",
            family(from),
            family(to),
            jumped * 1000.0
        );
        run(&mut app, 0.5);
        check(&app, "rested");
        ask(&mut app, new_root, emote::GET_UP);
        run(&mut app, 4.0);
        check(&app, "got up");
        assert_eq!(
            gesture_of(&app, new_root),
            None,
            "{:?} did not get up",
            family(to)
        );
    }
}

/// **Across a change of kind nothing is carried, either way** (#1875): a
/// person made winged and back, and a beast made winged and back, each get the
/// fresh driver a rebuild gives a body of another kind - a person's bow, a
/// beast's sit and a bird's perch all end with the body that played them.
#[test]
fn nothing_is_carried_into_or_out_of_a_winged_body() {
    let person = engine_default_for_did("did:plc:rebuild-test");
    let dog = default_beast(Family::Dog);
    let bird = default_winged(WingedFamily::Bird);
    let bat = default_winged(WingedFamily::Bat);
    for (other, other_plays, winged, winged_plays) in [
        (&person, Emote::Bow.gesture_name(), &bird, "Perch"),
        (&dog, "Sit", &bat, "Rest"),
        (&dog, "Sit", &bird, "Perch"),
        (&person, Emote::Bow.gesture_name(), &bat, "Rest"),
    ] {
        let (mut app, chassis) = standing(other);
        let root = root_of(&mut app, chassis);
        ask(&mut app, root, other_plays);
        run(&mut app, 0.2);
        assert!(
            gesture_of(&app, root).is_some(),
            "the premise: {other_plays}"
        );
        land(&mut app, chassis, winged, 64, 0.2);
        assert_eq!(
            gesture_under(&mut app, chassis),
            None,
            "{other_plays} carried into a winged body"
        );
        let root = root_of(&mut app, chassis);
        ask(&mut app, root, winged_plays);
        run(&mut app, 0.2);
        assert!(
            gesture_of(&app, root).is_some(),
            "the premise: {winged_plays}"
        );
        land(&mut app, chassis, other, 64, 0.2);
        assert_eq!(
            gesture_under(&mut app, chassis),
            None,
            "{winged_plays} carried out of a winged body"
        );
    }
}
