//! Sanitiser for the authored [`ContactEffects`] record (#246).
//!
//! A hostile or buggy record could otherwise weaponise the contact
//! particle dispatcher: an unbounded `count.max` / `max_particles_per_frame`
//! to OOM via entity spawning, a NaN trigger gate that matches every
//! frame, a kilometre-wide emitter shape, or a thousand recipes walked
//! per contact per frame. Every numeric is clamped finite and bounded;
//! the recipe list is capped deterministically (same name-sorted
//! truncation the generator map uses) so every peer keeps the same
//! survivor set.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::contact_effects::{ContactEffectRecord, ContactEffects, RecipeParticle};
use crate::pds::generator::EmitterShape;

impl Sanitize for ContactEffects {
    fn sanitize(&mut self) {
        self.max_particles_per_frame = self
            .max_particles_per_frame
            .min(limits::MAX_CONTACT_PARTICLES_PER_FRAME);

        // Deterministic cap: sort by name then truncate so every peer
        // keeps the same recipes (Vec order is author-controlled, but a
        // truncation must not diverge across clients).
        if self.recipes.len() > limits::MAX_CONTACT_RECIPES {
            self.recipes.sort_by(|a, b| a.name.cmp(&b.name));
            self.recipes.truncate(limits::MAX_CONTACT_RECIPES);
        }

        for r in &mut self.recipes {
            r.sanitize();
        }
    }
}

impl Sanitize for ContactEffectRecord {
    fn sanitize(&mut self) {
        if self.name.chars().count() > limits::MAX_CONTACT_RECIPE_NAME {
            self.name = self
                .name
                .chars()
                .take(limits::MAX_CONTACT_RECIPE_NAME)
                .collect();
        }

        self.min_speed.0 = clamp_finite(self.min_speed.0, 0.0, limits::MAX_CONTACT_MIN_SPEED, 0.0);
        self.min_intensity.0 = clamp_finite(self.min_intensity.0, 0.0, 1.0, 0.0);
        self.radius_scale.0 = clamp_finite(
            self.radius_scale.0,
            0.0,
            limits::MAX_CONTACT_RADIUS_SCALE,
            1.0,
        );
        self.velocity_inherit.0 = clamp_finite(
            self.velocity_inherit.0,
            0.0,
            limits::MAX_PARTICLE_INHERIT_VELOCITY,
            0.0,
        );
        self.cooldown.0 = clamp_finite(self.cooldown.0, 0.0, limits::MAX_CONTACT_COOLDOWN, 0.0);

        // Count model: finite coefficients, burst-capped max, min ≤ max.
        let burst_cap = limits::MAX_PARTICLE_BURST as f32;
        self.count.gain.0 = clamp_finite(self.count.gain.0, -burst_cap, burst_cap, 0.0);
        self.count.base.0 = clamp_finite(self.count.base.0, 0.0, burst_cap, 0.0);
        self.count.max = self.count.max.min(limits::MAX_PARTICLE_BURST);
        if self.count.min > self.count.max {
            self.count.min = self.count.max;
        }

        self.particle.sanitize();
    }
}

impl Sanitize for RecipeParticle {
    fn sanitize(&mut self) {
        self.max_particles = self.max_particles.min(limits::MAX_PARTICLES);

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
        for i in 0..4 {
            self.start_color.0[i] = unit(self.start_color.0[i], 1.0);
            self.end_color.0[i] = unit(self.end_color.0[i], if i == 3 { 0.0 } else { 1.0 });
        }

        // Emitter spawn shape — same bounds the ParticleSystem
        // sanitiser applies (kept inline rather than coupling the two
        // sanitisers' signatures).
        match &mut self.shape {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::contact_effects::{CountModel, default_contact_effects};
    use crate::pds::types::{Fp, Fp4};

    #[test]
    fn defaults_survive_sanitise_unchanged() {
        let mut e = default_contact_effects();
        let before = e.clone();
        e.sanitize();
        assert_eq!(e, before, "canonical defaults must already be in-bounds");
    }

    #[test]
    fn hostile_values_are_clamped_and_bounded() {
        let mut e = default_contact_effects();
        let r = &mut e.recipes[0];
        r.min_speed = Fp(f32::NAN);
        r.min_intensity = Fp(99.0);
        r.radius_scale = Fp(1.0e9);
        r.cooldown = Fp(-5.0);
        r.count = CountModel {
            gain: Fp(f32::INFINITY),
            base: Fp(1.0e9),
            min: 9_999,
            max: 1,
        };
        r.particle.start_color = Fp4([2.0, -1.0, f32::NAN, 5.0]);
        r.particle.lifetime_min = Fp(10.0);
        r.particle.lifetime_max = Fp(1.0);
        r.particle.max_particles = u32::MAX;
        e.max_particles_per_frame = u32::MAX;
        e.sanitize();

        let r = &e.recipes[0];
        assert_eq!(r.min_speed.0, 0.0); // NaN → default
        assert_eq!(r.min_intensity.0, 1.0); // clamped to 1
        assert!(r.radius_scale.0 <= limits::MAX_CONTACT_RADIUS_SCALE);
        assert_eq!(r.cooldown.0, 0.0); // negative → default
        assert!(r.count.max <= limits::MAX_PARTICLE_BURST);
        assert!(r.count.min <= r.count.max);
        assert!(
            r.particle
                .start_color
                .0
                .iter()
                .all(|&c| (0.0..=1.0).contains(&c))
        );
        assert!(r.particle.lifetime_max.0 >= r.particle.lifetime_min.0);
        assert!(r.particle.max_particles <= limits::MAX_PARTICLES);
        assert!(e.max_particles_per_frame <= limits::MAX_CONTACT_PARTICLES_PER_FRAME);
    }

    #[test]
    fn recipe_list_capped_deterministically() {
        let mut e = default_contact_effects();
        let proto = e.recipes[0].clone();
        e.recipes.clear();
        for i in 0..(limits::MAX_CONTACT_RECIPES + 20) {
            let mut r = proto.clone();
            r.name = format!("r{i:03}");
            e.recipes.push(r);
        }
        e.sanitize();
        assert_eq!(e.recipes.len(), limits::MAX_CONTACT_RECIPES);
        // Name-sorted survivor set: first MAX by name.
        assert_eq!(e.recipes[0].name, "r000");
    }
}
