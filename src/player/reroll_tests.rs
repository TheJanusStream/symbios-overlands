//! #1898: a seed re-roll into a person, from a body of every family standing
//! on the ground, through the systems the game runs.
//!
//! The owner's report: "when re-rolling avatars, sometimes when rolling a
//! humanoid, it ends up too low, underground, when the previous avatar was
//! standing on the ground". The editor's re-roll writes
//! `AvatarRecord::default_for_seed` into [`LiveAvatarRecord`]
//! (`ui::avatar`), and the body follows the record one of two ways: a
//! change of locomotion kind is a rebuild
//! (`hotswap::detect_local_locomotion_change` flags it,
//! `hotswap::apply_local_locomotion_rebuild` strips the old preset and
//! builds the new one on the same entity), and a re-roll within the on-foot
//! preset - a beast, a winged body or another person into a person - is
//! `preset::sync_chassis` reshaping the collider in place. Both keep the
//! body where it is, so the new collider is centred where the old one was.
//!
//! The bench runs both paths as `PlayerPlugin` does: every preset's
//! fixed-step systems in its order, avian's step, then `Update`'s freeze and
//! hot-swap, over a floor built by the game's own terrain collider builder,
//! flat or on a slope.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::pds::avatar::AvatarRecord;
use crate::seeded_defaults::{AvatarCharacter, ChassisFamily};
use crate::state::LiveAvatarRecord;

/// The game's fixed step.
const HZ: f64 = 64.0;

/// The slope the sloped rows stand on (degrees): a hillside a person walks
/// up, and every vehicle parks on.
const SLOPE_DEG: f32 = 12.0;

/// How far from the ground a person's capsule may stand and be standing on
/// it (m): avian rests a body on its contacts' depth, a millimetre or so.
const STANDS_WITHIN_M: f32 = 0.01;

/// How deep a person's capsule may go into the ground on its way to standing
/// (m), at any step after the re-roll.
const NEVER_DEEPER_M: f32 = 0.02;

/// The ground's height at `x` on a floor tilted by `slope` radians about Z,
/// rising toward +X.
fn ground_height(slope: f32, x: f32) -> f32 {
    x * slope.tan()
}

/// One body wearing a record, standing on the bench's floor, run frame by
/// frame as the game runs it.
struct RerollBench {
    app: App,
    body: Entity,
    slope: f32,
}

impl RerollBench {
    /// A body wearing `record` set down on the floor at the origin as the
    /// spawn sets it down - at its set-down height, flush on the slope
    /// unless it lies level (`spawn::spawn_local_player`) - the floor tilted
    /// by `slope` radians.
    fn new(record: &AvatarRecord, slope: f32) -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .add_plugins(super::GamePhysics);
        // What the rebuild's visual respawn reaches.
        super::visuals::add_spawn_path(&mut app);
        app.insert_resource(Time::<Fixed>::from_hz(HZ))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(LiveAvatarRecord(record.clone()))
            .insert_resource(crate::water::WaterSurfaces { planes: Vec::new() })
            .init_resource::<super::humanoid::JumpQueued>()
            .init_resource::<super::RigHold>()
            .init_resource::<super::AttentionHeld>()
            .init_resource::<crate::diagnostics::MetricsRegistry>();
        // The game's heightmap of the floor, as the flight systems read it.
        let samples = 513;
        let half = (samples - 1) as f32 * 0.5;
        let mut map = bevy_symbios_ground::HeightMap::new(samples, samples, 1.0);
        for z in 0..samples {
            for x in 0..samples {
                map.set(x, z, ground_height(slope, x as f32 - half));
            }
        }
        app.insert_resource(crate::terrain::FinishedHeightMap(map, None));
        // THE REAL SYSTEMS, in the order `PlayerPlugin` chains them, gated
        // as it gates them - all but the egui focus gate, there being no
        // egui here, and the fall recovery, which would hide a fall.
        use super::{avatar_visuals_row_selected, guard_attention_held};
        app.add_systems(
            FixedUpdate,
            (
                super::preset::sync_chassis,
                super::hover_boat::apply_hover_boat_suspension,
                super::hover_boat::apply_hover_boat_buoyancy,
                super::helicopter::apply_helicopter_stabilization,
                super::airplane::apply_airplane_uprighting.run_if(not(avatar_visuals_row_selected)),
                super::hover_boat::apply_hover_boat_drive
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::hover_boat::apply_hover_boat_uprighting
                    .run_if(not(avatar_visuals_row_selected)),
                super::flight::read_flight_keys
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::flight::fly_on_foot,
                super::humanoid::end_lying_turns,
                super::humanoid::apply_humanoid_walk
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::humanoid::clear_jump_queue,
                super::airplane::read_airplane_keys
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::airplane::fly_airplane,
                super::helicopter::apply_helicopter_forces
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::car::apply_car_suspension,
                super::car::apply_car_drive
                    .run_if(not(avatar_visuals_row_selected))
                    .run_if(not(guard_attention_held)),
                super::car::apply_car_uprighting.run_if(not(avatar_visuals_row_selected)),
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                super::freeze_local_avatar_while_editing,
                (
                    super::hotswap::detect_local_locomotion_change,
                    super::hotswap::apply_local_locomotion_rebuild,
                    super::hotswap::rebuild_local_visuals,
                )
                    .chain(),
            ),
        );

