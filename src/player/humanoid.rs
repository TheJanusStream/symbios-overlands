//! Humanoid preset — capsule rigid body with `LockedAxes` keeping it
//! upright, walk/wading/swim controller, jump impulse. Visual mesh comes
//! from the avatar's `visuals` generator tree; cosmetic root-level gait
//! animation (bounce / sway / head-turn) lives in [`super::gait`], while
//! per-limb articulation remains intentionally out of scope.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};
use crate::water::WaterSurfaces;

use super::HumanoidPreset;

/// The Froude number unshifted travel walks at (#1193, #1323).
///
/// The record's `walk_speed` was named before the engine grew a speed axis,
/// and its default sits far past the engine's Froude-0.5 walk-run transition
/// — a RUN, which milestone #11's diagnosis (symbios-avatar #325) measured
/// every player holding constantly. So the record's field is the **travel**
/// speed the run key asks for, and the walk is derived from the body instead,
/// read back through `Speed::from_froude(..).metres_per_second(rig)` so a
/// child walks slower and a giant faster on the same dimensionless number.
/// Derived, not a record field, on purpose: the lexicon does not move, and
/// every remote peer derives the identical speed from the record and rig it
/// already has.
///
/// **0.49 is the owner's walk (#1323): 1.85 m/s on the default body**, up
/// from the engine's natural-pace calibration point 0.43 (1.73 m/s), which
/// read a little slow in the app. It sits about 1% in speed under the
/// transition (1.87 m/s on that body), and the transition has no hysteresis
/// (`Speed::is_running` is `froude > 0.5`), so anything that jitters a
/// walker's pace upward by 1% draws a run. A local body's pace is avian's
/// own velocity and reads the constant exactly. A remote peer's is
/// differenced from its played-out transform, and that difference was the
/// risk: measured by `turning::probe_a_walking_peer_against_the_walk_run_
/// transition`, the playout itself is exact (Froude 0.4900 at 60 and 144 Hz,
/// jitter up to its whole render delay), and one long receiver frame of
/// 50–99 ms drew a run only because the fill divided by the wrong frame's
/// delta — fixed in `rigged::RiggedTrail`, after which no arm crosses 0.4932.
/// A body on a slope is the owner's eye to judge: the pace is planar, so a
/// slope moves it only through what the solver does to the planar velocity.
const WALK_FROUDE: f32 = 0.49;

/// Unshifted walk as a share of the record's travel speed, while this body
/// is still a naked capsule (#1193).
///
/// The derivation above needs the built rig, and a freshly spawned chassis
/// walks before its body lands. The share is the default body's own
/// derivation over the default record's run, measured by `the_derived_walk_
/// is_a_walk_on_the_engines_own_axis` — 1.848 m/s of 5.0 since #1323 (it was
/// 1.73 of 4.0) — which is that guard's job: the first value written here
/// was estimated off the viewer's pace scale instead (0.64) and the control
/// refuted it. It only steers the capsule for the build's second or two,
/// after which the rig answers.
const WALK_OF_TRAVEL_FALLBACK: f32 = 0.37;

/// The most the planar velocity may speed up, in m/s² (#1323).
///
/// The record's controller lerps the velocity toward the keys at
/// `acceleration` (12/s by default), which from standing is a step of
/// 39 m/s² — a player crossed the whole walking band in two fixed steps, and
/// a planted foot was held against a chassis that had left it behind. This
/// caps what the lerp may add along the velocity, so a start is a short ramp
/// the owner can see. A code constant, on top of the record, so it reaches
/// every avatar; the record's `acceleration` still shapes the approach under
/// it.
///
/// **9 m/s² is the owner's, by eye (#1323).** The sweep began from the
/// playbook's human load response (about 0.3 s from standing to a walk, §4,
/// about 6 m/s²), and 6 read too slow in the app. Measured through the real
/// controller (`speed_change::probe_how_fast_a_player_can_change_speed`):
/// 0.281 s from standing to 95% of the 1.85 m/s walk and 0.562 s to the
/// 5.0 m/s run, against 0.234 s for both unramped. The sweep (4 / 6 / 9
/// m/s², the slow-down 1.5x each) found no value better on every figure:
/// every ramp slides the planted soles less on a change of pace and on most
/// turns, and more on a start at the run, because the driver's 0.3 s eased
/// pace trails a ramp by a constant `rate × 0.3 s` for as long as the ramp
/// lasts; 4 m/s² brought the run start's dip back (pelvis −200 mm against
/// −120).
const SPEED_UP_LIMIT: f32 = 9.0;

/// The most the planar velocity may slow down, in m/s² (#1323). Half as
/// steep again as [`SPEED_UP_LIMIT`], so a stop still reads as a stop rather
/// than a coast — 0.188 s from a walk to 5% of it, 0.375 s from a run; the
/// record's `stop_damping` still shapes the tail under it.
const SLOW_DOWN_LIMIT: f32 = 13.5;

/// One fixed step of the planar velocity held to the ramp (#1323): the step
/// the controller proposed, `current → proposed`, with its part ALONG the
/// current velocity capped at [`SPEED_UP_LIMIT`] and [`SLOW_DOWN_LIMIT`], and
/// its part across it — the turn — left as the controller asked.
///
/// **Along the velocity, not on the speed.** A cap on the speed's magnitude
/// alone flips a reversal: the lerp's direction swings through the zero
/// crossing in one step while the capped magnitude is still large, so the
/// body would leap from walking forward to walking backward. Capping the
/// along component instead slows the body to rest down its own line and
/// starts it back up the other way, continuously.
///
/// Under 0.1 m/s — the speed under which the facing calls travel
/// directionless — the along axis is the keys' own direction instead: from
/// standing there is no velocity to measure along, and an exponential stop
/// leaves a crumb pointing wherever it pointed, which would otherwise let a
/// start at right angles to it through uncapped.
fn ramped(current: Vec3, proposed: Vec3, toward: Vec3, dt: f32) -> Vec3 {
    let axis = if current.length_squared() > 0.01 {
        current.normalize()
    } else {
        toward.normalize_or_zero()
    };
    if axis == Vec3::ZERO {
        return proposed;
    }
    let change = proposed - current;
    let along = change.dot(axis);
    let across = change - axis * along;
    current + axis * along.clamp(-SLOW_DOWN_LIMIT * dt, SPEED_UP_LIMIT * dt) + across
}

/// Update-side jump edge latch (#852). The drive systems run in
/// `FixedUpdate` (64 Hz) but a key's `just_pressed` edge lives for one
/// *render* frame: at 120/144 Hz many render frames execute zero fixed
/// steps, so a Space tap frequently evaporated before any fixed step
/// sampled it — and a hitchy frame running 2+ steps saw the same edge
/// in each, double-firing the impulse. [`latch_jump_input`] converts
/// the render-frame edge into this queued flag; the first fixed step
/// reads it and [`clear_jump_queue`] (chained right after the walk
/// system) wipes it, so exactly one step ever sees a given tap.
#[derive(Resource, Default)]
pub(super) struct JumpQueued(pub(super) bool);

/// Latch Space's render-frame press edge for the fixed step. Registered
/// under the same input gates as [`apply_humanoid_walk`] (egui keyboard
/// focus, visuals-row selection, guard modal), so typing a space in chat
/// never queues a jump for the moment focus returns.
pub(super) fn latch_jump_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut queued: ResMut<JumpQueued>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        queued.0 = true;
    }
}

/// Wipe the jump queue at the end of every fixed step — chained after
/// [`apply_humanoid_walk`] and deliberately NOT input-gated: whether the
/// walk system consumed the edge, ignored it (mid-air, swimming), or was
/// gated off entirely, a queued tap must never outlive the first fixed
/// step that had the chance to act on it.
pub(super) fn clear_jump_queue(mut queued: ResMut<JumpQueued>) {
    if queued.0 {
        queued.0 = false;
    }
}

/// Classification of the humanoid's relationship to the water surface
/// directly beneath them. Drives the three locomotion modes — walking on
/// land, slowed wading with feet under water, and free 3D swimming with
/// gravity overridden once the head is fully submerged.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum WaterState {
    #[default]
    Dry,
    /// Feet are below the water surface, head is above. `depth` is how
    /// much of the avatar's height (m) is submerged.
    Wading { depth: f32 },
    /// Head is below the water surface. `depth` is how far below the
    /// surface the avatar's centre is (m).
    Swimming { depth: f32 },
}

/// Classify the avatar's relationship to the water column at its XZ
/// position. The avatar is treated as a vertical line segment of length
/// `height` centred on `chassis_y` — its feet at `chassis_y - height/2`
/// and head at `chassis_y + height/2`. The classifier samples
/// [`WaterSurfaces::surface_at`] at the avatar's XZ to locate the
/// containing surface, then compares feet / head against that surface Y.
///
/// Returns [`WaterState::Dry`] when no water surface contains the
/// avatar's column — the same fall-through used when the player walks
/// outside every pond's footprint.
pub fn humanoid_water_state(
    chassis_y: f32,
    chassis_xz: Vec2,
    height: f32,
    water_surfaces: &WaterSurfaces,
) -> WaterState {
    let Some((_, surface_y)) = water_surfaces.surface_at(chassis_xz) else {
        return WaterState::Dry;
    };
    let half = height * 0.5;
    let feet_y = chassis_y - half;
    let head_y = chassis_y + half;
    if feet_y >= surface_y {
        WaterState::Dry
    } else if head_y >= surface_y {
        WaterState::Wading {
            depth: surface_y - feet_y,
        }
    } else {
        WaterState::Swimming {
            depth: surface_y - chassis_y,
        }
    }
}

