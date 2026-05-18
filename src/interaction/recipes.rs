//! Declarative contact→particle effect recipes (Phase 2, #244).
//!
//! A [`ContactEffectRecipe`] pairs a [`ContactTrigger`] predicate over a
//! [`ContactSample`] with a [`ParticleBurst`] describing the emitter to
//! spawn when it matches. The [`ContactRecipeRegistry`] resource holds
//! the active set; [`super::particle_channel::particle_dispatcher`]
//! walks `AvatarContacts × recipes` each frame.
//!
//! Templates are hardcoded coloured-quad [`ParticleEmitter`] snapshots
//! (no atlas / asset dependency — the `texture: None` path renders solid
//! billboarded quads). Designers iterate by flipping
//! [`ContactEffectRecipe::enabled`] or tuning the table here; a PDS-
//! authoring path is deferred to Phase 4 (#246).

use bevy::prelude::*;

use crate::pds::{
    AnimationFrameMode, ContactEffects, ContactPhaseKind, ContactSurfaceKind, EmitterShape, Fp,
    ParticleBlendMode, RecipeParticle, SimulationSpace, TextureFilter,
};
use crate::world_builder::particles::ParticleEmitter;

use super::contact::{ContactPhase, ContactSample, SurfaceKind};

/// Predicate side of a recipe: which samples it fires on.
#[derive(Debug, Clone, Copy)]
pub struct ContactTrigger {
    pub surface_kind: SurfaceKind,
    pub phase: ContactPhase,
    /// Minimum raw contact speed (m/s, `world_vel.length()`).
    pub min_speed: f32,
    /// Minimum normalised engagement (`ContactSample::intensity`).
    pub min_intensity: f32,
}

impl ContactTrigger {
    /// True when `sample` satisfies every clause.
    pub fn matches(&self, sample: &ContactSample) -> bool {
        sample.surface.kind() == self.surface_kind
            && sample.phase == self.phase
            && sample.world_vel.length() >= self.min_speed
            && sample.intensity >= self.min_intensity
    }
}

/// Declarative burst-count model — `count = clamp(speed·gain + base,
/// min, max)` where `speed` is the contact sample's `world_vel`
/// magnitude. A plain value (not a `fn` pointer) so it round-trips
/// through the PDS [`crate::pds::CountModel`] record (#246) and stays
/// trivially `Clone` + unit-testable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountCurve {
    pub gain: f32,
    pub base: f32,
    pub min: u32,
    pub max: u32,
}

impl CountCurve {
    /// Burst particle count for a matched sample.
    pub fn eval(&self, sample: &ContactSample) -> u32 {
        let raw = sample.world_vel.length() * self.gain + self.base;
        raw.clamp(self.min as f32, self.max as f32) as u32
    }
}

/// Spawn side of a recipe: the emitter template plus per-sample scaling.
#[derive(Clone)]
pub struct ParticleBurst {
    /// Base emitter snapshot. The dispatcher clones this and overlays
    /// the per-sample burst count, footprint-scaled spawn shape, and
    /// velocity-inherit fraction before spawning.
    pub template: ParticleEmitter,
    /// Matched sample → burst particle count. The dispatcher
    /// additionally clamps the sum across all recipes to the registry's
    /// per-frame ceiling.
    pub count: CountCurve,
    /// Multiplier applied to `sample.footprint_radius` to size the
    /// emitter's spawn shape (sphere radius / cone height / box extent).
    pub radius_scale: f32,
    /// Fraction of the avatar's world velocity the particles inherit
    /// (via the emitter's `inherit_velocity`, resolved by
    /// `update_emitter_motion` walking the `ChildOf` chain to the
    /// avatar's `LinearVelocity`).
    pub velocity_inherit: f32,
    /// Minimum seconds between emissions from one avatar for this
    /// recipe. `0.0` = every matching frame (fine for one-shot
    /// `Enter`/`Exit`, which only match once); `> 0.0` throttles a
    /// continuous `Dwell` trickle so it does not spawn an emitter every
    /// frame.
    pub cooldown: f32,
}

/// One declarative effect rule.
#[derive(Clone)]
pub struct ContactEffectRecipe {
    /// Identifier for debug / the editor (the per-avatar cooldown is
    /// keyed by recipe *index*, not this string, so an author renaming
    /// a recipe doesn't reset live cooldowns). Owned `String` so it can
    /// come from the authored [`crate::pds::ContactEffectRecord`].
    pub name: String,
    pub trigger: ContactTrigger,
    pub spawn: ParticleBurst,
    /// Designer kill-switch — `false` skips the recipe entirely without
    /// removing it from the table.
    pub enabled: bool,
}

