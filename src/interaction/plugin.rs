//! Bevy plugin that wires the interaction-framework resources and
//! systems.
//!
//! Producer (Phase 0): [`classify_contacts`] builds
//! [`AvatarContacts`] each frame inside [`ContactProducerSet`].
//!
//! Water-wake consumer (Phase 1, revised): a three-stage pipeline
//! turns contacts into shader displacement —
//!
//! ```text
//!   ContactProducerSet
//!     → tick_perturbations    (age the pool, cull expired, cap)
//!     → spawn_perturbations   (apply spawn rules from AvatarContacts)
//!     → feed_water_wakes      (pack live pool into water uniforms)
//! ```
//!
//! Ticking before spawning means a perturbation spawned this frame
//! renders at `age = 0` on its first visible frame.
//!
//! Everything is gated by [`crate::state::AppState::InGame`] — water
//! surfaces only exist after the world compiler runs, so the pipeline
//! would only churn empty resources during `Login` / `Loading`.

use bevy::prelude::*;

use crate::state::AppState;

use super::classifier::{ContactPersistence, PeerVelocityCache, classify_contacts};
use super::contact::AvatarContacts;
use super::particle_channel::{
    ParticleDispatchState, particle_dispatcher, retire_transient_emitters,
};
use super::perturbation::{
    PerturbationPool, PerturbationSpawnState, spawn_perturbations, tick_perturbations,
};
use super::recipes::ContactRecipeRegistry;
use super::stains::{WetCarry, setup_stains, update_stains};
use super::water_channel::feed_water_wakes;

/// System set the classifier runs in. Consumers configure their
/// systems `.after(ContactProducerSet)` so they observe the
/// freshly-built [`AvatarContacts`] within the same frame.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ContactProducerSet;

/// System set the perturbation pool's tick+spawn run in. The water
/// consumer (and any future surface consumer that reads the pool)
/// orders `.after(PerturbationSet)`.
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct PerturbationSet;

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AvatarContacts>()
            .init_resource::<PeerVelocityCache>()
            .init_resource::<ContactPersistence>()
            .init_resource::<PerturbationPool>()
            .init_resource::<PerturbationSpawnState>()
            .init_resource::<ContactRecipeRegistry>()
            .init_resource::<ParticleDispatchState>()
            .init_resource::<WetCarry>()
            // Allocate the (zeroed) stains image once at startup so the
            // terrain material can bind it as soon as it's built.
            .add_systems(Startup, setup_stains)
            .add_systems(
                Update,
                classify_contacts
                    .in_set(ContactProducerSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // Pool simulation: tick (age/cull) strictly before spawn so
            // new perturbations enter at age 0, both after the producer
            // so spawn sees this frame's contacts.
            .add_systems(
                Update,
                (tick_perturbations, spawn_perturbations)
                    .chain()
                    .in_set(PerturbationSet)
                    .after(ContactProducerSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // Pack the post-spawn pool into water material uniforms.
            .add_systems(
                Update,
                feed_water_wakes
                    .after(PerturbationSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // Particle consumer (Phase 2): dispatch contact→burst after
            // the producer so it sees this frame's contacts, and reclaim
            // finished transient emitters so they don't leak.
            //
            // Stains consumer (Phase 3): stamp/decay/upload the terrain
            // overlay, also after the producer so it sees this frame's
            // terrain contacts.
            .add_systems(
                Update,
                (
                    particle_dispatcher.after(ContactProducerSet),
                    retire_transient_emitters,
                    update_stains.after(ContactProducerSet),
                )
                    .run_if(in_state(AppState::InGame)),
            );

        // Optional Phase-4 consumer channels (#246 remainder). Each
        // registers its own resources + systems and is inert by
        // default (decal: `DecalStamper::enabled == false`; audio: no
        // `ContactAudioHook` installed), so the shipped Phase 1-3
        // behaviour is unchanged until a downstream app opts in.
        super::decal::build(app);
        super::audio::build(app);
    }
}
