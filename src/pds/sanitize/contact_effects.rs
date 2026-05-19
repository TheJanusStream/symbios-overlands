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
use crate::pds::contact_effects::{
    ContactEffectKind, ContactEffectRecord, ContactEffects, DecalParams, RecipeParticle,
};
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

        // Shared trigger gates (kind-independent).
        self.min_speed.0 = clamp_finite(self.min_speed.0, 0.0, limits::MAX_CONTACT_MIN_SPEED, 0.0);
        self.min_intensity.0 = clamp_finite(self.min_intensity.0, 0.0, 1.0, 0.0);
        self.cooldown.0 = clamp_finite(self.cooldown.0, 0.0, limits::MAX_CONTACT_COOLDOWN, 0.0);

        // Per-effect-kind payload bounds.
        match &mut self.effect {
            ContactEffectKind::ParticleBurst {
                count,
                radius_scale,
                velocity_inherit,
                particle,
            } => {
                radius_scale.0 =
                    clamp_finite(radius_scale.0, 0.0, limits::MAX_CONTACT_RADIUS_SCALE, 1.0);
                velocity_inherit.0 = clamp_finite(
                    velocity_inherit.0,
                    0.0,
                    limits::MAX_PARTICLE_INHERIT_VELOCITY,
                    0.0,
                );
                // Count model: finite coefficients, burst-capped max,
                // min ≤ max.
                let burst_cap = limits::MAX_PARTICLE_BURST as f32;
                count.gain.0 = clamp_finite(count.gain.0, -burst_cap, burst_cap, 0.0);
                count.base.0 = clamp_finite(count.base.0, 0.0, burst_cap, 0.0);
                count.max = count.max.min(limits::MAX_PARTICLE_BURST);
                if count.min > count.max {
                    count.min = count.max;
                }
                particle.sanitize();
            }
            ContactEffectKind::DecalStamp { decal } => decal.sanitize(),
            // A future/unknown effect kind has no fields we can bound;
            // the runtime mapper drops it anyway.
            ContactEffectKind::Unknown => {}
        }
    }
}

impl Sanitize for DecalParams {
    fn sanitize(&mut self) {
        self.ttl.0 = clamp_finite(
            self.ttl.0,
            limits::MIN_CONTACT_DECAL_TTL,
            limits::MAX_CONTACT_DECAL_TTL,
            limits::MIN_CONTACT_DECAL_TTL,
        );
        self.start_size.0 =
            clamp_finite(self.start_size.0, 0.0, limits::MAX_CONTACT_DECAL_SIZE, 0.45);
        self.end_size.0 = clamp_finite(self.end_size.0, 0.0, limits::MAX_CONTACT_DECAL_SIZE, 0.85);
        self.start_alpha.0 = clamp_finite(self.start_alpha.0, 0.0, 1.0, 0.55);
        self.end_alpha.0 = clamp_finite(self.end_alpha.0, 0.0, 1.0, 0.0);
        self.normal_offset.0 = clamp_finite(
            self.normal_offset.0,
            0.0,
            limits::MAX_CONTACT_DECAL_NORMAL_OFFSET,
            0.02,
        );
        for c in &mut self.color.0 {
            *c = clamp_finite(*c, 0.0, 1.0, 0.0);
        }
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
    use crate::pds::contact_effects::{
        ContactEffectRecord, ContactPhaseKind, ContactSurfaceKind, CountModel,
        default_contact_effects,
    };
    use crate::pds::types::{Fp, Fp3, Fp4};

    /// Mutable access to a record's ParticleBurst payload (every
    /// canonical default recipe is one).
    fn burst(
        r: &mut ContactEffectRecord,
    ) -> (&mut CountModel, &mut Fp, &mut Fp, &mut RecipeParticle) {
        match &mut r.effect {
            ContactEffectKind::ParticleBurst {
                count,
                radius_scale,
                velocity_inherit,
                particle,
            } => (count, radius_scale, velocity_inherit, particle),
            _ => unreachable!("canonical defaults are ParticleBurst"),
        }
    }

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
        {
            let r = &mut e.recipes[0];
            r.min_speed = Fp(f32::NAN);
            r.min_intensity = Fp(99.0);
            r.cooldown = Fp(-5.0);
            let (count, radius_scale, _vi, particle) = burst(r);
            *radius_scale = Fp(1.0e9);
            *count = CountModel {
                gain: Fp(f32::INFINITY),
                base: Fp(1.0e9),
                min: 9_999,
                max: 1,
            };
            particle.start_color = Fp4([2.0, -1.0, f32::NAN, 5.0]);
            particle.lifetime_min = Fp(10.0);
            particle.lifetime_max = Fp(1.0);
            particle.max_particles = u32::MAX;
        }
        e.max_particles_per_frame = u32::MAX;
        e.sanitize();

        let r = &mut e.recipes[0];
        assert_eq!(r.min_speed.0, 0.0); // NaN → default
        assert_eq!(r.min_intensity.0, 1.0); // clamped to 1
        assert_eq!(r.cooldown.0, 0.0); // negative → default
        let (count, radius_scale, _vi, particle) = burst(r);
        assert!(radius_scale.0 <= limits::MAX_CONTACT_RADIUS_SCALE);
        assert!(count.max <= limits::MAX_PARTICLE_BURST);
        assert!(count.min <= count.max);
        assert!(
            particle
                .start_color
                .0
                .iter()
                .all(|&c| (0.0..=1.0).contains(&c))
        );
        assert!(particle.lifetime_max.0 >= particle.lifetime_min.0);
        assert!(particle.max_particles <= limits::MAX_PARTICLES);
        assert!(e.max_particles_per_frame <= limits::MAX_CONTACT_PARTICLES_PER_FRAME);
    }

    #[test]
    fn hostile_decal_params_are_clamped() {
        let mut e = default_contact_effects();
        e.recipes.push(ContactEffectRecord {
            name: "evil_decal".into(),
            surface: ContactSurfaceKind::Terrain,
            phase: ContactPhaseKind::Dwell,
            min_speed: Fp(0.0),
            min_intensity: Fp(0.0),
            cooldown: Fp(0.0),
            enabled: true,
            effect: ContactEffectKind::DecalStamp {
                decal: DecalParams {
                    ttl: Fp(f32::INFINITY),
                    start_size: Fp(1.0e9),
                    end_size: Fp(-3.0),
                    start_alpha: Fp(9.0),
                    end_alpha: Fp(f32::NAN),
                    color: Fp3([2.0, -1.0, f32::NAN]),
                    normal_offset: Fp(1.0e6),
                },
            },
        });
        e.sanitize();
        let d = match &e.recipes.last().unwrap().effect {
            ContactEffectKind::DecalStamp { decal } => *decal,
            _ => unreachable!(),
        };
        assert!(
            d.ttl.0 >= limits::MIN_CONTACT_DECAL_TTL && d.ttl.0 <= limits::MAX_CONTACT_DECAL_TTL
        );
        assert!(d.start_size.0 >= 0.0 && d.start_size.0 <= limits::MAX_CONTACT_DECAL_SIZE);
        assert!(d.end_size.0 >= 0.0 && d.end_size.0 <= limits::MAX_CONTACT_DECAL_SIZE);
        assert!((0.0..=1.0).contains(&d.start_alpha.0));
        assert!((0.0..=1.0).contains(&d.end_alpha.0));
        assert!(d.color.0.iter().all(|&c| (0.0..=1.0).contains(&c)));
        assert!(d.normal_offset.0 <= limits::MAX_CONTACT_DECAL_NORMAL_OFFSET);
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
