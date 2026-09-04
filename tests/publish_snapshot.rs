//! #1116: a publish pins `stored` to the record it PUBLISHED, not to
//! whatever `live` holds when the round trip lands.
//!
//! The dirty flag is derived — `records_differ(live, stored)` — so `stored`
//! is not a convenience cache. It is this client's claim about what the PDS
//! actually holds, and every edit made between dispatch and landing sits
//! inside a window where that claim can be falsified. Pinning `stored` to
//! `live` at completion time asserted the claim about a record the PDS had
//! never seen: the edit read clean, Save greyed out, the status line went
//! green, and the change was gone at the next login.
//!
//! Since #1110 the same snapshot is the baseline the avatar's attachment
//! delete set is derived from (`stored` refs − `live` refs), so a wrong
//! snapshot here produces a wrong *delete* on the next save rather than
//! merely a missed write.
//!
//! Each test drives the real poll system with a task that has already
//! resolved `Ok(())`, having set `live` to something the task never carried.
//! Against the old behaviour every one of them ends with `stored == live`.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::{Broadcast, SendTo};
use symbios_overlands::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use symbios_overlands::protocol::OverlandsMessage;
use symbios_overlands::state::{
    LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, PublishFeedback, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord, records_differ,
};

/// A minimal app with the schedulers, task pools and clock the poll systems
/// need, and nothing else — these systems are pure state transitions once
/// their task has resolved.
///
/// `UiPanels` + `Toasts` are here for all three since #1137: a failed write
/// is reported outside its editor window, so every poll system now writes to
/// the toast stack and the panel set (the Inventory one already did).
fn harness() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<symbios_overlands::diagnostics::SessionLog>();
    app.init_resource::<symbios_overlands::diagnostics::MetricsRegistry>();
    app.init_resource::<symbios_overlands::ui::toolbar::UiPanels>();
    app.init_resource::<symbios_overlands::ui::toast::Toasts>();
    // The avatar poll broadcasts `AvatarRecordsPublished` on success (#1122);
    // registering the message types is what `SymbiosMultiuserPlugin` does in
    // the real app, and it lets the tests read what went on the wire.
    app.add_message::<Broadcast<OverlandsMessage>>();
    app.add_message::<SendTo<OverlandsMessage>>();
    app
}

/// Every message the app broadcast this run.
fn broadcasts(app: &mut App) -> Vec<OverlandsMessage> {
    app.world_mut()
        .resource_mut::<Messages<Broadcast<OverlandsMessage>>>()
        .drain()
        .map(|b| b.payload)
        .collect()
}

/// A `Task` that is already finished, standing in for a completed round
/// trip to the PDS.
fn landed_ok() -> bevy::tasks::Task<Result<(), String>> {
    bevy::tasks::IoTaskPool::get().spawn(async { Ok(()) })
}

/// Run the app until the poll system has consumed its task (the entity is
/// despawned) or we give up. Bounded so a genuine hang fails the test
/// rather than wedging the suite.
fn run_until_landed(app: &mut App, mut still_pending: impl FnMut(&mut App) -> bool) {
    for _ in 0..2_000 {
        app.update();
        if !still_pending(app) {
            return;
        }
    }
    panic!("publish task never landed");
}

#[test]
fn a_room_edit_made_during_a_save_stays_dirty() {
    let mut app = harness();
    app.add_systems(Update, symbios_overlands::ui::room::poll_publish_tasks);
    app.init_resource::<PublishFeedback<RoomRecord>>();

    let published = RoomRecord::default_for_did("did:plc:snapshot-room");
    // The edit the owner makes while the save is in flight: a new trait
    // entry, which `records_differ` sees through the serde model.
    let mut edited = published.clone();
    edited
        .traits
        .insert("mid-flight".into(), vec!["edited".into()]);
    assert!(records_differ(&edited, &published), "the edit is real");

    app.insert_resource(LiveRoomRecord(edited.clone()));
    app.insert_resource(StoredRoomRecord(RoomRecord::default_for_did(
        "did:plc:stale",
    )));

    app.world_mut()
        .spawn(symbios_overlands::ui::room::PublishRoomTask {
            task: landed_ok(),
            did: "did:plc:snapshot-room".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: published.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::room::PublishRoomTask>())
    });

    let stored = app.world().resource::<StoredRoomRecord>();
    assert!(
        !records_differ(&stored.0, &published),
        "stored must mirror what was published"
    );
    assert!(
        records_differ(&stored.0, &edited),
        "the mid-flight edit must still read dirty — it was never written"
    );
}

