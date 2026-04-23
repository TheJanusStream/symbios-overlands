//! Humanoid archetype — blocky biped rig, walk controller, and limb animator.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{AvatarBody, HumanoidPhenotype};
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};
use crate::world_builder::{OverlandsFoliageTasks, build_procedural_material};

use super::rover::with_tangents;
use super::{ChestBadge, HumanoidArchetype, HumanoidJoint, HumanoidVisualRoot};

#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild_humanoid_children(
    commands: &mut Commands,
    entity: Entity,
    phen: &HumanoidPhenotype,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    foliage_tasks: &mut OverlandsFoliageTasks,
    avatar_override: Option<&Handle<StandardMaterial>>,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    // Clamp every dimension so a malicious record cannot panic mesh
    // constructors. Bevy primitive ctors reject zero/negative/NaN inputs.
    let height = phen.height.0.clamp(0.4, 4.0);
    let tw = phen.torso_half_width.0.clamp(0.05, 1.0);
    let td = phen.torso_half_depth.0.clamp(0.05, 1.0);
    let head = phen.head_size.0.clamp(0.05, 1.0);
    let limb = phen.limb_thickness.0.clamp(0.03, 0.4);
    let arm_ratio = phen.arm_length_ratio.0.clamp(0.5, 1.5);
    let leg_ratio = phen.leg_length_ratio.0.clamp(0.3, 0.6);

    let body_mat = build_procedural_material(materials, foliage_tasks, &phen.body_material);
    let head_mat = build_procedural_material(materials, foliage_tasks, &phen.head_material);
    let limb_mat = build_procedural_material(materials, foliage_tasks, &phen.limb_material);
    let badge_mat = avatar_override.cloned().unwrap_or_else(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.9, 0.95),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    });

    let head_h = head;
    let torso_h = (height * 0.45).max(0.2);
    let leg_len = (height * leg_ratio).max(0.2);
    let arm_len = (torso_h * arm_ratio).max(0.15);
    // Capsule body's origin sits at the rigid-body centre; torso centre
    // sits at y = 0 in local space.
    let torso_y = 0.0;
    let head_y = torso_h * 0.5 + head_h * 0.5;
    let shoulder_y = torso_h * 0.45;
    let hip_y = -torso_h * 0.5;
    let shoulder_x = tw + limb * 0.5;
    let hip_x = tw * 0.6;

    let arm_mesh = meshes.add(with_tangents(
        Capsule3d::new(limb * 0.5, arm_len).mesh().build(),
    ));
    let leg_mesh = meshes.add(with_tangents(
        Capsule3d::new(limb * 0.6, leg_len).mesh().build(),
    ));
    let torso_mesh = meshes.add(with_tangents(
        Cuboid::new(tw * 2.0, torso_h, td * 2.0).mesh().build(),
    ));
    let head_mesh = meshes.add(with_tangents(
        Cuboid::new(head_h, head_h, head_h).mesh().build(),
    ));
    let show_badge = phen.show_badge;
    let badge_mesh = if show_badge {
        let badge_w = (tw * 1.6).min(tw * 2.0 - 0.02).max(0.05);
        let badge_h = (torso_h * 0.55).max(0.05);
        Some((
            meshes.add(with_tangents(
                Rectangle::new(badge_w, badge_h).mesh().build(),
            )),
            td,
        ))
    } else {
        None
    };

    commands.entity(entity).with_children(|root| {
        root.spawn((
            Transform::IDENTITY,
            Visibility::default(),
            HumanoidVisualRoot,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(torso_mesh),
                MeshMaterial3d(body_mat),
                Transform::from_xyz(0.0, torso_y, 0.0),
            ));
            parent.spawn((
                Mesh3d(head_mesh),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, head_y, 0.0),
            ));

            if let Some((badge_mesh, td)) = badge_mesh {
                parent.spawn((
                    Mesh3d(badge_mesh),
                    MeshMaterial3d(badge_mat),
                    Transform::from_xyz(0.0, torso_y, td + 0.01),
                    ChestBadge,
                ));
            }

            // Shoulders — rotating the joint swings the limb from its top
            // because the cylinder is offset downward by half its length.
            spawn_joint(
                parent,
                Vec3::new(-shoulder_x, shoulder_y, 0.0),
                HumanoidJoint {
                    phase_sign: 1.0,
                    phase_offset: 0.0,
                },
                arm_mesh.clone(),
                limb_mat.clone(),
                arm_len,
            );
            spawn_joint(
                parent,
                Vec3::new(shoulder_x, shoulder_y, 0.0),
                HumanoidJoint {
                    phase_sign: -1.0,
                    phase_offset: 0.0,
                },
                arm_mesh,
                limb_mat.clone(),
                arm_len,
            );

            // Hips — 180° out of phase with the arm on the same side so the
            // gait alternates (left arm forward ↔ left leg back).
            spawn_joint(
                parent,
                Vec3::new(-hip_x, hip_y, 0.0),
                HumanoidJoint {
                    phase_sign: -1.0,
                    phase_offset: 0.0,
                },
                leg_mesh.clone(),
                limb_mat.clone(),
                leg_len,
            );
            spawn_joint(
                parent,
                Vec3::new(hip_x, hip_y, 0.0),
                HumanoidJoint {
                    phase_sign: 1.0,
                    phase_offset: 0.0,
                },
                leg_mesh,
                limb_mat,
                leg_len,
            );
        });
    });
}

