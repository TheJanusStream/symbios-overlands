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

/// The Froude number unshifted travel walks at (#1193).
///
/// The record's `walk_speed` was named before the engine grew a speed axis,
/// and its 4.0 m/s default sits at Froude 1.81 — a RUN, which milestone #11's
/// diagnosis (symbios-avatar #325) measured every player holding constantly.
/// So the record's field is the **travel** speed the run key asks for, and
/// the walk is derived from the body instead: this is the engine's own
/// calibration point for a natural walking pace (`Stride::for_body` pins
/// pace 1.0 ≈ Froude 0.43 against the speed axis), read back through
/// `Speed::from_froude(..).metres_per_second(rig)` so a child walks slower
/// and a giant faster on the same dimensionless number — measured 1.73 m/s
/// on the default body, safely inside a walk band that tops out at 1.87
/// (the Froude-0.5 transition; anything brisker IS a run on this body).
/// Derived, not a record field, on purpose: the lexicon does not move, and
/// every remote peer derives the identical speed from the record and rig it
/// already has.
const WALK_FROUDE: f32 = 0.43;

/// Unshifted walk as a share of the record's travel speed, while this body
/// is still a naked capsule (#1193).
///
/// The derivation above needs the built rig, and a freshly spawned chassis
/// walks before its body lands. The share is the default body's own
/// derivation, measured by `the_derived_walk_is_a_walk_on_the_engines_own_
/// axis` — 1.73 m/s of 4.0 — which is that guard's job: the first value
/// written here was estimated off the viewer's pace scale instead (0.64) and
/// the control refuted it. It only steers the capsule for the build's second
/// or two, after which the rig answers.
const WALK_OF_TRAVEL_FALLBACK: f32 = 0.43;

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WaterState {
    Dry,
    /// Feet are below the water surface, head is above. `depth` is how
    /// much of the avatar's height (m) is submerged.
    Wading {
        depth: f32,
    },
    /// Head is below the water surface. `depth` is how far below the
    /// surface the avatar's centre is (m).
    Swimming {
        depth: f32,
    },
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
    camera: Query<&GlobalTransform, With<Camera3d>>,
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
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
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
                    symbios_avatar::Speed::from_froude(WALK_FROUDE)
                        .metres_per_second(&body.avatar.rig)
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
            let new_h = if any_input {
                let alpha = (p.acceleration.0 * dt).clamp(0.0, 1.0);
                current_h.lerp(desired, alpha)
            } else {
                // Snappy friction: collapse horizontal velocity to zero fast
                // (stops on a dime at the default `stop_damping`) instead of
                // coasting.
                let decay = (-p.stop_damping.0 * dt).exp();
                current_h * decay
            };
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
    let frozen = avatar_editor
        .map(|e| e.holds_avatar_still())
        .unwrap_or(false);
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
    /// held or released as `held` says.
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
    fn chassis_speeds(held: &[bool]) -> Vec<f32> {
        chassis_speeds_shifted(held, false)
    }

    /// As [`chassis_speeds`], with the run key held or not for the whole
    /// trajectory (#1193). No rigged body is spawned here, so the controller
    /// walks on [`WALK_OF_TRAVEL_FALLBACK`] — which is what makes the two
    /// converged speeds exactly predictable.
    fn chassis_speeds_shifted(held: &[bool], shift: bool) -> Vec<f32> {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<avian3d::collider_tree::ColliderTrees>();
        app.init_resource::<JumpQueued>();
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
        let mut speeds = Vec::with_capacity(held.len());
        for &down in held {
            {
                let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                if down {
                    keys.press(KeyCode::KeyW);
                } else {
                    keys.release(KeyCode::KeyW);
                }
            }
            app.world_mut()
                .resource_mut::<Time<Fixed>>()
                .advance_by(std::time::Duration::from_secs_f64(1.0 / HZ));
            app.world_mut()
                .run_system_once(apply_humanoid_walk)
                .expect("the walk controller runs");
            let velocity = app
                .world()
                .get::<LinearVelocity>(chassis)
                .expect("the chassis has a velocity")
                .0;
            speeds.push(Vec3::new(velocity.x, 0.0, velocity.z).length());
        }
        speeds
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
    #[test]
    #[ignore = "probe for engine #277: how fast can a player change speed"]
    fn probe_how_fast_a_player_can_change_speed() {
        // Held from rest, then released — the two extremes the controller
        // offers, since every other input (strafe, turn, wading) changes the
        // desired velocity by less than the whole of `walk_speed`.
        let start: Vec<bool> = std::iter::repeat_n(true, 64).collect();
        let stop: Vec<bool> = std::iter::repeat_n(true, 64)
            .chain(std::iter::repeat_n(false, 64))
            .collect();

        let accelerating = chassis_speeds(&start);
        let decelerating = chassis_speeds(&stop);

        println!(
            "accelerating from rest: {}",
            accelerating
                .iter()
                .take(12)
                .map(|speed| format!("{speed:.3}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!(
            "decelerating on release: {}",
            decelerating
                .iter()
                .skip(64)
                .take(12)
                .map(|speed| format!("{speed:.3}"))
                .collect::<Vec<_>>()
                .join(" ")
        );

        for (name, speeds) in [
            ("accelerate", &accelerating[..]),
            ("decelerate", &decelerating[64..]),
        ] {
            let (rate, at) = steepest(speeds);
            println!(
                "{name}: steepest {rate:.1} m/s^2 at {at:.2} m/s — a 0.7 m/s change at that \
                 rate takes {:.3} s",
                0.7 / rate.max(f32::EPSILON)
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

    /// The derivation the fallback stands in for (#1193): on a built default
    /// body, the walk the run key releases to is a WALK on the engine's own
    /// axis — below the walk-run transition — while the default record's
    /// travel speed is a run above it. Relations against the engine's own
    /// classifier, not millimetre thresholds.
    #[test]
    fn the_derived_walk_is_a_walk_on_the_engines_own_axis() {
        let rig = symbios_avatar::Rig::from_skeleton(
            &symbios_avatar::HumanoidParams::default()
                .skeleton(&symbios_avatar::Composites::default()),
        )
        .expect("the default body rigs");
        let walking = symbios_avatar::Speed::from_froude(WALK_FROUDE).metres_per_second(&rig);
        assert!(
            !symbios_avatar::Speed::new(&rig, walking).is_running(),
            "the derived walk reads as a run at {walking} m/s"
        );
        assert!(
            symbios_avatar::Speed::new(&rig, 4.0).is_running(),
            "the default travel speed stopped being a run"
        );
        // The fallback share tracks the real derivation on the default body,
        // so the capsule's second of walking is in family with the body that
        // lands on it.
        assert!(
            (walking / 4.0 - WALK_OF_TRAVEL_FALLBACK).abs() < 0.1,
            "the fallback share drifted from the default body's derivation: \
             {walking:.2} m/s of 4.0"
        );
    }
}
