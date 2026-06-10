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
//!         · room::palette    (OkLCH-coordinated colours)
//!         · room::terrain    (heightmap shape + erosion)
//!         · room::textures   (per-biome procedural generator knobs)
//!         · room::atmosphere (water, clouds, sun, fog)
//!         · room::scatters   (biome-biased tree-scatter specs)
//!         · room::rocks      (landform-biased boulder scatters)
//!         · room::particles  (biome-mood ambient emitter)
//!         · room::landmark   (biome-matched structure near spawn)
//!         · room::audio      (biome-matched ambient bed)
//!         · avatar::chassis  (visual family: boat/airship/humanoid/skiff)
//!         · avatar::body     (proportions)
//!         · avatar::vessel   (boat hull-form/mast/stack design)
//!         · avatar::airship  (envelope/gondola/fin design)
//!         · avatar::skiff    (running gear/canopy design)
//!         · avatar::humanoid_style (hat/backpack/eye-glow costume)
//!         · avatar::palette  (skin/hair/accent)
//!         · avatar::gait     (cadence/bounce/sway)
//! ```
//!
//! Resolution rule: record-authored values always win. The derivers fill
//! in fields that aren't explicitly set on the PDS record, so a brand-new
//! user (no record yet) sees a fully-seeded room/avatar while authored
//! rooms keep their stored overrides.

pub mod avatar;
pub mod hash;
pub mod oklch;
pub mod room;
pub mod scene;

pub use avatar::{
    AirshipDesign, AvatarBody, AvatarGait, AvatarPalette, BodyArchetype, BowStyle, CanopyStyle,
    ChassisFamily, EnvelopeForm, HatStyle, HullForm, HumanoidStyle, SkiffDesign, SkiffForm,
    VesselArchetype, VesselDesign,
};
pub use hash::fnv1a_64;
pub use room::{
    AmbientParticles, AmbientRecipe, Atmosphere, BiomeTextures, GeneratorKind, GroundTextureParams,
    Landmark, ParticleMood, RockScatters, RockTextureParams, RoomPalette, SplatRule, TerrainShape,
    TreeScatter, TreeScatters, TreeSpecies, WaterDynamics,
};
pub use scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, pick, range_f32, signed_unit_f32, unit_f32,
};