        // The floor, built by the game's terrain collider builder: a
        // heightfield, which is what a body stands on in the game.
        let cells = 64;
        let cell = 8.0;
        let reach = cells as f32 * cell * 0.5;
        let mut floor = bevy_symbios_ground::HeightMap::new(cells + 1, cells + 1, cell);
        for z in 0..=cells {
            for x in 0..=cells {
                floor.set(x, z, ground_height(slope, x as f32 * cell - reach));
            }
        }
        app.world_mut().spawn((
            RigidBody::Static,
            bevy_symbios_ground::build_heightfield_collider(&floor),
            Transform::IDENTITY,
        ));
        let y = super::preset::set_down_height(&record.locomotion);
        let rotation =
            super::preset::set_down_rotation(&record.locomotion, Quat::from_rotation_z(slope));
        let body = app
            .world_mut()
            .spawn(super::spawn::chassis_root_bundle(
                Transform::from_xyz(0.0, y, 0.0).with_rotation(rotation),
            ))
            .id();
        let mut commands = app.world_mut().commands();
        super::preset::build_preset_components(&mut commands, body, &record.locomotion);
        app.world_mut().flush();
        app.finish();
        app.cleanup();
        Self { app, body, slope }
    }

    /// One frame of the game's: its fixed step - the systems, then avian's
    /// step - then `Update`, the freeze and the hot-swap.
    fn step(&mut self) {
        let dt = std::time::Duration::from_secs_f64(1.0 / HZ);
        let world = self.app.world_mut();
        world.resource_mut::<Time<Fixed>>().advance_by(dt);
        let fixed = *world.resource::<Time<Fixed>>();
        *world.resource_mut::<Time>() = fixed.as_generic();
        world.run_schedule(FixedUpdate);
        world.run_schedule(FixedPostUpdate);
        world.run_schedule(Update);
    }

    fn run(&mut self, secs: f64) {
        for _ in 0..(secs * HZ).round() as usize {
            self.step();
        }
    }

    /// Aim the Avatar editor's gizmo at a visuals row, or let it go: what
    /// holds the body frozen.
    fn hold(&mut self, still: bool) {
        *self.app.world_mut().resource_mut::<super::RigHold>() = super::RigHold {
            still,
            visuals_row: still,
            ..default()
        };
    }

    /// The editor's re-roll: the record rewritten in place.
    fn reroll(&mut self, record: &AvatarRecord) {
        self.app.world_mut().resource_mut::<LiveAvatarRecord>().0 = record.clone();
    }

    fn sleeping(&self) -> bool {
        self.app.world().entity(self.body).contains::<Sleeping>()
    }

    fn position(&self) -> Vec3 {
        self.app.world().get::<Position>(self.body).unwrap().0
    }

    fn rotation(&self) -> Quat {
        self.app.world().get::<Rotation>(self.body).unwrap().0
    }

    /// How far the person wearing `record` stands off the ground (m),
    /// negative in it: the gap between its capsule and the ground's plane -
    /// the capsule's centre's distance from the plane, less how far the
    /// capsule reaches toward it as it is turned. Its feet: the rigged body
    /// hangs with its soles at the capsule's bottom (`player::rigged`).
    fn gap(&self, record: &AvatarRecord) -> f32 {
        let LocomotionConfig::Humanoid(p) = &record.locomotion else {
            unreachable!("a person")
        };
        let at = self.position();
        let up = Vec3::new(-self.slope.sin(), self.slope.cos(), 0.0);
        let axis = self.rotation() * Vec3::Y;
        let reach = p.capsule_radius.0 + 0.5 * p.capsule_length.0 * up.dot(axis).abs();
        (at.y - ground_height(self.slope, at.x)) * self.slope.cos() - reach
    }
}