/// Publish the local humanoid's water classification for the UI (#1241
/// f160).
///
/// Deliberately its own ungated `Update` system rather than a write inside
/// [`apply_humanoid_walk`]: the drive systems stand down whenever an egui
/// text field has focus (#821), so a banner fed from there would blink out
/// the moment the swimmer clicked into chat — and "the mode indicator
/// disappears while you type" is a worse lie than no indicator.
///
/// Guarded (#879): the resource is written only when the classification
/// actually changes, so an every-frame system does not mark it changed
/// forever.
/// The local humanoid chassis, as one param so the publisher stays under
/// clippy's argument budget.
pub(super) type LocalHumanoid<'w, 's> =
    Query<'w, 's, (Entity, &'static GlobalTransform), (With<LocalPlayer>, With<HumanoidPreset>)>;

pub(super) fn publish_movement_facts(
    live: Option<Res<LiveAvatarRecord>>,
    water_surfaces: Res<WaterSurfaces>,
    query: LocalHumanoid,
    bodies: Query<(&ChildOf, &bevy_symbios_avatar::AvatarBody), With<super::rigged::RiggedRoot>>,
    camera: Query<&GlobalTransform, crate::camera::IsWorldCamera>,
    mut published: ResMut<crate::player::LocalMovement>,
) {
    let mut facts = crate::player::LocalMovement::default();
    // The camera's own submersion, not the avatar's (#1241 f160): a
    // third-person orbit camera dips under the surface by itself, and the
    // water plane is back-face culled, so from below there is nothing at
    // all to see.
    if let Ok(cam) = camera.single() {
        let eye = cam.translation();
        facts.camera_submerged = water_surfaces
            .surface_at(Vec2::new(eye.x, eye.z))
            .is_some_and(|(_, surface_y)| eye.y < surface_y);
    }
    if let (Some(live), Ok((entity, global_tf))) = (live.as_deref(), query.single())
        && let LocomotionConfig::Humanoid(p) = &live.0.locomotion
    {
        let pos = global_tf.translation();
        facts.water = humanoid_water_state(
            pos.y,
            Vec2::new(pos.x, pos.z),
            p.total_height(),
            &water_surfaces,
        );
        // The same derivation `apply_humanoid_walk` uses for the unshifted
        // walk, off the same rig — read here so the locomotion editor and
        // the walk cannot disagree about what "walk" means (#1241 f168).
        facts.derived_walk = bodies
            .iter()
            .find(|(child_of, _)| child_of.parent() == entity)
            .map(|(_, body)| derived_walk_speed(&body.avatar.rig));
    }
    // A non-humanoid body is not in the water and has no derived walk: a
    // vehicle preset has its own buoyancy and its own keys.
    if *published != facts {
        *published = facts;
    }
}

/// The unshifted walk this rig walks at (m/s) — the engine's own
/// calibration point, [`WALK_FROUDE`], read back through the speed axis so
/// a child walks slower and a giant faster on the same dimensionless
/// number.
pub fn derived_walk_speed(rig: &symbios_avatar::Rig) -> f32 {
    symbios_avatar::Speed::from_froude(WALK_FROUDE).metres_per_second(rig)
}

/// True when the record's `walk_speed` — which IS the run since #1193 —
/// has been tuned at or below the body's derived walk, so Shift does
/// nothing at all (#1241 f168).
///
/// `apply_humanoid_walk` takes `walking.min(travel)`, so below the derived
/// walk both branches collapse to the same number rather than inverting
/// the key. The code comment there acknowledged it; nothing surfaced it,
/// and the slider's range starts at 1.0 m/s against a default body that
/// walks at ~1.85, so the bottom of its travel silently disables a key.
pub fn run_key_is_dead(record_walk_speed: f32, derived_walk: f32) -> bool {
    record_walk_speed <= derived_walk
}

/// Locomotion controller. Three modes selected by [`humanoid_water_state`]:
///
/// * **Dry** — original land-walking behavior: WASD on the camera-flat
///   horizontal plane, snappy friction on release, Space jumps when a
///   downward raycast hits ground. **Shift runs** (#1193): unshifted
///   movement is a true walk derived from the body itself, and holding
///   either Shift travels at the record's `walk_speed` — see
///   [`WALK_FROUDE`] for why the record's field is the run.
/// * **Wading** — same as Dry but the chosen speed is multiplied by
///   `wading_speed_factor`. Jump still works while grounded so the avatar
///   can clamber out of the shallows.
/// * **Swimming** — gravity is overridden by lerping the full 3D linear
///   velocity toward `cam_forward * swim_speed`. Forward direction uses
///   the camera's full 3D look vector so swimming forward while pitched
///   downward dives. Right strafe is projected onto the horizontal plane
///   so strafing while looking up doesn't hop you up-and-sideways.
///   Space ascends, Shift / Ctrl descend, both add `swim_vertical_speed`
///   to the desired Y. The terrain-raycast jump is bypassed — Space is
///   already swim-ascend.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_humanoid_walk(
    live: Res<LiveAvatarRecord>,
    water_surfaces: Res<WaterSurfaces>,
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, crate::camera::IsWorldCamera>,
    mut query: Query<
        (
            Entity,
            &mut LinearVelocity,
            &mut Transform,
            &GlobalTransform,
        ),
        (With<LocalPlayer>, With<HumanoidPreset>),
    >,
    sensors: Query<Entity, With<Sensor>>,
    spatial_query: SpatialQuery,
    traveling: Option<Res<TravelingTo>>,
    jump_queued: Res<JumpQueued>,
    hold: Res<super::RigHold>,
    bodies: Query<(&ChildOf, &bevy_symbios_avatar::AvatarBody), With<super::rigged::RiggedRoot>>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Humanoid(p) = &live.0.locomotion else {
        return;
    };
    let Ok((entity, mut lin_vel, mut chassis_tf, global_tf)) = query.single_mut() else {
        return;
    };

    let chassis_pos = global_tf.translation();
    let total_height = p.total_height();
    let state = humanoid_water_state(
        chassis_pos.y,
        Vec2::new(chassis_pos.x, chassis_pos.z),
        total_height,
        &water_surfaces,
    );

    let cam_tf = camera.single().ok();
    let cam_forward = cam_tf.map(|t| t.forward().as_vec3()).unwrap_or(Vec3::NEG_Z);
    let cam_right_world = cam_tf.map(|t| t.right().as_vec3()).unwrap_or(Vec3::X);
    // Horizontal-plane derivatives for land/wade mode.
    let h_forward = Vec3::new(cam_forward.x, 0.0, cam_forward.z).normalize_or_zero();
    let h_right = Vec3::new(-h_forward.z, 0.0, h_forward.x);

    let dt = time.delta_secs().max(1e-4);
    let pressed_w = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp);
    let pressed_s = keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown);
    let pressed_d = keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight);
    let pressed_a = keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft);

    // Visuals-root facing target: tracked across modes so the avatar
    // always turns toward its movement direction (or for swimming,
    // toward the horizontal projection of its swim direction so the
    // model still faces forward even during a vertical-only ascent).
    let mut facing_target: Option<Vec3> = None;

    match state {
        WaterState::Dry | WaterState::Wading { .. } => {
            let speed_scale = if matches!(state, WaterState::Wading { .. }) {
                p.wading_speed_factor.0
            } else {
                1.0
            };
            // **Shift runs** (#1193). The record's `walk_speed` is the travel
            // speed — a run on the speed axis — and unshifted movement walks
            // at the body's own natural pace, never faster than the travel
            // (`min`, so a record tuned slower than its body's walk collapses
            // to one speed instead of inverting the key). Land only: while
            // swimming, Shift keeps meaning descend. The visible gait follows
            // for free — the rigged driver reads the chassis' actual speed
            // through the speed axis, and the walk↔run posture change rides
            // the eased pace (#1192) rather than the key edge.
            let running =
                keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
            let travel = p.walk_speed.0;
            let walking = bodies
                .iter()
                .find(|(child_of, _)| child_of.parent() == entity)
                .map_or(travel * WALK_OF_TRAVEL_FALLBACK, |(_, body)| {
                    derived_walk_speed(&body.avatar.rig)
                })
                .min(travel);
            let walk_speed = if running { travel } else { walking } * speed_scale;

            let mut desired = Vec3::ZERO;
            let mut any_input = false;
            if pressed_w {
                desired += h_forward;
                any_input = true;
            }
            if pressed_s {
                desired -= h_forward;
                any_input = true;
            }
            if pressed_d {
                desired += h_right;
                any_input = true;
            }
            if pressed_a {
                desired -= h_right;
                any_input = true;
            }
            let desired = desired.normalize_or_zero() * walk_speed;

            let current_h = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
            let proposed = if any_input {
                let alpha = (p.acceleration.0 * dt).clamp(0.0, 1.0);
                current_h.lerp(desired, alpha)
            } else {
                // Snappy friction: collapse horizontal velocity to zero fast
                // (stops on a dime at the default `stop_damping`) instead of
                // coasting.
                let decay = (-p.stop_damping.0 * dt).exp();
                current_h * decay
            };
            // The ramp (#1323), after the record's own approach so its
            // `acceleration` and `stop_damping` still shape the curve under
            // the cap. Land and wading only — swimming keeps its unramped
            // lerp — and planar only: the jump's vertical is untouched.
            let new_h = ramped(current_h, proposed, desired, dt);
            lin_vel.0.x = new_h.x;
            lin_vel.0.z = new_h.z;

            if new_h.length_squared() > 0.01 {
                facing_target = Some(new_h.normalize());
            }

            // The Update-side latch, not `just_pressed` (#852): the edge
            // only lives one render frame, which frequently contains zero
            // fixed steps on >64 Hz displays. `clear_jump_queue` (chained
            // after this system) wipes the flag each step, so a tap fires
            // at most once and a mid-air tap can't fire on a later landing.
            if jump_queued.0 {
                let origin = chassis_pos + Vec3::Y * 0.05;
                let feet_distance = total_height * 0.5 + 0.1;
                // Exclude self + every sensor so a gateway veil / portal never
                // counts as ground for the jump check (#813) —
                // see [`super::ground_ray_filter`].
                let filter = super::ground_ray_filter(entity, sensors.iter());
                let grounded = spatial_query
                    .cast_ray(origin, Dir3::NEG_Y, feet_distance, true, &filter)
                    .is_some();
                if grounded {
                    let delta_v = p.jump_impulse.0 / p.mass.0.max(1.0);
                    lin_vel.0.y += delta_v;
                }
            }
        }
        WaterState::Swimming { .. } => {
            // 3D forward = full camera direction, so swimming forward while
            // pitched down dives. Right is the camera's right vector with
            // its Y component flattened so strafing stays in a horizontal
            // band relative to the body, not the head's tilt.
            let forward = cam_forward.normalize_or_zero();
            let right = Vec3::new(cam_right_world.x, 0.0, cam_right_world.z).normalize_or_zero();
            let mut desired = Vec3::ZERO;
            if pressed_w {
                desired += forward;
            }
            if pressed_s {
                desired -= forward;
            }
            if pressed_d {
                desired += right;
            }
            if pressed_a {
                desired -= right;
            }
            let mut desired = desired.normalize_or_zero() * p.swim_speed.0;
            // Vertical control on top of the planar swim direction so a
            // diagonal "WSpace" surfaces while still moving forward.
            if keyboard.pressed(KeyCode::Space) {
                desired.y += p.swim_vertical_speed.0;
            }
            // Ctrl is deliberately NOT a swim-down key on wasm (#839):
            // W+Ctrl while swimming is the browser's close-tab chord,
            // preventDefault cannot intercept it, and the session (plus
            // any unsaved edits) died with the tab. Shift and C cover
            // swim-down everywhere; native keeps Ctrl for muscle memory.
            // The Controls sheet rows in `ui::toolbar` mirror this —
            // change both together (#803).
            #[allow(unused_mut)]
            let mut swim_down = keyboard.pressed(KeyCode::ShiftLeft)
                || keyboard.pressed(KeyCode::ShiftRight)
                || keyboard.pressed(KeyCode::KeyC);
            #[cfg(not(target_arch = "wasm32"))]
            {
                swim_down = swim_down
                    || keyboard.pressed(KeyCode::ControlLeft)
                    || keyboard.pressed(KeyCode::ControlRight);
            }
            if swim_down {
                desired.y -= p.swim_vertical_speed.0;
            }

            let alpha = (p.acceleration.0 * dt).clamp(0.0, 1.0);
            lin_vel.0 = lin_vel.0.lerp(desired, alpha);

            // Face the horizontal projection of swim direction so the
            // avatar's mesh keeps a sensible orientation even on vertical
            // input. Skip when swim direction is purely vertical (looking
            // straight up / down with no WASD).
            let h = Vec3::new(desired.x, 0.0, desired.z);
            if h.length_squared() > 0.01 {
                facing_target = Some(h.normalize());
            }
        }
    }

    // Tangent flow current. While wading or swimming, a non-zero
    // `flow_strength` on the surface pushes the avatar along its
    // steepest-descent direction, scaled by submerged depth so a
    // shin-deep wader feels less push than a fully-immersed swimmer.
    // Query at feet position so wading avatars (chassis above the
    // waterline) still see the surface they're standing in.
    if matches!(
        state,
        WaterState::Wading { .. } | WaterState::Swimming { .. }
    ) {
        let feet_pos = Vec3::new(
            chassis_pos.x,
            chassis_pos.y - total_height * 0.5,
            chassis_pos.z,
        );
        if let Some(q) = water_surfaces.query(feet_pos)
            && q.flow_strength > 0.0
            && q.flow_dir != Vec3::ZERO
        {
            // Cap the contributing depth at the avatar's height so an
            // arbitrarily deep pond doesn't accelerate the swimmer past
            // any sane velocity.
            let depth = q.depth.min(total_height);
            lin_vel.0 += q.flow_dir * q.flow_strength * depth * dt;
        }
    }

    // Rotate the chassis transform to face the movement direction. The
    // physics body has all three rotation axes locked, so writing the
    // rotation here only steers the visual; Avian's solver keeps the
    // capsule axis-aligned regardless. Apply the slerp to the chassis
    // transform itself so the entire avatar visuals tree (a child of
    // chassis) follows.
    //
    // Skipped while the avatar-edit freeze holds the chassis (#852):
    // this is a raw `Transform` write, which `LockedAxes::ALL_LOCKED`
    // cannot constrain — with the Avatar window open and no row
    // selected the drive gates (deliberately selection-scoped, see
    // `super::avatar_visuals_row_selected`) let this system run, and
    // WASD slewed the "frozen" avatar's facing mid-edit.
    let frozen = hold.still;
    if let Some(facing) = facing_target
        && !frozen
    {
        let target = Transform::IDENTITY.looking_to(facing, Vec3::Y).rotation;
        let turn_alpha = (p.turn_rate.0 * dt).clamp(0.0, 1.0);
        chassis_tf.rotation = chassis_tf.rotation.slerp(target, turn_alpha);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::water::{WaterPlane, WaterSurfaces};

    fn pond(y: f32, half: f32) -> WaterSurfaces {
        WaterSurfaces {
            planes: vec![WaterPlane {
                world_from_local: Transform::from_xyz(0.0, y, 0.0),
                local_half_extents: Vec2::splat(half),
                flow_strength: 0.0,
                owner: WaterPlane::NO_OWNER,
            }],
        }
    }

    #[test]
    fn dry_when_outside_every_pond() {
        let surfaces = pond(0.0, 5.0);
        // Avatar at (100, 0) is outside the pond's XZ rectangle.
        let s = humanoid_water_state(0.0, Vec2::new(100.0, 0.0), 1.8, &surfaces);
        assert_eq!(s, WaterState::Dry);
    }

    #[test]
    fn dry_when_feet_above_surface() {
        let surfaces = pond(0.0, 50.0);
        // Chassis at y = 5, height 1.8 → feet at 4.1, head at 5.9 → both above.
        let s = humanoid_water_state(5.0, Vec2::ZERO, 1.8, &surfaces);
        assert_eq!(s, WaterState::Dry);
    }

    #[test]
    fn wading_when_feet_submerged_head_above() {
        let surfaces = pond(0.0, 50.0);
        // Chassis at y = 0.5, height 1.8 → feet at -0.4 (under), head at 1.4 (above).
        let s = humanoid_water_state(0.5, Vec2::ZERO, 1.8, &surfaces);
        assert!(matches!(s, WaterState::Wading { depth } if (depth - 0.4).abs() < 1e-5));
    }

    #[test]
    fn swimming_when_head_submerged() {
        let surfaces = pond(0.0, 50.0);
        // Chassis at y = -2, height 1.8 → feet at -2.9, head at -1.1 → both below.
        let s = humanoid_water_state(-2.0, Vec2::ZERO, 1.8, &surfaces);
        assert!(matches!(s, WaterState::Swimming { depth } if (depth - 2.0).abs() < 1e-5));
    }

    #[test]
    fn wading_to_swim_at_chin_height() {
        let surfaces = pond(0.0, 50.0);
        // Chassis y = -0.05, height 1.8 → feet -0.95, head 0.85 → still wading.
        assert!(matches!(
            humanoid_water_state(-0.05, Vec2::ZERO, 1.8, &surfaces),
            WaterState::Wading { .. }
        ));
        // Pull just below the surface — head 0 is on the surface, classifier
        // treats `head_y >= surface_y` as still-Wading at the threshold.
        assert!(matches!(
            humanoid_water_state(-0.9, Vec2::ZERO, 1.8, &surfaces),
            WaterState::Wading { .. }
        ));
        // One step deeper → head submerges → swimming.
        assert!(matches!(
            humanoid_water_state(-0.95, Vec2::ZERO, 1.8, &surfaces),
            WaterState::Swimming { .. }
        ));
    }

    #[test]
    fn picks_highest_stacked_surface() {
        let surfaces = WaterSurfaces {
            planes: vec![
                WaterPlane {
                    world_from_local: Transform::from_xyz(0.0, 0.0, 0.0),
                    local_half_extents: Vec2::splat(100.0),
                    flow_strength: 0.0,
                    owner: WaterPlane::NO_OWNER,
                },
                WaterPlane {
                    world_from_local: Transform::from_xyz(0.0, 5.0, 0.0),
                    local_half_extents: Vec2::splat(2.0),
                    flow_strength: 0.0,
                    owner: WaterPlane::NO_OWNER,
                },
            ],
        };
        // Inside both — the elevated pond at y=5 wins. With chassis at y=4.5,
        // height 1.8 → feet 3.6 (below 5), head 5.4 (above 5) → wading the
        // upper pond. If the lower sea were chosen instead, head 5.4 above
        // the sea at y=0 would yield Dry.
        let s = humanoid_water_state(4.5, Vec2::new(1.0, 0.0), 1.8, &surfaces);
        assert!(matches!(s, WaterState::Wading { .. }));
        // Same chassis Y but outside the elevated pond's footprint — the
        // sea (y=0) is the only candidate, and the avatar's feet at 3.6 are
        // far above it, so the result is Dry.
        let s = humanoid_water_state(4.5, Vec2::new(50.0, 0.0), 1.8, &surfaces);
        assert_eq!(s, WaterState::Dry);
    }
}

