//! Integration tests for the `RoomRecord` wire format and default recipe.
//!
//! These tests guard the DAG-CBOR and JSON invariants that the ATProto PDS
//! relies on — every `f32` is encoded as fixed-point `i32`, every `u64` as
//! a string, and the default recipe deserialises back into itself.

use symbios_overlands::pds::{
    BiomeFilter, Fp, Fp2, Fp3, GeneratorKind, Placement, RoomRecord, ScatterBounds, WaterRelation,
};

const TEST_DID: &str = "did:plc:z5yhcebtrvzblrojezn6pjgi";

// ---------------------------------------------------------------------------
// Regression guards carried over from the original inline unit tests.
// ---------------------------------------------------------------------------

/// Regression guard for issue #58: 64-bit seeds must serialize as JSON
/// strings, not numbers. Numeric form would round-trip through `f64` in
/// most parsers (including the ones in front of ATProto PDSes), losing
/// precision above `2^53` and triggering `500 InternalServerError`
/// from the DAG-CBOR encoder. The default DID-derived terrain seed
/// is FNV-1a 64-bit, which routinely lands well above the safe range.
#[test]
fn u64_seeds_serialize_as_strings() {
    let r = RoomRecord::default_for_did(TEST_DID);
    let json = serde_json::to_string(&r).expect("serialise");
    assert!(
        json.contains("\"seed\":\""),
        "terrain seed must be a string in JSON, got: {json}"
    );
    let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
    let original_seed = match r.generators.get("base_terrain").map(|g| &g.kind) {
        Some(GeneratorKind::Terrain(cfg)) => cfg.seed,
        _ => panic!("expected base_terrain"),
    };
    let round_seed = match back.generators.get("base_terrain").map(|g| &g.kind) {
        Some(GeneratorKind::Terrain(cfg)) => cfg.seed,
        _ => panic!("expected base_terrain"),
    };
    assert_eq!(original_seed, round_seed);
}

/// Regression guard for issue #48: a `RoomRecord` serialised via serde
/// must contain zero JSON floating-point literals. DAG-CBOR forbids
/// floats and the PDS returns `400 InvalidRequest` when it sees one,
/// so any future field that forgets its `Fp*` wrapper will be caught
/// here. Scans for a digit-dot-digit pattern so the test doesn't
/// false-positive on the `$type` string sigil.
#[test]
fn default_record_serialises_without_floats() {
    let mut record = RoomRecord::default_for_did("did:plc:test");
    record.environment.sun_color = Fp3([0.98, 0.95, 0.82]);
    if let Some(g) = record.generators.get_mut("base_water")
        && let GeneratorKind::Water { level_offset, .. } = &mut g.kind
    {
        *level_offset = Fp(2.5);
    }
    record.placements.push(Placement::Scatter {
        generator_ref: "base_terrain".to_string(),
        bounds: ScatterBounds::Circle {
            center: Fp2([10.5, -3.25]),
            radius: Fp(7.75),
        },
        count: 4,
        local_seed: 42,
        biome_filter: BiomeFilter {
            biomes: vec![0, 2],
            water: WaterRelation::Above,
        },
        snap_to_terrain: true,
        random_yaw: true,
    });

    assert_no_floats(&record);
}

// ---------------------------------------------------------------------------
// Extended record-level coverage.
// ---------------------------------------------------------------------------

/// A newly synthesised homeworld must round-trip through JSON with the
/// same structural shape — otherwise the "Load from PDS" button would
/// silently mutate the record on every fetch. Compared as `Value` because
/// the record carries `HashMap` fields whose iteration order is SipHash-
/// randomised per map.
#[test]
fn default_record_round_trips_through_json() {
    let original = RoomRecord::default_for_did(TEST_DID);
    let json = serde_json::to_string(&original).expect("serialise");
    let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
    let original_v: serde_json::Value = serde_json::to_value(&original).unwrap();
    let back_v: serde_json::Value = serde_json::to_value(&back).unwrap();
    assert_eq!(
        original_v, back_v,
        "default record must round-trip without drift"
    );
}