/// The `nth` seed the re-roll draws a body of `family` from.
fn seed_of(family: ChassisFamily, nth: usize) -> u64 {
    (0u64..)
        .filter(|&s| AvatarCharacter::for_seed(s).chassis == family)
        .nth(nth)
        .expect("a seed of every family")
}

fn stature(record: &AvatarRecord) -> f32 {
    match &record.locomotion {
        LocomotionConfig::Humanoid(p) => p.total_height(),
        _ => unreachable!("a person"),
    }
}

/// The shortest and the tallest of the first seeded people.
fn people() -> (AvatarRecord, AvatarRecord) {
    let mut all: Vec<AvatarRecord> = (0..24)
        .map(|n| AvatarRecord::default_for_seed(seed_of(ChassisFamily::Humanoid, n)))
        .collect();
    all.sort_by(|a, b| stature(a).total_cmp(&stature(b)));
    let (short, tall) = (all[0].clone(), all[all.len() - 1].clone());
    assert!(
        stature(&tall) - stature(&short) > 0.3,
        "the fixture: the seeded people no longer span statures worth testing"
    );
    (short, tall)
}

/// What one re-roll came to.
struct Outcome {
    /// The body was asleep when it was re-rolled.
    slept: bool,
    /// How far off the ground the person stands three seconds on (m).
    gap: f32,
    /// The deepest it went into the ground on the way (m).
    worst: f32,
    /// The highest it stood off the ground on the way (m): a drop from a
    /// vehicle's ride height, or a launch.
    highest: f32,
}

/// Stand `from` on the floor - a second's settle, then, for `asleep`, until
/// avian puts it to sleep - re-roll it into `to`, under the editor's freeze
/// when `frozen` (held a quarter second either side of the re-roll, then let
/// go), and watch three seconds.
fn re_roll(
    from: &AvatarRecord,
    to: &AvatarRecord,
    slope: f32,
    asleep: bool,
    frozen: bool,
) -> Outcome {
    let mut bench = RerollBench::new(from, slope);
    bench.run(1.0);
    if asleep {
        let mut waited = 1.0;
        while !bench.sleeping() && waited < 8.0 {
            bench.run(0.25);
            waited += 0.25;
        }
    }
    let slept = bench.sleeping();
    if frozen {
        bench.hold(true);
        bench.run(0.25);
    }
    bench.reroll(to);
    if frozen {
        bench.run(0.25);
        bench.hold(false);
    }
    let (mut worst, mut highest) = (f32::INFINITY, f32::NEG_INFINITY);
    for _ in 0..192 {
        bench.step();
        let gap = bench.gap(to);
        worst = worst.min(gap);
        highest = highest.max(gap);
    }
    Outcome {
        slept,
        gap: bench.gap(to),
        worst,
        highest,
    }
}

