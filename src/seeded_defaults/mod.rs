//! DID-seeded defaults for rooms and avatars.
//!
//! Centralised home for the deterministic per-user variation pipeline.
//! Every consumer (terrain config defaults, room environment palette,
//! avatar body / palette / gait) reads from this module so the FNV-1a
//! hash and ChaCha8 RNG seeding live in exactly one place — peers
//! visiting the same DID derive bit-exact identical defaults.
//!
//! The data flow per room:
//!
//! ```text
//!   DID string
//!     → fnv1a_64         (one u64 seed per DID)
//!     → SceneCharacter   (archetype + hue + temperature anchor)
//!     → per-domain derivers
//!         · room::accent     (theme identity accent: tint / haze /
//!                             particle-mood overrides)
//!         · room::palette    (OkLCH-coordinated colours)
//!         · room::terrain    (heightmap shape + erosion)
//!         · room::textures   (per-biome procedural generator knobs)
//!         · room::atmosphere (water, clouds, sun, fog)
//!         · room::scatters   (biome-biased tree-scatter specs)
//!         · room::rocks      (landform-biased boulder scatters)
//!         · room::particles  (biome-mood ambient emitter)
//!         · room::settlement (themed catalogue cluster near spawn)
//!         · room::audio      (biome-matched ambient bed)
//!         · avatar::character (anchor: chassis + style + ornateness/wear)
//!         · avatar::palette  (skin/hair + style/temperature/wear accents)
//!         · avatar::materials (MaterialKit: per-surface style/wear finish)
//!         · avatar::fx       (style-gated particle aura + audio voice)
//!         · avatar::outfit   (slot → part choice from the part catalogue)
//!         · avatar::body     (proportions)
//!         · avatar::blueprint (humanoid canon: joint anchors + capsule dims)
//!         · avatar::face     (head shape, features, resting expression)
//!         · avatar::gait     (cadence/bounce/sway)
//! ```
//!
//! Avatar silhouettes are composed from the tagged part catalogue
//! ([`crate::pds::avatar::parts`]) rather than per-family design derivers.
//!
//! Resolution rule: record-authored values always win. The derivers fill
//! in fields that aren't explicitly set on the PDS record, so a brand-new
//! user (no record yet) sees a fully-seeded room/avatar while authored
//! rooms keep their stored overrides.

pub mod avatar;
pub mod band;
pub mod hash;
pub mod oklch;
pub mod room;
pub mod scene;

pub use avatar::{
    AvatarBody, AvatarCharacter, AvatarFx, AvatarGait, AvatarOutfit, AvatarPalette, AvatarVoice,
    BodyArchetype, ChassisFamily, Disposition, FaceParams, FaceShape, FacialHair, FinishRegister,
    HairStyle, HumanoidBlueprint, MaterialKit, NoseKind, OrnatenessBand, OrnatenessTier,
    OutfitPart, ParticleAura, StylizationTier, WearBand, WearTier,
};
pub use hash::fnv1a_64;
pub use room::{
    AmbientParticles, AmbientRecipe, Atmosphere, BiomeTextures, GatewaySpot, GeneratorKind,
    GroundTextureParams, ParticleMood, RockScatters, RockTextureParams, RoomPalette, Settlement,
    SettlementMember, SplatRule, TerrainShape, ThemeAccent, TreeScatter, TreeScatters, TreeSpecies,
    WaterDynamics, theme_luminosity,
};
pub use scene::{
    BiomeArchetype, EscalationBand, EscalationTier, LandformArchetype, ProsperityBand,
    ProsperityTier, SceneCharacter, ThemeArchetype, pick, range_f32, signed_unit_f32, unit_f32,
};