/// Active recipe set + the global emission ceiling.
#[derive(Resource)]
pub struct ContactRecipeRegistry {
    pub recipes: Vec<ContactEffectRecipe>,
    /// Hard cap on particles spawned across *all* recipes and avatars
    /// in a single frame. Bounds a stutter-frame / many-avatar spike
    /// (a long frame can match many `Enter`s at once); excess is
    /// dropped, never queued.
    pub max_particles_per_frame: u32,
}

impl Default for ContactRecipeRegistry {
    fn default() -> Self {
        Self {
            recipes: default_water_recipes(),
            max_particles_per_frame: 240,
        }
    }
}

// ---------------------------------------------------------------------------
// Hardcoded templates (coloured billboard quads — no texture/atlas)
// ---------------------------------------------------------------------------

/// Shared base: a non-looping, burst-only, world-space coloured-quad
/// emitter. Callers override `shape`, `burst_count`, lifetimes, sizes
/// and colours. `duration` is tiny — the initial burst fires on the
/// emitter's first tick, then it goes inactive and
/// `retire_transient_emitters` despawns it once its particles age out.
fn transient_base() -> ParticleEmitter {
    ParticleEmitter {
        shape: EmitterShape::Point,
        rate_per_second: 0.0,
        burst_count: 0,
        max_particles: 64,
        looping: false,
        duration: 0.06,
        lifetime_min: 0.3,
        lifetime_max: 0.6,
        speed_min: 1.0,
        speed_max: 2.0,
        gravity_multiplier: 1.0,
        acceleration: Vec3::ZERO,
        linear_drag: 0.4,
        start_size: 0.10,
        end_size: 0.02,
        start_color: LinearRgba::new(0.85, 0.93, 1.0, 0.95),
        end_color: LinearRgba::new(0.70, 0.85, 1.0, 0.0),
        blend_mode: ParticleBlendMode::Alpha,
        billboard: true,
        simulation_space: SimulationSpace::World,
        inherit_velocity: 0.0,
        collide_terrain: false,
        collide_water: false,
        collide_colliders: false,
        bounce: 0.0,
        friction: 0.0,
        texture: None,
        texture_atlas: None,
        frame_mode: AnimationFrameMode::Still,
        texture_filter: TextureFilter::Linear,
    }
}

/// Splash on fast water entry — an upward droplet fan.
fn water_splash_template() -> ParticleEmitter {
    ParticleEmitter {
        // Cone apex at origin pointing local +Y: an upward spray. The
        // dispatcher scales `height` by the avatar footprint so a
        // hover-boat throws a wider splash than a swimmer.
        shape: EmitterShape::Cone {
            half_angle: Fp(0.7),
            height: Fp(0.4),
        },
        max_particles: 64,
        lifetime_min: 0.3,
        lifetime_max: 0.6,
        speed_min: 2.0,
        speed_max: 4.0,
        start_size: 0.13,
        end_size: 0.03,
        ..transient_base()
    }
}

/// Low trickle of droplets while swimming/wading (continuous `Dwell`).
fn water_droplet_template() -> ParticleEmitter {
    ParticleEmitter {
        shape: EmitterShape::Sphere { radius: Fp(0.2) },
        max_particles: 16,
        lifetime_min: 0.3,
        lifetime_max: 0.5,
        speed_min: 0.6,
        speed_max: 1.4,
        start_size: 0.06,
        end_size: 0.015,
        ..transient_base()
    }
}

/// The hardcoded initial water recipe set — the pre-Phase-4 fallback
/// used by [`ContactRecipeRegistry::default`] until a room compiles its
/// authored [`crate::pds::ContactEffects`]. Must stay value-equal to
/// [`crate::pds::default_contact_effects`] so a record that omits the
/// key behaves identically. Recipe 3 (ground dust) needs the Phase 3
/// `SurfaceContact::Terrain` variant and is intentionally absent until
/// #245 lands; the types above already accommodate it.
pub fn default_water_recipes() -> Vec<ContactEffectRecipe> {
    vec![
        ContactEffectRecipe {
            name: "water_splash".into(),
            trigger: ContactTrigger {
                surface_kind: SurfaceKind::Water,
                phase: ContactPhase::Enter,
                min_speed: 1.5,
                min_intensity: 0.0,
            },
            spawn: ParticleBurst {
                template: water_splash_template(),
                // clamp(speed*8, 0, 40)
                count: CountCurve {
                    gain: 8.0,
                    base: 0.0,
                    min: 0,
                    max: 40,
                },
                radius_scale: 1.0,
                velocity_inherit: 0.5,
                cooldown: 0.0,
            },
            enabled: true,
        },
        ContactEffectRecipe {
            name: "water_droplet".into(),
            trigger: ContactTrigger {
                surface_kind: SurfaceKind::Water,
                phase: ContactPhase::Dwell,
                // Some submersion (swimming/wading), not a hull skimming
                // the very surface.
                min_speed: 0.5,
                min_intensity: 0.25,
            },
            spawn: ParticleBurst {
                template: water_droplet_template(),
                // Flat trickle: const 2.
                count: CountCurve {
                    gain: 0.0,
                    base: 2.0,
                    min: 2,
                    max: 2,
                },
                radius_scale: 0.6,
                velocity_inherit: 0.7,
                cooldown: 0.25,
            },
            enabled: true,
        },
    ]
}

