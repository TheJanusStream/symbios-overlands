//! PDS-authored contact-effect recipes (#246 — config/GUI slice).
//!
//! Serializable mirror of the runtime `interaction::recipes` types so
//! designers can tune splash / droplet effects from the room editor
//! without a recompile. The world compiler
//! ([`crate::world_builder::compile::apply_contact_recipes`]) translates
//! this record into the runtime `ContactRecipeRegistry`.
//!
//! Decoupled from `GeneratorKind::ParticleSystem` on purpose: the
//! particle template here ([`RecipeParticle`]) is a *trimmed*,
//! purpose-built struct (no rate / looping / duration / seed / collision
//! — a transient contact burst fixes those), so this schema can evolve
//! without touching the live ParticleSystem wire format. It does reuse
//! the existing [`EmitterShape`] / [`ParticleBlendMode`] open unions
//! (adding a *user* of them is wire-safe; only mutating them is not).

use serde::{Deserialize, Serialize};

use super::generator::{EmitterShape, ParticleBlendMode};
use super::types::{Fp, Fp4};

/// Serializable mirror of `interaction::contact::SurfaceKind`. Open
/// union (`$type` tag + [`Self::Unknown`]) so a record authored by a
/// future engine version round-trips cleanly.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum ContactSurfaceKind {
    #[serde(rename = "network.symbios.contact.surface.water")]
    #[default]
    Water,
    #[serde(other)]
    Unknown,
}

/// Serializable mirror of `interaction::contact::ContactPhase`. Open
/// union — see [`ContactSurfaceKind`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum ContactPhaseKind {
    #[serde(rename = "network.symbios.contact.phase.enter")]
    #[default]
    Enter,
    #[serde(rename = "network.symbios.contact.phase.dwell")]
    Dwell,
    #[serde(rename = "network.symbios.contact.phase.exit")]
    Exit,
    #[serde(other)]
    Unknown,
}

/// Declarative burst-count model — the serializable replacement for the
/// runtime `fn(&ContactSample) -> u32` curve (a fn pointer cannot
/// round-trip). `count = clamp(speed * gain + base, min, max)`, where
/// `speed` is the contact sample's `world_vel` magnitude. The original
/// hardcoded curves map exactly: water-splash `gain 8, base 0, 0..40`;
/// water-droplet `gain 0, base 2, 2..2` (a flat trickle).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CountModel {
    pub gain: Fp,
    pub base: Fp,
    pub min: u32,
    pub max: u32,
}

/// Trimmed particle template for a contact burst. Only the fields a
/// transient one-shot burst needs — `rate_per_second`, `looping`,
/// `duration`, `seed`, collision and texturing are fixed by the
/// dispatcher / hardcoded for v1 coloured-quad effects.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RecipeParticle {
    pub shape: EmitterShape,
    pub lifetime_min: Fp,
    pub lifetime_max: Fp,
    pub speed_min: Fp,
    pub speed_max: Fp,
    pub gravity_multiplier: Fp,
    pub linear_drag: Fp,
    pub start_size: Fp,
    pub end_size: Fp,
    pub start_color: Fp4,
    pub end_color: Fp4,
    pub blend_mode: ParticleBlendMode,
    pub billboard: bool,
    /// Hard per-emitter alive cap (also raised to the burst count at
    /// spawn so a big splash is never silently truncated).
    pub max_particles: u32,
}

/// One authored recipe: trigger predicate + burst description.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ContactEffectRecord {
    /// Stable identifier (debug + the per-avatar cooldown key).
    pub name: String,
    pub surface: ContactSurfaceKind,
    pub phase: ContactPhaseKind,
    pub min_speed: Fp,
    pub min_intensity: Fp,
    pub count: CountModel,
    /// Multiplier on the sample footprint that sizes the emitter shape.
    pub radius_scale: Fp,
    /// Fraction of avatar velocity the particles inherit.
    pub velocity_inherit: Fp,
    /// Min seconds between emissions per avatar (`0` = every matching
    /// frame; throttles continuous `Dwell` recipes).
    pub cooldown: Fp,
    /// Designer kill-switch — `false` skips the recipe.
    pub enabled: bool,
    pub particle: RecipeParticle,
}

/// Room-level authored effect set. Stored on [`super::RoomRecord`] under
/// `#[serde(default)]` so pre-Phase-4 records (no key) load with the
/// canonical defaults and behave exactly as the old hardcoded registry.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ContactEffects {
    pub recipes: Vec<ContactEffectRecord>,
    /// Global ceiling on particles spawned across all recipes/avatars
    /// in one frame (stutter-frame / many-avatar guard).
    pub max_particles_per_frame: u32,
}