#[test]
fn an_inventory_edit_made_during_a_save_stays_dirty() {
    let mut app = harness();
    app.add_systems(
        Update,
        symbios_overlands::ui::inventory::poll_publish_inventory_tasks,
    );
    app.init_resource::<PublishFeedback<InventoryRecord>>();

    // The sequence from the report: accept gift A (published), then accept
    // gift B inside the round trip. B must not be marked saved.
    let mut published = InventoryRecord::default();
    published.generators.insert(
        "gift_a".into(),
        symbios_overlands::pds::Generator::default_cuboid(),
    );
    let mut edited = published.clone();
    edited.generators.insert(
        "gift_b".into(),
        symbios_overlands::pds::Generator::default_cuboid(),
    );

    app.insert_resource(LiveInventoryRecord(edited.clone()));
    app.insert_resource(StoredInventoryRecord(InventoryRecord::default()));

    app.world_mut()
        .spawn(symbios_overlands::ui::inventory::PublishInventoryTask {
            task: landed_ok(),
            did: "did:plc:snapshot-inv".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: published.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::inventory::PublishInventoryTask>())
    });

    let stored = app.world().resource::<StoredInventoryRecord>();
    assert!(
        stored.0.generators.contains_key("gift_a"),
        "the published gift is recorded as saved"
    );
    assert!(
        !stored.0.generators.contains_key("gift_b"),
        "the gift accepted mid-flight was never written, so it must not be \
         recorded as saved — otherwise Save greys out and it is lost at login"
    );
    assert!(records_differ(&stored.0, &edited), "still dirty");
}

#[test]
fn an_avatar_edit_made_during_a_save_stays_dirty() {
    let mut app = harness();
    app.add_systems(
        Update,
        symbios_overlands::ui::avatar::poll_publish_avatar_tasks,
    );
    app.init_resource::<PublishFeedback<AvatarRecord>>();

    let published = AvatarRecord::default_for_did("did:plc:snapshot-avatar");
    // The edit made mid-flight: a different seeded gait, which the derived
    // dirty check sees through the serde model like any other change.
    let mut edited = published.clone();
    edited.gait = Some(symbios_overlands::pds::GaitParams::for_seed(999));
    assert!(records_differ(&edited, &published), "the edit is real");

    app.insert_resource(LiveAvatarRecord(edited.clone()));
    app.insert_resource(StoredAvatarRecord(AvatarRecord::default_for_did(
        "did:plc:stale",
    )));

    app.world_mut()
        .spawn(symbios_overlands::ui::avatar::PublishAvatarTask {
            task: landed_ok(),
            did: "did:plc:snapshot-avatar".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: published.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::avatar::PublishAvatarTask>())
    });

    let stored = app.world().resource::<StoredAvatarRecord>();
    assert!(
        !records_differ(&stored.0, &published),
        "stored must mirror what was published"
    );
    assert!(
        records_differ(&stored.0, &edited),
        "the mid-flight edit must still read dirty"
    );
    // A generator body's payload IS the record, so the live preview already
    // showed peers everything and there is nothing to re-fetch (#1122).
    assert!(
        broadcasts(&mut app).is_empty(),
        "a generator body needs no post-publish nudge"
    );
}

