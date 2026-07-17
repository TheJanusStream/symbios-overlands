//! [`LoadedRecord`] policy impls for the three PDS record types, plus
//! the `OnEnter(AppState::Loading)` systems that kick each fetch off.
//!
//! The pipeline itself (task component, backoff, poll, retry timer)
//! lives in [`super::fetch`]; everything here is the per-record policy
//! the trait captures:
//!
//! | record    | retries | unrecoverable hook                         |
//! |-----------|---------|--------------------------------------------|
//! | Room      | 12      | raises [`RoomRecordRecovery`] banner       |
//! | Avatar    | 12      | raises [`AvatarRecordRecovery`] (#840)     |
//! | Inventory | 2       | raises [`InventoryRecordRecovery`] (#840)  |
//!
//! Every unrecoverable fallback installs the default as Live AND Stored,
//! which makes "dirty" read clean while the real record still sits on
//! the PDS — the recovery markers are what stop the next routine publish
//! from clobbering it (#840). A later clean fetch clears them.

use bevy::prelude::*;
use bevy::tasks::Task;

use crate::diagnostics::event::RecordKind;
use crate::pds::{self, AvatarRecord, FetchError, InventoryRecord, RoomRecord};
use crate::state::{
    AvatarRecordRecovery, CurrentRoomDid, InventoryRecordRecovery, LiveAvatarRecord,
    LiveInventoryRecord, LiveRoomRecord, RoomRecordRecovery, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord,
};

use super::fetch::{LoadedRecord, dispatch, spawn_record_fetch};

/// Shared retry budget for the gameplay-critical fetches. The backoff
/// saturates at 60 s after six attempts, so twelve attempts buys roughly
/// ten minutes of real-time retrying against a flaky PDS — past that,
/// persistent failure is overwhelmingly more likely than a transient
/// hiccup. Without the cap, a misbehaving endpoint would spin the
/// IoTaskPool indefinitely; on `wasm32` it would also pile up an
/// unbounded sequence of setTimeout futures in the browser event loop.
const MAX_RECORD_FETCH_ATTEMPTS: u32 = 12;

impl LoadedRecord for RoomRecord {
    const LABEL: &'static str = "Room";
    const RECORD_KIND: RecordKind = RecordKind::Room;
    const MAX_ATTEMPTS: u32 = MAX_RECORD_FETCH_ATTEMPTS;

    fn dispatch_fetch(did: String) -> Task<Result<Option<Self>, FetchError>> {
        dispatch(move |client| async move { pds::fetch_room_record(&client, &did).await })
    }

    fn default_for(did: &str) -> Self {
        // Zero-configuration homeworld: the owner has not customised
        // their overland yet (or it can't be read), so synthesise the
        // canonical default recipe keyed to their DID.
        RoomRecord::default_for_did(did)
    }

    fn install(mut self, commands: &mut Commands) {
        self.sanitize();
        info!(
            "Room record loaded: {} generators, {} placements",
            self.generators.len(),
            self.placements.len()
        );
        commands.insert_resource(StoredRoomRecord(self.clone()));
        commands.insert_resource(LiveRoomRecord(self));
    }

    /// Surface a recovery banner so the world editor can offer the owner
    /// a "Reset PDS to default" affordance — falling back to the default
    /// silently would risk a publish click clobbering the real record.
    fn on_unrecoverable(commands: &mut Commands, reason: String) {
        commands.insert_resource(RoomRecordRecovery { reason });
    }

    fn clear_recovery(commands: &mut Commands) {
        commands.remove_resource::<RoomRecordRecovery>();
    }
}

impl LoadedRecord for AvatarRecord {
    const LABEL: &'static str = "Avatar";
    const RECORD_KIND: RecordKind = RecordKind::Avatar;
    const MAX_ATTEMPTS: u32 = MAX_RECORD_FETCH_ATTEMPTS;

    fn dispatch_fetch(did: String) -> Task<Result<Option<Self>, FetchError>> {
        dispatch(move |client| async move { pds::fetch_avatar_record(&client, &did).await })
    }