impl Default for ContactEffects {
    fn default() -> Self {
        default_contact_effects()
    }
}

/// White→blue droplet, fading alpha to 0 — the shared look of both
/// hardcoded water effects (matches the old `transient_base` colours).
fn droplet_colours() -> (Fp4, Fp4) {
    (Fp4([0.85, 0.93, 1.0, 0.95]), Fp4([0.70, 0.85, 1.0, 0.0]))
}

/// The canonical seeded recipe set — the exact behaviour of the
/// pre-Phase-4 hardcoded `interaction::recipes::default_water_recipes`,
/// expressed as authored data so seeded and upgraded rooms are
/// pixel-identical until a designer edits them.
pub fn default_contact_effects() -> ContactEffects {
    let (start_color, end_color) = droplet_colours();
    ContactEffects {
        max_particles_per_frame: 240,
        recipes: vec![
            ContactEffectRecord {
                name: "water_splash".into(),
                surface: ContactSurfaceKind::Water,
                phase: ContactPhaseKind::Enter,
                min_speed: Fp(1.5),
                min_intensity: Fp(0.0),
                // clamp(speed*8, 0, 40)
                count: CountModel {
                    gain: Fp(8.0),
                    base: Fp(0.0),
                    min: 0,
                    max: 40,
                },
                radius_scale: Fp(1.0),
                velocity_inherit: Fp(0.5),
                cooldown: Fp(0.0),
                enabled: true,
                particle: RecipeParticle {
                    // Upward fan; `height` scaled by footprint at compile.
                    shape: EmitterShape::Cone {
                        half_angle: Fp(0.7),
                        height: Fp(0.4),
                    },
                    lifetime_min: Fp(0.3),
                    lifetime_max: Fp(0.6),
                    speed_min: Fp(2.0),
                    speed_max: Fp(4.0),
                    gravity_multiplier: Fp(1.0),
                    linear_drag: Fp(0.4),
                    start_size: Fp(0.13),
                    end_size: Fp(0.03),
                    start_color,
                    end_color,
                    blend_mode: ParticleBlendMode::Alpha,
                    billboard: true,
                    max_particles: 64,
                },
            },
            ContactEffectRecord {
                name: "water_droplet".into(),
                surface: ContactSurfaceKind::Water,
                phase: ContactPhaseKind::Dwell,
                min_speed: Fp(0.5),
                min_intensity: Fp(0.25),
                // Flat trickle: const 2.
                count: CountModel {
                    gain: Fp(0.0),
                    base: Fp(2.0),
                    min: 2,
                    max: 2,
                },
                radius_scale: Fp(0.6),
                velocity_inherit: Fp(0.7),
                cooldown: Fp(0.25),
                enabled: true,
                particle: RecipeParticle {
                    shape: EmitterShape::Sphere { radius: Fp(0.2) },
                    lifetime_min: Fp(0.3),
                    lifetime_max: Fp(0.5),
                    speed_min: Fp(0.6),
                    speed_max: Fp(1.4),
                    gravity_multiplier: Fp(1.0),
                    linear_drag: Fp(0.4),
                    start_size: Fp(0.06),
                    end_size: Fp(0.015),
                    start_color,
                    end_color,
                    blend_mode: ParticleBlendMode::Alpha,
                    billboard: true,
                    max_particles: 16,
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_two_enabled_water_recipes() {
        let e = ContactEffects::default();
        assert_eq!(e.recipes.len(), 2);
        assert!(e.recipes.iter().all(|r| r.enabled));
        assert_eq!(e.max_particles_per_frame, 240);
        assert_eq!(e.recipes[0].name, "water_splash");
        assert_eq!(e.recipes[0].phase, ContactPhaseKind::Enter);
        assert_eq!(e.recipes[1].name, "water_droplet");
        assert_eq!(e.recipes[1].phase, ContactPhaseKind::Dwell);
    }

    #[test]
    fn json_round_trips_all_fields() {
        let e = ContactEffects::default();
        let json = serde_json::to_string(&e).unwrap();
        let back: ContactEffects = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn unknown_enum_variants_fall_back_not_error() {
        // A future surface/phase tag must not break deserialization.
        let s: ContactSurfaceKind =
            serde_json::from_str(r#"{"$type":"network.symbios.contact.surface.lava"}"#).unwrap();
        assert_eq!(s, ContactSurfaceKind::Unknown);
        let p: ContactPhaseKind =
            serde_json::from_str(r#"{"$type":"network.symbios.contact.phase.graze"}"#).unwrap();
        assert_eq!(p, ContactPhaseKind::Unknown);
    }
}