/// #1122. Sequence: wear a circlet (a freshly minted TID, not on the PDS),
/// sculpt the face, press Save to PDS. A rigged body's payload lives in the
/// wardrobe and attachment records this write just changed, at the SAME
/// rkeys the live preview already broadcast — so a peer holding a resolution
/// has nothing in the references to notice. Nothing here marked
/// `LiveAvatarRecord` changed, so no broadcast fired at all, and peers kept
/// the pre-save body until their wearer next edited something.
#[test]
fn saving_a_rigged_body_tells_the_room_its_records_moved() {
    let mut app = harness();
    app.add_systems(
        Update,
        symbios_overlands::ui::avatar::poll_publish_avatar_tasks,
    );
    app.init_resource::<PublishFeedback<AvatarRecord>>();

    let published = AvatarRecord::wearing("3jzfcijpj2z2a");
    app.insert_resource(LiveAvatarRecord(published.clone()));
    app.insert_resource(StoredAvatarRecord(AvatarRecord::default_for_did(
        "did:plc:stale",
    )));

    app.world_mut()
        .spawn(symbios_overlands::ui::avatar::PublishAvatarTask {
            task: landed_ok(),
            did: "did:plc:snapshot-rigged".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: published.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::avatar::PublishAvatarTask>())
    });

    let sent = broadcasts(&mut app);
    assert!(
        sent.iter()
            .any(|m| matches!(m, OverlandsMessage::AvatarRecordsPublished)),
        "peers must be told to re-resolve, or they hold the pre-save body \
         indefinitely; sent instead: {sent:?}"
    );
}

#[test]
fn the_reset_path_pins_the_same_way() {
    // The hard-reset publish lands through the same system and had the
    // same defect; the recovery banner's button is exactly when an owner is
    // most likely to keep editing while the write is out.
    let mut app = harness();
    app.add_systems(Update, symbios_overlands::ui::room::poll_publish_tasks);
    app.init_resource::<PublishFeedback<RoomRecord>>();

    let published = RoomRecord::default_for_did("did:plc:snapshot-reset");
    let mut edited = published.clone();
    edited.traits.insert("mid-flight".into(), vec!["x".into()]);

    app.insert_resource(LiveRoomRecord(edited.clone()));
    app.insert_resource(StoredRoomRecord(RoomRecord::default_for_did(
        "did:plc:stale",
    )));

    app.world_mut()
        .spawn(symbios_overlands::ui::room::ResetRoomTask {
            task: landed_ok(),
            did: "did:plc:snapshot-reset".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: published.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::room::ResetRoomTask>())
    });

    let stored = app.world().resource::<StoredRoomRecord>();
    assert!(!records_differ(&stored.0, &published));
    assert!(records_differ(&stored.0, &edited));
}

// ---------------------------------------------------------------------------
// Recovery markers retire on the landed write, not the click (#1199)
// ---------------------------------------------------------------------------

/// A `Task` that resolved to a failure: the PDS said no, or the deadline
/// passed.
fn landed_err() -> bevy::tasks::Task<Result<(), String>> {
    bevy::tasks::IoTaskPool::get().spawn(async { Err(String::from("502 Bad Gateway")) })
}

