//! PDS-authored contact-effect recipes (#246 — config/GUI slice).
//!
//! Serializable mirror of the runtime `interaction::recipes` types so
//! designers can tune splash / droplet effects from the room editor
//! without a recompile. The world compiler
//! (`world_builder::compile::contact_recipes::apply_contact_recipes`)
//! translates this record into the runtime `ContactRecipeRegistry`.
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
use super::texture::{SovereignPuffConfig, SovereignSoftDiscConfig, SovereignTextureConfig};
use super::types::{Fp, Fp3, Fp4, Fp64};

/// Serializable mirror of `interaction::contact::SurfaceKind`. Open
/// union (`$type` tag + [`Self::Unknown`]) so a record authored by a
/// future engine version round-trips cleanly.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum ContactSurfaceKind {
    #[serde(rename = "network.symbios.contact.surface.water")]
    #[default]
    Water,
    #[serde(rename = "network.symbios.contact.surface.terrain")]
    Terrain,
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
    /// Procedural sprite billboard for the burst particles (#367). The
    /// baked alpha silhouette replaces the flat coloured quad; the
    /// emitter's `start_color`→`end_color` ramp still tints it through
    /// the texture multiply (so a near-white sprite carries the authored
    /// colour). `#[serde(default)]` → a pre-#367 record (no key) decodes
    /// to [`SovereignTextureConfig::None`], keeping its untextured
    /// flat-quad look byte-identical. The seeded defaults set an
    /// appropriate card: SoftDisc droplets for water, a Puff cloud for
    /// ground dust.
    #[serde(default)]
    pub procedural_texture: SovereignTextureConfig,
}

/// A flat, short-lived contact decal (consumer channel C, #261/#246).
/// Authored per-recipe; the runtime `interaction::decal` channel stamps
/// a quad that grows from `start_size`→`end_size` and fades
/// `start_alpha`→`end_alpha` over `ttl`, lifted `normal_offset` off the
/// surface to avoid z-fighting. v1 is a flat-colour quad (no texture
/// atlas yet — a textured source is a later extension).
///
/// [`Default`] is the canonical seed (mirrors the engine values that
/// were previously `config::interaction::decal` consts), so a freshly
/// added decal recipe already looks reasonable.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DecalParams {
    pub ttl: Fp,
    pub start_size: Fp,
    pub end_size: Fp,
    pub start_alpha: Fp,
    pub end_alpha: Fp,
    /// sRGB tint of the mark.
    pub color: Fp3,
    /// Lift (m) along the surface normal to avoid z-fighting.
    pub normal_offset: Fp,
}

impl Default for DecalParams {
    fn default() -> Self {
        Self {
            ttl: Fp(6.0),
            start_size: Fp(0.45),
            end_size: Fp(0.85),
            start_alpha: Fp(0.55),
            end_alpha: Fp(0.0),
            color: Fp3([0.14, 0.11, 0.09]),
            normal_offset: Fp(0.02),
        }
    }
}

/// Where an authored audio cue's clip comes from. Open union (`$type`
/// tag + [`Self::Unknown`]) — the forward-compat seam for a future
/// **procedurally synthesised** source (a planned
/// `bevy_symbios_synthesizer`): such a record decodes to `Unknown` on
/// today's clients and is skipped, never an error. Named
/// `AudioClipSource` (not `AudioSource`) to avoid colliding with
/// `bevy_audio::AudioSource` in the runtime consumer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum AudioClipSource {
    /// Direct HTTPS audio URL. v1 decodes Ogg/Vorbis (Bevy's default
    /// audio feature); other containers need extra `bevy` features.
    #[serde(rename = "network.symbios.contact.audio.url")]
    Url { url: String },
    /// ATProto blob pinned to a DID — resolves the PDS then
    /// `com.atproto.sync.getBlob`, same path Sign textures use.
    #[serde(rename = "network.symbios.contact.audio.atproto_blob")]
    AtprotoBlob { did: String, cid: String },
    /// A future/unknown source (e.g. forthcoming procedural synthesis)
    /// — decoded, never authored on this client; the cue is skipped.
    #[serde(other)]
    Unknown,
}

