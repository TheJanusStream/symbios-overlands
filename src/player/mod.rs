//! Local player plugin: spawns and drives the local avatar, hot-swaps
//! locomotion presets when the owner edits their PDS avatar record, and
//! paints matching visuals on remote peers.
//!
//! An [`AvatarRecord`](crate::pds::AvatarRecord) has two halves, and the
//! first of them is a `$type`-tagged open union rather than one shape
//! (`body: AvatarBody`, since #1056):
//!
//! - **`Rigged`** - a parametric `symbios-avatar` body, referenced by rkey
//!   into the identity's wardrobe and built by [`rigged`]. Every humanoid is
//!   one of these since #1060. It never goes near
//!   [`visuals::spawn_avatar_visuals`].
//! - **`Generator`** - a generator tree spawned by
//!   [`visuals::spawn_avatar_visuals`] (no colliders, no per-prim markers -
//!   pure cosmetics). Vehicles, and the pre-#1060 seeded humanoids.
//! - **`Absent`** (a pre-#1056 record: treated as "no record", so the seeded
//!   default is synthesised) and **`Unknown`** (a body kind this build does
//!   not know: rendered as a bare chassis, never re-serialized).
//!
//! The `locomotion` half is independent of which body kind is worn, and
//! selects one of five physics presets:
//!
//! - **HoverBoat** - `RigidBody::Dynamic` cuboid chassis with four
//!   raycast-suspension corners + buoyancy + WASD drive (Hooke's-law
//!   spring, lateral grip, jump impulse).
//! - **Humanoid** - "On foot": a capsule rigid body with `LockedAxes`
//!   keeping it upright, velocity-driven walk controller, jump impulse,
//!   swim/wading modes triggered by water depth. A person stands up in the
//!   capsule; a four-legged beast lies along it, turned by the physics about
//!   Y (#1802).
//! - **Airplane** - cuboid fuselage on three-wheel landing gear, a throttle
//!   that holds, lift along the wings that stalls, and rate control that
//!   levels the wings and the flight path when let go (#1615).
//! - **Helicopter** - cuboid fuselage, auto-stabilising hover thrust,
//!   cyclic + strafe + yaw input, vertical climb/descend on Space/Shift.
//! - **Car** - cuboid chassis, four-corner raycast suspension with a bump
//!   stop, drive + steering + handbrake through the wheels on the ground and
//!   an air model (pitch / yaw / roll control, self-levelling, air damping)
//!   off it, no buoyancy.
//!
//! All five read their tuning from the live
//! [`LiveAvatarRecord`](crate::state::LiveAvatarRecord), so UI
//! edits take effect the same frame the slider moves. Changing the
//! locomotion *variant* triggers the hot-swap system, which tears down
//! all preset-specific components (collider, markers, locked axes) and
//! rebuilds them in the new preset's shape without disturbing the parent
//! `Transform` or rigid-body identity.
//!
//! ## Sub-module map
//!
//! * [`spawn`] - `OnEnter(InGame)` local-avatar spawn + the chassis root
//!   bundle (#670 easing guard).
//! * [`preset`] - per-preset physics components: the `PresetComponents`
//!   trait (one impl per locomotion `*Params`), preset markers, and the
//!   build/strip pair.
//! * [`hotswap`] - locomotion-variant rebuild, visuals repaint,
//!   remote-peer mirroring, and the terrain-hot-load lift.
//! * [`respawn`] - fall-through recovery.
//! * [`visuals`] - generator-tree visual spawner (`spawn_avatar_visuals`).
//! * [`gait`] - cosmetic bounce / sway / look-around animation on the
//!   humanoid visual root, driven by the seeded `AvatarGait`.
//! * [`hover_boat`] - HoverBoat preset: suspension / buoyancy / drive /
//!   uprighting systems.
//! * [`humanoid`] - Humanoid preset: walk controller (dry/wading/swim
//!   modes) and the `humanoid_water_state` classifier.
//! * [`airplane`] - Airplane preset: the flight model and the landing gear.
//! * [`helicopter`] - Helicopter preset: auto-stabilised hover + cyclic.
//! * [`car`] - Car preset: suspension + bump stop, ground drive + steering +
//!   handbrake, the air model, uprighting.
//! * [`portal`] - `handle_portal_interaction`,
//!   `poll_portal_travel_tasks`, and the `PortalTravelTask` async job.
//!   `begin_portal_travel` / `PortalCooldown` are re-exported for the
//!   unsaved-edits guard in [`crate::ui::unsaved_guard`], which owns the
//!   confirm step between portal contact and the actual travel fetch.
//! * `sim` (tests only) - benches that run one body under the game's own
//!   flight systems (#1430) or car systems (#1524) and avian, stepped by
//!   hand.

#[cfg(test)]
mod air_fleet;
mod airplane;
pub(crate) mod attachments;
mod beast_seat;
mod car;
pub(crate) mod emote;
pub(crate) mod flight;
pub(crate) mod gait;
mod helicopter;
mod hotswap;
mod hover_boat;
pub mod humanoid;
mod portal;
mod preset;
#[cfg(test)]
mod reroll_tests;
mod respawn;
mod rigged;
#[cfg(test)]
pub(crate) mod sim;
mod spawn;
pub mod visuals;
mod winged_seat;

// The seeded airplane family sizes her gear by it (#1617).
pub(crate) use airplane::reach_for_stance as airplane_reach_for_stance;
// Read only by the agent's `drive` (#1527) and its airplane pilot (#1615),
// which are Unix-only like the rest of the agent: ungated, these are unused
// imports on wasm32.
#[cfg(unix)]
pub(crate) use airplane::Handling as AirplaneHandling;
#[cfg(unix)]
pub(crate) use car::CarContact;
pub(crate) use flight::{Flight, FlightMode, PeerFlight};
pub(crate) use hotswap::AppliedAvatar;
pub(crate) use portal::PORTAL_COOLDOWN_SECS;
pub use portal::PortalContact;
pub use portal::PortalCooldown;
pub use portal::PortalTravelTask;
pub(crate) use portal::begin_portal_travel;
pub use preset::{
    AirplanePreset, CarPreset, HelicopterPreset, HoverBoatPreset, HumanoidPreset, VehicleChassis,
};
pub(crate) use respawn::LandingWait;
pub use respawn::{FacingSet, PlayerMove, PlayerMoveRequest, go_to_pose, return_to_spawn_blocked};
pub(crate) use rigged::InFirstPerson;
pub use rigged::RiggedBuildFailed;
pub(crate) use rigged::RiggedRoot;
pub(crate) use rigged::StandIn;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use rigged::heading_of;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use rigged::{RestsOn, Underfoot};
// On every target: the seeder turns a world's signature hour to its
// landing's facing (#1769), and seeds derive in the browser too.
pub(crate) use spawn::spawn_facing;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::input::egui_wants_any_keyboard_input;

use crate::config::rover as cfg;
use crate::state::{AppState, LocalPlayer};

