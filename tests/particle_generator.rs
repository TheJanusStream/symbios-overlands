//! Integration tests for the [`ParticleSystem`](symbios_overlands::pds::GeneratorKind::ParticleSystem)
//! generator. Cover wire-format round-trips for every [`EmitterShape`],
//! [`ParticleBlendMode`], and [`SimulationSpace`] variant; forward-compat
//! for `Unknown` payloads on each open union; sanitiser clamps for the
//! full numeric envelope (rate, max_particles, lifetime, speed,
//! acceleration, inherit_velocity, bounce, friction); the `min ≤ max`
//! invariant on lifetime / speed pairs; and the `kind_tag` uniqueness
//! invariant the editor's variant picker depends on.

use symbios_overlands::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    SignSource, SimulationSpace, TextureAtlas, TextureFilter, limits, sanitize_generator,
};

/// Helper: build a ParticleSystem generator with the supplied
/// emitter-shape / blend / space, leaving every other field at the
/// `default_particles` baseline so each test isolates one knob.
fn sample_particles(
    shape: EmitterShape,
    blend: ParticleBlendMode,
    space: SimulationSpace,
) -> Generator {
    Generator::from_kind(GeneratorKind::ParticleSystem {
        emitter_shape: shape,
        rate_per_second: Fp(20.0),
        burst_count: 4,
        max_particles: 100,
        looping: true,
        duration: Fp(2.0),
        lifetime_min: Fp(0.5),
        lifetime_max: Fp(1.5),
        speed_min: Fp(1.0),
        speed_max: Fp(3.0),
        gravity_multiplier: Fp(1.0),
        acceleration: Fp3([0.0, 0.0, 0.0]),
        linear_drag: Fp(0.2),
        start_size: Fp(0.1),
        end_size: Fp(0.0),
        start_color: Fp4([1.0, 0.5, 0.2, 1.0]),
        end_color: Fp4([1.0, 0.5, 0.2, 0.0]),
        blend_mode: blend,
        billboard: true,
        simulation_space: space,
        inherit_velocity: Fp(0.0),
        collide_terrain: false,
        collide_water: false,
        collide_colliders: false,
        bounce: Fp(0.3),
        friction: Fp(0.5),
        seed: 0xCAFEBABE,
        texture: None,
        texture_atlas: None,
        frame_mode: AnimationFrameMode::Still,
        texture_filter: TextureFilter::Linear,
    })
}

/// Build a textured-particle Generator — used by the new round-trip
/// tests for the #200 follow-up.
fn sample_textured_particles(
    texture: SignSource,
    atlas: Option<TextureAtlas>,
    frame_mode: AnimationFrameMode,
    filter: TextureFilter,
) -> Generator {
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem {
        texture: t,
        texture_atlas: a,
        frame_mode: f,
        texture_filter: tf,
        ..
    } = &mut g.kind
    {
        *t = Some(texture);
        *a = atlas;
        *f = frame_mode;
        *tf = filter;
    }
    g
}

// ---------------------------------------------------------------------------
// Emitter-shape round-trips.
// ---------------------------------------------------------------------------

