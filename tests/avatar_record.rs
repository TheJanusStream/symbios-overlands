//! Integration tests for `AvatarRecord` — the record describing the
//! player's vessel or body. Covers DID-derived defaults, phenotype /
//! kinematics round-trip, and hot-swap tag stability.

use symbios_overlands::pds::{
    AvatarBody, AvatarRecord, HumanoidKinematics, HumanoidPhenotype, RoverKinematics,
    RoverPhenotype,
};

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn default_avatar_is_hover_rover() {
    let a = AvatarRecord::default_for_did("did:plc:alice");
    assert!(matches!(a.body, AvatarBody::HoverRover { .. }));
    assert_eq!(a.body.kind_tag(), "hover_rover");
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
    // every fresh player spawns in the same drab rover.
    let a = AvatarRecord::default_for_did("did:plc:alice");
    let b = AvatarRecord::default_for_did("did:plc:bob");
    let a_v: serde_json::Value = serde_json::to_value(&a).unwrap();
    let b_v: serde_json::Value = serde_json::to_value(&b).unwrap();
    assert_ne!(a_v, b_v);
}

// ---------------------------------------------------------------------------
// Round-trip
// ---------------------------------------------------------------------------

#[test]
fn hover_rover_body_round_trips() {
    let original = AvatarRecord {
        lex_type: "network.symbios.overlands.avatar".into(),
        body: AvatarBody::HoverRover {
            phenotype: Box::new(RoverPhenotype::default()),
            kinematics: Box::new(RoverKinematics::default()),
        },
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: AvatarRecord = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(original, back);
}

#[test]
fn humanoid_body_round_trips() {
    let original = AvatarRecord {
        lex_type: "network.symbios.overlands.avatar".into(),
        body: AvatarBody::Humanoid {
            phenotype: Box::new(HumanoidPhenotype::default()),
            kinematics: Box::new(HumanoidKinematics::default()),
        },
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: AvatarRecord = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(original, back);
}

#[test]
fn avatar_serialises_without_float_literals() {
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
// Hot-swap
// ---------------------------------------------------------------------------

#[test]
fn kind_tag_is_stable_across_variants() {
    let hover = AvatarBody::HoverRover {
        phenotype: Box::new(RoverPhenotype::default()),
        kinematics: Box::new(RoverKinematics::default()),
    };
    let humanoid = AvatarBody::Humanoid {
        phenotype: Box::new(HumanoidPhenotype::default()),
        kinematics: Box::new(HumanoidKinematics::default()),
    };
    assert_eq!(hover.kind_tag(), "hover_rover");
    assert_eq!(humanoid.kind_tag(), "humanoid");
    assert_ne!(hover.kind_tag(), humanoid.kind_tag());
    assert_eq!(AvatarBody::Unknown.kind_tag(), "unknown");
}

// ---------------------------------------------------------------------------
// Open-union forward compatibility
// ---------------------------------------------------------------------------

#[test]
fn unknown_avatar_body_decodes_to_unknown() {
    // Forward-compat: a peer on a newer client might publish a body
    // variant we can't model. It must deserialise to `Unknown`, never
    // panic or fail the whole decode.
    let json = r#"{
        "$type": "network.symbios.overlands.avatar",
        "body": { "$type": "network.symbios.avatar.submarine", "depth": 99 }
    }"#;
    let avatar: AvatarRecord = serde_json::from_str(json).expect("must decode to Unknown body");
    assert_eq!(avatar.body.kind_tag(), "unknown");
}

// ---------------------------------------------------------------------------
// Sanitize
// ---------------------------------------------------------------------------

#[test]
fn avatar_sanitize_clamps_non_finite_dimensions() {
    // Rover phenotype fields are `Fp` — the sanitize pass must clamp
    // NaN/infinity back into a safe range before the archetype spawner
    // uses them for collider and mesh sizes.
    use symbios_overlands::pds::Fp;
    let mut avatar = AvatarRecord::default_for_did("did:plc:alice");
    if let AvatarBody::HoverRover { phenotype, .. } = &mut avatar.body {
        phenotype.hull_length = Fp(f32::NAN);
        phenotype.hull_width = Fp(f32::INFINITY);
        phenotype.hull_depth = Fp(-1.0);
    }
    avatar.sanitize();
    if let AvatarBody::HoverRover { phenotype, .. } = &avatar.body {
        for &v in &[
            phenotype.hull_length.0,
            phenotype.hull_width.0,
            phenotype.hull_depth.0,
        ] {
            assert!(
                v.is_finite() && v > 0.0,
                "hull dim must be positive finite: got {v}"
            );
        }
    } else {
        panic!("expected HoverRover variant after sanitize");
    }
}