/// #1199 (finding 198). Sequence: the room fetch fell back to the default
/// with `RoomRecordRecovery` raised; the owner clicks "Reset PDS to
/// default", confirms, and the write fails. The marker used to be removed
/// on the confirm click, so the banner — the only surface carrying the
/// retry — was gone while the PDS still held the record that would not
/// load. The marker must outlive a failed write and retire only on a
/// landed one. Same contract for the ordinary publish, the avatar and the
/// inventory.
#[test]
fn a_recovery_marker_outlives_a_failed_write_and_retires_on_a_landed_one() {
    use symbios_overlands::state::{
        AvatarRecordRecovery, InventoryRecordRecovery, RoomRecordRecovery,
    };

    // Room: reset task, failed then landed.
    for (fails, retired) in [(true, false), (false, true)] {
        let mut app = harness();
        // After `harness()`: the task pool the fixture spawns on is the
        // plugin's.
        let task = if fails { landed_err() } else { landed_ok() };
        app.add_systems(Update, symbios_overlands::ui::room::poll_publish_tasks);
        app.init_resource::<PublishFeedback<RoomRecord>>();
        app.insert_resource(RoomRecordRecovery {
            reason: "decode error".into(),
        });
        let published = RoomRecord::default_for_did("did:plc:recovery-room");
        app.insert_resource(LiveRoomRecord(published.clone()));
        app.insert_resource(StoredRoomRecord(published.clone()));
        app.world_mut()
            .spawn(symbios_overlands::ui::room::ResetRoomTask {
                task,
                did: "did:plc:recovery-room".into(),
                spawned_at: 0.0,
                record_bytes: Some(1),
                published,
            });
        run_until_landed(&mut app, |app| {
            app.world()
                .iter_entities()
                .any(|e| e.contains::<symbios_overlands::ui::room::ResetRoomTask>())
        });
        assert_eq!(
            app.world().get_resource::<RoomRecordRecovery>().is_none(),
            retired,
            "room reset: marker retired={retired} expected"
        );
    }

    // Room: ordinary publish (the Ctrl+S door), failed then landed.
    for (fails, retired) in [(true, false), (false, true)] {
        let mut app = harness();
        // After `harness()`: the task pool the fixture spawns on is the
        // plugin's.
        let task = if fails { landed_err() } else { landed_ok() };
        app.add_systems(Update, symbios_overlands::ui::room::poll_publish_tasks);
        app.init_resource::<PublishFeedback<RoomRecord>>();
        app.insert_resource(RoomRecordRecovery {
            reason: "decode error".into(),
        });
        let published = RoomRecord::default_for_did("did:plc:recovery-room");
        app.insert_resource(StoredRoomRecord(published.clone()));
        app.world_mut()
            .spawn(symbios_overlands::ui::room::PublishRoomTask {
                task,
                did: "did:plc:recovery-room".into(),
                spawned_at: 0.0,
                record_bytes: Some(1),
                published,
            });
        run_until_landed(&mut app, |app| {
            app.world()
                .iter_entities()
                .any(|e| e.contains::<symbios_overlands::ui::room::PublishRoomTask>())
        });
        assert_eq!(
            app.world().get_resource::<RoomRecordRecovery>().is_none(),
            retired,
            "room publish: marker retired={retired} expected"
        );
    }

    // Avatar.
    for (fails, retired) in [(true, false), (false, true)] {
        let mut app = harness();
        // After `harness()`: the task pool the fixture spawns on is the
        // plugin's.
        let task = if fails { landed_err() } else { landed_ok() };
        app.add_systems(
            Update,
            symbios_overlands::ui::avatar::poll_publish_avatar_tasks,
        );
        app.init_resource::<PublishFeedback<AvatarRecord>>();
        app.insert_resource(AvatarRecordRecovery {
            reason: "timed out".into(),
        });
        let published = AvatarRecord::default_for_did("did:plc:recovery-avatar");
        app.insert_resource(StoredAvatarRecord(published.clone()));
        app.world_mut()
            .spawn(symbios_overlands::ui::avatar::PublishAvatarTask {
                task,
                did: "did:plc:recovery-avatar".into(),
                spawned_at: 0.0,
                record_bytes: Some(1),
                published,
            });
        run_until_landed(&mut app, |app| {
            app.world()
                .iter_entities()
                .any(|e| e.contains::<symbios_overlands::ui::avatar::PublishAvatarTask>())
        });
        assert_eq!(
            app.world().get_resource::<AvatarRecordRecovery>().is_none(),
            retired,
            "avatar: marker retired={retired} expected"
        );
    }

    // Inventory.
    for (fails, retired) in [(true, false), (false, true)] {
        let mut app = harness();
        // After `harness()`: the task pool the fixture spawns on is the
        // plugin's.
        let task = if fails { landed_err() } else { landed_ok() };
        app.add_systems(
            Update,
            symbios_overlands::ui::inventory::poll_publish_inventory_tasks,
        );
        app.init_resource::<PublishFeedback<InventoryRecord>>();
        app.insert_resource(InventoryRecordRecovery {
            reason: "timed out".into(),
        });
        let published = InventoryRecord::default();
        app.insert_resource(StoredInventoryRecord(published.clone()));
        app.world_mut()
            .spawn(symbios_overlands::ui::inventory::PublishInventoryTask {
                task,
                did: "did:plc:recovery-inventory".into(),
                spawned_at: 0.0,
                record_bytes: Some(1),
                published,
            });
        run_until_landed(&mut app, |app| {
            app.world()
                .iter_entities()
                .any(|e| e.contains::<symbios_overlands::ui::inventory::PublishInventoryTask>())
        });
        assert_eq!(
            app.world()
                .get_resource::<InventoryRecordRecovery>()
                .is_none(),
            retired,
            "inventory: marker retired={retired} expected"
        );
    }
}

