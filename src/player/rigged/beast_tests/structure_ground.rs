//! A beast resting on a structure (#1834) - a road's ribbon, a deck at its
//! edge, a floor walled in, a block that moves - and what it is handed, on
//! its owner's client and on a peer's.

use super::*;
use crate::player::rigged::underfoot::{PAST_AN_EDGE, RestsOn};
use avian3d::prelude::{Collider, RigidBody};
use symbios_avatar::FootingConfig;

/// A solid laid in a bench: its half-extents, its pose and its body.
type Solid = (Vec3, Transform, RigidBody);

/// A box spanning `min` to `max` with `body`, as [`bench`] lays one.
fn solid(min: Vec3, max: Vec3, body: RigidBody) -> Solid {
    let (half, at) = block(min, max);
    (half, at, body)
}

/// [`grounded`], with each solid's own body - a static floor or a kinematic
/// block - and `trimeshes` laid as static bodies, stepped until avian's
/// collider trees hold them.
fn bench(ground: &dyn Fn(f32, f32) -> f32, solids: &[Solid], trimeshes: &[Collider]) -> App {
    let ground = terrain(ground);
    let mut app = app();
    app.add_plugins(avian3d::prelude::PhysicsPlugins::default());
    app.world_mut().spawn(terrain_body(&ground));
    for (half, at, body) in solids {
        app.world_mut().spawn((
            *at,
            *body,
            Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0),
        ));
    }
    for trimesh in trimeshes {
        app.world_mut()
            .spawn((Transform::IDENTITY, RigidBody::Static, trimesh.clone()));
    }
    app.insert_resource(ground);
    app.finish();
    app.cleanup();
    app.update();
    app.update();
    app
}

/// A `family` beast resting at `place` in a [`bench`], on either client: the
/// app, the chassis and the root. A peer is played out where its owner's own
/// chassis came to rest there, as [`beast_resting`] plays one.
fn resting_on(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[Solid],
    trimeshes: &[Collider],
    under: Under,
    family: Family,
    place: Transform,
) -> (App, Entity, Entity) {
    let place = match under {
        Under::Local => place,
        Under::Peer => {
            let (app, chassis, _) =
                resting_on(ground, solids, trimeshes, Under::Local, family, place);
            *app.world().get::<Transform>(chassis).expect("a chassis")
        }
    };
    let mut app = bench(ground, solids, trimeshes);
    let (chassis, root) = standing(&mut app, under, family, place);
    (app, chassis, root)
}

/// The answer `root`'s body stands by, and how much of the structure's
/// ground its patch carries.
fn stands_by(app: &App, root: Entity) -> (Option<RestsOn>, f32) {
    app.world()
        .get::<RiggedTrail>(root)
        .expect("a trail")
        .rest
        .stands_by()
}

/// A road's ribbon as `terrain::roads` lays a network's deck (#1833): one
/// trimesh sheet `width` wide along `z` across `-length / 2..length / 2`,
/// `lift(x, z)` over `ground`, in half-metre lengths two triangles across.
fn ribbon(
    ground: &dyn Fn(f32, f32) -> f32,
    width: f32,
    length: f32,
    lift: &dyn Fn(f32, f32) -> f32,
) -> Collider {
    let segments = (length / 0.5).round() as u32;
    let mut vertices = Vec::new();
    for i in 0..=segments {
        let z = -length * 0.5 + i as f32 * 0.5;
        for x in [-width * 0.5, width * 0.5] {
            vertices.push(Vec3::new(x, ground(x, z) + lift(x, z), z));
        }
    }
    let indices = (0..segments)
        .flat_map(|i| {
            [
                [2 * i, 2 * i + 2, 2 * i + 1],
                [2 * i + 1, 2 * i + 2, 2 * i + 3],
            ]
        })
        .collect();
    Collider::trimesh(vertices, indices)
}

/// The patch's ground under the lowest point of each foot `root`'s body is
/// drawn standing on, against `surface` there: how far over it each sole is.
fn soles_over(app: &App, root: Entity, surface: &dyn Fn(f32, f32) -> f32) -> Vec<(Vec3, f32)> {
    drawn_soles(app, root)
        .into_iter()
        .map(|sole| (sole, sole.y - surface(sole.x, sole.z)))
        .collect()
}

