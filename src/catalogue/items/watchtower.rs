//! Stone watchtower — a slender garrison tower with stochastic height,
//! arrow slits, ember lamp niches, and either battlements or a
//! shingled spire, plus a small gabled annex hut at its foot.
//!
//! Footprint 12 × 12. The grammar reuses the castle's tower idiom
//! (Repeat facades, weighted tops) at a scale that fits a seeded home
//! region landmark — see `crate::seeded_defaults::room::landmark`.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignPlankConfig, SovereignRockConfig, SovereignShingleConfig, SovereignTextureConfig,
};

pub struct Watchtower;

impl CatalogueEntry for Watchtower {
    fn slug(&self) -> &'static str {
        "watchtower"
    }
    fn name(&self) -> &'static str {
        "Watchtower"
    }
    fn description(&self) -> &'static str {
        "Slender stone garrison tower with battlements or a spire, ember lamps, and an annex hut."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Buildings
    }
    fn build(&self, _local_did: &str) -> Generator {
        // Centred foundation root + corner-origin 12×12 grammar child
        // offset by -footprint/2 (see the villa for the rationale).
        let mut root = super::util::foundation_block(13.0, 13.0, [0.0, 0.0], 3.0);
        let mut tower = Generator::from_kind(build_kind());
        tower.transform.translation = crate::pds::Fp3([-6.0, 0.0, -6.0]);
        root.children.push(tower);
        root
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    materials.insert(
        "Stone".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.52, 0.50, 0.47]),
            roughness: Fp(0.9),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Rock(SovereignRockConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Shingle".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.26, 0.24, 0.22]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Wood".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.36, 0.21, 0.10]),
            roughness: Fp(0.7),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
                color_wood_light: Fp3([0.4, 0.22, 0.10]),
                color_wood_dark: Fp3([0.2, 0.11, 0.04]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    materials.insert(
        "Dark".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.02, 0.02, 0.03]),
            roughness: Fp(1.0),
            ..Default::default()
        },
    );
    // Warm window-glow niches — the tower reads inhabited at night.
    materials.insert(
        "Ember".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 0.62, 0.25]),
            emission_color: Fp3([1.0, 0.55, 0.2]),
            emission_strength: Fp(3.0),
            roughness: Fp(0.5),
            ..Default::default()
        },
    );
    materials.insert(
        "Grass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.20, 0.32, 0.14]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.28, 0.38, 0.18]),
                color_moist: Fp3([0.14, 0.24, 0.10]),
                macro_scale: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    let grammar_source = [
        // ── 1. Massing: tower plot + annex strip ──
        "Lot --> Split(X) { 8: TowerPlot | 4: AnnexPlot }",
        // ── 2. Tower (stochastic height) ──
        "TowerPlot --> 30% Extrude(16) TowerSub | 40% Extrude(22) TowerSub | 30% Extrude(28) TowerSub",
        "TowerSub --> Split(Y) { ~1: TowerBody | 3.5: TowerUpper }",
        "TowerBody --> Comp(Faces) { Side: TowerFacade }",
        "TowerFacade --> Repeat(Y, 4) { TowerFloor }",
        "TowerFloor --> Repeat(X, 3) { TowerBay }",
        "TowerBay --> 55% SolidWall | 30% ArrowSlitBay | 15% LampNiche",
        "LampNiche --> Split(Y) { ~1: SolidWall | 1.2: GlowSlot | ~1: SolidWall }",
        "GlowSlot --> Extrude(0.15) Mat(\"Ember\") I(\"Lamp\")",
        "TowerUpper --> Split(Y) { 0.6: CorbelBand | ~1: TowerTop }",
        "CorbelBand --> Comp(Faces) { Side: CorbelFace }",
        "CorbelFace --> Extrude(0.25) Mat(\"Stone\") I(\"Wall\")",
        "TowerTop --> 55% Battlements | 45% TowerSpire",
        "TowerSpire --> 60% Roof(Pyramid, 65, 0.25) { Slope: ShingleRoof } | 40% Roof(PyramidHip, 60, 0.25) { Slope: ShingleRoof }",
        // ── 3. Battlements ──
        "Battlements --> Comp(Faces) { Side: BattlementSide }",
        "BattlementSide --> Repeat(X, 1.4) { Crenellation }",
        "Crenellation --> Split(X) { 0.7: Merlon | ~1: Crenel }",
        "Merlon --> Extrude(0.25) Mat(\"Stone\") I(\"Wall\")",
        "Crenel --> Extrude(0.05) Mat(\"Dark\") I(\"Wall\")",
        // ── 4. Annex hut ──
        "AnnexPlot --> Split(Z) { ~1: AnnexYard | 6: AnnexHut }",
        "AnnexYard --> Mat(\"Grass\") I(\"Yard\")",
        "AnnexHut --> Extrude(4) Split(Y) { 3: HutBody | ~1: HutRoof }",
        "HutBody --> Comp(Faces) { Front: HutFacade | Back: SolidWall | Left: SolidWall | Right: SolidWall }",
        "HutFacade --> Split(X) { ~1: SolidWall | 1.6: HutDoor | ~1: SolidWall }",
        "HutDoor --> Split(Y) { 2.2: DoorPanel | ~1: SolidWall }",
        "DoorPanel --> Extrude(0.15) Mat(\"Wood\") I(\"Door\")",
        "HutRoof --> Roof(Gable, 35, 0.3) { Slope: ShingleRoof | GableEnd: SolidWall }",
        // ── 5. Shared terminals ──
        "ShingleRoof --> Mat(\"Shingle\") I(\"Roof\")",
        "SolidWall --> Extrude(0.4) Mat(\"Stone\") I(\"Wall\")",
        "ArrowSlitBay --> Split(X) { ~1: SolidWall | 0.4: ArrowSlit | ~1: SolidWall }",
        "ArrowSlit --> Split(Y) { 1.4: SolidWall | 2.2: SlitHole | ~1: SolidWall }",
        "SlitHole --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([12.0, 0.0, 12.0]),
        seed: 7,
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
        let mut g = Watchtower.build("");
        sanitize_generator(&mut g);
        // The entry root is now the centred foundation plinth; the
        // grammar hangs beneath it as the first child.
        assert!(
            matches!(g.kind, GeneratorKind::Cuboid { solid: true, .. }),
            "{} root must be the solid foundation plinth",
            "watchtower"
        );
        let shape = &g.children[0];
        match &shape.kind {
            GeneratorKind::Shape {
                root_rule,
                materials,
                ..
            } => {
                assert_eq!(root_rule, "Lot");
                for slot in ["Stone", "Shingle", "Wood", "Dark", "Ember", "Grass"] {
                    assert!(
                        materials.contains_key(slot),
                        "missing material slot: {slot}"
                    );
                }
            }
            other => panic!("watchtower root must remain Shape; got {other:?}"),
        }
    }

    #[test]
    fn grammar_parses_and_derives() {
        assert_grammar_parses_and_derives(build_kind(), "watchtower");
    }
}
