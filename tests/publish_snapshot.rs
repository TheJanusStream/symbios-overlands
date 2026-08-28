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
use symbios_overlands::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use symbios_overlands::state::{
    LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, PublishFeedback, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord, records_differ,
};

/// A minimal app with the schedulers, task pools and clock the poll systems
/// need, and nothing else — these systems are pure state transitions once
/// their task has resolved.
fn harness() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.init_resource::<symbios_overlands::diagnostics::SessionLog>();
    app.init_resource::<symbios_overlands::diagnostics::MetricsRegistry>();
    app
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
    app.init_resource::<symbios_overlands::ui::toolbar::UiPanels>();
    app.init_resource::<symbios_overlands::ui::toast::Toasts>();

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
