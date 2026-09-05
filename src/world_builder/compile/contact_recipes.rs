//! Compiles the room record's authored [`crate::pds::ContactEffects`]
//! into the runtime `ContactRecipeRegistry` (#246).
//!
//! Runs as its own system (mirroring `apply_environment_state`) rather
//! than inside `compile_room_record` — that system is already at Bevy's
//! 16-param `IntoSystem` limit, and rebuilding the registry has nothing
//! to do with despawning/spawning entities anyway. Reacts to
//! `RoomRecord` changes so an editor save (or a peer broadcast) takes
//! effect immediately without a code change or relog.

use bevy::prelude::*;

use crate::interaction::ContactRecipeRegistry;
use crate::state::LiveRoomRecord;

pub(crate) fn apply_contact_recipes(
    record: Option<Res<LiveRoomRecord>>,
    mut registry: ResMut<ContactRecipeRegistry>,
    // The three throttle tables are keyed by a recipe's INDEX in the list
    // this rebuild is about to replace (#1254 f322).
    mut particles: ResMut<crate::interaction::particle_channel::ParticleDispatchState>,
    mut decals: ResMut<crate::interaction::decal::DecalStampState>,
    mut audio: ResMut<crate::interaction::audio::AudioCueState>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }
    let record = &record.0;
    // The record is sanitised before the world compiler ever sees it
    // (`RoomRecord::sanitize`), so every numeric here is already bounded.
    *registry = ContactRecipeRegistry::from_effects(&record.contact_effects);
    // Every live throttle refers to positions in the list just replaced.
    // Deleting one recipe shifts every later index down and would hand its
    // cooldown to whichever recipe inherited the slot — silently, and in
    // the middle of exactly the workflow (delete, then test the survivors)
    // where the owner is least able to attribute it.
    particles.clear_cooldowns();
    decals.clear_cooldowns();
    audio.clear_cooldowns();
}
