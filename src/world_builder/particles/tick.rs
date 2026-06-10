//! Per-frame particle integration: emitter motion tracking, particle
//! aging / forces / collision / fade / orientation, end-of-life
//! despawn.

use avian3d::prelude::*;
use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;

use crate::pds::AnimationFrameMode;

use super::atlas::{ParticleAtlasMeshes, current_frame_index};
use super::ramp::EmitterMaterialRamp;
use super::{EmitterMotionTracker, EmitterState, Particle, ParticleEmitter, lerp_unit};

/// Update each emitter's tracked world velocity. Walks up the parent
/// chain looking for a `LinearVelocity` (avian3d `RigidBody`); if none
/// is found, falls back to numerical differentiation of
/// `GlobalTransform::translation` over one frame. The first frame after
/// spawn primes `last_translation` and reports zero velocity to avoid
/// emitting a one-frame teleport spike.
pub fn update_emitter_motion(
    time: Res<Time>,
    mut emitters: Query<(
        &mut EmitterMotionTracker,
        &GlobalTransform,
        Option<&ChildOf>,
    )>,
    velocity_query: Query<&LinearVelocity>,
    parent_query: Query<&ChildOf>,
) {
    let dt = time.delta_secs().max(1e-6);
    for (mut tracker, gxform, child_of) in emitters.iter_mut() {
        let world_pos = gxform.translation();

        // Resolve velocity: walk up the parent chain looking for a
        // `LinearVelocity`. The walk bounds at 16 steps so a pathological
        // hierarchy can't pin a frame in this query.
        let mut velocity_from_physics: Option<Vec3> = None;
        let mut probe: Option<Entity> = child_of.map(|p| p.parent());
        for _ in 0..16 {
            let Some(p) = probe else { break };
            if let Ok(lv) = velocity_query.get(p) {
                velocity_from_physics = Some(lv.0);
                break;
            }
            probe = parent_query.get(p).ok().map(|pp| pp.parent());
        }

        if !tracker.initialised {
            tracker.last_translation = world_pos;
            tracker.world_velocity = velocity_from_physics.unwrap_or(Vec3::ZERO);
            tracker.initialised = true;
            continue;
        }

        tracker.world_velocity =
            velocity_from_physics.unwrap_or_else(|| (world_pos - tracker.last_translation) / dt);
        tracker.last_translation = world_pos;
    }
}

