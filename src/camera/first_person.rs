//! First person (#1745): zoom in past the orbit's closest distance and the
//! camera moves into the body's own eyes; zoom out and it comes back.
//!
//! **One camera, still the orbit's.** A second world camera silently breaks
//! every query that means "the camera" (#1300), so first person is not a
//! camera of its own: it is where [`place_first_person_camera`] puts the one
//! world camera after `bevy_panorbit_camera` has run. The crate keeps reading
//! right-drag and one-finger touch into its yaw and pitch exactly as it does
//! to orbit, so turning the head is the gesture that orbits, with the same
//! smoothing; only its zoom is held while the eyes look (its zoom sensitivity
//! goes to 0, [`switch_view`]). On foot the eyes look level with the world
//! along the orbit's yaw and pitch. In a vehicle they ride its nose, turning,
//! pitching and banking with it, and the orbit stops following the vehicle's
//! turns (`follow_local_player`): its yaw since the eyes were entered is how
//! far the head is turned from straight ahead.
//!
//! **Into the eyes and out again.** The move takes
//! [`cfg::FIRST_PERSON_BLEND_S`] each way, eased: the camera is placed between
//! the orbit's own pose and the eyes' by how far the move has come, so the
//! zoom seems to carry on through the floor and back. Leaving puts the orbit
//! behind the body looking the way the eyes were turned, at the pitch it had
//! before them.
//!
//! **What switches.** A zoom-in that carries on at the orbit's floor
//! ([`cfg::ZOOM_LOWER_LIMIT`]) once it has rested there, and a zoom-out from
//! the eyes ([`ZoomPush`]); V from the keyboard (`ui::shortcuts`); opening the
//! Avatar editor, or the body going away, back to the orbit. The zoom half is
//! the player's to switch off (`LocalSettings::zoom_into_first_person`); V
//! works either way. The game reads the wheel, the pinch and the two-finger
//! spread itself, beside the crate: the crate clamps a zoom at its floor and
//! says nothing, so a zoom past it is invisible in its state.
//!
//! **The body leaves the view, its shadow stays.** Past
//! [`cfg::FIRST_PERSON_HIDE_AT`] of the move the chassis carries
//! `Propagate<RenderLayers>` onto [`cfg::FIRST_PERSON_BODY_LAYER`], which the
//! world camera does not draw and the sun shadows, so a body (and all it
//! wears, and its stand-in) is gone from its own view and still throws its
//! shadow on the ground. Peers are drawn as ever. `RenderLayers` does not
//! propagate down a hierarchy by itself in Bevy 0.19, and propagated, it is
//! taken off every descendant again when the source goes
//! ([`register_render_layer_propagation`]).
//!
//! **The eyes see through a perspective lens** whatever the projection
//! setting, as the agent's eye view does: an orthographic camera at eye
//! height is a wall of ground under a band of sky (`follow_projection_setting`).

use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_symbios_avatar::AvatarBody;
use symbios_avatar::rig::Landmark;

use super::{IsWorldCamera, WorldCamera, clear_of_terrain};
use crate::config::camera as cfg;
use crate::interaction::locomotion::{locomotion_footprint, locomotion_total_height};
use crate::pds::LocomotionConfig;
use crate::player::{RiggedRoot, VehicleChassis};
use crate::state::{LiveAvatarRecord, LocalPlayer, LocalSettings};
use crate::terrain::FinishedHeightMap;

/// Which view the world camera gives (#1745).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraView {
    /// The third-person orbit round the body.
    #[default]
    Orbit,
    /// Through the body's own eyes.
    Eyes,
}

/// The world camera's first-person state (#1745): the view asked for, how far
/// the camera has come toward it, and what the eyes changed in the orbit, to
/// give back when they leave.
#[derive(Resource, Debug, Default)]
pub struct FirstPerson {
    /// The view the player asked for.
    view: CameraView,
    /// How far the camera has come from the orbit toward the eyes, 0 to 1, in
    /// time; the pose eases it.
    blend: f32,
    /// Zoom gathered toward the next switch.
    push: ZoomPush,
    /// A switch asked for since the last frame: V.
    toggle: bool,
    /// The orbit yaw that meant "straight ahead" when the eyes rode a
    /// vehicle: the orbit's yaw since then is the head's turn.
    ahead_yaw: Option<f32>,
    /// On the way out of a vehicle's eyes, the head's turn on the vehicle as
    /// they left: the eyes' half of the move holds it while the orbit,
    /// counted from the world again, takes over.
    leaving_turn: Option<f32>,
    /// The orbit's pitch before the eyes, to give back when they leave.
    orbit_pitch: Option<f32>,
    /// The orbit's zoom sensitivity, held at 0 while the eyes look.
    zoom_sensitivity: Option<f32>,
    /// The chassis whose body is out of the camera's view.
    hidden: Option<Entity>,
}

impl FirstPerson {
    /// The view the player asked for.
    pub fn view(&self) -> CameraView {
        self.view
    }

    /// Whether the camera is at the eyes or on its way to or from them: the
    /// orthographic lens waits for the orbit to be whole again.
    pub(crate) fn engaged(&self) -> bool {
        self.view == CameraView::Eyes || self.blend > 0.0
    }

    /// Ask for the other view (V): applied on the next frame's switch.
    pub fn request_toggle(&mut self) {
        self.toggle = true;
    }

    /// Whether the orbit is to stop following a vehicle's turns: the eyes
    /// ride it, and the orbit's yaw is the head's turn.
    pub(crate) fn rides_vehicle(&self) -> bool {
        self.ahead_yaw.is_some()
    }
}

/// Whether the ui layer holds the world camera at the orbit (#1745): while
/// the Avatar editor is open, since looking out of a body while editing it
/// makes no sense. Written once a frame in `PreUpdate` by
/// `ui::toolbar::mirror_orbit_hold`, so the camera reads a fact it owns
/// instead of importing the egui layer (#1158, #1297).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrbitHold(pub bool);

/// Zoom gathered toward a switch of view (#1745), in the orbit crate's own
/// units (one wheel notch is 1.0). Pure, so the rules test without an app.
///
/// Toward the eyes a zoom-in counts only once the orbit has rested at its
/// floor, with no zoom-in arriving, for [`cfg::ZOOM_PUSH_SETTLE_S`]: a
/// trackpad swipe that brings it down there goes on sending input for as
/// long as its momentum lasts, and each of those starts the wait again, so
/// the swipe does not fall through. [`cfg::ZOOM_PUSH_NOTCH`] of it takes the
/// camera in;
/// [`cfg::ZOOM_PULL_NOTCH`] of zoom-out brings it back. A zoom the other way
/// starts the count again, and so does [`cfg::ZOOM_PUSH_IDLE_S`] without one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ZoomPush {
    /// When the orbit came to rest at its floor, while it is there.
    floor_since: Option<f64>,
    /// Zoom gathered toward the next switch.
    gathered: f32,
    /// When the last zoom that counted came.
    last: f64,
}