/// #1199 (finding 197). Sequence: click "Reset PDS to default" on the
/// recovery banner, confirm, then walk into a portal while the 30 s write
/// is still out. The guard's in-flight probe queried the three publish
/// tasks and not the reset, so it read "nothing in flight", travel swapped
/// `StoredRoomRecord` for the destination's, and the landing reset then
/// pinned the LOCAL default over it. A running reset must count as a write
/// the guard waits for.
#[test]
fn a_running_reset_counts_as_a_publish_in_flight() {
    #[derive(Resource, Default)]
    struct Probe(bool);
    fn probe(
        tasks: symbios_overlands::ui::unsaved_guard::GuardPublishTasks,
        mut out: ResMut<Probe>,
    ) {
        out.0 = tasks.any_in_flight();
    }

    let mut app = harness();
    app.init_resource::<Probe>();
    app.add_systems(Update, probe);
    app.update();
    assert!(
        !app.world().resource::<Probe>().0,
        "nothing spawned, nothing in flight"
    );

    let pending: bevy::tasks::Task<Result<(), String>> =
        bevy::tasks::IoTaskPool::get().spawn(std::future::pending());
    app.world_mut()
        .spawn(symbios_overlands::ui::room::ResetRoomTask {
            task: pending,
            did: "did:plc:reset-in-flight".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: RoomRecord::default_for_did("did:plc:reset-in-flight"),
        });
    app.update();
    assert!(
        app.world().resource::<Probe>().0,
        "a running reset is a write the guard must wait for"
    );
}

// ---------------------------------------------------------------------------
// A landed write answers only the session and room it was fired in (#1204)
// ---------------------------------------------------------------------------

/// #1204 (finding 194). Sequence: press Save, pick "Continue in background"
/// on the unsaved guard, walk through a portal; the save lands in the
/// destination. `stored` used to be pinned to the record that was
/// published — the room just LEFT — over the destination owner's record,
/// so the editor read dirty against a foreign baseline and "Revert to
/// saved" would have installed the previous world. The same shape via
/// logout on wasm, where a dropped task's fetch keeps running. A result
/// whose DID is not the current room's is dropped whole.
#[test]
fn a_save_that_lands_after_the_room_changed_does_not_pin_stored() {
    use symbios_overlands::state::{CurrentRoomDid, PublishStatus};

    let mut app = harness();
    app.add_systems(Update, symbios_overlands::ui::room::poll_publish_tasks);
    app.init_resource::<PublishFeedback<RoomRecord>>();

    let previous_room = RoomRecord::default_for_did("did:plc:previous-room");
    let destination = RoomRecord::default_for_did("did:plc:destination");
    app.insert_resource(CurrentRoomDid("did:plc:destination".into()));
    app.insert_resource(StoredRoomRecord(destination.clone()));

    app.world_mut()
        .spawn(symbios_overlands::ui::room::PublishRoomTask {
            task: landed_ok(),
            did: "did:plc:previous-room".into(),
            spawned_at: 0.0,
            record_bytes: Some(1),
            published: previous_room.clone(),
        });

    run_until_landed(&mut app, |app| {
        app.world()
            .iter_entities()
            .any(|e| e.contains::<symbios_overlands::ui::room::PublishRoomTask>())
    });

    let stored = app.world().resource::<StoredRoomRecord>();
    assert!(
        !records_differ(&stored.0, &destination),
        "the destination's stored mirror must survive a save for the room we left"
    );
    assert!(
        matches!(
            app.world().resource::<PublishFeedback<RoomRecord>>().status,
            PublishStatus::Idle
        ),
        "a stale result must not paint this room's status line"
    );
}
