//! Integration tests for `InventoryRecord` — the personal stash of
//! `Generator` blueprints owners keep across rooms.

use symbios_overlands::pds::{GeneratorKind, InventoryRecord, RoomRecord};

const TEST_DID: &str = "did:plc:inventory";

#[test]
fn default_inventory_is_empty() {
    let inv = InventoryRecord::default();
    assert!(inv.generators.is_empty());
    assert_eq!(inv.lex_type, "network.symbios.overlands.inventory");
}

#[test]
fn inventory_round_trips_through_json() {
    let mut inv = InventoryRecord::default();
    let template = RoomRecord::default_for_did(TEST_DID)
        .generators
        .get("base_terrain")
        .cloned()
        .expect("default record must carry base_terrain");
    inv.generators
        .insert("favourite_mountains".into(), template);

    let json = serde_json::to_string(&inv).expect("serialise");
    let back: InventoryRecord = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back.lex_type, inv.lex_type);
    assert_eq!(back.generators.len(), 1);
    assert!(back.generators.contains_key("favourite_mountains"));
}

#[test]
fn inventory_cannot_exceed_fifty_slots_after_sanitize() {
    let mut inv = InventoryRecord::default();
    let template = RoomRecord::default_for_did(TEST_DID)
        .generators
        .get("base_terrain")
        .cloned()
        .unwrap();
    for i in 0..500 {
        inv.generators
            .insert(format!("slot_{i:04}"), template.clone());
    }
    inv.sanitize();
    assert!(
        inv.generators.len() <= 50,
        "sanitize must cap the stash at 50 generators"
    );
}

#[test]
fn unknown_generator_survives_inventory_round_trip_as_unknown() {
    // A forward-compatible client might stash a future Generator variant
    // we don't understand. It must arrive back as `Unknown`, not crash
    // the whole InventoryRecord decode.
    let json = r#"{
        "$type": "network.symbios.overlands.inventory",
        "generators": {
            "exotic": { "$type": "network.symbios.gen.cat", "fur_density": 99 }
        }
    }"#;
    let inv: InventoryRecord =
        serde_json::from_str(json).expect("unknown generator must not break the decoder");
    let exotic = inv.generators.get("exotic").expect("entry preserved");
    assert!(matches!(exotic.kind, GeneratorKind::Unknown));
}

#[test]
fn inventory_stores_every_serde_renamed_variant() {
    // Each major generator variant must survive a round-trip from the
    // inventory — this is the stash's whole point.
    let mut inv = InventoryRecord::default();
    let room = RoomRecord::default_for_did(TEST_DID);
    for (k, g) in &room.generators {
        inv.generators.insert(k.clone(), g.clone());
    }
    let json = serde_json::to_string(&inv).expect("serialise");
    let back: InventoryRecord = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back.generators.len(), inv.generators.len());
    for key in inv.generators.keys() {
        assert!(
            back.generators.contains_key(key),
            "inventory dropped generator key `{key}` on round-trip"
        );
    }
}

#[test]
fn inventory_serialises_without_float_literals() {
    let mut inv = InventoryRecord::default();
    let template = RoomRecord::default_for_did(TEST_DID)
        .generators
        .get("base_terrain")
        .cloned()
        .unwrap();
    inv.generators.insert("seeded_terrain".into(), template);

    let json = serde_json::to_string(&inv).expect("serialise");
    let bytes = json.as_bytes();
    for i in 1..bytes.len().saturating_sub(1) {
        if bytes[i] == b'.' && bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit() {
            panic!("inventory record contains a float literal: {json}");
        }
    }
}