/// Whether a modal dialog owns the user's attention, so the player must
/// not drive (#852, widened by #1241 f164, inverted out of `ui` by #1297
/// group 3).
///
/// egui modals block the pointer but NOT game keys, so without this the
/// player could WASD away from a portal mid-"Unpublished changes"
/// decision and "Publish & travel" would fire from wherever they had
/// drifted. The five input-driven drive systems (and the jump latch) gate
/// on `not(guard_attention_held)`; passive physics (suspension,
/// stabilisation, gravity) keeps running, same policy as the egui-focus
/// gate.
///
/// Written once per frame by `ui::confirm::mirror_attention_held` in
/// `PreUpdate`, so the FixedUpdate steps later in the same frame read
/// this frame's answer. The `RigHold` shape of #1158 a fourth time:
/// `ui` publishes the fact, the player owns the resource, and neither
/// half has to import the other's module.
///
/// **Default false on purpose.** A frame with no mirror reports "nothing
/// is holding attention" and the player can move. The opposite default
/// would freeze anyone whose mirror had gone unregistered - a failure
/// `ui::tests::the_mirrored_consumers_do_not_import_the_ui_layer` catches
/// at test time precisely because it is invisible at run time.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttentionHeld(pub bool);

/// The predicate [`AttentionHeld`] carries: a modal is up, in either of
/// the two forms this app has one.
///
/// The two are not interchangeable and that is the whole history of this
/// gate. `confirm::note_modal_open` lives in egui's per-context store,
/// which only a system holding an egui context can read;
/// `ui::unsaved_guard::UnsavedGuard` is an ECS resource that leaves no
/// stamp at all. The gate asked about the guard alone and therefore knew
/// about one of six modals - a gift offer from a stranger blocked every
/// click while W kept walking the avatar into a portal, stacking a second
/// modal behind the first.
///
/// Pure, and shared by the mirror that writes the resource and the test
/// that asserts it, so the two read one sentence rather than two copies
/// of an `||`.
pub(crate) fn attention_is_held(modal_open: bool, guard_present: bool) -> bool {
    modal_open || guard_present
}

/// Run condition: something owns attention, so the drive systems stand
/// down. See [`AttentionHeld`].
pub(crate) fn guard_attention_held(held: Res<AttentionHeld>) -> bool {
    held.0
}

// Corner offsets in local space for the four suspension rays. The
// hover-boat and car presets share the same four-corner pattern; their
// chassis half-extents differ but the rig topology does not.
pub(super) const CORNER_OFFSETS_RAW: [[f32; 3]; 4] = [
    [1.0, -1.0, 1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, -1.0, -1.0],
];

/// Multiply the canonical `CORNER_OFFSETS_RAW` by the preset's chassis
/// half-extents to get the four world-local suspension-ray origins. Both
/// [`hover_boat`] and [`car`] share this helper because the suspension
/// math is identical - only the chassis size differs.
pub(super) fn chassis_corners(half_extents: Vec3) -> [Vec3; 4] {
    CORNER_OFFSETS_RAW.map(|raw| Vec3::new(raw[0], raw[1], raw[2]) * half_extents)
}

/// Build the ground-detection ray filter shared by the vehicle suspension
/// casts and the humanoid jump-grounding check, excluding the caster's own
/// `chassis` plus every `Sensor` collider (pass `sensors.iter()`).
///
/// Sensors - the gateway veil ([`GatewayMarker`](crate::world_builder::GatewayMarker))
/// and portal cubes - are phantom walk-in volumes: `Sensor` exempts them
/// from contact-force resolution, but avian's `cast_ray` still reports them
/// as hits. Left in the ground ray, a gateway box reads as ground and the
/// suspension spring drives the vehicle up its surface instead of letting it
/// pass through into the zone (#813). Excluding all sensors keeps the
/// invariant that ground rays only ever see solid ground, with no per-prim
/// tagging or collision-layer scheme to maintain.
pub(super) fn ground_ray_filter(
    chassis: Entity,
    sensors: impl IntoIterator<Item = Entity>,
) -> SpatialQueryFilter {
    SpatialQueryFilter::default().with_excluded_entities(std::iter::once(chassis).chain(sensors))
}

/// The least a contact's normal may lean from straight up for what it
/// touches to be UNDER a body (the cosine of 60 degrees): a wall beside it or
/// a roof over it is not what it rests on. Read by [`resting_on_something`]
/// for the agent, and by the airplane's gear and righting assist (#1626).
pub(crate) const UNDER_NORMAL_Y: f32 = 0.5;

/// Whether the physics has `body` resting on something under it: a contact
/// whose normal, turned to point out of the thing touched, leans less than
/// 60 degrees from straight up ([`UNDER_NORMAL_Y`]), with a point within
/// [`BODY_TOUCH_M`](cfg::BODY_TOUCH_M) of it, against anything but a sensor.
/// Avian counts a pair as touching from as far off as the body could close
/// in one step, 23 cm at 15 m/s, and a body flying that low over the ground
/// is not on it (#1528).
///
/// What the agent's sensing of its own height reads (#1430); a landed
/// rotorcraft (#1612) asks [`resting_on`] the same, on flatter ground.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn resting_on_something(
    contacts: &ContactGraph,
    sensors: &Query<Entity, With<Sensor>>,
    body: Entity,
) -> bool {
    resting_on(contacts, sensors, body, UNDER_NORMAL_Y)
}

/// Whether the physics has `body` resting on something under it, as
/// [`resting_on_something`] reads it, on nothing steeper than `min_normal_y`:
/// the cosine of the steepest slope that counts.
pub(crate) fn resting_on(
    contacts: &ContactGraph,
    sensors: &Query<Entity, With<Sensor>>,
    body: Entity,
    min_normal_y: f32,
) -> bool {
    ground_under(contacts, sensors, body, min_normal_y).is_some()
}

/// What [`resting_on`] finds `body` resting on, by its normal turned to
/// point out of it - up out of the ground under the body: the first such
/// contact's.
pub(crate) fn ground_under(
    contacts: &ContactGraph,
    sensors: &Query<Entity, With<Sensor>>,
    body: Entity,
    min_normal_y: f32,
) -> Option<Vec3> {
    resting_under(contacts, sensors, body, min_normal_y)
        .next()
        .map(|(_, normal)| normal)
}

/// Everything the physics has `body` resting on, as [`ground_under`] reads
/// one thing: each solid it touches under it, with that contact's normal
/// turned to point out of it, in the contact graph's order.
///
/// A beast is handed the terrain's own ground only while the terrain is all
/// it rests on (#1833), which is a question about every thing under it
/// rather than the first.
pub(crate) fn resting_under<'a>(
    contacts: &'a ContactGraph,
    sensors: &'a Query<Entity, With<Sensor>>,
    body: Entity,
    min_normal_y: f32,
) -> impl Iterator<Item = (Entity, Vec3)> + 'a {
    contacts
        .contact_pairs_with(body)
        .filter(|pair| pair.is_touching())
        .filter_map(move |pair| {
            let (other, out_of_other) = if pair.collider1 == body {
                (pair.collider2, -1.0)
            } else {
                (pair.collider1, 1.0)
            };
            if sensors.contains(other) {
                return None;
            }
            // A manifold's normal points from the first collider to the
            // second: turned to point out of the other, it points up out of
            // a thing under the body.
            pair.manifolds.iter().find_map(|manifold| {
                let normal = manifold.normal * out_of_other;
                (normal.y >= min_normal_y
                    && manifold
                        .points
                        .iter()
                        .any(|point| point.penetration > -cfg::BODY_TOUCH_M))
                .then_some((other, normal))
            })
        })
}

