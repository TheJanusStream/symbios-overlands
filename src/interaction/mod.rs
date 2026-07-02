//! Avatar-world interaction effects framework.
//!
//! A single per-frame producer ([`classifier::classify_contacts`])
//! classifies every avatar (local + peer) against the surface it
//! touches and writes the [`contact::AvatarContacts`] resource. A set
//! of independent consumer channels each read that resource the same
//! frame and turn contacts into effects. All channels are shipped:
//!
//! ## Architecture
//!
//! ```text
//!   avatars (local + peer Transforms,         producer
//!   LinearVelocity, locomotion record) ─▶  ContactClassifier  ─▶  AvatarContacts
//!                                                                     │
//!                                       ┌─────────────────────────────┘
//!                                       ▼
//!                  consumer channels (all live):
//!                  - water shader-impulse feeder   (perturbation pool → water uniforms)
//!                  - particle-burst dispatcher     (transient coloured-quad emitters)
//!                  - splat-stain stamper           (wet/dust terrain-overlay texture)
//!                  - projected-decal stamper       (fading surface-aligned quads)
//!                  - bevy_audio cue consumer       (one-shot, optionally spatial voices)
//!                  - material-keyed impact audio   (procedural footstep/landing SFX, #300)
//! ```
//!
//! The particle / decal / audio channels are **PDS-authored**: a room's
//! `network.symbios.overlands.room` record carries a `contact_effects`
//! block ([`crate::pds::ContactEffects`]) that the world compiler
//! translates into the [`recipes::ContactRecipeRegistry`]; a room that
//! omits it falls back to the default water-splash / droplet /
//! ground-dust set (two water recipes plus one terrain) with no decal
//! or authored audio. The water-wake and stains channels are always-on
//! and locomotion-scaled (see "Footprint radius" below); the
//! material-keyed impact-audio channel
//! (`audio_materials::play_terrain_impacts`, #300) is likewise
//! always-on, baking procedural footstep/landing SFX from the dominant
//! splat layer at each terrain contact.
//!
//! The producer is surface-agnostic at the call site:
//! [`contact::SurfaceContact`] is an enum (`Water` / `Terrain`) and each
//! consumer filters on the variant it wants.
//!
//! ## Footprint radius
//!
//! Every [`contact::ContactSample`] carries a `footprint_radius` derived
//! from the avatar's locomotion preset via the
//! [`locomotion::LocomotionFootprint`] trait. This is the single point of
//! truth for "how big does this avatar look on a surface" — used by
//! shaders to scale ripple radii, by particle systems to size emission
//! discs, and by stain stampers to size their texture splats. A bigger
//! avatar (hover-boat, helicopter) produces bigger effects than a
//! humanoid without any per-channel scaling.

pub mod audio;
pub mod classifier;
pub mod contact;
pub mod decal;
pub mod locomotion;
pub mod particle_channel;
pub mod perturbation;
pub mod plugin;
pub mod recipes;
pub mod stains;
pub mod water_channel;

pub use classifier::TerrainSurfaceQuery;
pub use contact::{AvatarContacts, ContactPhase, ContactSample, SurfaceContact, SurfaceKind};
pub use locomotion::{LocomotionFootprint, locomotion_footprint};
pub use particle_channel::{ParticleDispatchState, TransientEmitter};
pub use perturbation::{Perturbation, PerturbationKind, PerturbationPool};
pub use plugin::{ContactProducerSet, InteractionPlugin};
pub use recipes::{
    ContactEffectRecipe, ContactRecipeRegistry, ContactTrigger, DecalEffectRecipe, ParticleBurst,
};
pub use stains::StainsImage;
