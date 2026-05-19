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
    AnimationFrameMode, AudioClipSource, AudioParams, ContactEffectKind, ContactEffects,
    ContactPhaseKind, ContactSurfaceKind, DecalParams, EmitterShape, Fp, ParticleBlendMode,
    RecipeParticle, SimulationSpace, TextureFilter,
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

/// Runtime mirror of [`crate::pds::DecalParams`] — plain `f32`s so the
/// decal channel doesn't reach back into PDS types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecalRuntimeParams {
    pub ttl: f32,
    pub start_size: f32,
    pub end_size: f32,
    pub start_alpha: f32,
    pub end_alpha: f32,
    pub color: [f32; 3],
    pub normal_offset: f32,
}

impl From<&DecalParams> for DecalRuntimeParams {
    fn from(p: &DecalParams) -> Self {
        Self {
            ttl: p.ttl.0,
            start_size: p.start_size.0,
            end_size: p.end_size.0,
            start_alpha: p.start_alpha.0,
            end_alpha: p.end_alpha.0,
            color: p.color.0,
            normal_offset: p.normal_offset.0,
        }
    }
}

/// One declarative decal rule — the [`ContactEffectKind::DecalStamp`]
/// analogue of [`ContactEffectRecipe`], consumed by
/// [`super::decal::stamp_decals`].
#[derive(Clone)]
pub struct DecalEffectRecipe {
    pub name: String,
    pub trigger: ContactTrigger,
    pub params: DecalRuntimeParams,
    /// Min seconds between stamps from one avatar for this recipe
    /// (per-recipe, like [`ParticleBurst::cooldown`]).
    pub cooldown: f32,
    pub enabled: bool,
}

/// Runtime mirror of [`crate::pds::AudioParams`]. The clip source is
/// the PDS enum verbatim (small, and the consumer needs exactly it to
/// key the audio cache — no parallel enum to drift).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioRuntimeParams {
    pub source: AudioClipSource,
    pub volume: f32,
    pub volume_per_speed: f32,
    pub pitch: f32,
    pub pitch_jitter: f32,
    pub spatial: bool,
}

impl AudioRuntimeParams {
    /// Effective linear volume for a matched sample:
    /// `clamp(volume + speed·volume_per_speed, 0, 4)` (the 4 ceiling
    /// matches the sanitiser's `MAX_CONTACT_AUDIO_VOLUME`).
    pub fn volume_for(&self, sample: &ContactSample) -> f32 {
        (self.volume + sample.world_vel.length() * self.volume_per_speed).clamp(0.0, 4.0)
    }
}

impl From<&AudioParams> for AudioRuntimeParams {
    fn from(p: &AudioParams) -> Self {
        Self {
            source: p.source.clone(),
            volume: p.volume.0,
            volume_per_speed: p.volume_per_speed.0,
            pitch: p.pitch.0,
            pitch_jitter: p.pitch_jitter.0,
            spatial: p.spatial,
        }
    }
}

/// One declarative audio-cue rule — the [`ContactEffectKind::AudioCue`]
/// analogue of [`ContactEffectRecipe`], consumed by
/// [`super::audio::play_contact_audio`].
#[derive(Clone)]
pub struct AudioCueRecipe {
    pub name: String,
    pub trigger: ContactTrigger,
    pub params: AudioRuntimeParams,
    /// Min seconds between cues from one avatar for this recipe.
    pub cooldown: f32,
    pub enabled: bool,
}