/// The pose a vehicle's fixed-step systems read their chassis at (#1548):
/// avian's own `Position` and `Rotation`, as its solver left them at the end
/// of the last step. Not `GlobalTransform`: in `FixedUpdate` that is the pose
/// the last frame left - set by avian only before its solve, and eased by
/// `PostUpdate` from the interpolated `Transform` - so a step read the body a
/// whole step late or part of one, depending on how many steps the frame held
/// and where it fell on the step grid. A car at 12.5 frames a second drove off
/// its run at 64 within five steps, and two players on different displays flew
/// the same jump differently.
/// Taken as avian's `Forces` hands them over: since avian 0.8 its
/// `position()` and `rotation()` are the plain vector and quaternion.
pub(super) fn physics_pose(position: Vec3, rotation: Quat) -> GlobalTransform {
    GlobalTransform::from(Transform::from_translation(position).with_rotation(rotation))
}

/// Steering-direction multiplier from a vehicle's signed longitudinal speed:
/// `-1` while genuinely reversing (below `-REVERSE_STEER_SPEED`), else `+1`.
/// Both [`car`] and [`hover_boat`] multiply their A/D yaw torque by it so the
/// heading response inverts in reverse - with the wheels/rudder held one way a
/// real vehicle turns the opposite way backing up - while the deadband keeps
/// the forward sign (and so turn-in-place) around a standstill, so the sign
/// doesn't flip on sub-m/s creep.
pub(super) fn reverse_steer_sign(forward_speed: f32) -> f32 {
    if forward_speed < -cfg::REVERSE_STEER_SPEED {
        -1.0
    } else {
        1.0
    }
}

/// Draw an (x, z) pair uniformly distributed inside a square of
/// `SPAWN_SCATTER_SIZE` metres per side, centred on the origin.
pub(super) fn random_spawn_xz() -> (f32, f32) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let s = SEED.fetch_add(0xDA94_2042_E4DD_58B5, Ordering::Relaxed);
    let mut z = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let u = (z as u32 as f32) / (u32::MAX as f32);
    let v = ((z >> 32) as u32 as f32) / (u32::MAX as f32);
    let side = cfg::SPAWN_SCATTER_SIZE;
    ((u - 0.5) * side, (v - 0.5) * side)
}

/// A car driven in an app that is not the game - the render tool's
/// `--driver` (#1546): the game's own fixed-step car systems, chained as
/// [`PlayerPlugin`] chains them, without the gates an owner typing into a
/// chat field needs. Nobody types into a headless render.
///
/// With them, the cosmetic layer the game lays over a car's body: its lean
/// into a turn, its bob and its idle shiver (`gait::animate_avatar_gait`),
/// run in `Update` as `PlayerPlugin` runs it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn register_headless_car(app: &mut App) {
    app.init_resource::<RigHold>()
        .add_systems(
            FixedUpdate,
            (
                preset::sync_chassis,
                car::apply_car_suspension,
                car::apply_car_drive,
                car::apply_car_uprighting,
            )
                .chain()
                .in_set(HeadlessCarSystems),
        )
        .add_systems(Update, gait::animate_avatar_gait);
}

/// The set [`register_headless_car`] puts the car's systems in, so a caller
/// can press its keys before them.
#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct HeadlessCarSystems;

/// The chassis of a car driven in an app that is not the game (#1546): the
/// local player's root bundle and the physics of `locomotion`'s preset, as
/// `spawn::spawn_local_player` assembles them, at `at`, and - for a car - the
/// gait the game attaches to a signed-in player's own car
/// (`gait::attach_gait_animation`), seeded by `did`. The caller hangs the
/// body's visuals under it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_headless_chassis(
    commands: &mut Commands,
    at: Transform,
    locomotion: &crate::pds::LocomotionConfig,
    did: &str,
) -> Entity {
    let entity = commands.spawn(spawn::chassis_root_bundle(at)).id();
    preset::build_preset_components(commands, entity, locomotion);
    if matches!(locomotion, crate::pds::LocomotionConfig::Car(_)) {
        commands
            .entity(entity)
            .insert(gait::GaitAnimation::for_did(did, gait::GaitMode::Skiff));
    }
    entity
}

/// Avian, as every world here runs it: its plugins with contact recycling
/// off, which is how the narrow phase worked up to avian 0.7.
///
/// Avian 0.8 recycles a pair's contact points from the step before while
/// the two bodies have moved less than `NarrowPhaseConfig::recycle_distance`
/// (5 cm) and turned less than `recycle_angle` relative to each other,
/// updating only each point's depth. Every body here was tuned on contacts
/// measured afresh each step, and two kinds of motion fall through the
/// recycler's test: a lying beast's capsule turning beside a wall, its ends
/// sweeping while its centre stays put, pressed into the wall on recycled
/// points - the horse 37 mm, the dog 0.8 - where measured afresh it stops at
/// under a thousandth of a millimetre; and a body standing still whose
/// collider changes, a person re-rolled taller in place, whose recycled
/// contacts were the old capsule's, so it stood half a metre into the ground
/// for as long as it stood still.
///
/// Use it wherever avian is stood up, the game's tests and tools included,
/// so what they measure is the physics the game runs.
pub struct GamePhysics;