impl Default for AudioClipSource {
    fn default() -> Self {
        Self::Url { url: String::new() }
    }
}

/// Authored audio cue payload. Loudness scales with contact speed
/// (`volume = clamp(volume + speed*volume_per_speed, 0, cap)`); pitch
/// is a playback-speed multiplier with optional per-play random jitter
/// so repeats don't sound mechanical.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AudioParams {
    pub source: AudioClipSource,
    /// Linear volume floor (0 = silent, 1 = unity).
    pub volume: Fp,
    /// Extra linear volume per m/s of contact speed.
    pub volume_per_speed: Fp,
    /// Playback-speed multiplier (rodio couples speed+pitch); 1 = as
    /// recorded.
    pub pitch: Fp,
    /// Uniform ± random added to `pitch` each play (0 = none).
    pub pitch_jitter: Fp,
    /// Positional audio at the contact point (needs the camera's
    /// `SpatialListener`); `false` = non-positional.
    pub spatial: bool,
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            source: AudioClipSource::default(),
            volume: Fp(0.8),
            volume_per_speed: Fp(0.0),
            pitch: Fp(1.0),
            pitch_jitter: Fp(0.0),
            spatial: true,
        }
    }
}

/// The effect a matched contact produces. Open union (`$type` tag +
/// [`Self::Unknown`]) — same forward-compat contract as
/// [`ContactSurfaceKind`]: a record authored against a future effect
/// kind decodes to `Unknown` here and is skipped at compile time
/// rather than failing the whole room.
// The `ParticleBurst` variant carries a `RecipeParticle` whose
// `procedural_texture` is a full `SovereignTextureConfig` (~288 bytes,
// #367). Boxing it would force serde through a wrapping layer and churn
// the round-trip tests / deserialize shim for no real gain — recipes
// live in a small per-room `Vec`, never a hot path — so the size penalty
// is fine, consistent with how `GeneratorKind` handles the same config.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum ContactEffectKind {
    /// Transient particle burst (the Phase-2 effect; the only kind
    /// before #261).
    #[serde(rename = "network.symbios.contact.effect.particle")]
    ParticleBurst {
        count: CountModel,
        /// Multiplier on the sample footprint that sizes the emitter
        /// shape.
        radius_scale: Fp,
        /// Fraction of avatar velocity the particles inherit.
        velocity_inherit: Fp,
        particle: RecipeParticle,
    },
    /// Flat fading ground decal.
    #[serde(rename = "network.symbios.contact.effect.decal")]
    DecalStamp { decal: DecalParams },
    /// One-shot audio cue played through `bevy_audio` (#262).
    #[serde(rename = "network.symbios.contact.effect.audio")]
    AudioCue { audio: AudioParams },
    /// A future/unknown effect kind — decoded, never authored. Dropped
    /// by the runtime mapper.
    #[serde(other)]
    Unknown,
}

/// One authored recipe: shared trigger predicate + a tagged effect
/// payload ([`ContactEffectKind`]).
///
/// `Serialize` is derived (we always *write* the current tagged shape);
/// `Deserialize` is hand-written ([`RawContactEffectRecord`]) so a
/// **pre-#261 record** — which carried `count` / `radius_scale` /
/// `velocity_inherit` / `particle` *flat on the record* with no
/// `effect` key — still loads, folded into a
/// [`ContactEffectKind::ParticleBurst`]. (Forward-compat caveat: a
/// pre-#261 *client* cannot read a migrated room's new `effect` shape;
/// acceptable for this single-binary app where clients upgrade
/// together, and consistent with the surface/phase open-union
/// evolution.)
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ContactEffectRecord {
    /// Stable identifier (debug + the per-avatar cooldown key).
    pub name: String,
    pub surface: ContactSurfaceKind,
    pub phase: ContactPhaseKind,
    pub min_speed: Fp,
    pub min_intensity: Fp,
    /// Min seconds between emissions per avatar (`0` = every matching
    /// frame; throttles continuous `Dwell` recipes).
    pub cooldown: Fp,
    /// Designer kill-switch — `false` skips the recipe.
    pub enabled: bool,
    pub effect: ContactEffectKind,
}

