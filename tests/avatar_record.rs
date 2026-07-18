//! Integration tests for `AvatarRecord` — the player's vessel / body.
//!
//! Covers DID-derived defaults, locomotion-preset round-trip across all
//! five variants, the locomotion `kind_tag` hot-swap surface, open-union
//! forward compatibility, and sanitiser clamps on hostile chassis dims.

use symbios_overlands::pds::{
    AirplaneParams, AvatarRecord, CarParams, Fp, Fp3, HelicopterParams, HoverBoatParams,
    HumanoidParams, LocomotionConfig,
};
use symbios_overlands::seeded_defaults::ChassisFamily;

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn default_avatar_locomotion_matches_seeded_chassis_family() {
    // The default is DID-seeded: each chassis family maps to the
    // locomotion preset that matches its visuals (boat → HoverBoat,
    // airship → Helicopter, humanoid → Humanoid, skiff → Car) — see
    // `pds::avatar::default_visuals::build_for_did`.
    for did in [
        "did:plc:alice",
        "did:plc:bob",
        "did:plc:carol",
        "did:plc:dave",
    ] {
        let a = AvatarRecord::default_for_did(did);
        let expected = match ChassisFamily::for_did(did) {
            ChassisFamily::Boat => "hover_boat",
            ChassisFamily::Airship => "helicopter",
            ChassisFamily::Humanoid => "humanoid",
            ChassisFamily::Skiff => "car",
        };
        assert_eq!(
            a.locomotion.kind_tag(),
            expected,
            "{did}: locomotion preset must match the seeded chassis family"
        );
    }
}

#[test]
fn default_avatar_is_deterministic_across_dids() {
    // Remote peers rebuild avatars locally — if default_for_did drifted
    // we'd see different vessels across clients.
    let a = AvatarRecord::default_for_did("did:plc:alice");
    let b = AvatarRecord::default_for_did("did:plc:alice");
    let a_v: serde_json::Value = serde_json::to_value(&a).unwrap();
    let b_v: serde_json::Value = serde_json::to_value(&b).unwrap();
    assert_eq!(a_v, b_v);
}

#[test]
fn default_avatar_palette_differs_across_dids() {
    // Colour differentiation for free — part of the product; without it
    // every fresh player spawns in the same drab vessel.
    let a = AvatarRecord::default_for_did("did:plc:alice");
    let b = AvatarRecord::default_for_did("did:plc:bob");
    let a_v: serde_json::Value = serde_json::to_value(&a).unwrap();
    let b_v: serde_json::Value = serde_json::to_value(&b).unwrap();
    assert_ne!(a_v, b_v);
}

// ---------------------------------------------------------------------------
// Round-trip — every locomotion preset must serialise + deserialise back
// to an equal record so a published avatar reloads byte-identical.
// ---------------------------------------------------------------------------

fn record_with_locomotion(locomotion: LocomotionConfig) -> AvatarRecord {
    let mut a = AvatarRecord::default_for_did("did:plc:alice");
    a.locomotion = locomotion;
    a
}

/// Assert the record survives a wire round-trip without changing on the
/// wire. We compare JSON values rather than Rust structs because every
/// continuous field travels through `Fp` (i32 ÷ 10_000), so an arbitrary
/// `f32` (e.g. a DID-derived palette colour like `0.6315687`) quantises to
/// the nearest `0.0001` step on the way out — `Rust struct == ` would
/// fail on a precision-only diff that is invisible on the wire and is
/// exactly the post-quantisation form every peer actually sees.
fn assert_round_trips(record: &AvatarRecord) {
    let first = serde_json::to_value(record).expect("serialise");
    let back: AvatarRecord = serde_json::from_value(first.clone()).expect("deserialise");
    let second = serde_json::to_value(&back).expect("re-serialise");
    assert_eq!(first, second);
}

#[test]
fn hover_boat_locomotion_round_trips() {
    let a = record_with_locomotion(LocomotionConfig::HoverBoat(
        Box::<HoverBoatParams>::default(),
    ));
    assert_round_trips(&a);
}