// ---------------------------------------------------------------------------
// PDS-authored → runtime mapping (#246)
// ---------------------------------------------------------------------------

fn map_surface(s: ContactSurfaceKind) -> Option<SurfaceKind> {
    match s {
        ContactSurfaceKind::Water => Some(SurfaceKind::Water),
        // A future/unknown surface tag can't map to a runtime kind —
        // skip the recipe rather than guessing.
        ContactSurfaceKind::Unknown => None,
    }
}

fn map_phase(p: ContactPhaseKind) -> Option<ContactPhase> {
    match p {
        ContactPhaseKind::Enter => Some(ContactPhase::Enter),
        ContactPhaseKind::Dwell => Some(ContactPhase::Dwell),
        ContactPhaseKind::Exit => Some(ContactPhase::Exit),
        ContactPhaseKind::Unknown => None,
    }
}

/// Build a runtime emitter snapshot from an authored
/// [`RecipeParticle`]. The fixed transient-burst fields (no rate /
/// loop, tiny duration, World space, no texture/collision; the
/// dispatcher overrides `burst_count`, `inherit_velocity` and the
/// footprint-scaled shape) come from [`transient_base`]; everything the
/// designer controls is overlaid here.
fn recipe_particle_to_emitter(p: &RecipeParticle) -> ParticleEmitter {
    ParticleEmitter {
        shape: p.shape.clone(),
        lifetime_min: p.lifetime_min.0,
        lifetime_max: p.lifetime_max.0,
        speed_min: p.speed_min.0,
        speed_max: p.speed_max.0,
        gravity_multiplier: p.gravity_multiplier.0,
        linear_drag: p.linear_drag.0,
        start_size: p.start_size.0,
        end_size: p.end_size.0,
        start_color: LinearRgba::new(
            p.start_color.0[0],
            p.start_color.0[1],
            p.start_color.0[2],
            p.start_color.0[3],
        ),
        end_color: LinearRgba::new(
            p.end_color.0[0],
            p.end_color.0[1],
            p.end_color.0[2],
            p.end_color.0[3],
        ),
        blend_mode: p.blend_mode.clone(),
        billboard: p.billboard,
        max_particles: p.max_particles,
        ..transient_base()
    }
}