/// **A beast resting on a sloped road stands on its top** (#1834). A road's
/// ribbon - one trimesh sheet, as a network's deck is - laid 0.2 m over a
/// hill of 8 and 20 degrees takes a dog and a horse resting on it, facing up
/// the road and side on to it, on either client: every sample of the patch
/// over the ribbon is the ribbon's top, and every foot stands on it. On 20
/// degrees a horse's hind feet are half a metre down the road from where its
/// level capsule rests on its uphill end, past the step a level floor's band
/// allows, and are stood on the road all the same: the band is the road's
/// plane, not the level floor's. Until #1834 a beast resting on a road kept
/// the level floor through the slope, its downhill feet over the road and its
/// uphill ones in it.
///
/// **Near level is level**: on a ribbon whose grade is 1e-4, its vertices a
/// millimetre up or down, 2 cm over the terrain - and 2 cm under it, where
/// the terrain is what the chassis rests on - every sample is within 4 mm of
/// the root's own floor, which a patch of zeros is driven as bit for bit.
#[test]
fn a_beast_on_a_sloped_road_stands_on_its_top() {
    const WIDTH: f32 = 6.0;
    const LENGTH: f32 = 14.0;
    let noise = |x: f32, z: f32| 0.001 * libm::sinf(7.3 * x + 3.1) * libm::cosf(5.7 * z);
    let cases: [(f32, f32, bool); 4] = [
        (libm::tanf(20f32.to_radians()), 0.2, false),
        (libm::tanf(8f32.to_radians()), 0.2, false),
        (1e-4, 0.02, true),
        (1e-4, -0.02, true),
    ];
    for (grade, lift, near_level) in cases {
        // The hill rises toward -Z, the way a chassis faces at no yaw.
        let ground = move |_: f32, z: f32| -grade * z;
        let lift_at = move |x: f32, z: f32| lift + if near_level { noise(x, z) } else { 0.0 };
        let road = ribbon(&ground, WIDTH, LENGTH, &lift_at);
        // The ground a foot stands on over the road: its top, or the terrain
        // over a ribbon buried in it.
        let top = move |x: f32, z: f32| ground(x, z) + lift_at(x, z).max(0.0);
        for family in [Family::Dog, Family::Horse] {
            let capsule = capsule_of(family);
            let record = beast_record(family);
            for yaw in [0.0f32, std::f32::consts::FRAC_PI_2] {
                // Dropped onto the road from a hand over its highest reach.
                let end = capsule.to.max(-capsule.from) + capsule.radius;
                let place = Transform::from_xyz(
                    0.0,
                    lift.max(0.0) + grade * end + drop_of(&record) + 0.05,
                    0.0,
                )
                .with_rotation(Quat::from_rotation_y(yaw));
                for under in [Under::Local, Under::Peer] {
                    let (app, _, root) = resting_on(
                        &ground,
                        &[],
                        std::slice::from_ref(&road),
                        under,
                        family,
                        place,
                    );
                    let label = format!("{family:?} {under:?} at {yaw} on grade {grade} {lift}");
                    let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
                    let drive = app.world().get::<Drive>(root).expect("a drive");
                    assert!(
                        drive.ground.is_some(),
                        "{label}: on the road, and handed none"
                    );
                    let (mut over, mut worst) = (0, 0.0f32);
                    for (world, got) in patch_samples(drive, &posed) {
                        if world.x.abs() > WIDTH * 0.5 - 0.1 || world.z.abs() > LENGTH * 0.5 - 0.6 {
                            continue;
                        }
                        over += 1;
                        let want = top(world.x, world.z);
                        worst = worst.max((got - want).abs());
                        assert!(
                            (got - want).abs() < 2e-3,
                            "{label}: over the road at {world} the ground is {got:.4}, its top \
                             {want:.4}"
                        );
                        if near_level {
                            let floor = posed.translation().y;
                            assert!(
                                (got - floor).abs() < 4e-3,
                                "{label}: near level, a sample {:.1} mm off the floor at {world}",
                                (got - floor) * 1000.0
                            );
                        }
                    }
                    assert!(over > 40, "{label}: the road is not under the reach");
                    let soles = soles_over(&app, root, &top);
                    let lowest = soles.iter().map(|s| s.1).fold(f32::INFINITY, f32::min);
                    let highest = soles.iter().map(|s| s.1).fold(f32::NEG_INFINITY, f32::max);
                    println!(
                        "{label}: {over} samples within {:.2} mm of the top, soles {:+.1} to \
                         {:+.1} mm off it",
                        worst * 1000.0,
                        lowest * 1000.0,
                        highest * 1000.0
                    );
                    for (sole, off) in soles {
                        assert!(
                            (-0.025..=0.03).contains(&off),
                            "{label}: a foot at {sole} stands {:+.1} mm off the road",
                            off * 1000.0
                        );
                    }
                }
            }
        }
    }
}