/// Two different DIDs produce different default recipes — the DID-keyed
/// FNV hash drives the terrain seed and avatar palette, so every player's
/// fresh homeworld is recognisably their own.
#[test]
fn default_records_diverge_across_dids() {
    let a = RoomRecord::default_for_did("did:plc:alice");
    let b = RoomRecord::default_for_did("did:plc:bob");
    let seed_of = |r: &RoomRecord| match r.generators.get("base_terrain").map(|g| &g.kind) {
        Some(GeneratorKind::Terrain(cfg)) => cfg.seed,
        _ => panic!("expected base_terrain"),
    };
    assert_ne!(seed_of(&a), seed_of(&b));
}

/// Same DID → same record every time. Remote peers rebuild the terrain
/// locally from the owner's DID, so any non-determinism in
/// `default_for_did` would desynchronise the shared reality. Compared as
/// `Value` because HashMap iteration order is per-map randomised.
#[test]
fn default_record_is_deterministic() {
    let a = RoomRecord::default_for_did(TEST_DID);
    let b = RoomRecord::default_for_did(TEST_DID);
    let a_v: serde_json::Value = serde_json::to_value(&a).unwrap();
    let b_v: serde_json::Value = serde_json::to_value(&b).unwrap();
    assert_eq!(a_v, b_v);
}

/// Every placement-type variant (Absolute / Scatter / Grid) must
/// round-trip without losing fields. Regression guard for the
/// `#[serde(default)]` fields added over time (`snap_to_terrain`,
/// `random_yaw`, `biome_filter`).
#[test]
fn every_placement_variant_round_trips() {
    use symbios_overlands::pds::TransformData;

    let mut record = RoomRecord::default_for_did(TEST_DID);
    record.placements.push(Placement::Absolute {
        generator_ref: "base_terrain".into(),
        transform: TransformData::default(),
        snap_to_terrain: false,
    });
    record.placements.push(Placement::Scatter {
        generator_ref: "base_terrain".into(),
        bounds: ScatterBounds::Rect {
            center: Fp2([1.0, 2.0]),
            extents: Fp2([10.0, 10.0]),
            rotation: Fp(0.25),
        },
        count: 7,
        local_seed: 999,
        biome_filter: BiomeFilter {
            biomes: vec![2, 3],
            water: WaterRelation::Below,
        },
        snap_to_terrain: true,
        random_yaw: false,
    });
    record.placements.push(Placement::Grid {
        generator_ref: "base_terrain".into(),
        transform: TransformData::default(),
        counts: [3, 1, 3],
        gaps: Fp3([2.0, 0.0, 2.0]),
        snap_to_terrain: true,
        random_yaw: true,
    });

    let json = serde_json::to_string(&record).expect("serialise");
    let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back.placements.len(), record.placements.len());
    assert_no_floats(&back);
}

