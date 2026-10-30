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
//! the PDS - the recovery markers are what stop the next routine publish
//! from clobbering it (#840). A later clean fetch clears them.

use bevy::prelude::*;
use bevy::tasks::Task;

use crate::diagnostics::event::RecordKind;
use crate::pds::{self, AvatarRecord, FetchError, InventoryRecord, RoomRecord};
use crate::state::{
    AvatarRecordRecovery, CurrentRoomDid, InventoryRecordRecovery, LiveAvatarRecord,
    LiveInventoryRecord, LiveRoomRecord, RecoveryCause, RoomRecordRecovery, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord,
};

use super::fetch::{LoadedRecord, dispatch, spawn_record_fetch};

/// Shared retry budget for the gameplay-critical fetches. The backoff
/// saturates at 60 s after six attempts, so twelve attempts buys roughly
/// ten minutes of real-time retrying against a flaky PDS - past that,
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

    /// Surface a recovery banner - falling back to the default silently
    /// would risk a publish click clobbering the real record.
    ///
    /// The cause rides along (#1265 f210): the deliberate overwrite the
    /// banner offers is the remedy for a record this build cannot decode
    /// and a hard delete of a healthy one when the server was merely
    /// unreachable, so only the room banner branches on it.
    fn on_unrecoverable(commands: &mut Commands, cause: RecoveryCause, reason: String) {
        commands.insert_resource(RoomRecordRecovery { cause, reason });
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

    /// The owner's own record, arrived with its body resolved, stood on the
    /// shape its body takes (`restore_body_shape`): a beast that a client
    /// older than its shape saved upright - such a client drops the key it
    /// cannot read on write-back - back on its lying capsule (#1802), or it
    /// would hang from a person's capsule as tall as its trunk is long; and
    /// a person whose wardrobe entry another app re-saved under a beast's
    /// lying collider back on their feet (#1803). Refitted in BOTH copies,
    /// so nothing reads as unsaved: the owner did nothing, and Save offering
    /// itself on arrival would say they had. The published record keeps the
    /// preset it was saved with - peers draw that, as published - until the
    /// owner's next save of anything, which carries the refit with it.
    fn install(mut self, commands: &mut Commands) {
        self.sanitize();
        if pds::avatar::locomotion::restore_body_shape(&mut self) {
            info!(
                "Avatar record: the on-foot preset was saved for another kind of body - \
                 refitted to its body"
            );
        }
        commands.insert_resource(LiveAvatarRecord(self.clone()));
        commands.insert_resource(StoredAvatarRecord(self));
    }

    /// The default was installed as Live AND Stored, so "dirty" reads
    /// clean while the real record still sits on the PDS - without this
    /// marker the next routine avatar publish clobbers it (#840). The
    /// Avatar editor shows a banner and gates the first publish behind
    /// a confirm while the marker is present.
    /// `cause` is unused here on purpose: this banner already words itself
    /// cause-neutrally ("could not be loaded") and offers nothing
    /// destructive, so there is nothing for it to branch. It stays on the
    /// hook so a banner that *does* branch never has to re-derive it.
    fn on_unrecoverable(commands: &mut Commands, _cause: RecoveryCause, reason: String) {
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
    /// blip installed an empty stash presented as a green success - and
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
    /// Cause-neutral for the same reason as the avatar's.
    fn on_unrecoverable(commands: &mut Commands, _cause: RecoveryCause, reason: String) {
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
/// advance - we never reach Loading without a session in normal flow.
pub(crate) fn start_avatar_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    time: Res<Time>,
) {
    let Some(sess) = session else {
        warn!("start_avatar_record_fetch: no session - local avatar will not load");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::LocomotionPreset;
    use crate::pds::avatar::ResolvedRig;
    use crate::pds::{HumanoidParams, LocomotionConfig};
    use symbios_avatar::plan::Family;

    /// The owner's horse as an older client wrote it back: its fitted
    /// preset with the shape it could not read dropped, decoded here as a
    /// peer decodes the published record.
    fn stripped_horse() -> (AvatarRecord, HumanoidParams) {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(ResolvedRig {
                body: crate::pds::avatar::locomotion::default_beast(Family::Horse),
                attachments: Vec::new(),
            });
        }
        let LocomotionConfig::Humanoid(fitted) =
            crate::pds::avatar::locomotion::beast_record(Family::Horse).locomotion
        else {
            unreachable!("a beast on foot");
        };
        let mut wire = serde_json::to_value(&*fitted).expect("writes");
        wire.as_object_mut().expect("an object").remove("shape");
        let mut old: HumanoidParams = serde_json::from_value(wire).expect("reads");
        old.sanitize();
        assert!(!old.is_lying(), "the premise: the shape is gone");
        record.locomotion = old.into_config();
        (record, *fitted)
    }

    /// **A beast saved without its shape stands on it again on its owner's
    /// client, and as published for a peer** (#1802): installed as the
    /// owner's own record, it lies on the capsule its body fits - in the
    /// live copy and the stored one alike, so nothing reads as unsaved - and
    /// a person's record installs exactly as it arrived. The published
    /// record, which is what a peer reads and sanitises, keeps the upright
    /// preset it was saved with: the defence is the owner's alone.
    #[test]
    fn a_beast_saved_without_its_shape_stands_on_it_again_for_its_owner() {
        let (published, fitted) = stripped_horse();
        let mut world = World::new();
        published.clone().install(&mut world.commands());
        world.flush();
        let live = &world.resource::<LiveAvatarRecord>().0;
        let LocomotionConfig::Humanoid(p) = &live.locomotion else {
            panic!("the owner's horse went off foot");
        };
        assert_eq!(**p, fitted, "the owner's horse is not its fit");
        assert!(
            !crate::pds::avatar::avatar_is_dirty(live, &world.resource::<StoredAvatarRecord>().0),
            "the refit reads as unsaved work"
        );
        // As a peer reads the record it was published as.
        let mut peer: AvatarRecord =
            serde_json::from_value(serde_json::to_value(&published).expect("writes"))
                .expect("reads");
        peer.sanitize();
        let LocomotionConfig::Humanoid(seen) = &peer.locomotion else {
            panic!("a peer's horse went off foot");
        };
        assert!(!seen.is_lying(), "a peer's copy was refitted");
        // A person installs as it arrived.
        let mut person = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = person.body.rigged_mut() {
            rig.resolved = Some(ResolvedRig {
                body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:install"),
                attachments: Vec::new(),
            });
        }
        let mut world = World::new();
        person.clone().install(&mut world.commands());
        world.flush();
        let mut want = person;
        want.sanitize();
        assert_eq!(world.resource::<LiveAvatarRecord>().0, want);
    }

    /// **A person saved under a beast's lying collider stands up on their
    /// owner's client, and lies as published for a peer** (#1803): the
    /// mirror of the beast's defence above. A wardrobe entry re-saved as a
    /// person by another app, under a record still carrying the horse's
    /// fit, installs on the stature rule's upright capsule with the default
    /// person's mass, jump and run - and a tuning no body sizes, here the
    /// acceleration, as it was - in the live copy and the stored one alike,
    /// so nothing reads as unsaved. A body this build cannot read keeps the
    /// lying preset a newer client wrote for it.
    #[test]
    fn a_person_saved_under_a_lying_preset_stands_up_for_its_owner() {
        use crate::pds::types::Fp;
        let body = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:install-person");
        let LocomotionConfig::Humanoid(horse) =
            crate::pds::avatar::locomotion::beast_record(Family::Horse).locomotion
        else {
            unreachable!("a beast on foot");
        };
        let lying = HumanoidParams {
            acceleration: Fp(17.5),
            ..(*horse).clone()
        };
        assert!(lying.is_lying(), "the premise: the horse's collider lies");
        let wearing = |body: crate::pds::avatar::EngineAvatarRecord| {
            let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
            if let Some(rig) = record.body.rigged_mut() {
                rig.resolved = Some(ResolvedRig {
                    body,
                    attachments: Vec::new(),
                });
            }
            record.locomotion = lying.clone().into_config();
            record
        };
        let published = wearing(body.clone());
        let mut world = World::new();
        published.clone().install(&mut world.commands());
        world.flush();
        let live = &world.resource::<LiveAvatarRecord>().0;
        let LocomotionConfig::Humanoid(p) = &live.locomotion else {
            panic!("the owner's person went off foot");
        };
        let person = HumanoidParams::default();
        let want = crate::pds::avatar::fit_on_foot(
            &body,
            &HumanoidParams {
                mass: person.mass,
                jump_impulse: person.jump_impulse,
                walk_speed: person.walk_speed,
                ..lying.clone()
            },
        );
        assert!(!want.is_lying(), "the premise: the stature rule stands");
        assert_eq!(
            **p, want,
            "the owner's person still lies on the horse's collider"
        );
        assert_eq!(p.acceleration, Fp(17.5), "a tuning no body sizes moved");
        assert!(
            !crate::pds::avatar::avatar_is_dirty(live, &world.resource::<StoredAvatarRecord>().0),
            "the refit reads as unsaved work"
        );
        // As a peer reads the record it was published as.
        let mut peer: AvatarRecord =
            serde_json::from_value(serde_json::to_value(&published).expect("writes"))
                .expect("reads");
        peer.sanitize();
        let LocomotionConfig::Humanoid(seen) = &peer.locomotion else {
            panic!("a peer's person went off foot");
        };
        assert!(seen.is_lying(), "a peer's copy was refitted");
        // A body this build cannot read installs as it arrived.
        let mut unknown = body;
        unknown.archetype = symbios_avatar::Archetype::Unknown {
            type_name: String::from("network.symbios.avatar.archetype#bird"),
            fields: serde_json::Map::new(),
        };
        let mut world = World::new();
        wearing(unknown.clone()).install(&mut world.commands());
        world.flush();
        let mut want = wearing(unknown);
        want.sanitize();
        assert_eq!(
            world.resource::<LiveAvatarRecord>().0.locomotion,
            want.locomotion,
            "an unknown body's preset was refitted"
        );
    }
}