/// **A beast at a deck's edge holds its legs over the drop at the deck**
/// (#1834). A deck a metre over flat ground takes a dog and a horse resting
/// with their rear end, and then their side, a hand short of its edge, on
/// either client: their reach runs on past the edge, over the ground a metre
/// down - out of the step's band under the deck's plane - and every sample
/// there is held at the deck's plane, or as far under it as
/// [`super::super::underfoot::PAST_AN_EDGE`] says (the owner's call), so no
/// foot is put down under the deck less the engine's own `max_pelvis_drop`;
/// held at it, every foot stands on the deck's plane. Read through the band,
/// those samples were the ground a metre down and the legs over the edge hung
/// for it; resting on a deck before #1834, a beast was handed no patch at all.
#[test]
fn a_beast_at_a_decks_edge_holds_its_legs_at_the_deck() {
    const DECK: f32 = 1.0;
    let flat = |_: f32, _: f32| 0.0;
    let drop_limit = FootingConfig::default().max_pelvis_drop;
    for family in [Family::Dog, Family::Horse] {
        let capsule = capsule_of(family);
        let record = beast_record(family);
        // Facing -Z, its rear (the rig's -Z) is toward +Z, its side toward +X.
        let rear = -capsule.from + capsule.radius + 0.05;
        let side = capsule.radius + 0.05;
        let edges = [
            ("rear", Vec3::new(4.0, DECK, rear)),
            ("side", Vec3::new(side, DECK, 6.0)),
        ];
        for (way, max) in edges {
            let deck = solid(Vec3::new(-4.0, 0.0, -6.0), max, RigidBody::Static);
            let place = Transform::from_xyz(0.0, DECK + drop_of(&record) + 0.01, 0.0);
            for under in [Under::Local, Under::Peer] {
                let (app, _, root) = resting_on(&flat, &[deck], &[], under, family, place);
                let label = format!("{family:?} {under:?}, its {way} to the edge");
                let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
                let drive = app.world().get::<Drive>(root).expect("a drive");
                assert!(
                    drive.ground.is_some(),
                    "{label}: on the deck, and handed none"
                );
                let mut past = 0;
                for (world, got) in patch_samples(drive, &posed) {
                    // Clear of the edge, where a sample may be either.
                    let (beyond, on) = (
                        world.x > max.x + 0.1 || world.z > max.z + 0.1,
                        world.x < max.x - 0.1 && world.z < max.z - 0.1,
                    );
                    past += usize::from(beyond);
                    assert!(
                        got >= DECK - drop_limit - 1e-3,
                        "{label}: at {world} a foot is put down at {got:.3}, under the deck less \
                         the pelvis' drop"
                    );
                    let want = if beyond { DECK - PAST_AN_EDGE } else { DECK };
                    assert!(
                        !(beyond || on) || (got - want).abs() < 1e-3,
                        "{label}: at {world} the ground is {got:.3}, {want} wanted"
                    );
                }
                assert!(past > 10, "{label}: the reach does not run past the edge");
                for (sole, off) in soles_over(&app, root, &|_, _| DECK) {
                    let lowest = if PAST_AN_EDGE == 0.0 {
                        -0.02
                    } else {
                        -drop_limit - 0.02
                    };
                    assert!(
                        (lowest..=0.005).contains(&off),
                        "{label}: a foot at {sole} stands {:+.1} mm off the deck",
                        off * 1000.0
                    );
                }
            }
        }
    }
}

