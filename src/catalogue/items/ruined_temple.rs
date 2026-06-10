//! Ruined temple — a weathered colonnade fronting a half-collapsed
//! cella, with stochastic broken columns, breached walls, and a rubble
//! rear court. Reads as an ancient site reclaimed by the landscape.
//!
//! Footprint 24 × 14. Every weighted rule biases toward decay, so two
//! placements with different grammar seeds crumble differently — one
//! keeps its gable, the other is open to the sky.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignRockConfig, SovereignStuccoConfig, SovereignTextureConfig,
};

pub struct RuinedTemple;

impl CatalogueEntry for RuinedTemple {
    fn slug(&self) -> &'static str {
        "ruined_temple"
    }
    fn name(&self) -> &'static str {
        "Ruined Temple"
    }
    fn description(&self) -> &'static str {
        "Weathered colonnade and half-collapsed cella with stochastic broken columns and breaches."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Buildings
    }
    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation root + corner-origin 24×14 grammar child
        // offset by -footprint/2 (see the villa for the rationale).
        let mut root = super::util::foundation_block(25.0, 15.0, [0.0, 0.0], 2.5);
        let mut temple = Generator::from_kind(build_kind());
        temple.transform.translation = crate::pds::Fp3([-12.0, 0.0, -7.0]);
        root.children.push(temple);
        root
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    // Weathered marble: pale stucco surface over warm stone.
    materials.insert(
        "Marble".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.82, 0.78, 0.70]),
            roughness: Fp(0.75),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
                color_base: Fp3([0.80, 0.76, 0.68]),
                roughness: Fp64(0.5),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    materials.insert(
        "Rubble".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.55, 0.51, 0.45]),
            roughness: Fp(0.95),
            uv_scale: Fp(2.5),
            texture: SovereignTextureConfig::Rock(SovereignRockConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Dark".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.03, 0.03, 0.04]),
            roughness: Fp(1.0),
            ..Default::default()
        },
    );
    // Overgrowth creeping across the stylobate and rubble court.
    materials.insert(
        "Moss".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.22, 0.34, 0.16]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.30, 0.38, 0.18]),
                color_moist: Fp3([0.14, 0.26, 0.10]),
                macro_scale: Fp64(3.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let grammar_source = [
        // ── 1. Massing: portico strip, cella, collapsed rear court ──
        "Lot --> Split(X) { 6: Portico | ~1: CellaPlot | 5: RearCourt }",
        // ── 2. Portico: column rows on a low stylobate ──
        "Portico --> Split(Z) { ~1: ColumnRow | ~1: ColumnRow | ~1: ColumnRow | ~1: ColumnRow }",
        "ColumnRow --> Split(Z) { ~1: Stylobate | 1.6: ColumnSpot | ~1: Stylobate }",
        "Stylobate --> Extrude(0.4) Mat(\"Moss\") I(\"Plinth\")",
        "ColumnSpot --> Split(X) { ~1: Stylobate | 1.6: Column | ~1: Stylobate }",
        "Column --> 55% FullColumn | 45% BrokenColumn",
        "FullColumn --> Extrude(7) ColumnShaft",
        "BrokenColumn --> 50% Extrude(2.2) ColumnShaft | 50% Extrude(3.8) ColumnShaft",
        "ColumnShaft --> Mat(\"Marble\") I(\"Column\")",
        // ── 3. Cella: walls with stochastic breaches, roof often gone ──
        "CellaPlot --> Extrude(6) Split(Y) { ~1: CellaBody | 1.2: CellaTop }",
        "CellaBody --> Comp(Faces) { Side: CellaWall }",
        "CellaWall --> Repeat(X, 3) { WallBay }",
        "WallBay --> 50% MarbleWall | 25% CrackedBay | 25% BreachBay",
        "MarbleWall --> Extrude(0.4) Mat(\"Marble\") I(\"Wall\")",
        "CrackedBay --> Split(Y) { ~1: MarbleWall | 1.5: DarkGap }",
        "BreachBay --> Split(Y) { 1.8: MarbleWall | ~1: DarkGap }",
        "DarkGap --> Extrude(0.08) Mat(\"Dark\") I(\"Hole\")",
        "CellaTop --> 60% RubbleTop | 40% Roof(Gable, 24, 0.3) { Slope: RubbleTop | GableEnd: MarbleWall }",
        "RubbleTop --> Mat(\"Rubble\") I(\"Rubble\")",
        // ── 4. Rear court: knee-high broken walls around a mossy floor ──
        "RearCourt --> Split(Z) { 1.2: LowWall | ~1: CourtFloor | 1.2: LowWall }",
        "CourtFloor --> Extrude(0.2) Mat(\"Moss\") I(\"Court\")",
        "LowWall --> 65% Extrude(2.0) RuinWallSub | 35% Extrude(1.2) RuinWallSub",
        "RuinWallSub --> Comp(Faces) { Side: RuinWallFace | Top: RubbleTop }",
        "RuinWallFace --> Repeat(X, 2.5) { RuinBay }",
        "RuinBay --> 60% MarbleWall | 40% DarkGap",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([24.0, 0.0, 14.0]),
        seed: 23,
        materials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = RuinedTemple.build("");
        sanitize_generator(&mut g);
        // The entry root is now the centred foundation plinth; the
        // grammar hangs beneath it as the first child.
        assert!(
            matches!(g.kind, GeneratorKind::Cuboid { solid: true, .. }),
            "{} root must be the solid foundation plinth",
            "temple"
        );
        let shape = &g.children[0];
        match &shape.kind {
            GeneratorKind::Shape {
                root_rule,
                materials,
                ..
            } => {
                assert_eq!(root_rule, "Lot");
                for slot in ["Marble", "Rubble", "Dark", "Moss"] {
                    assert!(
                        materials.contains_key(slot),
                        "missing material slot: {slot}"
                    );
                }
            }
            other => panic!("temple root must remain Shape; got {other:?}"),
        }
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "ruined_temple");
    }
}