#[test]
fn point_emitter_round_trips() {
    let g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn sphere_emitter_round_trips() {
    let g = sample_particles(
        EmitterShape::Sphere { radius: Fp(2.5) },
        ParticleBlendMode::Additive,
        SimulationSpace::Local,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn box_emitter_round_trips() {
    let g = sample_particles(
        EmitterShape::Box {
            half_extents: Fp3([1.0, 0.5, 2.0]),
        },
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn cone_emitter_round_trips() {
    let g = sample_particles(
        EmitterShape::Cone {
            half_angle: Fp(0.6),
            height: Fp(1.5),
        },
        ParticleBlendMode::Alpha,
        SimulationSpace::Local,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Forward-compat: unknown variants on each sub-union.
// ---------------------------------------------------------------------------

#[test]
fn unknown_emitter_shape_decodes_to_unknown() {
    let json = json_with_emitter_shape(
        r#"{ "$type": "network.symbios.particle.future_galaxy_2030", "stars": 1000 }"#,
    );
    let kind: GeneratorKind =
        serde_json::from_str(&json).expect("unknown shape must not crash decode");
    match kind {
        GeneratorKind::ParticleSystem { emitter_shape, .. } => {
            assert!(matches!(emitter_shape, EmitterShape::Unknown))
        }
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

#[test]
fn unknown_blend_mode_decodes_to_unknown() {
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem { blend_mode, .. } = &mut g.kind {
        *blend_mode = ParticleBlendMode::Unknown;
    }
    // Round-trip via JSON literal carrying an unknown blend tag.
    let json =
        json_with_blend_mode(r#"{ "$type": "network.symbios.particle.blend.future_glow_2030" }"#);
    let kind: GeneratorKind =
        serde_json::from_str(&json).expect("unknown blend must not crash decode");
    match kind {
        GeneratorKind::ParticleSystem { blend_mode, .. } => {
            assert!(matches!(blend_mode, ParticleBlendMode::Unknown))
        }
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

#[test]
fn unknown_simulation_space_decodes_to_unknown() {
    let json = json_with_simulation_space(
        r#"{ "$type": "network.symbios.particle.space.future_warp_2030" }"#,
    );
    let kind: GeneratorKind =
        serde_json::from_str(&json).expect("unknown space must not crash decode");
    match kind {
        GeneratorKind::ParticleSystem {
            simulation_space, ..
        } => assert!(matches!(simulation_space, SimulationSpace::Unknown)),
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Sanitiser clamps.
// ---------------------------------------------------------------------------

#[test]
fn sanitiser_clamps_max_particles_and_rate() {
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem {
        max_particles,
        rate_per_second,
        burst_count,
        ..
    } = &mut g.kind
    {
        *max_particles = u32::MAX;
        *rate_per_second = Fp(1_000_000.0);
        *burst_count = u32::MAX;
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem {
        max_particles,
        rate_per_second,
        burst_count,
        ..
    } = &g.kind
    {
        assert!(*max_particles <= limits::MAX_PARTICLES);
        assert!(rate_per_second.0 <= limits::MAX_PARTICLE_RATE);
        assert!(*burst_count <= limits::MAX_PARTICLE_BURST);
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_enforces_min_le_max_on_lifetime_and_speed() {
    // Inverted intervals would make the per-particle sampler return
    // garbage; the sanitiser swaps so `max ≥ min`.
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem {
        lifetime_min,
        lifetime_max,
        speed_min,
        speed_max,
        ..
    } = &mut g.kind
    {
        *lifetime_min = Fp(5.0);
        *lifetime_max = Fp(1.0);
        *speed_min = Fp(10.0);
        *speed_max = Fp(2.0);
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem {
        lifetime_min,
        lifetime_max,
        speed_min,
        speed_max,
        ..
    } = &g.kind
    {
        assert!(lifetime_max.0 >= lifetime_min.0, "lifetime min ≤ max");
        assert!(speed_max.0 >= speed_min.0, "speed min ≤ max");
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_clamps_acceleration_per_axis() {
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem { acceleration, .. } = &mut g.kind {
        *acceleration = Fp3([f32::INFINITY, -1_000_000.0, f32::NAN]);
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem { acceleration, .. } = &g.kind {
        for a in &acceleration.0 {
            assert!(a.is_finite());
            assert!(a.abs() <= limits::MAX_PARTICLE_ACCEL);
        }
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_clamps_inherit_velocity_bounce_friction() {
    let mut g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    if let GeneratorKind::ParticleSystem {
        inherit_velocity,
        bounce,
        friction,
        ..
    } = &mut g.kind
    {
        *inherit_velocity = Fp(99.0);
        *bounce = Fp(2.5);
        *friction = Fp(-10.0);
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem {
        inherit_velocity,
        bounce,
        friction,
        ..
    } = &g.kind
    {
        assert!(inherit_velocity.0 <= limits::MAX_PARTICLE_INHERIT_VELOCITY);
        assert!(inherit_velocity.0 >= 0.0);
        assert!(bounce.0 >= 0.0 && bounce.0 <= 1.0);
        assert!(friction.0 >= 0.0 && friction.0 <= 1.0);
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_clamps_emitter_shape_extents() {
    // Sphere radius / box half-extents / cone height all clamp into a
    // 100 m envelope so a hostile record can't smuggle a galaxy-sized
    // emitter into the spawn shape sampler.
    for shape in [
        EmitterShape::Sphere {
            radius: Fp(10_000.0),
        },
        EmitterShape::Box {
            half_extents: Fp3([10_000.0, 10_000.0, 10_000.0]),
        },
        EmitterShape::Cone {
            half_angle: Fp(100.0),
            height: Fp(10_000.0),
        },
    ] {
        let mut g = sample_particles(shape, ParticleBlendMode::Alpha, SimulationSpace::World);
        sanitize_generator(&mut g);
        if let GeneratorKind::ParticleSystem { emitter_shape, .. } = &g.kind {
            match emitter_shape {
                EmitterShape::Sphere { radius } => {
                    assert!(radius.0 <= limits::MAX_PARTICLE_SHAPE_RADIUS)
                }
                EmitterShape::Box { half_extents } => {
                    for h in &half_extents.0 {
                        assert!(*h <= limits::MAX_PARTICLE_SHAPE_HALF_EXTENT);
                    }
                }
                EmitterShape::Cone { half_angle, height } => {
                    assert!(half_angle.0 <= limits::MAX_PARTICLE_CONE_HALF_ANGLE);
                    assert!(height.0 <= limits::MAX_PARTICLE_SHAPE_HEIGHT);
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Editor invariants.
// ---------------------------------------------------------------------------

#[test]
fn particle_kind_tag_is_unique() {
    let kinds: Vec<&'static str> = vec![
        GeneratorKind::default_cuboid().kind_tag(),
        GeneratorKind::default_sign().kind_tag(),
        GeneratorKind::default_particles().kind_tag(),
    ];
    let mut seen = std::collections::HashSet::new();
    for k in &kinds {
        assert!(seen.insert(*k), "duplicate kind_tag: {k}");
    }
    assert!(kinds.contains(&"ParticleSystem"));
}

#[test]
fn default_particles_round_trips() {
    let g = Generator::from_kind(GeneratorKind::default_particles());
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );

    let mut sanitised = g.clone();
    sanitize_generator(&mut sanitised);
    assert_eq!(
        serde_json::to_value(&sanitised).unwrap(),
        serde_json::to_value(&g).unwrap(),
        "default_particles must be sanitiser-stable"
    );
}

#[test]
fn seed_serialises_as_string() {
    // u64 seeds must serialise as JSON strings (DAG-CBOR rejects
    // numbers above 2^53 — same reason terrain seeds use the
    // `u64_as_string` adapter).
    let g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    assert!(
        json.contains("\"seed\":\""),
        "seed must serialise as string in JSON, got {json}"
    );
}

// ---------------------------------------------------------------------------
// Textured-particle round-trip + sanitiser tests (#200).
// ---------------------------------------------------------------------------

#[test]
fn textured_particles_url_source_round_trips() {
    let g = sample_textured_particles(
        SignSource::Url {
            url: "https://example.org/sparks.png".into(),
        },
        Some(TextureAtlas { rows: 4, cols: 8 }),
        AnimationFrameMode::OverLifetime { fps: Fp(12.0) },
        TextureFilter::Nearest,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn textured_particles_atproto_blob_source_round_trips() {
    let g = sample_textured_particles(
        SignSource::AtprotoBlob {
            did: "did:plc:vfx".into(),
            cid: "bafyparticles".into(),
        },
        None,
        AnimationFrameMode::Still,
        TextureFilter::Linear,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn textured_particles_did_pfp_source_round_trips() {
    let g = sample_textured_particles(
        SignSource::DidPfp {
            did: "did:plc:author".into(),
        },
        Some(TextureAtlas { rows: 1, cols: 1 }),
        AnimationFrameMode::RandomFrame,
        TextureFilter::Linear,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        serde_json::to_value(&back).unwrap()
    );
}

#[test]
fn untextured_particles_omit_texture_fields_in_json() {
    // The texture / atlas Option fields use
    // `serde(skip_serializing_if = "Option::is_none")` so a record
    // without textures stays byte-identical to the pre-#200 wire
    // format. Verifying this keeps backward compat with rooms
    // published before the textured-particles patch landed.
    let g = sample_particles(
        EmitterShape::Point,
        ParticleBlendMode::Alpha,
        SimulationSpace::World,
    );
    let json = serde_json::to_string(&g).expect("serialise");
    assert!(
        !json.contains("\"texture\":"),
        "untextured particles must omit `texture` field, got {json}"
    );
    assert!(
        !json.contains("\"texture_atlas\":"),
        "untextured particles must omit `texture_atlas` field, got {json}"
    );
}

// ---------------------------------------------------------------------------
// Forward-compat: unknown frame_mode / texture_filter / texture-source
// variants decode to Unknown.
// ---------------------------------------------------------------------------

#[test]
fn unknown_frame_mode_decodes_to_unknown() {
    let json = json_with_frame_mode(
        r#"{ "$type": "network.symbios.particle.frame.future_motion_2030", "speed": 1 }"#,
    );
    let kind: GeneratorKind =
        serde_json::from_str(&json).expect("unknown frame_mode must not crash decode");
    match kind {
        GeneratorKind::ParticleSystem { frame_mode, .. } => {
            assert!(matches!(frame_mode, AnimationFrameMode::Unknown))
        }
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

#[test]
fn unknown_texture_filter_decodes_to_unknown() {
    let json = json_with_texture_filter(
        r#"{ "$type": "network.symbios.particle.filter.future_anisotropic_2030", "level": 16 }"#,
    );
    let kind: GeneratorKind =
        serde_json::from_str(&json).expect("unknown filter must not crash decode");
    match kind {
        GeneratorKind::ParticleSystem { texture_filter, .. } => {
            assert!(matches!(texture_filter, TextureFilter::Unknown))
        }
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

#[test]
fn missing_texture_fields_decode_to_defaults() {
    // Records without the new fields (pre-#200) must deserialise
    // cleanly — `texture` / `texture_atlas` to `None`, `frame_mode`
    // to `Still`, `texture_filter` to `Linear`.
    let json = base_particles_json(DEFAULT_SHAPE, DEFAULT_BLEND, DEFAULT_SPACE);
    let kind: GeneratorKind = serde_json::from_str(&json).expect("legacy decode");
    match kind {
        GeneratorKind::ParticleSystem {
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
            ..
        } => {
            assert!(texture.is_none());
            assert!(texture_atlas.is_none());
            assert!(matches!(frame_mode, AnimationFrameMode::Still));
            assert!(matches!(texture_filter, TextureFilter::Linear));
        }
        other => panic!("expected ParticleSystem, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Sanitiser clamps for the new texture envelope.
// ---------------------------------------------------------------------------

#[test]
fn sanitiser_clamps_atlas_dims() {
    let mut g = sample_textured_particles(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        Some(TextureAtlas {
            rows: 9999,
            cols: 0,
        }),
        AnimationFrameMode::Still,
        TextureFilter::Linear,
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem { texture_atlas, .. } = &g.kind {
        let atlas = texture_atlas.as_ref().expect("atlas survived");
        assert!(atlas.rows >= 1 && atlas.rows <= limits::MAX_PARTICLE_ATLAS_DIM);
        assert!(atlas.cols >= 1 && atlas.cols <= limits::MAX_PARTICLE_ATLAS_DIM);
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_clamps_over_lifetime_fps() {
    let mut g = sample_textured_particles(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        Some(TextureAtlas { rows: 4, cols: 4 }),
        AnimationFrameMode::OverLifetime {
            fps: Fp(f32::INFINITY),
        },
        TextureFilter::Linear,
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem { frame_mode, .. } = &g.kind {
        if let AnimationFrameMode::OverLifetime { fps } = frame_mode {
            assert!(fps.0.is_finite());
            assert!(fps.0 >= 0.0 && fps.0 <= limits::MAX_PARTICLE_FRAME_FPS);
        } else {
            panic!("expected OverLifetime, got {frame_mode:?}");
        }
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

#[test]
fn sanitiser_truncates_textured_url() {
    let huge_url = format!(
        "https://example.org/{}",
        "a".repeat(limits::MAX_SIGN_URL_BYTES * 2)
    );
    let mut g = sample_textured_particles(
        SignSource::Url {
            url: huge_url.clone(),
        },
        None,
        AnimationFrameMode::Still,
        TextureFilter::Linear,
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::ParticleSystem { texture, .. } = &g.kind {
        if let Some(SignSource::Url { url }) = texture {
            assert!(url.len() <= limits::MAX_SIGN_URL_BYTES);
        } else {
            panic!("expected Url texture, got {texture:?}");
        }
    } else {
        panic!("expected ParticleSystem after sanitise");
    }
}

// ---------------------------------------------------------------------------
// Helpers for the forward-compat tests — substitute one sub-union slot
// at a time, leaving the others at their default-known values.
// ---------------------------------------------------------------------------

const DEFAULT_SHAPE: &str = r#"{ "$type": "network.symbios.particle.point" }"#;
const DEFAULT_BLEND: &str = r#"{ "$type": "network.symbios.particle.blend.alpha" }"#;
const DEFAULT_SPACE: &str = r#"{ "$type": "network.symbios.particle.space.world" }"#;

fn json_with_emitter_shape(shape_json: &str) -> String {
    base_particles_json(shape_json, DEFAULT_BLEND, DEFAULT_SPACE)
}

fn json_with_blend_mode(blend_json: &str) -> String {
    base_particles_json(DEFAULT_SHAPE, blend_json, DEFAULT_SPACE)
}

fn json_with_simulation_space(space_json: &str) -> String {
    base_particles_json(DEFAULT_SHAPE, DEFAULT_BLEND, space_json)
}

/// Inject a custom `frame_mode` payload into an otherwise-default
/// ParticleSystem JSON record. Used to verify forward-compat for
/// unknown frame-mode tags.
fn json_with_frame_mode(frame_mode_json: &str) -> String {
    let base = base_particles_json(DEFAULT_SHAPE, DEFAULT_BLEND, DEFAULT_SPACE);
    // Inject the new field just before the closing brace.
    let close = base.rfind('}').expect("closing brace");
    let (head, tail) = base.split_at(close);
    format!("{head}, \"frame_mode\": {frame_mode_json}{tail}")
}

/// Same shape as `json_with_frame_mode`, but injects a custom
/// `texture_filter` payload.
fn json_with_texture_filter(filter_json: &str) -> String {
    let base = base_particles_json(DEFAULT_SHAPE, DEFAULT_BLEND, DEFAULT_SPACE);
    let close = base.rfind('}').expect("closing brace");
    let (head, tail) = base.split_at(close);
    format!("{head}, \"texture_filter\": {filter_json}{tail}")
}

fn base_particles_json(shape: &str, blend: &str, space: &str) -> String {
    // FP_SCALE = 10000; the literals here mirror the wire format.
    format!(
        r#"{{
            "$type": "network.symbios.gen.particles",
            "emitter_shape": {shape},
            "rate_per_second": 200000,
            "burst_count": 4,
            "max_particles": 100,
            "looping": true,
            "duration": 20000,
            "lifetime_min": 5000,
            "lifetime_max": 15000,
            "speed_min": 10000,
            "speed_max": 30000,
            "gravity_multiplier": 10000,
            "acceleration": [0, 0, 0],
            "linear_drag": 2000,
            "start_size": 1000,
            "end_size": 0,
            "start_color": [10000, 5000, 2000, 10000],
            "end_color": [10000, 5000, 2000, 0],
            "blend_mode": {blend},
            "billboard": true,
            "simulation_space": {space},
            "inherit_velocity": 0,
            "collide_terrain": false,
            "collide_water": false,
            "collide_colliders": false,
            "bounce": 3000,
            "friction": 5000,
            "seed": "12345"
        }}"#
    )
}