/// **A beast in a walled room stands on its floor** (#1834). A floor 0.25 m
/// over flat ground, its walls two metres high standing on it a hand beyond
/// the body on every side, takes a dog and a horse resting on it, on either
/// client: the reach runs over the walls and on past them, and every sample
/// is the floor's top. A ray down over a wall starts inside it and reads
/// nothing, and the sample is the floor's plane - not the terrain under the
/// floor, a quarter of a metre down and inside the step's band, which a wall
/// read as the terrain path reads one would have put a foot down on.
#[test]
fn a_beast_in_a_walled_room_stands_on_its_floor() {
    const FLOOR: f32 = 0.25;
    let flat = |_: f32, _: f32| 0.0;
    for family in [Family::Dog, Family::Horse] {
        let capsule = capsule_of(family);
        let record = beast_record(family);
        let (w, l) = (
            capsule.radius + 0.15,
            capsule.to.max(-capsule.from) + capsule.radius + 0.15,
        );
        let wall = |min: Vec3, max: Vec3| solid(min, max, RigidBody::Static);
        let (low, high) = (FLOOR, FLOOR + 2.0);
        let room = [
            solid(
                Vec3::new(-4.0, -0.5, -6.0),
                Vec3::new(4.0, FLOOR, 6.0),
                RigidBody::Static,
            ),
            wall(
                Vec3::new(w, low, -l - 0.3),
                Vec3::new(w + 0.3, high, l + 0.3),
            ),
            wall(
                Vec3::new(-w - 0.3, low, -l - 0.3),
                Vec3::new(-w, high, l + 0.3),
            ),
            wall(Vec3::new(-w, low, l), Vec3::new(w, high, l + 0.3)),
            wall(Vec3::new(-w, low, -l - 0.3), Vec3::new(w, high, -l)),
        ];
        let place = Transform::from_xyz(0.0, FLOOR + drop_of(&record) + 0.01, 0.0);
        for under in [Under::Local, Under::Peer] {
            let (app, _, root) = resting_on(&flat, &room, &[], under, family, place);
            let label = format!("{family:?} {under:?}");
            assert_eq!(
                stands_by(&app, root),
                (Some(RestsOn::Structure), 1.0),
                "{label}: not standing by the floor"
            );
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let drive = app.world().get::<Drive>(root).expect("a drive");
            assert!(
                drive.ground.is_some(),
                "{label}: on the floor, and handed none"
            );
            let mut in_walls = 0;
            for (world, got) in patch_samples(drive, &posed) {
                let in_wall = (w..w + 0.3).contains(&world.x.abs())
                    || ((l..l + 0.3).contains(&world.z.abs()) && world.x.abs() < w);
                in_walls += usize::from(in_wall);
                assert!(
                    (got - FLOOR).abs() < 1e-3,
                    "{label}: at {world}{} the ground is {got:.3}, the floor's top {FLOOR}",
                    if in_wall { ", in a wall," } else { "" }
                );
            }
            assert!(in_walls > 10, "{label}: the walls are not inside the reach");
            for (sole, off) in soles_over(&app, root, &|_, _| FLOOR) {
                assert!(
                    (-0.02..=0.005).contains(&off),
                    "{label}: a foot at {sole} stands {:+.1} mm off the floor",
                    off * 1000.0
                );
            }
        }
    }
}

/// **A beast on a block that moves is handed no patch** (#1834). A block
/// 0.3 m over flat ground takes a dog and a horse resting on it, on either
/// client: static, it is a structure and its top is handed over; kinematic -
/// a lift, a boat, anything a script moves - it is not, and the body keeps
/// the level floor its collider stands it on, as one on another body or a
/// boat always has: a patch sampled off a moving deck is a frame behind it.
#[test]
fn a_beast_on_a_block_that_moves_is_handed_no_patch() {
    const TOP: f32 = 0.3;
    let flat = |_: f32, _: f32| 0.0;
    for family in [Family::Dog, Family::Horse] {
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, TOP + drop_of(&record) + 0.01, 0.0);
        for (body, patched, rests) in [
            (RigidBody::Static, true, RestsOn::Structure),
            (RigidBody::Kinematic, false, RestsOn::Elsewhere),
        ] {
            let block = solid(Vec3::new(-3.0, -0.5, -3.0), Vec3::new(3.0, TOP, 3.0), body);
            for under in [Under::Local, Under::Peer] {
                let (app, _, root) = resting_on(&flat, &[block], &[], under, family, place);
                let drive = app.world().get::<Drive>(root).expect("a drive");
                assert_eq!(
                    (drive.ground.is_some(), stands_by(&app, root).0),
                    (patched, Some(rests)),
                    "{family:?} {under:?} on a {body:?} block"
                );
            }
        }
    }
}

