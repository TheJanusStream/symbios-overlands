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

        // PDS-authored consumer channels (#261 decal / #262 audio).
        // Each registers its own resources + systems but stays inert
        // (early-returns, zero cost) until a room authors the matching
        // `ContactEffectKind` recipe — `registry.decals` /
        // `registry.audio` are empty by default — so a room that omits
        // `contact_effects` keeps the particle-only water-wake / stains
        // behaviour unchanged.
        super::decal::build(app);
        super::audio::build(app);

        // Procedural impact channel (#300): generate material-keyed
        // footstep / landing SFX from the dominant splat layer at the
        // contact point. Always-on baseline; ordered after the
        // producer so it sees this frame's Enter samples.
        app.init_resource::<crate::audio_materials::ImpactCooldowns>()
            .add_systems(
                Update,
                crate::audio_materials::play_terrain_impacts
                    .after(ContactProducerSet)
                    .run_if(in_state(AppState::InGame)),
            );

        // Audio editor monitor (#314): the room-admin audio editor's
        // "Audition" button bakes the patch/sequence off-thread and
        // loops it through Bevy's audio. Registering the crate's plugin
        // adds the `MonitorRequest` message + `AudioMonitor` resource
        // and the bake/poll systems; the editor UI writes requests and
        // reads `AudioMonitor::last_samples` for its waveform.
        app.add_plugins(bevy_symbios_audio::ui::AudioEditorPlugin);
    }
}
