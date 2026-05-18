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
use crate::pds::RoomRecord;

pub(crate) fn apply_contact_recipes(
    record: Option<Res<RoomRecord>>,
    mut registry: ResMut<ContactRecipeRegistry>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }
    // The record is sanitised before the world compiler ever sees it
    // (`RoomRecord::sanitize`), so every numeric here is already bounded.
    *registry = ContactRecipeRegistry::from_effects(&record.contact_effects);
}