/// **A peer's cast says what its owner's contacts say** (#1834). On each
/// bench - the bare hill, a deck laid over it, a pad cut into it at its
/// uphill edge with the body's front at that edge, the hill rising past it,
/// a static block and a kinematic one - a dog and a
/// horse's own chassis comes to rest, and its contacts are read; a peer
/// played out where it came to rest is cast down onto the same world, and
/// says the same: the terrain, a structure with or without the terrain
/// beside it, or elsewhere for a body that moves.
#[test]
fn a_peers_cast_says_what_the_owners_contacts_say() {
    let grade = libm::tanf(20f32.to_radians());
    let hill = move |_: f32, z: f32| -grade * z;
    let flat = |_: f32, _: f32| 0.0;
    let tilt = libm::atanf(grade);
    let half = Vec3::new(2.0, 0.025, 4.0);
    let deck = (
        half,
        Transform::from_xyz(
            0.0,
            0.08 - (libm::cosf(tilt) + grade * libm::sinf(tilt)) * half.y,
            0.0,
        )
        .with_rotation(Quat::from_rotation_x(tilt)),
        RigidBody::Static,
    );
    let pad = solid(
        Vec3::new(-4.0, -4.0 * grade - 1.0, -4.0),
        Vec3::new(4.0, 4.0 * grade, 4.0),
        RigidBody::Static,
    );
    let block = |body| solid(Vec3::new(-3.0, -0.5, -3.0), Vec3::new(3.0, 0.3, 3.0), body);
    for family in [Family::Dog, Family::Horse] {
        let capsule = capsule_of(family);
        let drop = drop_of(&beast_record(family));
        let front = capsule.to + capsule.radius;
        type Bench = (
            &'static str,
            Box<dyn Fn(f32, f32) -> f32>,
            Vec<Solid>,
            Transform,
            RestsOn,
        );
        let benches: Vec<Bench> = vec![
            (
                "the hill",
                Box::new(hill),
                Vec::new(),
                Transform::from_xyz(0.0, grade * front + drop + 0.05, 0.0),
                RestsOn::Terrain,
            ),
            (
                "a deck on the hill",
                Box::new(hill),
                vec![deck],
                Transform::from_xyz(0.0, 0.08 + grade * front + drop + 0.05, 0.0),
                RestsOn::Structure,
            ),
            (
                "a pad cut into the hill",
                Box::new(hill),
                vec![pad],
                Transform::from_xyz(0.0, 4.0 * grade + drop + 0.01, -4.0 + front + 0.05),
                RestsOn::Structure,
            ),
            (
                "a static block",
                Box::new(flat),
                vec![block(RigidBody::Static)],
                Transform::from_xyz(0.0, 0.3 + drop + 0.01, 0.0),
                RestsOn::Structure,
            ),
            (
                "a kinematic block",
                Box::new(flat),
                vec![block(RigidBody::Kinematic)],
                Transform::from_xyz(0.0, 0.3 + drop + 0.01, 0.0),
                RestsOn::Elsewhere,
            ),
        ];
        for (name, ground, solids, place, expected) in benches {
            let (mut app, chassis, _) =
                resting_on(&*ground, &solids, &[], Under::Local, family, place);
            let owner = app
                .world_mut()
                .run_system_once(move |underfoot: crate::player::Underfoot| {
                    underfoot.contacts_say(chassis)
                })
                .expect("the read runs");
            let (mut app, _, root) = resting_on(&*ground, &solids, &[], Under::Peer, family, place);
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let shape = capsule.collider();
            let peer = app
                .world_mut()
                .run_system_once(move |underfoot: crate::player::Underfoot| {
                    underfoot.resting_cast(&shape, &posed)
                })
                .expect("the cast runs");
            println!(
                "{family:?} on {name}: the owner's contacts {owner:?}, the peer's cast {peer:?}"
            );
            assert_eq!(
                (owner, peer),
                (expected, expected),
                "{family:?} on {name}: the owner's contacts and the peer's cast"
            );
        }
    }
}

/// How far a sample may be off the surface really under it, metres, in the
/// tests of a small solid and a road's crest (#1834 review).
const ON_ITS_SURFACE: f32 = 0.02;