impl ZoomPush {
    /// One frame's zoom - positive in, negative out - at `now`, in `view`,
    /// with the orbit at its floor or not; the view to switch to, if this
    /// frame's zoom completes a push.
    pub(crate) fn feed(
        &mut self,
        now: f64,
        view: CameraView,
        at_floor: bool,
        zoom: f32,
    ) -> Option<CameraView> {
        if self.gathered != 0.0 && now - self.last > cfg::ZOOM_PUSH_IDLE_S {
            self.gathered = 0.0;
        }
        match view {
            CameraView::Orbit => {
                if !at_floor {
                    *self = Self::default();
                    return None;
                }
                let since = *self.floor_since.get_or_insert(now);
                if zoom > 0.0 && now - since < cfg::ZOOM_PUSH_SETTLE_S {
                    // Still arriving: the rest starts from this input.
                    self.floor_since = Some(now);
                } else if zoom > 0.0 {
                    self.gathered += zoom;
                    self.last = now;
                    if self.gathered >= cfg::ZOOM_PUSH_NOTCH {
                        *self = Self::default();
                        return Some(CameraView::Eyes);
                    }
                } else if zoom < 0.0 {
                    self.gathered = 0.0;
                }
                None
            }
            CameraView::Eyes => {
                self.floor_since = None;
                if zoom < 0.0 {
                    self.gathered -= zoom;
                    self.last = now;
                    if self.gathered >= cfg::ZOOM_PULL_NOTCH {
                        *self = Self::default();
                        return Some(CameraView::Orbit);
                    }
                } else if zoom > 0.0 {
                    self.gathered = 0.0;
                }
                None
            }
        }
    }
}

/// Where a body's eyes are (#1745).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EyeMount {
    /// On foot: this far above the chassis centre, on its axis. The look is
    /// the orbit's, level with the world.
    OnFoot { above_centre: f32 },
    /// In a vehicle: here in the chassis' own frame - its nose, near its top.
    /// The look turns, pitches and banks with the vehicle.
    Vehicle { at: Vec3 },
}

impl EyeMount {
    /// The eyes of a body moving by `locomotion`: a vehicle's at the front of
    /// its footprint, [`cfg::EYE_HEIGHT_FRACTION`] of the way up (the agent's
    /// eye view, #1420); a person's at their rig's own eye line when it has
    /// one, `rig_eyes` above the root that hangs `root_y` off the chassis
    /// centre (its swim lift with it), and otherwise at that fraction of the
    /// collider's height.
    pub(crate) fn of(
        locomotion: &LocomotionConfig,
        vehicle: bool,
        root_y: Option<f32>,
        rig_eyes: Option<f32>,
    ) -> Self {
        let height = locomotion_total_height(locomotion);
        let fraction_above_centre = (cfg::EYE_HEIGHT_FRACTION - 0.5) * height;
        if vehicle {
            return Self::Vehicle {
                at: Vec3::new(
                    0.0,
                    fraction_above_centre,
                    -locomotion_footprint(locomotion),
                ),
            };
        }
        let above_centre = match (root_y, rig_eyes) {
            (Some(root_y), Some(eyes)) => root_y + eyes,
            _ => fraction_above_centre,
        };
        Self::OnFoot { above_centre }
    }

    /// The eyes' pose on `chassis`, the head turned `yaw` and pitched `pitch`
    /// by the orbit's own convention (`pitch` down, as the orbit's camera
    /// looks down at its focus): on foot `yaw` is a heading, in a vehicle the
    /// turn from straight ahead.
    pub(crate) fn pose(self, chassis: &Transform, yaw: f32, pitch: f32) -> Transform {
        let head = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
        match self {
            Self::OnFoot { above_centre } => Transform {
                translation: chassis.translation + Vec3::Y * above_centre,
                rotation: head,
                ..default()
            },
            Self::Vehicle { at } => Transform {
                translation: chassis.translation + chassis.rotation * at,
                rotation: chassis.rotation * head,
                ..default()
            },
        }
    }
}

/// The orbit camera's own pose at `yaw`, `pitch` and `radius` round `focus`:
/// `bevy_panorbit_camera`'s `update_orbit_transform`, perspective.
fn orbit_pose(yaw: f32, pitch: f32, radius: f32, focus: Vec3) -> Transform {
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    Transform {
        translation: focus + rotation * Vec3::new(0.0, 0.0, radius),
        rotation,
        ..default()
    }
}

/// A vehicle's heading as the orbit's yaw counts it - the yaw at which the
/// orbit looks along it - or none near the vertical, where it has none
/// (`follow_local_player` freezes there too, #853).
fn heading(chassis: &Transform) -> Option<f32> {
    let forward = chassis.rotation * Vec3::NEG_Z;
    (forward.y.abs() <= cfg::YAW_FREEZE_FORWARD_Y).then(|| (-forward.x).atan2(-forward.z))
}

/// `angle` moved by whole turns to within half a turn of `near`: the orbit's
/// yaw is unbounded, and a target a turn away from it spins the camera round.
fn near_turn(angle: f32, near: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    near + (angle - near + PI).rem_euclid(TAU) - PI
}

/// This frame's zoom from the wheel, the trackpad and two fingers, in the
/// orbit crate's units - positive in - as its default trackpad behaviour
/// reads them: a wheel line is 1, a pixel 0.005, a pinch ten times its
/// scale (only when `pinch_counts`: the crate drops it under its orbit or
/// pan modifier), a change in two fingers' spread 0.015 a pixel.
///
/// The spread is measured against `last_spread`, last frame's, as the
/// crate's own touch tracker keeps it: a touch's `previous_position` moves
/// only on a frame that brings touch input, so on a quiet one - two fingers
/// held still - it still holds the last change, which would count again.
fn zoom_this_frame(
    wheel: &mut MessageReader<MouseWheel>,
    pinch: &mut MessageReader<PinchGesture>,
    touches: Option<&Touches>,
    pinch_counts: bool,
    last_spread: &mut Option<f32>,
) -> f32 {
    let scroll: f32 = wheel
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y * 0.005,
        })
        .sum();
    let pinched: f32 = pinch.read().map(|event| event.0 * 10.0).sum();
    let spread_now = touches.and_then(|touches| {
        let mut fingers = touches.iter();
        match (fingers.next(), fingers.next(), fingers.next()) {
            (Some(a), Some(b), None) => Some(a.position().distance(b.position())),
            _ => None,
        }
    });
    let spread = match (spread_now, std::mem::replace(last_spread, spread_now)) {
        (Some(now), Some(before)) => (now - before) * 0.015,
        _ => 0.0,
    };
    scroll + if pinch_counts { pinched } else { 0.0 } + spread
}

