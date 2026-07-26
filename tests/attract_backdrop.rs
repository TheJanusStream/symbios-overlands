//! Integration tests for the login-screen attract backdrop's teardown and
//! re-roll paths ([`symbios_overlands::attract`], #978).
//!
//! The seeding side is guard-heavy and target-specific (the wasm boot
//! handoff, the native loopback wait), so it is not reachable from a
//! native test harness. The teardown side is plain ECS and is where the
//! subtle invariants live: the re-roll must hand `AttractScene` straight
//! across the swap — the world pipeline gates and the login screen's
//! "New world" chip both key off it — while the exit path must drop it
//! along with any re-roll that was queued in the same frame.

use bevy::prelude::*;

use symbios_overlands::attract::{
    AttractReroll, AttractScene, end_attract_scene, reroll_attract_scene,
};
use symbios_overlands::pds::RoomRecord;
use symbios_overlands::state::LiveRoomRecord;
use symbios_overlands::world_builder::RoomEntity;

/// A bare app holding a compiled demo world: the marker, a record, and a
/// handful of spawned room entities. No plugins — the systems under test
/// only touch resources, commands and one marker query.
fn app_with_demo_world(demo_did: &str) -> App {
    let mut app = App::new();
    app.insert_resource(AttractScene);
    app.insert_resource(LiveRoomRecord(RoomRecord::default_for_did(demo_did)));
    for _ in 0..3 {
        app.world_mut().spawn(RoomEntity);
    }
    app
}

fn json(record: &RoomRecord) -> String {
    serde_json::to_string(record).expect("RoomRecord serialises")
}

fn room_entity_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<RoomEntity>>()
        .iter(app.world())
        .count()
}

#[test]
fn reroll_swaps_the_world_but_holds_the_marker() {
    let mut app = app_with_demo_world("did:test:old");
    app.add_systems(Update, reroll_attract_scene);
    app.insert_resource(AttractReroll);
    app.update();

    // The old world is gone…
    assert_eq!(
        room_entity_count(&mut app),
        0,
        "re-roll must despawn the outgoing demo world's entities"
    );
    // …replaced in the same pass by a fresh seed, so no frame ever sees
    // `Login` with the backdrop enabled and no record to compile. A
    // `RoomRecord` doesn't carry the DID it was seeded from, so the two
    // worlds are compared by content.
    let record = app
        .world()
        .get_resource::<LiveRoomRecord>()
        .expect("re-roll must seed a replacement record");
    assert_ne!(
        json(&record.0),
        json(&RoomRecord::default_for_did("did:test:old")),
        "the replacement must be a different seed, not the same world rebuilt"
    );
    // Load-bearing: `world_pipeline_active` and the login screen's
    // "New world" chip both read this. Dropping it even for one frame
    // would stall the pipeline and blink the chip out mid-rebuild.
    assert!(
        app.world().get_resource::<AttractScene>().is_some(),
        "re-roll must carry AttractScene across the swap"
    );
    assert!(
        app.world().get_resource::<AttractReroll>().is_none(),
        "the request must be consumed, not left to re-fire every frame"
    );
}

#[test]
fn reroll_without_a_demo_world_consumes_itself() {
    // The backdrop toggle going off in the same frame as a click (or any
    // other way the request outlives its world) must not latch the
    // system on: the flag comes off before the `AttractScene` check.
    let mut app = App::new();
    app.add_systems(Update, reroll_attract_scene);
    app.insert_resource(AttractReroll);
    app.update();

    assert!(
        app.world().get_resource::<AttractReroll>().is_none(),
        "a request with nothing to re-roll must still clear itself"
    );
    assert!(
        app.world().get_resource::<LiveRoomRecord>().is_none(),
        "no demo world means nothing to seed — this is not a way in"
    );
}

#[test]
fn exit_drops_the_marker_and_any_queued_reroll() {
    let mut app = app_with_demo_world("did:test:old");
    app.add_systems(Update, end_attract_scene);
    // Worst case: the user clicked "New world" on the very frame their
    // login completed. Left behind, the request would re-seed a demo
    // world behind the loading screen.
    app.insert_resource(AttractReroll);
    app.update();

    assert_eq!(room_entity_count(&mut app), 0);
    assert!(app.world().get_resource::<LiveRoomRecord>().is_none());
    assert!(app.world().get_resource::<AttractScene>().is_none());
    assert!(
        app.world().get_resource::<AttractReroll>().is_none(),
        "a re-roll queued on the login frame must not survive into Loading"
    );
}

#[test]
fn exit_is_inert_without_the_marker() {
    // `OnExit(Login)` also fires on transitions the backdrop never armed
    // (the toggle off, a wasm session resume). The sweep must not touch
    // a world it does not own.
    let mut app = App::new();
    app.insert_resource(LiveRoomRecord(RoomRecord::default_for_did("did:test:real")));
    app.world_mut().spawn(RoomEntity);
    app.add_systems(Update, end_attract_scene);
    app.update();

    assert_eq!(room_entity_count(&mut app), 1);
    assert!(app.world().get_resource::<LiveRoomRecord>().is_some());
}