impl Plugin for GamePhysics {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default())
            .insert_resource(NarrowPhaseConfig {
                recycle_distance: 0.0,
                ..default()
            });
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        // The sibling crate's frame order (Build → Animate → Apply), its
        // pose-apply system and, since #1171, the per-body motion driver
        // itself. Its `AnimatorPlugin` is still deliberately absent, though
        // the reason changed under #1309: it is no longer a second driver -
        // at 0.6.0 it steers the same `AvatarDriver` this file does - but a
        // control SURFACE, one set of switches and an egui panel deciding
        // what every body it owns is doing. That is a viewer's shape. Here
        // each body is told what to do by its own chassis, which is the whole
        // of what `fill_rigged_drive` says. The two cannot collide in any
        // case: the window only steers bodies with no `Drive`, and every body
        // this file installs carries one. There is no clip library to own
        // either (#1067): every motion source is the engine's procedural
        // layer, so nothing is fetched, embedded or indexed before a body can
        // move.
        app.add_plugins(bevy_symbios_avatar::AvatarPlugin)
            // AFTER the plugin, which `init_resource`s its own 12 m default:
            // `init_resource` keeps whatever is already there, so inserting
            // this first would be silently thrown away. The two constants are
            // this lens's, not the adapter's - see
            // `crate::config::camera::HAIR_SWITCH` for the pixel arithmetic
            // and `HAIR_MARGIN` for why a band is a WebGL2 crash rather than
            // a taste (#1358). (`cfg` in this file is the rover block.)
            .insert_resource(bevy_symbios_avatar::HairLod {
                switch: crate::config::camera::HAIR_SWITCH,
                margin: crate::config::camera::HAIR_MARGIN,
            })
            .add_systems(
                Update,
                (
                    rigged::kick_rigged_builds,
                    rigged::land_rigged_builds,
                    // #1255, after the land so a body installed this frame
                    // retires the stand-in in the same frame's commands:
                    // the rigged build is the one wait that escapes the
                    // loading screen, and until it lands the owner used to
                    // be nothing at all.
                    rigged::announce_slow_builds,
                    rigged::sync_local_placeholder,
                    attachments::sync_rigged_attachments,
                )
                    .chain()
                    .in_set(bevy_symbios_avatar::AvatarSystems::Build)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_message::<emote::EmoteRequest>()
            .add_systems(
                Update,
                // The fill and the sibling crate's driver are one unit
                // (#1171): `fill_rigged_drive` writes what the chassis is
                // doing and `drive_avatar_bodies` turns it into a pose, so
                // the ordering is a correctness requirement rather than a
                // preference. `start_emotes` leads, so an emote requested
                // this frame is posed this frame rather than a frame after
                // the message that asked for it; the strain count trails,
                // because `Drove` only exists once a body has been driven.
                (
                    (rigged::start_emotes, rigged::fill_rigged_drive)
                        .chain()
                        .before(bevy_symbios_avatar::drive_avatar_bodies),
                    rigged::count_motion_strain.after(bevy_symbios_avatar::drive_avatar_bodies),
                )
                    .in_set(bevy_symbios_avatar::AvatarSystems::Animate)
                    .run_if(in_state(AppState::InGame)),
            );
        app.add_systems(OnEnter(AppState::InGame), spawn::spawn_local_player)
            .add_systems(
                Update,
                (
                    hotswap::detect_local_locomotion_change,
                    hotswap::apply_local_locomotion_rebuild,
                    hotswap::detect_remote_change,
                    hotswap::rebuild_local_visuals,
                    hotswap::lift_player_above_new_ground,
                    gait::attach_gait_animation,
                    gait::animate_avatar_gait,
                    portal::handle_portal_interaction,
                    portal::poll_portal_travel_tasks,
                    // After the poll, so the frame the record lands cannot
                    // also release the gate it just closed (#1231 f20):
                    // `WorldCompiled` is removed by that system and the
                    // command applies at the sync point, which is after
                    // this one has run.
                    portal::release_travel_on_arrival,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    preset::sync_chassis,
                    hover_boat::apply_hover_boat_suspension,
                    hover_boat::apply_hover_boat_buoyancy,
                    // Passive stabilisation runs UNGATED, exactly like
                    // suspension/buoyancy above: helicopter hover and
                    // self-righting are not input responses, so egui focus
                    // must not cut them (#821 - the airship used to fall
                    // out of the sky whenever a text field grabbed the
                    // keyboard). The airplane's flight model is ungated too,
                    // below, after her keys are read.
                    helicopter::apply_helicopter_stabilization,
                    // The fourth preset's righting assist (#1240 f162),
                    // ungated like the car's and the hover-boat's: a
                    // flipped aircraft keeps righting even while the owner
                    // types in a chat field.
                    airplane::apply_airplane_uprighting.run_if(not(avatar_visuals_row_selected)),
                    // Disable keyboard-driven control systems while the
                    // owner is typing in an egui text field - otherwise
                    // WASD-heavy chat messages steer the vehicle through
                    // walls. Physics (suspension, buoyancy, gravity,
                    // hover/lift stabilisation) and the uprighting /
                    // respawn passes still run so a vehicle left mid-air
                    // keeps obeying gravity - and an airship keeps
                    // hovering.
                    hover_boat::apply_hover_boat_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    hover_boat::apply_hover_boat_uprighting
                        .run_if(not(avatar_visuals_row_selected)),
                    // Ungated + chained right before the walk system: a
                    // beast's turn lasts the step the walk set it for, and a
                    // step the walk is stood down for turns nothing (#1802).
                    // A winged body's flight (#1861), before the walk: its
                    // keys gated as the walk's are, the flight itself not, so
                    // a body typing in the air glides on.
                    flight::read_flight_keys
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    flight::fly_on_foot,
                    humanoid::end_lying_turns,
                    humanoid::apply_humanoid_walk
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    // Ungated + chained right after the walk system: a
                    // queued jump tap must die with the first fixed step
                    // that could have acted on it, even when the walk
                    // system itself was gated off (#852).
                    humanoid::clear_jump_queue,
                    airplane::read_airplane_keys
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    // The airplane's flight model (#1615) runs UNGATED
                    // straight after her keys are read: the held throttle,
                    // the air, the gear and the levelling are hers, not the
                    // player's, and a step the keys stood down on is a step
                    // with none.
                    airplane::fly_airplane,
                    helicopter::apply_helicopter_forces
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    car::apply_car_suspension,
                    car::apply_car_drive
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                    car::apply_car_uprighting.run_if(not(avatar_visuals_row_selected)),
                    respawn::respawn_if_fallen,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .init_resource::<RigHold>()
            .init_resource::<AttentionHeld>()
            .init_resource::<humanoid::JumpQueued>()
            .init_resource::<respawn::PlayerMoveRequest>()
            .add_message::<respawn::FacingSet>()
            .init_resource::<crate::player::LocalMovement>()
            .add_systems(
                Update,
                (
                    freeze_local_avatar_while_editing,
                    // The owner's unstuck command (#1240 f159). `Update`,
                    // not `FixedUpdate`: it is a one-shot position write
                    // answering a click, not part of the physics step.
                    respawn::apply_player_move,
                    // The swim/wade classification, for the mode banner
                    // (#1241 f160). UNGATED on egui focus, unlike the
                    // drive systems it mirrors - see its own doc.
                    humanoid::publish_movement_facts,
                    // Same gates as `apply_humanoid_walk`, so a space typed
                    // into chat (or pressed under a guard modal) never
                    // queues a jump for the moment focus returns (#852).
                    humanoid::latch_jump_input
                        .run_if(not(egui_wants_any_keyboard_input))
                        .run_if(not(avatar_visuals_row_selected))
                        .run_if(not(guard_attention_held)),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Facts about how the local avatar is currently moving that the movement
/// code knows and no UI could see (#1241 f160, f168).
///
/// `WaterState` was referenced outside `player::humanoid` only by
/// `player::rigged::motion` and never by `src/ui` at all, so the key remap
/// it drives had no surface anywhere; the derived walk speed existed only
/// as a local inside the drive system, so the editor could not tell the
/// owner that their Run slider had gone below it.
///
/// Written by [`humanoid::publish_movement_facts`], which is deliberately
/// NOT the drive system: the drive systems stand down while an egui text
/// field has focus, and a banner that vanished whenever the player clicked
/// into chat would be worse than none.
///
/// Lives in `player` rather than `ui::modes` (#1158): these are facts
/// about how the local body is moving, produced here and merely
/// DISPLAYED by the mode banner and the locomotion editor. It already
/// carried `player::humanoid::WaterState`, so the type pointed this way
/// before the module did. The mirror image of [`RigHold`], which the
/// editor writes and this module reads.
///
// the player clicked into chat would be worse than none.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct LocalMovement {
    /// Dry / wading / swimming, from `humanoid_water_state`.
    pub water: humanoid::WaterState,
    /// The unshifted walk this body actually walks at (m/s), derived from
    /// the built rig - `None` until the rigged body lands. Read by the
    /// locomotion editor so the Run slider can say when it has been
    /// dragged below it (#1241 f168).
    pub derived_walk: Option<f32>,
    /// The CAMERA is below a water surface (#1241 f160). Separate from
    /// [`Self::water`], which classifies the avatar: a third-person orbit
    /// camera dips under the surface on its own and, because the water
    /// plane is back-face culled (`world_builder::material`), there is
    /// nothing to see from below - no tint, no fog swap, no surface at
    /// all. The player cannot tell swimming from falling through empty
    /// space, and the flow current then moves them for no visible reason.
    pub camera_submerged: bool,
}

/// Whether, and how, the avatar editor is holding the local body still
/// this frame (#1158).
///
/// The four questions the player systems ask, answered as data. They used
/// to ask `ui::avatar::AvatarEditorState` directly, which put an egui
/// resource type in the signature of the physics and animation drivers -
/// so a UI refactor could change locomotion, `player`'s unit tests had to
/// construct an editor state to exercise a gait, and the headless render
/// tool dragged the panel's state into scope to walk a body.
///
/// Mirrored once per frame by `ui::avatar::mirror_rig_hold`, which is the
/// only writer. Absent editor state (before login, in the render tool)
/// leaves every field `false`, which is exactly what the old
/// `Option<Res<…>>` degraded to.
///
/// The distinctions are NOT interchangeable and each is load-bearing -
/// see the predicates this mirrors on [`crate::ui::avatar::AvatarEditorState`],
/// which carry the reasoning (#1103, #1106).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RigHold {
    /// A whole-prop gizmo is aimed: snap the rig to its REST pose, so the
    /// offset being edited and the body it is measured against agree.
    pub at_rest: bool,
    /// A part gizmo is aimed: hold the body EXACTLY where it stands. A
    /// pause, not a re-pose - selecting must never move anything.
    pub pose: bool,
    /// Any avatar-side gizmo is aimed: freeze the chassis and the
    /// cosmetic sway.
    pub still: bool,
    /// A visuals ROW is selected - narrower than [`Self::still`]. Gates
    /// the drive systems, whose non-physics side effects (gait state,
    /// jump triggers) only need suppressing while a row is being edited.
    pub visuals_row: bool,
}

/// Run condition: true when the avatar editor has a visuals row
/// selected - any node in the visuals tree, root or descendant. The five
/// locomotion drive systems gate on `not(this)` so WASD input does
/// nothing while the owner is editing visuals, and the hover-boat's
/// uprighting torque is gated so a gizmo-rotated chassis stays where the
/// user put it.
///
/// The actual full-body freeze lives in
/// [`freeze_local_avatar_while_editing`], which takes the chassis out of
/// the simulation, and the cosmetic sway hold lives in
/// [`gait::animate_avatar_gait`] - both keyed on
/// [`RigHold::still`], which since #1103 is true exactly while a gizmo
/// is aimed at the avatar or something it wears
/// (visuals row, worn prop, worn-prop part). This input gate is narrower
/// still - the visuals row only - because the drive systems have
/// non-physics side effects (gait state, jump triggers) that only need
/// suppressing while a row is actively being edited, and the freeze
/// already neutralizes any movement they would cause under a prop gizmo.
fn avatar_visuals_row_selected(hold: Res<RigHold>) -> bool {
    hold.visuals_row
}

/// Marker carried by the chassis while the visuals-edit freeze holds it,
/// beside the `RigidBodyDisabled` that does the holding.
///
/// `pub(super)` for the drive systems that stand down while it is present
/// (the airplane's flight model, the winged flight, the helicopter): a
/// disabled body is outside the solver, but the forces and key state those
/// systems keep are theirs, and a frozen body owes them nothing.
#[derive(Component)]
pub(super) struct VisualsEditFreeze;

/// Hold the local player's chassis frozen while a gizmo is aimed at the
/// avatar or at something it wears ([`RigHold::still`]), and while a
/// portal travel is in flight: the body takes avian's `RigidBodyDisabled`
/// until the selection releases. Freezing the chassis (rather than just
/// gating the drive systems) stops the passive movers too - suspension,
/// buoyancy, gravity/falling, slope creep - so the avatar holds its exact
/// pose during the edit, even mid-air. That matters for correctness as
/// well as ergonomics: the drag commit's world→local conversion reads the
/// parent chassis's `GlobalTransform`, which must be stable while the
/// gizmo is attached.
///
/// The gate matches the cosmetic gait/sway hold in
/// [`gait::animate_avatar_gait`]: both key on the same selection-scoped
/// gate, so physics and sway agree. #814 had widened both to "window
/// open" because a selection-scoped freeze under a window-wide sway hold
/// left the body translating while its sway was pinned; #1103 (owner
/// direction - the World editor's contract, pinned only under a gizmo)
/// narrowed both together, which keeps them consistent the other way
/// round. Every path that hides the window releases the selections, so a
/// closed editor never holds.
///
/// From #740 until #1894 this froze by "parking" instead - a full axis
/// lock, zero gravity and per-frame momentum zeroing, the body left in
/// the simulation - because through avian 0.7 an insert/remove cycle of
/// `RigidBodyDisabled` on a body with touching contacts linked the
/// contact into an island twice and later panicked the solver. Avian 0.8
/// rebuilt that bookkeeping, and `tests/freeze_rigid_body.rs`'s
/// `plain_rigid_body_disabled_cycle`, which runs in every gate, holds it
/// to that with a live ground contact on both sides of the cycle.
///
/// State-synced rather than edge-triggered, so recovery heals on the next
/// frame: a fresh chassis entity (room travel respawn) has neither
/// component and is frozen from scratch, and a body that lost its
/// `RigidBodyDisabled` mid-hold gets it back. A locomotion rebuild under
/// the freeze needs nothing from here: it leaves `RigidBody` and both
/// components alone, and avian files the new collider as a disabled
/// body's.
///
/// The momentum clear while frozen is not physics - a disabled body
/// integrates nothing. The drive systems gate on the visuals ROW only
/// ([`avatar_visuals_row_selected`]), so under a worn prop's gizmo the walk
/// still writes the velocity WASD asks for each fixed step; the clear here
/// keeps that velocity from being what the body carries out of the freeze
/// on release. The network broadcast does not lean on it: that runs in
/// `FixedUpdate`, unordered against both the drives and this, so it reads
/// `RigidBodyDisabled` itself and rests a frozen body at the rest rate
/// whatever velocity a drive wrote (#1897). Written only when there is
/// something to clear.
///
/// The release hands the body back to the broad phase
/// ([`rejoin_broad_phase`]), so its ground contact is found on the first
/// step rather than after it has sunk out of its enlarged AABB.
#[allow(clippy::type_complexity)]
fn freeze_local_avatar_while_editing(
    mut commands: Commands,
    hold: Res<RigHold>,
    traveling: Option<Res<crate::state::TravelingTo>>,
    mut q: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut AngularVelocity,
            Has<VisualsEditFreeze>,
            Has<RigidBodyDisabled>,
        ),
        (With<LocalPlayer>, With<RigidBody>),
    >,
) {
    // Held for an avatar-editing session AND while a portal travel is in
    // flight (#842): travel suppresses the drive systems but used to
    // leave the chassis loose under gravity/physics, so aircraft sagged
    // and boats drifted through the fetch. Same freeze/release either way.
    let held = hold.still || traveling.is_some();
    for (entity, mut lin, mut ang, frozen, disabled) in q.iter_mut() {
        if held {
            if lin.0 != Vec3::ZERO {
                lin.0 = Vec3::ZERO;
            }
            if ang.0 != Vec3::ZERO {
                ang.0 = Vec3::ZERO;
            }
            if !(frozen && disabled) {
                commands
                    .entity(entity)
                    .try_insert((VisualsEditFreeze, RigidBodyDisabled));
            }
        } else if frozen {
            commands
                .entity(entity)
                .try_remove::<(RigidBodyDisabled, VisualsEditFreeze)>();
            commands.queue(move |world: &mut World| rejoin_broad_phase(world, entity));
        }
    }
}

/// Hand a body that has just lost its `RigidBodyDisabled` back to avian's
/// broad phase, so its pairs are looked for on the next step.
///
/// Avian 0.8 drops a body's contacts when it is disabled and again when
/// it is enabled, so they are re-created fresh - but it re-creates them
/// only for a collider in `MovedProxies`, and a re-enabled body that has
/// not moved is not in it. Left to itself, a body released on the ground
/// falls with no contact until its AABB leaves the enlarged one avian
/// keeps for it (a margin of up to 5 cm), then is pushed back out: the
/// humanoid chassis of `tests/freeze_rigid_body.rs` sank 6 cm over seven
/// steps. Marking its colliders moved finds the ground on the first step.
///
/// Two more things avian leaves behind, put right here:
///
/// - **The proxy's `BODY_DISABLED` flag.** Avian sets it from an observer of
///   `Discard<RigidBodyDisabled>` that reads whether the body has the
///   component - which it still does while the observer runs - so the flag
///   is set by the removal and stays set on a live body. The narrow phase
///   makes its pairs generate constraints regardless, but swept CCD of
///   other bodies passes over a proxy flagged so. Cleared on each proxy;
///   `tests/freeze_rigid_body.rs` pins avian's side, so the clear can go
///   when upstream reads the flag right.
/// - **The colliders' own `CollidingEntities`.** Disabling the body drops
///   its contacts and takes it out of every other collider's set, but not
///   the others out of its own, so a body moved out of a sensor while
///   frozen still reads the sensor after the release (the gateway picker
///   and the portal read the chassis's set). Cleared on each collider that
///   holds anything; every pair still touching adds itself back on the next
///   step.
///
/// Call it after the removal has applied. A collider not in its tree any
/// more is skipped (the broad phase would unwrap its proxy), and one that
/// leaves its tree before the step takes its key out of `MovedProxies` on
/// the way. A body that is gone, or still disabled, is left alone.
pub fn rejoin_broad_phase(world: &mut World, body: Entity) {
    use avian3d::collider_tree::{
        ColliderTreeProxyFlags, ColliderTreeProxyKey, ColliderTrees, MovedProxies,
    };
    let Ok(entity) = world.get_entity(body) else {
        return;
    };
    if entity.contains::<RigidBodyDisabled>() {
        return;
    }
    let Some(colliders) = entity.get::<RigidBodyColliders>() else {
        return;
    };
    let colliders: Vec<Entity> = colliders.iter().collect();
    for &collider in &colliders {
        if let Some(mut colliding) = world.get_mut::<CollidingEntities>(collider)
            && !colliding.0.is_empty()
        {
            colliding.0.clear();
        }
    }
    let mut keys: Vec<ColliderTreeProxyKey> = colliders
        .iter()
        .filter_map(|&collider| world.get::<ColliderTreeProxyKey>(collider).copied())
        .collect();
    match world.get_resource_mut::<ColliderTrees>() {
        Some(mut trees) => keys.retain(|&key| {
            let flagged = match trees.get_proxy(key) {
                Some(proxy) => proxy.flags.contains(ColliderTreeProxyFlags::BODY_DISABLED),
                None => return false,
            };
            if flagged && let Some(proxy) = trees.get_proxy_mut(key) {
                proxy.flags.remove(ColliderTreeProxyFlags::BODY_DISABLED);
            }
            true
        }),
        None => keys.clear(),
    }
    if let Some(mut moved) = world.get_resource_mut::<MovedProxies>() {
        for key in keys {
            moved.insert(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The suspension / grounding ray must skip its own chassis *and* every
    /// sensor (gateway veils, portals), so a vehicle drives into a gateway
    /// rather than climbing its surface (#813).
    #[test]
    fn ground_ray_filter_excludes_chassis_and_all_sensors() {
        let mut world = World::new();
        let chassis = world.spawn_empty().id();
        let gateway = world.spawn_empty().id();
        let portal = world.spawn_empty().id();
        let terrain = world.spawn_empty().id();

        let filter = ground_ray_filter(chassis, [gateway, portal]);

        assert!(filter.excluded_entities.contains(&chassis));
        assert!(filter.excluded_entities.contains(&gateway));
        assert!(filter.excluded_entities.contains(&portal));
        // Solid ground stays visible to the ray.
        assert!(!filter.excluded_entities.contains(&terrain));
        assert_eq!(filter.excluded_entities.len(), 3);
    }

    /// The steer-sign shared by the car (#723) and hover-boat (#724) drives:
    /// forward sign held through a standstill (turn-in-place), inverted only
    /// once clearly reversing past the deadband.
    #[test]
    fn steer_sign_holds_forward_and_flips_only_when_clearly_reversing() {
        // Driving forward - normal steering.
        assert_eq!(reverse_steer_sign(5.0), 1.0);
        // Clearly reversing - inverted.
        assert_eq!(reverse_steer_sign(-5.0), -1.0);
        // Stopped - forward sign, so turn-in-place is preserved.
        assert_eq!(reverse_steer_sign(0.0), 1.0);
        // Within the reverse deadband (creep) - still forward sign.
        let deadband = cfg::REVERSE_STEER_SPEED;
        assert_eq!(reverse_steer_sign(-deadband * 0.5), 1.0);
        // Just past the deadband - inverted.
        assert_eq!(reverse_steer_sign(-deadband - 0.1), -1.0);
    }
}

#[cfg(test)]
mod hair_lod_tests {
    use super::*;

    /// A crossfading hair LOD quits every WebGL2 client, so the band this
    /// crate asks for is zero - and stays zero (#1358).
    ///
    /// Named for the consequence rather than for the value, because the value
    /// looks like a tuning knob and is not one. In Bevy 0.19.1 a non-zero
    /// `VisibilityRange` margin switches on the dither shader, which reads the
    /// range table as a 64-entry uniform (1024 bytes) on WebGL2 while
    /// `bevy_pbr`'s mesh view layout sizes that binding at one `Vec4`;
    /// `pbr_opaque_mesh_pipeline` then fails validation and the app exits on
    /// the first frame a body is inside the band. Measured in Chromium against
    /// the published adapter with 40 bodies (bevy_symbios_avatar #48), where
    /// margin 0 ran clean for 65 s and margin 4 quit.
    ///
    /// Asserted through the ranges rather than only on the constant, because
    /// what keeps the shader out of the build is `start_margin` being empty -
    /// the number is just how we get there.
    #[test]
    fn a_hair_crossfade_would_quit_every_webgl2_client() {
        use bevy_symbios_avatar::{HairLod, HairTier};

        assert_eq!(
            crate::config::camera::HAIR_MARGIN,
            0.0,
            "a non-zero hair margin is a WebGL2 crash, not a look"
        );
        let lod = HairLod {
            switch: crate::config::camera::HAIR_SWITCH,
            margin: crate::config::camera::HAIR_MARGIN,
        };
        for tier in [HairTier::Near, HairTier::Far] {
            let range = lod.range(tier);
            assert!(
                range.start_margin.start == range.start_margin.end
                    && range.end_margin.start == range.end_margin.end,
                "{tier:?} crossfades over start {:?} end {:?}; Bevy compiles \
                 its dither shader for any range with width, and that shader \
                 does not pass pipeline validation on WebGL2",
                range.start_margin,
                range.end_margin
            );
        }

        // The control: the same assertion has to be able to FAIL. A band of a
        // metre is what the adapter measured crashing.
        let band = HairLod {
            switch: crate::config::camera::HAIR_SWITCH,
            margin: 1.0,
        };
        let range = band.range(HairTier::Far);
        assert!(
            range.start_margin.start < range.start_margin.end,
            "a margin of a metre must read as a crossfade, or this guard is \
             asserting nothing"
        );
    }

    /// The app runs on THIS lens's hair switch, not the adapter's default -
    /// and the insert has to come after the plugin to do it (#1358).
    ///
    /// `AvatarPlugin` `init_resource`s its own `HairLod`, and `init_resource`
    /// keeps whatever is already there. So an insert moved above the plugin
    /// line is not an error, a warning or a panic: it is thrown away, every
    /// body silently goes back to switching at 12 m, and the only way to see
    /// it is to render a body at 14 m and know what it should look like.
    /// Registering the two in the same order `PlayerPlugin` does is what this
    /// pins.
    #[test]
    fn the_app_switches_hair_at_this_lenss_distance_and_not_the_adapters() {
        use bevy_symbios_avatar::{HAIR_SWITCH as ADAPTER_SWITCH, HairLod};

        let mut app = App::new();
        app.add_plugins(bevy_symbios_avatar::AvatarPlugin)
            .insert_resource(HairLod {
                switch: crate::config::camera::HAIR_SWITCH,
                margin: crate::config::camera::HAIR_MARGIN,
            });
        assert_eq!(
            app.world().resource::<HairLod>().switch,
            crate::config::camera::HAIR_SWITCH,
            "the plugin's default won; the insert must come after it"
        );
        assert_ne!(
            crate::config::camera::HAIR_SWITCH,
            ADAPTER_SWITCH,
            "this guard only says anything while the two differ - if the \
             adapter has adopted this lens's distance, delete it rather than \
             leaving a test that passes for the wrong reason"
        );

        // The control: the same registration the other way round loses.
        let mut backwards = App::new();
        backwards
            .insert_resource(HairLod {
                switch: crate::config::camera::HAIR_SWITCH,
                margin: crate::config::camera::HAIR_MARGIN,
            })
            .add_plugins(bevy_symbios_avatar::AvatarPlugin);
        assert_eq!(
            backwards.world().resource::<HairLod>().switch,
            crate::config::camera::HAIR_SWITCH,
            "init_resource must keep an inserted value - if this ever fails, \
             the order above stopped mattering and the comment is stale"
        );
    }
}

/// The visuals-edit freeze on a real walker (#1894): avian's
/// `RigidBodyDisabled` in, out, and the body back on the ground.
#[cfg(test)]
mod freeze_tests {
    use super::*;
    use crate::pds::avatar::AvatarRecord;
    use crate::pds::{Fp, LocomotionConfig};
    use crate::player::sim::WalkBench;
    use crate::state::LiveAvatarRecord;
    use bevy::ecs::system::RunSystemOnce;

    /// A person on foot, the preset's defaults.
    fn person() -> AvatarRecord {
        let mut record = AvatarRecord::default_for_did("did:plc:visuals-edit-freeze");
        record.locomotion = LocomotionConfig::Humanoid(Box::default());
        record
    }

    /// The same person re-rolled 0.6 m taller: a collider changed in place
    /// by `preset::sync_chassis`.
    fn taller(record: &AvatarRecord) -> AvatarRecord {
        let mut taller = record.clone();
        let LocomotionConfig::Humanoid(p) = &mut taller.locomotion else {
            unreachable!("a person")
        };
        p.capsule_length = Fp(p.capsule_length.0 + 0.6);
        taller
    }

    fn standing_height(record: &AvatarRecord) -> f32 {
        let LocomotionConfig::Humanoid(p) = &record.locomotion else {
            unreachable!("a person")
        };
        p.total_height() * 0.5
    }

    /// The body has a touching pair with manifolds that makes constraints:
    /// the ground holds it up this step.
    fn on_the_ground(bench: &mut WalkBench) -> bool {
        let body = bench.body();
        bench
            .world_mut()
            .resource::<ContactGraph>()
            .contact_pairs_with(body)
            .any(|pair| {
                pair.is_touching() && !pair.manifolds.is_empty() && pair.generates_constraints()
            })
    }

    fn set_hold(bench: &mut WalkBench, still: bool) {
        *bench.world_mut().resource_mut::<RigHold>() = RigHold {
            still,
            ..RigHold::default()
        };
        bench
            .world_mut()
            .run_system_once(freeze_local_avatar_while_editing)
            .expect("the freeze runs");
    }

    fn frozen(bench: &mut WalkBench) -> (bool, bool) {
        let body = bench.body();
        let entity = bench.world_mut().entity(body);
        (
            entity.contains::<VisualsEditFreeze>(),
            entity.contains::<RigidBodyDisabled>(),
        )
    }

    /// Whether avian's proxy of the body's collider is flagged as a disabled
    /// body's, which swept CCD of other bodies passes over.
    fn flagged_disabled(bench: &mut WalkBench) -> bool {
        use avian3d::collider_tree::{ColliderTreeProxyFlags, ColliderTreeProxyKey, ColliderTrees};
        let body = bench.body();
        let world = bench.world_mut();
        let key = *world
            .get::<ColliderTreeProxyKey>(body)
            .expect("the collider is in a tree");
        world
            .resource::<ColliderTrees>()
            .get_proxy(key)
            .expect("a proxy")
            .flags
            .contains(ColliderTreeProxyFlags::BODY_DISABLED)
    }

    /// What the body's own `CollidingEntities` holds.
    fn colliding(bench: &mut WalkBench) -> Vec<Entity> {
        let body = bench.body();
        bench
            .world_mut()
            .get::<CollidingEntities>(body)
            .expect("the chassis keeps its collisions")
            .iter()
            .copied()
            .collect()
    }

    /// A gizmo on a worn prop (the visuals row is NOT selected, so the walk
    /// still runs) with W held: the body does not move, and on release it
    /// stands where it stood, on the ground from the first step - not
    /// sinking out of its enlarged AABB first - with its proxy no longer
    /// flagged as a disabled body's.
    #[test]
    fn the_freeze_holds_the_body_still_and_lets_it_go_on_the_ground() {
        let record = person();
        let mut bench = WalkBench::standing(&record, 0.0, 0.0);
        assert!(on_the_ground(&mut bench), "the fixture: it stands");

        bench.hold(&[KeyCode::KeyW]);
        set_hold(&mut bench, true);
        assert_eq!(frozen(&mut bench), (true, true), "the freeze took hold");
        let held = bench.position();
        for step in 0..64 {
            bench.step();
            set_hold(&mut bench, true);
            assert_eq!(bench.position(), held, "it moved at frozen step {step}");
        }

        bench.hold(&[]);
        set_hold(&mut bench, false);
        assert_eq!(frozen(&mut bench), (false, false), "the release let go");
        bench.step();
        assert!(
            on_the_ground(&mut bench),
            "no ground contact on the first step after the release"
        );
        assert!(
            !flagged_disabled(&mut bench),
            "the released body's proxy is still flagged as a disabled body's"
        );
        for step in 0..64 {
            bench.step();
            let y = bench.position().y;
            assert!(
                (y - held.y).abs() < 2e-3,
                "{:+.1} mm from where it was held, {step} steps after the release",
                (y - held.y) * 1e3
            );
        }
    }

    /// The chassis' own `CollidingEntities` across a freeze, as the gateway
    /// picker and the portal read it: a body frozen standing in a sensor
    /// reads it again on the first step after the release, and one moved out
    /// of it while frozen reads it no more. Avian's disable takes the body
    /// out of the sensor's set but leaves the sensor in the body's, for good.
    #[test]
    fn a_body_moved_out_of_a_sensor_while_frozen_reads_it_no_more() {
        let record = person();
        let mut bench = WalkBench::standing(&record, 0.0, 0.0);
        let body = bench.body();
        let at = bench.position();
        let sensor = bench
            .world_mut()
            .spawn((
                RigidBody::Static,
                Collider::cuboid(2.0, 4.0, 2.0),
                Sensor,
                Transform::from_translation(at),
            ))
            .id();
        bench.run(0.1);
        assert!(
            colliding(&mut bench).contains(&sensor),
            "the fixture: it does not read the sensor it stands in"
        );

        // Frozen where it stands, and let go.
        set_hold(&mut bench, true);
        for _ in 0..8 {
            bench.step();
            set_hold(&mut bench, true);
        }
        set_hold(&mut bench, false);
        bench.step();
        assert!(
            colliding(&mut bench).contains(&sensor),
            "released in the sensor, it does not read it on the first step"
        );

        // Frozen, moved out of it, and let go.
        set_hold(&mut bench, true);
        let out = at + Vec3::X * 6.0;
        let mut entity = bench.world_mut().entity_mut(body);
        entity.get_mut::<Position>().expect("a position").0 = out;
        entity
            .get_mut::<Transform>()
            .expect("a transform")
            .translation = out;
        for _ in 0..8 {
            bench.step();
            set_hold(&mut bench, true);
        }
        set_hold(&mut bench, false);
        for step in 0..8 {
            bench.step();
            assert!(
                !colliding(&mut bench).contains(&sensor),
                "moved out of the sensor while frozen, it still reads it {step} steps after the \
                 release"
            );
        }
        assert!(
            (bench.position() - out).xz().length() < 0.05,
            "the fixture: the body is not where it was moved: {:?}",
            bench.position()
        );
    }

    /// #867, on the new freeze: kind-changing re-seeds (the locomotion
    /// rebuild's strip and build - a car, a hover-boat, a person again)
    /// and a re-roll changing the collider in place, all while the body is
    /// disabled. Released, it settles on its new capsule standing on the
    /// ground, finite throughout, and walks on it.
    #[test]
    fn a_re_seed_under_the_freeze_stands_on_the_ground_after_release() {
        let record = person();
        let mut bench = WalkBench::standing(&record, 0.0, 0.0);
        set_hold(&mut bench, true);
        let body = bench.body();

        let mut car = record.clone();
        car.locomotion = LocomotionConfig::Car(Box::default());
        let mut boat = record.clone();
        boat.locomotion = LocomotionConfig::HoverBoat(Box::default());
        for (round, seed) in [&car, &boat, &record].into_iter().enumerate() {
            let mut commands = bench.world_mut().commands();
            preset::strip_preset_components(&mut commands, body);
            preset::build_preset_components(&mut commands, body, &seed.locomotion);
            bench.world_mut().flush();
            bench
                .world_mut()
                .insert_resource(LiveAvatarRecord(seed.clone()));
            for _ in 0..16 {
                bench.step();
                set_hold(&mut bench, true);
            }
            assert_eq!(
                frozen(&mut bench),
                (true, true),
                "re-seed {round} thawed it"
            );
        }
        // Same kind, taller: `sync_chassis` changes the capsule in place.
        let tall = taller(&record);
        bench
            .world_mut()
            .insert_resource(LiveAvatarRecord(tall.clone()));
        for _ in 0..16 {
            bench.step();
            set_hold(&mut bench, true);
        }

        set_hold(&mut bench, false);
        let mut grounded = false;
        for step in 0..192 {
            bench.step();
            let at = bench.position();
            assert!(at.is_finite(), "non-finite at step {step}: {at:?}");
            assert!(
                at.y > 0.0,
                "fell through the floor at step {step}: y = {}",
                at.y
            );
            grounded = on_the_ground(&mut bench);
        }
        assert!(
            grounded,
            "no ground contact three seconds after the release"
        );
        let want = standing_height(&tall);
        let y = bench.position().y;
        assert!(
            (y - want).abs() < 0.01,
            "stands {:+.1} cm off its new capsule's height",
            (y - want) * 100.0
        );

        bench.hold(&[KeyCode::KeyW]);
        for step in 0..64 {
            bench.step();
            let y = bench.position().y;
            assert!((y - want).abs() < 0.05, "walking, {y} at step {step}");
            assert!(
                on_the_ground(&mut bench),
                "walked off the ground at step {step}"
            );
        }
        assert!(
            bench.position().xz().length() > 0.5,
            "the walk went nowhere: {:?}",
            bench.position()
        );
    }
}