/// Decide the view (#1745): the zoom past the floor and back, V, and what
/// sends the camera back to the orbit - no body, or the Avatar editor open.
/// Runs before the orbit crate, after `gate_camera_on_gui`, so the wheel
/// over a window (which egui scrolls) counts for neither.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn switch_view(
    time: Res<Time>,
    settings: Res<LocalSettings>,
    hold: Res<OrbitHold>,
    keyboard: Res<ButtonInput<KeyCode>>,
    touches: Option<Res<Touches>>,
    mut wheel: MessageReader<MouseWheel>,
    mut pinch: MessageReader<PinchGesture>,
    mut first_person: ResMut<FirstPerson>,
    players: Query<(&Transform, Has<VehicleChassis>), (With<LocalPlayer>, Without<WorldCamera>)>,
    mut cameras: Query<&mut PanOrbitCamera, IsWorldCamera>,
    mut last_spread: Local<Option<f32>>,
) {
    let orbit = cameras.single_mut().ok();
    // The crate's own modifiers this frame (the pan's is Alt while no button
    // is held, `gate_camera_on_gui`): under either it takes no pinch.
    let pinch_counts = orbit.as_deref().is_some_and(|orbit| {
        [orbit.modifier_orbit, orbit.modifier_pan]
            .into_iter()
            .flatten()
            .all(|key| !keyboard.pressed(key))
    });
    // Read whether or not there is a camera, so no input waits a frame.
    let zoom = zoom_this_frame(
        &mut wheel,
        &mut pinch,
        touches.as_deref(),
        pinch_counts,
        &mut last_spread,
    );
    let Some(mut orbit) = orbit else {
        return;
    };
    // The push gathers every frame the wheel turns; the resource is stamped
    // changed only when the view does.
    let fp = first_person.bypass_change_detection();
    let before = fp.view;
    let player = players.single().ok();
    let held_off = player.is_none() || hold.0;

    let mut want = fp.view;
    if std::mem::take(&mut fp.toggle) {
        want = match fp.view {
            CameraView::Orbit => CameraView::Eyes,
            CameraView::Eyes => CameraView::Orbit,
        };
    }
    // The wheel over a window is egui's, and the gate says so.
    let zoom = if orbit.enabled { zoom } else { 0.0 };
    let at_floor = orbit.target_radius <= cfg::ZOOM_LOWER_LIMIT + 1e-3
        && orbit
            .radius
            .is_some_and(|radius| radius <= cfg::ZOOM_LOWER_LIMIT + 0.05);
    if fp.view == CameraView::Orbit && !settings.zoom_into_first_person {
        fp.push = ZoomPush::default();
    } else if let Some(view) = fp
        .push
        .feed(time.elapsed_secs_f64(), fp.view, at_floor, zoom)
    {
        want = view;
    }
    if held_off {
        want = CameraView::Orbit;
    }

    match (fp.view, want, player) {
        (CameraView::Orbit, CameraView::Eyes, Some((chassis, vehicle))) => {
            enter_eyes(fp, &mut orbit, chassis, vehicle);
        }
        (CameraView::Eyes, CameraView::Orbit, player) => leave_eyes(fp, &mut orbit, player),
        // A body that changes kind while the eyes look - a vehicle worn in
        // place of feet, or the other way about - takes the eyes with it: on
        // board they look along its nose, and off it they go on looking the
        // way they looked.
        (CameraView::Eyes, CameraView::Eyes, Some((chassis, vehicle)))
            if vehicle != fp.rides_vehicle() =>
        {
            if vehicle {
                fp.ahead_yaw = Some(look_ahead(&mut orbit, chassis));
            } else {
                ride_off(fp, &mut orbit, Some(chassis));
            }
        }
        _ => {}
    }
    if fp.view != before {
        first_person.set_changed();
    }
}

/// Point the orbit straight ahead along a vehicle, and say which of its yaws
/// that is - the one nearest where it was.
fn look_ahead(orbit: &mut PanOrbitCamera, chassis: &Transform) -> f32 {
    let ahead = heading(chassis).map_or(orbit.target_yaw, |yaw| near_turn(yaw, orbit.target_yaw));
    orbit.target_yaw = ahead;
    orbit.target_pitch = 0.0;
    orbit.force_update = true;
    ahead
}

/// Into the eyes: hold the orbit's zoom at its floor, keep its pitch to give
/// back, and look level - along a vehicle's nose, or the way the orbit
/// looked.
fn enter_eyes(
    fp: &mut FirstPerson,
    orbit: &mut PanOrbitCamera,
    chassis: &Transform,
    vehicle: bool,
) {
    fp.view = CameraView::Eyes;
    fp.push = ZoomPush::default();
    fp.leaving_turn = None;
    fp.zoom_sensitivity = Some(orbit.zoom_sensitivity);
    orbit.zoom_sensitivity = 0.0;
    fp.orbit_pitch = Some(orbit.target_pitch);
    fp.ahead_yaw = vehicle.then(|| look_ahead(orbit, chassis));
    orbit.target_pitch = 0.0;
    orbit.force_update = true;
}

/// Out to the orbit: its zoom back, its old pitch, and behind the body the
/// way the eyes were turned - for a vehicle, its heading now and the head's
/// turn on it.
fn leave_eyes(
    fp: &mut FirstPerson,
    orbit: &mut PanOrbitCamera,
    player: Option<(&Transform, bool)>,
) {
    fp.view = CameraView::Orbit;
    fp.push = ZoomPush::default();
    if let Some(sensitivity) = fp.zoom_sensitivity.take() {
        orbit.zoom_sensitivity = sensitivity;
    }
    fp.leaving_turn = ride_off(fp, orbit, player.map(|(chassis, _)| chassis));
    if let Some(pitch) = fp.orbit_pitch.take() {
        orbit.target_pitch = pitch;
    }
    orbit.force_update = true;
}

/// Off a vehicle's frame (#1745). While the eyes rode it, the orbit's yaw
/// counted the head's turn from the heading they boarded at; from here it
/// counts from the world again - the vehicle's heading now, taken the near
/// way round from that one, plus the head's turn on it - the current yaw
/// and the target alike, so the orbit does not swing round to it. The
/// head's turn, if the eyes rode a vehicle.
fn ride_off(
    fp: &mut FirstPerson,
    orbit: &mut PanOrbitCamera,
    chassis: Option<&Transform>,
) -> Option<f32> {
    let ahead = fp.ahead_yaw.take()?;
    let turn = orbit.yaw.unwrap_or(orbit.target_yaw) - ahead;
    if let Some(now) = chassis.and_then(heading) {
        let shift = near_turn(now, ahead) - ahead;
        orbit.target_yaw += shift;
        if let Some(yaw) = orbit.yaw.as_mut() {
            *yaw += shift;
        }
    }
    orbit.force_update = true;
    Some(turn)
}