impl ContactRecipeRegistry {
    /// Compile an authored [`ContactEffects`] record into the runtime
    /// registry. Recipes whose surface/phase is an unknown (future)
    /// tag are dropped — the engine can't dispatch a kind it doesn't
    /// model. The numeric fields are assumed already sanitised by
    /// [`crate::pds::RoomRecord::sanitize`].
    pub fn from_effects(effects: &ContactEffects) -> Self {
        let recipes = effects
            .recipes
            .iter()
            .filter_map(|r| {
                Some(ContactEffectRecipe {
                    name: r.name.clone(),
                    trigger: ContactTrigger {
                        surface_kind: map_surface(r.surface)?,
                        phase: map_phase(r.phase)?,
                        min_speed: r.min_speed.0,
                        min_intensity: r.min_intensity.0,
                    },
                    spawn: ParticleBurst {
                        template: recipe_particle_to_emitter(&r.particle),
                        count: CountCurve {
                            gain: r.count.gain.0,
                            base: r.count.base.0,
                            min: r.count.min,
                            max: r.count.max,
                        },
                        radius_scale: r.radius_scale.0,
                        velocity_inherit: r.velocity_inherit.0,
                        cooldown: r.cooldown.0,
                    },
                    enabled: r.enabled,
                })
            })
            .collect();
        Self {
            recipes,
            max_particles_per_frame: effects.max_particles_per_frame,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water_sample(phase: ContactPhase, speed: f32, intensity: f32) -> ContactSample {
        ContactSample {
            avatar: Entity::PLACEHOLDER,
            world_pos: Vec3::ZERO,
            world_vel: Vec3::new(speed, 0.0, 0.0),
            footprint_radius: 0.5,
            surface: super::super::contact::SurfaceContact::Water {
                plane_idx: 0,
                depth: 1.0,
                flow_dir: Vec2::ZERO,
            },
            intensity,
            phase,
        }
    }

    #[test]
    fn trigger_matches_all_clauses() {
        let t = ContactTrigger {
            surface_kind: SurfaceKind::Water,
            phase: ContactPhase::Enter,
            min_speed: 1.5,
            min_intensity: 0.0,
        };
        assert!(t.matches(&water_sample(ContactPhase::Enter, 2.0, 0.5)));
        // Wrong phase.
        assert!(!t.matches(&water_sample(ContactPhase::Dwell, 2.0, 0.5)));
        // Below speed gate.
        assert!(!t.matches(&water_sample(ContactPhase::Enter, 1.0, 0.5)));
    }

    #[test]
    fn trigger_respects_min_intensity() {
        let t = ContactTrigger {
            surface_kind: SurfaceKind::Water,
            phase: ContactPhase::Dwell,
            min_speed: 0.5,
            min_intensity: 0.25,
        };
        assert!(t.matches(&water_sample(ContactPhase::Dwell, 1.0, 0.3)));
        assert!(!t.matches(&water_sample(ContactPhase::Dwell, 1.0, 0.1)));
    }

    #[test]
    fn count_curve_splash_scales_and_clamps() {
        // The default splash curve: clamp(speed*8, 0, 40).
        let c = CountCurve {
            gain: 8.0,
            base: 0.0,
            min: 0,
            max: 40,
        };
        assert_eq!(c.eval(&water_sample(ContactPhase::Enter, 0.0, 0.0)), 0);
        assert_eq!(c.eval(&water_sample(ContactPhase::Enter, 3.0, 0.0)), 24);
        // Clamped at 40 (speed 5 → 40, speed 100 → still 40).
        assert_eq!(c.eval(&water_sample(ContactPhase::Enter, 5.0, 0.0)), 40);
        assert_eq!(c.eval(&water_sample(ContactPhase::Enter, 100.0, 0.0)), 40);
    }

    #[test]
    fn count_curve_droplet_is_a_steady_low_trickle() {
        // The default droplet curve: gain 0, base 2, clamped 2..2.
        let c = CountCurve {
            gain: 0.0,
            base: 2.0,
            min: 2,
            max: 2,
        };
        assert_eq!(c.eval(&water_sample(ContactPhase::Dwell, 0.5, 0.3)), 2);
        assert_eq!(c.eval(&water_sample(ContactPhase::Dwell, 9.0, 0.9)), 2);
    }

    #[test]
    fn from_effects_maps_defaults_equivalently() {
        // The PDS canonical defaults must compile to a runtime registry
        // value-equal to the hardcoded fallback (so a record that omits
        // the key behaves identically).
        let reg = ContactRecipeRegistry::from_effects(&crate::pds::default_contact_effects());
        let fallback = ContactRecipeRegistry::default();
        assert_eq!(reg.recipes.len(), fallback.recipes.len());
        assert_eq!(
            reg.max_particles_per_frame,
            fallback.max_particles_per_frame
        );
        for (a, b) in reg.recipes.iter().zip(&fallback.recipes) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.enabled, b.enabled);
            assert_eq!(a.trigger.phase, b.trigger.phase);
            assert_eq!(a.trigger.surface_kind, b.trigger.surface_kind);
            assert_eq!(a.spawn.count, b.spawn.count);
            assert!((a.spawn.cooldown - b.spawn.cooldown).abs() < 1e-6);
            assert!((a.spawn.radius_scale - b.spawn.radius_scale).abs() < 1e-6);
        }
    }

    #[test]
    fn default_registry_has_enabled_water_recipes() {
        let reg = ContactRecipeRegistry::default();
        assert_eq!(reg.recipes.len(), 2);
        assert!(reg.recipes.iter().all(|r| r.enabled));
        assert!(reg.max_particles_per_frame > 0);
        // Enter recipe fires once (no cooldown); Dwell trickle is
        // cooldown-throttled so it can't spawn an emitter every frame.
        let splash = &reg.recipes[0];
        let droplet = &reg.recipes[1];
        assert_eq!(splash.trigger.phase, ContactPhase::Enter);
        assert_eq!(splash.spawn.cooldown, 0.0);
        assert_eq!(droplet.trigger.phase, ContactPhase::Dwell);
        assert!(droplet.spawn.cooldown > 0.0);
    }
}