/// **A beast resting on a log stands on the ground beside it** (#1834
/// review, finding 1). A log 0.3 m thick lies across flat ground behind a
/// dog's and a horse's centre, and each comes to rest on it, on either
/// client: the physics has it on a structure, and the log is one or two rows
/// of the patch's samples, curved. Every sample is the surface really under
/// it - the log's top, or the ground a log's thickness under the root's
/// floor, within the step's band of it - and no sole stands higher than the
/// level floor every beast on a structure stood on before #1834. The plane
/// fitted through the log's rows alone tilted with their curve and was
/// carried across the whole patch: samples 0.68 m over the ground behind a
/// horse, its hind soles 0.39 m over it.
#[test]
fn a_beast_resting_on_a_log_stands_on_the_ground_beside_it() {
    const RADIUS: f32 = 0.15;
    let flat = |_: f32, _: f32| 0.0;
    let open_under = 0.1;
    for family in [Family::Dog, Family::Horse] {
        let capsule = capsule_of(family);
        let record = beast_record(family);
        // Behind the centre (toward +Z, facing -Z), under the capsule's rear.
        let behind = (0.5 * -capsule.from).min(0.3);
        let log = Collider::compound(vec![(
            Vec3::new(0.0, RADIUS, behind),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Collider::cylinder(RADIUS, 8.0),
        )]);
        // The surface under a sample, and whether a sample there may read
        // either the log or the ground: at its sides, or where its underside
        // stands as high over the ground as a foot may stand under it.
        let surface = move |z: f32| {
            let off = z - behind;
            if off.abs() >= RADIUS {
                return (0.0, false);
            }
            let half = (RADIUS * RADIUS - off * off).sqrt();
            let (top, under) = (RADIUS + half, RADIUS - half);
            let either = off.abs() > RADIUS - 0.005 || (under - open_under).abs() < 0.01;
            (if under < open_under { top } else { 0.0 }, either)
        };
        let place = Transform::from_xyz(0.0, 2.0 * RADIUS + drop_of(&record) + 0.01, 0.0);
        for under in [Under::Local, Under::Peer] {
            let (app, _, root) =
                resting_on(&flat, &[], std::slice::from_ref(&log), under, family, place);
            let label = format!("{family:?} {under:?} on a log {behind:.2} m behind");
            assert_eq!(
                stands_by(&app, root).0,
                Some(RestsOn::Structure),
                "{label}: not on the log"
            );
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let floor = posed.translation().y;
            let drive = app.world().get::<Drive>(root).expect("a drive");
            let mut worst = 0.0f32;
            for (world, got) in patch_samples(drive, &posed) {
                let (want, either) = surface(world.z);
                let off = if either {
                    (got - want).abs().min(got.abs())
                } else {
                    (got - want).abs()
                };
                worst = worst.max(off);
                assert!(
                    off < ON_ITS_SURFACE,
                    "{label}: at {world} the ground is {got:.3}, the surface under it {want:.3}"
                );
            }
            let soles = soles_over(&app, root, &|_, z| surface(z).0);
            println!(
                "{label}: the floor at {floor:.3}, every sample within {:.1} mm of its surface, \
                 soles {:?} mm over it",
                worst * 1000.0,
                soles
                    .iter()
                    .map(|(_, off)| (off * 1000.0).round())
                    .collect::<Vec<_>>()
            );
            for (sole, _) in soles {
                assert!(
                    sole.y <= floor + 0.005,
                    "{label}: a foot at {sole} stands over the level floor at {floor:.3}"
                );
            }
        }
    }
}

