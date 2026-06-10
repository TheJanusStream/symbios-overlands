//! Loading-phase plumbing: the shared record-fetch state machine
//! (instantiated for room / avatar / inventory), exponential-backoff
//! retry, the seeded ambient-audio bake, and the all-resources-present
//! gate that unblocks `AppState::InGame`.
//!
//! All three fetches run in parallel during [`AppState::Loading`]. Room
//! and avatar fetches retry transient failures with capped exponential
//! backoff (deferred through the frame loop, not by parking
//! `IoTaskPool` workers); inventory is best-effort and falls through to
//! an empty stash on any failure. The per-record policy lives in one
//! [`fetch::LoadedRecord`] impl each — see the sub-module map below.
//!
//! The frame the room record lands, [`start_ambient_bake`] dispatches
//! the gate's fifth task: rendering the room's
//! `environment.ambient_audio` recipe to WAV off the main thread (or
//! handing a `Referenced` source to
//! [`crate::world_builder::audio_resolver`]) and publishing the result
//! as [`AmbientHandle`]; [`spawn_ambient_player`] turns that handle
//! into the looping ambient player on `InGame` entry.
//!
//! [`check_loading_complete`] only transitions to `InGame` once every
//! resource the first `InGame` frame depends on is present, so a slow
//! PDS round-trip cannot strand a half-loaded recipe behind the world
//! builder — or start gameplay silent.
//!
//! ## Sub-module map
//!
//! * [`fetch`] — the generic machinery: [`fetch::LoadedRecord`] policy
//!   trait, `RecordFetchTask<R>` / `PendingRecordRetry<R>` components,
//!   the shared poll + backoff-retry systems, and the IoTaskPool /
//!   tokio dispatch wrapper.
//! * [`records`] — the three `LoadedRecord` impls (room raises the
//!   recovery banner, avatar retries quietly, inventory is best-effort)
//!   and the `OnEnter(Loading)` start systems.
//! * [`ambient`] — the ambient-audio bake task, [`AmbientHandle`],
//!   and the `InGame` ambient-player spawner.

pub mod fetch;
mod records;

mod ambient;

pub use ambient::{AmbientHandle, AmbientPlayer};
pub(crate) use ambient::{
    poll_ambient_bake_task, reset_ambient_bake_state, spawn_ambient_player, start_ambient_bake,
};
pub(crate) use fetch::{fire_pending_record_retries, poll_record_task};
pub(crate) use records::{
    start_avatar_record_fetch, start_inventory_record_fetch, start_room_record_fetch,
};

use bevy::prelude::*;

use crate::state::{
    AppState, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord,
};
use crate::terrain;

/// Transition out of `Loading` only once *every* resource the first
/// `InGame` frame relies on is present:
///
/// - [`terrain::FinishedHeightMap`] — collider is solid
/// - [`LiveRoomRecord`] — live room recipe (world builder consumes this)
/// - [`StoredRoomRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveAvatarRecord`] — live avatar driving `spawn_local_player`
/// - [`StoredAvatarRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveInventoryRecord`] / [`StoredInventoryRecord`] — owner's Generator stash
/// - [`AmbientHandle`] — ambient audio either baked or explicitly absent
///
/// Advancing early leaves the poll systems orphaned (they only run in
/// `Loading`), which would strand a slower PDS round-trip and leave the
/// owner unable to edit what was never fetched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<LiveRoomRecord>>,
    stored_room: Option<Res<StoredRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    stored_avatar: Option<Res<StoredAvatarRecord>>,
    live_inventory: Option<Res<LiveInventoryRecord>>,
    stored_inventory: Option<Res<StoredInventoryRecord>>,
    ambient: Option<Res<AmbientHandle>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some()
        && room_record.is_some()
        && stored_room.is_some()
        && live_avatar.is_some()
        && stored_avatar.is_some()
        && live_inventory.is_some()
        && stored_inventory.is_some()
        && ambient.is_some()
    {
        next_state.set(AppState::InGame);
    }
}