    fn default_for(did: &str) -> Self {
        AvatarRecord::default_for_did(did)
    }

    fn install(mut self, commands: &mut Commands) {
        self.sanitize();
        commands.insert_resource(LiveAvatarRecord(self.clone()));
        commands.insert_resource(StoredAvatarRecord(self));
    }

    /// The default was installed as Live AND Stored, so "dirty" reads
    /// clean while the real record still sits on the PDS — without this
    /// marker the next routine avatar publish clobbers it (#840). The
    /// Avatar editor shows a banner and gates the first publish behind
    /// a confirm while the marker is present.
    fn on_unrecoverable(commands: &mut Commands, reason: String) {
        commands.insert_resource(AvatarRecordRecovery { reason });
    }

    fn clear_recovery(commands: &mut Commands) {
        commands.remove_resource::<AvatarRecordRecovery>();
    }
}

impl LoadedRecord for InventoryRecord {
    const LABEL: &'static str = "Inventory";
    const RECORD_KIND: RecordKind = RecordKind::Inventory;
    /// Two quick retries (2 s + 4 s of backoff), then fall through to
    /// the empty default. The old budget of ZERO meant one transient
    /// blip installed an empty stash presented as a green success — and
    /// the next publish wiped the real one (#840). The stash still isn't
    /// gameplay-critical, so it doesn't get the room/avatar ten-minute
    /// budget; the recovery marker below covers the fall-through.
    const MAX_ATTEMPTS: u32 = 2;

    fn dispatch_fetch(did: String) -> Task<Result<Option<Self>, FetchError>> {
        dispatch(move |client| async move { pds::fetch_inventory_record(&client, &did).await })
    }

    fn default_for(_did: &str) -> Self {
        InventoryRecord::default()
    }

    fn install(mut self, commands: &mut Commands) {
        self.sanitize();
        commands.insert_resource(LiveInventoryRecord(self.clone()));
        commands.insert_resource(StoredInventoryRecord(self));
    }

    /// The session runs "degraded": the stash shows empty while items
    /// may still exist on the PDS (#840). The Inventory window shows a
    /// banner and gates the first publish behind a confirm while the
    /// marker is present.
    fn on_unrecoverable(commands: &mut Commands, reason: String) {
        commands.insert_resource(InventoryRecordRecovery { reason });
    }

    fn clear_recovery(commands: &mut Commands) {
        commands.remove_resource::<InventoryRecordRecovery>();
    }
}

/// Kick off the async ATProto `getRecord` fetch for the room the client
/// is visiting. Runs exactly once on entry to `AppState::Loading`; the
/// result is picked up by `poll_record_task::<RoomRecord>` on subsequent
/// frames.
pub(crate) fn start_room_record_fetch(
    mut commands: Commands,
    room_did: Res<CurrentRoomDid>,
    time: Res<Time>,
) {
    spawn_record_fetch::<RoomRecord>(
        &mut commands,
        room_did.0.clone(),
        0,
        time.elapsed_secs_f64(),
    );
}

/// Kick off the async `getRecord` fetch for the local player's avatar.
/// Silently no-ops if the user never logged in (session absent), in
/// which case [`super::check_loading_complete`] will also refuse to
/// advance — we never reach Loading without a session in normal flow.
pub(crate) fn start_avatar_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    time: Res<Time>,
) {
    let Some(sess) = session else {
        warn!("start_avatar_record_fetch: no session — local avatar will not load");
        return;
    };
    spawn_record_fetch::<AvatarRecord>(&mut commands, sess.did.clone(), 0, time.elapsed_secs_f64());
}

/// Kick off the best-effort inventory fetch for the local player.
pub(crate) fn start_inventory_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    time: Res<Time>,
) {
    let Some(sess) = session else {
        return;
    };
    spawn_record_fetch::<InventoryRecord>(
        &mut commands,
        sess.did.clone(),
        0,
        time.elapsed_secs_f64(),
    );
}
