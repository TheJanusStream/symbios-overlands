//! Tests for [`BiomeFilter`] — the multi-biome allow-list + water relation
//! used by every `Placement::Scatter` entry.

use symbios_overlands::pds::{BiomeFilter, WaterRelation};

#[test]
fn default_is_noop() {
    let f = BiomeFilter::default();
    assert!(f.is_noop());
    assert!(f.accepts(0, 0.0, None));
    assert!(f.accepts(3, -100.0, Some(0.0)));
}

#[test]
fn empty_biomes_means_any_biome_passes() {
    let f = BiomeFilter {
        biomes: vec![],
        water: WaterRelation::Both,
    };
    for b in 0u8..=3 {
        assert!(
            f.accepts(b, 0.0, None),
            "biome {b} must pass empty allow-list"
        );
    }
}

#[test]
fn biome_allow_list_rejects_foreign_biomes() {
    // 0 = Grass, 1 = Dirt, 2 = Rock, 3 = Snow
    let f = BiomeFilter {
        biomes: vec![2, 3],
        water: WaterRelation::Both,
    };
    assert!(!f.accepts(0, 0.0, None));
    assert!(!f.accepts(1, 0.0, None));
    assert!(f.accepts(2, 0.0, None));
    assert!(f.accepts(3, 0.0, None));
}

#[test]
fn water_above_rejects_points_below_surface() {
    let f = BiomeFilter {
        biomes: vec![],
        water: WaterRelation::Above,
    };
    assert!(f.accepts(0, 5.0, Some(0.0)));
    assert!(!f.accepts(0, -1.0, Some(0.0)));
    // Exactly on the surface is "Above" — the filter must be inclusive,
    // otherwise a placement anchored to water level would be rejected.
    assert!(f.accepts(0, 0.0, Some(0.0)));
}

#[test]
fn water_below_rejects_points_at_or_above_surface() {
    let f = BiomeFilter {
        biomes: vec![],
        water: WaterRelation::Below,
    };
    assert!(!f.accepts(0, 5.0, Some(0.0)));
    assert!(f.accepts(0, -1.0, Some(0.0)));
    // `Below` is strictly below — `y < water_level`. At exactly water
    // level, the point is "on" the surface, which we treat as Above.
    assert!(!f.accepts(0, 0.0, Some(0.0)));
}

#[test]
fn missing_water_level_passes_water_relative_filters() {
    // Dry-land records (no water generator) — water-relative filters
    // should collapse to accept so a scatter intended for "above water"
    // still drops onto a no-water room.
    let above = BiomeFilter {
        biomes: vec![],
        water: WaterRelation::Above,
    };
    let below = BiomeFilter {
        biomes: vec![],
        water: WaterRelation::Below,
    };
    assert!(above.accepts(0, 5.0, None));
    assert!(below.accepts(0, -5.0, None));
}

#[test]
fn combined_biome_and_water_constraints_both_apply() {
    let f = BiomeFilter {
        biomes: vec![0], // grass only
        water: WaterRelation::Above,
    };
    assert!(f.accepts(0, 10.0, Some(0.0)));
    assert!(!f.accepts(1, 10.0, Some(0.0))); // wrong biome
    assert!(!f.accepts(0, -1.0, Some(0.0))); // below water
}

#[test]
fn noop_filter_round_trips_as_defaults() {
    // `BiomeFilter::default()` is serde-default via `#[serde(default)]` on
    // the parent field — an empty object must decode back to the no-op.
    let json = "{}";
    let back: BiomeFilter = serde_json::from_str(json).expect("decode");
    assert!(back.is_noop());
}