/// **A dog resting on a stepping stone on a hill stands on the stone and the
/// hill** (#1834 review, finding 1). A level stone 0.6 m square stands a
/// little proud of a 20-degree hill, and a dog comes to rest on it, on
/// either client: every sample of the patch is the surface really under it -
/// the stone's top, or the hill around it - and no sole stands higher than
/// the level floor it stood on before #1834. The stone's level plane, read as
/// the structure under its feet, held the hill below it at that plane, 0.48 m
/// over the hill near its feet.
#[test]
fn a_dog_on_a_stepping_stone_on_a_hill_stands_on_both() {
    const HALF: f32 = 0.3;
    let grade = libm::tanf(20f32.to_radians());
    // The hill rises toward -Z, the way a chassis faces at no yaw.
    let hill = move |_: f32, z: f32| -grade * z;
    let family = Family::Dog;
    let capsule = capsule_of(family);
    let record = beast_record(family);
    // Proud of the hill under the capsule's uphill end, so it rests on it.
    let top = grade * (capsule.to + capsule.radius) + 0.03;
    let stone = solid(
        Vec3::new(-HALF, -1.0, -HALF),
        Vec3::new(HALF, top, HALF),
        RigidBody::Static,
    );
    let surface = move |x: f32, z: f32| {
        if x.abs() < HALF && z.abs() < HALF {
            top
        } else {
            hill(x, z)
        }
    };
    let place = Transform::from_xyz(0.0, top + drop_of(&record) + 0.01, 0.0);
    for under in [Under::Local, Under::Peer] {
        let (app, _, root) = resting_on(&hill, &[stone], &[], under, family, place);
        let label = format!("{family:?} {under:?} on a stone {top:.3} m up");
        assert_eq!(
            stands_by(&app, root).0,
            Some(RestsOn::Structure),
            "{label}: not on the stone"
        );
        let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
        let floor = posed.translation().y;
        let drive = app.world().get::<Drive>(root).expect("a drive");
        let mut worst = 0.0f32;
        for (world, got) in patch_samples(drive, &posed) {
            // At the stone's edge a sample may be either.
            if (world.x.abs() - HALF).abs() < 0.01 || (world.z.abs() - HALF).abs() < 0.01 {
                continue;
            }
            let want = surface(world.x, world.z);
            worst = worst.max((got - want).abs());
            assert!(
                (got - want).abs() < ON_ITS_SURFACE,
                "{label}: at {world} the ground is {got:.3}, the surface under it {want:.3}"
            );
        }
        let soles = soles_over(&app, root, &surface);
        println!(
            "{label}: the floor at {floor:.3}, every sample within {:.1} mm of its surface, \
             soles {:?} mm over it",
            worst * 1000.0,
            soles
                .iter()
                .map(|(_, off)| (off * 1000.0).round())
                .collect::<Vec<_>>()
        );
        for (sole, _) in soles {
            assert!(
                sole.y <= floor + 0.005,
                "{label}: a foot at {sole} stands over the level floor at {floor:.3}"
            );
        }
    }
}

/// **A beast on a road over a crest or in a sag stands on the road out to
/// its reach** (#1834 review, finding 2). A road's ribbon 0.2 m over a crest
/// falling away at 15 and at 20 degrees either side of it, and over a sag
/// rising at 20 degrees either side, takes a dog and a horse resting on it,
/// facing along the road and across it, on either client: every sample over
/// the road, out to the edge of the reach, is the road's top. The road curves
/// away from any plane through it, and read through a band of one, the
/// samples past the band were held at the plane - 0.38 m over the road at the
/// reach's edge on a 15-degree crest, 0.52 on a 20, and 0.52 under it in a sag.
#[test]
fn a_beast_on_a_road_over_a_crest_or_in_a_sag_stands_on_it() {
    const WIDTH: f32 = 6.0;
    const LENGTH: f32 = 14.0;
    const LIFT: f32 = 0.2;
    for (degrees, crest) in [(15.0f32, true), (20.0, true), (20.0, false)] {
        let grade = libm::tanf(degrees.to_radians());
        let fall = if crest { -grade } else { grade };
        let ground = move |_: f32, z: f32| fall * z.abs();
        let road = ribbon(&ground, WIDTH, LENGTH, &|_, _| LIFT);
        let top = move |x: f32, z: f32| ground(x, z) + LIFT;
        let shape = if crest { "crest" } else { "sag" };
        for family in [Family::Dog, Family::Horse] {
            let capsule = capsule_of(family);
            let record = beast_record(family);
            let end = capsule.to.max(-capsule.from) + capsule.radius;
            // Over the crest, or over where the sag's sides meet its ends.
            let rise = if crest { 0.0 } else { grade * end };
            for yaw in [0.0f32, std::f32::consts::FRAC_PI_2] {
                let place = Transform::from_xyz(0.0, LIFT + rise + drop_of(&record) + 0.05, 0.0)
                    .with_rotation(Quat::from_rotation_y(yaw));
                for under in [Under::Local, Under::Peer] {
                    let (app, _, root) = resting_on(
                        &ground,
                        &[],
                        std::slice::from_ref(&road),
                        under,
                        family,
                        place,
                    );
                    let label = format!("{family:?} {under:?} at {yaw} on a {degrees} deg {shape}");
                    let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
                    let drive = app.world().get::<Drive>(root).expect("a drive");
                    let (mut over, mut worst) = (0, 0.0f32);
                    for (world, got) in patch_samples(drive, &posed) {
                        if world.x.abs() > WIDTH * 0.5 - 0.1 || world.z.abs() > LENGTH * 0.5 - 0.6 {
                            continue;
                        }
                        over += 1;
                        let want = top(world.x, world.z);
                        worst = worst.max((got - want).abs());
                        assert!(
                            (got - want).abs() < 0.01,
                            "{label}: over the road at {world} the ground is {got:.3}, its top \
                             {want:.3}"
                        );
                    }
                    assert!(over > 40, "{label}: the road is not under the reach");
                    println!("{label}: {over} samples within {:.2} mm", worst * 1000.0);
                }
            }
        }
    }
}