fn default_enabled() -> bool {
    true
}

/// Deserialize shim accepting **both** the #261 tagged shape and the
/// legacy pre-#261 flat shape. Every field is optional so a partial /
/// legacy record never hard-errors; missing pieces fall back to sane
/// canonical values.
#[derive(Deserialize)]
struct RawContactEffectRecord {
    name: String,
    #[serde(default)]
    surface: ContactSurfaceKind,
    #[serde(default)]
    phase: ContactPhaseKind,
    #[serde(default)]
    min_speed: Fp,
    #[serde(default)]
    min_intensity: Fp,
    #[serde(default)]
    cooldown: Fp,
    #[serde(default = "default_enabled")]
    enabled: bool,
    /// #261 tagged payload.
    #[serde(default)]
    effect: Option<ContactEffectKind>,
    // --- legacy pre-#261 flat particle fields ---
    #[serde(default)]
    count: Option<CountModel>,
    #[serde(default)]
    radius_scale: Option<Fp>,
    #[serde(default)]
    velocity_inherit: Option<Fp>,
    #[serde(default)]
    particle: Option<RecipeParticle>,
}

impl<'de> Deserialize<'de> for ContactEffectRecord {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawContactEffectRecord::deserialize(d)?;
        let effect = match raw.effect {
            // New tagged shape — use it as authored.
            Some(e) => e,
            // No `effect` key: fold the legacy flat particle fields
            // into a ParticleBurst. A legacy record always carried
            // `particle`; if it somehow doesn't, default the burst so
            // the recipe is still well-formed (the sanitiser + runtime
            // mapper bound/skip it as needed).
            None => ContactEffectKind::ParticleBurst {
                count: raw.count.unwrap_or(CountModel {
                    gain: Fp(0.0),
                    base: Fp(1.0),
                    min: 1,
                    max: 1,
                }),
                radius_scale: raw.radius_scale.unwrap_or(Fp(1.0)),
                velocity_inherit: raw.velocity_inherit.unwrap_or(Fp(0.0)),
                particle: raw.particle.unwrap_or_else(canonical_particle),
            },
        };
        Ok(ContactEffectRecord {
            name: raw.name,
            surface: raw.surface,
            phase: raw.phase,
            min_speed: raw.min_speed,
            min_intensity: raw.min_intensity,
            cooldown: raw.cooldown,
            enabled: raw.enabled,
            effect,
        })
    }
}

/// Soft round droplet sprite for water splash / droplet bursts (#367).
/// Near-white so the emitter's blue-white colour ramp tints it through
/// the texture multiply (the same convention the seeded ambient
/// particles use). A single variant — the transient burst draws frame 0
/// (`AnimationFrameMode::Still`), so a multi-cell atlas would bake cells
/// the burst never shows.
///
/// Shared with `interaction::recipes` (the hardcoded runtime fallback
/// templates) so the authored default and the fallback can't drift — the
/// `from_effects_maps_defaults_equivalently` test guards the pairing.
pub(crate) fn droplet_sprite() -> SovereignTextureConfig {
    SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
        variant_rows: 1,
        variant_cols: 1,
        color_core: Fp3([1.0, 1.0, 1.0]),
        color_halo: Fp3([0.92, 0.96, 1.0]),
        ..Default::default()
    })
}