/// A `GeneratorKind::Shape` round-trips losslessly through JSON. Guards
/// the string-keyed `materials` map (PDS deserialises JSON object keys
/// as strings, which is the natural shape here) and the `seed` field
/// using the shared `u64_as_string` helper used by every other 64-bit
/// numeric on the wire.
#[test]
fn shape_generator_round_trips() {
    use std::collections::HashMap;
    use symbios_overlands::pds::{Generator, SovereignMaterialSettings};

    let mut record = RoomRecord::default_for_did(TEST_DID);
    let mut materials: HashMap<String, SovereignMaterialSettings> = HashMap::new();
    materials.insert(
        "Brick".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.7, 0.3, 0.2]),
            ..Default::default()
        },
    );
    record.generators.insert(
        "tower".into(),
        Generator::from_kind(GeneratorKind::Shape {
            grammar_source: "Lot --> Extrude(8) I(\"Tower\")".into(),
            root_rule: "Lot".into(),
            footprint: Fp3([10.0, 0.0, 10.0]),
            // Above 2^53 to exercise the string-encoded seed path.
            seed: 18_446_744_073_709_551_557,
            materials,
        }),
    );

    let json = serde_json::to_string(&record).expect("serialise");
    // Wire form must encode the seed as a string — see `u64_as_string`.
    assert!(
        json.contains("\"seed\":\"18446744073709551557\""),
        "shape seed must round-trip as a JSON string, got: {json}"
    );
    let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
    let kind = back
        .generators
        .get("tower")
        .map(|g| &g.kind)
        .expect("tower generator");
    let GeneratorKind::Shape {
        grammar_source,
        root_rule,
        footprint,
        seed,
        materials,
    } = kind
    else {
        panic!("expected Shape variant after round-trip, got {:?}", kind);
    };
    assert_eq!(grammar_source, "Lot --> Extrude(8) I(\"Tower\")");
    assert_eq!(root_rule, "Lot");
    assert_eq!(footprint.0, [10.0, 0.0, 10.0]);
    assert_eq!(*seed, 18_446_744_073_709_551_557);
    assert!(materials.contains_key("Brick"));
    assert_no_floats(&back);
}

/// A default recipe carries at least a terrain generator. Regression
/// guard against an accidental empty default slipping through — without
/// terrain the loading gate in `main`/`lib.rs` would stall forever
/// because no heightmap task would ever be spawned.
#[test]
fn default_record_carries_terrain_generator() {
    let r = RoomRecord::default_for_did(TEST_DID);
    assert!(matches!(
        r.generators.get("base_terrain").map(|g| &g.kind),
        Some(GeneratorKind::Terrain(_))
    ));
}

/// Serialised payload size guard: the default record must stay under the
/// `putRecord` body cap the PDS enforces (~1 MiB). If this fails, a
/// zero-config player could never even publish their first homeworld.
#[test]
fn default_record_stays_well_under_pds_body_cap() {
    let r = RoomRecord::default_for_did(TEST_DID);
    let json = serde_json::to_string(&r).expect("serialise");
    const ONE_MIB: usize = 1024 * 1024;
    assert!(
        json.len() < ONE_MIB,
        "default record serialised to {} bytes — uncomfortably close to the PDS cap",
        json.len()
    );
}

/// `Environment` fields use `#[serde(default)]` so pre-atmosphere records
/// (only carrying `sun_color`) still decode without stranding the owner
/// on the recovery banner.
#[test]
fn legacy_environment_with_only_sun_color_decodes() {
    let mut record = RoomRecord::default_for_did(TEST_DID);
    record.environment.sun_color = Fp3([1.0, 0.5, 0.1]);

    let mut value: serde_json::Value = serde_json::to_value(&record).expect("serialise to value");
    // Simulate a pre-atmosphere record: strip every `environment` field
    // except `sun_color`.
    if let Some(env) = value
        .get_mut("environment")
        .and_then(serde_json::Value::as_object_mut)
    {
        let sun = env.get("sun_color").cloned();
        env.clear();
        if let Some(v) = sun {
            env.insert("sun_color".into(), v);
        }
    }
    let back: RoomRecord = serde_json::from_value(value).expect("missing-field defaults must fill");
    assert_eq!(back.environment.sun_color.0, [1.0, 0.5, 0.1]);
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Scan a record's JSON encoding for IEEE-float literals.
/// Defuses DAG-CBOR rejections at the edit boundary instead of at the PDS.
fn assert_no_floats(record: &RoomRecord) {
    let json = serde_json::to_string(record).expect("serialise");
    let bytes = json.as_bytes();
    for i in 1..bytes.len().saturating_sub(1) {
        if bytes[i] == b'.' && bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit() {
            panic!("expected fixed-point integers, got float in `{json}`");
        }
    }
}
