//! Avatar-world interaction effects framework — Phase 0 primitives.
//!
//! Phase 0 ships the data layer and the per-frame producer that builds it.
//! Every later phase (water wakes, particle splashes, splat-stain footprints,
//! authored effect recipes) reads from the [`contact::AvatarContacts`]
//! resource that [`classifier::classify_contacts`] writes each frame.
//!
//! ## Architecture
//!
//! ```text
//!   avatars (local + peer Transforms,         producer
//!   LinearVelocity, locomotion record) ─▶  ContactClassifier  ─▶  AvatarContacts
//!                                                                     │
//!                                       ┌─────────────────────────────┘
//!                                       ▼
//!                  consumer channels (added by later phases):
//!                  - water shader-impulse feeder (Phase 1)
//!                  - particle dispatcher          (Phase 2)
//!                  - splat-stain stamper          (Phase 3)
//!                  - decal stamper / audio hook   (Phase 4)
//! ```
//!
//! The producer is intentionally surface-agnostic at the call site:
//! [`contact::SurfaceContact`] is an enum, and Phase 3 will fill in its
//! `Terrain` variant without touching consumers that only care about
//! water. Consumers filter on the variant they want.
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
pub mod audio_example;
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

pub use audio::{ContactAudioDispatch, ContactAudioHook};
pub use audio_example::FootstepAudioHook;
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
