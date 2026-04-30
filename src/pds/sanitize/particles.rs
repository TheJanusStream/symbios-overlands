//! Sanitiser for the `ParticleSystem` generator. Defends against the
//! three weaponised inputs particle systems are historically vulnerable
//! to: emit rates so high they pin every frame on entity spawning,
//! lifetimes so long the steady-state population never decays, and
//! acceleration / drag values that produce NaN positions inside one
//! tick. Also enforces `min ≤ max` on the sampled ranges so the
//! deterministic per-particle sampler can't trip on an inverted interval.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::generator::{AnimationFrameMode, EmitterShape, SignSource, TextureAtlas};
use crate::pds::types::{Fp, Fp3, Fp4};

#[allow(clippy::too_many_arguments)]
pub(super) fn sanitize_particles(
    emitter_shape: &mut EmitterShape,
    rate_per_second: &mut Fp,
    burst_count: &mut u32,
    max_particles: &mut u32,
    duration: &mut Fp,
    lifetime_min: &mut Fp,
    lifetime_max: &mut Fp,
    speed_min: &mut Fp,
    speed_max: &mut Fp,
    gravity_multiplier: &mut Fp,
    acceleration: &mut Fp3,
    linear_drag: &mut Fp,
    start_size: &mut Fp,
    end_size: &mut Fp,
    start_color: &mut Fp4,
    end_color: &mut Fp4,
    inherit_velocity: &mut Fp,
    bounce: &mut Fp,
    friction: &mut Fp,
    texture: &mut Option<SignSource>,
    texture_atlas: &mut Option<TextureAtlas>,
    frame_mode: &mut AnimationFrameMode,
) {
    *max_particles = (*max_particles).min(limits::MAX_PARTICLES);
    rate_per_second.0 = clamp_finite(rate_per_second.0, 0.0, limits::MAX_PARTICLE_RATE, 0.0);
    *burst_count = (*burst_count).min(limits::MAX_PARTICLE_BURST);
    duration.0 = clamp_finite(
        duration.0,
        limits::MIN_PARTICLE_DURATION,
        limits::MAX_PARTICLE_DURATION,
        1.0,
    );

    lifetime_min.0 = clamp_finite(
        lifetime_min.0,
        limits::MIN_PARTICLE_LIFETIME,
        limits::MAX_PARTICLE_LIFETIME,
        limits::MIN_PARTICLE_LIFETIME,
    );
    lifetime_max.0 = clamp_finite(
        lifetime_max.0,
        limits::MIN_PARTICLE_LIFETIME,
        limits::MAX_PARTICLE_LIFETIME,
        limits::MIN_PARTICLE_LIFETIME,
    );
    if lifetime_max.0 < lifetime_min.0 {
        lifetime_max.0 = lifetime_min.0;
    }

    speed_min.0 = clamp_finite(speed_min.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
    speed_max.0 = clamp_finite(speed_max.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
    if speed_max.0 < speed_min.0 {
        speed_max.0 = speed_min.0;
    }

    gravity_multiplier.0 = clamp_finite(
        gravity_multiplier.0,
        -limits::MAX_PARTICLE_GRAVITY_MULT,
        limits::MAX_PARTICLE_GRAVITY_MULT,
        0.0,
    );
    let a = limits::MAX_PARTICLE_ACCEL;
    acceleration.0[0] = clamp_finite(acceleration.0[0], -a, a, 0.0);
    acceleration.0[1] = clamp_finite(acceleration.0[1], -a, a, 0.0);
    acceleration.0[2] = clamp_finite(acceleration.0[2], -a, a, 0.0);
    linear_drag.0 = clamp_finite(linear_drag.0, 0.0, limits::MAX_PARTICLE_DRAG, 0.0);

    start_size.0 = clamp_finite(
        start_size.0,
        limits::MIN_PARTICLE_SIZE,
        limits::MAX_PARTICLE_SIZE,
        0.1,
    );
    end_size.0 = clamp_finite(
        end_size.0,
        limits::MIN_PARTICLE_SIZE,
        limits::MAX_PARTICLE_SIZE,
        0.1,
    );

    let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
    *start_color = Fp4([
        unit(start_color.0[0], 1.0),
        unit(start_color.0[1], 1.0),
        unit(start_color.0[2], 1.0),
        unit(start_color.0[3], 1.0),
    ]);
    *end_color = Fp4([
        unit(end_color.0[0], 1.0),
        unit(end_color.0[1], 1.0),
        unit(end_color.0[2], 1.0),
        unit(end_color.0[3], 1.0),
    ]);

    inherit_velocity.0 = clamp_finite(
        inherit_velocity.0,
        0.0,
        limits::MAX_PARTICLE_INHERIT_VELOCITY,
        0.0,
    );
    bounce.0 = clamp_finite(bounce.0, 0.0, 1.0, 0.0);
    friction.0 = clamp_finite(friction.0, 0.0, 1.0, 0.0);

    if let Some(src) = texture {
        src.sanitize();
    }
    if let Some(atlas) = texture_atlas {
        atlas.rows = atlas.rows.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
        atlas.cols = atlas.cols.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
    }
    if let AnimationFrameMode::OverLifetime { fps } = frame_mode {
        fps.0 = clamp_finite(fps.0, 0.0, limits::MAX_PARTICLE_FRAME_FPS, 0.0);
    }

    match emitter_shape {
        EmitterShape::Sphere { radius } => {
            radius.0 = clamp_finite(radius.0, 0.0, limits::MAX_PARTICLE_SHAPE_RADIUS, 0.5);
        }
        EmitterShape::Box { half_extents } => {
            let h = limits::MAX_PARTICLE_SHAPE_HALF_EXTENT;
            half_extents.0[0] = clamp_finite(half_extents.0[0], 0.0, h, 0.5);
            half_extents.0[1] = clamp_finite(half_extents.0[1], 0.0, h, 0.5);
            half_extents.0[2] = clamp_finite(half_extents.0[2], 0.0, h, 0.5);
        }
        EmitterShape::Cone { half_angle, height } => {
            half_angle.0 =
                clamp_finite(half_angle.0, 0.0, limits::MAX_PARTICLE_CONE_HALF_ANGLE, 0.4);
            height.0 = clamp_finite(height.0, 0.0, limits::MAX_PARTICLE_SHAPE_HEIGHT, 0.5);
        }
        EmitterShape::Point | EmitterShape::Unknown => {}
    }
}