/// Tick every alive particle: integrate velocity / position, fade size
/// (via `Transform::scale`) and colour (via a shared ramp-bucket handle
/// swap — never a material mutation, see
/// [`super::ramp`]), optionally collide, despawn at end-of-life.
/// Decrements the source emitter's `alive_count` on despawn so the
/// spawn cap stays accurate.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn tick_particles(
    mut commands: Commands,
    time: Res<Time>,
    gravity: Res<Gravity>,
    spatial: SpatialQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    mut atlas_meshes: ResMut<ParticleAtlasMeshes>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut emitters: Query<&mut EmitterState>,
    emitter_lookup: Query<(&ParticleEmitter, Option<&EmitterMaterialRamp>)>,
    mut particles: Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let camera_pos = cameras.iter().next().map(|t| t.translation());

    for (entity, mut particle, mut transform, mut material, mut mesh3d) in particles.iter_mut() {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            commands.entity(entity).despawn();
            if let Ok(mut state) = emitters.get_mut(particle.emitter) {
                state.alive_count = state.alive_count.saturating_sub(1);
            }
            continue;
        }

        let Ok((emitter, ramp)) = emitter_lookup.get(particle.emitter) else {
            // Source emitter was despawned (room rebuild). Keep
            // integrating the particle's own velocity so it coasts to
            // age-out instead of freezing mid-air — we just can't
            // refresh the size/colour curves or apply emitter-authored
            // forces (gravity_multiplier, drag, acceleration), which
            // lived only on the snapshot. The orientation update at the
            // bottom of the loop does need the `emitter` snapshot so we
            // skip it for orphaned particles, but the existing rotation
            // remains visually fine for a brief residual lifetime.
            transform.translation += particle.velocity * dt;
            continue;
        };

        // Apply forces.
        let accel = emitter.acceleration + emitter.gravity_multiplier * gravity.0;
        particle.velocity += accel * dt;
        // Exponential drag: v *= exp(-drag * dt).
        if emitter.linear_drag > 0.0 {
            let factor = (-emitter.linear_drag * dt).exp();
            particle.velocity *= factor;
        }

        let mut new_pos = transform.translation + particle.velocity * dt;

        if emitter.collide_terrain || emitter.collide_water || emitter.collide_colliders {
            apply_collisions(
                &spatial,
                emitter,
                &mut particle.velocity,
                &mut new_pos,
                transform.translation,
                dt,
            );
        }

        transform.translation = new_pos;

        // Fade size / colour over lifetime. Size is continuous (scale);
        // colour quantises into the emitter's shared ramp buckets and
        // advances by swapping the material *handle* — an asset-id
        // copy, exactly like the atlas-mesh swap below. The ramp can
        // only be absent for one frame between an emitter's first
        // emission and its deferred component insert; the colour swap
        // just waits that frame out.
        let t = (particle.age / particle.lifetime).clamp(0.0, 1.0);
        let size = lerp_unit(t, emitter.start_size, emitter.end_size);
        transform.scale = Vec3::splat(size);

        if let Some(ramp) = ramp {
            let bucket = ramp.bucket_for(t);
            if bucket != particle.ramp_index {
                particle.ramp_index = bucket;
                material.0 = ramp.handle(bucket).clone();
            }
        }

        // Update the active atlas frame (only meaningful for
        // OverLifetime; the other modes pinned a frame at spawn). When
        // the index advances, swap the Mesh3d handle to the cached
        // mesh for the new cell — the atlas-mesh cache reuses handles
        // across particles so the swap is just an asset-id copy, not
        // a new allocation.
        if let AnimationFrameMode::OverLifetime { .. } = particle.frame_mode
            && let Some((rows, cols)) = particle.atlas_dim
        {
            let new_frame = current_frame_index(
                &particle.frame_mode,
                particle.age,
                particle.frame_index,
                particle.atlas_dim,
            );
            if new_frame != particle.frame_index {
                particle.frame_index = new_frame;
                let handle = atlas_meshes.get_or_create(meshes.as_mut(), rows, cols, new_frame);
                mesh3d.0 = handle;
            }
        }

        // Orient the quad: billboard towards the camera, or align with
        // the velocity vector. If neither is meaningful (no camera in
        // scene, zero velocity), leave the previous orientation.
        if emitter.billboard {
            if let Some(cam) = camera_pos {
                let to_cam = cam - transform.translation;
                if to_cam.length_squared() > 1e-6 {
                    transform.look_at(cam, Vec3::Y);
                }
            }
        } else if particle.velocity.length_squared() > 1e-6 {
            let dir = particle.velocity.normalize();
            transform.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        }
    }
}

/// Resolve collisions for a particle stepping from `prev_pos` along
/// `velocity * dt`. Uses a single ray cast (sphere-cast would be more
/// accurate but ray is cheap enough at 512-particle scale and matches
/// the visual fidelity of an unrotated quad). Reflects velocity with
/// `bounce`, scales tangential by `(1 - friction)`, and snaps the
/// particle to the contact point. Honours the three collide_* flags
/// only insofar as the ray hits any matching collider (terrain / water
/// finite surfaces / arbitrary) — we don't currently distinguish them
/// at the avian level, so the flags act as a single "collide vs
/// pure-visual" toggle in v1. Future work can split them when avian
/// gains layer support.
fn apply_collisions(
    spatial: &SpatialQuery,
    emitter: &ParticleEmitter,
    velocity: &mut Vec3,
    new_pos: &mut Vec3,
    prev_pos: Vec3,
    dt: f32,
) {
    let delta = *new_pos - prev_pos;
    let dist = delta.length();
    if dist <= 1e-5 {
        return;
    }
    let dir = delta / dist;
    let Ok(ray_dir) = Dir3::new(dir) else {
        return;
    };
    let filter = SpatialQueryFilter::default();
    if let Some(hit) = spatial.cast_ray(prev_pos, ray_dir, dist, true, &filter) {
        let normal = hit.normal;
        // Reflect: v' = v - (1 + bounce) * (v · n) n
        let vn = velocity.dot(normal);
        let reflected = *velocity - (1.0 + emitter.bounce) * vn * normal;
        // Apply tangential friction.
        let tangential = reflected - reflected.dot(normal) * normal;
        let normal_component = reflected - tangential;
        *velocity = normal_component + tangential * (1.0 - emitter.friction);
        *new_pos = prev_pos + dir * hit.distance;
        // Step away from the surface a hair so the next frame's ray
        // doesn't immediately re-hit and produce a runaway reflection.
        *new_pos += normal * 1e-3;
        // Damp out tangential velocity that's nearly stopped — keeps a
        // dust particle from sliding forever along a slope.
        if velocity.length_squared() < 1e-3 {
            *velocity = Vec3::ZERO;
        }
        let _ = dt;
    }
}
