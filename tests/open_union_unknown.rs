//! Forward-compatibility tests: every tagged-union record field carries a
//! `#[serde(other)] Unknown` arm so a client visiting a record authored by
//! a newer engine silently skips the unrecognised variants instead of
//! failing the whole decode.
//!
//! These tests synthesise payloads carrying `$type` values that don't exist
//! in the current lexicon and assert the decoder preserves them as
//! `Unknown`.
//!
//! Decoding is only half the contract. Every `Unknown` arm is also
//! `skip_serializing` (#1111), so a record holding one cannot be written
//! back — the save and the peer broadcast are refused with a sentence
//! instead of replacing the newer client's content with `{"$type":
//! "Unknown"}`. The tests at the end of this file pin that half.

use symbios_overlands::pds::{GeneratorKind, Placement, RoomRecord};

#[test]
fn unknown_generator_type_decodes_to_unknown() {
    let json = r#"{
        "$type": "network.symbios.overlands.room",
        "environment": {
            "sun_color": [9800, 9500, 8200]
        },
        "generators": {
            "future_forest": { "$type": "network.symbios.gen.trees2026", "density": 42 }
        },
        "placements": [],
        "traits": {}
    }"#;
    let room: RoomRecord =
        serde_json::from_str(json).expect("unknown generator must not crash decode");
    let g = room
        .generators
        .get("future_forest")
        .expect("entry preserved");
    assert!(matches!(g.kind, GeneratorKind::Unknown));
}

#[test]
fn unknown_placement_type_decodes_to_unknown() {
    let json = r#"{
        "$type": "network.symbios.overlands.room",
        "environment": {},
        "generators": {},
        "placements": [
            { "$type": "network.symbios.place.hexgrid",
              "generator_ref": "base_terrain" }
        ],
        "traits": {}
    }"#;
    let room: RoomRecord =
        serde_json::from_str(json).expect("unknown placement must not crash decode");
    assert_eq!(room.placements.len(), 1);
    assert!(matches!(room.placements[0], Placement::Unknown));
}

#[test]
fn mixed_known_and_unknown_variants_coexist() {
    // A realistic "forward-compat" record: some known generators, some
    // unknown, and both kinds of placements. Loading such a record must
    // keep the known entries intact so a client can still render what it
    // understands.
    let json = r#"{
        "$type": "network.symbios.overlands.room",
        "environment": {},
        "generators": {
            "base_water": { "$type": "network.symbios.gen.water", "level_offset": 0 },
            "future_arch": { "$type": "network.symbios.gen.archways2027" }
        },
        "placements": [
            { "$type": "network.symbios.place.absolute",
              "generator_ref": "base_water",
              "transform": {
                  "translation": [0, 0, 0],
                  "rotation": [0, 0, 0, 10000],
                  "scale": [10000, 10000, 10000]
              },
              "snap_to_terrain": false
            },
            { "$type": "network.symbios.place.spiral2027" }
        ],
        "traits": {}
    }"#;
    let room: RoomRecord = serde_json::from_str(json).expect("mixed payload must decode");
    assert!(matches!(
        room.generators.get("base_water").map(|g| &g.kind),
        Some(GeneratorKind::Water { .. })
    ));
    assert!(matches!(
        room.generators.get("future_arch").map(|g| &g.kind),
        Some(GeneratorKind::Unknown)
    ));
    assert_eq!(room.placements.len(), 2);
    assert!(matches!(room.placements[0], Placement::Absolute { .. }));
    assert!(matches!(room.placements[1], Placement::Unknown));
}

#[test]
fn unknown_variants_survive_sanitize_without_panic() {
    // Sanitize walks every placement + generator. Unknown variants must
    // be a no-op for it, not a panic — a malicious peer could otherwise
    // gate the whole client behind a forward-compat decode branch.
    let json = r#"{
        "$type": "network.symbios.overlands.room",
        "environment": {},
        "generators": {
            "future_arch": { "$type": "network.symbios.gen.archways2027" }
        },
        "placements": [
            { "$type": "network.symbios.place.spiral2027" }
        ],
        "traits": {}
    }"#;
    let mut room: RoomRecord = serde_json::from_str(json).expect("must decode");
    room.sanitize();
    // Still present after the clamp pass.
    assert!(room.generators.contains_key("future_arch"));
    assert!(!room.placements.is_empty());
}