/// Soft cloud puff sprite for the ground-dust burst (#367). Neutral grey
/// so the emitter's tan colour ramp drives the hue through the texture
/// multiply. Single variant for the same `Still`-mode reason as
/// [`droplet_sprite`].
pub(crate) fn dust_sprite() -> SovereignTextureConfig {
    SovereignTextureConfig::Puff(SovereignPuffConfig {
        variant_rows: 1,
        variant_cols: 1,
        color_base: Fp3([0.94, 0.94, 0.94]),
        color_shadow: Fp3([0.58, 0.58, 0.58]),
        edge_falloff: Fp64(2.2),
        ..Default::default()
    })
}

/// The canonical splash particle — the fallback used when a malformed
/// legacy record omits `particle` entirely. Kept tiny and benign.
fn canonical_particle() -> RecipeParticle {
    let (start_color, end_color) = droplet_colours();
    RecipeParticle {
        shape: EmitterShape::Sphere { radius: Fp(0.2) },
        lifetime_min: Fp(0.3),
        lifetime_max: Fp(0.6),
        speed_min: Fp(1.0),
        speed_max: Fp(2.0),
        gravity_multiplier: Fp(1.0),
        linear_drag: Fp(0.4),
        start_size: Fp(0.1),
        end_size: Fp(0.02),
        start_color,
        end_color,
        blend_mode: ParticleBlendMode::Alpha,
        billboard: true,
        max_particles: 32,
        procedural_texture: droplet_sprite(),
    }
}

/// Build a [`ContactEffectKind::ParticleBurst`] — keeps the default
/// recipe builders readable now the payload is nested.
fn particle_effect(
    count: CountModel,
    radius_scale: Fp,
    velocity_inherit: Fp,
    particle: RecipeParticle,
) -> ContactEffectKind {
    ContactEffectKind::ParticleBurst {
        count,
        radius_scale,
        velocity_inherit,
        particle,
    }
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

impl ContactEffects {
    /// `true` when the set equals the canonical default registry — the
    /// wire-format skip predicate for `RoomRecord::contact_effects` (#695):
    /// a room that never customised its contact effects doesn't spend
    /// ~2.6 KiB re-stating the built-in recipes.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// White→blue droplet, fading alpha to 0 — the shared look of both
/// hardcoded water effects (matches the old `transient_base` colours).
fn droplet_colours() -> (Fp4, Fp4) {
    (Fp4([0.85, 0.93, 1.0, 0.95]), Fp4([0.70, 0.85, 1.0, 0.0]))
}

/// Dusty tan → transparent — the kicked-up ground-dust puff. Kept in
/// sync with `interaction::recipes::ground_dust_template` (the
/// from-effects equivalence test guards the recipe-level fields).
fn dust_colours() -> (Fp4, Fp4) {
    (Fp4([0.55, 0.45, 0.32, 0.70]), Fp4([0.50, 0.42, 0.30, 0.0]))
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
                cooldown: Fp(0.0),
                enabled: true,
                effect: particle_effect(
                    // clamp(speed*8, 0, 40)
                    CountModel {
                        gain: Fp(8.0),
                        base: Fp(0.0),
                        min: 0,
                        max: 40,
                    },
                    Fp(1.0),
                    Fp(0.5),
                    RecipeParticle {
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
                        procedural_texture: droplet_sprite(),
                    },
                ),
            },
            ContactEffectRecord {
                name: "water_droplet".into(),
                surface: ContactSurfaceKind::Water,
                phase: ContactPhaseKind::Dwell,
                min_speed: Fp(0.5),
                min_intensity: Fp(0.25),
                cooldown: Fp(0.25),
                enabled: true,
                effect: particle_effect(
                    // Flat trickle: const 2.
                    CountModel {
                        gain: Fp(0.0),
                        base: Fp(2.0),
                        min: 2,
                        max: 2,
                    },
                    Fp(0.6),
                    Fp(0.7),
                    RecipeParticle {
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
                        procedural_texture: droplet_sprite(),
                    },
                ),
            },
            // Ground dust — the recipe #244 deliberately deferred until
            // the Phase 3 `Terrain` surface existed (#245). A brisk run
            // (raw speed ≥ 4 m/s) kicks up a short-lived tan puff;
            // throttled by a Dwell cooldown so it puffs a few times a
            // second rather than every frame.
            ground_dust_record(),
        ],
    }
}

