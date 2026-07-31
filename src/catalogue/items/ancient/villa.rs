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
//! The classical reading comes from turned shafts, shadowed
//! intercolumniations, a `Roof(Gable)` pediment (oriented front-facing by
//! making the porch scope deeper than it is wide), and `Roof(Hip)` tile
//! roofs. True arches and domes remain out of reach of the grammar, but
//! the columns are genuinely round: `round_meshes` marks the `Column`
//! terminal so it bakes as an entasis-tapered cylinder rather than a
//! square pier, while the grammar still derives it as an ordinary box.

use std::collections::HashMap;

use crate::catalogue::items::util::{tile, tiles_per_metre};
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
        let mut root = crate::catalogue::items::util::footing(21.0, 17.0, [0.0, 0.0], 13.5);
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
            uv_scale: tiles_per_metre(tile::GROUND),
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
        // Depth matches the 0.5 bay slot so the turned shaft is circular
        // in plan rather than an oval; `Taper` then reads as entasis.
        "Column --> Extrude(0.5) Taper(0.12) Mat(\"Marble\") I(\"Column\")",
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
        // The colonnade's shafts are turned; the entablature, architrave
        // and tympanum share the same marble but stay flat.
        round_meshes: vec!["Column".to_string()],
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

    /// Walks every grammar line through the shared harness — the same
    /// `parse_statement` / `add_statement` path the runtime uses — then
    /// derives against the entry's footprint and checks every `Mat("...")`
    /// slot resolves.
    #[test]
    fn grammar_parses_and_derives() {
        crate::catalogue::items::shape_grammar_test::assert_grammar_parses_and_derives(
            build_kind(),
            "villa",
        );
    }
}
