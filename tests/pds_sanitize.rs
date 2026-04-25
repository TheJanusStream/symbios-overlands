//! Integration tests for `pds::sanitize` — the clamp pass that every
//! inbound record traverses before the world compiler touches it.
//!
//! The overarching contract is that sanitise never panics on pathological
//! input — NaN, infinities, negative dimensions, recursive generator trees,
//! giant counts — and that every numeric field lands inside the
//! `pds::limits` envelope afterwards.

use symbios_overlands::pds::{
    Fp, Fp3, Generator, GeneratorKind, InventoryRecord, RoomRecord, limits, sanitize_generator,
};

const TEST_DID: &str = "did:plc:sanitise";

// ---------------------------------------------------------------------------
// Terrain-generator clamps
// ---------------------------------------------------------------------------

#[test]
fn terrain_grid_size_clamped_to_max() {
    let mut r = RoomRecord::default_for_did(TEST_DID);
    if let Some(g) = r.generators.get_mut("base_terrain")
        && let GeneratorKind::Terrain(cfg) = &mut g.kind
    {
        cfg.grid_size = u32::MAX;
        cfg.octaves = u32::MAX;
        cfg.erosion_drops = u32::MAX;
        cfg.thermal_iterations = u32::MAX;
        cfg.voronoi_num_seeds = u32::MAX;
        cfg.voronoi_num_terraces = u32::MAX;
        cfg.material.texture_size = u32::MAX;
    }
    r.sanitize();

    match r.generators.get("base_terrain").map(|g| &g.kind) {
        Some(GeneratorKind::Terrain(cfg)) => {
            assert!(cfg.grid_size <= limits::MAX_GRID_SIZE);
            assert!(cfg.octaves <= limits::MAX_OCTAVES);
            assert!(cfg.erosion_drops <= limits::MAX_EROSION_DROPS);
            assert!(cfg.thermal_iterations <= limits::MAX_THERMAL_ITERATIONS);
            assert!(cfg.voronoi_num_seeds <= limits::MAX_VORONOI_SEEDS);
            assert!(cfg.voronoi_num_terraces <= limits::MAX_VORONOI_TERRACES);
            assert!(cfg.material.texture_size <= limits::MAX_TEXTURE_SIZE);
        }
        other => panic!("expected Terrain, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Placement clamps
// ---------------------------------------------------------------------------

#[test]
fn scatter_count_clamped_to_max() {
    use symbios_overlands::pds::{BiomeFilter, Fp2, Placement, ScatterBounds};
    let mut r = RoomRecord::default_for_did(TEST_DID);
    r.placements.push(Placement::Scatter {
        generator_ref: "base_terrain".into(),
        bounds: ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(16.0),
        },
        count: u32::MAX,
        local_seed: 1,
        biome_filter: BiomeFilter::default(),
        snap_to_terrain: true,
        random_yaw: true,
    });
    r.sanitize();
    for p in &r.placements {
        if let Placement::Scatter { count, .. } = p {
            assert!(*count <= limits::MAX_SCATTER_COUNT);
        }
    }
}

#[test]
fn placements_over_cap_are_trimmed() {
    use symbios_overlands::pds::{Placement, TransformData};
    let mut r = RoomRecord::default_for_did(TEST_DID);
    for _ in 0..(limits::MAX_PLACEMENTS * 2) {
        r.placements.push(Placement::Absolute {
            generator_ref: "base_terrain".into(),
            transform: TransformData::default(),
            snap_to_terrain: false,
        });
    }
    r.sanitize();
    assert!(r.placements.len() <= limits::MAX_PLACEMENTS);
}

// ---------------------------------------------------------------------------
// Generator-tree clamps
// ---------------------------------------------------------------------------

#[test]
fn generator_depth_and_node_budget_enforced() {
    // Build a pathological chain twice as deep as the limit.
    let mut deep = Generator::default();
    let chain_depth = (limits::MAX_GENERATOR_DEPTH * 4) as usize;
    let mut cursor = &mut deep;
    for _ in 0..chain_depth {
        cursor.children.push(Generator::default());
        cursor = cursor.children.last_mut().unwrap();
    }

    sanitize_generator(&mut deep);

    let mut d = 0u32;
    let mut c = 0u32;
    count_nodes(&deep, 0, &mut d, &mut c);
    assert!(d <= limits::MAX_GENERATOR_DEPTH, "depth {d} exceeds limit");
    assert!(
        c <= limits::MAX_GENERATOR_NODES,
        "node count {c} exceeds limit"
    );
}

fn count_nodes(node: &Generator, depth: u32, max_depth: &mut u32, count: &mut u32) {
    *count += 1;
    if depth > *max_depth {
        *max_depth = depth;
    }
    for c in &node.children {
        count_nodes(c, depth + 1, max_depth, count);
    }
}

#[test]
fn generator_wide_fan_is_truncated_to_budget() {
    // A fan one level deep with more children than the node budget must
    // have its tail actually dropped, not silently left in the tree. The
    // previous off-by-one resolved to a no-op on the nominal break path,
    // letting the unvisited subtrees bypass every downstream NaN/size
    // clamp.
    let mut root = Generator::default();
    let fan_width = (limits::MAX_GENERATOR_NODES * 4) as usize;
    for _ in 0..fan_width {
        root.children.push(Generator::default());
    }

    sanitize_generator(&mut root);

    let mut d = 0u32;
    let mut c = 0u32;
    count_nodes(&root, 0, &mut d, &mut c);
    assert!(
        c <= limits::MAX_GENERATOR_NODES,
        "wide-fan sanitize left {c} nodes (> budget {})",
        limits::MAX_GENERATOR_NODES
    );
}

#[test]
fn generator_rejects_terrain_in_child_position() {
    // Strict scheme: Terrain is root-only. The terrain plugin owns the
    // world's heightmap, so a Terrain in a child slot would either spawn
    // a second heightfield collider (Avian forbids that) or be silently
    // ignored. The sanitizer overwrites it with a default cuboid.
    let mut root = Generator::default();
    root.children
        .push(Generator::from_kind(GeneratorKind::Terrain(
            Default::default(),
        )));

    sanitize_generator(&mut root);

    for child in &root.children {
        assert!(
            !matches!(&child.kind, GeneratorKind::Terrain(_)),
            "Terrain survived as a child"
        );
    }
}

#[test]
fn generator_rejects_water_at_root() {
    // Strict scheme: Water is child-only. A root Water is overwritten so
    // every Water volume in a record has an ancestor whose transform
    // anchors it (typically a Terrain root in a region blueprint).
    let mut root = Generator::from_kind(GeneratorKind::Water {
        level_offset: Fp(0.0),
        surface: Default::default(),
    });
    sanitize_generator(&mut root);
    assert!(
        !matches!(&root.kind, GeneratorKind::Water { .. }),
        "Water survived at root after sanitize"
    );
}

#[test]
fn water_is_allowed_as_child() {
    // Inverse of the rule above: Water *is* welcome as a descendant of
    // any other generator. Saving a region blueprint to inventory relies
    // on this — the homeworld's water lives inside the terrain root and
    // must round-trip unchanged.
    let mut root = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
    root.children
        .push(Generator::from_kind(GeneratorKind::Water {
            level_offset: Fp(0.0),
            surface: Default::default(),
        }));

    sanitize_generator(&mut root);

    assert!(matches!(&root.kind, GeneratorKind::Terrain(_)));
    assert_eq!(root.children.len(), 1, "Water child must survive sanitize");
    assert!(matches!(
        &root.children[0].kind,
        GeneratorKind::Water { .. }
    ));
}

#[test]
fn terrain_root_keeps_authored_children() {
    // A Terrain root anchors a region blueprint — its children (water,
    // L-systems, props, …) must travel with the inventory item, so the
    // sanitizer leaves the children list alone (subject to the depth /
    // total-node budget enforced elsewhere).
    let mut terrain_root = Generator::from_kind(GeneratorKind::Terrain(Default::default()));
    terrain_root.children.push(Generator::default_cuboid());
    terrain_root
        .children
        .push(Generator::from_kind(GeneratorKind::Water {
            level_offset: Fp(0.0),
            surface: Default::default(),
        }));

    sanitize_generator(&mut terrain_root);
    assert_eq!(
        terrain_root.children.len(),
        2,
        "Terrain root must preserve its children list"
    );
}

#[test]
fn water_child_strips_authored_grandchildren() {
    // Water itself is still a leaf — `spawn_water_volume` doesn't consume
    // children, so the sanitizer strips them to keep the editor and the
    // spawner in sync.
    let mut root = Generator::default();
    let mut water_child = Generator::from_kind(GeneratorKind::Water {
        level_offset: Fp(0.0),
        surface: Default::default(),
    });
    water_child.children.push(Generator::default_cuboid());
    water_child.children.push(Generator::default_cuboid());
    root.children.push(water_child);

    sanitize_generator(&mut root);

    assert_eq!(root.children.len(), 1);
    assert!(matches!(
        &root.children[0].kind,
        GeneratorKind::Water { .. }
    ));
    assert!(
        root.children[0].children.is_empty(),
        "Water children must be stripped (Water is a leaf)"
    );
}

#[test]
fn lsystem_material_octaves_are_clamped() {
    use std::collections::HashMap;
    use symbios_overlands::pds::{
        PropMeshType, SovereignBarkConfig, SovereignMaterialSettings, SovereignTextureConfig,
    };

    let mut materials: HashMap<u8, SovereignMaterialSettings> = HashMap::new();
    let bark_slot = SovereignMaterialSettings {
        emission_strength: Fp(f32::NAN),
        uv_scale: Fp(f32::INFINITY),
        texture: SovereignTextureConfig::Bark(SovereignBarkConfig {
            octaves: 4_000_000_000,
            ..SovereignBarkConfig::default()
        }),
        ..SovereignMaterialSettings::default()
    };
    materials.insert(0, bark_slot);

    let mut lsys = Generator::from_kind(GeneratorKind::LSystem {
        source_code: "omega: F".into(),
        finalization_code: String::new(),
        iterations: 2,
        seed: 0,
        angle: Fp(25.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.0),
        tropism: None,
        materials,
        prop_mappings: HashMap::<u16, PropMeshType>::new(),
        prop_scale: Fp(1.0),
        mesh_resolution: 8,
    });
    sanitize_generator(&mut lsys);

    let GeneratorKind::LSystem { materials, .. } = &lsys.kind else {
        panic!("sanitize changed LSystem variant");
    };
    let settings = materials.get(&0).expect("bark slot missing after sanitize");
    assert!(
        settings.emission_strength.0.is_finite(),
        "emission_strength left non-finite"
    );
    assert!(settings.uv_scale.0.is_finite(), "uv_scale left non-finite");
    match &settings.texture {
        SovereignTextureConfig::Bark(b) => {
            assert!(
                b.octaves <= limits::MAX_ROCK_OCTAVES,
                "bark octaves {} > cap",
                b.octaves
            );
            assert!(b.octaves >= 1, "bark octaves clamped below floor");
        }
        other => panic!("bark variant mutated: {other:?}"),
    }
}

#[test]
fn generator_node_transform_rejects_non_finite_fields() {
    use symbios_overlands::pds::{Fp4, TransformData};
    let mut generator = Generator {
        kind: GeneratorKind::Cuboid {
            size: Fp3([1.0, 1.0, 1.0]),
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        },
        transform: TransformData {
            translation: Fp3([f32::NAN, f32::INFINITY, 0.0]),
            rotation: Fp4([f32::NAN, f32::NAN, f32::NAN, f32::NAN]),
            scale: Fp3([-1.0, 0.0, f32::INFINITY]),
        },
        children: Vec::new(),
    };
    // Must not panic.
    sanitize_generator(&mut generator);

    for &v in generator.transform.translation.0.iter() {
        assert!(v.is_finite(), "translation contains non-finite after clamp");
    }
    for &v in generator.transform.rotation.0.iter() {
        assert!(v.is_finite(), "rotation contains non-finite after clamp");
    }
    for &v in generator.transform.scale.0.iter() {
        assert!(v.is_finite() && v > 0.0, "scale must be strictly positive");
    }
    let q = generator.transform.rotation.0;
    let m = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    assert!(
        (m - 1.0).abs() < 1e-3,
        "rotation must normalise to unit quaternion"
    );
}

#[test]
fn primitive_torture_clamped() {
    // NaN/infinity/out-of-range torture parameters on a top-level
    // primitive must be driven back into the finite envelope so the
    // CPU-side vertex mutation pass never sees non-finite math.
    let mut prim = Generator::from_kind(GeneratorKind::Cuboid {
        size: Fp3([1.0, 1.0, 1.0]),
        solid: true,
        material: Default::default(),
        twist: Fp(f32::INFINITY),
        taper: Fp(f32::NAN),
        bend: Fp3([f32::INFINITY, f32::NAN, 1_000.0]),
    });
    sanitize_generator(&mut prim);
    if let GeneratorKind::Cuboid {
        twist, taper, bend, ..
    } = &prim.kind
    {
        assert!(twist.0.is_finite());
        assert!(taper.0.is_finite());
        assert!(twist.0.abs() <= limits::MAX_TORTURE_TWIST + 1e-3);
        assert!(taper.0.abs() <= limits::MAX_TORTURE_TAPER + 1e-3);
        for &v in bend.0.iter() {
            assert!(v.is_finite());
            assert!(v.abs() <= limits::MAX_TORTURE_BEND + 1e-3);
        }
    } else {
        panic!("sanitize mutated Cuboid into another variant");
    }
}

// ---------------------------------------------------------------------------
// Generator-count cap
// ---------------------------------------------------------------------------

#[test]
fn generator_map_over_cap_is_trimmed() {
    let mut r = RoomRecord::default_for_did(TEST_DID);
    let template = r.generators.get("base_terrain").cloned().unwrap();
    for i in 0..(limits::MAX_GENERATORS * 2) {
        r.generators.insert(format!("extra_{i}"), template.clone());
    }
    r.sanitize();
    assert!(r.generators.len() <= limits::MAX_GENERATORS);
}

// ---------------------------------------------------------------------------
// Inventory record
// ---------------------------------------------------------------------------

#[test]
fn inventory_stash_over_cap_is_trimmed_deterministically() {
    let mut inv = InventoryRecord::default();
    let template = match RoomRecord::default_for_did(TEST_DID)
        .generators
        .get("base_terrain")
        .cloned()
    {
        Some(g) => g,
        None => panic!("expected base_terrain in default record"),
    };
    for i in 0..200 {
        inv.generators
            .insert(format!("slot_{i:03}"), template.clone());
    }
    inv.sanitize();
    assert!(inv.generators.len() <= 50);

    // Deterministic lexicographic trim: survivors all come from the front
    // of the sorted key order.
    let mut keys: Vec<String> = inv.generators.keys().cloned().collect();
    keys.sort();
    let last = keys.last().unwrap();
    assert!(
        last.as_str() <= "slot_049",
        "lexicographic trim surfaced a key past slot_049: {last}"
    );
}

// ---------------------------------------------------------------------------
// Sanitise is idempotent.
// ---------------------------------------------------------------------------

#[test]
fn sanitize_is_idempotent_on_a_pathological_record() {
    use symbios_overlands::pds::{BiomeFilter, Fp2, Placement, ScatterBounds};
    let mut r = RoomRecord::default_for_did(TEST_DID);
    r.placements.push(Placement::Scatter {
        generator_ref: "base_terrain".into(),
        bounds: ScatterBounds::Circle {
            center: Fp2([f32::NAN, f32::NAN]),
            radius: Fp(-1.0),
        },
        count: u32::MAX,
        local_seed: 0,
        biome_filter: BiomeFilter::default(),
        snap_to_terrain: true,
        random_yaw: true,
    });
    r.sanitize();
    let first: serde_json::Value = serde_json::to_value(&r).unwrap();
    r.sanitize();
    let second: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(first, second, "sanitize drift across passes");
}