#[test]
fn humanoid_locomotion_round_trips() {
    let a = record_with_locomotion(LocomotionConfig::Humanoid(Box::<HumanoidParams>::default()));
    assert_round_trips(&a);
}

#[test]
fn airplane_locomotion_round_trips() {
    let a = record_with_locomotion(LocomotionConfig::Airplane(Box::<AirplaneParams>::default()));
    assert_round_trips(&a);
}

#[test]
fn helicopter_locomotion_round_trips() {
    let a = record_with_locomotion(LocomotionConfig::Helicopter(
        Box::<HelicopterParams>::default(),
    ));
    assert_round_trips(&a);
}

#[test]
fn car_locomotion_round_trips() {
    let a = record_with_locomotion(LocomotionConfig::Car(Box::<CarParams>::default()));
    assert_round_trips(&a);
}

#[test]
fn avatar_serialises_without_float_literals() {
    // DAG-CBOR forbids floats — every continuous field must hop through
    // the fixed-point Fp wrappers and land on the wire as an integer.
    // A literal `0.5` anywhere in the JSON is a regression that the PDS
    // would reject with `400 InvalidRequest`.
    let a = AvatarRecord::default_for_did("did:plc:alice");
    let json = serde_json::to_string(&a).expect("serialise");
    let bytes = json.as_bytes();
    for i in 1..bytes.len().saturating_sub(1) {
        if bytes[i] == b'.' && bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit() {
            panic!("avatar record contains a float literal: {json}");
        }
    }
}

// ---------------------------------------------------------------------------
// Hot-swap — `kind_tag` is the cheap discriminator the player module uses
// to decide whether a `Changed<LiveAvatarRecord>` event should trigger a
// full preset rebuild or just a slider sync.
// ---------------------------------------------------------------------------

#[test]
fn kind_tag_is_stable_across_variants() {
    let hover = LocomotionConfig::HoverBoat(Box::<HoverBoatParams>::default());
    let humanoid = LocomotionConfig::Humanoid(Box::<HumanoidParams>::default());
    let airplane = LocomotionConfig::Airplane(Box::<AirplaneParams>::default());
    let helicopter = LocomotionConfig::Helicopter(Box::<HelicopterParams>::default());
    let car = LocomotionConfig::Car(Box::<CarParams>::default());

    assert_eq!(hover.kind_tag(), "hover_boat");
    assert_eq!(humanoid.kind_tag(), "humanoid");
    assert_eq!(airplane.kind_tag(), "airplane");
    assert_eq!(helicopter.kind_tag(), "helicopter");
    assert_eq!(car.kind_tag(), "car");
    assert_eq!(LocomotionConfig::Unknown.kind_tag(), "unknown");

    // Every distinct preset must have a distinct tag — collisions would
    // make hot-swap detection silently miss a variant change.
    let tags = [
        hover.kind_tag(),
        humanoid.kind_tag(),
        airplane.kind_tag(),
        helicopter.kind_tag(),
        car.kind_tag(),
        LocomotionConfig::Unknown.kind_tag(),
    ];
    for (i, a) in tags.iter().enumerate() {
        for b in &tags[i + 1..] {
            assert_ne!(a, b, "kind_tag collision: {a}");
        }
    }
}

// ---------------------------------------------------------------------------
// Open-union forward compatibility
// ---------------------------------------------------------------------------

#[test]
fn unknown_locomotion_decodes_to_unknown() {
    // Forward-compat: a peer on a newer client might publish a locomotion
    // variant we can't model. It must deserialise to `Unknown`, never
    // panic or fail the whole decode — otherwise an upgrade on one side
    // of the network bricks every other peer's view of that user.
    let alice = AvatarRecord::default_for_did("did:plc:alice");
    let mut value: serde_json::Value = serde_json::to_value(&alice).unwrap();
    value["locomotion"] = serde_json::json!({
        "$type": "network.symbios.locomotion.submarine",
        "depth": 99,
    });
    let avatar: AvatarRecord =
        serde_json::from_value(value).expect("must decode to Unknown locomotion");
    assert_eq!(avatar.locomotion.kind_tag(), "unknown");
}

// ---------------------------------------------------------------------------
// Sanitize
// ---------------------------------------------------------------------------