/// The seeded `ground_dust` recipe. Factored out so it can be reused
/// and so the runtime mirror
/// (`interaction::recipes::default_water_recipes`) can be kept
/// byte-for-byte value-equal (enforced by
/// `recipes::tests::from_effects_maps_defaults_equivalently`).
fn ground_dust_record() -> ContactEffectRecord {
    let (start_color, end_color) = dust_colours();
    ContactEffectRecord {
        name: "ground_dust".into(),
        surface: ContactSurfaceKind::Terrain,
        phase: ContactPhaseKind::Dwell,
        // Raw contact speed gate: a brisk run, not a walk. Terrain
        // intensity floors at the grounded value, so `min_intensity`
        // stays 0 (the speed gate alone selects "running").
        min_speed: Fp(4.0),
        min_intensity: Fp(0.0),
        // Throttle the continuous Dwell so it puffs a few times a
        // second instead of spawning an emitter every frame.
        cooldown: Fp(0.2),
        enabled: true,
        effect: particle_effect(
            // clamp(speed*3, 4, 18) — denser puff the faster you run.
            CountModel {
                gain: Fp(3.0),
                base: Fp(0.0),
                min: 4,
                max: 18,
            },
            Fp(0.8),
            Fp(0.3),
            RecipeParticle {
                shape: EmitterShape::Sphere { radius: Fp(0.25) },
                lifetime_min: Fp(0.4),
                lifetime_max: Fp(0.9),
                speed_min: Fp(0.3),
                speed_max: Fp(1.2),
                // Dust hangs — almost no gravity, heavy drag.
                gravity_multiplier: Fp(0.15),
                linear_drag: Fp(0.6),
                start_size: Fp(0.18),
                end_size: Fp(0.05),
                start_color,
                end_color,
                blend_mode: ParticleBlendMode::Alpha,
                billboard: true,
                max_particles: 48,
                procedural_texture: dust_sprite(),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_seeded_water_and_ground_recipes() {
        let e = ContactEffects::default();
        assert_eq!(e.recipes.len(), 3);
        assert!(e.recipes.iter().all(|r| r.enabled));
        assert_eq!(e.max_particles_per_frame, 240);
        assert_eq!(e.recipes[0].name, "water_splash");
        assert_eq!(e.recipes[0].surface, ContactSurfaceKind::Water);
        assert_eq!(e.recipes[0].phase, ContactPhaseKind::Enter);
        assert_eq!(e.recipes[1].name, "water_droplet");
        assert_eq!(e.recipes[1].surface, ContactSurfaceKind::Water);
        assert_eq!(e.recipes[1].phase, ContactPhaseKind::Dwell);
        assert_eq!(e.recipes[2].name, "ground_dust");
        assert_eq!(e.recipes[2].surface, ContactSurfaceKind::Terrain);
        assert_eq!(e.recipes[2].phase, ContactPhaseKind::Dwell);
        assert_eq!(e.recipes[2].min_speed, Fp(4.0));
    }

    #[test]
    fn terrain_surface_kind_round_trips() {
        let json = serde_json::to_string(&ContactSurfaceKind::Terrain).unwrap();
        assert!(json.contains("network.symbios.contact.surface.terrain"));
        let back: ContactSurfaceKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ContactSurfaceKind::Terrain);
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

    #[test]
    fn unknown_effect_kind_falls_back_not_error() {
        // A record authored against a still-future effect kind must
        // decode to Unknown, not fail the room. (`audio` is now a real
        // variant as of #262, so use a tag that is still unmodelled.)
        let e: ContactEffectKind =
            serde_json::from_str(r#"{"$type":"network.symbios.contact.effect.hologram","glow":1}"#)
                .unwrap();
        assert_eq!(e, ContactEffectKind::Unknown);
    }

    #[test]
    fn audio_cue_round_trips_and_unknown_source_falls_back() {
        let mut e = ContactEffects::default();
        e.recipes.push(ContactEffectRecord {
            name: "footstep".into(),
            surface: ContactSurfaceKind::Terrain,
            phase: ContactPhaseKind::Enter,
            min_speed: Fp(0.0),
            min_intensity: Fp(0.0),
            cooldown: Fp(0.15),
            enabled: true,
            effect: ContactEffectKind::AudioCue {
                audio: AudioParams {
                    source: AudioClipSource::AtprotoBlob {
                        did: "did:plc:abc".into(),
                        cid: "bafyclip".into(),
                    },
                    ..AudioParams::default()
                },
            },
        });
        let json = serde_json::to_string(&e).unwrap();
        let back: ContactEffects = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
        assert!(json.contains("network.symbios.contact.effect.audio"));
        assert!(json.contains("network.symbios.contact.audio.atproto_blob"));

        // A future procedural source decodes to Unknown, not an error.
        let s: AudioClipSource =
            serde_json::from_str(r#"{"$type":"network.symbios.contact.audio.synth","wave":"saw"}"#)
                .unwrap();
        assert_eq!(s, AudioClipSource::Unknown);
    }

    #[test]
    fn legacy_flat_record_decodes_into_particle_burst() {
        // A pre-#261 record carried count/radius_scale/velocity_inherit
        // /particle FLAT on the record with no `effect` key. It must
        // still load, folded into a ParticleBurst.
        let canonical = &default_contact_effects().recipes[0];
        let (rs, vi, count, particle) = match &canonical.effect {
            ContactEffectKind::ParticleBurst {
                radius_scale,
                velocity_inherit,
                count,
                particle,
            } => (*radius_scale, *velocity_inherit, *count, particle.clone()),
            _ => unreachable!("canonical splash is a ParticleBurst"),
        };
        // Hand-build the legacy flat JSON shape.
        let legacy = serde_json::json!({
            "name": "legacy_splash",
            "surface": canonical.surface,
            "phase": canonical.phase,
            "min_speed": canonical.min_speed,
            "min_intensity": canonical.min_intensity,
            "count": count,
            "radius_scale": rs,
            "velocity_inherit": vi,
            "cooldown": canonical.cooldown,
            "enabled": true,
            "particle": particle,
        });
        let rec: ContactEffectRecord = serde_json::from_value(legacy).unwrap();
        assert_eq!(rec.name, "legacy_splash");
        match rec.effect {
            ContactEffectKind::ParticleBurst {
                radius_scale,
                velocity_inherit,
                count: c,
                particle: p,
            } => {
                assert_eq!(radius_scale, rs);
                assert_eq!(velocity_inherit, vi);
                assert_eq!(c, count);
                assert_eq!(p, particle);
            }
            _ => panic!("legacy flat record must fold into ParticleBurst"),
        }
    }

    #[test]
    fn decal_recipe_round_trips() {
        let mut e = ContactEffects::default();
        e.recipes.push(ContactEffectRecord {
            name: "wet_footprint".into(),
            surface: ContactSurfaceKind::Terrain,
            phase: ContactPhaseKind::Dwell,
            min_speed: Fp(0.0),
            min_intensity: Fp(0.0),
            cooldown: Fp(0.4),
            enabled: true,
            effect: ContactEffectKind::DecalStamp {
                decal: DecalParams::default(),
            },
        });
        let json = serde_json::to_string(&e).unwrap();
        let back: ContactEffects = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
        assert!(json.contains("network.symbios.contact.effect.decal"));
        // The seeded particle recipes still serialize tagged.
        assert!(json.contains("network.symbios.contact.effect.particle"));
    }
}