/// A body a person is re-rolled from, named, and the people it is re-rolled
/// into, by their size.
type Row = (String, AvatarRecord, Vec<(&'static str, AvatarRecord)>);

/// The bodies a person is re-rolled from: one of each seeded family, and a
/// short and a tall person, each with the people it is re-rolled into.
fn rows() -> Vec<Row> {
    let (short, tall) = people();
    let both = vec![("short", short.clone()), ("tall", tall.clone())];
    let mut rows: Vec<_> = [
        ("skiff", ChassisFamily::Skiff),
        ("boat", ChassisFamily::Boat),
        ("airship", ChassisFamily::Airship),
        ("helicopter", ChassisFamily::Helicopter),
        ("airplane", ChassisFamily::Airplane),
        ("beast", ChassisFamily::Beast),
        ("winged", ChassisFamily::Winged),
    ]
    .into_iter()
    .map(|(name, family)| {
        (
            name.to_string(),
            AvatarRecord::default_for_seed(seed_of(family, 0)),
            both.clone(),
        )
    })
    .collect();
    rows.push((
        "short person".into(),
        short.clone(),
        vec![("tall", tall.clone())],
    ));
    rows.push(("tall person".into(), tall, vec![("short", short)]));
    rows
}

/// Every row, on flat ground and on the slope, awake and asleep, `frozen`
/// or not: a table of what each came to, and the rows that did not stand.
fn sweep(frozen: bool) -> Vec<String> {
    let mut failed = Vec::new();
    for (name, from, into) in rows() {
        for (size, to) in &into {
            for slope in [0.0, SLOPE_DEG.to_radians()] {
                for asleep in [false, true] {
                    let out = re_roll(&from, to, slope, asleep, frozen);
                    let stands = out.gap.abs() < STANDS_WITHIN_M && out.worst > -NEVER_DEEPER_M;
                    let row = format!(
                        "{name} -> {size} person, {} ground, {}{}{}: stands {:+.1} mm off the \
                         ground, at worst {:+.1} mm, at highest {:+.1} mm",
                        if slope == 0.0 { "flat" } else { "sloped" },
                        if asleep { "left to sleep" } else { "awake" },
                        if asleep && !out.slept {
                            " (never slept)"
                        } else {
                            ""
                        },
                        if frozen { ", frozen" } else { "" },
                        out.gap * 1e3,
                        out.worst * 1e3,
                        out.highest * 1e3,
                    );
                    eprintln!("{} {row}", if stands { "  ok  " } else { " FAIL " });
                    if !stands {
                        failed.push(row);
                    }
                }
            }
        }
    }
    failed
}

/// #1898: a person re-rolled from a body of any family standing on the
/// ground - flat or sloped, awake or asleep - stands on the ground, and never
/// goes into it on the way. Its new collider starts centred where the old
/// one was; where it reaches lower its bottom started in the terrain, and a
/// capsule whose core runs through a heightfield's surface makes no contact
/// with it: a person rolled from a helicopter fell out of the world, and one
/// rolled from a beast or a winged body on a slope stood buried to the
/// chest.
#[test]
fn a_person_re_rolled_from_any_body_stands_on_the_ground() {
    let failed = sweep(false);
    assert!(
        failed.is_empty(),
        "re-rolled into a person, {} rows did not stand on the ground:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// [`a_person_re_rolled_from_any_body_stands_on_the_ground`] with the
/// re-roll made under the Avatar editor's freeze (#1894): the body rebuilt
/// or reshaped while disabled, and let go a quarter second on.
#[test]
fn a_person_re_rolled_under_the_freeze_stands_on_the_ground() {
    let failed = sweep(true);
    assert!(
        failed.is_empty(),
        "re-rolled into a person under the freeze, {} rows did not stand on the ground:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