/// Active recipe set + the global emission ceiling.
///
/// One registry carries every effect kind, split by runtime channel:
/// [`Self::recipes`] feeds the particle dispatcher, [`Self::decals`]
/// the decal stamper, [`Self::audio`] the bevy_audio cue consumer
/// (#262).
#[derive(Resource)]
pub struct ContactRecipeRegistry {
    pub recipes: Vec<ContactEffectRecipe>,
    pub decals: Vec<DecalEffectRecipe>,
    pub audio: Vec<AudioCueRecipe>,
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
            // No decal / audio cue is seeded by default — the shipped
            // behaviour is particle-only; both are opt-in per room
            // (#261 / #262).
            decals: Vec::new(),
            audio: Vec::new(),
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

/// Dusty tan ground puff that hangs then settles — kicked up by a
/// brisk run on terrain. Mirrors
/// `crate::pds::contact_effects::ground_dust_record`'s `RecipeParticle`
/// (the from-effects equivalence test guards the recipe-level fields).
fn ground_dust_template() -> ParticleEmitter {
    ParticleEmitter {
        shape: EmitterShape::Sphere { radius: Fp(0.25) },
        max_particles: 48,
        lifetime_min: 0.4,
        lifetime_max: 0.9,
        speed_min: 0.3,
        speed_max: 1.2,
        // Dust hangs — almost no gravity, heavy drag.
        gravity_multiplier: 0.15,
        linear_drag: 0.6,
        start_size: 0.18,
        end_size: 0.05,
        start_color: LinearRgba::new(0.55, 0.45, 0.32, 0.70),
        end_color: LinearRgba::new(0.50, 0.42, 0.30, 0.0),
        ..transient_base()
    }
}

/// The hardcoded initial recipe set — the pre-Phase-4 fallback used by
/// [`ContactRecipeRegistry::default`] until a room compiles its
/// authored [`crate::pds::ContactEffects`]. Must stay value-equal to
/// [`crate::pds::default_contact_effects`] so a record that omits the
/// key behaves identically (enforced by
/// [`tests::from_effects_maps_defaults_equivalently`]). As of Phase 3
/// (#245) this includes the `ground_dust` terrain recipe that #244
/// deliberately deferred.
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
        ContactEffectRecipe {
            name: "ground_dust".into(),
            trigger: ContactTrigger {
                surface_kind: SurfaceKind::Terrain,
                phase: ContactPhase::Dwell,
                // Brisk run, not a walk. Terrain intensity floors at
                // the grounded value so the speed gate alone selects
                // "running" (min_intensity stays 0).
                min_speed: 4.0,
                min_intensity: 0.0,
            },
            spawn: ParticleBurst {
                template: ground_dust_template(),
                // clamp(speed*3, 4, 18)
                count: CountCurve {
                    gain: 3.0,
                    base: 0.0,
                    min: 4,
                    max: 18,
                },
                radius_scale: 0.8,
                velocity_inherit: 0.3,
                cooldown: 0.2,
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
        ContactSurfaceKind::Terrain => Some(SurfaceKind::Terrain),
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
    /// registry, routing each recipe to its channel by effect kind. A
    /// recipe is dropped (not guessed) when its surface/phase tag is
    /// unknown, or its effect kind is unknown — the engine can't
    /// dispatch a kind it doesn't model. Numeric fields are assumed
    /// already sanitised by [`crate::pds::RoomRecord::sanitize`].
    pub fn from_effects(effects: &ContactEffects) -> Self {
        let mut recipes = Vec::new();
        let mut decals = Vec::new();
        let mut audio = Vec::new();

        for r in &effects.recipes {
            // Shared trigger; a record with an unknown surface/phase is
            // skipped wholesale regardless of effect kind.
            let (Some(surface_kind), Some(phase)) = (map_surface(r.surface), map_phase(r.phase))
            else {
                continue;
            };
            let trigger = ContactTrigger {
                surface_kind,
                phase,
                min_speed: r.min_speed.0,
                min_intensity: r.min_intensity.0,
            };

            match &r.effect {
                ContactEffectKind::ParticleBurst {
                    count,
                    radius_scale,
                    velocity_inherit,
                    particle,
                } => recipes.push(ContactEffectRecipe {
                    name: r.name.clone(),
                    trigger,
                    spawn: ParticleBurst {
                        template: recipe_particle_to_emitter(particle),
                        count: CountCurve {
                            gain: count.gain.0,
                            base: count.base.0,
                            min: count.min,
                            max: count.max,
                        },
                        radius_scale: radius_scale.0,
                        velocity_inherit: velocity_inherit.0,
                        cooldown: r.cooldown.0,
                    },
                    enabled: r.enabled,
                }),
                ContactEffectKind::DecalStamp { decal } => decals.push(DecalEffectRecipe {
                    name: r.name.clone(),
                    trigger,
                    params: DecalRuntimeParams::from(decal),
                    cooldown: r.cooldown.0,
                    enabled: r.enabled,
                }),
                ContactEffectKind::AudioCue { audio: a } => audio.push(AudioCueRecipe {
                    name: r.name.clone(),
                    trigger,
                    params: AudioRuntimeParams::from(a),
                    cooldown: r.cooldown.0,
                    enabled: r.enabled,
                }),
                // Future/unknown effect kind — can't dispatch it.
                ContactEffectKind::Unknown => {}
            }
        }

        Self {
            recipes,
            decals,
            audio,
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
        // No decal / audio cue is seeded by default on either side.
        assert!(reg.decals.is_empty());
        assert!(fallback.decals.is_empty());
        assert!(reg.audio.is_empty());
        assert!(fallback.audio.is_empty());
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
    fn default_registry_has_enabled_water_and_ground_recipes() {
        let reg = ContactRecipeRegistry::default();
        assert_eq!(reg.recipes.len(), 3);
        assert!(reg.recipes.iter().all(|r| r.enabled));
        assert!(reg.max_particles_per_frame > 0);
        // Enter recipe fires once (no cooldown); Dwell trickle is
        // cooldown-throttled so it can't spawn an emitter every frame.
        let splash = &reg.recipes[0];
        let droplet = &reg.recipes[1];
        let dust = &reg.recipes[2];
        assert_eq!(splash.trigger.phase, ContactPhase::Enter);
        assert_eq!(splash.trigger.surface_kind, SurfaceKind::Water);
        assert_eq!(splash.spawn.cooldown, 0.0);
        assert_eq!(droplet.trigger.phase, ContactPhase::Dwell);
        assert!(droplet.spawn.cooldown > 0.0);
        // Ground dust: terrain Dwell, speed-gated to a run, throttled.
        assert_eq!(dust.name, "ground_dust");
        assert_eq!(dust.trigger.surface_kind, SurfaceKind::Terrain);
        assert_eq!(dust.trigger.phase, ContactPhase::Dwell);
        assert_eq!(dust.trigger.min_speed, 4.0);
        assert!(dust.spawn.cooldown > 0.0);
    }

    #[test]
    fn ground_dust_recipe_fires_only_on_fast_terrain_dwell() {
        let reg = ContactRecipeRegistry::default();
        let dust = reg
            .recipes
            .iter()
            .find(|r| r.name == "ground_dust")
            .unwrap();
        let terrain = |phase, speed| ContactSample {
            avatar: Entity::PLACEHOLDER,
            world_pos: Vec3::ZERO,
            world_vel: Vec3::new(speed, 0.0, 0.0),
            footprint_radius: 0.5,
            surface: super::super::contact::SurfaceContact::Terrain {
                material_blend: [1.0, 0.0, 0.0, 0.0],
                normal: Vec3::Y,
            },
            intensity: 0.12,
            phase,
        };
        // Running on terrain → match.
        assert!(dust.trigger.matches(&terrain(ContactPhase::Dwell, 6.0)));
        // Walking (below the 4 m/s gate) → no dust.
        assert!(!dust.trigger.matches(&terrain(ContactPhase::Dwell, 2.0)));
        // A water sample never matches a terrain recipe.
        let water = ContactSample {
            surface: super::super::contact::SurfaceContact::Water {
                plane_idx: 0,
                depth: 1.0,
                flow_dir: Vec2::ZERO,
            },
            ..terrain(ContactPhase::Dwell, 6.0)
        };
        assert!(!dust.trigger.matches(&water));
    }

    #[test]
    fn decal_record_routes_to_the_decal_channel() {
        use crate::pds::{
            ContactEffectKind, ContactEffectRecord, ContactPhaseKind, ContactSurfaceKind,
            DecalParams, Fp,
        };
        let mut effects = crate::pds::default_contact_effects();
        effects.recipes.push(ContactEffectRecord {
            name: "scuff".into(),
            surface: ContactSurfaceKind::Terrain,
            phase: ContactPhaseKind::Enter,
            min_speed: Fp(0.0),
            min_intensity: Fp(0.0),
            cooldown: Fp(0.4),
            enabled: true,
            effect: ContactEffectKind::DecalStamp {
                decal: DecalParams::default(),
            },
        });
        let reg = ContactRecipeRegistry::from_effects(&effects);
        // The 3 seeded particle recipes still land in `recipes`; the
        // decal lands in `decals`, not as a particle.
        assert_eq!(reg.recipes.len(), 3);
        assert_eq!(reg.decals.len(), 1);
        let d = &reg.decals[0];
        assert_eq!(d.name, "scuff");
        assert_eq!(d.trigger.surface_kind, SurfaceKind::Terrain);
        assert_eq!(d.trigger.phase, ContactPhase::Enter);
        assert!((d.cooldown - 0.4).abs() < 1e-6);
        assert!((d.params.ttl - DecalParams::default().ttl.0).abs() < 1e-6);
        assert!(reg.audio.is_empty());
    }

    #[test]
    fn audio_record_routes_to_the_audio_channel() {
        use crate::pds::{
            AudioClipSource, AudioParams, ContactEffectKind, ContactEffectRecord, ContactPhaseKind,
            ContactSurfaceKind, Fp,
        };
        let mut effects = crate::pds::default_contact_effects();
        effects.recipes.push(ContactEffectRecord {
            name: "splash_sfx".into(),
            surface: ContactSurfaceKind::Water,
            phase: ContactPhaseKind::Enter,
            min_speed: Fp(1.0),
            min_intensity: Fp(0.0),
            cooldown: Fp(0.2),
            enabled: true,
            effect: ContactEffectKind::AudioCue {
                audio: AudioParams {
                    source: AudioClipSource::Url {
                        url: "https://x.test/splash.ogg".into(),
                    },
                    volume: Fp(0.5),
                    volume_per_speed: Fp(0.1),
                    pitch: Fp(1.0),
                    pitch_jitter: Fp(0.2),
                    spatial: true,
                },
            },
        });
        let reg = ContactRecipeRegistry::from_effects(&effects);
        // Particle defaults untouched; the cue routes to `audio`, not
        // `recipes`/`decals`.
        assert_eq!(reg.recipes.len(), 3);
        assert!(reg.decals.is_empty());
        assert_eq!(reg.audio.len(), 1);
        let a = &reg.audio[0];
        assert_eq!(a.name, "splash_sfx");
        assert_eq!(a.trigger.surface_kind, SurfaceKind::Water);
        assert!((a.cooldown - 0.2).abs() < 1e-6);
        assert!(a.params.spatial);
        // volume_for scales with contact speed: 0.5 + speed*0.1.
        let s = ContactSample {
            avatar: Entity::PLACEHOLDER,
            world_pos: Vec3::ZERO,
            world_vel: Vec3::new(3.0, 0.0, 0.0),
            footprint_radius: 0.5,
            surface: super::super::contact::SurfaceContact::Water {
                plane_idx: 0,
                depth: 1.0,
                flow_dir: Vec2::ZERO,
            },
            intensity: 0.5,
            phase: ContactPhase::Enter,
        };
        assert!((a.params.volume_for(&s) - 0.8).abs() < 1e-5);
    }
}
