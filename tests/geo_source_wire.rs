//! Wire guard for a geodata region's source on the room record (#1583).
//!
//! Three directions, as every record field gets: a record without a source
//! - every record written before #1583 - decodes, keeps no key and
//! re-encodes unchanged; a record with one names it `geo_source`, with the
//! bytes pinned in `tests/fixtures/geo_source_wire.jsonl`; and a newer
//! client's source - a dataset this build cannot draw, keys it does not know
//! - decodes rather than failing the room. Regenerate the fixture only when
//! the wire form is meant to move, with `GEO_SOURCE_WIRE_BLESS=1`, and say
//! so in the commit.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};
use symbios_overlands::pds::geo_source::BERLIN;
use symbios_overlands::pds::{Environment, GeoSource, RoomRecord};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/geo_source_wire.jsonl")
}

fn dom() -> GeoSource {
    GeoSource::berlin(geodata::GeoSquare {
        min_e: 391_000,
        min_n: 5_819_500,
        size_m: 1_000,
    })
}

fn empty_room() -> RoomRecord {
    RoomRecord {
        lex_type: "network.symbios.room".to_owned(),
        environment: Environment::default(),
        generators: HashMap::new(),
        placements: Vec::new(),
        traits: HashMap::new(),
        contact_effects: Default::default(),
        default_landing: None,
        geo_source: None,
        opaque_refs: Default::default(),
    }
}

fn corpus() -> Vec<String> {
    let other = GeoSource {
        dataset: "hamburg".into(),
        min_e: 560_000,
        min_n: 5_930_000,
        size_m: 2_500,
    };
    [("berlin/dom", dom()), ("hamburg/unknown-dataset", other)]
        .into_iter()
        .map(|(label, source)| {
            let bytes = serde_json::to_string(&source).expect("a source serialises");
            format!("{label}\t{bytes}")
        })
        .collect()
}

#[test]
fn geo_source_bytes_are_pinned() {
    let lines = corpus();
    let path = fixture_path();
    if std::env::var_os("GEO_SOURCE_WIRE_BLESS").is_some() {
        std::fs::write(&path, lines.join("\n") + "\n").expect("bless the fixture");
        return;
    }
    let pinned = std::fs::read_to_string(&path).expect("the fixture exists");
    let pinned: Vec<&str> = pinned.lines().collect();
    assert_eq!(
        pinned.len(),
        lines.len(),
        "corpus and fixture differ in length"
    );
    for (got, want) in lines.iter().zip(pinned) {
        assert_eq!(got, want, "a geo source's wire bytes moved");
    }
}

#[test]
fn a_record_without_a_source_keeps_no_key_and_round_trips() {
    let bytes = serde_json::to_string(&empty_room()).unwrap();
    assert!(!bytes.contains("geo_source"), "an absent source is elided");
    let decoded: RoomRecord = serde_json::from_str(&bytes).unwrap();
    assert_eq!(decoded.geo_source, None);
    assert_eq!(serde_json::to_string(&decoded).unwrap(), bytes);
}

#[test]
fn a_record_with_a_source_names_it_geo_source() {
    let mut room = empty_room();
    room.geo_source = Some(dom());
    let value = serde_json::to_value(&room).unwrap();
    assert_eq!(
        value["geo_source"],
        json!({"dataset": BERLIN, "min_e": 391_000, "min_n": 5_819_500, "size_m": 1_000})
    );
    let decoded: RoomRecord = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.geo_source, Some(dom()));
}

#[test]
fn a_newer_clients_source_decodes_rather_than_failing_the_room() {
    let mut value = serde_json::to_value(empty_room()).unwrap();
    value["geo_source"] = json!({
        "dataset": "hamburg",
        "min_e": 560_000,
        "min_n": 5_930_000,
        "size_m": 2_500,
        "layer_hashes": {"terrain": "0123abcd"},
        "vintage": 2027,
    });
    let mut decoded: RoomRecord = serde_json::from_value(value).unwrap();
    decoded.sanitize();
    // What this build writes back on a save: the dataset and its square
    // survive; the keys it does not know are dropped, as from every record
    // field.
    assert_eq!(
        serde_json::to_value(&decoded).unwrap()["geo_source"],
        json!({"dataset": "hamburg", "min_e": 560_000, "min_n": 5_930_000, "size_m": 2_500})
    );
    let source = decoded.geo_source.expect("an unknown dataset is kept");
    assert_eq!(source.berlin_square(), None, "and draws nothing here");
}

#[test]
fn a_hostile_source_is_put_right_or_dropped_with_the_record() {
    let coverage = geodata::berlin::Coverage::berlin();
    let with = |source: Value| {
        let mut value = serde_json::to_value(empty_room()).unwrap();
        value["geo_source"] = source;
        let mut room: RoomRecord = serde_json::from_value(value).expect("decodes");
        room.sanitize();
        room.geo_source
    };
    // Off the map and too big: moved on and clamped.
    let moved =
        with(json!({"dataset": BERLIN, "min_e": 0, "min_n": 0, "size_m": 4_000_000_000u32}))
            .expect("a Berlin square is put right");
    assert!(coverage.contains(&moved.square()));
    assert_eq!(moved.size_m, geodata::square::SIZE_MAX_M);
    // A dataset id that is not a plain name: dropped.
    let id = "../berlin";
    assert_eq!(
        with(json!({"dataset": id, "min_e": 391_000, "min_n": 5_819_500, "size_m": 1_000})),
        None
    );
}
