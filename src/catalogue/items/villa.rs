//! Detailed modern villa — a two-storey brick / stucco main house with
//! a gable shingle roof, attached metal-roofed garage, paver driveway,
//! and wood deck. Adapted from `bevy_symbios_shape`'s `detailed_villa`
//! example.
//!
//! Was the hard-coded default Shape generator under
//! [`crate::ui::room::widgets`] before the catalogue existed; relocated
//! here so all multi-material "complete building" entries live in one
//! place. The widgets' `default_shape_kind` now delegates to this
//! entry via [`super::super::by_slug`].

use std::collections::HashMap;

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignGroundConfig, SovereignMaterialSettings, SovereignMetalConfig, SovereignPaversConfig,
    SovereignPlankConfig, SovereignShingleConfig, SovereignStuccoConfig, SovereignTextureConfig,
    SovereignWindowConfig,
};

pub struct Villa;

impl CatalogueEntry for Villa {
    fn slug(&self) -> &'static str {
        "villa"
    }
    fn name(&self) -> &'static str {
        "Modern Villa"
    }
    fn description(&self) -> &'static str {
        "Two-storey brick / stucco house with a gable shingle roof, attached garage, and deck."
    }
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Buildings
    }
    fn build(&self) -> Generator {
        Generator::from_kind(build_kind())
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    materials.insert(
        "Brick".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.5, 0.25, 0.15]),
            roughness: Fp(0.9),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
                aspect_ratio: Fp64(3.0),
                color_brick: Fp3([0.45, 0.22, 0.15]),
                scale: Fp64(8.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Stucco".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.88, 0.84, 0.78]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
                color_base: Fp3([0.87, 0.83, 0.77]),
                roughness: Fp64(0.35),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Concrete".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.6, 0.6, 0.6]),
            roughness: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
                formwork_lines: Fp64(3.0),
                formwork_depth: Fp64(0.1),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Shingle".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.2, 0.25]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Metal".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.18, 0.18, 0.2]),
            roughness: Fp(0.3),
            metallic: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
                style: bevy_symbios_texture::metal::MetalStyle::StandingSeam,
                seam_count: Fp64(6.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Glass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.1, 0.2, 0.3]),
            roughness: Fp(0.05),
            metallic: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Window(SovereignWindowConfig {
                panes_x: 2,
                panes_y: 2,
                frame_width: Fp64(0.1),
                glass_opacity: Fp64(0.3),
                mullion_thickness: Fp64(0.12),
                corner_radius: Fp64(0.18),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Wood".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.38, 0.22, 0.12]),
            roughness: Fp(0.6),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
                color_wood_light: Fp3([0.4, 0.24, 0.14]),
                color_wood_dark: Fp3([0.22, 0.12, 0.06]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Pavers".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.5, 0.48, 0.45]),
            roughness: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Pavers(SovereignPaversConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Grass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.35, 0.15]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.3, 0.4, 0.2]),
                color_moist: Fp3([0.15, 0.25, 0.1]),
                macro_scale: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // Grammar adapted from `bevy_symbios_shape/examples/detailed_villa.rs`.
    // Footprint is 20 × 16 (Lot splits 14:HouseMass | 6:GarageMass on X;
    // each mass splits 3+13 / 4+12 on Z).
    let grammar_source = [
        // ── 1. Massing ──
        "Lot --> Split(X) { 14: HouseMass | 6: GarageMass }",
        "HouseMass --> Split(Z) { 3: DeckArea | 13: MainHouse }",
        "GarageMass --> Split(Z) { 4: Driveway | 12: GarageStruct }",
        // ── 2. Platforms & ground ──
        "DeckArea --> Extrude(0.3) Mat(\"Wood\") I(\"Deck\")",
        "Driveway --> Extrude(0.1) Mat(\"Pavers\") I(\"Drive\")",
        // ── 3. Main house volume ──
        "MainHouse --> Extrude(9.5) Split(Y) { 3.5: GroundFloor | 0.3: BeltCourse | 3.2: UpperFloor | 0.3: RoofFascia | 2.2: MainRoof }",
        // ── 4. Garage volume ──
        "GarageStruct --> Extrude(4.0) Split(Y) { 3.5: GarageBody | 0.5: GarageRoof }",
        // ── 5. Roofs ──
        "MainRoof --> Roof(Gable, 30) { Slope: ShingleSlope | GableEnd: GableWall }",
        "ShingleSlope --> Mat(\"Shingle\") I(\"RoofTile\")",
        "GableWall --> Mat(\"Stucco\") I(\"Wall\")",
        "GarageRoof --> Comp(Faces) { Top: FlatRoof | Side: GarageFascia }",
        "GarageFascia --> Extrude(0.1) Mat(\"Metal\") I(\"Fascia\")",
        "FlatRoof --> Mat(\"Metal\") I(\"GarageRoofTile\")",
        "BeltCourse --> Comp(Faces) { Side: BeltFace }",
        "BeltFace --> Extrude(0.25) Mat(\"Concrete\") I(\"Trim\")",
        "RoofFascia --> Comp(Faces) { Side: FasciaFace }",
        "FasciaFace --> Extrude(0.05) Mat(\"Metal\") I(\"Fascia\")",
        // ── 6. Facades ──
        "GroundFloor --> Comp(Faces) { Front: FrontEntryFacade | Back: SideFacade | Left: SideFacade | Right: SideFacade }",
        "FrontEntryFacade --> Split(X) { 1.5: BrickWall | 2.5: EntryDoor | 1.0: BrickWall | 4.0: PictureWindow | ~1: BrickWall }",
        "SideFacade --> Repeat(X, 4.0) { SideBay }",
        "SideBay --> Split(X) { ~1: BrickWall | 2.0: StandardWindowBrick | ~1: BrickWall }",
        "UpperFloor --> Comp(Faces) { Side: UpperFacade }",
        "UpperFacade --> Repeat(X, 3.5) { UpperBay }",
        "UpperBay --> Split(X) { ~1: StuccoWall | 1.5: StandardWindowStucco | ~1: StuccoWall }",
        "GarageBody --> Comp(Faces) { Front: GarageFront | Back: BrickWall | Left: BrickWall | Right: BrickWall }",
        "GarageFront --> Split(X) { ~1: BrickWall | 5.0: GarageDoor | ~1: BrickWall }",
        // ── 7. Windows & walls ──
        "StandardWindowBrick --> Split(Y) { 0.9: BrickWall | 1.6: WinAssembly | ~1: BrickWall }",
        "StandardWindowStucco --> Split(Y) { 0.9: StuccoWall | 1.6: WinAssembly | ~1: StuccoWall }",
        "PictureWindow --> Split(Y) { 0.8: BrickWall | 2.2: WinAssembly | ~1: BrickWall }",
        "WinAssembly --> Split(X) { 0.15: ConcreteFrame | ~1: WinCenter | 0.15: ConcreteFrame }",
        "WinCenter --> Split(Y) { 0.15: ConcreteFrame | ~1: GlassPane | 0.15: ConcreteFrame }",
        "ConcreteFrame --> Extrude(0.25) Mat(\"Concrete\") I(\"Frame\")",
        "GlassPane --> Extrude(0.05) Mat(\"Glass\") I(\"Pane\")",
        "EntryDoor --> Split(Y) { 2.4: DoorAssembly | ~1: BrickWall }",
        "DoorAssembly --> Split(X) { 0.15: ConcreteFrame | ~1: DoorPanel | 0.15: ConcreteFrame }",
        "DoorPanel --> Split(Y) { ~1: WoodPanel | 0.15: ConcreteFrame }",
        "WoodPanel --> Extrude(0.1) Mat(\"Wood\") I(\"Door\")",
        "GarageDoor --> Split(Y) { 2.5: GaragePanel | ~1: BrickWall }",
        "GaragePanel --> Extrude(0.1) Mat(\"Metal\") I(\"GDoor\")",
        "BrickWall --> Extrude(0.2) Mat(\"Brick\") I(\"Wall\")",
        "StuccoWall --> Extrude(0.2) Mat(\"Stucco\") I(\"Wall\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([20.0, 0.0, 16.0]),
        seed: 99,
        materials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_generator;

    #[test]
    fn build_round_trips_through_sanitize() {
        let mut g = Villa.build();
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
                assert!(materials.contains_key("Brick"));
                assert!(materials.contains_key("Stucco"));
            }
            other => panic!("villa root must remain Shape after sanitise; got {other:?}"),
        }
    }

    /// Walk every grammar line through the same `parse_rule` /
    /// `add_weighted_rules` path the runtime uses, then derive against
    /// the default footprint. Catches typos and ensures every
    /// `Mat("...")` slot referenced in the grammar has a matching
    /// entry in the materials map — otherwise a hand-edit that drops
    /// a slot or breaks a rule only surfaces as a runtime warning the
    /// first time someone drops the entry in a room.
    #[test]
    fn grammar_parses_and_derives() {
        use std::collections::HashSet;
        use symbios_shape::grammar::parse_rule;
        use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

        let GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
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
                .unwrap_or_else(|e| panic!("villa rule line {} failed to parse: {}", i + 1, e));
            for mat in line
                .split("Mat(\"")
                .skip(1)
                .filter_map(|chunk| chunk.split('"').next())
            {
                referenced_mats.insert(mat.to_string());
            }
            interp
                .add_weighted_rules(&rule.name, rule.variants)
                .unwrap_or_else(|e| panic!("villa rule {} rejected: {}", rule.name, e));
        }

        assert!(
            interp.has_rule(&root_rule),
            "root rule `{root_rule}` missing from villa grammar"
        );
        for name in &referenced_mats {
            assert!(
                materials.contains_key(name),
                "villa grammar references Mat(\"{name}\") but no material slot is defined"
            );
        }

        let scope = Scope::new(
            SVec3::ZERO,
            SQuat::IDENTITY,
            SVec3::new(
                footprint.0[0] as f64,
                footprint.0[1] as f64,
                footprint.0[2] as f64,
            ),
        );
        let model = interp
            .derive(scope, &root_rule)
            .expect("villa grammar must derive against its default footprint");
        assert!(
            !model.terminals.is_empty(),
            "villa derivation produced zero terminals — footprint is starving the splits"
        );
    }
}