#[test]
fn avatar_sanitize_clamps_non_finite_chassis_dimensions() {
    // HoverBoat chassis half-extents are `Fp3` — the sanitize pass must
    // clamp NaN/infinity/negative back into a safe positive range before
    // the spawner uses them for `Collider::cuboid`, which panics on
    // non-finite or non-positive sides.
    // Pin the HoverBoat preset explicitly — the DID-seeded default may
    // land on any chassis family.
    let mut avatar = AvatarRecord::default_for_did("did:plc:alice");
    let mut params = Box::<HoverBoatParams>::default();
    params.chassis_half_extents = Fp3([f32::NAN, f32::INFINITY, -1.0]);
    params.mass = Fp(f32::NAN);
    avatar.locomotion = LocomotionConfig::HoverBoat(params);

    avatar.sanitize();

    let LocomotionConfig::HoverBoat(params) = &avatar.locomotion else {
        panic!("sanitize must not change the locomotion variant");
    };
    for &v in &params.chassis_half_extents.0 {
        assert!(
            v.is_finite() && v > 0.0,
            "chassis half-extent must be positive finite: got {v}"
        );
    }
    assert!(
        params.mass.0.is_finite() && params.mass.0 > 0.0,
        "mass must be positive finite: got {}",
        params.mass.0
    );
}

// ---------------------------------------------------------------------------
// Wire back-compat (#874/#876)
// ---------------------------------------------------------------------------

/// A record published before #874 (no `gait` section) and before #876 (no
/// promoted feel fields on its locomotion preset) must still deserialize —
/// field-level serde defaults fill in the historical constants, and the
/// missing gait section stays `None` (the DID-seeded fallback).
#[test]
fn pre_gait_pre_feel_records_still_deserialize() {
    let presets: [LocomotionConfig; 5] = [
        LocomotionConfig::HoverBoat(Box::default()),
        LocomotionConfig::Humanoid(Box::default()),
        LocomotionConfig::Airplane(Box::default()),
        LocomotionConfig::Helicopter(Box::default()),
        LocomotionConfig::Car(Box::default()),
    ];
    // Every field #874/#876 added, across all presets. Stripping keys a
    // preset doesn't carry is a no-op.
    let added_fields = [
        "stop_damping",
        "turn_rate",
        "upright_engage_tilt_degrees",
        "upright_assist_accel",
        "upright_assist_damping",
        "center_of_mass_drop",
        "stabilize_torque",
        "cruise_throttle",
    ];
    for preset in presets {
        let mut avatar = AvatarRecord::default_for_did("did:plc:alice");
        let tag = preset.kind_tag();
        avatar.locomotion = preset;

        let mut value = serde_json::to_value(&avatar).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("gait").expect("default record carries gait");
        let loco = obj["locomotion"].as_object_mut().unwrap();
        for field in added_fields {
            loco.remove(field);
        }

        let old: AvatarRecord = serde_json::from_value(value)
            .unwrap_or_else(|e| panic!("{tag}: pre-#874/#876 record must decode: {e}"));
        assert!(old.gait.is_none(), "{tag}: absent gait stays None");
        // The defaults reproduce the exact record the new schema builds,
        // so old and new clients agree on the feel. Compared on the wire
        // (same as `assert_round_trips`): continuous fields quantise
        // through `Fp`, so struct equality would fail on precision-only
        // diffs invisible to every peer.
        assert_eq!(
            serde_json::to_value(&old.locomotion).unwrap(),
            serde_json::to_value(&avatar.locomotion).unwrap(),
            "{tag}"
        );
    }
}

/// The re-roll path derives the gait from the same master seed as the
/// visuals, so two peers reading the published record and a client
/// re-deriving from the seed agree.
#[test]
fn reroll_gait_follows_the_seed() {
    let a = AvatarRecord::default_for_seed(42);
    let b = AvatarRecord::default_for_seed(42);
    assert_eq!(a.gait, b.gait);
    assert!(a.gait.is_some());
    let c = AvatarRecord::default_for_seed(43);
    assert_ne!(a.gait, c.gait, "different seed, different idle motion");
}
