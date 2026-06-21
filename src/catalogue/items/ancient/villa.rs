//! Roman peristyle villa — a pedimented temple-front porch carried on a
//! marble colonnade, flanked by lower colonnaded wings under hipped
//! terracotta roofs, with a rear peristyle garden ringed by a low
//! portico. Dressed in veined marble, coursed sandstone ashlar and
//! terracotta tile — the affluent residence of the AncientClassical kit.
//!
//! Was the hard-coded default Shape generator under
//! `crate::ui::room::widgets` before the catalogue existed; relocated
//! here so all multi-material "complete building" entries live in one
//! place. The widgets' `default_shape_kind` now delegates to this
//! entry via [`crate::catalogue::by_slug`].
//!
//! Shape-grammar massing only: the DSL extrudes boxes and parametric
//! roofs, so the classical reading is built from entasis-tapered piers
//! (`Extrude` + `Taper`), shadowed intercolumniations, a `Roof(Gable)`
//! pediment (oriented front-facing by making the porch scope deeper than
//! it is wide), and `Roof(Hip)` tile roofs — not round shafts, true
//! arches or domes, which the grammar cannot express.

use std::collections::HashMap;

use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, SovereignGroundConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{
    MARBLE_WHITE, SANDSTONE_GOLD, SANDSTONE_WEATHERED, STONE_VOID, TERRACOTTA, marble, sandstone,
    terracotta,
};

pub struct Villa;

impl CatalogueEntry for Villa {
    fn slug(&self) -> &'static str {
        "villa"
    }
    fn name(&self) -> &'static str {
        "Roman Villa"
    }
    fn description(&self) -> &'static str {
        "Pedimented temple-front portico and colonnaded wings around a rear peristyle garden, in marble, ashlar and terracotta."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// An affluent residence — the prosperous end of the kit.
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 13.5,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        // The centred foundation plinth is the root; the corner-origin
        // 20×16 grammar hangs beneath it offset by -footprint/2, so the
        // whole entry is centred on its anchor (placement yaw turns the
        // building around its middle, and the dry-land clearance ring
        // measures from the true centre).
        let mut root = crate::catalogue::items::util::foundation_block(21.0, 17.0, [0.0, 0.0], 3.0);
        let mut house = Generator::from_kind(build_kind());
        house.transform.translation = crate::pds::Fp3([-10.0, 0.0, -8.0]);
        root.children.push(house);
        root
    }
}