/// Put the world camera where the view says (#1745): at the eyes, at the
/// orbit, or between them while it moves; and keep the body out of its own
/// view past [`cfg::FIRST_PERSON_HIDE_AT`] of the way in. Runs after the
/// orbit crate and the orbit's own corrections (the terrain clamp, the
/// orthographic stand-off), whose pose it replaces while it is engaged, and
/// before transforms propagate. Writes nothing while the orbit is whole.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn place_first_person_camera(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<LocalSettings>,
    heightmap: Option<Res<FinishedHeightMap>>,
    record: Option<Res<LiveAvatarRecord>>,
    mut first_person: ResMut<FirstPerson>,
    players: Query<
        (Entity, &Transform, Has<VehicleChassis>),
        (With<LocalPlayer>, Without<WorldCamera>),
    >,
    rigs: Query<
        (Entity, &ChildOf, &Transform, Ref<AvatarBody>),
        (With<RiggedRoot>, Without<WorldCamera>),
    >,
    mut cameras: Query<(&mut PanOrbitCamera, &mut Transform), IsWorldCamera>,
    mut eye_line: Local<Option<(Entity, Option<f32>)>>,
) {
    let was = first_person.blend;
    let target = match first_person.view {
        CameraView::Orbit => 0.0,
        CameraView::Eyes => 1.0,
    };
    let step = time.delta_secs() / cfg::FIRST_PERSON_BLEND_S;
    let blend = if target > first_person.blend {
        (first_person.blend + step).min(target)
    } else {
        (first_person.blend - step).max(target)
    };
    if blend != first_person.blend {
        first_person.blend = blend;
    }
    let player = players.single().ok();
    let eased = blend * blend * (3.0 - 2.0 * blend);
    let hide_on = player
        .filter(|_| eased >= cfg::FIRST_PERSON_HIDE_AT)
        .map(|(chassis, _, _)| chassis);
    if first_person.hidden != hide_on {
        if let Some(shown) = first_person.hidden {
            commands
                .entity(shown)
                .try_remove::<Propagate<RenderLayers>>();
        }
        if let Some(hidden) = hide_on {
            commands
                .entity(hidden)
                .try_insert(Propagate(RenderLayers::layer(cfg::FIRST_PERSON_BODY_LAYER)));
        }
        first_person.hidden = hide_on;
    }
    // Nothing to place while the orbit is whole. On the frame it becomes
    // whole its own pose is written: the crate may write nothing that frame,
    // which would leave the last step of the way out standing.
    if blend <= 0.0 && was <= 0.0 {
        return;
    }
    if blend <= 0.0 && first_person.leaving_turn.is_some() {
        first_person.leaving_turn = None;
    }
    let (Some((chassis_entity, chassis, vehicle)), Some(record)) = (player, record) else {
        return;
    };
    let Ok((mut orbit, mut transform)) = cameras.single_mut() else {
        return;
    };
    // The crate turns a drag's yaw the other way when the camera is upside
    // down as the drag starts. In the eyes the camera is the vehicle's, on
    // its back with it, and a drag turns the head about the vehicle's own
    // up: a look to the right is to the right whichever way up it is.
    if orbit.is_upside_down {
        orbit.is_upside_down = false;
    }
    let (Some(yaw), Some(pitch), Some(radius)) = (orbit.yaw, orbit.pitch, orbit.radius) else {
        return;
    };

    // The rig's eye line, read once a body (#1745): the landmark is the rest
    // pose's, so walking does not bob the view.
    let rig = rigs
        .iter()
        .find(|(_, parent, _, _)| parent.parent() == chassis_entity);
    let rig_eyes =
        rig.and_then(
            |(root, _, _, body)| match eye_line.filter(|(at, _)| *at == root) {
                Some((_, eyes)) if !body.is_changed() => eyes,
                _ => {
                    let eyes = body
                        .avatar
                        .rig
                        .landmarks()
                        .get(Landmark::EyeLine)
                        .map(|anchor| anchor.position.y);
                    *eye_line = Some((root, eyes));
                    eyes
                }
            },
        );
    let mount = EyeMount::of(
        &record.0.locomotion,
        vehicle,
        rig.map(|(_, _, root, _)| root.translation.y),
        rig_eyes,
    );
    let look_yaw = match (first_person.ahead_yaw, first_person.leaving_turn) {
        (Some(ahead), _) if vehicle => yaw - ahead,
        // On the way out of a vehicle's eyes the orbit counts from the world
        // again; the eyes' half holds the head's turn as they left.
        (None, Some(turn)) if vehicle => turn,
        _ => yaw,
    };
    let eyes = mount.pose(chassis, look_yaw, pitch);
    let pose = if eased >= 1.0 {
        eyes
    } else {
        let mut around = orbit_pose(yaw, pitch, radius, orbit.focus);
        if let Some(hm) = heightmap.as_deref() {
            around.translation = clear_of_terrain(orbit.focus, around.translation, &settings, hm);
        }
        if eased <= 0.0 {
            around
        } else {
            Transform {
                translation: around.translation.lerp(eyes.translation, eased),
                rotation: around.rotation.slerp(eyes.rotation, eased),
                ..default()
            }
        }
    };
    if *transform != pose {
        *transform = pose;
    }
}

/// Back to the orbit when the world is left (#1745): the attract scene and
/// the next sign-in start from it, with the body shown and the zoom whole.
pub(super) fn leave_first_person(
    mut commands: Commands,
    mut first_person: ResMut<FirstPerson>,
    mut cameras: Query<&mut PanOrbitCamera, IsWorldCamera>,
) {
    if let Some(hidden) = first_person.hidden.take() {
        commands
            .entity(hidden)
            .try_remove::<Propagate<RenderLayers>>();
    }
    if let Ok(mut orbit) = cameras.single_mut()
        && first_person.view == CameraView::Eyes
    {
        leave_eyes(first_person.as_mut(), &mut orbit, None);
    }
    if first_person.engaged() {
        *first_person = FirstPerson::default();
    }
}