// ---------------------------------------------------------------------------
// How fast the chassis can actually change speed (engine #277)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod speed_change {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::water::WaterSurfaces;
    use bevy::ecs::system::RunSystemOnce;
    use symbios_avatar::BodyPlan;

    /// The fixed step the drive systems run at.
    const HZ: f64 = 64.0;

    /// The speed the chassis reaches at each fixed step, in m/s, with `W`
    /// held or released as `held` says and the run key held or not for the
    /// whole trajectory (#1193). No rigged body is spawned here, so the
    /// controller walks on [`WALK_OF_TRAVEL_FALLBACK`] — which is what makes
    /// the two converged speeds exactly predictable.
    fn chassis_speeds_shifted(held: &[bool], shift: bool) -> Vec<f32> {
        let keys: Vec<&[KeyCode]> = held
            .iter()
            .map(|&down| -> &[KeyCode] {
                if down { &[KeyCode::KeyW] } else { &[] }
            })
            .collect();
        chassis_velocities(&keys, shift)
            .into_iter()
            .map(|velocity| Vec3::new(velocity.x, 0.0, velocity.z).length())
            .collect()
    }

    /// The chassis' velocity at each fixed step, with exactly the keys each
    /// step names held, and the run key held throughout or not. The one
    /// harness under every probe and test in this module.
    ///
    /// **Driven by [`apply_humanoid_walk`] itself**, not by a re-derivation
    /// of its arithmetic, which is the whole point of it (engine #277). The
    /// question that issue has to answer is how fast a *player* can change
    /// speed, and a probe that reimplements the controller's exponential in
    /// order to measure the controller's exponential answers nothing — this
    /// crate has caught three such probes measuring their own arithmetic in
    /// a month.
    ///
    /// Nothing here is stubbed but the world: no camera, so the controller
    /// falls back to its own `Vec3::NEG_Z` forward; no water planes, so the
    /// state is `Dry`; no queued jump, so the ground raycast never runs and
    /// `ColliderTrees::default` is enough to satisfy `SpatialQuery`.
    fn chassis_velocities(steps: &[&[KeyCode]], shift: bool) -> Vec<Vec3> {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<avian3d::collider_tree::ColliderTrees>();
        app.init_resource::<JumpQueued>();
        // Nothing held: this harness walks a body, it does not edit one.
        app.init_resource::<crate::player::RigHold>();
        app.insert_resource(WaterSurfaces { planes: Vec::new() });
        app.insert_resource(LiveAvatarRecord(AvatarRecord::wearing("3jzfcijpj2z2a")));
        app.insert_resource(Time::<Fixed>::from_hz(HZ));
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                HumanoidPreset,
                LinearVelocity::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();

        if shift {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::ShiftLeft);
        }
        let mut velocities = Vec::with_capacity(steps.len());
        for &held in steps {
            {
                let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                for key in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD] {
                    if held.contains(&key) {
                        keys.press(key);
                    } else {
                        keys.release(key);
                    }
                }
            }
            app.world_mut()
                .resource_mut::<Time<Fixed>>()
                .advance_by(std::time::Duration::from_secs_f64(1.0 / HZ));
            app.world_mut()
                .run_system_once(apply_humanoid_walk)
                .expect("the walk controller runs");
            velocities.push(
                app.world()
                    .get::<LinearVelocity>(chassis)
                    .expect("the chassis has a velocity")
                    .0,
            );
        }
        velocities
    }

    /// The steepest speed change in a trajectory, in m/s², and the speed it
    /// happened at.
    fn steepest(speeds: &[f32]) -> (f32, f32) {
        speeds
            .windows(2)
            .map(|pair| ((pair[1] - pair[0]).abs() * HZ as f32, pair[0]))
            .fold(
                (0.0, 0.0),
                |worst, now| if now.0 > worst.0 { now } else { worst },
            )
    }

    /// How fast a player can change speed, which is the question engine #277
    /// had to answer before it could be worked or dropped.
    ///
    /// That issue guessed the chassis was physics-driven with damping and so
    /// might never change speed abruptly enough to matter. It is not: the
    /// controller assigns the horizontal velocity itself every fixed step, an
    /// exponential lerp toward `walk_speed` while a key is held and an
    /// exponential decay on release, at `acceleration` and `stop_damping`.
    ///
    /// Measured 2026-08-19 on the default record (walk_speed 4.0,
    /// acceleration 12/s, stop_damping 20/s):
    ///
    /// ```text
    ///   accelerating from rest, m/s   0.750 1.359 1.854 2.257 2.584 2.849 ...
    ///   decelerating on release, m/s  2.926 2.141 1.566 1.146 0.838 0.613 ...
    ///
    ///   accelerate  steepest 39.0 m/s^2 — a 0.7 m/s change takes 0.018 s
    ///   decelerate  steepest 50.3 m/s^2 — a 0.7 m/s change takes 0.014 s
    /// ```
    ///
    /// Against the ramps #277's columns were taken on — 0.25 s over 0.7 m/s is
    /// 2.8 m/s², 0.05 s is 14.0 — the real chassis produces 2.8x to 3.6x the
    /// column filed as ABRUPT. A player crosses the whole walking band in two
    /// frames. So the wontfix that issue offered itself is not available.
    ///
    /// **Under the ramp (#1323)**, walk 1.85 / run 5.0, time to 95% of pace
    /// from rest and to 5% on release, and the steepest rate (m/s²):
    ///
    /// ```text
    ///                     walk start      walk stop       run start       run stop
    ///   no ramp           0.234 s  22.2   0.156 s  31.8   0.234 s  60.0   0.156 s  85.9
    ///   ramp 4 / 6        0.484 s   4.0   0.328 s   6.0   1.203 s   4.0   0.797 s   6.0
    ///   ramp 6 / 9        0.359 s   6.0   0.250 s   9.0   0.812 s   6.0   0.547 s   9.0  (too slow, by eye)
    ///   ramp 9 / 13.5     0.281 s   9.0   0.188 s  13.5   0.562 s   9.0   0.375 s  13.5  (shipped)
    /// ```
    #[test]
    #[ignore = "probe for engine #277 and #1323: how fast can a player change speed"]
    fn probe_how_fast_a_player_can_change_speed() {
        // Held from rest, then released — the two extremes the controller
        // offers, since every other input (strafe, turn, wading) changes the
        // desired velocity by less than the whole of `walk_speed`. At the
        // walk and at the run: two seconds each way, parts-per-million
        // converged before either end.
        let held: Vec<bool> = std::iter::repeat_n(true, 128)
            .chain(std::iter::repeat_n(false, 128))
            .collect();
        let list = |speeds: &[f32]| {
            speeds
                .iter()
                .take(12)
                .map(|speed| format!("{speed:.3}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        for (pace, shift) in [("walk", false), ("run", true)] {
            let speeds = chassis_speeds_shifted(&held, shift);
            let (starting, stopping) = speeds.split_at(128);
            let top = starting[127];
            // The first step from rest counts as a change from zero.
            let from_rest: Vec<f32> = std::iter::once(0.0)
                .chain(starting.iter().copied())
                .collect();
            let after = |speeds: &[f32], done: &dyn Fn(f32) -> bool| {
                speeds
                    .iter()
                    .position(|&speed| done(speed))
                    .map_or(f32::NAN, |step| (step + 1) as f32 / HZ as f32)
            };
            let to_pace = after(starting, &|speed| speed >= 0.95 * top);
            let to_rest = after(stopping, &|speed| speed <= 0.05 * top);
            let (up, up_at) = steepest(&from_rest);
            let (down, down_at) = steepest(&speeds[127..]);
            println!(
                "{pace} {top:.2} m/s: from rest {to_pace:.3} s to 95%, steepest {up:.1} m/s^2 \
                 at {up_at:.2} m/s; stopping {to_rest:.3} s to 5%, steepest {down:.1} m/s^2 at \
                 {down_at:.2} m/s"
            );
            println!("  from rest, m/s: {}", list(starting));
            println!("  on release, m/s: {}", list(stopping));
        }
    }

    /// The ramp, end to end through the controller (#1323): from standing to
    /// the walk and to the run, then released to rest, no step changes speed
    /// faster than its cap — and the cap still lets the body reach its pace,
    /// and stop, inside two seconds.
    ///
    /// Without the ramp the controller's steepest step was 22.2 m/s² from
    /// standing to the walk and 60.0 to the run, 31.8 and 85.9 on release.
    /// Under the owner's 9 / 13.5 m/s² (agreed by eye; 6 / 9 read too slow)
    /// the walk is 95% there in 0.281 s and the run in 0.562 s, and each
    /// stops to 5% in 0.188 s and 0.375 s.
    #[test]
    fn the_ramp_caps_every_change_and_still_reaches_pace_within_two_seconds() {
        let LocomotionConfig::Humanoid(p) =
            AvatarRecord::wearing("3jzfcijpj2z2a").locomotion.clone()
        else {
            panic!("the harness record is a humanoid");
        };
        let held: Vec<bool> = std::iter::repeat_n(true, 128)
            .chain(std::iter::repeat_n(false, 128))
            .collect();
        for (pace, shift, target) in [
            ("walk", false, p.walk_speed.0 * WALK_OF_TRAVEL_FALLBACK),
            ("run", true, p.walk_speed.0),
        ] {
            let speeds = chassis_speeds_shifted(&held, shift);
            let from_rest: Vec<f32> = std::iter::once(0.0)
                .chain(speeds[..128].iter().copied())
                .collect();
            let (up, up_at) = steepest(&from_rest);
            let (down, down_at) = steepest(&speeds[127..]);
            println!(
                "{pace}: steepest {up:.2} m/s^2 speeding up (at {up_at:.2} m/s), {down:.2} \
                 slowing (at {down_at:.2}); {:.3} m/s after 2 s, {:.4} after release",
                speeds[127], speeds[255]
            );
            assert!(
                up <= SPEED_UP_LIMIT * 1.001,
                "{pace}: sped up at {up:.2} m/s^2, over the {SPEED_UP_LIMIT} cap"
            );
            assert!(
                down <= SLOW_DOWN_LIMIT * 1.001,
                "{pace}: slowed at {down:.2} m/s^2, over the {SLOW_DOWN_LIMIT} cap"
            );
            assert!(
                (speeds[127] - target).abs() < 0.01 * target,
                "{pace}: {:.3} m/s after 2 s, short of {target:.3}",
                speeds[127]
            );
            assert!(
                speeds[255] < 0.01 * target,
                "{pace}: still moving at {:.3} m/s 2 s after release",
                speeds[255]
            );
        }
    }

    /// A reversal slows the body to rest down its own line and starts it back
    /// up the other way (#1323) — never a leap from forward to backward.
    ///
    /// The trap the ramp was written around: a cap on the speed's MAGNITUDE
    /// alone lets the lerp's direction swing through the zero crossing in one
    /// step while the capped speed is still large, which is a jump of twice
    /// that speed in a single step. So W for two seconds and then S, through
    /// the real controller at a walk and a run, and every step's change of
    /// velocity — as a vector, so a flip cannot hide inside an unchanged
    /// speed — must stay within one step of the steeper cap, and the
    /// velocity must pass through rest on the way. Without the ramp the
    /// controller's lerp steps 0.69 m/s at a walk and 1.88 at a run on the
    /// first step of the reversal.
    #[test]
    fn a_reversal_passes_through_rest_within_the_ramp() {
        let steps: Vec<&[KeyCode]> = std::iter::repeat_n(&[KeyCode::KeyW][..], 128)
            .chain(std::iter::repeat_n(&[KeyCode::KeyS][..], 128))
            .collect();
        let cap = SPEED_UP_LIMIT.max(SLOW_DOWN_LIMIT) / HZ as f32;
        for shift in [false, true] {
            let velocities = chassis_velocities(&steps, shift);
            let (step, worst) = velocities
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).length())
                .enumerate()
                .fold((0, 0.0f32), |worst, (at, jump)| {
                    if jump > worst.1 {
                        (at + 1, jump)
                    } else {
                        worst
                    }
                });
            assert!(
                worst <= cap * 1.001,
                "shift {shift}: step {step} changed the velocity by {worst:.3} m/s, \
                 over the ramp's {cap:.3} a step"
            );
            let slowest = velocities[128..]
                .iter()
                .map(|velocity| velocity.length())
                .fold(f32::MAX, f32::min);
            assert!(
                slowest <= cap,
                "shift {shift}: the reversal never came to rest — slowest {slowest:.3} m/s"
            );
            let (before, after) = (velocities[127], velocities[255]);
            assert!(
                before.dot(after) < 0.0,
                "shift {shift}: the reversal did not end travelling back: {before} then {after}"
            );
        }
    }

    /// The run key, end to end through the controller (#1193): W alone
    /// converges on the walk, W with Shift held converges on the record's
    /// travel speed. Driven by [`apply_humanoid_walk`] itself, like
    /// everything in this module — a probe re-deriving the arithmetic would
    /// measure its own arithmetic. The harness spawns no rigged body, so the
    /// walk is the fallback share of the record's own (seeded, per-DID)
    /// travel speed — read off the record rather than assumed 4.0.
    #[test]
    fn shift_is_the_run_key_and_unshifted_is_a_walk() {
        let LocomotionConfig::Humanoid(p) =
            AvatarRecord::wearing("3jzfcijpj2z2a").locomotion.clone()
        else {
            panic!("the harness record is a humanoid");
        };
        let travel = p.walk_speed.0;
        // Two seconds at 64 Hz: the controller's 12/s exponential is
        // parts-per-million converged long before the end.
        let walked = chassis_speeds_shifted(&[true; 128], false);
        let ran = chassis_speeds_shifted(&[true; 128], true);
        let (walk, run) = (*walked.last().unwrap(), *ran.last().unwrap());
        assert!(
            (run - travel).abs() < 0.02,
            "shift must travel at the record's speed: {run} against {travel}"
        );
        assert!(
            (walk - travel * WALK_OF_TRAVEL_FALLBACK).abs() < 0.02,
            "unshifted must walk the fallback share: {walk} of {travel}"
        );
        assert!(walk < run, "the walk outran the run: {walk} vs {run}");
    }

    /// #1241 f168. Sequence: drag "Run speed" to 1.5 m/s to make the
    /// avatar amble; afterwards Shift does nothing at all — no message, no
    /// disabled control, just a key that stopped working.
    /// `apply_humanoid_walk` takes `walking.min(travel)`, so below the
    /// body's derived walk both branches collapse to one number rather
    /// than inverting the key. The slider starts at 1.0 m/s against a
    /// default body that walks at ~1.85, so the bottom of its travel is a
    /// dead band — measured here off the real rig rather than asserted.
    #[test]
    fn the_run_key_dies_below_the_derived_walk_and_the_slider_can_reach_it() {
        let rig = symbios_avatar::Rig::from_skeleton(
            &symbios_avatar::HumanoidParams::default()
                .skeleton(&symbios_avatar::Composites::default()),
        )
        .expect("the default body rigs");
        let walk = derived_walk_speed(&rig);
        assert!(!run_key_is_dead(walk + 0.01, walk));
        assert!(run_key_is_dead(walk, walk), "equal collapses the min too");
        assert!(run_key_is_dead(walk - 0.01, walk));
        // The trap is reachable: the Run slider's lower bound is 1.0 m/s
        // (src/ui/avatar/locomotion/humanoid.rs), well under the walk.
        assert!(
            run_key_is_dead(1.0, walk),
            "the dead band must exist, or the warning has nothing to warn about"
        );
        assert!(!run_key_is_dead(
            crate::pds::HumanoidParams::default().walk_speed.0,
            walk
        ));
    }

    /// The derivation the fallback stands in for (#1193): on a built default
    /// body, the walk the run key releases to is a WALK on the engine's own
    /// axis — below the walk-run transition — while the default record's
    /// travel speed is a run above it. Relations against the engine's own
    /// classifier, not millimetre thresholds.
    ///
    /// The travel speed is read off the default record, not written here:
    /// this guard used to carry 4.0 twice, and its share tolerance was 0.1 —
    /// wide enough that #1323's move to walk 1.85 of run 5.0 (a share of
    /// 0.37) still passed against the old 0.43. The share is written to two
    /// places, so the tolerance is the rounding: 0.43 against 1.848 / 5.0
    /// reads 0.0604 off and fails; 0.37 reads 0.0004.
    #[test]
    fn the_derived_walk_is_a_walk_on_the_engines_own_axis() {
        let rig = symbios_avatar::Rig::from_skeleton(
            &symbios_avatar::HumanoidParams::default()
                .skeleton(&symbios_avatar::Composites::default()),
        )
        .expect("the default body rigs");
        let travel = crate::pds::HumanoidParams::default().walk_speed.0;
        let walking = symbios_avatar::Speed::from_froude(WALK_FROUDE).metres_per_second(&rig);
        assert!(
            !symbios_avatar::Speed::new(&rig, walking).is_running(),
            "the derived walk reads as a run at {walking} m/s"
        );
        assert!(
            symbios_avatar::Speed::new(&rig, travel).is_running(),
            "the default travel speed stopped being a run at {travel} m/s"
        );
        // The fallback share tracks the real derivation on the default body,
        // so the capsule's second of walking is in family with the body that
        // lands on it.
        let off = (walking / travel - WALK_OF_TRAVEL_FALLBACK).abs();
        assert!(
            off < 0.005,
            "the fallback share drifted from the default body's derivation: \
             {walking:.3} m/s of {travel} is {:.4}, {off:.4} off {WALK_OF_TRAVEL_FALLBACK}",
            walking / travel
        );
    }
}