fn build_kind() -> GeneratorKind {
    let mut materials = HashMap::new();

    // Veined white marble — columns, entablature, pediment tympanum.
    materials.insert("Marble".to_string(), marble(MARBLE_WHITE));
    // Coursed sandstone ashlar — dressed stylobate / podium courses.
    materials.insert("Sandstone".to_string(), sandstone(SANDSTONE_GOLD));
    // Weathered sandstone — the lower garden walls and walks.
    materials.insert("Travertine".to_string(), sandstone(SANDSTONE_WEATHERED));
    // Fired terracotta — the tile roofs.
    materials.insert("Tile".to_string(), terracotta(TERRACOTTA));

    // Deep shadow filling the intercolumniations behind the colonnade.
    materials.insert(
        "Shade".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3(STONE_VOID),
            roughness: Fp(1.0),
            ..Default::default()
        },
    );

    // Planted court inside the rear peristyle — a Roman hortus.
    materials.insert(
        "Garden".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.22, 0.35, 0.16]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.32, 0.40, 0.20]),
                color_moist: Fp3([0.14, 0.26, 0.10]),
                macro_scale: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // A Roman domus rendered in pure box-and-roof grammar. Footprint is
    // 20 (X) × 16 (Z); the front face is −Z. The pedimented porch is made
    // deeper (Z) than wide (X) so its `Roof(Gable)` ridges along Z and the
    // tympanum triangle faces the front (see interpreter `sx >= sz`).
    let grammar_source = [
        // ── 1. Massing: temple-front + flanking wings, rear peristyle garden ──
        "Lot --> Split(Z) { 9: HouseRange | 7: GardenRange }",
        "HouseRange --> Split(X) { 7: HouseWing | 6: CentralBlock | 7: HouseWing }",
        "CentralBlock --> Split(Z) { 7: FrontPorch | 2: HallLink }",
        // ── 2. Flanking wings — colonnade walls under a hipped tile roof ──
        "HouseWing --> Extrude(5.5) Split(Y) { 0.5: Stylobate | ~1: Colonnade | 0.6: Entablature | 1.6: HipRoof }",
        "HallLink --> Extrude(5.0) Split(Y) { 0.5: Stylobate | ~1: Colonnade | 0.6: Entablature | 1.2: HipRoof }",
        // ── 3. Temple front — a taller colonnade carrying a pediment ──
        "FrontPorch --> Extrude(7.0) Split(Y) { 0.5: Stylobate | ~1: Colonnade | 0.6: Architrave | 1.7: Pediment }",
        "Pediment --> Roof(Gable, 32, 0.4) { Slope: TileSlope | GableEnd: PedimentField }",
        "PedimentField --> Mat(\"Marble\") I(\"Tympanum\")",
        "Architrave --> Mat(\"Marble\") I(\"Architrave\")",
        // ── 4. Shared colonnade facade — entasis piers, shadowed bays ──
        "Colonnade --> Comp(Faces) { Side: ColonnadeFace }",
        "ColonnadeFace --> Repeat(X, 1.6) { ColumnBay }",
        "ColumnBay --> Split(X) { 0.5: Column | ~1: Intercolumniation }",
        "Column --> Extrude(0.3) Taper(0.12) Mat(\"Marble\") I(\"Column\")",
        "Intercolumniation --> Extrude(0.05) Mat(\"Shade\") I(\"Bay\")",
        // ── 5. Bases, cornices, tile roofs ──
        "Stylobate --> Mat(\"Sandstone\") I(\"Stylobate\")",
        "Entablature --> Mat(\"Marble\") I(\"Entablature\")",
        "HipRoof --> Roof(Hip, 22, 0.4) { Slope: TileSlope | All: TileSlope }",
        "TileSlope --> Mat(\"Tile\") I(\"Tile\")",
        // ── 6. Rear peristyle garden — low walks around a planted court ──
        "GardenRange --> Split(Z) { ~1: CourtBody | 3: RearPortico }",
        "CourtBody --> Split(X) { 3.5: GardenWalk | ~1: GardenCourt | 3.5: GardenWalk }",
        "GardenWalk --> Extrude(3.2) Split(Y) { 0.4: GardenBase | ~1: Colonnade | 0.5: Entablature }",
        "RearPortico --> Extrude(3.5) Split(Y) { 0.4: GardenBase | ~1: Colonnade | 0.6: Entablature }",
        "GardenBase --> Mat(\"Travertine\") I(\"GardenBase\")",
        "GardenCourt --> Extrude(0.3) Mat(\"Garden\") I(\"Garden\")",
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
        let mut g = Villa.build("");
        sanitize_generator(&mut g);
        // The entry root is now the centred foundation plinth; the
        // grammar hangs beneath it as the first child.
        assert!(
            matches!(g.kind, GeneratorKind::Cuboid { solid: true, .. }),
            "{} root must be the solid foundation plinth",
            "villa"
        );
        let shape = &g.children[0];
        match &shape.kind {
            GeneratorKind::Shape {
                grammar_source,
                root_rule,
                materials,
                ..
            } => {
                assert!(!grammar_source.is_empty());
                assert_eq!(root_rule, "Lot");
                // Classical material bar: marble facing over sandstone ashlar.
                assert!(materials.contains_key("Marble"));
                assert!(materials.contains_key("Sandstone"));
                assert!(materials.contains_key("Tile"));
                // The suburban palette must be gone.
                assert!(!materials.contains_key("Brick"));
                assert!(!materials.contains_key("Shingle"));
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
