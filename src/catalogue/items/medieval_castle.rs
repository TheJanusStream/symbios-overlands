//! Procedural medieval castle — courtyard-based layout with corner
//! towers, gatehouse, cloistered wings, and a great keep (sometimes
//! ruined). Adapted from `bevy_symbios_shape`'s `medieval_castle`
//! example.
//!
//! Heavy use of stochastic alternatives (`weight%` syntax) means each
//! place dropped into a room generates a slightly different castle —
//! tower heights vary, some get spires vs battlements, the keep may
//! be intact or ruined, walls intersperse arrow-slits among solid
//! sections. Variation is driven by the per-generator `seed`; the
//! catalogue ships with a fixed seed for predictable starter results,
//! but the user can re-roll by changing it in the editor.

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignPlankConfig, SovereignRockConfig, SovereignShingleConfig, SovereignTextureConfig,
    SovereignWindowConfig,
};

pub struct MedievalCastle;

impl CatalogueEntry for MedievalCastle {
    fn slug(&self) -> &'static str {
        "medieval_castle"
    }
    fn name(&self) -> &'static str {
        "Medieval Castle"
    }
    fn description(&self) -> &'static str {
        "Courtyard castle with corner towers, gatehouse, cloistered wings, and a great keep."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Buildings
    }
    fn build(&self, _local_did: &str) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    materials.insert(
        "Stone".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.55, 0.52, 0.48]),
            roughness: Fp(0.9),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Rock(SovereignRockConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Shingle".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.28, 0.25, 0.22]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Wood".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.38, 0.22, 0.10]),
            roughness: Fp(0.7),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
                color_wood_light: Fp3([0.4, 0.22, 0.10]),
                color_wood_dark: Fp3([0.22, 0.12, 0.04]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Glass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.12, 0.18, 0.28]),
            roughness: Fp(0.05),
            metallic: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Window(SovereignWindowConfig {
                panes_x: 2,
                panes_y: 3,
                frame_width: Fp64(0.1),
                glass_opacity: Fp64(0.35),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // "Dark" — solid near-black, no texture. Used for arrow slits, gate
    // mouth, cloister arches; reads as deep shadow / void.
    materials.insert(
        "Dark".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.02, 0.02, 0.03]),
            roughness: Fp(1.0),
            uv_scale: Fp(1.0),
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

    // Grammar adapted from `bevy_symbios_shape/examples/medieval_castle.rs`.
    // Stochastic alternatives use the `weight%` syntax documented by
    // `symbios_shape::grammar::parse_rule`. Footprint 75×75 m matches the
    // example's `DVec3::new(75.0, 0.0, 75.0)`.
    let grammar_source = [
        // ── 1. Macro layout (concentric wards) ──
        "Lot --> Split(X) { 8: LeftWall | ~1: CastleCore | 8: RightWall }",
        "CastleCore --> Split(Z) { 8: FrontWall | ~1: InnerWard | 22: KeepMass }",
        "InnerWard --> Split(X) { 8: WestWing | ~1: Courtyard | 8: EastWing }",
        // ── 2. Courtyard & cloisters ──
        "Courtyard --> Split(Z) { 4: CloisterZ | ~1: YardZ | 4: CloisterZ }",
        "YardZ --> Split(X) { 4: CloisterX | ~1: YardCenter | 4: CloisterX }",
        "YardCenter --> Mat(\"Grass\") I(\"Grass\")",
        "CloisterZ --> Repeat(X, 4) { CloisterBlock }",
        "CloisterX --> Repeat(Z, 4) { CloisterBlock }",
        "CloisterBlock --> Extrude(4.5) Split(Y) { ~1: CloisterBody | 1.5: CloisterVault }",
        "CloisterBody --> Comp(Faces) { Side: CloisterFacade }",
        "CloisterFacade --> Split(X) { 0.5: SolidWall | ~1: OpenArch | 0.5: SolidWall }",
        "OpenArch --> Extrude(0.2) Mat(\"Dark\") I(\"Hole\")",
        "CloisterVault --> Roof(Pyramid, 35, 0.2) { Slope: ShingleRoof }",
        // ── 3. Barracks / wings (stochastic heights) ──
        "WestWing --> 50% Extrude(14) WingSub | 50% Extrude(10) WingSub",
        "EastWing --> 50% Extrude(14) WingSub | 50% Extrude(18) WingSub",
        "WingSub --> Repeat(Z, 15) { WingPavilion }",
        "WingPavilion --> Split(Y) { ~1: WingBody | 5: WingRoof }",
        "WingBody --> Comp(Faces) { Side: KeepFacade }",
        "WingRoof --> 40% Roof(Gable, 40, 0.3) { Slope: ShingleRoof | GableEnd: SolidWall } | 30% Roof(Jerkinhead, 45, overhang=0.3, tier=0.3) { Slope: ShingleRoof | GableEnd: SolidWall | HipEnd: ShingleRoof } | 20% Roof(DutchGable, 40, overhang=0.4, tier=0.5) { Slope: ShingleRoof | GableEnd: SolidWall } | 10% Roof(Gambrel, 55, 20, overhang=0.3, tier=0.6) { LowerSlope: ShingleRoof | UpperSlope: ShingleRoof | GableEnd: SolidWall }",
        "ShingleRoof --> Mat(\"Shingle\") I(\"Roof\")",
        // ── 4. Outer walls & gatehouse ──
        "LeftWall --> Split(Z) { 10: Tower | ~1: WallSegment | 10: Tower | ~1: WallSegment | 10: Tower }",
        "RightWall --> Split(Z) { 10: Tower | ~1: WallSegment | 10: Tower | ~1: WallSegment | 10: Tower }",
        "FrontWall --> Split(X) { ~1: WallSegment | 14: Gatehouse | ~1: WallSegment }",
        "WallSegment --> Extrude(12) Split(Y) { ~1: WallBody | 1.5: Battlements }",
        "WallBody --> Comp(Faces) { Side: WallFacade | Top: WallWalkway }",
        "WallWalkway --> Mat(\"Stone\") I(\"Walkway\")",
        "WallFacade --> Repeat(X, 4) { WallBay }",
        "WallBay --> 70% SolidWall | 30% ArrowSlitBay",
        "Gatehouse --> Extrude(22) Split(Y) { 8: GatePassage | ~1: GateUpper | 2: Battlements }",
        "GatePassage --> Comp(Faces) { Front: GateArch | Back: GateArch | Side: SolidWall }",
        "GateArch --> Split(X) { ~1: SolidWall | 6: Portcullis | ~1: SolidWall }",
        "Portcullis --> Split(Y) { ~1: GateHole | 4: WoodGate }",
        "GateHole --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
        "WoodGate --> Extrude(0.4) Mat(\"Wood\") I(\"Gate\")",
        "GateUpper --> Comp(Faces) { Side: KeepFacade }",
        // ── 5. Towers (stochastic heights + tops) ──
        "Tower --> 30% Extrude(18) TowerSub | 40% Extrude(26) TowerSub | 30% Extrude(34) TowerSub",
        "TowerSub --> Split(Y) { ~1: TowerBody | 3: TowerUpper }",
        "TowerBody --> Comp(Faces) { Side: TowerFacade }",
        "TowerUpper --> Split(Y) { 0.5: CorbelBand | ~1: TowerTop }",
        "CorbelBand --> Comp(Faces) { Side: CorbelFace }",
        "CorbelFace --> Extrude(0.2) Mat(\"Stone\") I(\"Wall\")",
        "TowerTop --> 50% Battlements | 50% TowerSpire",
        "TowerSpire --> 40% Roof(Pyramid, 70, 0.2) { Slope: ShingleRoof } | 30% Roof(Mansard, 75, 20, overhang=0.2, tier=0.6) { LowerSlope: ShingleRoof | UpperSlope: ShingleRoof } | 30% Roof(PyramidHip, 65, 0.2) { Slope: ShingleRoof }",
        "TowerFacade --> Repeat(Y, 4) { TowerFloor }",
        "TowerFloor --> Repeat(X, 3) { TowerBay }",
        "TowerBay --> 60% SolidWall | 40% ArrowSlitBay",
        // ── 6. Battlements & details ──
        "Battlements --> Comp(Faces) { Side: BattlementSide }",
        "BattlementSide --> Repeat(X, 1.5) { Crenellation }",
        "Crenellation --> Split(X) { 0.8: Merlon | ~1: Crenel }",
        "Merlon --> Extrude(0.2) Mat(\"Stone\") I(\"Wall\")",
        "Crenel --> Extrude(0.05) Mat(\"Dark\") I(\"Wall\")",
        // ── 7. The Great Keep (with stochastic ruin variation) ──
        "KeepMass --> 70% GreatKeep | 30% RuinedKeep",
        "GreatKeep --> Extrude(45) Split(Y) { 37: KeepLower | 8: KeepUpper }",
        "KeepLower --> Comp(Faces) { Side: KeepFacade | Top: WallWalkway }",
        "KeepUpper --> Split(Y) { 1: CorbelBand | ~1: KeepTopBody | 1.5: Battlements }",
        "KeepTopBody --> Comp(Faces) { Side: TowerFacade }",
        "RuinedKeep --> Split(Z) { ~1: GreatKeep | 16: RuinedSection }",
        "RuinedSection --> Extrude(18) Comp(Faces) { Bottom: RuinFloor | Back: KeepFacade | Left: KeepFacade | Right: KeepFacade }",
        "RuinFloor --> Mat(\"Stone\") I(\"Rubble\")",
        "KeepFacade --> Repeat(Y, 6) { KeepFloor }",
        "KeepFloor --> Repeat(X, 4) { KeepBay }",
        "KeepBay --> 50% SolidWall | 30% LargeWindowBay | 20% BalconyBay",
        "LargeWindowBay --> Split(X) { ~1: SolidWall | 2.5: WindowVert | ~1: SolidWall }",
        "WindowVert --> Split(Y) { 1.5: SolidWall | 3.5: GlassWindow | ~1: SolidWall }",
        "GlassWindow --> Extrude(0.4) Mat(\"Glass\") I(\"Pane\")",
        "BalconyBay --> Split(Y) { 1.5: BalconySupport | 3.5: BalconyDoor | ~1: SolidWall }",
        "BalconySupport --> Extrude(1.0) Mat(\"Stone\") I(\"Balcony\")",
        "BalconyDoor --> Split(X) { ~1: SolidWall | 1.8: WoodDoor | ~1: SolidWall }",
        "WoodDoor --> Extrude(0.3) Mat(\"Wood\") I(\"Door\")",
        // ── 8. Core terminal geometry ──
        "SolidWall --> Extrude(0.5) Mat(\"Stone\") I(\"Wall\")",
        "ArrowSlitBay --> Split(X) { ~1: SolidWall | 0.4: ArrowSlit | ~1: SolidWall }",
        "ArrowSlit --> Split(Y) { 1.5: SolidWall | 2.5: SlitHole | ~1: SolidWall }",
        "SlitHole --> Extrude(0.1) Mat(\"Dark\") I(\"Hole\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([75.0, 0.0, 75.0]),
        seed: 42,
        materials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = MedievalCastle.build("");
        sanitize_generator(&mut g);
        match &g.kind {
            GeneratorKind::Shape {
                grammar_source,
                root_rule,
                materials,
                ..
            } => {
                assert!(!grammar_source.is_empty());
                assert_eq!(root_rule, "Lot");
                for slot in ["Stone", "Shingle", "Wood", "Glass", "Dark", "Grass"] {
                    assert!(
                        materials.contains_key(slot),
                        "missing material slot: {slot}"
                    );
                }
            }
            other => panic!("castle root must remain Shape after sanitise; got {other:?}"),
        }
    }

    /// Parse every grammar line through `parse_rule` and add it to a
    /// fresh interpreter, then assert the root rule resolves and that
    /// every `Mat("...")` reference has a backing materials entry.
    /// Critical for the castle because its rules use weighted
    /// alternatives (`70% A | 30% B`) the simple villa doesn't —
    /// regressions there would only surface as runtime warnings.
    #[test]
    fn grammar_parses_and_resolves_materials() {
        use std::collections::HashSet;
        use symbios_shape::Interpreter;
        use symbios_shape::grammar::parse_rule;

        let GeneratorKind::Shape {
            grammar_source,
            root_rule,
            seed,
            materials,
            ..
        } = build_kind()
        else {
            panic!("build_kind must return GeneratorKind::Shape");
        };

        let mut interp = Interpreter::new();
        interp.seed = seed;
        let mut referenced_mats: HashSet<String> = HashSet::new();

        for (i, raw) in grammar_source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let rule = parse_rule(line)
                .unwrap_or_else(|e| panic!("castle rule line {} failed to parse: {}", i + 1, e));
            for mat in line
                .split("Mat(\"")
                .skip(1)
                .filter_map(|chunk| chunk.split('"').next())
            {
                referenced_mats.insert(mat.to_string());
            }
            interp
                .add_weighted_rules(&rule.name, rule.variants)
                .unwrap_or_else(|e| panic!("castle rule {} rejected: {}", rule.name, e));
        }

        assert!(
            interp.has_rule(&root_rule),
            "root rule `{root_rule}` missing from castle grammar"
        );
        for name in &referenced_mats {
            assert!(
                materials.contains_key(name),
                "castle grammar references Mat(\"{name}\") but no material slot is defined"
            );
        }
    }
}
