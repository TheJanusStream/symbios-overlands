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
//! as [`AmbientHandle`]; [`ambient::swap_ambient_player_to_handle`] turns
//! that handle into the looping ambient player in-game, after a short
//! settle window so the sink isn't born during a stall.
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
//!   the settle-gated in-game ambient-player spawn/swap, and the
//!   [`AmbientSettle`] de-chop window.

pub mod fetch;
mod records;

mod ambient;

pub(crate) use ambient::{
    AmbientBakeTask, AmbientRebakePending, AmbientRebakeTask, AmbientSettle, LiveAmbientConfig,
    PlayingAmbient, arm_ambient_settle, poll_ambient_task, rebake_ambient_on_record_change,
    reset_ambient_bake_state, start_ambient_bake, swap_ambient_player_to_handle,
    tick_ambient_settle,
};
pub use ambient::{AmbientHandle, AmbientPlayer};
pub(crate) use fetch::{fire_pending_record_retries, poll_record_task};
pub(crate) use records::{
    start_avatar_record_fetch, start_inventory_record_fetch, start_room_record_fetch,
};

use bevy::prelude::*;

/// `OnEnter(Loading)`: forget the previous pass's terminal fetch
/// outcomes (#840) so a portal hop's loading screen doesn't inherit the
/// last login's fallback markers.
pub(crate) fn reset_fetch_outcomes(mut commands: Commands) {
    commands.insert_resource(fetch::RecordFetchOutcomes::default());
}

/// `OnEnter(InGame)`: one toast naming every record whose fetch fell
/// back to a default for a FAILURE reason (#840) — the loading screen's
/// amber rows scroll away with the state change, and without this the
/// only trace of "your avatar is not your avatar" was the session log.
pub(crate) fn toast_fetch_fallbacks(
    outcomes: Res<fetch::RecordFetchOutcomes>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
) {
    let fallen = outcomes.failure_fallback_labels();
    if fallen.is_empty() {
        return;
    }
    let plural = fallen.len() > 1;
    toasts.warn(
        format!(
            "Using default{} for {} — the stored cop{} could not be loaded. \
             Saving would overwrite what's stored.",
            if plural { "s" } else { "" },
            fallen.join(", "),
            if plural { "ies" } else { "y" },
        ),
        time.elapsed_secs_f64(),
    );
}

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
/// - [`WorldCompiled`](crate::world_builder::WorldCompiled) — the room
///   compile has produced the world's entities. The compile runs during
///   `Loading` (see the registration note in
///   [`crate::world_builder::WorldBuilderPlugin`]) precisely so its
///   multi-second wasm main-thread stall lands behind the loading
///   screen instead of right after an "everything done" checklist.
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
    world_compiled: Option<Res<crate::world_builder::WorldCompiled>>,
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
        && world_compiled.is_some()
    {
        next_state.set(AppState::InGame);
    }
}