/// `RenderLayers` carried down a hierarchy from a `Propagate` on its root,
/// in `PostUpdate` before visibility is checked, so a layer set this frame -
/// on a body hidden from its own eyes (#1745), on the item preview's stage
/// (#1288) - reaches its descendants, the ones spawned this frame among them,
/// before the world camera decides what it sees. Registered once, by
/// whichever plugin asks first.
pub(crate) fn register_render_layer_propagation(app: &mut App) {
    if app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
        return;
    }
    app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate))
        .configure_sets(
            PostUpdate,
            PropagateSet::<RenderLayers>::default().before(VisibilitySystems::CheckVisibility),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_panorbit_camera::PanOrbitCameraSystemSet;

    const AT_REST: f64 = cfg::ZOOM_PUSH_SETTLE_S + 0.01;

    /// **A wheel notch at the floor takes the camera in** (#1745), once the
    /// orbit has come to rest there; before it has, nothing does.
    #[test]
    fn a_notch_at_the_floor_takes_the_camera_into_the_eyes() {
        let mut push = ZoomPush::default();
        assert_eq!(push.feed(10.0, CameraView::Orbit, true, 0.0), None);
        assert_eq!(
            push.feed(10.0 + AT_REST, CameraView::Orbit, true, 1.0),
            Some(CameraView::Eyes)
        );
        let mut early = ZoomPush::default();
        assert_eq!(early.feed(10.0, CameraView::Orbit, true, 1.0), None);
        assert_eq!(early.feed(10.05, CameraView::Orbit, true, 1.0), None);
    }

    /// **A trackpad swipe that brings the orbit down to its floor does not
    /// fall through** (#1745): the input it sends as it arrives, and for as
    /// long as its momentum glides on after, counts for nothing; once it has
    /// stopped and the orbit has rested, it takes a notch's worth of pixels,
    /// not a few, to go in.
    #[test]
    fn a_swipe_down_to_the_floor_does_not_fall_through() {
        let mut push = ZoomPush::default();
        let mut t = 0.0;
        // Arriving and gliding on: sixty frames a second of 20-pixel steps
        // (0.1 each) for a whole second, three notches' worth.
        while t < 1.0 {
            assert_eq!(push.feed(t, CameraView::Orbit, true, 0.1), None, "at {t}");
            t += 1.0 / 60.0;
        }
        // Rested, the next few steps are a fraction of a notch.
        t += cfg::ZOOM_PUSH_SETTLE_S;
        for _ in 0..5 {
            t += 1.0 / 60.0;
            assert_eq!(push.feed(t, CameraView::Orbit, true, 0.1), None, "at {t}");
        }
        // A whole notch of them goes in.
        let mut went = None;
        for _ in 0..10 {
            t += 1.0 / 60.0;
            went = went.or(push.feed(t, CameraView::Orbit, true, 0.1));
        }
        assert_eq!(went, Some(CameraView::Eyes));
    }

    /// **Away from the floor, a zoom-in is only a zoom** (#1745); and a
    /// half-push left for a while starts again.
    #[test]
    fn only_a_zoom_at_the_floor_counts_and_a_stale_one_lapses() {
        let mut push = ZoomPush::default();
        assert_eq!(push.feed(0.0, CameraView::Orbit, false, 5.0), None);
        let mut push = ZoomPush::default();
        push.feed(0.0, CameraView::Orbit, true, 0.0);
        assert_eq!(push.feed(AT_REST, CameraView::Orbit, true, 0.6), None);
        let later = AT_REST + cfg::ZOOM_PUSH_IDLE_S + 0.1;
        assert_eq!(push.feed(later, CameraView::Orbit, true, 0.6), None);
    }

    /// **From the eyes, a zoom-out brings the camera back and a zoom-in is
    /// nothing** (#1745).
    #[test]
    fn a_zoom_out_from_the_eyes_comes_back_and_a_zoom_in_stays() {
        let mut push = ZoomPush::default();
        assert_eq!(push.feed(0.0, CameraView::Eyes, true, 3.0), None);
        assert_eq!(
            push.feed(0.1, CameraView::Eyes, true, -1.0),
            Some(CameraView::Orbit)
        );
        let mut trackpad = ZoomPush::default();
        assert_eq!(trackpad.feed(0.0, CameraView::Eyes, true, -0.3), None);
        assert_eq!(
            trackpad.feed(0.02, CameraView::Eyes, true, -0.3),
            Some(CameraView::Orbit)
        );
    }

    /// The default person: a capsule 1.8 m tall.
    fn walker() -> LocomotionConfig {
        LocomotionConfig::Humanoid(Box::default())
    }

    /// **A person's eyes are their rig's eye line, on their axis, and look the
    /// orbit's way level with the world** (#1745). Without a rig to read, they
    /// are the agent's eye height.
    #[test]
    fn a_persons_eyes_are_on_their_axis_at_their_eye_line() {
        let feet = -0.9;
        let mount = EyeMount::of(&walker(), false, Some(feet), Some(1.66));
        assert_eq!(
            mount,
            EyeMount::OnFoot {
                above_centre: feet + 1.66
            }
        );
        let chassis =
            Transform::from_xyz(3.0, 10.0, -2.0).with_rotation(Quat::from_rotation_y(1.0));
        let pose = mount.pose(&chassis, 0.0, 0.0);
        assert!((pose.translation - Vec3::new(3.0, 10.76, -2.0)).length() < 1e-5);
        assert!(
            (pose.forward().as_vec3() - Vec3::NEG_Z).length() < 1e-5,
            "level, the orbit's yaw"
        );
        let pitched = mount.pose(&chassis, 0.0, 0.5);
        assert!(
            pitched.forward().y < -0.4,
            "a pitch looks down, as the orbit's does"
        );
        let fallback = EyeMount::of(&walker(), false, None, None);
        let expected = (cfg::EYE_HEIGHT_FRACTION - 0.5) * locomotion_total_height(&walker());
        assert!(
            matches!(fallback, EyeMount::OnFoot { above_centre } if (above_centre - expected).abs() < 1e-5)
        );
    }

    /// **A vehicle's eyes ride its nose and turn, pitch and bank with it**
    /// (#1745): straight ahead they look along it, and a nose pitched up 30
    /// degrees carries the eyes and their look up with it.
    #[test]
    fn a_vehicles_eyes_ride_its_nose() {
        let car = LocomotionConfig::Car(Box::default());
        let mount = EyeMount::of(&car, true, None, None);
        let EyeMount::Vehicle { at } = mount else {
            panic!("a vehicle's eyes are on the vehicle: {mount:?}");
        };
        assert!(at.z < -1.0 && at.y > 0.0, "the nose, near the top: {at:?}");
        let up = Transform::from_xyz(0.0, 5.0, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_6));
        let pose = mount.pose(&up, 0.0, 0.0);
        let forward = pose.forward().as_vec3();
        assert!(
            (forward - up.forward().as_vec3()).length() < 1e-5,
            "along the nose"
        );
        assert!(
            pose.translation.y > 5.0 + at.y,
            "the raised nose lifts the eyes"
        );
        let banked = Transform::from_rotation(Quat::from_rotation_z(0.4));
        let rolled = mount.pose(&banked, 0.0, 0.0);
        assert!(
            (rolled.up().as_vec3() - banked.up().as_vec3()).length() < 1e-5,
            "the bank too"
        );
        let turned = mount.pose(&Transform::IDENTITY, 0.5, 0.0);
        assert!(turned.forward().x < -0.4, "the head's turn is off the nose");
    }

    /// **The orbit's pose stands its radius from its focus, looking at it, and
    /// pitch puts it above** (#1745): what the move into the eyes starts from
    /// is where the orbit camera stands.
    #[test]
    fn the_orbit_pose_looks_at_its_focus_from_its_radius() {
        let focus = Vec3::new(1.0, 2.0, 3.0);
        let pose = orbit_pose(0.3, 0.4, 2.0, focus);
        assert!(((pose.translation - focus).length() - 2.0).abs() < 1e-5);
        let to_focus = (focus - pose.translation).normalize();
        assert!((pose.forward().as_vec3() - to_focus).length() < 1e-5);
        assert!(
            pose.translation.y > focus.y,
            "a positive pitch looks down from above"
        );
    }

    /// **A heading a turn away is taken the short way** (#1745): the orbit's
    /// yaw is unbounded, and a target a whole turn off spun the camera round.
    #[test]
    fn a_heading_is_taken_the_short_way_round() {
        use std::f32::consts::TAU;
        assert!((near_turn(0.1, 3.0 * TAU) - (3.0 * TAU + 0.1)).abs() < 1e-4);
        assert!((near_turn(-3.0, 3.0) - (TAU - 3.0)).abs() < 1e-4);
    }

    /// A world camera with its orbit resting at its floor round a person's
    /// chassis, which has one part hung under it; the view systems on a
    /// clock of 50 ms frames, either side of the orbit crate's set as the
    /// game orders them. The camera, the chassis and the part.
    fn eyes_app(orthographic: bool) -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(50),
        ));
        register_render_layer_propagation(&mut app);
        app.add_message::<MouseWheel>()
            .add_message::<PinchGesture>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<FirstPerson>()
            .init_resource::<OrbitHold>()
            .insert_resource(LocalSettings {
                orthographic_camera: orthographic,
                ..default()
            })
            .insert_resource(LiveAvatarRecord(crate::pds::AvatarRecord {
                lex_type: String::new(),
                body: Default::default(),
                locomotion: walker(),
                gait: None,
            }))
            .add_systems(
                PostUpdate,
                (
                    (switch_view, super::super::follow_projection_setting)
                        .chain()
                        .before(PanOrbitCameraSystemSet),
                    place_first_person_camera.after(PanOrbitCameraSystemSet),
                )
                    .chain(),
            );
        let floor = cfg::ZOOM_LOWER_LIMIT;
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                WorldCamera,
                // The game's bindings and envelope (`spawn_orbit_camera`).
                PanOrbitCamera {
                    yaw: Some(0.0),
                    pitch: Some(0.4),
                    radius: Some(floor),
                    target_yaw: 0.0,
                    target_pitch: 0.4,
                    target_radius: floor,
                    focus: Vec3::Y,
                    target_focus: Vec3::Y,
                    button_orbit: MouseButton::Right,
                    button_pan: MouseButton::Middle,
                    zoom_lower_limit: floor,
                    zoom_upper_limit: Some(cfg::ZOOM_UPPER_LIMIT),
                    pitch_lower_limit: Some(cfg::PITCH_LOWER_LIMIT),
                    pitch_upper_limit: Some(cfg::PITCH_UPPER_LIMIT),
                    ..default()
                },
                Transform::default(),
                Projection::Perspective(PerspectiveProjection::default()),
            ))
            .id();
        let chassis = app
            .world_mut()
            .spawn((Transform::from_translation(Vec3::Y), LocalPlayer))
            .id();
        let part = app
            .world_mut()
            .spawn((Transform::default(), ChildOf(chassis)))
            .id();
        (app, camera, chassis, part)
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    fn hidden_layer() -> RenderLayers {
        RenderLayers::layer(cfg::FIRST_PERSON_BODY_LAYER)
    }

    /// **V takes the camera into the eyes and out again, and leaves nothing
    /// behind** (#1745): through the perspective lens though the orthographic
    /// one is set, with the orbit's zoom held and its floor lowered, the body
    /// and what hangs under it on the layer the camera does not draw; and back
    /// out, every layer gone, the zoom and the floor given back, the
    /// orthographic lens again.
    #[test]
    fn v_goes_into_the_eyes_and_back_leaving_nothing_behind() {
        let (mut app, camera, chassis, part) = eyes_app(true);
        frames(&mut app, 2);
        assert!(matches!(
            app.world().get::<Projection>(camera),
            Some(Projection::Orthographic(_))
        ));

        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut app, 10);
        let fp = app.world().resource::<FirstPerson>();
        assert_eq!(fp.view(), CameraView::Eyes);
        let orbit = app.world().get::<PanOrbitCamera>(camera).unwrap();
        assert_eq!(orbit.zoom_sensitivity, 0.0, "the zoom is held");
        assert_eq!(
            orbit.pitch_lower_limit,
            Some(cfg::FIRST_PERSON_PITCH_LOWER_LIMIT)
        );
        assert!(matches!(
            app.world().get::<Projection>(camera),
            Some(Projection::Perspective(_))
        ));
        let eye = app.world().get::<Transform>(camera).unwrap().translation;
        let above = (cfg::EYE_HEIGHT_FRACTION - 0.5) * locomotion_total_height(&walker());
        assert!(
            (eye - (Vec3::Y + Vec3::Y * above)).length() < 1e-4,
            "the camera is at the eyes: {eye}"
        );
        for entity in [chassis, part] {
            assert_eq!(
                app.world().get::<RenderLayers>(entity),
                Some(&hidden_layer())
            );
        }

        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut app, 10);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
        assert!(!app.world().resource::<FirstPerson>().engaged());
        for entity in [chassis, part] {
            assert_eq!(
                app.world().get::<RenderLayers>(entity),
                None,
                "{entity} left layered"
            );
        }
        let orbit = app.world().get::<PanOrbitCamera>(camera).unwrap();
        assert_eq!(orbit.zoom_sensitivity, 1.0, "the zoom is given back");
        assert_eq!(orbit.pitch_lower_limit, Some(cfg::ORTHO_PITCH_LOWER_LIMIT));
        assert_eq!(
            orbit.target_pitch, 0.4,
            "the orbit's own pitch is given back"
        );
        assert!(matches!(
            app.world().get::<Projection>(camera),
            Some(Projection::Orthographic(_))
        ));
    }

    /// **A wheel notch at the orbit's floor goes in, and one out comes back**
    /// (#1745), read from the wheel itself beside the orbit crate - and not
    /// while a window has the pointer, or with the zoom switched off.
    #[test]
    fn the_wheel_goes_in_past_the_floor_and_out_again() {
        let notch = |y: f32| MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        };
        let (mut app, camera, _, _) = eyes_app(false);
        frames(&mut app, 5);
        app.world_mut().write_message(notch(1.0));
        frames(&mut app, 1);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Eyes
        );
        frames(&mut app, 3);
        app.world_mut().write_message(notch(-1.0));
        frames(&mut app, 1);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );

        // Over a window the wheel is egui's: the gate disables the orbit.
        frames(&mut app, 10);
        app.world_mut()
            .get_mut::<PanOrbitCamera>(camera)
            .unwrap()
            .enabled = false;
        app.world_mut().write_message(notch(1.0));
        frames(&mut app, 1);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );

        let (mut off, _, _, _) = eyes_app(false);
        off.world_mut()
            .resource_mut::<LocalSettings>()
            .zoom_into_first_person = false;
        frames(&mut off, 5);
        off.world_mut().write_message(notch(1.0));
        frames(&mut off, 1);
        assert_eq!(
            off.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
        off.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut off, 1);
        assert_eq!(
            off.world().resource::<FirstPerson>().view(),
            CameraView::Eyes,
            "V works with the zoom switched off"
        );
    }

    /// **Right-drag looks round from the eyes, and the wheel moves nothing
    /// there** (#1745), with the orbit crate itself in the loop: in the eyes
    /// it still turns its yaw and pitch under a drag, and so the eyes - to
    /// the right for a drag to the right, and up for a drag up, past the
    /// orbit's own floor - while a trackpad's nudge out, short of what
    /// leaves the eyes, which it would take for a zoom, leaves its radius
    /// at the floor. A notch out then comes back.
    #[test]
    fn a_right_drag_looks_round_from_the_eyes_and_the_wheel_holds() {
        use bevy::input::mouse::MouseMotion;

        let notch = |y: f32| MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y,
            window: Entity::PLACEHOLDER,
            phase: bevy::input::touch::TouchPhase::Moved,
        };
        let (mut app, camera, _) = crate_app();
        frames(&mut app, 5);
        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut app, 10);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Eyes
        );
        let eyes = *app.world().get::<Transform>(camera).unwrap();
        assert!(
            (eyes.forward().as_vec3() - Vec3::NEG_Z).length() < 1e-3,
            "level, the orbit's way: {:?}",
            eyes.forward()
        );

        // The crate turns a window's width into a whole turn of yaw and its
        // height into half a turn of pitch: this drag is an eighth of a turn
        // right and an eighth up.
        let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        buttons.press(MouseButton::Right);
        for _ in 0..4 {
            app.world_mut().write_message(MouseMotion {
                delta: Vec2::new(40.0, -45.0),
            });
            app.update();
        }
        let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        buttons.release(MouseButton::Right);
        frames(&mut app, 40);
        let looked = *app.world().get::<Transform>(camera).unwrap();
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let expected = Vec3::new(0.5, half, -0.5);
        assert!(
            (looked.forward().as_vec3() - expected).length() < 1e-2,
            "right and up: {:?}",
            looked.forward()
        );
        assert!(
            (looked.translation - eyes.translation).length() < 1e-4,
            "the head turns where it stands"
        );

        app.world_mut().write_message(MouseWheel {
            unit: MouseScrollUnit::Pixel,
            y: -40.0,
            ..notch(0.0)
        });
        frames(&mut app, 10);
        let orbit = app.world().get::<PanOrbitCamera>(camera).unwrap();
        assert_eq!(
            (orbit.radius, orbit.target_radius),
            (Some(cfg::ZOOM_LOWER_LIMIT), cfg::ZOOM_LOWER_LIMIT),
            "the zoom held"
        );
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Eyes
        );
        app.world_mut().write_message(notch(-1.0));
        frames(&mut app, 1);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
    }

    /// [`eyes_app`] with the orbit crate itself in the loop, taking its input
    /// from a 1280 x 720 window it is told is its own. The camera and the
    /// chassis.
    fn crate_app() -> (App, Entity, Entity) {
        use bevy::input::mouse::MouseMotion;
        use bevy_panorbit_camera::{ActiveCameraData, PanOrbitCameraPlugin};

        let (mut app, camera, chassis, _) = eyes_app(false);
        let screen = Vec2::new(1280.0, 720.0);
        app.add_plugins(PanOrbitCameraPlugin)
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .add_message::<MouseMotion>()
            .insert_resource(ActiveCameraData {
                entity: Some(camera),
                viewport_size: Some(screen),
                window_size: Some(screen),
                manual: true,
            });
        (app, camera, chassis)
    }

    /// V.
    fn toggle(app: &mut App) {
        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
    }

    /// The player's body becomes a car, its chassis turned to `heading`.
    fn board(app: &mut App, chassis: Entity, heading: f32) {
        app.world_mut()
            .resource_mut::<LiveAvatarRecord>()
            .0
            .locomotion = LocomotionConfig::Car(Box::default());
        app.world_mut().entity_mut(chassis).insert((
            VehicleChassis,
            Transform::from_translation(Vec3::Y).with_rotation(Quat::from_rotation_y(heading)),
        ));
    }

    /// The way the camera looks over the ground, as the orbit's yaw counts it.
    fn looking(app: &App, camera: Entity) -> f32 {
        heading(app.world().get::<Transform>(camera).unwrap()).expect("not looking straight down")
    }

    /// Whether two yaws are the same way round, whole turns apart or not.
    fn same_way(a: f32, b: f32) -> bool {
        (near_turn(a, b) - b).abs() < 0.02
    }

    /// **Out of a vehicle's eyes the view leaves the way it looked** (#1745).
    /// The eyes board a car facing one way, the car turns, and the head looks
    /// nearly behind it. On every frame of the way out the camera looks the
    /// way the eyes did - the eyes' half once read the orbit's yaw as the
    /// head's turn on the car, turning it by the car's first heading - and
    /// the orbit, counted from the world again, has nothing to swing round
    /// to: taking the car's heading the near way from the head's turn spun
    /// it most of a turn.
    #[test]
    fn out_of_a_vehicles_eyes_the_view_leaves_the_way_it_looked() {
        let (mut app, camera, chassis) = crate_app();
        board(&mut app, chassis, 1.2);
        frames(&mut app, 5);
        toggle(&mut app);
        frames(&mut app, 30);
        let turn = 3.0;
        app.world_mut()
            .get_mut::<PanOrbitCamera>(camera)
            .unwrap()
            .target_yaw += turn;
        frames(&mut app, 30);
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .unwrap()
            .rotation = Quat::from_rotation_y(-0.5);
        frames(&mut app, 2);
        let eyes = looking(&app, camera);
        assert!(same_way(eyes, -0.5 + turn), "the eyes look {eyes}");

        toggle(&mut app);
        for frame in 0..8 {
            frames(&mut app, 1);
            let now = looking(&app, camera);
            assert!(
                same_way(now, eyes),
                "{frame} frames out the camera looks {now}, the eyes looked {eyes}"
            );
            let orbit = app.world().get::<PanOrbitCamera>(camera).unwrap();
            assert!(
                (orbit.yaw.unwrap() - orbit.target_yaw).abs() < 1e-3,
                "{frame} frames out the orbit swings from {:?} to {}",
                orbit.yaw,
                orbit.target_yaw
            );
        }
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
    }

    /// **Off a vehicle inside the eyes, the view stays** (#1745): the car
    /// turns after the eyes board it, then the body becomes a person's. The
    /// eyes, on foot now and reading the orbit's yaw as the world's, went on
    /// looking the way the car had faced as they boarded it.
    #[test]
    fn off_a_vehicle_inside_the_eyes_the_view_stays() {
        let (mut app, camera, chassis) = crate_app();
        board(&mut app, chassis, 0.0);
        frames(&mut app, 5);
        toggle(&mut app);
        frames(&mut app, 30);
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .unwrap()
            .rotation = Quat::from_rotation_y(1.0);
        frames(&mut app, 2);
        let along = looking(&app, camera);
        assert!(same_way(along, 1.0), "along the nose: {along}");

        app.world_mut()
            .entity_mut(chassis)
            .remove::<VehicleChassis>();
        app.world_mut()
            .resource_mut::<LiveAvatarRecord>()
            .0
            .locomotion = walker();
        for frame in 0..8 {
            frames(&mut app, 1);
            let now = looking(&app, camera);
            assert!(same_way(now, along), "{frame} frames on foot: {now}");
        }
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Eyes
        );
    }

    /// **On its back, a vehicle's eyes still look right for a drag right**
    /// (#1745): the crate turns a drag's yaw the other way when the camera
    /// is upside down as the drag starts, and in the eyes the camera is the
    /// vehicle's, on its back with it - where the head turns about the
    /// vehicle's own up, so the drag looked left.
    #[test]
    fn on_its_back_a_vehicles_eyes_still_look_right_for_a_drag_right() {
        use bevy::input::mouse::MouseMotion;

        let (mut app, camera, chassis) = crate_app();
        board(&mut app, chassis, 0.0);
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .unwrap()
            .rotation = Quat::from_rotation_z(std::f32::consts::PI);
        frames(&mut app, 5);
        toggle(&mut app);
        frames(&mut app, 30);
        let before = app
            .world()
            .get::<PanOrbitCamera>(camera)
            .unwrap()
            .target_yaw;
        let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        buttons.press(MouseButton::Right);
        app.update();
        // A press is new for one frame, as the input plugin keeps it: the
        // crate reads which way up the camera is on that frame only.
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        for _ in 0..4 {
            app.world_mut().write_message(MouseMotion {
                delta: Vec2::new(40.0, 0.0),
            });
            app.update();
        }
        let after = app
            .world()
            .get::<PanOrbitCamera>(camera)
            .unwrap()
            .target_yaw;
        // Forty pixels a frame for four frames is an eighth of a turn to the
        // right, and a turn to the right lowers the head's yaw.
        assert!(
            after < before - 0.5,
            "a drag right turns the head right: {before} to {after}"
        );
    }

    /// **Two fingers held still count once** (#1745). A pinch's spread is
    /// measured against last frame's, as the crate's own tracker measures
    /// it: a touch keeps its last change in `previous_position` through a
    /// frame with no touch input, so the change counted again on every frame
    /// the fingers rested, and a small pinch at the floor went into the eyes
    /// by itself.
    #[test]
    fn two_fingers_held_still_count_once() {
        use bevy::input::touch::{TouchInput, TouchPhase, touch_screen_input_system};

        let (mut app, _, _, _) = eyes_app(false);
        app.init_resource::<Touches>()
            .add_message::<TouchInput>()
            .add_systems(PreUpdate, touch_screen_input_system);
        let finger = |id: u64, x: f32, phase: TouchPhase| TouchInput {
            phase,
            position: Vec2::new(x, 300.0),
            window: Entity::PLACEHOLDER,
            force: None,
            id,
        };
        frames(&mut app, 5);
        app.world_mut()
            .write_message(finger(0, 500.0, TouchPhase::Started));
        app.world_mut()
            .write_message(finger(1, 600.0, TouchPhase::Started));
        frames(&mut app, 1);
        // Thirty pixels apart: under half a notch.
        app.world_mut()
            .write_message(finger(1, 630.0, TouchPhase::Moved));
        frames(&mut app, 1);
        frames(&mut app, 10);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit,
            "two fingers at rest are not a push"
        );
    }

    /// **A pinch counts as the crate counts it** (#1745): the crate drops a
    /// pinch only while its own orbit or pan modifier is held - the pan's is
    /// the left Alt, `gate_camera_on_gui` - and the push took either Alt for
    /// one, so with the right Alt held a pinch zoomed the orbit and never
    /// reached the eyes.
    #[test]
    fn a_pinch_counts_as_the_crate_counts_it() {
        let pinch = |app: &mut App, held: KeyCode| {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.release_all();
            keyboard.press(held);
            app.world_mut().write_message(PinchGesture(0.2));
            frames(app, 1);
            app.world().resource::<FirstPerson>().view()
        };
        let (mut app, camera, _, _) = eyes_app(false);
        app.world_mut()
            .get_mut::<PanOrbitCamera>(camera)
            .unwrap()
            .modifier_pan = Some(KeyCode::AltLeft);
        frames(&mut app, 5);
        assert_eq!(
            pinch(&mut app, KeyCode::AltLeft),
            CameraView::Orbit,
            "under the pan's modifier"
        );
        assert_eq!(pinch(&mut app, KeyCode::AltRight), CameraView::Eyes);
    }

    /// **The way out ends on the orbit's own pose** (#1745): on the frame
    /// the move comes home the orbit's pose is written. The crate, with
    /// nothing left to ease, may write nothing that frame, and the last step
    /// of the way out stood there - here 7% of the way to the eyes, at frames
    /// of 70 ms, which do not divide the move into even steps as 50 ms do.
    #[test]
    fn the_way_out_ends_on_the_orbits_own_pose() {
        let (mut app, camera, _, _) = eyes_app(false);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(70),
        ));
        toggle(&mut app);
        frames(&mut app, 10);
        toggle(&mut app);
        frames(&mut app, 10);
        let orbit = app.world().get::<PanOrbitCamera>(camera).unwrap();
        let home = orbit_pose(
            orbit.yaw.unwrap(),
            orbit.pitch.unwrap(),
            orbit.radius.unwrap(),
            orbit.focus,
        );
        let pose = app.world().get::<Transform>(camera).unwrap();
        assert!(
            (pose.translation - home.translation).length() < 1e-4,
            "the camera stands at {}, the orbit at {}",
            pose.translation,
            home.translation
        );
        assert!(pose.rotation.angle_between(home.rotation) < 1e-4);
    }

    /// **The Avatar editor keeps the camera at the orbit** (#1745): looking
    /// out of a body while editing it makes no sense, so opening the editor
    /// leaves the eyes, and V does nothing while it is open.
    #[test]
    fn the_avatar_editor_keeps_the_orbit() {
        let (mut app, _, chassis, _) = eyes_app(false);
        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut app, 10);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Eyes
        );
        app.insert_resource(OrbitHold(true));
        frames(&mut app, 10);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
        assert_eq!(app.world().get::<RenderLayers>(chassis), None);
        app.world_mut()
            .resource_mut::<FirstPerson>()
            .request_toggle();
        frames(&mut app, 2);
        assert_eq!(
            app.world().resource::<FirstPerson>().view(),
            CameraView::Orbit
        );
    }
}