fn spawn_joint(
    parent: &mut ChildSpawnerCommands,
    position: Vec3,
    joint: HumanoidJoint,
    limb_mesh: Handle<Mesh>,
    limb_mat: Handle<StandardMaterial>,
    limb_length: f32,
) {
    parent
        .spawn((
            Transform::from_translation(position),
            Visibility::default(),
            joint,
        ))
        .with_children(|pivot| {
            pivot.spawn((
                Mesh3d(limb_mesh),
                MeshMaterial3d(limb_mat),
                Transform::from_xyz(0.0, -limb_length * 0.5, 0.0),
            ));
        });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn apply_humanoid_walk(
    live: Res<LiveAvatarRecord>,
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut query: Query<
        (Entity, &mut LinearVelocity, &GlobalTransform, &Children),
        (With<LocalPlayer>, With<HumanoidArchetype>),
    >,
    mut visual_roots: Query<&mut Transform, With<HumanoidVisualRoot>>,
    spatial_query: SpatialQuery,
    traveling: Option<Res<TravelingTo>>,
) {
    if traveling.is_some() {
        return;
    }
    let AvatarBody::Humanoid {
        kinematics,
        phenotype,
    } = &live.0.body
    else {
        return;
    };
    let Ok((entity, mut lin_vel, global_tf, children)) = query.single_mut() else {
        return;
    };

    let cam_forward = camera
        .single()
        .ok()
        .map(|t| t.forward().as_vec3())
        .unwrap_or(Vec3::NEG_Z);
    let forward = Vec3::new(cam_forward.x, 0.0, cam_forward.z).normalize_or_zero();
    let right = Vec3::new(-forward.z, 0.0, forward.x);

    let mut desired = Vec3::ZERO;
    let mut any_input = false;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        desired += forward;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        desired -= forward;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        desired += right;
        any_input = true;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        desired -= right;
        any_input = true;
    }
    desired = desired.normalize_or_zero() * kinematics.walk_speed.0;

    let dt = time.delta_secs().max(1e-4);
    let current_h = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
    let new_h = if any_input {
        let alpha = (kinematics.acceleration.0 * dt).clamp(0.0, 1.0);
        current_h.lerp(desired, alpha)
    } else {
        // Snappy friction: collapse horizontal velocity to zero fast so the
        // avatar stops on a dime instead of coasting.
        let decay = (-20.0 * dt).exp();
        current_h * decay
    };
    lin_vel.0.x = new_h.x;
    lin_vel.0.z = new_h.z;

    // Rotate the visual root to face movement. The physics body has all
    // rotation axes locked, so this is purely cosmetic — and exactly what
    // a traditional character controller does.
    if new_h.length_squared() > 0.01 {
        let facing = new_h.normalize();
        let target = Transform::IDENTITY.looking_to(facing, Vec3::Y).rotation;
        let turn_alpha = (12.0 * dt).clamp(0.0, 1.0);
        for child in children.iter() {
            if let Ok(mut tf) = visual_roots.get_mut(child) {
                tf.rotation = tf.rotation.slerp(target, turn_alpha);
            }
        }
    }

    if keyboard.just_pressed(KeyCode::Space) {
        let origin = global_tf.translation() + Vec3::Y * 0.05;
        let feet_distance = phenotype.height.0 * 0.5 + 0.1;
        let filter = SpatialQueryFilter::default().with_excluded_entities([entity]);
        let grounded = spatial_query
            .cast_ray(origin, Dir3::NEG_Y, feet_distance, true, &filter)
            .is_some();
        if grounded {
            let delta_v = kinematics.jump_impulse.0 / kinematics.mass.0.max(1.0);
            lin_vel.0.y += delta_v;
        }
    }
}

/// Procedural gait animation: swing each shoulder/hip joint in a sine
/// counter-rotation, scaled by horizontal speed. Legs are 180° out of
/// phase with their paired arm so the walk alternates naturally. When
/// stopped the joints smoothly slerp back to the idle (identity) pose.
#[allow(clippy::type_complexity)]
pub(super) fn animate_humanoid_limbs(
    time: Res<Time>,
    players: Query<(&LinearVelocity, &Children), With<HumanoidArchetype>>,
    visual_roots: Query<&Children, With<HumanoidVisualRoot>>,
    mut joints: Query<(&HumanoidJoint, &mut Transform)>,
) {
    const SWING_AMPLITUDE: f32 = 0.9;
    const PHASE_SPEED: f32 = 2.2;
    const IDLE_SLERP_RATE: f32 = 10.0;

    let dt = time.delta_secs().max(1e-4);
    let t = time.elapsed_secs();

    for (lin_vel, children) in players.iter() {
        let horiz = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
        let speed = horiz.length();
        let amplitude = SWING_AMPLITUDE * (speed / 4.0).clamp(0.0, 1.0);
        let phase = t * PHASE_SPEED * speed.max(0.0);

        for chassis_child in children.iter() {
            let Ok(root_children) = visual_roots.get(chassis_child) else {
                continue;
            };
            for joint_entity in root_children.iter() {
                let Ok((joint, mut tf)) = joints.get_mut(joint_entity) else {
                    continue;
                };
                if amplitude < 1e-3 {
                    let idle_alpha = (IDLE_SLERP_RATE * dt).clamp(0.0, 1.0);
                    tf.rotation = tf.rotation.slerp(Quat::IDENTITY, idle_alpha);
                } else {
                    let angle = (phase + joint.phase_offset).sin() * amplitude * joint.phase_sign;
                    tf.rotation = Quat::from_rotation_x(angle);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared respawn / recovery
// ---------------------------------------------------------------------------
