//! Humanoid preset — capsule rigid body with `LockedAxes` keeping it
//! upright, walk/wading/swim controller, jump impulse. Visual mesh comes
//! from the avatar's `visuals` generator tree (no procedural limb gait
//! animation in v1 — see the avatar-unification chainlink issue for the
//! intentional drop).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};
use crate::water::WaterSurfaces;

use super::HumanoidPreset;

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
///   downward raycast hits ground.
/// * **Wading** — same as Dry but `walk_speed` is multiplied by
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
    spatial_query: SpatialQuery,
    traveling: Option<Res<TravelingTo>>,
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
            let walk_speed = p.walk_speed.0 * speed_scale;

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
                // so the avatar stops on a dime instead of coasting.
                let decay = (-20.0 * dt).exp();
                current_h * decay
            };
            lin_vel.0.x = new_h.x;
            lin_vel.0.z = new_h.z;

            if new_h.length_squared() > 0.01 {
                facing_target = Some(new_h.normalize());
            }

            if keyboard.just_pressed(KeyCode::Space) {
                let origin = chassis_pos + Vec3::Y * 0.05;
                let feet_distance = total_height * 0.5 + 0.1;
                let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
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
            if keyboard.pressed(KeyCode::ShiftLeft)
                || keyboard.pressed(KeyCode::ShiftRight)
                || keyboard.pressed(KeyCode::ControlLeft)
                || keyboard.pressed(KeyCode::ControlRight)
            {
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
    if let Some(facing) = facing_target {
        let target = Transform::IDENTITY.looking_to(facing, Vec3::Y).rotation;
        let turn_alpha = (12.0 * dt).clamp(0.0, 1.0);
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
                },
                WaterPlane {
                    world_from_local: Transform::from_xyz(0.0, 5.0, 0.0),
                    local_half_extents: Vec2::splat(2.0),
                    flow_strength: 0.0,
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
