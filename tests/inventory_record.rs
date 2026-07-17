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
fn sanitize_bounds_the_stash_at_the_dos_backstop_not_the_gameplay_cap() {
    // #841 changed the contract this test used to pin: sanitize no longer
    // truncates to the 50-item gameplay cap (that silently deleted items
    // in lexicographic order on login — the alphabet chose which). An
    // over-cap stash must now SURVIVE the load — the Inventory window
    // surfaces it red and blocks publishing until the user prunes — while
    // the hostile-PDS DoS backstop still bounds the allocation.
    let cap = symbios_overlands::config::state::MAX_INVENTORY_ITEMS;
    let bound = symbios_overlands::config::state::MAX_INVENTORY_SANITIZE_ITEMS;
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
        inv.generators.len() > cap,
        "an over-cap stash must survive sanitize — the {cap}-item cap is \
         enforced by the UI, not by silent truncation (#841)"
    );
    assert_eq!(
        inv.generators.len(),
        bound,
        "sanitize truncates only at the {bound}-item DoS backstop"
    );
    // Deterministic survivors: lexicographic low keys stay.
    assert!(inv.generators.contains_key("slot_0000"));
    assert!(!inv.generators.contains_key(&format!("slot_{:04}", bound)));
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
