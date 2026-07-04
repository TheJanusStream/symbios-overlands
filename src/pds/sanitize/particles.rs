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
use crate::pds::generator::{AnimationFrameMode, EmitterShape, ParticleParams};
use crate::pds::types::Fp4;

impl Sanitize for ParticleParams {
    fn sanitize(&mut self) {
        self.max_particles = self.max_particles.min(limits::MAX_PARTICLES);
        // Clamp the procedural sprite's atlas dims + per-feature loop counts
        // (the `SovereignTextureConfig` sanitiser) so a hostile emitter can't
        // smuggle an unbounded sprite bake through the particle slot.
        self.procedural_texture.sanitize();
        self.rate_per_second.0 =
            clamp_finite(self.rate_per_second.0, 0.0, limits::MAX_PARTICLE_RATE, 0.0);
        self.burst_count = self.burst_count.min(limits::MAX_PARTICLE_BURST);
        self.duration.0 = clamp_finite(
            self.duration.0,
            limits::MIN_PARTICLE_DURATION,
            limits::MAX_PARTICLE_DURATION,
            1.0,
        );

        self.lifetime_min.0 = clamp_finite(
            self.lifetime_min.0,
            limits::MIN_PARTICLE_LIFETIME,
            limits::MAX_PARTICLE_LIFETIME,
            limits::MIN_PARTICLE_LIFETIME,
        );
        self.lifetime_max.0 = clamp_finite(
            self.lifetime_max.0,
            limits::MIN_PARTICLE_LIFETIME,
            limits::MAX_PARTICLE_LIFETIME,
            limits::MIN_PARTICLE_LIFETIME,
        );
        if self.lifetime_max.0 < self.lifetime_min.0 {
            self.lifetime_max.0 = self.lifetime_min.0;
        }

        self.speed_min.0 = clamp_finite(self.speed_min.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
        self.speed_max.0 = clamp_finite(self.speed_max.0, 0.0, limits::MAX_PARTICLE_SPEED, 0.0);
        if self.speed_max.0 < self.speed_min.0 {
            self.speed_max.0 = self.speed_min.0;
        }

        self.gravity_multiplier.0 = clamp_finite(
            self.gravity_multiplier.0,
            -limits::MAX_PARTICLE_GRAVITY_MULT,
            limits::MAX_PARTICLE_GRAVITY_MULT,
            0.0,
        );
        let a = limits::MAX_PARTICLE_ACCEL;
        self.acceleration.0[0] = clamp_finite(self.acceleration.0[0], -a, a, 0.0);
        self.acceleration.0[1] = clamp_finite(self.acceleration.0[1], -a, a, 0.0);
        self.acceleration.0[2] = clamp_finite(self.acceleration.0[2], -a, a, 0.0);
        self.linear_drag.0 = clamp_finite(self.linear_drag.0, 0.0, limits::MAX_PARTICLE_DRAG, 0.0);

        self.start_size.0 = clamp_finite(
            self.start_size.0,
            limits::MIN_PARTICLE_SIZE,
            limits::MAX_PARTICLE_SIZE,
            0.1,
        );
        self.end_size.0 = clamp_finite(
            self.end_size.0,
            limits::MIN_PARTICLE_SIZE,
            limits::MAX_PARTICLE_SIZE,
            0.1,
        );

        let unit = |v: f32, default: f32| clamp_finite(v, 0.0, 1.0, default);
        self.start_color = Fp4([
            unit(self.start_color.0[0], 1.0),
            unit(self.start_color.0[1], 1.0),
            unit(self.start_color.0[2], 1.0),
            unit(self.start_color.0[3], 1.0),
        ]);
        self.end_color = Fp4([
            unit(self.end_color.0[0], 1.0),
            unit(self.end_color.0[1], 1.0),
            unit(self.end_color.0[2], 1.0),
            unit(self.end_color.0[3], 1.0),
        ]);

        self.inherit_velocity.0 = clamp_finite(
            self.inherit_velocity.0,
            0.0,
            limits::MAX_PARTICLE_INHERIT_VELOCITY,
            0.0,
        );
        self.bounce.0 = clamp_finite(self.bounce.0, 0.0, 1.0, 0.0);
        self.friction.0 = clamp_finite(self.friction.0, 0.0, 1.0, 0.0);

        if let Some(src) = &mut self.texture {
            src.sanitize();
        }
        if let Some(atlas) = &mut self.texture_atlas {
            atlas.rows = atlas.rows.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
            atlas.cols = atlas.cols.clamp(1, limits::MAX_PARTICLE_ATLAS_DIM);
        }
        if let AnimationFrameMode::OverLifetime { fps } = &mut self.frame_mode {
            fps.0 = clamp_finite(fps.0, 0.0, limits::MAX_PARTICLE_FRAME_FPS, 0.0);
        }

        match &mut self.emitter_shape {
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
}