#[test]
fn unknown_scatter_bounds_type_rejects_decode() {
    // `ScatterBounds` intentionally does NOT carry an `Unknown` fallback —
    // it's a closed union. A mistyped entry must surface as an error so
    // the caller can flag the record as corrupt, rather than silently
    // behaving as "no bounds" and scattering forever.
    let json = r#"{
        "$type": "network.symbios.place.scatter",
        "generator_ref": "x",
        "bounds": { "type": "galaxy", "size": 1 },
        "count": 1,
        "local_seed": "0",
        "biome_filter": { "biomes": [], "water": "Both" },
        "snap_to_terrain": true,
        "random_yaw": true
    }"#;
    let result: Result<Placement, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "closed ScatterBounds union must reject unknown variants instead of silently accepting them"
    );
}

// ---------------------------------------------------------------------------
// The other half of the contract: what is decoded as Unknown is never written
// back (#1111).
// ---------------------------------------------------------------------------

/// A room whose only generator is a kind from a newer client.
fn room_with_unknown_generator() -> RoomRecord {
    let json = r#"{
        "$type": "network.symbios.overlands.room",
        "environment": { "sun_color": [9800, 9500, 8200] },
        "generators": {
            "future_forest": { "$type": "network.symbios.gen.trees2026", "density": 42 }
        },
        "placements": [],
        "traits": {}
    }"#;
    serde_json::from_str(json).expect("decodes")
}

#[test]
fn an_unknown_generator_cannot_be_serialized_back() {
    // The defect this closes: `#[serde(other)]` governs DEcoding only, so
    // `Unknown` used to serialize as `{"$type":"Unknown"}`. An older client
    // that merely opened a room and pressed Save replaced the newer
    // client's generator with that husk — and, because a child record is
    // content-addressed, the split-wire publish then GC'd the original as
    // an orphan. Irrecoverable, and silent.
    let room = room_with_unknown_generator();
    assert!(
        serde_json::to_string(&room).is_err(),
        "a record holding an Unknown arm must refuse to serialize"
    );
}

#[test]
fn the_refusal_reaches_the_owner_as_a_sentence() {
    // A serde error string in the status line is not an explanation. The
    // publish paths route it through `unserializable_reason`, which names
    // the cause and the way out.
    let room = room_with_unknown_generator();
    let err = symbios_overlands::pds::record_size::wire_ready(&room, "world").expect_err("refused");
    assert!(err.contains("newer version"), "explains the cause: {err}");
    assert!(err.contains("Update"), "offers a way out: {err}");
    assert!(
        !err.contains("cannot be serialized"),
        "and does not leak the serde string: {err}"
    );
}

#[test]
fn an_unknown_room_is_not_broadcast_to_peers() {
    // Same rule on the P2P path, which does no size preflight. The old
    // constructor logged and broadcast an EMPTY payload, so every guest
    // decoded nothing at all.
    use symbios_overlands::protocol::OverlandsMessage;
    assert!(
        OverlandsMessage::room_state_update(&room_with_unknown_generator()).is_none(),
        "a room that cannot be written back is not broadcast either"
    );
}

#[test]
fn a_room_this_build_fully_understands_still_publishes_and_broadcasts() {
    // The control: the refusal must be about Unknown arms, not about rooms.
    use symbios_overlands::protocol::OverlandsMessage;
    let room = RoomRecord::default_for_did("did:plc:wire-ready-control");
    assert!(symbios_overlands::pds::record_size::wire_ready(&room, "world").is_ok());
    assert!(OverlandsMessage::room_state_update(&room).is_some());
}