/// **A beast on a narrow sloped beam follows its slope** (#1834 review). A
/// beam 0.2 m wide, sloped at 20 degrees along its length and two metres
/// over flat ground - more than a pelvis drop over it all along the reach,
/// so the ground is never joined to it - lies under one column of the
/// patch's samples and takes a dog and a horse resting on it facing along
/// it, on either client: the column is the beam's top, and every foot - over
/// the beam or beside it, over the drop - stands within a few centimetres of
/// the beam's top where it is. One column of samples is a line, and a plane
/// fitted through it was handed level across and along both, and the column
/// past the band held at it: the hooves at the root's level, as before #1834.
#[test]
fn a_beast_on_a_narrow_sloped_beam_follows_its_slope() {
    const HIGH: f32 = 2.0;
    const HALF: Vec3 = Vec3::new(0.1, 0.05, 4.0);
    let flat = |_: f32, _: f32| 0.0;
    let tilt = 20f32.to_radians();
    let grade = libm::tanf(tilt);
    // The beam's top along its centre line, rising toward -Z.
    let beam_top = move |z: f32| HIGH - grade * z;
    for family in [Family::Dog, Family::Horse] {
        let capsule = capsule_of(family);
        let record = beast_record(family);
        // Under the patch column nearest the body's centre line: the grid is
        // centred on the reach, a whole number of steps across.
        let rig = built(&default_beast(family), 64).rig;
        let reach = bevy_symbios_avatar::GroundPatch::reach(&rig).expect("a reach");
        let side = ((reach.size().max_element() / 0.25).ceil() + 1.0).max(2.0);
        let first = reach.center().x - (side - 1.0) * 0.25 * 0.5;
        let column = first + ((0.0 - first) / 0.25).round() * 0.25;
        // The rigged root's half turn mirrors the body's x into the world's.
        let x = -column;
        // The body over the centre line, the beam under the column.
        let beam = (
            HALF,
            Transform::from_xyz(x, HIGH - HALF.y / libm::cosf(tilt), 0.0)
                .with_rotation(Quat::from_rotation_x(tilt)),
            RigidBody::Static,
        );
        let front = capsule.to + capsule.radius;
        let place = Transform::from_xyz(0.0, beam_top(-front) + drop_of(&record) + 0.05, 0.0);
        for under in [Under::Local, Under::Peer] {
            let (app, _, root) = resting_on(&flat, &[beam], &[], under, family, place);
            let label = format!("{family:?} {under:?} on a 20 deg beam");
            assert_eq!(
                stands_by(&app, root).0,
                Some(RestsOn::Structure),
                "{label}: not on the beam"
            );
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let drive = app.world().get::<Drive>(root).expect("a drive");
            let mut on = 0;
            for (world, got) in patch_samples(drive, &posed) {
                if (world.x - x).abs() > 0.05 || world.z.abs() > HALF.z - 0.5 {
                    continue;
                }
                on += 1;
                let want = beam_top(world.z);
                assert!(
                    (got - want).abs() < 0.01,
                    "{label}: over the beam at {world} the ground is {got:.3}, its top {want:.3}"
                );
            }
            let nearest = patch_samples(drive, &posed)
                .into_iter()
                .map(|(world, _)| world.x - x)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()));
            assert!(
                on > 5,
                "{label}: no column over the beam at {x}, the root at {}, the nearest {nearest:?}",
                posed.translation()
            );
            let soles = soles_over(&app, root, &|_, z| beam_top(z));
            println!(
                "{label}: {on} samples on the beam, soles {:?} mm over its top",
                soles
                    .iter()
                    .map(|(_, off)| (off * 1000.0).round())
                    .collect::<Vec<_>>()
            );
            for (sole, off) in soles {
                assert!(
                    (-0.03..=0.03).contains(&off),
                    "{label}: a foot at {sole} stands {:+.1} mm off the beam's top",
                    off * 1000.0
                );
            }
        }
    }
}