// ---------------------------------------------------------------------------
// What a turn does to a rigged body (#1323)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod turning {
    use std::time::Duration;

    use super::*;
    use crate::pds::AvatarRecord;
    use crate::pds::avatar::EngineAvatarRecord;
    use crate::pds::avatar::wardrobe::engine_default_for_did;
    use crate::player::rigged::{INSTRUMENT_SEED, RiggedRoot, drive_frame, install_built_body};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
    use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarDriver, AvatarPose, Drive, Drove};
    use bevy_symbios_multiuser::prelude::TransformBuffer;
    use symbios_avatar::anim::driver::{Carriage, Driver, DriverConfig, Source};
    use symbios_avatar::{Limb, Zone};

    /// The fixed step the controller runs at — and, in this harness, the
    /// driver too.
    const HZ: f64 = 64.0;
    /// How long a body stands before its script's first key, so every script
    /// starts from a settled idle rather than from a body built mid-breath.
    const STAND_SECS: f32 = 1.0;
    /// How long every script's keys last after the stand. One length for all
    /// of them, so every comparison is the same frame count.
    const SCRIPT_SECS: f32 = 4.0;

    /// Keys held over consecutive spans, after [`STAND_SECS`] of none.
    struct Script {
        name: &'static str,
        spans: &'static [(f32, &'static [KeyCode])],
    }

    impl Script {
        /// The keys held at `t` seconds into the run.
        fn keys_at(&self, t: f32) -> &'static [KeyCode] {
            let mut from = STAND_SECS;
            if t < from {
                return &[];
            }
            for &(secs, keys) in self.spans {
                if t < from + secs {
                    return keys;
                }
                from += secs;
            }
            &[]
        }
    }

    const W: KeyCode = KeyCode::KeyW;
    const S: KeyCode = KeyCode::KeyS;
    const D: KeyCode = KeyCode::KeyD;
    const SHIFT: KeyCode = KeyCode::ShiftLeft;

    /// The scripts, control first.
    ///
    /// **W then W+D is 45°, not 90°.** A humanoid's camera orbits freely —
    /// only a vehicle inherits the chassis' yaw (`camera::follow_local_player`)
    /// — so the keys are camera-fixed and W+D asks for the diagonal. The
    /// quarter turn is W then D, carried alongside.
    ///
    /// The last two change pace rather than direction, with Shift held inside
    /// one span: the engine's replica chassis (symbios-avatar `tests/driver.rs`)
    /// carries the same two under the same names, so its figures and these can
    /// be read line against line. Under the run-throughout pass (`shift`) both
    /// are simply a straight run.
    const SCRIPTS: [Script; 7] = [
        Script {
            name: "straight W (control)",
            spans: &[(SCRIPT_SECS, &[W])],
        },
        Script {
            name: "W 2 s then W+D (45 deg)",
            spans: &[(2.0, &[W]), (2.0, &[W, D])],
        },
        Script {
            name: "W 2 s then D (90 deg)",
            spans: &[(2.0, &[W]), (2.0, &[D])],
        },
        Script {
            name: "W 2 s then S (reversal)",
            spans: &[(2.0, &[W]), (2.0, &[S])],
        },
        Script {
            name: "D from standing",
            spans: &[(SCRIPT_SECS, &[D])],
        },
        Script {
            name: "W 2 s then Shift (walk to run)",
            spans: &[(2.0, &[W]), (2.0, &[W, SHIFT])],
        },
        Script {
            name: "Shift+W 2 s then W (run to walk)",
            spans: &[(2.0, &[W, SHIFT]), (2.0, &[W])],
        },
    ];

    /// One arm of the bisection: what differs from the app as shipped.
    #[derive(Clone, Copy)]
    struct Treatment {
        name: &'static str,
        /// The record's `turn_rate` where this arm overrides it — the owner's
        /// in-app Locomotion knob, replicated through the record the
        /// controller actually reads.
        turn_rate: Option<f32>,
        /// What carries the body's root. `Own` is the ledger-off control: on
        /// flat ground with no jump, the foothold ledger is the ONLY thing
        /// the engine's `Carriage` decides (its other consumer gives a leap's
        /// flight height back, and nothing here leaves the ground).
        carriage: Carriage,
        /// The driver's pace lag, where this arm overrides it. A diagnostic,
        /// not a lever: zero hands the gait the chassis' raw speed, which is
        /// how to ask whether a reading is the lag's.
        pace_response: Option<f32>,
    }

    const SHIPPED: Treatment = Treatment {
        name: "as shipped",
        turn_rate: None,
        carriage: Carriage::Chassis,
        pace_response: None,
    };

    /// What one frame of a run left behind, read off the chassis and off the
    /// pose the body is actually drawn in.
    struct Frame {
        t: f32,
        keys: &'static [KeyCode],
        source: Source,
        /// Planar speed of the chassis, m/s.
        speed: f32,
        /// The eased pace the gait was built from this frame, m/s — the
        /// driver's own `speed()`, `None` whenever the gait is not carrying
        /// the body.
        paced: Option<f32>,
        /// Signed angle from where the body faces to where it travels,
        /// degrees, positive toward the body's own left; `None` below
        /// 0.05 m/s, where travel has no direction.
        angle: Option<f32>,
        /// The chassis' yaw, degrees — the fill's `facing`, in degrees.
        facing: f32,
        /// The left foot's lateral position less the right's, in the body
        /// frame (the pose's own), metres.
        separation: f32,
        /// Each foot's lateral displacement from its rest position, body
        /// frame, metres, left then right — positive toward the body's left.
        drift: [f32; 2],
        /// The left foot's fore-aft position less the right's, body frame,
        /// metres: the legs apart along the walk rather than across it.
        split: f32,
        /// The pelvis' height less its standing height, metres.
        root: f32,
        /// `Drove::strained`, as the fill + driver pair wrote it this frame.
        strained: bool,
        /// Every sole point of each foot in the WORLD, left foot first, through
        /// the transforms the body is rendered under: the joint's rest position
        /// dropped to the ground plane and carried by the ankle, which is how
        /// the engine's roll models a sole and the only point of a planted foot
        /// that is pinned (#1082).
        soles: [Vec<Vec3>; 2],
        /// Whether the gait the driver ran this frame has each foot in stance.
        down: [bool; 2],
    }

    /// One run, with the rest-pose figures its frames are read against.
    struct Run {
        frames: Vec<Frame>,
        /// Lateral separation of the two feet at rest, metres.
        rest_separation: f32,
        /// The pelvis' standing height and the leg's whole reach, metres —
        /// printed so a depth can be read against the body it happened to.
        standing: f32,
        reach: f32,
    }

    /// A test app with everything [`apply_humanoid_walk`] reads, the record's
    /// locomotion as given, and no chassis yet.
    fn walk_app(record: AvatarRecord) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<Image>();
        app.init_asset::<SkinnedMeshInverseBindposes>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<avian3d::collider_tree::ColliderTrees>();
        app.init_resource::<JumpQueued>();
        app.init_resource::<crate::player::RigHold>();
        app.insert_resource(WaterSurfaces { planes: Vec::new() });
        app.insert_resource(LiveAvatarRecord(record));
        app.insert_resource(Time::<Fixed>::from_hz(HZ));
        app
    }

    /// Installs `body` at atlas 64 under `chassis`, with its driver built from
    /// `config` on [`INSTRUMENT_SEED`]. Returns the rigged root.
    ///
    /// The turning harness passes the stop instrument's body
    /// (did:plc:stop-test), so every instrument reads one skeleton. Seed
    /// pinned by hand: `install_built_body` seeds off a process-wide counter,
    /// and `Driver::new` with the default config is `Driver::seeded` to the
    /// field.
    fn install_instrument_body(
        app: &mut App,
        chassis: Entity,
        offset: f32,
        body: &EngineAvatarRecord,
        config: DriverConfig,
    ) -> Entity {
        let avatar = symbios_avatar::Avatar::build_with(
            body,
            &symbios_avatar::AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        )
        .expect("the seeded default engine body builds");
        let mut built = Some(avatar);
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    let Some(avatar) = built.take() else {
                        return;
                    };
                    install_built_body(
                        &mut commands,
                        chassis,
                        offset,
                        avatar,
                        &[],
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    );
                },
            )
            .expect("runs");
        let mut roots = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<RiggedRoot>>();
        let root = roots
            .iter(app.world())
            .find(|(_, child_of)| child_of.parent() == chassis)
            .map(|(root, _)| root)
            .expect("a rigged root under the chassis");
        app.world_mut()
            .entity_mut(root)
            .insert(AvatarDriver(Driver::new(config, INSTRUMENT_SEED)));
        root
    }

    /// Drives `script` through the real controller and the real rigged body.
    ///
    /// **Keys against [`apply_humanoid_walk`] itself**, the shape of the
    /// `speed_change` probes: the controller's velocity lerp and facing slerp
    /// are what a turn IS here, and a harness re-deriving them would measure
    /// its own arithmetic. Then the real body under the chassis, driven by the
    /// app's own fill and the upstream driver through
    /// [`crate::player::rigged::drive_frame`], on [`INSTRUMENT_SEED`] — an
    /// unpinned seed is a moving number (#1194).
    ///
    /// **The one re-derived line is the integration**, `at += velocity · dt`:
    /// avian is not in this app, so nothing else moves the chassis. The
    /// controller assigns the planar velocity every step and the jump never
    /// fires, so the solver would integrate exactly this on flat open ground.
    ///
    /// Two differences from the app, both deliberate: the driver runs once per
    /// fixed step rather than once per render frame, and the chassis carries
    /// no `TransformInterpolation`, so the facing the fill reads steps at
    /// 64 Hz instead of being eased between steps. Neither changes what a turn
    /// asks of a planted foot, which is a matter of degrees over tenths of a
    /// second.
    fn turned(script: &Script, shift: bool, treatment: Treatment) -> Run {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        let LocomotionConfig::Humanoid(params) = &mut record.locomotion else {
            panic!("the harness record is a humanoid");
        };
        if let Some(rate) = treatment.turn_rate {
            params.turn_rate.0 = rate;
        }
        let offset = params.total_height() * 0.5;

        let mut app = walk_app(record);
        let chassis = app
            .world_mut()
            .spawn((
                LocalPlayer,
                HumanoidPreset,
                LinearVelocity::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();

        let shipped = DriverConfig::default();
        let root = install_instrument_body(
            &mut app,
            chassis,
            offset,
            &engine_default_for_did("did:plc:stop-test"),
            DriverConfig {
                carriage: treatment.carriage,
                pace_response: treatment.pace_response.unwrap_or(shipped.pace_response),
                ..shipped
            },
        );
        let rig = app
            .world()
            .get::<BuiltBody>(root)
            .expect("the body landed")
            .avatar
            .rig
            .clone();
        let pelvis = rig
            .joints
            .iter()
            .position(|joint| joint.parent.is_none())
            .expect("a root joint");
        let standing = rig.joints[pelvis].position.y;
        let limbs = [Limb::HindLeft, Limb::HindRight];
        let feet = limbs.map(|limb| rig.in_zone(Zone::Extremity(limb))[0]);
        let rest = feet.map(|foot| rig.joints[foot].position.x);
        let soles = limbs.map(|limb| {
            let joints = rig.extremity_joints(limb);
            (joints[0], joints[1..].to_vec())
        });
        let hung = *app
            .world()
            .get::<Transform>(root)
            .expect("the rigged root has its offset");

        let dt = Duration::from_secs_f64(1.0 / HZ);
        let frames = ((STAND_SECS + SCRIPT_SECS) * HZ as f32).round() as usize;
        let mut run = Run {
            frames: Vec::with_capacity(frames),
            rest_separation: rest[0] - rest[1],
            standing,
            reach: rig.limb_reach(Limb::HindLeft).unwrap_or(0.0),
        };
        for frame in 0..frames {
            let t = frame as f32 / HZ as f32;
            let keys = script.keys_at(t);
            {
                let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                for key in [W, S, D, KeyCode::KeyA, SHIFT] {
                    input.release(key);
                }
                for &key in keys {
                    input.press(key);
                }
                if shift {
                    input.press(SHIFT);
                }
            }
            app.world_mut().resource_mut::<Time<Fixed>>().advance_by(dt);
            app.world_mut()
                .run_system_once(apply_humanoid_walk)
                .expect("the walk controller runs");
            // The one re-derived line: see the doc.
            let velocity = app
                .world()
                .get::<LinearVelocity>(chassis)
                .expect("the chassis has a velocity")
                .0;
            let placed = {
                let mut transform = app
                    .world_mut()
                    .get_mut::<Transform>(chassis)
                    .expect("the chassis has a transform");
                transform.translation += velocity * dt.as_secs_f32();
                *transform
            };
            *app.world_mut()
                .get_mut::<GlobalTransform>(chassis)
                .expect("the chassis has a global transform") = GlobalTransform::from(placed);
            app.world_mut().resource_mut::<Time>().advance_by(dt);
            drive_frame(&mut app);

            let planar = Vec3::new(velocity.x, 0.0, velocity.z);
            let forward = placed.rotation * Vec3::NEG_Z;
            let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
            let left = Vec3::new(forward.z, 0.0, -forward.x);
            let posed = app
                .world()
                .get::<AvatarPose>(root)
                .expect("a pose")
                .0
                .forward(&rig);
            let lateral = feet.map(|foot| posed.positions[foot].x);
            let driver = app.world().get::<AvatarDriver>(root).expect("a driver");
            let rendered = GlobalTransform::from(placed) * GlobalTransform::from(hung);
            let gait = driver.speed().map(|speed| speed.gait(&rig));
            run.frames.push(Frame {
                t,
                keys,
                source: driver.source(),
                speed: planar.length(),
                paced: driver.speed().map(|speed| speed.metres_per_second(&rig)),
                angle: (planar.length() > 0.05)
                    .then(|| planar.dot(left).atan2(planar.dot(forward)).to_degrees()),
                facing: forward.x.atan2(forward.z).to_degrees(),
                separation: lateral[0] - lateral[1],
                drift: [lateral[0] - rest[0], lateral[1] - rest[1]],
                split: posed.positions[feet[0]].z - posed.positions[feet[1]].z,
                root: posed.positions[pelvis].y - standing,
                strained: app
                    .world()
                    .get::<Drove>(root)
                    .is_some_and(|drove| drove.strained),
                soles: soles.clone().map(|(ankle, sole)| {
                    sole.iter()
                        .map(|&joint| {
                            let at_rest = rig.joints[joint].position;
                            rendered.transform_point(
                                posed.positions[ankle]
                                    + posed.rotations[ankle]
                                        * (Vec3::new(at_rest.x, 0.0, at_rest.z)
                                            - rig.joints[ankle].position),
                            )
                        })
                        .collect()
                }),
                down: limbs.map(|limb| {
                    gait.as_ref().is_some_and(|gait| {
                        gait.limbs
                            .iter()
                            .position(|&of| of == limb)
                            .is_some_and(|index| gait.phase(index, driver.cycle()).is_stance())
                    })
                }),
            });
        }
        run
    }

    fn key_label(keys: &[KeyCode]) -> String {
        if keys.is_empty() {
            return "-".into();
        }
        keys.iter()
            .map(|key| match key {
                KeyCode::KeyW => "W",
                KeyCode::KeyS => "S",
                KeyCode::KeyD => "D",
                KeyCode::KeyA => "A",
                KeyCode::ShiftLeft => "Sh",
                _ => "?",
            })
            .collect::<Vec<_>>()
            .join("+")
    }

    /// The furthest any sole point slid through the world while it was down,
    /// in metres, over a window of frames.
    ///
    /// **Each point against itself, inside its own stance episode** — the
    /// ruler `rigged::tests::skate_through` uses (#277, #1082): a point counts
    /// as down while the gait has its foot in stance AND its own height is
    /// within 5 mm of the lowest it gets in the window, and each episode is
    /// anchored where the point first came down. Horizontal only.
    fn slide(frames: &[&Frame]) -> f32 {
        const CLEARANCE: f32 = 0.005;
        let mut worst = 0.0f32;
        for foot in 0..2 {
            let points = frames.first().map_or(0, |frame| frame.soles[foot].len());
            for point in 0..points {
                let floor = frames
                    .iter()
                    .map(|frame| frame.soles[foot][point].y)
                    .fold(f32::MAX, f32::min);
                let mut anchor: Option<Vec3> = None;
                for frame in frames {
                    let world = frame.soles[foot][point];
                    if frame.down[foot] && world.y - floor <= CLEARANCE {
                        let from = *anchor.get_or_insert(world);
                        worst =
                            worst.max(Vec3::new(world.x - from.x, 0.0, world.z - from.z).length());
                    } else {
                        anchor = None;
                    }
                }
            }
        }
        worst
    }

    /// The per-frame table, then one summary line per window.
    ///
    /// **Windowed, because a run-wide extreme is the start's.** Every script
    /// begins with a start from standing, and a turn later in the run is only
    /// readable against what the same run did before it — so the start
    /// (the first two seconds of keys) and the change (the last two) are
    /// summarised apart. For "D from standing" the start IS the turn.
    fn print_run(treatment: Treatment, script: &Script, shift: bool, run: &Run) {
        let pace = if shift { "run" } else { "walk" };
        println!("=== {} | {} | {pace} ===", treatment.name, script.name);
        println!(
            "      t keys src    speed paced  angle  facing  sep mm  splay L drift R drift split mm  root mm str"
        );
        for frame in &run.frames {
            println!(
                "  {:5.3} {:4} {:5} {:5.2} {:>5} {:>6} {:7.1} {:7.1} {:+6.1} {:+7.1} {:+7.1} {:+8.1} {:+8.1} {}",
                frame.t,
                key_label(frame.keys),
                format!("{:?}", frame.source),
                frame.speed,
                frame
                    .paced
                    .map_or_else(|| "-".to_string(), |paced| format!("{paced:.2}")),
                frame
                    .angle
                    .map_or_else(|| "-".to_string(), |angle| format!("{angle:+.1}")),
                frame.facing,
                frame.separation * 1000.0,
                (frame.separation - run.rest_separation) * 1000.0,
                frame.drift[0] * 1000.0,
                frame.drift[1] * 1000.0,
                frame.split * 1000.0,
                frame.root * 1000.0,
                u8::from(frame.strained),
            );
        }
        let change = STAND_SECS + 2.0;
        for (window, from, to) in [
            ("start", STAND_SECS, change),
            ("change", change, STAND_SECS + SCRIPT_SECS),
        ] {
            let frames: Vec<&Frame> = run
                .frames
                .iter()
                .filter(|frame| frame.t >= from && frame.t < to)
                .collect();
            let range = |value: &dyn Fn(&Frame) -> f32| {
                frames
                    .iter()
                    .map(|frame| value(frame))
                    .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
            };
            let max_angle = frames
                .iter()
                .filter_map(|frame| frame.angle)
                .fold(0.0f32, |worst, angle| worst.max(angle.abs()));
            let splay = range(&|frame| (frame.separation - run.rest_separation) * 1000.0);
            let split = range(&|frame| frame.split.abs() * 1000.0);
            let root = range(&|frame| frame.root * 1000.0);
            let strained = frames.iter().filter(|frame| frame.strained).count();
            let idled = frames
                .iter()
                .filter(|frame| frame.source != Source::Gait)
                .count();
            println!(
                "SUMMARY {} | {} | {pace} | {window}: |angle| max {max_angle:.1} deg; splay \
                 {:+.1}..{:+.1} mm; |split| max {:.1} mm; root {:+.1}..{:+.1} mm; slide {:.1} mm; \
                 strained {strained}/{}; not-gait frames {idled}",
                treatment.name,
                script.name,
                splay.0,
                splay.1,
                split.1,
                root.0,
                root.1,
                slide(&frames) * 1000.0,
                frames.len(),
            );
        }
    }

    /// What a turn does to a rigged body's legs and height (#1323), for
    /// every script at a walk and a run, under each bisection arm that needs
    /// no production edit.
    ///
    /// The owner's report: turning — before forward movement or during it —
    /// forces the legs apart and dips the body. The columns are the ones the
    /// question needs: the angle between where the body faces and where it
    /// travels (the controller's lag), the lateral separation of the feet in
    /// the body frame (the splay) and their fore-aft split, the pelvis against
    /// its standing height (the dip), `Drove::strained` — and the sole slide,
    /// which is what any fix for the other two trades them for. Without it an
    /// arm that removes both by skating reads as a cure: the ledger-off arm
    /// did exactly that on #1323, at 470.9 mm of slide on a straight start.
    ///
    /// Asserts nothing: it is an instrument, and the owner's eye in the app is
    /// the verdict (geometry before instruments). The arms that DO need a
    /// production edit — the fill feeding `Drive.turn` — are run by editing
    /// the fill and re-running this, one change at a time.
    #[test]
    #[ignore = "probe for #1323: prints the turning tables, asserts nothing"]
    fn probe_what_a_turn_does_to_the_legs() {
        let treatments = [
            SHIPPED,
            Treatment {
                name: "turn_rate 30",
                turn_rate: Some(30.0),
                ..SHIPPED
            },
            Treatment {
                name: "Carriage::Own (ledger off)",
                carriage: Carriage::Own,
                ..SHIPPED
            },
            Treatment {
                name: "pace_response 0 (diagnostic)",
                pace_response: Some(0.0),
                ..SHIPPED
            },
        ];
        let mut first = true;
        for treatment in treatments {
            for script in &SCRIPTS {
                for shift in [false, true] {
                    let run = turned(script, shift, treatment);
                    if first {
                        println!(
                            "BODY did:plc:stop-test, seed {INSTRUMENT_SEED}: pelvis stands {:.1} mm, \
                             leg reach {:.1} mm, feet {:.1} mm apart at rest",
                            run.standing * 1000.0,
                            run.reach * 1000.0,
                            run.rest_separation * 1000.0
                        );
                        first = false;
                    }
                    print_run(treatment, script, shift, &run);
                }
            }
        }
    }

    /// The turn the owner agreed by eye (#1323), held: a straight start from
    /// standing and a quarter turn at a walk keep the stance the app shipped
    /// on — the pelvis within 100 mm of the gait's own steady bob, the feet
    /// splayed by no more than the engine's across bound plus a margin, no
    /// strained frame, and the planted soles sliding under a ceiling.
    ///
    /// **The slide ceiling is the half that stops a skate passing for a
    /// cure.** Every lever that removes the dip and the splay trades them for
    /// sole slide; the ledger-off arm of #1323's bisection removed both and
    /// slid 470.9 mm on a straight start. So the traded quantity is guarded
    /// beside the ones the owner saw.
    ///
    /// The splay bound is the engine's: a hold yields past half of its limb's
    /// lateral offset (`HOLD_ACROSS`, symbios-avatar #337), and half of each
    /// foot's offset is a quarter of the rest stance; the margin takes it to
    /// 0.35 of the rest separation.
    ///
    /// Figures, walk, on this harness (did:plc:stop-test, seed 7, 64 Hz):
    ///
    /// ```text
    ///                        start pelvis  start slide  90° splay       90° pelvis  90° slide  strained
    ///   symbios-avatar 0.7.0   -741.6 mm     138.0 mm   -297.9..+7.1    -352.3 mm   147.6 mm   up to 7
    ///   agreed (0.8.0, walk    -182.9 mm     364.9 mm   -25.8..+25.1    -116.8 mm   339.3 mm   0
    ///     1.85, run 5.0,
    ///     ramp 9 / 13.5)
    ///   steady bob             -115.3 mm
    /// ```
    #[test]
    fn a_start_and_a_quarter_turn_keep_the_agreed_stance() {
        let control = turned(&SCRIPTS[0], false, SHIPPED);
        let quarter = turned(&SCRIPTS[2], false, SHIPPED);
        let change = STAND_SECS + 2.0;
        let within = |run: &Run, from: f32, to: f32| -> Vec<f32> {
            run.frames
                .iter()
                .filter(|frame| frame.t >= from && frame.t < to)
                .map(|frame| frame.root)
                .collect()
        };
        let steady = within(&control, change, STAND_SECS + SCRIPT_SECS)
            .into_iter()
            .fold(f32::MAX, f32::min);
        for (name, run, from, to) in [
            ("straight start", &control, STAND_SECS, change),
            ("quarter turn", &quarter, change, STAND_SECS + SCRIPT_SECS),
        ] {
            let frames: Vec<&Frame> = run
                .frames
                .iter()
                .filter(|frame| frame.t >= from && frame.t < to)
                .collect();
            let pelvis = frames
                .iter()
                .map(|frame| frame.root)
                .fold(f32::MAX, f32::min);
            let splay = frames
                .iter()
                .map(|frame| (frame.separation - run.rest_separation).abs())
                .fold(0.0f32, f32::max);
            let strained = frames.iter().filter(|frame| frame.strained).count();
            let slid = slide(&frames);
            let splay_bound = 0.35 * run.rest_separation;
            println!(
                "{name}: pelvis {:+.1} mm (steady bob {:+.1}); |splay| {:.1} mm (bound {:.1}); \
                 slide {:.1} mm; strained {strained}",
                pelvis * 1000.0,
                steady * 1000.0,
                splay * 1000.0,
                splay_bound * 1000.0,
                slid * 1000.0,
            );
            assert!(
                pelvis >= steady - 0.100,
                "{name}: the pelvis dipped to {:+.1} mm, over 100 mm under the steady bob {:+.1}",
                pelvis * 1000.0,
                steady * 1000.0
            );
            assert!(
                splay <= splay_bound,
                "{name}: the feet splayed {:.1} mm, over {:.1}",
                splay * 1000.0,
                splay_bound * 1000.0
            );
            assert_eq!(strained, 0, "{name}: {strained} strained frames");
            assert!(
                slid <= 0.420,
                "{name}: the planted soles slid {:.1} mm, over the 420 mm ceiling",
                slid * 1000.0
            );
        }
    }

    /// How long the peer probe runs, seconds, and where its steady window
    /// begins: three seconds after the key, when the sender's lerp, the
    /// playout's 0.1 s delay and the driver's 0.3 s pace lag have all long
    /// converged.
    const PEER_SECS: f64 = 12.0;
    const STEADY_FROM: f64 = 4.0;
    /// When a receiver's one long frame falls, seconds.
    const HITCH_AT: f64 = 7.0;
    /// The sender's render rate, Hz: its fixed-step broadcasts leave in bursts
    /// at its own frames, which is arrival jitter the app always has.
    const SENDER_HZ: f64 = 60.0;
    /// One-way latency, seconds. A constant shifts the whole stream and changes
    /// nothing a difference can see; it is here so jitter has room to sit.
    const LATENCY: f64 = 0.05;

    /// The receiving client, for the peer probe.
    #[derive(Clone, Copy)]
    struct Receiver {
        name: &'static str,
        /// Its render rate, Hz.
        render_hz: f64,
        /// Every packet's extra delay, drawn uniformly from `0..jitter`
        /// seconds on a fixed seed, delivered in order — a packet never
        /// overtakes the one sent before it, so jitter arrives as the bursts
        /// and gaps the playout was built to absorb.
        jitter: f64,
        /// One render frame at [`HITCH_AT`] that lasts this long, seconds.
        hitch: Option<f64>,
    }

    const CLEAN_60: Receiver = Receiver {
        name: "60 Hz, clean network",
        render_hz: 60.0,
        jitter: 0.0,
        hitch: None,
    };

    /// What a peer probe run read in its steady window.
    struct PeerRun {
        /// The walk the sender's controller asked for, m/s.
        walk: f32,
        /// The peer body's eased pace, as a Froude number, lowest and highest.
        froude: (f32, f32),
        /// Frames the peer's driver called a run, and how many times it went
        /// from walking to running.
        running: usize,
        flips: usize,
        frames: usize,
        /// The highest planar speed the fill handed the peer's driver, m/s.
        handed: f32,
        /// The same two figures for the sender's own body, the control: its
        /// velocity is avian's, not a difference.
        local_froude: f32,
        local_running: usize,
    }

    /// Walks the sender (the real controller, `W` held after the stand) and
    /// plays its broadcasts out on a remote peer's body the way the app does:
    /// one transform per fixed step, pushed into the app's own
    /// [`TransformBuffer`] under the app's own smoother config
    /// ([`crate::network::SmootherConfigRes::from_fixed_timestep`]) as they
    /// arrive at the receiver's frames, evaluated at the frame's time, written
    /// to a bare chassis with no `LinearVelocity`, and handed to the real fill
    /// and driver through [`drive_frame`].
    ///
    /// **The fill sees LAST frame's playout, as in the app**: the smoother
    /// writes a peer's `Transform` in `Update`, a bare transform reaches
    /// `GlobalTransform` only at `PostUpdate`'s propagation, and the fill
    /// reads `GlobalTransform`. So this writes the peer's `GlobalTransform`
    /// one frame behind its `Transform` —
    /// `rigged::tests::a_peers_speed_is_its_travel_over_the_frame_the_travel_took`
    /// pins that premise through Bevy's real schedule.
    fn walked_as_a_peer(body: &EngineAvatarRecord, receiver: Receiver) -> PeerRun {
        let record = AvatarRecord::wearing("3jzfcijpj2z2a");
        let LocomotionConfig::Humanoid(params) = &record.locomotion else {
            panic!("the harness record is a humanoid");
        };
        let offset = params.total_height() * 0.5;
        let mut app = walk_app(record);
        let local = app
            .world_mut()
            .spawn((
                LocalPlayer,
                HumanoidPreset,
                LinearVelocity::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        // A remote peer's chassis is a bare transform, so the fill differences
        // its position rather than reading a velocity.
        let peer = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let local_root =
            install_instrument_body(&mut app, local, offset, body, DriverConfig::default());
        let peer_root =
            install_instrument_body(&mut app, peer, offset, body, DriverConfig::default());
        let rig = app
            .world()
            .get::<BuiltBody>(peer_root)
            .expect("the body landed")
            .avatar
            .rig
            .clone();

        let smoothing = crate::network::SmootherConfigRes::from_fixed_timestep(1.0 / HZ).0;
        let mut buffer = TransformBuffer::default();
        let mut in_flight = std::collections::VecDeque::new();
        let mut arrived = 0.0f64;
        let mut noise = 0x9E37_79B9_7F4A_7C15u64;
        let mut shown = Transform::default();
        let (mut now, mut ticks, mut hitched) = (0.0f64, 0u64, false);
        let mut run = PeerRun {
            walk: derived_walk_speed(&rig),
            froude: (f32::MAX, f32::MIN),
            running: 0,
            flips: 0,
            frames: 0,
            handed: 0.0,
            local_froude: f32::MIN,
            local_running: 0,
        };
        let mut was_running = false;
        while now < PEER_SECS {
            let delta = match receiver.hitch {
                Some(long) if !hitched && now >= HITCH_AT => {
                    hitched = true;
                    long
                }
                _ => 1.0 / receiver.render_hz,
            };
            now += delta;
            // The sender's fixed steps due by this frame, each broadcasting
            // its chassis transform (`broadcast_local_state`).
            while (ticks + 1) as f64 / HZ <= now {
                ticks += 1;
                let stepped = ticks as f64 / HZ;
                {
                    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                    if stepped >= f64::from(STAND_SECS) {
                        input.press(W);
                    } else {
                        input.release(W);
                    }
                }
                app.world_mut()
                    .resource_mut::<Time<Fixed>>()
                    .advance_by(Duration::from_secs_f64(1.0 / HZ));
                app.world_mut()
                    .run_system_once(apply_humanoid_walk)
                    .expect("the walk controller runs");
                let velocity = app
                    .world()
                    .get::<LinearVelocity>(local)
                    .expect("the chassis has a velocity")
                    .0;
                let placed = {
                    let mut transform = app
                        .world_mut()
                        .get_mut::<Transform>(local)
                        .expect("the chassis has a transform");
                    transform.translation += velocity / HZ as f32;
                    *transform
                };
                *app.world_mut()
                    .get_mut::<GlobalTransform>(local)
                    .expect("the chassis has a global transform") = GlobalTransform::from(placed);
                // It leaves with the sender's frame, arrives after the
                // latency and its jitter, and never overtakes its predecessor.
                noise ^= noise << 13;
                noise ^= noise >> 7;
                noise ^= noise << 17;
                let extra = receiver.jitter * (noise >> 11) as f64 / (1u64 << 53) as f64;
                let departs = (stepped * SENDER_HZ).ceil() / SENDER_HZ;
                arrived = arrived.max(departs + LATENCY + extra);
                in_flight.push_back((arrived, placed.translation, placed.rotation));
            }
            // `handle_incoming_messages`, then `smooth_remote_transforms`.
            while in_flight.front().is_some_and(|&(at, ..)| at <= now) {
                let (_, position, rotation) = in_flight.pop_front().expect("checked");
                buffer.push_sample(position, rotation, now, &smoothing);
            }
            let before = shown;
            if let Some((position, rotation)) = buffer.smoothed_at(now, &smoothing) {
                shown = Transform::from_translation(position).with_rotation(rotation);
            }
            *app.world_mut()
                .get_mut::<Transform>(peer)
                .expect("the peer has a transform") = shown;
            *app.world_mut()
                .get_mut::<GlobalTransform>(peer)
                .expect("the peer has a global transform") = GlobalTransform::from(before);
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f64(delta));
            drive_frame(&mut app);

            if now < STEADY_FROM {
                continue;
            }
            let speed_of = |root: Entity| {
                app.world()
                    .get::<AvatarDriver>(root)
                    .expect("a driver")
                    .speed()
            };
            let paced = speed_of(peer_root);
            let froude = paced.map_or(0.0, |speed| speed.froude());
            let running = paced.is_some_and(|speed| speed.is_running());
            run.froude = (run.froude.0.min(froude), run.froude.1.max(froude));
            run.running += usize::from(running);
            run.flips += usize::from(running && !was_running);
            was_running = running;
            run.frames += 1;
            let handed = app
                .world()
                .get::<Drive>(peer_root)
                .expect("the peer carries a drive")
                .velocity;
            run.handed = run.handed.max(Vec2::new(handed.x, handed.z).length());
            let local = speed_of(local_root);
            run.local_froude = run
                .local_froude
                .max(local.map_or(0.0, |speed| speed.froude()));
            run.local_running += usize::from(local.is_some_and(|speed| speed.is_running()));
        }
        run
    }

    /// Whether a remote peer walking at the derived walk can read as a run
    /// (#1323, the owner's condition on WALK_FROUDE 0.49).
    ///
    /// The walk sits about 1% in speed under the engine's walk-run transition
    /// (Froude 0.5), which has no hysteresis. A local body's velocity is
    /// avian's; a PEER's is differenced from its played-out transform, and the
    /// driver's 0.3 s eased pace is the only filter between that difference
    /// and `Speed::is_running`. This walks a sender through the real
    /// controller on the default body and plays it out on a peer body the way
    /// the app does (see [`walked_as_a_peer`]), under the receiving
    /// conditions the playout itself defines: its render rates, the bursts and
    /// gaps it was built to absorb (in-order jitter up to its whole 0.1 s
    /// render delay), the sender's frame-grouped departures, and the frame
    /// times a client actually has — one dropped vsync frame, and the two
    /// main-thread stalls this app documents for a wasm avatar build
    /// (`rigged` module: 68 ms draft, 277 ms full atlas). Then a sweep of one
    /// long frame, to find where the flip begins.
    ///
    /// Measured 2026-09-10 (eased pace, Froude; the playout is exact: 0.4900
    /// at 60 and 144 Hz with jitter up to the whole render delay). While the
    /// fill divided a peer's displacement by THIS frame's delta, one long
    /// frame at 60 Hz read, at WALK_FROUDE 0.43 | 0.49:
    ///
    /// ```text
    ///   33.3 ms (a dropped vsync frame)  0.4347 | 0.4954
    ///   50 ms                            0.4410 | 0.5026  (a run, 3 frames)
    ///   68 ms (a wasm draft build)       0.4492 | 0.5119  (a run, 3 frames)
    ///   100 ms and longer                idle one frame, then Froude 16-22, at both
    /// ```
    ///
    /// The carrier was that lag, not the playout: a diagnostic handing the
    /// fill this frame's playout never crossed 0.4940. Divided by the delta
    /// the displacement was travelled over (`rigged::RiggedTrail`, #1323),
    /// at 0.49 every arm reads 0 flips — 33.3 ms 0.4920, 50 ms 0.4932, 68 ms
    /// 0.4927, 100 ms 0.4908, 277 ms 0.4900.
    ///
    /// Asserts nothing: `rigged::tests::a_peers_speed_is_its_travel_over_the_
    /// frame_the_travel_took` is the guard; this is the measurement the owner
    /// shipped the walk on.
    #[test]
    #[ignore = "probe for #1323: can a walking peer read as a run"]
    fn probe_a_walking_peer_against_the_walk_run_transition() {
        let body = EngineAvatarRecord::default();
        let mut receivers = vec![
            CLEAN_60,
            Receiver {
                name: "144 Hz, clean network",
                render_hz: 144.0,
                ..CLEAN_60
            },
            Receiver {
                name: "60 Hz, in-order jitter 0..25 ms",
                jitter: 0.025,
                ..CLEAN_60
            },
            Receiver {
                name: "60 Hz, in-order jitter 0..100 ms (the whole render delay)",
                jitter: 0.1,
                ..CLEAN_60
            },
            Receiver {
                name: "144 Hz, in-order jitter 0..100 ms",
                render_hz: 144.0,
                jitter: 0.1,
                ..CLEAN_60
            },
            Receiver {
                name: "60 Hz, one dropped frame (33.3 ms)",
                hitch: Some(2.0 / 60.0),
                ..CLEAN_60
            },
            Receiver {
                name: "60 Hz, a 68 ms stall (wasm draft build)",
                hitch: Some(0.068),
                ..CLEAN_60
            },
            Receiver {
                name: "60 Hz, a 277 ms stall (wasm full build)",
                hitch: Some(0.277),
                ..CLEAN_60
            },
        ];
        for millis in [
            20.0, 25.0, 33.3, 40.0, 50.0, 68.0, 100.0, 150.0, 200.0, 277.0,
        ] {
            receivers.push(Receiver {
                name: "sweep: one long frame at 60 Hz",
                hitch: Some(millis / 1000.0),
                ..CLEAN_60
            });
        }
        println!("WALK_FROUDE {WALK_FROUDE}; the walk-run transition is Froude 0.5");
        for receiver in receivers {
            let run = walked_as_a_peer(&body, receiver);
            println!(
                "PEER {}{}: walk {:.3} m/s; peer eased Froude {:.4}..{:.4}; running {}/{} frames, \
                 {} flips; fill handed up to {:.2} m/s | local control: Froude max {:.4}, running {}",
                receiver.name,
                receiver
                    .hitch
                    .map_or_else(String::new, |long| format!(" [{:.1} ms]", long * 1000.0)),
                run.walk,
                run.froude.0,
                run.froude.1,
                run.running,
                run.frames,
                run.flips,
                run.handed,
                run.local_froude,
                run.local_running,
            );
        }
    }
}
