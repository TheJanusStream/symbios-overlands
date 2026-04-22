//! Forward-compatibility tests: every tagged-union record field carries a
//! `#[serde(other)] Unknown` arm so a client visiting a record authored by
//! a newer engine silently skips the unrecognised variants instead of
//! failing the whole decode.
//!
//! These tests synthesise payloads carrying `$type` values that don't exist
//! in the current lexicon and assert the decoder preserves them as
//! `Unknown`.

use symbios_overlands::pds::{Generator, Placement, RoomRecord};

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
    assert!(matches!(g, Generator::Unknown));
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
        room.generators.get("base_water"),
        Some(Generator::Water { .. })
    ));
    assert!(matches!(
        room.generators.get("future_arch"),
        Some(Generator::Unknown)
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
